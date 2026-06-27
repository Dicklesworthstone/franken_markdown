//! Bundled font registry tests. May use `unwrap`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::{FontStyle, body_bytes, load_body, load_mono, mono_bytes};
use franken_markdown::theme::FontFamily;

#[test]
fn font_style_from_flags() {
    assert_eq!(FontStyle::new(false, false), FontStyle::Regular);
    assert_eq!(FontStyle::new(true, false), FontStyle::Bold);
    assert_eq!(FontStyle::new(false, true), FontStyle::Italic);
    assert_eq!(FontStyle::new(true, true), FontStyle::BoldItalic);
}

#[test]
fn every_bundled_face_parses_with_outlines() {
    for fam in [FontFamily::Sans, FontFamily::Serif] {
        for st in [
            FontStyle::Regular,
            FontStyle::Bold,
            FontStyle::Italic,
            FontStyle::BoldItalic,
        ] {
            let font = load_body(fam, st).unwrap_or_else(|_| panic!("{fam:?}/{st:?} should parse"));
            assert!(font.has_glyf_outlines(), "{fam:?}/{st:?} subsettable");
            assert_ne!(font.glyph_index('A'), 0, "{fam:?}/{st:?} maps 'A'");
            assert!(!body_bytes(fam, st).is_empty());
        }
    }
    let mono = load_mono(FontStyle::Regular).unwrap();
    assert!(mono.has_glyf_outlines());
    assert_eq!(
        mono.advance_1000('i'),
        mono.advance_1000('W'),
        "mono face is monospaced"
    );
}

#[test]
fn bundled_face_is_subsettable() {
    // The registry's bytes flow straight into the subsetter the embedder will use.
    let serif = load_body(FontFamily::Serif, FontStyle::Regular).unwrap();
    let sub = serif
        .subset(&['f', 'i', '(', ')', 'x'])
        .expect("subset bundled serif");
    assert!(
        sub.len() < body_bytes(FontFamily::Serif, FontStyle::Regular).len() / 3,
        "subset is much smaller than the full face"
    );
}

#[test]
fn families_and_styles_are_distinct_data() {
    assert_ne!(
        body_bytes(FontFamily::Sans, FontStyle::Regular),
        body_bytes(FontFamily::Serif, FontStyle::Regular),
        "sans != serif"
    );
    assert_ne!(
        body_bytes(FontFamily::Sans, FontStyle::Regular),
        body_bytes(FontFamily::Sans, FontStyle::Bold),
        "regular != bold"
    );
    assert!(!mono_bytes(FontStyle::Regular).is_empty());
}
