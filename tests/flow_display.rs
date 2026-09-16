//! Focused tests for the synchronous-resumable flow display engine
//! (FCB-074.A). The core oracle: budget exhaustion followed by resume
//! produces identical output to whole-input processing.

#![forbid(unsafe_code)]

use franken_markdown::flow_display::{
    DisplayBlock, FlowDisplayError, ResumableFlowDisplay,
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

Final paragraph.
";

#[test]
fn step_resume_equals_whole_at_every_batch_size() {
    for batch in [1usize, 2, 3, 5, 8, 13, 21] {
        // Resumable: step through the document.
        let mut engine = ResumableFlowDisplay::new(DOC, batch);
        let mut resumable_blocks = Vec::new();
        loop {
            match engine.step().expect("step succeeds") {
                Some(result) => resumable_blocks.extend(result.blocks),
                None => break,
            }
        }

        // Whole: process the entire document in one call.
        let mut whole_engine = ResumableFlowDisplay::new(DOC, usize::MAX);
        let whole_blocks = whole_engine.process_all().expect("whole processing");

        assert_eq!(
            resumable_blocks, whole_blocks,
            "batch size {batch}: resume must equal whole"
        );
    }
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

    // Verify we see the expected block kinds.
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

    assert!(has_heading, "heading detected");
    assert!(has_paragraph, "paragraph detected");
    assert!(has_list_item, "list item detected");
    assert!(has_code_block, "code block detected");
    assert!(has_quote, "quote detected");
    assert!(has_rule, "horizontal rule detected");
    assert!(has_table, "table detected");
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
