//! Symbol-fallback regression tests (GH issue #3): common math and arrow
//! glyphs must render as real glyphs — not `.notdef` boxes — in every font
//! profile, via the bundled curated Noto Sans Math fallback face.
//!
//! Structure follows tests/pdf_test.rs: byte-level assertions on the
//! deterministic writer output plus the public `render_warnings` contract.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::theme::FontFamily;
use franken_markdown::{PdfOptions, RenderWarning, parse_markdown, render_pdf, render_warnings};

/// The exact repertoire reported as `.notdef` boxes in GH issue #3, plus the
/// comparison operators any technical document leans on.
const ISSUE_3_REPRO: &str = "Sans: approximate ≈ minus − arrow → double arrow ⇒ left ← multiply × not equal ≠ \
     le ≤ ge ≥ sum ∑ sqrt √ inf ∞";

fn opts_with_family(font: FontFamily) -> PdfOptions {
    let mut opts = PdfOptions::default();
    opts.theme.font = font;
    opts
}

fn missing_glyph_warnings(src: &str, opts: &PdfOptions) -> Vec<(usize, String)> {
    let doc = parse_markdown(src);
    render_warnings(&doc, opts)
        .into_iter()
        .filter_map(|warning| match warning {
            RenderWarning::MissingGlyphs { count, sample } => Some((count, sample)),
            _ => None,
        })
        .collect()
}

#[test]
fn issue_3_repro_chars_have_glyphs_in_both_font_profiles() {
    for font in [FontFamily::Sans, FontFamily::Serif] {
        let opts = opts_with_family(font);
        let warnings = missing_glyph_warnings(ISSUE_3_REPRO, &opts);
        assert!(
            warnings.is_empty(),
            "{font:?}: no missing-glyph warning expected after symbol fallback, got {warnings:?}"
        );
        // The render itself must also succeed and embed the fallback face.
        let pdf = render_pdf(ISSUE_3_REPRO, &opts).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(
            text.contains("/F6"),
            "{font:?}: the symbol fallback face (/F6) should be embedded and referenced"
        );
    }
}

#[test]
fn symbol_fallback_covers_code_blocks_tables_and_headings() {
    let src = "# Drift ⇒ halt\n\n\
               | col ⇒ | value |\n| --- | --- |\n| a ≠ b | ∑ = 10 |\n\n\
               ```rust\nlet delta = a − b; // a ⇒ b, x ≤ y\n```\n";
    for font in [FontFamily::Sans, FontFamily::Serif] {
        let opts = opts_with_family(font);
        let warnings = missing_glyph_warnings(src, &opts);
        assert!(
            warnings.is_empty(),
            "{font:?}: headings/tables/code must fall back too, got {warnings:?}"
        );
        let pdf = render_pdf(src, &opts).unwrap();
        assert!(
            String::from_utf8_lossy(&pdf).contains("/F6"),
            "{font:?}: fallback face must be embedded"
        );
    }
}

#[test]
fn genuinely_unsupported_glyphs_still_warn() {
    // Neither the text faces nor the curated symbol fallback cover emoji, so
    // the degraded-output warning must keep firing.
    let warnings = missing_glyph_warnings("emoji: 😀", &PdfOptions::default());
    assert_eq!(warnings.len(), 1, "one missing-glyph warning expected");
    let (count, sample) = &warnings[0];
    assert_eq!(*count, 1);
    assert_eq!(sample, "😀");
}

#[test]
fn documents_without_fallback_chars_do_not_embed_the_symbol_face() {
    // Pure-ASCII output must stay byte-identical to the pre-fallback writer:
    // the symbol face is embedded only when a run actually uses it.
    let pdf = render_pdf("plain ascii only", &PdfOptions::default()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        !text.contains("/F6"),
        "an ASCII-only document must not reference the fallback face"
    );
}

#[test]
fn fallback_render_is_deterministic() {
    let opts = opts_with_family(FontFamily::Serif);
    let first = render_pdf(ISSUE_3_REPRO, &opts).unwrap();
    let second = render_pdf(ISSUE_3_REPRO, &opts).unwrap();
    assert_eq!(
        first, second,
        "fallback-bearing renders must stay deterministic"
    );
}

#[test]
fn bundled_symbol_face_parses_and_covers_the_required_repertoire() {
    let font = franken_markdown::fonts::load_symbol().unwrap();
    assert!(
        font.has_glyf_outlines(),
        "fallback face must be subsettable"
    );
    for c in [
        '×', '÷', '±', '°', '·', '−', '→', '←', '↔', '⇐', '⇒', '⇔', '≈', '≠', '≡', '≤', '≥', '∑',
        '∏', '√', '∞', '∫', '∂', '∈', '∉', '∅', '∧', '∨', '⊂', '⊃', '⊕', '⊗', '⟵', '⟶', '⟹',
    ] {
        assert_ne!(
            font.glyph_index(c),
            0,
            "symbol fallback face must map {c:?} (U+{:04X})",
            u32::from(c)
        );
    }
}

#[test]
fn fallback_symbols_remain_selectable_text() {
    // The ⇒ run must keep a ToUnicode mapping: the CID for the fallback glyph
    // maps back to U+21D2 so extraction/selection still sees the character.
    let opts = opts_with_family(FontFamily::Sans);
    let pdf = render_pdf("drift ⇒ halt", &opts).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("21D2"),
        "ToUnicode CMap must map the fallback glyph back to U+21D2"
    );
}
