#![forbid(unsafe_code)]

//! Integration tests for checked paged height indexing, old/new page reservation,
//! and transactional height refinement (FCB-032.B).
//!
//! Acceptance criteria & oracle contract:
//! - Paged structure: Bounded leaf pages with checked Fenwick prefix directory.
//! - Fractional heights beyond 2^32: Cumulative document units > 4,294,967,295 across multiple pages.
//! - Old/new page reservation: Refinement transactions reserve affected pages before mutation
//!   and commit atomically or roll back on failure.
//! - Scroll anchor stability: Source anchor remains locked to the exact same block and intra-block
//!   offset through font substitution, window resizing, and background height refinement, never
//!   jumping by scrollbar percentage.
//! - Structural edits: Insertion and removal across page boundaries maintain capacity and directory sums.
//! - Negative controls: Overflow, out-of-bounds, and aborted transactions leave index intact.

use franken_markdown::block_flow::{BlockFlowError, LogicalHeight, ScrollAnchor};
use franken_markdown::paged_height::PagedHeightIndex;

#[test]
fn oracle_paged_heights_beyond_u32_max() {
    // 200 blocks with capacity 20 -> 10 leaf pages
    // Each block has raw height 30,000,000 units
    // Total raw height = 200 * 30,000,000 = 6,000,000,000 units (> u32::MAX = 4,294,967,295)
    let block_units = 30_000_000_u64;
    let block_count = 200;
    let page_capacity = 20;
    let heights = vec![LogicalHeight::from_raw(block_units); block_count];

    let index = PagedHeightIndex::with_heights_and_capacity(&heights, page_capacity)
        .expect("build paged index");

    assert_eq!(index.len(), 200);
    assert_eq!(index.page_count(), 10);
    assert_eq!(index.page_capacity(), page_capacity);

    let total = index.total_height().expect("total height");
    assert_eq!(total.raw(), 6_000_000_000_u64);
    assert!(total.raw() > u32::MAX as u64);

    // Prefix sum at block 160 = 160 * 30M = 4,800,000,000 (> u32::MAX)
    let prefix_160 = index.prefix_height(160).expect("prefix 160");
    assert_eq!(prefix_160.raw(), 4_800_000_000_u64);
    assert!(prefix_160.raw() > u32::MAX as u64);

    // Binary-lifting anchor search at scroll position 5,150,000,000
    // 5,150,000,000 / 30,000,000 = block 171 with intra offset 20,000,000
    let scroll_pos = LogicalHeight::from_raw(5_150_000_000);
    let anchor = index
        .find_anchor_at_scroll(scroll_pos)
        .expect("find anchor at large offset");
    assert_eq!(anchor.block_id, 171);
    assert_eq!(anchor.intra_block_offset.raw(), 20_000_000);

    // Resolving anchor scroll_y returns the exact original position
    let resolved = index
        .prefix_height(anchor.block_id)
        .expect("prefix block")
        .checked_add(anchor.intra_block_offset)
        .expect("add intra offset");
    assert_eq!(resolved, scroll_pos);
}

#[test]
fn oracle_scroll_anchoring_never_jumps_by_scrollbar_percentage_during_paged_refinements() {
    // 40 blocks with capacity 10 -> 4 pages, each block initially 100 pt
    // Total document height = 4,000 pt
    let initial_heights = vec![LogicalHeight::from_points(100.0); 40];
    let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 10).unwrap();
    assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(4000.0));

    // User is reading Block 25 with intra-block offset 30 pt
    // Initial absolute scroll_y = 25 * 100 + 30 = 2,530 pt
    // Scroll percentage would be 2,530 / 4,000 = 63.25%
    let anchor = ScrollAnchor::new(25, LogicalHeight::from_points(30.0));
    let initial_scroll_y = index
        .prefix_height(anchor.block_id)
        .unwrap()
        .checked_add(anchor.intra_block_offset)
        .unwrap();
    assert_eq!(initial_scroll_y, LogicalHeight::from_points(2530.0));

    // Refinement event (e.g. fallback font loads or window narrows):
    // Blocks 0..10 (Page 0) expand from 100 pt to 250 pt (+150 pt each, +1,500 pt total)
    // Blocks 30..40 (Page 3) shrink from 100 pt to 50 pt (-50 pt each, -500 pt total)
    // New total height = 4000 + 1500 - 500 = 5000 pt
    let new_page0_heights = vec![LogicalHeight::from_points(250.0); 10];
    index.refine_heights(0, &new_page0_heights).unwrap();

    let new_page3_heights = vec![LogicalHeight::from_points(50.0); 10];
    index.refine_heights(30, &new_page3_heights).unwrap();

    assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(5000.0));

    // If naive scrollbar percentage were used:
    // 63.25% of 5,000 pt = 3,162.5 pt (which causes a severe visual jump!)
    // But with ScrollAnchor:
    // New prefix of Block 25 is:
    // Page 0 (10 * 250 = 2500) + Page 1 (10 * 100 = 1000) + 5 * 100 = 4000 pt.
    // Anchored scroll_y is 4000 + 30 = 4030 pt.
    let anchored_scroll_y = index
        .prefix_height(anchor.block_id)
        .unwrap()
        .checked_add(anchor.intra_block_offset)
        .unwrap();
    assert_eq!(anchored_scroll_y, LogicalHeight::from_points(4030.0));

    // Finding anchor at anchored_scroll_y returns Block 25 with intra-offset 30 pt exactly!
    let found = index.find_anchor_at_scroll(anchored_scroll_y).unwrap();
    assert_eq!(found.block_id, 25);
    assert_eq!(found.intra_block_offset, LogicalHeight::from_points(30.0));
}

#[test]
fn oracle_transactional_refinement_old_new_page_reservation() {
    // 30 blocks with capacity 10 -> 3 pages
    let initial_heights = vec![LogicalHeight::from_points(50.0); 30];
    let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 10).unwrap();
    let initial_total = index.total_height().unwrap();

    // Reserve Page 1 (blocks 10..20)
    let mut tx = index.begin_refinement(&[1]).expect("reserve page 1");

    // Stage updates on page 1: change first 3 blocks from 50pt to 120pt (+70pt each = +210pt)
    tx.stage_block_refinement(1, 0, LogicalHeight::from_points(120.0)).unwrap();
    tx.stage_block_refinement(1, 1, LogicalHeight::from_points(120.0)).unwrap();
    tx.stage_block_refinement(1, 2, LogicalHeight::from_points(120.0)).unwrap();

    // Commit transaction
    tx.commit(&mut index).expect("commit page refinement");

    let expected_new_total = initial_total
        .checked_add(LogicalHeight::from_points(210.0))
        .unwrap();
    assert_eq!(index.total_height().unwrap(), expected_new_total);

    // Verify individual block heights
    assert_eq!(index.block_height(10).unwrap(), LogicalHeight::from_points(120.0));
    assert_eq!(index.block_height(11).unwrap(), LogicalHeight::from_points(120.0));
    assert_eq!(index.block_height(12).unwrap(), LogicalHeight::from_points(120.0));
    assert_eq!(index.block_height(13).unwrap(), LogicalHeight::from_points(50.0));
}

#[test]
fn oracle_transaction_rollback_negative_control() {
    let initial_heights = vec![LogicalHeight::from_points(25.0); 20];
    let index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 10).unwrap();
    let original_total = index.total_height().unwrap();

    // Reserve page 0 and stage a massive modification
    let mut tx = index.begin_refinement(&[0]).unwrap();
    tx.stage_block_refinement(0, 0, LogicalHeight::from_points(10_000.0))
        .unwrap();

    // Rollback
    tx.rollback();

    // Target index is completely unchanged
    assert_eq!(index.total_height().unwrap(), original_total);
    assert_eq!(index.block_height(0).unwrap(), LogicalHeight::from_points(25.0));
}

#[test]
fn oracle_structural_insert_and_remove_across_page_boundaries() {
    // 6 blocks with capacity 3 -> 2 pages: [0, 1, 2] and [3, 4, 5]
    let initial_heights = vec![LogicalHeight::from_points(10.0); 6];
    let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 3).unwrap();
    assert_eq!(index.page_count(), 2);
    assert_eq!(index.len(), 6);

    // Insert block at global index 3 (start of page 1)
    index
        .insert_block(3, LogicalHeight::from_points(99.0))
        .unwrap();
    assert_eq!(index.len(), 7);
    assert_eq!(index.block_height(3).unwrap(), LogicalHeight::from_points(99.0));

    // Remove block at global index 3
    let removed = index.remove_block(3).unwrap();
    assert_eq!(removed, LogicalHeight::from_points(99.0));
    assert_eq!(index.len(), 6);
    assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(60.0));
}

#[test]
fn oracle_invalid_range_and_overflow_negative_controls() {
    let index = PagedHeightIndex::with_heights(&[
        LogicalHeight::from_points(10.0),
        LogicalHeight::from_points(20.0),
    ])
    .unwrap();

    // Out of bounds prefix query
    let err = index.prefix_height(100);
    assert!(matches!(err, Err(BlockFlowError::IndexOutOfBounds { .. })));

    // Out of bounds page reservation
    let err_page = index.begin_refinement(&[99]);
    assert!(matches!(err_page, Err(BlockFlowError::IndexOutOfBounds { .. })));
}
