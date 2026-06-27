//! Structural tests for the clean-room PDF MVP. These are intentionally
//! byte-level: they pin deterministic writer invariants without depending on a
//! third-party PDF parser in the clean-room project.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{PdfOptions, render_pdf};

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn pdf_has_valid_header_xref_and_eof_marker() {
    let pdf = render_pdf(
        "# PDF\n\nA paragraph with **strong** text.\n\n- one\n- two\n",
        &PdfOptions::default(),
    )
    .unwrap();

    assert!(pdf.starts_with(b"%PDF-1.7\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));

    let text = as_text(&pdf);
    let startxref_pos = text.rfind("startxref\n").unwrap();
    let number_start = startxref_pos + "startxref\n".len();
    let number_end = text[number_start..].find('\n').unwrap() + number_start;
    let xref_offset: usize = text[number_start..number_end].parse().unwrap();

    assert_eq!(&pdf[xref_offset..xref_offset + 4], b"xref");
    assert!(text.contains("/Type /Catalog"));
    assert!(text.contains("/Type /Pages"));
    assert!(text.contains("/Subtype /Type1 /BaseFont /Helvetica"));
}

#[test]
fn pdf_title_metadata_is_indirect_when_title_is_set() {
    let opts = PdfOptions {
        title: Some("Quarterly Memo".to_string()),
        ..PdfOptions::default()
    };
    let pdf = render_pdf("# PDF\n\nBody.", &opts).unwrap();
    let text = as_text(&pdf);

    assert!(text.contains("/Info "));
    assert!(text.contains(" 0 R"));
    assert!(text.contains("/Title (Quarterly Memo)"));
}
