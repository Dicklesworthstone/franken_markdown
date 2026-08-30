//! Regression tests: the profiled PDF entry point must produce byte-identical
//! output to the production entry point.
//!
//! `render_pdf_document_profiled` historically skipped
//! `transform_footnotes_for_pdf`, so any footnote-bearing document rendered
//! DIFFERENT bytes through the profiled path than through
//! `render_pdf_document`. The perf harness (`examples/fmd_perf_harness.rs`)
//! asserts this equality as a proof obligation (`profiled_bytes_equal_normal_pdf`)
//! and used to abort the whole `corpus` scenario for footnote-bearing inputs.
//! These tests pin the fix at the library boundary.

use franken_markdown::{parse_markdown, render_pdf_document, render_pdf_document_profiled};

fn assert_profiled_bytes_equal_normal(src: &str) {
    let doc = parse_markdown(src);
    let normal = render_pdf_document(&doc, &Default::default())
        .unwrap_or_else(|e| panic!("normal render failed: {e}"));
    let profiled = render_pdf_document_profiled(&doc, &Default::default())
        .unwrap_or_else(|e| panic!("profiled render failed: {e}"));
    assert_eq!(
        normal.len(),
        profiled.bytes.len(),
        "profiled PDF length differs from normal (footnote transform mismatch?)"
    );
    assert_eq!(
        normal, profiled.bytes,
        "profiled PDF bytes differ from normal render"
    );
    // Repeated profiled renders must also be stable against each other.
    let again = render_pdf_document_profiled(&doc, &Default::default())
        .unwrap_or_else(|e| panic!("second profiled render failed: {e}"));
    assert_eq!(
        profiled.bytes, again.bytes,
        "profiled renders are not stable"
    );
}

#[test]
fn profiled_pdf_matches_normal_for_footnote_documents() {
    // Minimal footnote document: reference + definition.
    assert_profiled_bytes_equal_normal(
        "Text with a footnote ref[^1].\n\n[^1]: The footnote text.\n",
    );
    // Multiple references to one definition, referenced in order.
    assert_profiled_bytes_equal_normal(
        "First[^a] and second[^b].\n\n[^a]: First note.\n\n[^b]: Second note with `code`.\n",
    );
    // Definition nested in a list + blockquote shapes.
    assert_profiled_bytes_equal_normal(
        "- Item with note[^x].\n\n> Quote with note[^y].\n\n[^x]: List note.\n\n[^y]: Quote note.\n",
    );
    // Unreferenced definition (still rendered by the current transform rules).
    assert_profiled_bytes_equal_normal("Plain paragraph.\n\n[^orphan]: Never referenced.\n");
    // Notes citing notes.
    assert_profiled_bytes_equal_normal("Body[^1].\n\n[^1]: Note one[^2].\n\n[^2]: Note two.\n");
}

#[test]
fn profiled_pdf_matches_normal_for_footnote_free_documents() {
    // Control: documents without footnotes were always equal; keep them pinned.
    assert_profiled_bytes_equal_normal(
        "# Heading\n\nPlain prose with **bold** and a [link](https://example.com).\n",
    );
    assert_profiled_bytes_equal_normal(
        "```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
    );
}
