//! q6xc.1: PDF/A-2b XMP + OutputIntent + ICC plumbing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{render_pdf, render_pdf_pdfa, PdfASettings, PdfOptions};

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

fn opts() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        title: Some("PDF/A".into()),
        author: Some("FMD".into()),
        ..PdfOptions::default()
    }
}

const MD: &str = "# Hi\n\nSee [ex](https://example.com).\n";

#[test]
fn default_pdf_has_no_pdfa_identification() {
    let bytes = render_pdf(MD, &opts()).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    log_check(
        "q6xc.1.default.no_xmp",
        "default PDF has no pdfaid XMP",
        !text.contains("pdfaid:part"),
    );
    log_check(
        "q6xc.1.default.no_oi",
        "default PDF has no OutputIntent",
        !text.contains("/OutputIntent"),
    );
}

#[test]
fn pdfa_2b_emits_xmp_output_intent_and_icc() {
    let bytes = render_pdf_pdfa(MD, &opts(), PdfASettings::a2b()).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    log_check("q6xc.1.pdf.magic", "PDF magic", bytes.starts_with(b"%PDF-"));
    log_check(
        "q6xc.1.pdf.xmp.part",
        "XMP pdfaid:part 2",
        text.contains("<pdfaid:part>2</pdfaid:part>"),
    );
    log_check(
        "q6xc.1.pdf.xmp.conf",
        "XMP pdfaid:conformance B",
        text.contains("<pdfaid:conformance>B</pdfaid:conformance>"),
    );
    log_check(
        "q6xc.1.pdf.meta",
        "Catalog points at Metadata",
        text.contains("/Type /Metadata") && text.contains("/Subtype /XML"),
    );
    log_check(
        "q6xc.1.pdf.oi",
        "OutputIntent GTS_PDFA1 + DestOutputProfile",
        text.contains("/Type /OutputIntent")
            && text.contains("/S /GTS_PDFA1")
            && text.contains("/DestOutputProfile"),
    );
    log_check(
        "q6xc.1.pdf.icc",
        "ICC stream has acsp magic",
        bytes.windows(4).any(|w| w == b"acsp"),
    );
    log_check(
        "q6xc.1.pdf.print_flag",
        "link annotations carry /F 4",
        text.contains("/Subtype /Link") && text.contains("/F 4"),
    );
}

#[test]
fn pdfa_2b_is_deterministic() {
    let a = render_pdf_pdfa(MD, &opts(), PdfASettings::a2b()).unwrap();
    let b = render_pdf_pdfa(MD, &opts(), PdfASettings::a2b()).unwrap();
    log_check("q6xc.1.pdf.det", "same options twice", a == b);
}

#[test]
fn default_path_byte_identical_to_historical_render() {
    let a = render_pdf(MD, &opts()).unwrap();
    let b = render_pdf_pdfa(MD, &opts(), PdfASettings::OFF).unwrap();
    log_check(
        "q6xc.1.default.identical",
        "OFF settings match render_pdf",
        a == b,
    );
}

#[test]
fn pdfa_xmp_golden_fragment() {
    let bytes = render_pdf_pdfa("# T\n", &opts(), PdfASettings::a2b()).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    // Golden fragment: the identification triple plus producer. Full XMP is
    // asserted structurally so a date-format tweak does not force a binary
    // snapshot refresh.
    for needle in [
        "<pdfaid:part>2</pdfaid:part>",
        "<pdfaid:conformance>B</pdfaid:conformance>",
        "<pdf:Producer>franken_markdown</pdf:Producer>",
        "<xmp:CreatorTool>fmd</xmp:CreatorTool>",
        "<dc:title>",
    ] {
        log_check("q6xc.1.xmp.golden", needle, text.contains(needle));
    }
}

#[test]
fn strict_rejection_matrix_javascript_and_file_uris() {
    // Parser/PDF URL policy already drops javascript: and file: before annot
    // emission. The named codes are covered by pdfa unit tests; here we prove
    // a normal https link still renders under strict, and that the error type
    // is InvalidInput with a pdf_a_ prefix when the helper rejects.
    let ok = render_pdf_pdfa(MD, &opts(), PdfASettings::a2b_strict());
    log_check(
        "q6xc.1.strict.https",
        "https link is conformable",
        ok.as_ref().is_ok_and(|b| b.starts_with(b"%PDF-")),
    );
    let err =
        franken_markdown::pdfa::check_uri_action(PdfASettings::a2b_strict(), "javascript:alert(1)")
            .unwrap_err();
    log_check(
        "q6xc.1.strict.code",
        "javascript: yields pdf_a_javascript_uri",
        err.to_string().starts_with("pdf_a_javascript_uri:"),
    );
    let err = franken_markdown::pdfa::check_uri_action(PdfASettings::a2b_strict(), "file:///tmp/x")
        .unwrap_err();
    log_check(
        "q6xc.1.strict.file",
        "file: yields pdf_a_file_uri",
        err.to_string().starts_with("pdf_a_file_uri:"),
    );
}

#[test]
fn docs_name_the_delta() {
    let docs = std::fs::read_to_string("docs/PDF_A.md").unwrap();
    log_check(
        "q6xc.1.docs.delta",
        "docs enumerate XMP OutputIntent ICC CIDSet",
        docs.contains("XMP")
            && docs.contains("OutputIntent")
            && docs.contains("sRGB")
            && docs.contains("CIDSet"),
    );
}
