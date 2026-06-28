//! Source-span and diagnostic scaffold tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{fs, path::Path};

use franken_markdown::{
    DiagnosticSeverity, HtmlOptions, SourceSpan, parse_markdown, parse_markdown_spanned,
    parse_markdown_spanned_profiled, render_html_document,
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
fn utf8_bom_is_ignored_without_skewing_spanned_blocks() {
    let heading_src = "\u{feff}# Title\n\nBody";
    let heading_doc = parse_markdown_spanned(heading_src);

    assert_eq!(heading_doc.blocks.len(), 2);
    assert_eq!(
        heading_doc.blocks[0].span.slice(heading_src).unwrap(),
        "# Title"
    );
    assert_eq!(
        heading_doc.blocks[1].span.slice(heading_src).unwrap(),
        "Body"
    );

    let ref_src = "\u{feff}[good]: /ok\n\nUse [good].\n\nTail.";
    let ref_doc = parse_markdown_spanned(ref_src);

    assert_eq!(ref_doc.diagnostics, Vec::new());
    assert_eq!(ref_doc.blocks.len(), 2);
    assert_eq!(
        ref_doc.blocks[0].span.slice(ref_src).unwrap(),
        "Use [good]."
    );
    assert_eq!(ref_doc.blocks[1].span.slice(ref_src).unwrap(), "Tail.");
    assert_eq!(parse_markdown(ref_src), ref_doc.to_document());
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
fn multiline_reference_title_lines_are_consumed_before_span_collection() {
    let src = "[good]: /ok\n  \"Good title\"\n\nUse [good].\n\nTail.";
    let doc = parse_markdown_spanned(src);

    assert_eq!(doc.diagnostics, Vec::new());
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[0].span.slice(src).unwrap(), "Use [good].");
    assert_eq!(doc.blocks[1].span.slice(src).unwrap(), "Tail.");
}

#[test]
fn scanner_edge_fixture_preserves_spans_and_diagnostic_contract() {
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parser/scanner_edges.md");
    let src = fs::read_to_string(fixture_path).unwrap();
    let doc = parse_markdown_spanned(&src);

    assert_eq!(doc.to_document(), parse_markdown(&src));
    assert!(
        doc.blocks.len() >= 12,
        "scanner fixture should exercise many top-level block shapes"
    );

    let block_slices: Vec<&str> = doc
        .blocks
        .iter()
        .map(|block| block.span.slice(&src).unwrap())
        .collect();

    assert!(
        block_slices
            .iter()
            .any(|slice| slice.starts_with("# Scanner Edge Corpus"))
    );
    assert!(
        block_slices
            .iter()
            .any(|slice| slice.contains("| Key | Expr | Note |"))
    );
    assert!(
        block_slices
            .iter()
            .any(|slice| slice.contains("```text\ninside\n   ```"))
    );
    assert!(block_slices.contains(&"[bad]:"));

    assert_eq!(doc.diagnostics.len(), 1);
    assert_eq!(doc.diagnostics[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(doc.diagnostics[0].span.slice(&src).unwrap(), "[bad]:");
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
fn profiled_spanned_parser_matches_normal_ast_and_reports_span_stages() {
    let src = "# Profiled Spans\n\nBody.\n\n[broken]: <unterminated\n\n```rust\nfn main() {}\n";
    let plain = parse_markdown(src);
    let profiled = parse_markdown_spanned_profiled(src);

    assert_eq!(profiled.document.to_document(), plain);
    assert!(
        profiled
            .document
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning),
        "fixture should exercise diagnostic attribution"
    );

    let stages: Vec<&str> = profiled.stages.iter().map(|stage| stage.stage).collect();
    assert!(stages.contains(&"span_collection"));
    assert!(stages.contains(&"diagnostics_collection"));
    assert!(stages.contains(&"block_parse_total"));
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
