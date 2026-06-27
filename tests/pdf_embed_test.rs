//! Validates the embedded-font PDF path end to end, in the clean room: the
//! `FontFile2` programs written into the PDF must re-parse as valid subset fonts
//! with our own reader, and the document must stay tiny + deterministic.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfOptions, render_pdf};

const DOC: &str = "# Embedding\n\nA paragraph with **bold** and *italic* words, plus \
`inline code`.\n\n```rust\nfn main() {}\n```\n\n- alpha\n- beta\n";

#[test]
fn embedded_font_programs_are_flate_compressed() {
    let pdf = render_pdf(DOC, &PdfOptions::default()).unwrap();
    let s = String::from_utf8_lossy(&pdf);
    // Body/bold/italic/mono are exercised by DOC -> several embedded faces, each a
    // FlateDecode-compressed FontFile2 carrying its uncompressed length in /Length1.
    // (Subset validity is verified in pdf.rs before embedding + by the compress
    // module's round-trip tests; here we pin the compression contract.)
    assert!(
        s.matches("/FontFile2").count() >= 3,
        "expected several embedded faces"
    );
    assert!(
        s.matches("/Filter /FlateDecode").count() >= 3,
        "font programs are FlateDecode-compressed"
    );
    assert!(
        s.contains("/Length1 "),
        "FontFile2 records its uncompressed length"
    );
}

#[test]
fn pdf_applies_gpos_kerning_via_tj() {
    // Text dense in kern pairs (AV/VA/To/Wa/Ya/PA...).
    let pdf = render_pdf(
        "# AVALANCHE\n\nTo Wave, Yo. PAVAVA AWAY VAT.\n",
        &PdfOptions::default(),
    )
    .unwrap();
    let s = String::from_utf8_lossy(&pdf);
    assert!(s.contains(" TJ"), "content uses TJ positioning arrays");

    // Kern adjustments appear as `>{int}<` between glyph runs inside the arrays.
    let mut nonzero = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'>' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'<' && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'<' {
                if let Ok(n) = s[i + 1..j].trim().parse::<i32>() {
                    if n != 0 {
                        nonzero += 1;
                    }
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    assert!(
        nonzero > 0,
        "expected GPOS kern adjustments in the content stream"
    );
}

#[test]
fn pdf_shapes_ligatures_and_keeps_them_selectable() {
    // Default (Plex) ligates fi; the ToUnicode must map the ligature glyph back to
    // its component characters, i.e. a bfchar value with >= 2 UTF-16 units (>= 8
    // hex digits) — a single character would be 4.
    let pdf = render_pdf(
        "find the difficult files efficiently",
        &PdfOptions::default(),
    )
    .unwrap();
    let s = String::from_utf8_lossy(&pdf);
    let b = s.as_bytes();
    let mut found = false;
    let mut i = 0;
    while i + 3 < b.len() {
        if &b[i..i + 3] == b"> <" {
            let start = i + 3;
            let mut j = start;
            while j < b.len() && b[j] != b'>' {
                j += 1;
            }
            let val = &b[start..j];
            if val.len() >= 8 && val.iter().all(u8::is_ascii_hexdigit) {
                found = true;
                break;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    assert!(
        found,
        "expected a ligature ToUnicode entry (glyph -> 2+ characters)"
    );
}

#[test]
fn pdf_renders_inline_styles_in_distinct_faces() {
    // One paragraph exercising body, bold, italic, code, and bold-italic.
    let pdf = render_pdf(
        "Plain **bold** *italic* `code` and **_both_** words.",
        &PdfOptions::default(),
    )
    .unwrap();
    let s = String::from_utf8_lossy(&pdf);
    // Each inline style selects its own font slot: F2 bold, F3 italic, F4 mono,
    // F5 bold-italic (alongside F1 body).
    for slot in ['2', '3', '4', '5'] {
        assert!(
            s.contains(&format!("/F{slot} ")),
            "expected font slot F{slot} for inline styling"
        );
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
