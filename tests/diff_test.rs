//! Contract tests for the semantic AST diff report in `src/diff.rs`:
//! `diff_documents` / `DiffReport` / `ChangeKind` / `report_json` /
//! `report_text` (per-change JSON schema `fmd-diff-changes-v1`).
//!
//! The module is registered as `franken_markdown::diff`, so these tests
//! exercise the public crate surface directly.

use franken_markdown::diff::{ChangeKind, DiffReport, diff_documents, report_json, report_text};
use franken_markdown::parse_markdown;

fn diff_md(old: &str, new: &str) -> DiffReport {
    diff_documents(&parse_markdown(old), &parse_markdown(new))
}

#[test]
fn identical_documents_report_empty_diff() {
    let md = "# Title\n\nSome *styled* paragraph text.\n\n```rust\nlet x = 1;\n```\n";
    let report = diff_md(md, md);
    assert!(report.identical);
    assert!(report.changes.is_empty());
    assert_eq!(
        report_json(&report),
        "{\"schema\":\"fmd-diff-changes-v1\",\"identical\":true,\"changes\":[]}"
    );
    assert_eq!(report_text(&report), "fmd diff: documents identical\n");
}

#[test]
fn adversarial_empty_documents_both_sides() {
    // Both empty.
    let report = diff_md("", "");
    assert!(report.identical);
    assert!(report.changes.is_empty());

    // Whitespace-only parses to zero blocks: identical to empty.
    let report = diff_md("  \n\n\t\n", "");
    assert!(report.identical);
    assert!(report.changes.is_empty());

    // Empty -> content: pure addition.
    let report = diff_md("", "# Added\n\nBody text.\n");
    assert!(!report.identical);
    assert_eq!(report.changes.len(), 2);
    assert_eq!(report.changes[0].kind, ChangeKind::HeadingAdded);
    assert_eq!(report.changes[0].old_index, None);
    assert_eq!(report.changes[0].new_index, Some(0));
    assert_eq!(report.changes[1].kind, ChangeKind::ParagraphEdited);
    assert_eq!(report.changes[1].old_index, None);
    assert_eq!(report.changes[1].new_index, Some(1));

    // Content -> empty: pure removal.
    let report = diff_md("# Gone\n\nBody text.\n", "");
    assert!(!report.identical);
    assert_eq!(report.changes.len(), 2);
    assert_eq!(report.changes[0].kind, ChangeKind::HeadingRemoved);
    assert_eq!(report.changes[0].old_index, Some(0));
    assert_eq!(report.changes[0].new_index, None);
    assert_eq!(report.changes[1].kind, ChangeKind::ParagraphEdited);
    assert_eq!(report.changes[1].old_index, Some(1));
    assert_eq!(report.changes[1].new_index, None);
}

#[test]
fn heading_rename_is_changed_not_add_remove() {
    let report = diff_md("# Old Title\n\nBody.\n", "# New Title\n\nBody.\n");
    assert_eq!(report.changes.len(), 1);
    let change = &report.changes[0];
    assert_eq!(change.kind, ChangeKind::HeadingChanged { level: 1 });
    assert_eq!(change.old_index, Some(0));
    assert_eq!(change.new_index, Some(0));
    assert_eq!(
        change.summary,
        "heading changed: \"Old Title\" -> \"New Title\""
    );
}

#[test]
fn heading_level_change_is_detected() {
    let report = diff_md("# Title\n", "## Title\n");
    assert_eq!(report.changes.len(), 1);
    let change = &report.changes[0];
    assert_eq!(change.kind, ChangeKind::HeadingChanged { level: 2 });
    assert_eq!(change.summary, "heading level changed 1 -> 2: \"Title\"");
}

#[test]
fn heading_added_and_removed_mid_document() {
    let report = diff_md("# A\n\nPara.\n\n# B\n", "# A\n\nPara.\n\n# New\n\n# B\n");
    assert_eq!(report.changes.len(), 1);
    let change = &report.changes[0];
    assert_eq!(change.kind, ChangeKind::HeadingAdded);
    assert_eq!(change.old_index, None);
    assert_eq!(change.new_index, Some(2));
    assert_eq!(change.summary, "heading added: \"New\"");
}

#[test]
fn paragraph_reflow_is_not_a_diff() {
    let old = "Hello world this is a test paragraph with several words.\n";
    let new = "Hello world this is\na test   paragraph\nwith several words.\n";
    let report = diff_md(old, new);
    assert!(report.identical, "reflow must not diff: {report:?}");
    assert!(report.changes.is_empty());
}

#[test]
fn table_cell_edit_is_localized() {
    let old = "# T\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nTail.\n";
    let new = "# T\n\n| A | B |\n|---|---|\n| 1 | 3 |\n\nTail.\n";
    let report = diff_md(old, new);
    assert_eq!(report.changes.len(), 1);
    let change = &report.changes[0];
    assert_eq!(change.kind, ChangeKind::TableChanged);
    assert_eq!(change.old_index, Some(1));
    assert_eq!(change.new_index, Some(1));
}

#[test]
fn link_target_change_is_detected() {
    let report = diff_md(
        "See [the docs](old.md) now.\n",
        "See [the docs](new.md) now.\n",
    );
    assert_eq!(report.changes.len(), 1);
    let change = &report.changes[0];
    assert_eq!(
        change.kind,
        ChangeKind::LinkTargetChanged {
            old: "old.md".to_string(),
            new: "new.md".to_string(),
        }
    );
    assert_eq!(change.old_index, Some(0));
    assert_eq!(change.new_index, Some(0));
}

#[test]
fn block_move_is_detected() {
    let old = "A one.\n\nB two.\n\nC three.\n\nD four.\n";
    let new = "A one.\n\nC three.\n\nD four.\n\nB two.\n";
    let report = diff_md(old, new);
    assert_eq!(report.changes.len(), 1, "expected one move: {report:?}");
    let change = &report.changes[0];
    assert_eq!(change.kind, ChangeKind::BlockMoved);
    assert_eq!(change.old_index, Some(1));
    assert_eq!(change.new_index, Some(3));
    assert_eq!(
        change.summary,
        "paragraph moved from block 1 to block 3: \"B two.\""
    );
}

#[test]
fn list_and_code_changes_are_typed() {
    let report = diff_md("- a\n- b\n", "- a\n- c\n");
    assert_eq!(report.changes.len(), 1);
    assert_eq!(report.changes[0].kind, ChangeKind::ListChanged);

    let report = diff_md("```rust\nlet a = 1;\n```\n", "```rust\nlet a = 2;\n```\n");
    assert_eq!(report.changes.len(), 1);
    assert_eq!(
        report.changes[0].kind,
        ChangeKind::CodeBlockChanged {
            lang: Some("rust".to_string()),
        }
    );
    assert_eq!(report.changes[0].summary, "code block changed (rust)");
}

#[test]
fn extended_block_kinds_are_covered() {
    // Math block.
    let report = diff_md("$$\nx + 1\n$$\n", "$$\nx + 2\n$$\n");
    assert_eq!(report.changes.len(), 1, "math diff: {report:?}");
    assert_eq!(
        report.changes[0].kind,
        ChangeKind::CodeBlockChanged { lang: None }
    );
    assert_eq!(report.changes[0].summary, "math block changed");

    // Block quote.
    let report = diff_md("> quoted text\n", "> changed text\n");
    assert_eq!(report.changes.len(), 1, "quote diff: {report:?}");
    assert!(
        report.changes[0]
            .summary
            .starts_with("block quote changed:")
    );

    // Footnote definition.
    let report = diff_md(
        "Text.[^a]\n\n[^a]: note one\n",
        "Text.[^a]\n\n[^a]: note two\n",
    );
    assert_eq!(report.changes.len(), 1, "footnote diff: {report:?}");
    assert!(
        report.changes[0]
            .summary
            .starts_with("footnote definition changed:")
    );

    // Definition list.
    let report = diff_md("Term\n: def one\n", "Term\n: def two\n");
    assert_eq!(report.changes.len(), 1, "deflist diff: {report:?}");
    assert_eq!(report.changes[0].kind, ChangeKind::ListChanged);
    assert!(
        report.changes[0]
            .summary
            .starts_with("definition list changed:")
    );

    // Thematic break removal.
    let report = diff_md("A.\n\n---\n\nB.\n", "A.\n\nB.\n");
    assert_eq!(report.changes.len(), 1, "break diff: {report:?}");
    assert_eq!(report.changes[0].summary, "thematic break removed");
}

#[test]
fn json_schema_golden_is_stable() {
    let report = diff_md(
        "# Title\n\nHello world.\n\n[link](a.md)\n",
        "# Title\n\nHello brave world.\n\n[link](b.md)\n",
    );
    let expected = "{\"schema\":\"fmd-diff-changes-v1\",\"identical\":false,\"changes\":[\
        {\"kind\":\"paragraph_edited\",\"summary\":\"paragraph edited: \\\"Hello world.\\\" -> \\\"Hello brave world.\\\"\",\"old_index\":1,\"new_index\":1},\
        {\"kind\":\"link_target_changed\",\"old\":\"a.md\",\"new\":\"b.md\",\"summary\":\"link target changed: a.md -> b.md\",\"old_index\":2,\"new_index\":2}\
        ]}";
    assert_eq!(report_json(&report), expected);
}

#[test]
fn text_report_golden_is_stable() {
    let report = diff_md(
        "# Title\n\nHello world.\n\n[link](a.md)\n",
        "# Title\n\nHello brave world.\n\n[link](b.md)\n",
    );
    let expected = "fmd diff: 2 changes\n\
        ~ [1->1] paragraph edited: \"Hello world.\" -> \"Hello brave world.\"\n\
        ~ [2->2] link target changed: a.md -> b.md\n";
    assert_eq!(report_text(&report), expected);
}

#[test]
fn reports_are_byte_identical_across_runs() {
    let old = "# A\n\nPara one with a [link](x.md).\n\n- i1\n- i2\n\n| H |\n|---|\n| c |\n";
    let new =
        "# B\n\nPara   one with a [link](y.md).\n\n- i1\n- i9\n\n| H |\n|---|\n| d |\n\nExtra.\n";
    let first = diff_md(old, new);
    let second = diff_md(old, new);
    assert_eq!(first, second);
    assert_eq!(report_json(&first), report_json(&second));
    assert_eq!(report_text(&first), report_text(&second));
}

#[test]
fn compute_diff_counts_math_and_footnote_words() {
    use franken_markdown::diff::compute_diff;

    let old_doc = parse_markdown("# Old\n\n$$\nx^2 + y^2 = z^2\n$$\n\n[^1]: note alpha beta\n");
    let new_doc = parse_markdown(
        "# New\n\n$$\na^2 + b^2 = c^2 + d^2\n$$\n\n[^1]: note gamma delta epsilon\n",
    );

    let diff = compute_diff(&old_doc, &new_doc, "a.md", "b.md");
    assert!(diff.stats.words_inserted > 0);
    assert!(diff.stats.words_deleted > 0);
}
