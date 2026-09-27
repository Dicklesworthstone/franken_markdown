//! Canonical Latin glyph selection while retaining the original PDF text.
//!
//! This is a bounded, font-aware composition profile, not general Unicode
//! normalization. A complete base/mark chain must compose into a glyph in the
//! requested face. Unsupported chains remain intact, with no partial accent
//! consumption, compatibility mappings, or implicit font changes.

use crate::fonts::BundledFlowFonts;
use crate::text::Font;
use std::borrow::Cow;

const MAX_MARKS: usize = 64;

pub(super) fn is_mark(ch: char) -> bool {
    matches!(ch as u32, 0x0300..=0x036f)
}

/// Iterate exact source slices, keeping every base and its following marks
/// together. A leading run of orphan marks is also one indivisible slice.
/// This only recognizes the combining-mark range admitted by this profile;
/// it does not claim to provide general Unicode grapheme segmentation.
pub(super) fn source_clusters(source: &str) -> impl Iterator<Item = &str> {
    let mut remaining = source;
    std::iter::from_fn(move || {
        let mut chars = remaining.char_indices();
        chars.next()?;
        let end = chars
            .find_map(|(offset, ch)| (!is_mark(ch)).then_some(offset))
            .unwrap_or(remaining.len());
        let (cluster, rest) = remaining.split_at(end);
        remaining = rest;
        Some(cluster)
    })
}

/// Select transient glyph text without mutating source spans or extraction
/// text. Callers must preserve `source` for `/ActualText` when this is owned.
/// Admission depends only on each cluster and the actual face, so shaping a
/// word, line, or full paragraph produces the same composition decisions.
pub(super) fn glyph_text<'a>(source: &'a str, font: &Font) -> Cow<'a, str> {
    compose(source, |ch| {
        let gid = font.glyph_index(ch);
        gid != 0 && gid < font.num_glyphs
    })
}

fn compose(source: &str, covers: impl Fn(char) -> bool) -> Cow<'_, str> {
    if source.is_ascii() || !source.chars().any(is_mark) {
        return Cow::Borrowed(source);
    }

    let mut result: Option<String> = None;
    let mut offset = 0;
    for cluster in source_clusters(source) {
        if let Some(composite) = complete_composite(cluster).filter(|&ch| covers(ch)) {
            let text = result.get_or_insert_with(|| {
                let mut text = String::with_capacity(source.len());
                text.push_str(&source[..offset]);
                text
            });
            text.push(composite);
        } else if let Some(text) = result.as_mut() {
            text.push_str(cluster);
        }
        offset += cluster.len();
    }
    result.map_or(Cow::Borrowed(source), Cow::Owned)
}

/// Return a replacement only if every mark in the source cluster is consumed.
/// The caller still checks that the final composite exists in its actual face.
fn complete_composite(cluster: &str) -> Option<char> {
    let mut chars = cluster.chars();
    let mut base = chars.next()?;
    if is_mark(base) {
        return None;
    }
    let mut marks = 0;
    for mark in chars {
        marks += 1;
        if marks > MAX_MARKS || !is_mark(mark) {
            return None;
        }
        base = BundledFlowFonts::canonical_latin_composite(base, mark)?;
    }
    (marks > 0).then_some(base)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::fonts::{self, FontStyle};
    use crate::theme::FontFamily;

    #[test]
    fn source_clusters_retain_exact_utf8_and_orphan_mark_runs() {
        let source = "\u{0301}\u{0308}λCafe\u{0301}A\u{030a}\u{0301}∑a\u{034f}";
        let clusters: Vec<_> = source_clusters(source).collect();
        assert_eq!(
            clusters,
            [
                "\u{0301}\u{0308}",
                "λ",
                "C",
                "a",
                "f",
                "e\u{0301}",
                "A\u{030a}\u{0301}",
                "∑",
                "a\u{034f}",
            ]
        );
        assert_eq!(clusters.concat(), source);
        assert_eq!(source_clusters("").next(), None);
    }

    #[test]
    fn unchanged_text_borrows_the_original_source() {
        for source in ["", "ordinary office text", "Café λ∑", "ﬃ ① ²"] {
            assert!(matches!(compose(source, |_| true), Cow::Borrowed(value) if value == source));
        }
    }

    #[test]
    fn complete_chains_compose_without_compatibility_normalization() {
        assert_eq!(
            compose("λCafe\u{0301} A\u{030a} u\u{0308}\u{0301} ﬃ①²", |_| true),
            "λCafé Å ǘ ﬃ①²"
        );
        assert_eq!(compose("U\u{0308}\u{0304}", |_| true), "Ǖ");
    }

    #[test]
    fn unsupported_or_uncovered_chains_remain_whole() {
        for source in [
            "\u{0301}a",
            "a\u{0301}\u{0307}",
            "e\u{0301}\u{0301}",
            "a\u{034f}",
        ] {
            assert!(matches!(compose(source, |_| true), Cow::Borrowed(value) if value == source));
        }
        assert!(matches!(
            compose("e\u{0301}", |_| false),
            Cow::Borrowed("e\u{0301}")
        ));
        let unsupported = "a\u{0301}\u{0307}";
        let source = format!("e\u{0301} {unsupported} A\u{030a}");
        assert_eq!(compose(&source, |_| true), format!("é {unsupported} Å"));
        let excessive = format!("e{}", "\u{0301}".repeat(MAX_MARKS + 1));
        assert!(
            matches!(compose(&excessive, |_| true), Cow::Borrowed(value) if value == excessive)
        );
    }

    #[test]
    fn bundled_faces_choose_the_precomposed_glyph_text() {
        let source = "office cafe\u{0301}\tA\u{030a}ngstro\u{0308}m";
        let canonical = "office café\tÅngström";
        for family in [FontFamily::Sans, FontFamily::Serif] {
            for style in [
                FontStyle::Regular,
                FontStyle::Bold,
                FontStyle::Italic,
                FontStyle::BoldItalic,
            ] {
                let font = fonts::body_font(family, style).unwrap();
                assert_eq!(glyph_text(source, font), canonical, "{family:?} {style:?}");
            }
        }
        assert_eq!(
            glyph_text(source, fonts::mono_font(FontStyle::Regular).unwrap()),
            canonical
        );
    }

    #[test]
    fn actual_face_coverage_and_segmentation_determine_admission() {
        let symbol = fonts::symbol_font().unwrap();
        assert_eq!(symbol.glyph_index('é'), 0);
        assert!(matches!(
            glyph_text("e\u{0301}", symbol),
            Cow::Borrowed("e\u{0301}")
        ));

        let font = fonts::body_font(FontFamily::Sans, FontStyle::Regular).unwrap();
        let source = format!("{}cafe\u{0301}", "a".repeat(5000));
        let whole = glyph_text(&source, font);
        let by_cluster = source_clusters(&source)
            .map(|cluster| glyph_text(cluster, font))
            .collect::<String>();
        assert_eq!(whole, by_cluster);
        assert!(whole.ends_with("café"));
    }
}
