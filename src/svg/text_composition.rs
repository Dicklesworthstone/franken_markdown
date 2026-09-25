//! Canonical glyph selection without changing the original SVG source spans.
//!
//! Reuse the reader's composition data. A complete base/mark chain must compose
//! into a glyph in the actual requested face; partial composition, accent
//! removal, compatibility mappings and cross-face attachment are not allowed.

use super::{RStyle, ShapedText, Shaper};
use franken_markdown::fonts::BundledFlowFonts;
use std::ops::Range;

const MAX_SCALARS: usize = 4096;

struct Unit {
    glyph_text: Range<usize>,
    source: Range<usize>,
}

struct Composed {
    text: String,
    units: Vec<Unit>,
}

pub(super) fn is_mark(ch: char) -> bool {
    matches!(ch as u32, 0x0300..=0x036f)
}

pub(super) fn shape(
    shaper: &Shaper<'_>, source: &str, style: RStyle, size: f64,
) -> Option<ShapedText> {
    if !source.chars().any(is_mark) {
        return None;
    }
    let primary = if style.mono { 4 } else { usize::from(style.bold) + 2 * usize::from(style.italic) };
    let face = shaper.poster.faces[primary].as_ref()?;
    let composed = compose(source, |ch| {
        let id = face.glyph_index(ch);
        id != 0 && id < face.num_glyphs
    })?;
    let mut shaped = shaper.shape_uncomposed(&composed.text, style, size);
    // Substituted ligatures may cover several composed units. Map both ends
    // of each surviving cluster, never assume one glyph per input scalar.
    for cluster in &mut shaped.clusters {
        let first = composed.units.binary_search_by_key(
            &cluster.bytes.start, |unit| unit.glyph_text.start,
        ).ok()?;
        let last = composed.units.binary_search_by_key(
            &cluster.bytes.end, |unit| unit.glyph_text.end,
        ).ok()?;
        if first > last { return None; }
        cluster.bytes = composed.units[first].source.start..composed.units[last].source.end;
    }
    Some(shaped)
}

fn compose(source: &str, covers: impl Fn(char) -> bool) -> Option<Composed> {
    if source.chars().take(MAX_SCALARS + 1).count() > MAX_SCALARS {
        return None;
    }
    let mut text = String::with_capacity(source.len());
    let mut units = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((start, mut base)) = chars.next() {
        if is_mark(base) { return None; }
        let mut end = start + base.len_utf8();
        let mut count = 0;
        while let Some(&(offset, mark)) = chars.peek() {
            if !is_mark(mark) { break; }
            count += 1;
            if count > 64 { return None; }
            base = BundledFlowFonts::canonical_latin_composite(base, mark)?;
            end = offset + mark.len_utf8();
            chars.next();
        }
        if count > 0 && !covers(base) { return None; }
        let glyph_start = text.len();
        text.push(base);
        units.push(Unit { glyph_text: glyph_start..text.len(), source: start..end });
    }
    Some(Composed { text, units })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use super::super::super::super::{Op, Piece, Poster, SvgOptions};
    use franken_markdown::theme::FontFamily;

    #[test]
    fn canonical_glyph_selection_has_exact_original_utf8_ranges() {
        let source = "λCafe\u{0301}A\u{030a}∑";
        let result = compose(source, |_| true).unwrap();
        assert_eq!(result.text, "λCaféÅ∑");
        assert_eq!(result.units[4].glyph_text, 5..7);
        assert_eq!(result.units[4].source, 5..8);
        assert_eq!(result.units[5].glyph_text, 7..9);
        assert_eq!(result.units[5].source, 8..11);
        assert_eq!(result.units[6].source, 11..14);
        assert_eq!(compose("u\u{0308}\u{0301}", |_| true).unwrap().text, "ǘ");
    }

    #[test]
    fn unsupported_chains_and_missing_composites_do_not_partially_compose() {
        for source in ["\u{0301}a", "a\u{0301}\u{0307}", "e\u{0301}\u{0301}", "a\u{034f}"] {
            assert!(compose(source, |_| true).is_none());
        }
        assert!(compose("e\u{0301}", |_| false).is_none());
        assert!(compose(&format!("{}e\u{0301}", "a".repeat(MAX_SCALARS)), |_| true).is_none());
        assert_eq!(compose("ﬃ①²", |_| true).unwrap().text, "ﬃ①²");
    }

    #[test]
    fn bundled_styles_paint_the_same_glyphs_as_precomposed_spelling() {
        for family in [FontFamily::Sans, FontFamily::Serif] {
            let mut options = SvgOptions::default();
            options.theme.font = family;
            for (bold, italic, mono) in [(false, false, false), (true, false, false),
                (false, true, false), (true, true, false), (false, false, true)]
            {
                let style = RStyle { bold, italic, mono, ..RStyle::BODY };
                let mut actual = Poster::new(&options);
                let mut expected = Poster::new(&options);
                let a = Shaper::new(&actual).shape("officecafe\u{0301}", style, 14.0);
                let b = Shaper::new(&expected).shape("officecafé", style, 14.0);
                assert_eq!(a.width(), b.width());
                assert_eq!(a.clusters.last().unwrap().bytes.end, "officecafe\u{0301}".len());
                assert_eq!(a.paint(&mut actual, 0.0, 20.0, style, 14.0),
                    b.paint(&mut expected, 0.0, 20.0, style, 14.0));
                assert_eq!(actual.ops, expected.ops);
                assert_eq!(actual.missing, 0);
            }
        }
    }

    #[test]
    fn same_style_ast_fragments_do_not_separate_a_base_from_its_accent() {
        let poster = Poster::new(&SvgOptions::default());
        let split = [Piece::Text("caf".into(), RStyle::BODY),
            Piece::Text("e".into(), RStyle::BODY), Piece::Text("\u{0301}".into(), RStyle::BODY)];
        let whole = [Piece::Text("cafe\u{0301}".into(), RStyle::BODY)];
        let a = poster.wrap(&split, 11.0, 100.0);
        let b = poster.wrap(&whole, 11.0, 100.0);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].len(), 1);
        assert_eq!(a[0][0].text, "cafe\u{0301}");
        assert_eq!(poster.words_width(&a[0], 11.0), poster.words_width(&b[0], 11.0));
    }

    #[test]
    fn emergency_wrapping_preserves_decomposed_text_and_cluster_boundaries() {
        let mut poster = Poster::new(&SvgOptions::default());
        let source = "officecafe\u{0301}".repeat(40);
        let lines = poster.wrap(&[Piece::Text(source.clone(), RStyle::BODY)], 11.0, 35.0);
        assert!(lines.len() > 5);
        assert_eq!(lines.iter().flatten().map(|word| word.text.as_str()).collect::<String>(), source);
        for line in &lines {
            assert!(poster.words_width(line, 11.0) <= 35.0 + 0.00001);
            assert!(!line[0].text.chars().next().is_some_and(is_mark));
            poster.draw_words(line, 0.0, 20.0, 11.0);
        }
        assert_eq!(poster.missing, 0);
        assert!(poster.ops.iter().any(|op| matches!(op, Op::Glyph { .. })));
    }
}
