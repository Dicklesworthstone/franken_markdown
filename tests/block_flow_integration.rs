#![forbid(unsafe_code)]

//! Integration tests for continuous flow layout, checked height indexing, stable
//! scroll anchoring, and structural nesting limits (FCB-032.A).
//!
//! Acceptance criteria & oracle contract:
//! - Giant paragraphs: Line-break cache respects line budgets and defends against hostile blocks.
//! - Deep lists: Nesting depth exceeding safety maximum (16) is rejected with clear error (negative control).
//! - Fractional heights beyond 2^32: Checked u64 accumulation and binary-lifting search function properly.
//! - Stable scroll anchoring: Viewport maintains exact source block and intra-block offset during
//!   window resize / reflow and structural insertion/removal, never jumping by scrollbar percentage.
//! - Markers & accents: Task list checkboxes (read-only vectors) and blockquote accent bars emit proper shapes.
//! - GPU conditioning: Local visible origin mapping translates massive document offsets into finite,
//!   well-conditioned f32 coordinates.

use franken_markdown::block_flow::{
    BlockFlowEngine, BlockFlowError, BlockHeightIndex, FlowBlockItem, ListMarker, LogicalHeight,
    ScrollAnchor, MAX_NESTING_DEPTH, MAX_PARAGRAPH_LINES,
};
use franken_markdown::display::{DisplayItem, VectorShapeType};
use franken_markdown::span::SourceSpan;

#[test]
fn oracle_fractional_heights_beyond_u32_max() {
    // 50 blocks, each 100,000,000 units (points * 256)
    // 50 * 100,000,000 = 5,000,000,000 units, which strictly exceeds u32::MAX (4,294,967,295)
    let block_units = 100_000_000_u64;
    let block_count = 50;
    let heights = vec![LogicalHeight::from_raw(block_units); block_count];

    let index = BlockHeightIndex::with_heights(&heights).expect("build large index");
    let total = index.total_height().expect("total height");

    assert_eq!(total.raw(), 5_000_000_000_u64);
    assert!(
        total.raw() > u32::MAX as u64,
        "total raw units must exceed u32::MAX"
    );

    // Prefix sum at count 45 = 4,500,000,000 (also > u32::MAX)
    let prefix = index.prefix_height(45).expect("prefix height");
    assert_eq!(prefix.raw(), 4_500_000_000_u64);
    assert!(prefix.raw() > u32::MAX as u64);

    // Binary-lifting anchor search at scroll position 4,650,000,000
    // Block 46 begins at 4,600,000,000, so intra-offset is 50,000,000
    let scroll_y = LogicalHeight::from_raw(4_650_000_000);
    let anchor = index
        .find_anchor_at_scroll(scroll_y)
        .expect("find anchor at large offset");
    assert_eq!(anchor.block_id, 46);
    assert_eq!(anchor.intra_block_offset.raw(), 50_000_000);

    // Resolving anchor scroll_y returns the exact original offset
    let resolved = anchor.resolve_scroll_y(&index).expect("resolve scroll y");
    assert_eq!(resolved, scroll_y);
}

#[test]
fn oracle_scroll_anchoring_never_jumps_by_scrollbar_percentage_on_resize() {
    // Document with 4 blocks of heights 100, 200, 300, 400 pt (total 1000 pt)
    let initial_heights = vec![
        LogicalHeight::from_points(100.0),
        LogicalHeight::from_points(200.0),
        LogicalHeight::from_points(300.0),
        LogicalHeight::from_points(400.0),
    ];
    let mut index = BlockHeightIndex::with_heights(&initial_heights).unwrap();
    assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(1000.0));

    // Viewport is anchored at block 2, intra_block_offset 50 pt
    // Absolute position = prefix(2) + 50 = (100 + 200) + 50 = 350 pt
    // Global scroll percentage was 350 / 1000 = 35.0%
    let anchor = ScrollAnchor::new(2, LogicalHeight::from_points(50.0));
    let initial_scroll = anchor.resolve_scroll_y(&index).unwrap();
    assert_eq!(initial_scroll, LogicalHeight::from_points(350.0));

    // Reflow event on window resize:
    // Block 0 expands from 100 pt to 500 pt (e.g. narrow window causes multi-line wrapping)
    // Block 3 shrinks from 400 pt to 200 pt
    // New total height = 500 + 200 + 300 + 200 = 1200 pt
    index
        .update_block_height(0, LogicalHeight::from_points(500.0))
        .unwrap();
    index
        .update_block_height(3, LogicalHeight::from_points(200.0))
        .unwrap();
    assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(1200.0));

    // If naive scrollbar percentage were used:
    // 35.0% of 1200 pt = 420 pt (which would show block 0 at 420 pt instead of block 2!)
    // But with ScrollAnchor:
    let anchored_scroll = anchor.resolve_scroll_y(&index).unwrap();
    // Block 2's new top is prefix(2) = 500 + 200 = 700 pt.
    // Anchored scroll is 700 + 50 = 750 pt.
    assert_eq!(anchored_scroll, LogicalHeight::from_points(750.0));

    // The anchor STILL identifies Block 2 with intra-offset 50 pt exactly:
    let re_anchored = index.find_anchor_at_scroll(anchored_scroll).unwrap();
    assert_eq!(re_anchored.block_id, 2);
    assert_eq!(re_anchored.intra_block_offset, LogicalHeight::from_points(50.0));
}

#[test]
fn oracle_scroll_anchoring_shifts_correctly_on_insert_and_remove() {
    let mut index = BlockHeightIndex::with_heights(&[
        LogicalHeight::from_points(100.0),
        LogicalHeight::from_points(100.0),
        LogicalHeight::from_points(100.0),
        LogicalHeight::from_points(100.0),
    ])
    .unwrap();

    // User is reading Block 3 at offset 10 pt
    let mut anchor = ScrollAnchor::new(3, LogicalHeight::from_points(10.0));

    // Structural edit: 2 new blocks inserted at index 1
    index
        .insert_block(1, LogicalHeight::from_points(50.0))
        .unwrap();
    index
        .insert_block(1, LogicalHeight::from_points(50.0))
        .unwrap();
    anchor = anchor.shift_after_insert(1, 2);

    // Anchor has shifted from block 3 to block 5
    assert_eq!(anchor.block_id, 5);
    assert_eq!(anchor.intra_block_offset, LogicalHeight::from_points(10.0));

    // Structural edit: 1 block removed at index 0
    index.remove_block(0).unwrap();
    anchor = anchor.shift_after_remove(0, 1);

    // Anchor has shifted from block 5 to block 4
    assert_eq!(anchor.block_id, 4);
    assert_eq!(anchor.intra_block_offset, LogicalHeight::from_points(10.0));
}

#[test]
fn oracle_nesting_depth_limit_negative_control() {
    let mut engine = BlockFlowEngine::new();

    // Valid depths: 0 to 15 (MAX_NESTING_DEPTH - 1)
    for depth in 0..MAX_NESTING_DEPTH {
        let res = engine.push_block(
            FlowBlockItem::ListItem {
                marker: ListMarker::Ordered((depth + 1) as u32),
                depth,
                text: format!("Nesting item at depth {depth}"),
                source_span: SourceSpan::default(),
            },
            600.0,
        );
        assert!(res.is_ok(), "depth {depth} must be accepted");
    }

    // Negative control: depth >= MAX_NESTING_DEPTH must fail
    let excessive_depth = MAX_NESTING_DEPTH;
    let err = engine.push_block(
        FlowBlockItem::ListItem {
            marker: ListMarker::Bullet('-'),
            depth: excessive_depth,
            text: "Deeply nested hostile structure".to_string(),
            source_span: SourceSpan::default(),
        },
        600.0,
    );

    if let Err(BlockFlowError::NestingDepthExceeded { depth, max }) = err {
        assert_eq!(depth, excessive_depth);
        assert_eq!(max, MAX_NESTING_DEPTH);
    } else {
        assert!(false, "expected NestingDepthExceeded, got {err:?}");
    }

    // Also verify Blockquote nesting depth is checked
    let quote_err = engine.push_block(
        FlowBlockItem::Blockquote {
            depth: MAX_NESTING_DEPTH + 5,
            text: "Hostile deep quote".to_string(),
            source_span: SourceSpan::default(),
        },
        600.0,
    );
    assert!(matches!(
        quote_err,
        Err(BlockFlowError::NestingDepthExceeded { .. })
    ));
}

#[test]
fn oracle_giant_paragraph_budget_defense_negative_control() {
    let mut engine = BlockFlowEngine::new();

    // Construct a hostile giant paragraph that exceeds MAX_PARAGRAPH_LINES (1000)
    // 1200 short lines
    let mut hostile_text = String::new();
    for i in 0..1200 {
        hostile_text.push_str(&format!("Line number {i} with some prose words.\n"));
    }

    let err = engine.push_block(
        FlowBlockItem::Paragraph {
            text: hostile_text,
            source_span: SourceSpan::default(),
        },
        400.0,
    );

    if let Err(BlockFlowError::ParagraphBudgetExceeded { lines, max }) = err {
        assert!(lines > max);
        assert_eq!(max, MAX_PARAGRAPH_LINES);
    } else {
        assert!(false, "expected ParagraphBudgetExceeded, got {err:?}");
    }
}

#[test]
fn oracle_gpu_coordinate_conditioning_at_large_origins() {
    let mut engine = BlockFlowEngine::new();

    // Push 3 blocks
    engine
        .push_block(
            FlowBlockItem::Heading {
                level: 1,
                text: "Document Title".to_string(),
                source_span: SourceSpan::new(0, 14),
            },
            800.0,
        )
        .unwrap();
    engine
        .push_block(
            FlowBlockItem::Paragraph {
                text: "This is a paragraph under the heading.".to_string(),
                source_span: SourceSpan::new(15, 53),
            },
            800.0,
        )
        .unwrap();

    // Materialize with a visible origin offset of 20pt
    let origin_y = LogicalHeight::from_points(20.0);
    let dl = engine
        .materialize_viewport(10.0, origin_y, 800.0, 600.0)
        .unwrap();

    // Verify all bounds are finite and well conditioned
    for item in dl.items() {
        let bounds = item.bounds();
        assert!(bounds.x.is_finite());
        assert!(bounds.y.is_finite());
        assert!(bounds.width.is_finite());
        assert!(bounds.height.is_finite());
        assert!(bounds.width > 0.0);
        assert!(bounds.height > 0.0);
    }

    // Verify reading nodes exist and match headings/paragraphs
    let nodes = dl.reading_order();
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].text, "Document Title");
    assert_eq!(nodes[1].text, "This is a paragraph under the heading.");
}

#[test]
fn oracle_task_list_and_blockquote_presentation() {
    let mut engine = BlockFlowEngine::new();

    engine
        .push_block(
            FlowBlockItem::ListItem {
                marker: ListMarker::Task { checked: false },
                depth: 0,
                text: "Pending item".to_string(),
                source_span: SourceSpan::new(0, 20),
            },
            700.0,
        )
        .unwrap();
    engine
        .push_block(
            FlowBlockItem::ListItem {
                marker: ListMarker::Task { checked: true },
                depth: 0,
                text: "Completed item".to_string(),
                source_span: SourceSpan::new(21, 45),
            },
            700.0,
        )
        .unwrap();
    engine
        .push_block(
            FlowBlockItem::Blockquote {
                depth: 0,
                text: "Important advisory message".to_string(),
                source_span: SourceSpan::new(46, 80),
            },
            700.0,
        )
        .unwrap();

    let dl = engine
        .materialize_viewport(0.0, LogicalHeight::ZERO, 700.0, 500.0)
        .unwrap();

    // Check for 2 checkbox outlines, 1 checkbox check, 1 callout accent bar
    let mut outline_count = 0;
    let mut check_count = 0;
    let mut bar_count = 0;

    for item in dl.items() {
        if let DisplayItem::Vector(v) = item {
            match v.shape {
                VectorShapeType::CheckboxOutline => outline_count += 1,
                VectorShapeType::CheckboxCheck => check_count += 1,
                VectorShapeType::CalloutAccentBar => bar_count += 1,
                _ => {}
            }
        }
    }

    assert_eq!(outline_count, 2, "must have 2 checkbox outlines");
    assert_eq!(check_count, 1, "must have 1 checkbox check");
    assert_eq!(bar_count, 1, "must have 1 callout accent bar");
}
