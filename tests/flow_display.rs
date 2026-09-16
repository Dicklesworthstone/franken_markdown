//! Focused tests for the synchronous-resumable flow display engine
//! (FCB-074.A).
//!
//! The core oracles:
//! 1. Budget exhaustion followed by resume produces identical output to whole-input processing.
//! 2. Stale asset generations are rejected with [`FlowDisplayError::StaleAssetGeneration`].
//! 3. Display lists and reading orders are materialized without GPU, AppKit, or ambient I/O.

#![forbid(unsafe_code)]

use franken_markdown::display::{
    AccessibleReadingRole, DisplayItem, VectorShapeType,
};
use franken_markdown::flow_display::{
    AssetRequestId, AssetResult, DisplayBlock, FlowDisplayError, ResumableFlowDisplay,
};

const DOC: &str = "\
# Title

A paragraph of text.

## Section

- item one
- item two

```rust
fn main() {}
```

> a quote

| a | b |
|---|---|
| 1 | 2 |

---

3. ordered item

![Architecture Overview](images/architecture.png)

Final paragraph.
";

#[test]
fn step_resume_equals_whole_at_every_batch_size() {
    for batch in [1usize, 2, 3, 5, 8, 13, 21] {
        // Resumable: step through the document with finite batch budget.
        let mut engine = ResumableFlowDisplay::new(DOC, batch);
        let mut resumable_blocks = Vec::new();
        let mut resumable_assets = Vec::new();

        loop {
            match engine.step().expect("step succeeds") {
                Some(result) => {
                    resumable_blocks.extend(result.blocks);
                    resumable_assets.extend(result.unresolved_assets);
                }
                None => break,
            }
        }

        // Whole: process the entire document in one pass.
        let mut whole_engine = ResumableFlowDisplay::new(DOC, usize::MAX);
        let whole_blocks = whole_engine.process_all().expect("whole processing");
        let whole_assets = whole_engine.unresolved_assets().to_vec();

        assert_eq!(
            resumable_blocks, whole_blocks,
            "batch size {batch}: resume blocks must equal whole"
        );
        assert_eq!(
            resumable_assets, whole_assets,
            "batch size {batch}: resume asset requests must equal whole"
        );

        // Verify that materializing DisplayLists also produces identical counts and bounds.
        let dl_resumable = engine.to_display_list();
        let dl_whole = whole_engine.to_display_list();

        assert_eq!(
            dl_resumable.items().len(),
            dl_whole.items().len(),
            "batch size {batch}: display item count must match"
        );
        assert_eq!(
            dl_resumable.reading_order().len(),
            dl_whole.reading_order().len(),
            "batch size {batch}: reading order count must match"
        );
        assert_eq!(
            dl_resumable.total_bounds(),
            dl_whole.total_bounds(),
            "batch size {batch}: total bounds must match"
        );
    }
}

#[test]
fn stale_asset_generation_rejected() {
    let source = "![Architecture](diagram.png)";
    let mut engine = ResumableFlowDisplay::with_generation(source, 10, 1);
    let step = engine.step().expect("step").expect("has result");
    assert_eq!(step.unresolved_assets.len(), 1);
    let req = &step.unresolved_assets[0];
    assert_eq!(req.generation, 1);
    assert_eq!(req.url, "diagram.png");

    // Advance engine generation to simulate document edit/reflow.
    engine.set_generation(2);

    // Supplying an asset result with the old generation (1) must fail.
    let stale_res = AssetResult {
        request_id: req.id,
        generation: 1,
        width: 800,
        height: 600,
        bytes: Some(vec![1, 2, 3]),
    };
    let err = engine.provide_asset(stale_res).unwrap_err();
    assert_eq!(
        err,
        FlowDisplayError::StaleAssetGeneration {
            expected: 2,
            actual: 1
        }
    );

    // Supplying an asset result matching the current generation (2) must succeed.
    let current_res = AssetResult {
        request_id: req.id,
        generation: 2,
        width: 800,
        height: 600,
        bytes: Some(vec![1, 2, 3]),
    };
    engine
        .provide_asset(current_res)
        .expect("current generation matches");
    assert!(engine.is_asset_resolved(req.id));
    assert_eq!(engine.unresolved_assets().len(), 0);
    assert_eq!(engine.resolved_assets().len(), 1);
}

#[test]
fn unknown_asset_request_rejected() {
    let mut engine = ResumableFlowDisplay::with_generation("Hello", 5, 1);
    let res = AssetResult {
        request_id: AssetRequestId(9999),
        generation: 1,
        width: 100,
        height: 100,
        bytes: None,
    };
    let err = engine.provide_asset(res).unwrap_err();
    assert_eq!(err, FlowDisplayError::UnknownAssetRequest(AssetRequestId(9999)));
}

#[test]
fn display_list_primitives_without_gpu() {
    let mut engine = ResumableFlowDisplay::new(DOC, usize::MAX);
    engine.process_all().expect("process all");

    let dl = engine.to_display_list();
    assert!(!dl.items().is_empty());
    assert!(!dl.reading_order().is_empty());

    // Verify presence of text runs, vector paths, anchors, and images.
    let has_text = dl.items().iter().any(|i| matches!(i, DisplayItem::Text(_)));
    let has_vector = dl.items().iter().any(|i| matches!(i, DisplayItem::Vector(_)));
    let has_anchor = dl.items().iter().any(|i| matches!(i, DisplayItem::Anchor(_)));
    let has_image = dl.items().iter().any(|i| matches!(i, DisplayItem::Image(_)));

    assert!(has_text, "display list contains text runs");
    assert!(has_vector, "display list contains vector paths (rules, quotes, tables)");
    assert!(has_anchor, "display list contains semantic anchors");
    assert!(has_image, "display list contains image placeholder");

    // Hit testing works without GPU or window context.
    let hit = dl.hit_test(10.0, 10.0);
    assert!(hit.is_some(), "hit test returns top-level item");

    // Check vector shape types.
    let has_accent_bar = dl.items().iter().any(|i| match i {
        DisplayItem::Vector(v) => v.shape == VectorShapeType::CalloutAccentBar,
        _ => false,
    });
    let has_rule = dl.items().iter().any(|i| match i {
        DisplayItem::Vector(v) => v.shape == VectorShapeType::HorizontalRule,
        _ => false,
    });
    assert!(has_accent_bar, "callout accent bar vector shape present");
    assert!(has_rule, "horizontal rule vector shape present");

    // Check reading tree structure.
    let reading = dl.reading_order();
    assert!(
        reading
            .iter()
            .any(|n| matches!(n.role, AccessibleReadingRole::Heading { level: 1 }))
    );
    assert!(
        reading
            .iter()
            .any(|n| matches!(n.role, AccessibleReadingRole::Paragraph))
    );
    assert!(
        reading
            .iter()
            .any(|n| matches!(n.role, AccessibleReadingRole::ThematicBreak))
    );
}

#[test]
fn step_reports_has_more_correctly() {
    let mut engine = ResumableFlowDisplay::new(DOC, 1);
    let mut saw_has_more_true = false;
    let mut saw_has_more_false = false;
    loop {
        match engine.step().expect("step succeeds") {
            Some(result) => {
                if result.has_more {
                    saw_has_more_true = true;
                } else {
                    saw_has_more_false = true;
                }
            }
            None => break,
        }
    }
    assert!(saw_has_more_true, "document has multiple blocks");
    assert!(saw_has_more_false, "the final step reports no more");
}

#[test]
fn all_blocks_are_produced_after_finish() {
    let mut engine = ResumableFlowDisplay::new(DOC, 3);
    while engine.step().expect("step").is_some() {}
    assert!(engine.is_finished());

    let blocks = engine.blocks();
    assert!(!blocks.is_empty(), "a rich document produces blocks");

    // Verify we see all expected block kinds including UnresolvedAsset.
    let has_heading = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::Heading { .. }));
    let has_paragraph = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::Paragraph { .. }));
    let has_list_item = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::ListItem { .. }));
    let has_code_block = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::CodeBlock { .. }));
    let has_quote = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::Quote { .. }));
    let has_rule = blocks.iter().any(|b| matches!(b, DisplayBlock::Rule));
    let has_table = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::TableHeader { .. }));
    let has_asset = blocks
        .iter()
        .any(|b| matches!(b, DisplayBlock::UnresolvedAsset { .. }));

    assert!(has_heading, "heading detected");
    assert!(has_paragraph, "paragraph detected");
    assert!(has_list_item, "list item detected");
    assert!(has_code_block, "code block detected");
    assert!(has_quote, "quote detected");
    assert!(has_rule, "horizontal rule detected");
    assert!(has_table, "table detected");
    assert!(has_asset, "unresolved asset detected");
}

#[test]
fn empty_document_produces_no_blocks() {
    let mut engine = ResumableFlowDisplay::new("", 10);
    let result = engine.step().expect("empty document steps");
    assert!(result.is_none(), "empty document produces no blocks");
    assert!(engine.blocks().is_empty());
}

#[test]
fn batch_size_one_processes_one_line_per_step() {
    let source = "para one\n\npara two";
    let mut engine = ResumableFlowDisplay::new(source, 1);
    let first = engine.step().expect("first step");
    assert!(first.is_some());
    let blocks_after_first = first.unwrap().blocks.len();
    assert!(
        blocks_after_first <= 1,
        "batch size 1 processes at most 1 line per step"
    );
}

#[test]
fn finish_then_step_returns_none() {
    let mut engine = ResumableFlowDisplay::new("hello", 5);
    while engine.step().expect("step").is_some() {}
    assert!(engine.is_finished());
    let result = engine.step();
    assert!(result.expect("no error").is_none(), "finished engine yields None");
}

#[test]
fn heading_slug_and_anchors() {
    let heading = DisplayBlock::Heading {
        level: 2,
        text: "My Cool Heading!".to_string(),
    };
    assert_eq!(heading.slug(), Some("my-cool-heading".to_string()));
}
