//! Long revision histories retain unchanged content without quadratic fallbacks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::diff::{ChangeKind, DiffBlock, DocumentDiff, compute_diff, diff_documents};
use franken_markdown::{Align, Block, Document, Inline, Table, parse_markdown};

fn paragraph(text: impl Into<String>) -> Block {
    Block::Paragraph(vec![Inline::Text(text.into())])
}

fn assert_revisions(diff: &DocumentDiff, old: &Document, new: &Document) {
    let mut before = Vec::new();
    let mut after = Vec::new();
    for block in &diff.blocks {
        match block {
            DiffBlock::Unchanged(block) => {
                before.push(block.clone());
                after.push(block.clone());
            }
            DiffBlock::Inserted(block) => after.push(block.clone()),
            DiffBlock::Deleted(block) => before.push(block.clone()),
            DiffBlock::Modified { old, new, .. } => {
                before.push((**old).clone());
                after.push((**new).clone());
            }
        }
    }
    assert_eq!(
        before, old.blocks,
        "the old revision must remain recoverable"
    );
    assert_eq!(
        after, new.blocks,
        "the new revision must remain recoverable"
    );
}

#[test]
fn two_edits_in_a_long_document_do_not_mark_thousands_of_paragraphs_modified() {
    const UNCHANGED: usize = 5_000;
    let middle: String = (0..UNCHANGED)
        .map(|index| format!("Shared paragraph {index}, including café.\n\n"))
        .collect();
    let old = parse_markdown(&format!("Old introduction.\n\n{middle}Old conclusion.\n"));
    let new = parse_markdown(&format!("New introduction.\n\n{middle}New conclusion.\n"));
    let diff = compute_diff(&old, &new, "old.md", "new.md");
    assert_eq!(diff.stats.unchanged_blocks, UNCHANGED);
    assert_eq!(diff.stats.modified_blocks, 2);
    assert_eq!(diff.stats.inserted_blocks, 0);
    assert_eq!(diff.stats.deleted_blocks, 0);
    assert_eq!(diff.blocks.len(), UNCHANGED + 2);
    assert!(
        diff.blocks[1..=UNCHANGED]
            .iter()
            .all(|block| matches!(block, DiffBlock::Unchanged(_)))
    );
    assert_revisions(&diff, &old, &new);

    let semantic = diff_documents(&old, &new);
    assert_eq!(semantic.changes.len(), 2);
    for (change, index) in semantic.changes.iter().zip([0, UNCHANGED + 1]) {
        assert_eq!(change.kind, ChangeKind::ParagraphEdited);
        assert_eq!(change.old_index, Some(index));
        assert_eq!(change.new_index, Some(index));
    }
}

#[test]
fn repeated_content_and_shifted_edits_keep_all_exact_matches() {
    let middle = vec![paragraph("Repeated evidence."); 6_000];
    let mut old = Document {
        blocks: vec![paragraph("Old beginning.")],
    };
    old.blocks.extend(middle.clone());
    old.blocks.push(paragraph("Old ending."));
    let mut new = Document {
        blocks: vec![
            paragraph("First insertion."),
            paragraph("Second insertion."),
        ],
    };
    new.blocks.extend(middle);
    new.blocks.push(paragraph("New ending."));
    let diff = compute_diff(&old, &new, "old", "new");
    assert_eq!(diff.stats.unchanged_blocks, 6_000);
    assert_eq!(diff.stats.modified_blocks, 2);
    assert_eq!(diff.stats.inserted_blocks, 1);
    assert_eq!(diff.stats.deleted_blocks, 0);
    assert_revisions(&diff, &old, &new);
    assert_eq!(compute_diff(&old, &new, "old", "new"), diff);
}

#[test]
fn large_code_math_and_inline_edits_count_only_changed_words() {
    let words: String = (0..6_000).map(|index| format!("word{index} ")).collect();
    for (old_source, new_source) in [
        (
            format!("```text\nold {words}before\n```\n"),
            format!("```text\nnew {words}after\n```\n"),
        ),
        (
            format!("$$\nold {words}before\n$$\n"),
            format!("$$\nnew {words}after\n$$\n"),
        ),
    ] {
        let old = parse_markdown(&old_source);
        let new = parse_markdown(&new_source);
        let diff = compute_diff(&old, &new, "old", "new");
        assert_eq!(diff.stats.modified_blocks, 1);
        assert_eq!(diff.stats.words_inserted, 2);
        assert_eq!(diff.stats.words_deleted, 2);
        assert_revisions(&diff, &old, &new);
    }

    let code_spans: String = (0..3_000).map(|index| format!("`word{index}` ")).collect();
    let old = parse_markdown(&format!("old {code_spans}before"));
    let new = parse_markdown(&format!("new {code_spans}after"));
    let diff = compute_diff(&old, &new, "old", "new");
    assert_eq!(diff.stats.modified_blocks, 1);
    assert_eq!(diff.stats.words_inserted, 2);
    assert_eq!(diff.stats.words_deleted, 2);
    let DiffBlock::Modified { inline_diff, .. } = &diff.blocks[0] else {
        panic!("expected the paragraph edit");
    };
    assert_eq!(
        inline_diff
            .iter()
            .filter(|inline| matches!(inline, franken_markdown::diff::DiffInline::Unchanged(_)))
            .count(),
        5_999,
    );
    assert_revisions(&diff, &old, &new);
}

#[test]
fn disjoint_large_revisions_keep_every_change_after_search_budget_exhaustion() {
    // The LCS matrix exceeds its memory budget and the edit-frontier search
    // cannot finish inside its work/trace limits. Both reporting surfaces must
    // still retain all replacements; candidate pairing must not rescan each
    // already-consumed insertion from the beginning.
    const BLOCKS: usize = 12_000;
    let old = Document {
        blocks: (0..BLOCKS)
            .map(|index| paragraph(format!("Before {index}.")))
            .collect(),
    };
    let new = Document {
        blocks: (0..BLOCKS)
            .map(|index| paragraph(format!("After {index}.")))
            .collect(),
    };
    let diff = compute_diff(&old, &new, "old", "new");
    assert_eq!(diff.stats.modified_blocks, BLOCKS);
    assert_eq!(diff.stats.unchanged_blocks, 0);
    assert_eq!(diff.stats.words_inserted, BLOCKS * 2);
    assert_eq!(diff.stats.words_deleted, BLOCKS * 2);
    assert_revisions(&diff, &old, &new);

    let report = diff_documents(&old, &new);
    assert_eq!(report.changes.len(), BLOCKS);
    for (index, change) in report.changes.iter().enumerate() {
        assert_eq!(change.kind, ChangeKind::ParagraphEdited);
        assert_eq!(change.old_index, Some(index));
        assert_eq!(change.new_index, Some(index));
    }
}

#[test]
fn indexed_pairing_retains_first_compatible_kind_order_and_unpaired_containers() {
    let heading = |text: &str| Block::Heading {
        level: 2,
        inlines: vec![Inline::Text(text.into())],
    };
    let code = |text: &str| Block::CodeBlock {
        lang: Some("text".into()),
        code: text.into(),
    };
    let table = |text: &str| {
        Block::Table(Table {
            align: vec![Align::None],
            head: vec![vec![Inline::Text(text.into())]],
            rows: Vec::new(),
        })
    };
    let old = Document {
        blocks: vec![
            paragraph("old paragraph"),
            heading("old heading"),
            code("old code"),
            Block::BlockQuote(vec![paragraph("old quote")]),
            Block::MathBlock("old math".into()),
            table("old table"),
        ],
    };
    let new = Document {
        blocks: vec![
            code("new code"),
            heading("new heading"),
            table("new table"),
            paragraph("new paragraph"),
            Block::BlockQuote(vec![paragraph("new quote")]),
            Block::MathBlock("new math".into()),
        ],
    };
    let diff = compute_diff(&old, &new, "old", "new");
    assert_eq!(diff.stats.modified_blocks, 5);
    assert_eq!(diff.stats.deleted_blocks, 1);
    assert_eq!(diff.stats.inserted_blocks, 1);
    for (position, new_index) in [(0, 3), (1, 1), (2, 0), (4, 5), (5, 2)] {
        let DiffBlock::Modified {
            old: before,
            new: after,
            ..
        } = &diff.blocks[position]
        else {
            panic!("expected modification at {position}");
        };
        assert_eq!(**before, old.blocks[position]);
        assert_eq!(**after, new.blocks[new_index]);
    }
    assert_eq!(diff.blocks[3], DiffBlock::Deleted(old.blocks[3].clone()));
    assert_eq!(diff.blocks[6], DiffBlock::Inserted(new.blocks[4].clone()));
}
