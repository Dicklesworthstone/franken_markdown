//! Images inside running PDF text (badges, icons, small diagrams in prose).
//!
//! A standalone image paragraph keeps its own figure line. Any other image
//! used to print its alt text. Inline images now enter the Knuth-Plass
//! paragraph as unbreakable boxes, like inline formulas: the token's slot is
//! [`F_IMAGE`], which never names a font, and its text encodes the image's
//! natural pixel size, optional containing-width cap, destination and alt text.
//! Encoding the size in the token keeps measurement and line extents a pure
//! function of the laid-out line; the decoded image itself lives in the render's
//! [`Faces`] table, which the writer reads to draw and to emit XObjects.

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
/// Widest inline image at its preferred size. Its actual containing measure
/// can impose a smaller cap during paragraph or table-cell layout.
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
    /// Absolute point limit installed by layout only when the preferred box
    /// exceeds its containing measure. The original image bytes stay intact.
    pub(super) max_width: Option<f32>,
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
        literal: false,
        link,
        strike,
    });
}

pub(super) fn parts(text: &str) -> Option<InlineImageParts<'_>> {
    let (size, rest) = text.split_once(SEP)?;
    let (dest, alt) = rest.split_once(SEP)?;
    let (size, max_width) = if let Some((size, cap)) = size.split_once('@') {
        let width = f32::from_bits(u32::from_str_radix(cap, 16).ok()?);
        if !width.is_finite() || width <= 0.0 {
            return None;
        }
        (size, Some(width))
    } else {
        (size, None)
    };
    let (width, height) = size.split_once('x')?;
    Some(InlineImageParts {
        width_px: width.parse().ok()?,
        height_px: height.parse().ok()?,
        max_width,
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
    let mut image = box_for(parts.width_px, parts.height_px, size)?;
    if let Some(width) = parts.max_width.filter(|&width| width < image.width) {
        let scale = width / image.width;
        image.width = width;
        image.height *= scale;
        image.descent *= scale;
    }
    Some(image)
}

/// Return a fitted copy only for an image that exceeds this measure. A cap
/// travels with the token into every later measurement and paint operation;
/// changing only a segment's advance would leave pixels and links overflowing.
pub(super) fn fitted_token(tok: &Tok, size: f32, max_width: f32) -> Option<Tok> {
    if tok.slot != F_IMAGE || !max_width.is_finite() || max_width <= 0.0 {
        return None;
    }
    let image = image_box(&tok.text, size)?;
    if image.width <= max_width {
        return None;
    }
    let parts = parts(&tok.text)?;
    let mut fitted = tok.clone();
    fitted.text = format!(
        "{}x{}@{:08x}{SEP}{}{SEP}{}",
        parts.width_px,
        parts.height_px,
        max_width.to_bits(),
        parts.dest,
        parts.alt,
    );
    Some(fitted)
}

/// Fit resolved images before building paragraph boxes. Ordinary image tokens
/// are untouched, preserving their preferred geometry and existing output.
pub(super) fn fit_tokens(toks: &mut [Tok], size: f32, max_width: f32) {
    for tok in toks {
        if let Some(fitted) = fitted_token(tok, size, max_width) {
            *tok = fitted;
        }
    }
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
                let (width, height) = image.display_dimensions();
                tok.text = format!("{width}x{height}{SEP}{dest}{SEP}{alt}");
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
                let natural_h = image.display_dimensions().1 as f32 * PDF_IMAGE_DPI_SCALE;
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
    use super::super::{
        F_BODY, FORCED_BREAK_PENALTY, LayoutUnit, PageGeom, ParagraphItem, ParagraphPolicy,
        SvgImageTransform, Theme, WidthCache, append_svg_image_transform_prefix, build_paragraph,
        font_size_of, inline_paragraph_measure, lu_from_points_f32,
    };
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    fn image_token(width: u32, height: u32, dest: &str) -> Tok {
        Tok {
            text: format!("{width}x{height}{SEP}{dest}{SEP}Example image"),
            slot: F_IMAGE,
            space: false,
            hard_break: false,
            literal: false,
            link: Some(7),
            strike: true,
        }
    }

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
        assert_eq!(parts.max_width, None);
        assert_eq!(parts.dest, "img/a b.png");
        assert_eq!(parts.alt, "Alt text");
        assert_eq!(alt_text(&text), "Alt text");
        assert!(parts_from_unresolved_is_none());
    }

    #[test]
    fn fitting_preserves_aspect_ratio_baseline_and_token_ownership() {
        let original = image_token(1200, 20, "images/banner @2x.svg");
        let preferred = image_box(&original.text, 10.0).unwrap();
        assert_eq!(preferred.width, 360.0);
        let fitted = fitted_token(&original, 10.0, 60.0).unwrap();
        let displayed = image_box(&fitted.text, 10.0).unwrap();
        assert_eq!(displayed.width, 60.0);
        assert!((displayed.height - 1.0).abs() < 0.0001);
        assert!((displayed.descent - 0.25).abs() < 0.0001);
        assert_eq!(advance(&fitted.text, 10.0), 60.0);
        assert_eq!(fitted.link, original.link);
        assert_eq!(fitted.strike, original.strike);
        assert_eq!(fitted.slot, F_IMAGE);
        assert!(!fitted.space && !fitted.hard_break);
        let decoded = parts(&fitted.text).unwrap();
        assert_eq!((decoded.width_px, decoded.height_px), (1200, 20));
        assert_eq!(decoded.dest, "images/banner @2x.svg");
        assert_eq!(decoded.alt, "Example image");
        assert_eq!(decoded.max_width, Some(60.0));
        assert_eq!(parts(&original.text).unwrap().max_width, None);
        assert_eq!(image_box(&original.text, 10.0).unwrap(), preferred);
    }

    #[test]
    fn fitting_is_idempotent_and_never_enlarges_a_badge() {
        let original = image_token(90, 20, "badge.png");
        assert!(fitted_token(&original, 11.0, 67.5).is_none());
        assert!(fitted_token(&original, 11.0, 300.0).is_none());
        let fitted = fitted_token(&original, 11.0, 40.0).unwrap();
        assert!(fitted_token(&fitted, 11.0, 40.0).is_none());
        assert!(fitted_token(&fitted, 11.0, 300.0).is_none());
        let narrower = fitted_token(&fitted, 11.0, 20.0).unwrap();
        assert_eq!(image_box(&narrower.text, 11.0).unwrap().width, 20.0);
        // The containing-width cap also survives a later larger font size.
        assert_eq!(image_box(&narrower.text, 24.0).unwrap().width, 20.0);
        let mut prose = original.clone();
        prose.slot = F_BODY;
        assert!(fitted_token(&prose, 11.0, 20.0).is_none());
    }

    #[test]
    fn invalid_width_caps_cannot_enter_an_image_box() {
        let original = image_token(90, 20, "badge.png");
        for width in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(fitted_token(&original, 11.0, width).is_none());
            let malformed = format!("90x20@{:08x}{SEP}badge.png{SEP}Badge", width.to_bits());
            assert!(parts(&malformed).is_none());
            assert!(image_box(&malformed, 11.0).is_none());
        }
        assert!(parts(&format!("90x20@invalid{SEP}badge.png{SEP}Badge")).is_none());
    }

    #[test]
    fn adjacent_badges_gain_only_needed_invisible_breaks() -> crate::Result<()> {
        let faces = Faces::load(&crate::PdfOptions::default())?;
        let cache = RefCell::new(HashMap::new());
        let width_cache = RefCell::new(WidthCache::default());
        let toks = [
            image_token(70, 20, "first.png"),
            image_token(70, 20, "second.png"),
        ];
        for (measure, boxes) in [(140.0, 1), (60.0, 2)] {
            let built = build_paragraph(
                &toks,
                font_size_of(11.0),
                &faces,
                ParagraphPolicy::RAGGED,
                None,
                &cache,
                &width_cache,
                Some(lu_from_points_f32(measure)),
            );
            assert_eq!(
                built
                    .items
                    .iter()
                    .filter(|item| matches!(item, ParagraphItem::Box(_)))
                    .count(),
                boxes
            );
            let discretionary: Vec<_> = built
                .items
                .iter()
                .filter_map(|item| match item {
                    ParagraphItem::Penalty(penalty) if penalty.penalty != FORCED_BREAK_PENALTY => {
                        Some(penalty)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(discretionary.len(), boxes - 1);
            for penalty in discretionary {
                assert_eq!(penalty.width, LayoutUnit::ZERO);
                assert_eq!(penalty.penalty, 0);
                assert!(!penalty.flagged);
            }
            assert!(built.break_toks.iter().all(Option::is_none));
            assert!(
                !built
                    .items
                    .iter()
                    .any(|item| matches!(item, ParagraphItem::Glue(_)))
            );
            let retained: Vec<_> = built
                .item_toks
                .iter()
                .flat_map(|group| group.first.iter().chain(group.rest.iter()))
                .map(|tok| (tok.text.as_str(), tok.link, tok.strike))
                .collect();
            assert_eq!(
                retained,
                toks.iter()
                    .map(|tok| (tok.text.as_str(), tok.link, tok.strike))
                    .collect::<Vec<_>>()
            );
        }
        Ok(())
    }

    #[test]
    fn images_use_the_remaining_indented_measure_below_the_text_floor() {
        let page = PageGeom::from_theme(&Theme::default());
        let indent = page.content_w - 20.0;
        let image = image_token(90, 20, "badge.png");
        assert_eq!(
            inline_paragraph_measure(&[image], page, indent).to_points_f32(),
            20.0
        );
        assert_eq!(
            inline_paragraph_measure(&[], page, indent).to_points_f32(),
            40.0
        );
    }

    #[test]
    fn svg_transform_retains_tiny_scales_without_exponents() {
        for scale in [
            176.0 / 1200.0,
            0.000_014_667,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
        ] {
            let transform = SvgImageTransform {
                sx: scale,
                sy: scale,
                tx: 31.125,
                ty: 98.75,
                viewport_clip: None,
            };
            let mut precise = String::new();
            append_svg_image_transform_prefix(&mut precise, transform);
            let fields: Vec<_> = precise.split_ascii_whitespace().collect();
            assert_eq!(fields.len(), 8, "{precise}");
            assert_eq!(fields[0], "q");
            assert_eq!(fields[7], "cm");
            assert!(!fields[1..7].iter().any(|value| value.contains(['e', 'E'])));
            assert_eq!(fields[1].parse::<f32>().unwrap(), scale);
            assert_eq!(fields[4].parse::<f32>().unwrap(), -scale);
            assert_eq!(fields[5].parse::<f32>().unwrap(), 31.125);
            assert_eq!(fields[6].parse::<f32>().unwrap(), 98.75);
        }
        let mut ordinary = String::new();
        append_svg_image_transform_prefix(
            &mut ordinary,
            SvgImageTransform {
                sx: 0.75,
                sy: 0.75,
                tx: 31.25,
                ty: 98.75,
                viewport_clip: None,
            },
        );
        assert_eq!(ordinary, "q 0.75 0 0 -0.75 31.25 98.75 cm\n");
    }

    fn parts_from_unresolved_is_none() -> bool {
        let mut toks = Vec::new();
        push_token("a.png", "A", 1, false, None, &mut toks);
        parts(&toks[0].text).is_none()
    }
}
