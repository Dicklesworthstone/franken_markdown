//! Validates the embedded-font PDF path end to end, in the clean room: the
//! `FontFile2` programs written into the PDF must re-parse as valid subset fonts
//! with our own reader, and the document must stay tiny + deterministic.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::Font;
use franken_markdown::{PdfOptions, render_pdf};

const DOC: &str = "# Embedding\n\nA paragraph with **bold** and *italic* words, plus \
`inline code`.\n\n```rust\nfn main() {}\n```\n\n- alpha\n- beta\n";

/// Pull every `stream ... endstream` blob and keep the ones that parse as fonts
/// (the FontFile2 programs); content + ToUnicode streams fail `Font::parse`.
fn embedded_fonts(pdf: &[u8]) -> Vec<Font> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = find(&pdf[i..], b"stream\n") {
        let start = i + rel + b"stream\n".len();
        let Some(erel) = find(&pdf[start..], b"\nendstream") else {
            break;
        };
        let blob = &pdf[start..start + erel];
        if let Ok(font) = Font::parse(blob.to_vec()) {
            out.push(font);
        }
        i = start + erel + b"\nendstream".len();
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn embedded_fontfile2_programs_are_valid_subsets() {
    let pdf = render_pdf(DOC, &PdfOptions::default()).unwrap();
    let fonts = embedded_fonts(&pdf);

    // Body, bold, italic, and mono are all exercised by DOC -> several faces.
    assert!(
        fonts.len() >= 3,
        "expected several embedded faces, got {}",
        fonts.len()
    );
    for font in &fonts {
        assert!(
            font.has_glyf_outlines(),
            "embedded face keeps glyf outlines"
        );
        // Subsets are small — far fewer glyphs than a full face.
        assert!(
            font.num_glyphs < 120,
            "embedded face is a subset ({} glyphs)",
            font.num_glyphs
        );
        assert!(font.units_per_em > 0);
    }
}

#[test]
fn embedded_pdf_is_tiny_and_deterministic() {
    let a = render_pdf(DOC, &PdfOptions::default()).unwrap();
    let b = render_pdf(DOC, &PdfOptions::default()).unwrap();
    assert_eq!(a, b, "PDF output is deterministic");
    // Embedded subset fonts, yet still small.
    assert!(
        a.len() < 60_000,
        "subset-embedded PDF stays tiny ({} bytes)",
        a.len()
    );
    assert!(a.starts_with(b"%PDF-1.7\n") && a.ends_with(b"%%EOF\n"));
}
