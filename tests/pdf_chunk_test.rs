//! u9jt.2: chunked PDF page emission must match the monolithic writer byte-for-byte.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfEmitOptions, PdfOptions, parse_markdown, render_pdf_document_emitted, render_pdf_emitted,
};

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
        title: Some("chunked-parity".into()),
        author: Some("FMD".into()),
        ..PdfOptions::default()
    }
}

const FILLER: &str = "deterministic typography optimization representation hyphenation pagination markdown rendering ligature kerning paragraph document ";

fn generate_pages(pages: usize, lines: usize, width: usize) -> String {
    let line = padded_line(width);
    let mut para = String::with_capacity(lines.saturating_mul(width.saturating_add(1)));
    for i in 0..lines {
        para.push_str(&line);
        if i + 1 < lines {
            para.push(' ');
        }
    }
    let mut out = String::with_capacity(pages.saturating_mul(para.len().saturating_add(24)));
    for i in 0..pages {
        out.push_str("## S");
        out.push_str(&i.to_string());
        out.push_str("\n\n");
        out.push_str(&para);
        out.push_str("\n\n");
    }
    out
}

fn padded_line(width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut line = String::with_capacity(width);
    while line.len() < width {
        line.push_str(FILLER);
    }
    line.truncate(width);
    line
}

fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    let n = a.len().min(b.len());
    (0..n)
        .find(|&i| a[i] != b[i])
        .or_else(|| if a.len() == b.len() { None } else { Some(n) })
}

fn assert_bytes_eq(id: &str, subject: &str, chunked: &[u8], monolithic: &[u8]) {
    if chunked == monolithic {
        log_check(id, subject, true);
        return;
    }
    let at = first_diff(chunked, monolithic).unwrap_or(0);
    let lo = at.saturating_sub(8);
    let hi_a = (at + 8).min(chunked.len());
    let hi_b = (at + 8).min(monolithic.len());
    eprintln!(
        "check id={id} subject={subject} outcome=FAIL at={at} chunked_len={} monolithic_len={} chunked[{lo}..{hi_a}]={:?} monolithic[{lo}..{hi_b}]={:?}",
        chunked.len(),
        monolithic.len(),
        &chunked[lo..hi_a],
        &monolithic[lo..hi_b]
    );
    panic!("{id}: {subject} byte mismatch at {at}");
}

fn render_both(src: &str) -> (Vec<u8>, Vec<u8>) {
    let doc = parse_markdown(src);
    let o = opts();
    let chunked = render_pdf_document_emitted(&doc, &o, PdfEmitOptions::default()).unwrap();
    let monolithic = render_pdf_document_emitted(&doc, &o, PdfEmitOptions::monolithic()).unwrap();
    (chunked, monolithic)
}

#[test]
fn empty_document_chunked_matches_monolithic() {
    let (chunked, monolithic) = render_both("");
    log_check(
        "u9jt.2.empty.magic",
        "empty PDF starts with %PDF-",
        chunked.starts_with(b"%PDF-"),
    );
    assert_bytes_eq(
        "u9jt.2.empty.parity",
        "empty document chunked == monolithic",
        &chunked,
        &monolithic,
    );
}

#[test]
fn short_doc_chunked_matches_monolithic() {
    let md =
        "# Hi\n\nA short paragraph with a [link](https://example.com).\n\n- item one\n- item two\n";
    let (chunked, monolithic) = render_both(md);
    log_check(
        "u9jt.2.short.magic",
        "short PDF starts with %PDF-",
        chunked.starts_with(b"%PDF-"),
    );
    assert_bytes_eq(
        "u9jt.2.short.parity",
        "short document chunked == monolithic",
        &chunked,
        &monolithic,
    );
}

#[test]
fn overlap_fixture_chunked_matches_monolithic() {
    // ~dozens of pages so page-break / keep / table-header choices actually fire.
    let md = generate_pages(80, 24, 72);
    let (chunked, monolithic) = render_both(&md);
    log_check(
        "u9jt.2.overlap.pages",
        "overlap fixture produced a multi-page PDF",
        chunked.len() > 8_000,
    );
    assert_bytes_eq(
        "u9jt.2.overlap.parity",
        "80-section overlap fixture chunked == monolithic",
        &chunked,
        &monolithic,
    );
}

#[test]
fn thousand_page_overlap_chunked_matches_monolithic() {
    let md = generate_pages(1000, 48, 72);
    let (chunked, monolithic) = render_both(&md);
    log_check(
        "u9jt.2.1k.magic",
        "1k-page fixture starts with %PDF-",
        chunked.starts_with(b"%PDF-"),
    );
    log_check(
        "u9jt.2.1k.size",
        "1k-page fixture is a large PDF",
        chunked.len() > 50_000,
    );
    assert_bytes_eq(
        "u9jt.2.1k.parity",
        "1k-page overlap fixture chunked == monolithic",
        &chunked,
        &monolithic,
    );
}

#[test]
fn heap_ceiling_rejects_when_retained_exceeds_max() {
    let md = generate_pages(8, 12, 40);
    let err = render_pdf_emitted(&md, &opts(), PdfEmitOptions::chunked_with_ceiling(1))
        .expect_err("1-byte ceiling must reject a real PDF");
    let msg = err.to_string();
    log_check(
        "u9jt.2.ceiling.named",
        "ceiling error carries pdf_heap_ceiling:",
        msg.contains("pdf_heap_ceiling:"),
    );
    log_check(
        "u9jt.2.ceiling.actionable",
        "ceiling error names the retained vs max bytes",
        msg.contains("exceeds 1"),
    );
}
