//! Cached bundled faces must render exactly like the same caller-supplied bytes.
//! Alternating families in one process catches accidental cache keys based on
//! the per-render Cow storage address instead of the underlying static face.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::{FontAssets, FontFamily, PdfOptions, render_pdf};

fn owned_faces(family: FontFamily) -> FontAssets {
    FontAssets {
        body_regular: Some(fonts::body_bytes(family, FontStyle::Regular).to_vec()),
        body_bold: Some(fonts::body_bytes(family, FontStyle::Bold).to_vec()),
        body_italic: Some(fonts::body_bytes(family, FontStyle::Italic).to_vec()),
        body_bold_italic: Some(fonts::body_bytes(family, FontStyle::BoldItalic).to_vec()),
        mono_regular: Some(fonts::mono_bytes(FontStyle::Regular).to_vec()),
        ..FontAssets::default()
    }
}

#[test]
fn bundled_font_cache_preserves_pdf_bytes_when_families_change() {
    let source = "The of**fi**ce reviewed cash[flow](https://example.com) and \
        re~~vised~~ forecasts while ∑revenue grew. The team considered \
        a**ffi**ne models and re*financing* terms before updating the report. ";
    let source = source.repeat(6);
    // Both directions, repeated in the same process, including glyph expansion.
    // The owned-font reference never uses registry-address memoization.
    for expansion in [0, 15] {
        for family in [
            FontFamily::Sans,
            FontFamily::Serif,
            FontFamily::Sans,
            FontFamily::Serif,
        ] {
            let mut options = PdfOptions::default();
            options.theme.font = family;
            options.microtype.max_expansion_per_mille = expansion;
            let mut reference_options = options.clone();
            reference_options.font_assets = owned_faces(family);
            let reference = render_pdf(&source, &reference_options).unwrap();
            let cached = render_pdf(&source, &options).unwrap();
            let first_difference = reference
                .iter()
                .zip(&cached)
                .position(|(expected, actual)| expected != actual);
            assert!(
                reference == cached,
                "{family:?}, expansion {expansion}: cached font changed PDF glyphs/metrics; \
                 reference {} bytes, cached {} bytes, first difference {first_difference:?}",
                reference.len(),
                cached.len()
            );
        }
    }
}
