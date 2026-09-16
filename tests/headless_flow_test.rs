//! Comprehensive test suite for spanned API zero-copy compatibility,
//! source mapping, and headless continuous-flow layout (FCB-073.B).
//!
//! Verifies:
//! - Zero-copy block inspection on `SpannedDocument` (avoid per-frame cloning).
//! - Truthful enclosing-source versus rendered-text copy distinction (Plan §12.4).
//! - Rejection of invented contiguous literal Markdown for disjoint ranges (Plan §12.4).
//! - Heading anchor resolution mapping heading identity to source anchor without pixel caching (Plan §12.4).
//! - Headless continuous-flow layout consumer with line wrapping and source tracking (Plan §12.9 & §27.4).
//! - Deterministic semantic layout fixture serialization with no FCB dependency (Plan §12.9).
//! - Giant/hostile document budgets and negative controls (Plan §27.4).
//! - Bi-directional position synchronization (rendered <-> source).
//! - Integration with ProvenanceOracle verifying zero invented contiguous slices.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::{
    FlowBudgets, FlowConstraints, FlowError, HeadlessFlowConsumer, ProvenanceOracle,
    SourceMapError, SourceOrigin, SourceSpan, SpannedBlock,
    TextSelectionRange, parse_markdown_spanned,
};

#[test]
fn spanned_document_zero_copy_inspection() {
    let source = "# Heading 1\n\nFirst paragraph.\n\n```rust\nfn main() {}\n```\n";
    let doc = parse_markdown_spanned(source);

    // 1. Zero-copy accessors
    assert_eq!(doc.block_count(), 3);
    assert!(!doc.is_empty());
    assert_eq!(doc.source_span(), SourceSpan::new(0, source.len()));

    // 2. Borrowed blocks slice
    let blocks: &[SpannedBlock] = doc.blocks();
    assert_eq!(blocks.len(), 3);

    // 3. Block by index and span
    let b0 = doc.block_at(0).expect("block 0 exists");
    let s0 = doc.source_span_for_block(0).expect("span 0 exists");
    assert_eq!(b0.span, s0);
    assert_eq!(s0.start, 0);

    // 4. Block at offset
    let (idx, b_found) = doc.block_at_offset(15).expect("offset 15 in block 1");
    assert_eq!(idx, 1);
    assert!(b_found.span.contains(15));

    // 5. IntoIterator without cloning
    let mut count = 0;
    for block in &doc {
        assert!(!block.span.is_empty());
        count += 1;
    }
    assert_eq!(count, 3);

    // 6. Out of bounds index returns None
    assert!(doc.block_at(99).is_none());
    assert!(doc.source_span_for_block(99).is_none());
    assert!(doc.block_at_offset(999_999).is_none());
}

#[test]
fn truthful_copy_rendered_vs_enclosing_source() {
    let source = "# Welcome\n\nHere is **bold** and *italic* styling.\n";
    let doc = parse_markdown_spanned(source);
    let map = doc.source_map(source).expect("source map built successfully");

    // Check rendered text contains clean text without markdown delimiters
    let rendered = map.rendered_text();
    assert!(rendered.contains("Welcome"));
    assert!(rendered.contains("bold"));
    assert!(rendered.contains("italic"));

    // Find rendered range for "bold"
    let bold_pos = rendered.find("bold").expect("bold present");
    let bold_range = TextSelectionRange::new(bold_pos, bold_pos + 4);

    // 1. Copy rendered reading text
    let copied_text = map.copy_rendered_text(bold_range).expect("copy rendered");
    assert_eq!(copied_text, "bold");

    // 2. Copy enclosing Markdown source block
    let enclosing_src = map
        .copy_enclosing_source(bold_range, source)
        .expect("copy enclosing source");
    assert!(
        enclosing_src.contains("**bold**"),
        "enclosing source must contain original markdown syntax, got: {}",
        enclosing_src
    );

    // 3. Copy exact source ranges
    let exact_ranges = map
        .copy_exact_source_ranges(bold_range)
        .expect("exact ranges");
    assert!(!exact_ranges.is_empty());
    assert_eq!(exact_ranges[0].origin, SourceOrigin::Primary);

    // 4. Copy exact source slices
    let exact_slices = map
        .copy_exact_source_slices(bold_range, source)
        .expect("exact slices");
    assert_eq!(exact_slices.len(), 1);
    assert_eq!(exact_slices[0], "bold");
}

#[test]
fn negative_control_reject_invented_contiguous_disjoint_ranges() {
    let source = "Start *bold* middle _italic_ end.\n";
    let doc = parse_markdown_spanned(source);
    let map = doc.source_map(source).expect("map built");

    // Select entire rendered text across both emphasis blocks
    let full_range = TextSelectionRange::new(0, map.rendered_len());
    let exact_ranges = map
        .copy_exact_source_ranges(full_range)
        .expect("exact ranges");

    // If there are multiple disjoint source ranges, claiming a single contiguous slice
    // from min_start to max_end would invent unselected delimiter bytes.
    if exact_ranges.len() > 1 {
        let min_start = exact_ranges.iter().map(|q| q.span.start).min().unwrap();
        let max_end = exact_ranges.iter().map(|q| q.span.end).max().unwrap();
        let purported_span = SourceSpan::new(min_start, max_end);

        // Plan §12.4 Negative control: Rejecting invented contiguous slice!
        let reject_result = map.reject_invented_contiguous(full_range, purported_span);
        assert_eq!(
            reject_result,
            Err(SourceMapError::InventedContiguous {
                enclosing: purported_span,
                disjoint_count: exact_ranges.len(),
            })
        );
    }
}

#[test]
fn heading_anchor_resolution_identity_not_cached_pixels() {
    let source = "# Introduction\n\nIntro body.\n\n## Getting Started\n\nStart guide.\n\n## Getting Started\n\nAdvanced guide.\n";
    let doc = parse_markdown_spanned(source);
    let map = doc.source_map(source).expect("source map built");

    assert_eq!(map.headings().len(), 3);

    // 1. Primary heading resolution by slug
    let h1 = map
        .resolve_source_anchor("introduction")
        .expect("introduction anchor exists");
    assert_eq!(h1.slug, "introduction");
    assert_eq!(h1.title, "Introduction");
    assert_eq!(h1.level, 1);
    assert_eq!(h1.origin, SourceOrigin::Primary);
    assert_eq!(h1.block_index, 0);
    assert_eq!(h1.source_span, SourceSpan::new(0, 14));

    // 2. Sub-heading resolution
    let h2 = map
        .resolve_source_anchor("getting-started")
        .expect("getting-started anchor exists");
    assert_eq!(h2.slug, "getting-started");
    assert_eq!(h2.title, "Getting Started");
    assert_eq!(h2.level, 2);

    // 3. Collision disambiguation: duplicate heading gets deterministic suffix
    let h2_dup = map
        .resolve_source_anchor("getting-started-2")
        .expect("getting-started-2 anchor exists");
    assert_eq!(h2_dup.slug, "getting-started-2");
    assert_eq!(h2_dup.title, "Getting Started");
    assert_eq!(h2_dup.level, 2);
    assert_ne!(h1.source_span, h2_dup.source_span);

    // 4. Negative control: non-existent slug returns None
    assert!(map.resolve_source_anchor("non-existent-section").is_none());
}

#[test]
fn headless_flow_consumer_layout_and_wrapping() {
    let source = "# Quick Start\n\nFrankenMarkdown provides ultra-fast dependency-free Markdown processing for native apps.\n";
    let consumer = HeadlessFlowConsumer::with_constraints(FlowConstraints {
        viewport_width: 30, // 30 columns forces wrapping
        line_height: 16,
        char_width: 1,
        max_viewport_lines: None,
    });

    let output = consumer.consume_source(source).expect("flow succeeded");

    // Output checks
    assert!(output.lines.len() >= 2, "narrow viewport must wrap paragraph into multiple lines");
    assert!(output.total_height >= 32);
    assert_eq!(output.consumed_blocks, 2);
    assert_eq!(output.consumed_bytes, source.len());

    // Check individual line properties
    for (i, line) in output.lines.iter().enumerate() {
        assert_eq!(line.line_index, i);
        assert_eq!(line.baseline_y, (i as u32) * 16);
        assert!(!line.rendered_text.is_empty());
        assert!(!line.rendered_range.is_empty());
        assert!(!line.source_span.is_empty());
    }
}

#[test]
fn deterministic_semantic_layout_fixture_reproducibility() {
    let source = "# Guide\n\nStep 1: Install.\n\nStep 2: Run.\n";
    let consumer = HeadlessFlowConsumer::default();

    let output1 = consumer.consume_source(source).expect("flow 1");
    let fixture1 = output1.to_semantic_fixture();

    let output2 = consumer.consume_source(source).expect("flow 2");
    let fixture2 = output2.to_semantic_fixture();

    // Deterministic serialization: byte-for-byte identical across runs
    assert_eq!(fixture1, fixture2);
    assert!(fixture1.contains("=== FRANKEN_MARKDOWN SEMANTIC LAYOUT FIXTURE ==="));
    assert!(fixture1.contains("#guide [level=1]"));
    assert!(fixture1.contains("--- LINES ---"));
    assert!(fixture1.contains("--- PROVENANCE AUDIT ---"));
    assert!(fixture1.contains("=== END FIXTURE ==="));
}

#[test]
fn bidirectional_source_and_rendered_synchronization() {
    let source = "Hello world from Rust.\n";
    let doc = parse_markdown_spanned(source);
    let map = doc.source_map(source).expect("map built");

    // Source offset 6 is 'w' in "world"
    let rendered_offset = map.sync_source_to_rendered(6).expect("sync to rendered");
    let sync_back = map.sync_rendered_to_source(rendered_offset).expect("sync to source");
    assert_eq!(sync_back, 6);

    // Hit test element by rendered offset
    let elem = map.element_at_rendered_offset(rendered_offset).expect("element found");
    assert_eq!(elem.block_index, 0);
    assert!(!elem.is_generated);

    // Hit test element by source offset
    let elem_src = map.element_at_source_offset(6).expect("element by source");
    assert_eq!(elem_src.id, elem.id);
}

#[test]
fn negative_controls_budget_guards_hostile_documents() {
    let source = "# Title\n\nParagraph 1.\n\nParagraph 2.\n\nParagraph 3.\n";

    // 1. Exceed byte budget
    let tiny_bytes_consumer = HeadlessFlowConsumer::new(
        FlowConstraints::default(),
        FlowBudgets {
            max_bytes: 10,
            max_blocks: 100,
            max_lines: 100,
            max_items: 100,
        },
    );
    assert!(matches!(
        tiny_bytes_consumer.consume_source(source),
        Err(FlowError::BudgetExceeded { .. })
    ));

    // 2. Exceed block budget
    let tiny_blocks_consumer = HeadlessFlowConsumer::new(
        FlowConstraints::default(),
        FlowBudgets {
            max_bytes: 10_000,
            max_blocks: 1, // Document has 4 blocks
            max_lines: 100,
            max_items: 100,
        },
    );
    assert!(matches!(
        tiny_blocks_consumer.consume_source(source),
        Err(FlowError::BudgetExceeded { .. })
    ));

    // 3. Exceed lines budget
    let tiny_lines_consumer = HeadlessFlowConsumer::new(
        FlowConstraints::default(),
        FlowBudgets {
            max_bytes: 10_000,
            max_blocks: 100,
            max_lines: 1, // Document has multiple lines
            max_items: 100,
        },
    );
    assert!(matches!(
        tiny_lines_consumer.consume_source(source),
        Err(FlowError::BudgetExceeded { .. })
    ));

    // 4. Invalid constraints
    let zero_viewport = HeadlessFlowConsumer::with_constraints(FlowConstraints {
        viewport_width: 0,
        line_height: 16,
        char_width: 1,
        max_viewport_lines: None,
    });
    assert!(matches!(
        zero_viewport.consume_source(source),
        Err(FlowError::InvalidConstraint { .. })
    ));

    let zero_line_height = HeadlessFlowConsumer::with_constraints(FlowConstraints {
        viewport_width: 80,
        line_height: 0,
        char_width: 1,
        max_viewport_lines: None,
    });
    assert!(matches!(
        zero_line_height.consume_source(source),
        Err(FlowError::InvalidConstraint { .. })
    ));
}

#[test]
fn negative_controls_invalid_selection_ranges() {
    let source = "# Hello\n\nWorld.\n";
    let doc = parse_markdown_spanned(source);
    let map = doc.source_map(source).expect("map built");

    // Reversed range
    let reversed = TextSelectionRange::new(10, 5);
    assert!(matches!(
        map.copy_rendered_text(reversed),
        Err(SourceMapError::InvalidSelectionRange { .. })
    ));
    assert!(matches!(
        map.copy_enclosing_source(reversed, source),
        Err(SourceMapError::InvalidSelectionRange { .. })
    ));

    // Out of bounds range
    let out_of_bounds = TextSelectionRange::new(0, 999_999);
    assert!(matches!(
        map.copy_rendered_text(out_of_bounds),
        Err(SourceMapError::InvalidSelectionRange { .. })
    ));
    assert!(matches!(
        map.copy_enclosing_source(out_of_bounds, source),
        Err(SourceMapError::InvalidSelectionRange { .. })
    ));
}

#[test]
fn provenance_oracle_truthfulness_integration() {
    let source = "# Overview\n\nTesting nested *provenance* graph with [link](https://example.com) and `code`.\n";
    let consumer = HeadlessFlowConsumer::default();
    let output = consumer.consume_source(source).expect("flow consumer succeeded");

    // Run ProvenanceOracle over the generated provenance graph
    let report = ProvenanceOracle::verify_truthfulness(
        output.source_map.provenance_graph(),
        source,
        &|_| None,
    )
    .expect("provenance graph must be strictly truthful");

    assert!(report.total_nodes > 0);
    assert!(report.zero_invented_contiguous_slices);
    println!("Provenance report: {:?}", report);
}
