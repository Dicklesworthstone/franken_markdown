//! Native builds (`emoji-face`, on with `cli`) draw common emoji with the
//! curated monochrome Noto Emoji face in PDF and SVG instead of `.notdef`
//! boxes; documents without emoji never embed it.
#![cfg(feature = "emoji-face")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{PdfOptions, RenderWarning, parse_markdown, render_pdf, render_warnings};

const README_LINE: &str = "# 🚀 Quick start\n\n✅ Done · ❌ Failed · ⚠️ Warning · 🎉 · 💡 · 📦 · 👍\n\n- 🐛 Fixes\n- ✨ Features\n";

fn embedded_fonts(pdf: &[u8]) -> usize {
    String::from_utf8_lossy(pdf).matches("/FontFile2").count()
}

#[test]
fn common_emoji_render_without_missing_glyph_warnings() {
    let doc = parse_markdown(README_LINE);
    let warnings = render_warnings(&doc, &PdfOptions::default());
    assert!(
        !warnings
            .iter()
            .any(|warning| matches!(warning, RenderWarning::MissingGlyphs { .. })),
        "{warnings:?}"
    );
    let (_, _, svg_warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
    assert!(svg_warnings.is_empty(), "{svg_warnings:?}");
}

#[test]
fn the_emoji_face_is_embedded_only_when_a_document_uses_it() {
    let opts = PdfOptions::default();
    let plain = render_pdf("# Quick start\n\nDone and failed.\n", &opts).unwrap();
    let with_emoji = render_pdf("# Quick start\n\nDone ✅ and failed ❌.\n", &opts).unwrap();
    assert_eq!(embedded_fonts(&with_emoji), embedded_fonts(&plain) + 1);
    // Math symbols keep using the symbol face, not the emoji face.
    let symbols = render_pdf("x ≠ y ⇒ z\n", &opts).unwrap();
    let both = render_pdf("x ≠ y ⇒ z 🚀\n", &opts).unwrap();
    assert_eq!(embedded_fonts(&both), embedded_fonts(&symbols) + 1);
}

#[test]
fn variation_selectors_and_joiners_render_invisibly() {
    // ⚠️ is U+26A0 U+FE0F; 👩‍💻 joins two emoji with U+200D.
    let doc = parse_markdown("⚠\u{FE0F} and 👩\u{200D}💻\n");
    let warnings = render_warnings(&doc, &PdfOptions::default());
    assert!(
        !warnings
            .iter()
            .any(|warning| matches!(warning, RenderWarning::MissingGlyphs { .. })),
        "{warnings:?}"
    );
}

#[test]
fn emoji_pdfs_are_deterministic() {
    let opts = PdfOptions::default();
    assert_eq!(
        render_pdf(README_LINE, &opts).unwrap(),
        render_pdf(README_LINE, &opts).unwrap()
    );
}
