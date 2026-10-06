//! Embedded poster images and host-supplied faces. Resource keys are identifiers,
//! not permission to fetch a URL or read a file. SVG is always an image
//! subresource, never interpolated as active markup into the poster.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::image_source::{MAX_IMAGE_BYTES, decode_data_uri, failure, inspect};
use super::{Ink, Op, Poster, RStyle, SvgWarning, Word};
use franken_markdown::{FontAssetSlot, FontAssets, PdfImageAsset, RenderError};

const MAX_IMAGES: usize = 4096;
const MAX_RETAINED_BYTES: usize = 128 * 1024 * 1024;
const MAX_RESOURCE_KEY_BYTES: usize = 4096;

#[derive(Debug, PartialEq)]
pub(super) struct ImageData {
    pub(super) uri: String,
    pub(super) width: f64,
    pub(super) height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ImageRun {
    pub(super) image: Rc<ImageData>,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) alt: String,
}

#[derive(Default)]
struct Cache {
    entries: BTreeMap<String, Result<Rc<ImageData>, SvgWarning>>,
    retained_bytes: usize,
}

#[derive(Default)]
pub(super) struct ImageStore {
    cache: RefCell<Cache>,
}

impl ImageStore {
    pub(super) fn with_assets(assets: &[PdfImageAsset]) -> Result<Self, RenderError> {
        let invalid =
            |message: &str| RenderError::InvalidInput(format!("svg_resources: {message}"));
        if assets.len() > MAX_IMAGES {
            return Err(invalid("more than 4096 image assets"));
        }
        let mut total = 0usize;
        // Admission precedes encoding; duplicate payloads still count against
        // ingress limits even though the first matching key wins.
        for asset in assets {
            if asset.bytes.len() > MAX_IMAGE_BYTES {
                return Err(invalid("image exceeds 32 MiB"));
            }
            if asset.destination.trim().is_empty()
                || asset.destination.len() > MAX_RESOURCE_KEY_BYTES
            {
                return Err(invalid("image key must contain 1..=4096 bytes"));
            }
            total = total
                .checked_add(asset.bytes.len())
                .filter(|n| *n <= MAX_RETAINED_BYTES)
                .ok_or_else(|| invalid("image input payloads exceed 128 MiB"))?;
        }
        let mut cache = Cache::default();
        for asset in assets {
            let key = asset.destination.trim();
            if cache.entries.contains_key(key) {
                continue;
            }
            let budget = MAX_RETAINED_BYTES
                .saturating_sub(cache.retained_bytes)
                .saturating_sub(key.len());
            let value = image_data(&asset.bytes, None, budget);
            // Host asset admission remains a typed error, not a silently
            // cached placeholder when valid image data cannot fit.
            if matches!(&value, Err(warning) if warning.code == "svg_image_limit") {
                return Err(invalid("encoded image cache exceeds 128 MiB"));
            }
            let cost = key
                .len()
                .checked_add(retained_size(&value))
                .and_then(|n| cache.retained_bytes.checked_add(n))
                .filter(|n| *n <= MAX_RETAINED_BYTES)
                .ok_or_else(|| invalid("encoded image cache exceeds 128 MiB"))?;
            cache.retained_bytes = cost;
            cache.entries.insert(key.to_owned(), value);
        }
        Ok(Self {
            cache: RefCell::new(cache),
        })
    }

    fn resolve(&self, destination: &str) -> Result<Rc<ImageData>, SvgWarning> {
        let key = destination.trim();
        let is_data_uri = key
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"));
        if key.len() > MAX_RESOURCE_KEY_BYTES && !is_data_uri {
            return Err(failure("svg_image_limit", "image key exceeds 4096 bytes"));
        }
        let budget = {
            let cache = self.cache.borrow();
            if let Some(value) = cache.entries.get(key) {
                return value.clone();
            }
            if cache.entries.len() >= MAX_IMAGES
                || key.len() > MAX_RETAINED_BYTES.saturating_sub(cache.retained_bytes)
            {
                return Err(failure(
                    "svg_image_limit",
                    "image cache admission limit reached",
                ));
            }
            MAX_RETAINED_BYTES - cache.retained_bytes - key.len()
        };
        let value = if is_data_uri {
            decode_data_uri(key).and_then(|bytes| {
                // The decoder validates the header before this slice is used.
                let declared = key[5..].split([';', ',']).next().unwrap_or("");
                image_data(&bytes, Some(declared), budget)
            })
        } else {
            let shown: String = key.chars().take(120).collect();
            Err(failure(
                "svg_image_missing",
                &format!(
                    "image '{shown}' needs caller-supplied bytes; the renderer does not fetch \
                     files or URLs (add --pdf-image '{shown}=PATH')"
                ),
            ))
        };
        let mut cache = self.cache.borrow_mut();
        let next = cache
            .retained_bytes
            .checked_add(key.len())
            .and_then(|n| n.checked_add(retained_size(&value)))
            .filter(|n| *n <= MAX_RETAINED_BYTES)
            .ok_or_else(|| failure("svg_image_limit", "encoded image cache exceeds 128 MiB"))?;
        cache.retained_bytes = next;
        cache.entries.insert(key.to_owned(), value.clone());
        value
    }
}

fn retained_size(value: &Result<Rc<ImageData>, SvgWarning>) -> usize {
    match value {
        Ok(image) => image.uri.len(),
        Err(warning) => warning.message.len(),
    }
}

fn image_data(
    bytes: &[u8],
    declared_mime: Option<&str>,
    budget: usize,
) -> Result<Rc<ImageData>, SvgWarning> {
    let (mime, width, height) = inspect(bytes)?;
    if declared_mime.is_some_and(|declared| !declared.eq_ignore_ascii_case(mime)) {
        return Err(failure(
            "svg_image_mime",
            "declared image MIME type does not match the image container",
        ));
    }
    // inspect() has already bounded bytes to 32 MiB, so this arithmetic cannot
    // overflow. Include the canonical header, not just the encoded payload.
    let uri_size = "data:".len() + mime.len() + ";base64,".len() + bytes.len().div_ceil(3) * 4;
    if uri_size > budget {
        return Err(failure(
            "svg_image_limit",
            "encoded image cache exceeds 128 MiB",
        ));
    }
    let encoded = franken_markdown::html::base64_encode(bytes);
    Ok(Rc::new(ImageData {
        uri: format!("data:{mime};base64,{encoded}"),
        width,
        height,
    }))
}

impl Poster {
    pub(super) fn with_resources(
        mut self,
        fonts: &FontAssets,
        images: &[PdfImageAsset],
    ) -> Result<Self, RenderError> {
        // Bound aggregate font input too; the shared validator bounds each face.
        let total = FontAssetSlot::ALL
            .iter()
            .try_fold(0usize, |total, &slot| {
                total.checked_add(fonts.slot_bytes(slot).map_or(0, <[u8]>::len))
            })
            .filter(|total| *total <= MAX_RETAINED_BYTES);
        if total.is_none() {
            return Err(RenderError::InvalidInput(
                "svg_resources: font payloads exceed 128 MiB".to_owned(),
            ));
        }
        fonts.validate()?;
        for (index, slot) in FontAssetSlot::ALL.into_iter().enumerate() {
            let Some(bytes) = fonts.resolved_bytes(slot) else {
                continue;
            };
            let font = franken_markdown::text::Font::parse(bytes.to_vec()).map_err(|error| {
                RenderError::InvalidInput(format!("svg_resources: {} font: {error}", slot.as_str()))
            })?;
            self.faces[index] = Some(if font.instance_bounds(*b"wght").is_some() {
                font.instance(f32::from(fonts.effective_weight(slot)))
                    .ok_or_else(|| {
                        RenderError::InvalidInput(format!(
                            "svg_resources: cannot instance {} font",
                            slot.as_str()
                        ))
                    })?
            } else {
                font
            });
        }
        self.images = ImageStore::with_assets(images)?;
        Ok(self)
    }

    pub(super) fn image_word(
        &self,
        destination: &str,
        alt: &str,
        style: RStyle,
        size: f64,
        width: f64,
    ) -> Word {
        match self.images.resolve(destination) {
            Ok(image) => {
                let width = image.width.min(width.max(1.0));
                let height = image.height * (width / image.width);
                Word {
                    text: String::new(),
                    style,
                    w: width,
                    gap: 0.0,
                    formula: None,
                    warning: None,
                    shaped: None,
                    image: Some(ImageRun {
                        image,
                        width,
                        height,
                        alt: alt.to_owned(),
                    }),
                }
            }
            Err(warning) => {
                let text = if alt.is_empty() {
                    "[image]".to_owned()
                } else {
                    format!("[{alt}]")
                };
                let style = RStyle {
                    ink: Ink::FgMuted,
                    ..style
                };
                Word {
                    w: self.measure(&text, style, size),
                    text,
                    style,
                    gap: 0.0,
                    formula: None,
                    image: None,
                    warning: Some(warning),
                    shaped: None,
                }
            }
        }
    }

    pub(super) fn draw_image(&mut self, run: &ImageRun, x: f64, baseline: f64) {
        self.ops.push(Op::Image {
            run: run.clone(),
            x,
            y: baseline - run.height,
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn declared_mime_must_match_the_inspected_container() {
        let store = ImageStore::default();
        for mime in ["image/png", "image/jpeg"] {
            let uri = format!("data:{mime},%3Csvg/%3E");
            assert_eq!(store.resolve(&uri).unwrap_err().code, "svg_image_mime");
            assert_eq!(store.resolve(&uri).unwrap_err().code, "svg_image_mime");
        }
        let image = store.resolve("DATA:IMAGE/SVG+XML,%3Csvg/%3E").unwrap();
        assert!(image.uri.starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn encoding_budget_includes_the_canonical_header() {
        let bytes = b"<svg/>";
        let image = image_data(bytes, None, MAX_RETAINED_BYTES).unwrap();
        let exact = image.uri.len();
        assert_eq!(image_data(bytes, None, exact).unwrap(), image);
        for budget in [0, 1, exact - 1] {
            assert_eq!(
                image_data(bytes, None, budget).unwrap_err().code,
                "svg_image_limit"
            );
        }
    }

    #[test]
    fn lazy_images_cannot_exceed_the_remaining_cache_budget() {
        let store = ImageStore::default();
        let key = "data:image/svg+xml,%3Csvg/%3E";
        let initial = MAX_RETAINED_BYTES - key.len() - 1;
        store.cache.borrow_mut().retained_bytes = initial;
        assert_eq!(store.resolve(key).unwrap_err().code, "svg_image_limit");
        assert!(store.cache.borrow().entries.is_empty());
        assert_eq!(store.cache.borrow().retained_bytes, initial);
    }

    #[test]
    fn supplied_asset_keys_remain_opaque_and_take_precedence() {
        let key = "data:image/png,opaque-host-key";
        let store =
            ImageStore::with_assets(&[PdfImageAsset::new(key, b"<svg/>".to_vec())]).unwrap();
        assert!(
            store
                .resolve(key)
                .unwrap()
                .uri
                .starts_with("data:image/svg+xml;")
        );
    }

    #[test]
    fn mismatched_images_keep_alt_text_and_a_stable_diagnostic() {
        let poster = Poster::new(&super::super::SvgOptions::default());
        let word = poster.image_word(
            "data:image/png,%3Csvg/%3E",
            "Architecture",
            RStyle::BODY,
            11.0,
            200.0,
        );
        assert!(word.image.is_none());
        assert_eq!(word.text, "[Architecture]");
        assert_eq!(word.warning.unwrap().code, "svg_image_mime");
    }
}
