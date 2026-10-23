//! Source-span and diagnostic scaffold tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{fs, path::Path};

use franken_markdown::{
    Block, DiagnosticSeverity, HtmlOptions, SourceSpan, SpannedDocument, parse_markdown,
    parse_markdown_spanned, parse_markdown_spanned_profiled, render_html_document,
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

fn assert_spans(source: &str, expected: &[&str]) -> SpannedDocument {
    let document = parse_markdown_spanned(source);
    assert_eq!(document.source_len, source.len());
    assert_eq!(document.blocks.len(), expected.len(), "{source:?}");
    assert_eq!(
        document
            .blocks
            .iter()
            .map(|block| block.node.clone())
            .collect::<Vec<_>>(),
        parse_markdown(source).blocks,
        "tracking must not change the rendered document"
    );
    let mut previous_end = 0;
    for (block, expected) in document.blocks.iter().zip(expected) {
        assert!(
            block.span.start >= previous_end,
            "overlapping or reordered spans"
        );
        assert_eq!(
            block.span.slice(source),
            Some(*expected),
            "{:?}",
            block.node
        );
        previous_end = block.span.end;
    }
    assert_eq!(parse_markdown_spanned_profiled(source).document, document);
    document
}

#[test]
fn lazy_blockquote_continuations_do_not_shift_later_blocks() {
    let source =
        "> First paragraph\nlazy continuation\n> last line\n\n# After\n\n[missing](#absent)";
    let document = assert_spans(
        source,
        &[
            "> First paragraph\nlazy continuation\n> last line",
            "# After",
            "[missing](#absent)",
        ],
    );
    assert!(matches!(document.blocks[0].node, Block::BlockQuote(_)));
    assert!(matches!(
        document.blocks[1].node,
        Block::Heading { level: 1, .. }
    ));
}

#[test]
fn multiline_math_with_blank_lines_and_block_markers_owns_one_span() {
    let source = "$$\n# literal math\n\nx + y\n$$\n\n# After\n\n[missing](#absent)";
    let document = assert_spans(
        source,
        &[
            "$$\n# literal math\n\nx + y\n$$",
            "# After",
            "[missing](#absent)",
        ],
    );
    assert!(matches!(document.blocks[0].node, Block::MathBlock(_)));
}

#[test]
fn definition_list_items_separated_by_blank_lines_share_one_span() {
    let source = "First term\n: first definition\n\nSecond term\n: second definition\n\n# After";
    let document = assert_spans(
        source,
        &[
            "First term\n: first definition\n\nSecond term\n: second definition",
            "# After",
        ],
    );
    assert!(matches!(&document.blocks[0].node, Block::DefinitionList(items) if items.len() == 2));
}

#[test]
fn indented_paragraph_continuations_are_not_separate_code_spans() {
    let source = "Opening paragraph\n    continued with indentation\n\n# After";
    let document = assert_spans(
        source,
        &[
            "Opening paragraph\n    continued with indentation",
            "# After",
        ],
    );
    assert!(matches!(document.blocks[0].node, Block::Paragraph(_)));
}

#[test]
fn bom_frontmatter_and_crlf_keep_original_byte_coordinates() {
    let source = "\u{feff}---\r\ntitle: |\r\n  # After\r\n  ```metadata, not code\r\n---\r\n\r\n# After\r\n\r\n😀 [missing](#absent)\r\n";
    let document = assert_spans(source, &["# After", "😀 [missing](#absent)"]);
    assert!(
        document.diagnostics.is_empty(),
        "metadata must not produce Markdown diagnostics"
    );
    assert_eq!(
        document.blocks[0].span.start,
        source.rfind("# After").unwrap()
    );
}

#[test]
fn tab_expansion_and_reference_removal_keep_real_line_offsets() {
    let source = "[ref]: /target\n\t\"not a reference title\"\n\n# After\n\n[ref]\n";
    let document = assert_spans(source, &["\t\"not a reference title\"", "# After", "[ref]"]);
    assert!(matches!(document.blocks[0].node, Block::CodeBlock { .. }));
}

#[test]
fn removed_references_cannot_join_the_source_spans_of_adjacent_tables() {
    let source = "| A |\n| --- |\n| one |\n[ref]: /target\n| B |\n| --- |\n| two |\n\n# After";
    let document = assert_spans(
        source,
        &[
            "| A |\n| --- |\n| one |",
            "| B |\n| --- |\n| two |",
            "# After",
        ],
    );
    assert!(matches!(document.blocks[0].node, Block::Table(_)));
    assert!(matches!(document.blocks[1].node, Block::Table(_)));
}

#[test]
fn nested_containers_do_not_consume_top_level_span_entries() {
    let source = "> - nested list\n>   > nested quote\n>\n> tail\n\n# After\n\n[^n]: first paragraph\n\n    second paragraph\n\n[missing](#absent)";
    let document = assert_spans(
        source,
        &[
            "> - nested list\n>   > nested quote\n>\n> tail",
            "# After",
            "[^n]: first paragraph\n\n    second paragraph",
            "[missing](#absent)",
        ],
    );
    assert!(matches!(
        document.blocks[2].node,
        Block::FootnoteDefinition { .. }
    ));
}

#[test]
fn empty_and_metadata_only_documents_do_not_invent_blocks() {
    for source in ["", "\u{feff}", "\r\n\n\t\n", "---\ntitle: Empty\n---\n"] {
        assert_spans(source, &[]);
    }
}

#[test]
fn unclosed_final_blocks_keep_their_actual_source_extent() {
    let source = "# Before\n\n$$\nmath\n\n# still math";
    assert_spans(source, &["# Before", "$$\nmath\n\n# still math"]);
    let source = "# Before\n\n```rust\nlet x = 1;\r";
    let document = assert_spans(source, &["# Before", "```rust\nlet x = 1;\r"]);
    assert_eq!(document.diagnostics.len(), 1);
    assert_eq!(document.diagnostics[0].span, document.blocks[1].span);
}
