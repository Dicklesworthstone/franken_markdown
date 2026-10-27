//! Images inside running PDF text (badges, icons, small diagrams in prose).
//!
//! A standalone image paragraph keeps its own figure line. Any other image
//! used to print its alt text. Inline images now enter the Knuth-Plass
//! paragraph as unbreakable boxes, like inline formulas: the token's slot is
//! [`F_IMAGE`], which never names a font, and its text encodes the image's
//! natural pixel size, destination and alt text. Encoding the size in the
//! token keeps measurement and line extents a pure function of the laid-out
//! line; the decoded image itself lives in the render's [`Faces`] table,
//! which the writer reads to draw and to emit XObjects.

use std::collections::BTreeMap;
use std::rc::Rc;

use super::{
    Block, F_IMAGE, Faces, Inline, PDF_IMAGE_DPI_SCALE, PdfImageData, Tok, push_text_tokens,
    resolve_pdf_image, standalone_image,
};

/// Field separator inside an [`F_IMAGE`] token's text (never in a path).
const SEP: char = '\u{1F}';
/// Marks a token that has not been resolved against the render's assets yet;
/// the next byte is the font slot its alt text falls back to.
const UNRESOLVED: char = '?';

/// Tallest inline image, in ems of the line's font size. Badges (20 px) fit
/// at their natural size; larger pictures scale down to an icon in the line.
const MAX_HEIGHT_EM: f32 = 1.75;
/// Widest inline image, in ems, so one picture can never outgrow a measure.
const MAX_WIDTH_EM: f32 = 36.0;
/// How far an inline image's bottom sits below the baseline, in ems: about a
/// descender, so badges center on the text rather than perch above it.
const DROP_EM: f32 = 0.18;
/// An image-only paragraph whose images are this many ems tall or more is a
/// gallery: its images keep their own figure lines instead of shrinking.
const GALLERY_MIN_HEIGHT_EM: f32 = 3.0;

/// Per-render decoded inline images, keyed by trimmed destination.
pub(super) type InlineImageTable = std::cell::RefCell<BTreeMap<String, Option<Rc<PdfImageData>>>>;

/// Placement of an inline image at a font size, in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct InlineImageBox {
    pub(super) width: f32,
    pub(super) height: f32,
    /// Distance from the baseline down to the image's bottom edge.
    pub(super) descent: f32,
}

/// The parts of a resolved [`F_IMAGE`] token text.
pub(super) struct InlineImageParts<'a> {
    pub(super) width_px: u32,
    pub(super) height_px: u32,
    pub(super) dest: &'a str,
    pub(super) alt: &'a str,
}

/// Token for an image in running text, resolved later by
/// [`resolve_tokens`] once the render's image table is known.
pub(super) fn push_token(
    dest: &str,
    alt: &str,
    alt_slot: u8,
    strike: bool,
    link: Option<u32>,
    out: &mut Vec<Tok>,
) {
    out.push(Tok {
        text: format!("{UNRESOLVED}{alt_slot}{SEP}{}{SEP}{alt}", dest.trim()),
        slot: F_IMAGE,
        space: false,
        hard_break: false,
        link,
        strike,
    });
}

pub(super) fn parts(text: &str) -> Option<InlineImageParts<'_>> {
    let (size, rest) = text.split_once(SEP)?;
    let (dest, alt) = rest.split_once(SEP)?;
    let (width, height) = size.split_once('x')?;
    Some(InlineImageParts {
        width_px: width.parse().ok()?,
        height_px: height.parse().ok()?,
        dest,
        alt,
    })
}

/// Alt text of an [`F_IMAGE`] token, for text layers and accessibility.
pub(super) fn alt_text(text: &str) -> &str {
    text.rsplit_once(SEP).map_or("", |(_, alt)| alt)
}

/// Display box of a resolved [`F_IMAGE`] token text at `size` points.
pub(super) fn image_box(text: &str, size: f32) -> Option<InlineImageBox> {
    let parts = parts(text)?;
    box_for(parts.width_px, parts.height_px, size)
}

fn box_for(width_px: u32, height_px: u32, size: f32) -> Option<InlineImageBox> {
    let natural_w = width_px as f32 * PDF_IMAGE_DPI_SCALE;
    let natural_h = height_px as f32 * PDF_IMAGE_DPI_SCALE;
    if !(natural_w > 0.0 && natural_h > 0.0 && size > 0.0) {
        return None;
    }
    let scale = (MAX_HEIGHT_EM * size / natural_h)
        .min(MAX_WIDTH_EM * size / natural_w)
        .min(1.0);
    let width = natural_w * scale;
    let height = natural_h * scale;
    Some(InlineImageBox {
        width,
        height,
        descent: (DROP_EM * size).min(height * 0.25),
    })
}

/// Advance of an [`F_IMAGE`] token at `size` points (zero if unresolved).
pub(super) fn advance(text: &str, size: f32) -> f32 {
    image_box(text, size).map_or(0.0, |image| image.width)
}

/// Replace unresolved image tokens by resolved ones when the render's table
/// holds a decodable image for the destination, or by their alt text in the
/// original font slot otherwise (the behavior before inline images).
pub(super) fn resolve_tokens(toks: &mut Vec<Tok>, faces: &Faces) {
    if !toks
        .iter()
        .any(|tok| tok.slot == F_IMAGE && tok.text.starts_with(UNRESOLVED))
    {
        return;
    }
    let table = faces.inline_images.borrow();
    let mut out = Vec::with_capacity(toks.len());
    for mut tok in std::mem::take(toks) {
        if tok.slot != F_IMAGE || !tok.text.starts_with(UNRESOLVED) {
            out.push(tok);
            continue;
        }
        let mut fields = tok.text[UNRESOLVED.len_utf8()..].splitn(3, SEP);
        let alt_slot = fields
            .next()
            .and_then(|slot| slot.parse().ok())
            .unwrap_or(1);
        let dest = fields.next().unwrap_or_default();
        let alt = fields.next().unwrap_or_default();
        match table.get(dest).and_then(Option::as_ref) {
            Some(image) => {
                tok.text = format!(
                    "{}x{}{SEP}{dest}{SEP}{alt}",
                    image.width_px, image.height_px
                );
                out.push(tok);
            }
            None => push_text_tokens(alt, alt_slot, tok.strike, tok.link, &mut out),
        }
    }
    *toks = out;
}

/// Decoded image for a resolved token's destination.
pub(super) fn image_for(faces: &Faces, dest: &str) -> Option<Rc<PdfImageData>> {
    faces.inline_images.borrow().get(dest).cloned().flatten()
}

/// Decode every image the layout will place inside running text, once per
/// destination. Standalone figure paragraphs are skipped: they keep their
/// own (separately decoded) figure lines.
pub(super) fn prepare(blocks: &[Block], assets: &[crate::PdfImageAsset], faces: &Faces) {
    if assets.is_empty() {
        return;
    }
    let mut table = faces.inline_images.borrow_mut();
    let mut pending: Vec<&[Block]> = vec![blocks];
    while let Some(blocks) = pending.pop() {
        for block in blocks {
            match block {
                Block::Paragraph(inlines) => {
                    if standalone_image(inlines).is_none() {
                        collect(inlines, assets, &mut table);
                    }
                }
                Block::Heading { inlines, .. } => collect(inlines, assets, &mut table),
                Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                    pending.push(inner);
                }
                Block::List(list) => {
                    pending.extend(list.items.iter().map(|item| item.blocks.as_slice()));
                }
                Block::Table(table_block) => {
                    for cell in table_block
                        .head
                        .iter()
                        .chain(table_block.rows.iter().flatten())
                    {
                        collect(cell, assets, &mut table);
                    }
                }
                Block::DefinitionList(items) => {
                    for item in items {
                        for inlines in item.terms.iter().chain(&item.definitions) {
                            collect(inlines, assets, &mut table);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn collect(
    inlines: &[Inline],
    assets: &[crate::PdfImageAsset],
    table: &mut BTreeMap<String, Option<Rc<PdfImageData>>>,
) {
    let mut pending: Vec<&[Inline]> = vec![inlines];
    while let Some(inlines) = pending.pop() {
        for inline in inlines {
            match inline {
                Inline::Image { dest, .. } => {
                    let key = dest.trim();
                    if !key.is_empty() && !table.contains_key(key) {
                        let image = resolve_pdf_image(assets, key).map(Rc::new);
                        table.insert(key.to_string(), image);
                    }
                }
                Inline::Emphasis(children)
                | Inline::Strong(children)
                | Inline::Strikethrough(children) => pending.push(children),
                Inline::Link { content, .. } => pending.push(content),
                _ => {}
            }
        }
    }
}

/// The images of a paragraph made only of large images (and whitespace or
/// line breaks between them), so each can keep its own figure line. Badge
/// rows and icons stay inline.
pub(super) fn gallery_images<'a>(
    inlines: &'a [Inline],
    faces: &Faces,
    size: f32,
) -> Option<Vec<&'a Inline>> {
    let mut images = Vec::new();
    let mut large = false;
    for inline in inlines {
        match inline {
            Inline::SoftBreak | Inline::HardBreak => {}
            Inline::Text(text) if text.trim().is_empty() => {}
            _ => {
                let (dest, _, _) = standalone_image(std::slice::from_ref(inline))?;
                let image = image_for(faces, dest.trim())?;
                let natural_h = image.height_px as f32 * PDF_IMAGE_DPI_SCALE;
                large |= natural_h >= GALLERY_MIN_HEIGHT_EM * size;
                images.push(inline);
            }
        }
    }
    (images.len() >= 2 && large).then_some(images)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn badges_keep_natural_size_and_large_pictures_shrink_to_the_line() {
        // A 90x20 px badge at 11 pt: 67.5 x 15 pt, under the 19.25 pt cap.
        let badge = box_for(90, 20, 11.0).unwrap();
        assert!((badge.width - 67.5).abs() < 0.01 && (badge.height - 15.0).abs() < 0.01);
        // A 800x600 px screenshot is capped to 1.75 em tall.
        let picture = box_for(800, 600, 10.0).unwrap();
        assert!((picture.height - 17.5).abs() < 0.01, "{picture:?}");
        assert!((picture.width / picture.height - 800.0 / 600.0).abs() < 0.001);
        // A very wide strip is capped by width instead.
        let strip = box_for(10_000, 10, 10.0).unwrap();
        assert!((strip.width - 360.0).abs() < 0.01, "{strip:?}");
        assert!(box_for(0, 10, 10.0).is_none());
    }

    #[test]
    fn token_text_round_trips_size_destination_and_alt() {
        let text = format!("12x34{SEP}img/a b.png{SEP}Alt text");
        let parts = parts(&text).unwrap();
        assert_eq!((parts.width_px, parts.height_px), (12, 34));
        assert_eq!(parts.dest, "img/a b.png");
        assert_eq!(parts.alt, "Alt text");
        assert_eq!(alt_text(&text), "Alt text");
        assert!(parts_from_unresolved_is_none());
    }

    fn parts_from_unresolved_is_none() -> bool {
        let mut toks = Vec::new();
        push_token("a.png", "A", 1, false, None, &mut toks);
        parts(&toks[0].text).is_none()
    }
}
