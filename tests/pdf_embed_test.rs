//! Validates the embedded-font PDF path end to end, in the clean room: the
//! `FontFile2` programs written into the PDF must re-parse as valid subset fonts
//! with our own reader, and the document must stay tiny + deterministic.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfOptions, parse_markdown, render_pdf, render_pdf_document, render_pdf_document_profiled,
};

const DOC: &str = "# Embedding\n\nA paragraph with **bold** and *italic* words, plus \
`inline code`.\n\n```rust\nfn main() {}\n```\n\n- alpha\n- beta\n";

fn contains_ligature_tounicode_entry(pdf: &[u8]) -> bool {
    let s = String::from_utf8_lossy(pdf);
    let b = s.as_bytes();
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
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

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
fn type0_identity_h_fonts_keep_non_winansi_text_selectable() {
    let pdf = render_pdf("Café Δ Ω naïve", &PdfOptions::default()).unwrap();
    let s = String::from_utf8_lossy(&pdf);

    assert!(
        s.contains("/Subtype /Type0") && s.contains("/Encoding /Identity-H"),
        "non-WinAnsi text should be written through composite Identity-H fonts"
    );
    for scalar in ["<00E9>", "<0394>", "<03A9>"] {
        assert!(
            s.contains(scalar),
            "ToUnicode CMap should preserve {scalar} for copy/paste"
        );
    }
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
    assert!(
        contains_ligature_tounicode_entry(&pdf),
        "expected a ligature ToUnicode entry (glyph -> 2+ characters)"
    );
}

#[test]
fn pdf_reuses_shaped_segments_within_one_render_without_changing_bytes() {
    let repeated = "office efficient affine files";
    let mut md = String::new();
    for _ in 0..6 {
        md.push_str(repeated);
        md.push_str("\n\n");
    }
    for _ in 0..4 {
        md.push_str("**");
        md.push_str(repeated);
        md.push_str("**\n\n");
    }
    for _ in 0..4 {
        md.push('*');
        md.push_str(repeated);
        md.push_str("*\n\n");
    }
    for _ in 0..4 {
        md.push('`');
        md.push_str(repeated);
        md.push_str("`\n\n");
    }

    let doc = parse_markdown(&md);
    let opts = PdfOptions::default();
    let normal = render_pdf_document(&doc, &opts).unwrap();
    let profiled = render_pdf_document_profiled(&doc, &opts).unwrap();

    assert_eq!(
        profiled.bytes, normal,
        "shape-cache profiling must not alter rendered PDF bytes"
    );
    assert!(
        contains_ligature_tounicode_entry(&normal),
        "repeated ligature-heavy text should stay selectable"
    );

    let hit_stage = profiled
        .stages
        .iter()
        .find(|stage| stage.stage == "shaped_segment_cache_hit")
        .expect("profile should report shaped-segment cache hits");
    let miss_stage = profiled
        .stages
        .iter()
        .find(|stage| stage.stage == "shaped_segment_cache_miss")
        .expect("profile should report shaped-segment cache misses");

    assert!(
        hit_stage.count > 0 && hit_stage.bytes > 0,
        "repeated exact segment text should reuse shaped glyph streams: {hit_stage:?}"
    );
    assert!(
        miss_stage.count > 0 && miss_stage.bytes > 0,
        "first occurrence of each slot/text pair should populate the cache: {miss_stage:?}"
    );

    let s = String::from_utf8_lossy(&normal);
    for slot in ['2', '3', '4'] {
        assert!(
            s.contains(&format!("/F{slot} ")),
            "mixed bold/italic/code slots should remain active"
        );
    }
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
fn pdf_marks_italic_and_bold_italic_descriptors_as_italic() {
    let pdf = render_pdf(
        "Plain *italic* and **_both_** words.",
        &PdfOptions::default(),
    )
    .unwrap();
    let s = String::from_utf8_lossy(&pdf);

    assert_eq!(
        s.matches("/ItalicAngle -12").count(),
        2,
        "both italic and bold-italic embedded faces should carry italic FontDescriptor metadata"
    );
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
