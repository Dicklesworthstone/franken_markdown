//! Source-span and diagnostic scaffold tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    DiagnosticSeverity, HtmlOptions, SourceSpan, parse_markdown, parse_markdown_spanned,
    render_html_document,
};

#[test]
fn top_level_block_spans_slice_original_source() {
    let src = "# Title\n\nParagraph with **strong** text.\n\n- one\n- two\n";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.source_len, src.len());
    assert_eq!(doc.diagnostics, Vec::new());
    assert_eq!(doc.blocks.len(), 3);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "# Title");
    assert_eq!(
        doc.blocks[1].span.slice(src).unwrap(),
        "Paragraph with **strong** text."
    );
    assert_eq!(doc.blocks[2].span.slice(src).unwrap(), "- one\n- two");
}

#[test]
fn setext_and_fenced_blocks_get_multiline_spans() {
    let src = "Title\n=====\n\n```rust\nfn main() {}\n```\n";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "Title\n=====");
    assert_eq!(
        doc.blocks[1].span.slice(src).unwrap(),
        "```rust\nfn main() {}\n```"
    );
}

#[test]
fn valid_reference_definitions_are_not_rendered_blocks_but_bad_ones_warn() {
    let src = "[good]: /ok\n\nUse [good].\n\n[bad]:\n";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "Use [good].");
    assert_eq!(doc.blocks[1].span.slice(src).unwrap(), "[bad]:");
    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(doc.diagnostics[0].span.slice(src).unwrap(), "[bad]:");
    assert!(
        doc.diagnostics[0]
            .message
            .contains("malformed link reference")
    );
}

#[test]
fn unclosed_fence_reports_warning_span_to_end_of_source() {
    let src = "before\n\n```text\nunterminated\n";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(
        doc.diagnostics[0].span.slice(src).unwrap(),
        "```text\nunterminated\n"
    );
    assert!(doc.diagnostics[0].message.contains("unclosed fenced"));
}

#[test]
fn spanned_api_does_not_change_renderer_facing_document_contract() {
    let src = "# Same\n\nBody with [a link](https://example.com).";
    let plain = parse_markdown(src);
    let spanned = parse_markdown_spanned(src).into_document();

    assert_eq!(plain, spanned);
    assert_eq!(
        render_html_document(&plain, &HtmlOptions::default()).unwrap(),
        render_html_document(&spanned, &HtmlOptions::default()).unwrap()
    );
}

#[test]
fn source_span_helpers_are_total_for_invalid_ranges() {
    let source = "abcdef";
    let a = SourceSpan::new(1, 3);
    let b = SourceSpan::new(4, 6);
    let malformed = SourceSpan::new(5, 2);

    assert_eq!(a.len(), 2);
    assert!(a.contains(2));
    assert!(!a.contains(3));
    assert_eq!(a.merge(b), SourceSpan::new(1, 6));
    assert_eq!(a.slice(source).unwrap(), "bc");
    assert_eq!(malformed.len(), 0);
    assert!(malformed.is_empty());
    assert_eq!(malformed.slice(source), None);
}
