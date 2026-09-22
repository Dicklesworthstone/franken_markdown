//! Embedded poster images and host-supplied faces. Resource keys are identifiers,
//! not permission to fetch a URL or read a file. SVG is always an image
//! subresource, never interpolated as active markup into the poster.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use franken_markdown::{FontAssetSlot, FontAssets, PdfImageAsset, RenderError};
use super::{Ink, Op, Poster, RStyle, SvgWarning, Word};
use super::image_source::{MAX_IMAGE_BYTES, decode_data_uri, failure, inspect};

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
        let invalid = |message: &str| RenderError::InvalidInput(format!("svg_resources: {message}"));
        if assets.len() > MAX_IMAGES { return Err(invalid("more than 4096 image assets")); }
        let mut total = 0usize;
        // Admission precedes encoding; duplicate payloads still count against
        // ingress limits even though the first matching key wins.
        for asset in assets {
            if asset.bytes.len() > MAX_IMAGE_BYTES { return Err(invalid("image exceeds 32 MiB")); }
            if asset.destination.trim().is_empty() || asset.destination.len() > MAX_RESOURCE_KEY_BYTES {
                return Err(invalid("image key must contain 1..=4096 bytes"));
            }
            total = total.checked_add(asset.bytes.len()).filter(|n| *n <= MAX_RETAINED_BYTES)
                .ok_or_else(|| invalid("image input payloads exceed 128 MiB"))?;
        }
        let mut cache = Cache::default();
        for asset in assets {
            let key = asset.destination.trim();
            if cache.entries.contains_key(key) { continue; }
            let value = image_data(&asset.bytes);
            let cost = key.len().checked_add(retained_size(&value))
                .and_then(|n| cache.retained_bytes.checked_add(n))
                .filter(|n| *n <= MAX_RETAINED_BYTES)
                .ok_or_else(|| invalid("encoded image cache exceeds 128 MiB"))?;
            cache.retained_bytes = cost;
            cache.entries.insert(key.to_owned(), value);
        }
        Ok(Self { cache: RefCell::new(cache) })
    }

    fn resolve(&self, destination: &str) -> Result<Rc<ImageData>, SvgWarning> {
        let key = destination.trim();
        if key.len() > MAX_RESOURCE_KEY_BYTES && !key.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:")) {
            return Err(failure("svg_image_limit", "image key exceeds 4096 bytes"));
        }
        {
            let cache = self.cache.borrow();
            if let Some(value) = cache.entries.get(key) { return value.clone(); }
            if cache.entries.len() >= MAX_IMAGES || key.len() > MAX_RETAINED_BYTES.saturating_sub(cache.retained_bytes) {
                return Err(failure("svg_image_limit", "image cache admission limit reached"));
            }
        }
        let value = if key.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:")) {
            decode_data_uri(key).and_then(|bytes| image_data(&bytes))
        } else {
            Err(failure("svg_image_missing", "image needs caller-supplied bytes; the renderer does not fetch files or URLs"))
        };
        let mut cache = self.cache.borrow_mut();
        let next = cache.retained_bytes.checked_add(key.len())
            .and_then(|n| n.checked_add(retained_size(&value)))
            .filter(|n| *n <= MAX_RETAINED_BYTES)
            .ok_or_else(|| failure("svg_image_limit", "encoded image cache exceeds 128 MiB"))?;
        cache.retained_bytes = next;
        cache.entries.insert(key.to_owned(), value.clone());
        value
    }
}

fn retained_size(value: &Result<Rc<ImageData>, SvgWarning>) -> usize {
    match value { Ok(image) => image.uri.len(), Err(warning) => warning.message.len() }
}

fn image_data(bytes: &[u8]) -> Result<Rc<ImageData>, SvgWarning> {
    let (mime, width, height) = inspect(bytes)?;
    let encoded = franken_markdown::html::base64_encode(bytes);
    Ok(Rc::new(ImageData { uri: format!("data:{mime};base64,{encoded}"), width, height }))
}

impl Poster {
    pub(super) fn with_resources(mut self, fonts: &FontAssets, images: &[PdfImageAsset])
        -> Result<Self, RenderError>
    {
        // Bound aggregate font input too; the shared validator bounds each face.
        let total = FontAssetSlot::ALL.iter().try_fold(0usize, |total, &slot| {
            total.checked_add(fonts.slot_bytes(slot).map_or(0, <[u8]>::len))
        }).filter(|total| *total <= MAX_RETAINED_BYTES);
        if total.is_none() { return Err(RenderError::InvalidInput("svg_resources: font payloads exceed 128 MiB".to_owned())); }
        fonts.validate()?;
        for (index, slot) in FontAssetSlot::ALL.into_iter().enumerate() {
            let Some(bytes) = fonts.resolved_bytes(slot) else { continue; };
            let font = franken_markdown::text::Font::parse(bytes.to_vec())
                .map_err(|error| RenderError::InvalidInput(format!("svg_resources: {} font: {error}", slot.as_str())))?;
            self.faces[index] = Some(if font.instance_bounds(*b"wght").is_some() {
                font.instance(f32::from(fonts.effective_weight(slot))).ok_or_else(||
                    RenderError::InvalidInput(format!("svg_resources: cannot instance {} font", slot.as_str())))?
            } else { font });
        }
        self.images = ImageStore::with_assets(images)?;
        Ok(self)
    }

    pub(super) fn image_word(&self, destination: &str, alt: &str, style: RStyle, size: f64, width: f64) -> Word {
        match self.images.resolve(destination) {
            Ok(image) => {
                let width = image.width.min(width.max(1.0));
                let height = image.height * (width / image.width);
                Word {
                    text: String::new(), style, w: width, gap: 0.0, formula: None, warning: None,
                    image: Some(ImageRun { image, width, height, alt: alt.to_owned() }),
                }
            }
            Err(warning) => {
                let text = if alt.is_empty() { "[image]".to_owned() } else { format!("[{alt}]") };
                let style = RStyle { ink: Ink::FgMuted, ..style };
                Word { w: self.measure(&text, style, size), text, style, gap: 0.0,
                    formula: None, image: None, warning: Some(warning) }
            }
        }
    }

    pub(super) fn draw_image(&mut self, run: &ImageRun, x: f64, baseline: f64) {
        self.ops.push(Op::Image { run: run.clone(), x, y: baseline - run.height });
    }
}
