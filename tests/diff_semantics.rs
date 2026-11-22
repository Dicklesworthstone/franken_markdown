//! The semantic differ must preserve literal payloads and AST boundaries while
//! ignoring ordinary prose reflow. These tests use only the public document and
//! diff APIs; no canonical-key implementation is used as an oracle.

use franken_markdown::diff::{ChangeKind, DiffReport, diff_documents};
use franken_markdown::{
    Align, Block, DefinitionItem, Document, Inline, List, ListItem, Table, parse_markdown,
};

fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text(value.to_string())]
}

#[test]
fn equal_plain_text_changes_do_not_report_a_fake_heading_level_change() {
    for (old, new, summary) in [
        (
            "# Use `a  b`\n",
            "# Use `a b`\n",
            "heading content or formatting changed:",
        ),
        (
            "Use `a  b` here.\n",
            "Use `a b` here.\n",
            "paragraph content or formatting changed:",
        ),
    ] {
        let report = diff_documents(&parse_markdown(old), &parse_markdown(new));
        assert_eq!(report.changes.len(), 1, "{report:?}");
        assert!(report.changes[0].summary.starts_with(summary), "{report:?}");
        assert!(!report.changes[0].summary.contains("level changed"));
    }
}

fn paragraph(value: &str) -> Block {
    Block::Paragraph(text(value))
}

fn document(block: Block) -> Document {
    Document {
        blocks: vec![block],
    }
}

fn list(items: Vec<Vec<Block>>) -> Block {
    Block::List(List {
        ordered: false,
        start: 1,
        tight: true,
        items: items
            .into_iter()
            .map(|blocks| ListItem { task: None, blocks })
            .collect(),
    })
}

fn changed(label: &str, old: &Document, new: &Document, kind: ChangeKind) -> DiffReport {
    assert_ne!(old, new, "{label}: the fixture must contain an AST change");
    let report = diff_documents(old, new);
    assert!(
        !report.identical,
        "{label}: lost semantic change: {report:?}"
    );
    assert_eq!(report.changes.len(), 1, "{label}: {report:?}");
    assert_eq!(report.changes[0].kind, kind, "{label}: {report:?}");
    assert_eq!(report.changes[0].old_index, Some(0), "{label}");
    assert_eq!(report.changes[0].new_index, Some(0), "{label}");
    report
}

fn identical(label: &str, old: &Document, new: &Document) {
    let report = diff_documents(old, new);
    assert!(
        report.identical,
        "{label}: prose reflow is not an edit: {report:?}"
    );
    assert!(report.changes.is_empty(), "{label}: {report:?}");
}

#[test]
fn parsed_inline_literals_keep_meaningful_whitespace() {
    for (label, old, new) in [
        ("code", "Use `a  b` here.\n", "Use `a b` here.\n"),
        (
            "HTML attribute",
            "Read <span title=\"a  b\">this</span>.\n",
            "Read <span title=\"a b\">this</span>.\n",
        ),
        (
            "inline math text",
            "Read $\\text{a  b}$ here.\n",
            "Read $\\text{a b}$ here.\n",
        ),
        (
            "inline display math text",
            "Read $$\\text{a  b}$$ here.\n",
            "Read $$\\text{a b}$$ here.\n",
        ),
    ] {
        changed(
            label,
            &parse_markdown(old),
            &parse_markdown(new),
            ChangeKind::ParagraphEdited,
        );
    }
}

#[test]
fn code_block_trailing_spaces_tabs_and_blank_lines_are_not_erased() {
    for old in [
        "```text\nvalue  \n```\n",
        "```text\nvalue\t\n```\n",
        "```text\nvalue\n\n```\n",
    ] {
        changed(
            old,
            &parse_markdown(old),
            &parse_markdown("```text\nvalue\n```\n"),
            ChangeKind::CodeBlockChanged {
                lang: Some("text".to_string()),
            },
        );
    }
}

#[test]
fn literal_payload_boundaries_are_not_trimmed_or_normalized() {
    // The public AST accepts already parsed literal payloads. Unlike Markdown
    // source parsing, semantic comparison must not apply code-span trimming or
    // reinterpret the caller's math/HTML bytes.
    for (old, new) in [
        (
            Inline::Code("value  ".to_string()),
            Inline::Code("value ".to_string()),
        ),
        (
            Inline::Math(" \t\\text{value}\n".to_string()),
            Inline::Math("\\text{value}".to_string()),
        ),
        (
            Inline::DisplayMath("\n\\text{value}  ".to_string()),
            Inline::DisplayMath("\\text{value}".to_string()),
        ),
        (
            Inline::Html("<span title=\"value  \">".to_string()),
            Inline::Html("<span title=\"value \">".to_string()),
        ),
    ] {
        changed(
            "literal payload boundaries",
            &document(Block::Paragraph(vec![old])),
            &document(Block::Paragraph(vec![new])),
            ChangeKind::ParagraphEdited,
        );
    }
    changed(
        "display block payload boundaries",
        &document(Block::MathBlock("\n\\text{value} \t".to_string())),
        &document(Block::MathBlock("\\text{value}".to_string())),
        ChangeKind::CodeBlockChanged { lang: None },
    );
}

#[test]
fn literal_whitespace_survives_nested_inline_and_cell_containers() {
    fn containers(value: &str) -> Vec<(Block, ChangeKind)> {
        let literal = vec![Inline::Strong(vec![Inline::Code(value.to_string())])];
        vec![
            (
                Block::Paragraph(literal.clone()),
                ChangeKind::ParagraphEdited,
            ),
            (
                Block::Heading {
                    level: 2,
                    inlines: literal.clone(),
                },
                ChangeKind::HeadingChanged { level: 2 },
            ),
            (
                Block::Table(Table {
                    align: vec![Align::Left],
                    head: vec![text("Header")],
                    rows: vec![vec![literal.clone()]],
                }),
                ChangeKind::TableChanged,
            ),
            (
                Block::DefinitionList(vec![DefinitionItem {
                    terms: vec![text("Term")],
                    definitions: vec![literal.clone()],
                }]),
                ChangeKind::ListChanged,
            ),
            (
                Block::BlockQuote(vec![Block::Paragraph(literal.clone())]),
                ChangeKind::ParagraphEdited,
            ),
            (
                list(vec![vec![Block::Paragraph(literal)]]),
                ChangeKind::ListChanged,
            ),
        ]
    }
    for ((old, kind), (new, _)) in containers("two  spaces")
        .into_iter()
        .zip(containers("two spaces"))
    {
        changed("nested literal", &document(old), &document(new), kind);
    }
}

#[test]
fn link_and_image_destinations_and_titles_are_literal_fields() {
    for image in [false, true] {
        let node = |dest: &str, title: &str| {
            document(Block::Paragraph(vec![if image {
                Inline::Image {
                    dest: dest.to_string(),
                    title: Some(title.to_string()),
                    alt: "same description".to_string(),
                }
            } else {
                Inline::Link {
                    dest: dest.to_string(),
                    title: Some(title.to_string()),
                    content: text("same description"),
                }
            }]))
        };
        let old_dest = "asset?q=two  spaces";
        let new_dest = "asset?q=two spaces";
        changed(
            "destination whitespace",
            &node(old_dest, "title"),
            &node(new_dest, "title"),
            ChangeKind::LinkTargetChanged {
                old: old_dest.to_string(),
                new: new_dest.to_string(),
            },
        );
        changed(
            "title whitespace",
            &node("asset", "two  spaces"),
            &node("asset", "two spaces"),
            ChangeKind::ParagraphEdited,
        );
    }
}

#[test]
fn nested_blocks_keep_their_kind_and_container_depth() {
    changed(
        "quoted paragraph versus heading",
        &parse_markdown("> h1:Title\n"),
        &parse_markdown("> # Title\n"),
        ChangeKind::ParagraphEdited,
    );
    for (old, new) in [
        (
            Block::BlockQuote(vec![paragraph("hr")]),
            Block::BlockQuote(vec![Block::ThematicBreak]),
        ),
        (
            Block::BlockQuote(vec![paragraph("same text")]),
            Block::BlockQuote(vec![Block::BlockQuote(vec![paragraph("same text")])]),
        ),
        (
            Block::FootnoteDefinition {
                id: "note".to_string(),
                blocks: vec![paragraph("h1:Title")],
            },
            Block::FootnoteDefinition {
                id: "note".to_string(),
                blocks: vec![Block::Heading {
                    level: 1,
                    inlines: text("Title"),
                }],
            },
        ),
    ] {
        changed(
            "nested block structure",
            &document(old),
            &document(new),
            ChangeKind::ParagraphEdited,
        );
    }
    changed(
        "list child kind",
        &document(list(vec![vec![paragraph("hr")]])),
        &document(list(vec![vec![Block::ThematicBreak]])),
        ChangeKind::ListChanged,
    );
}

#[test]
fn middle_alignment_cannot_match_a_paragraph_as_a_heading() {
    // Different neighbours force the actual LCS middle path instead of the
    // same-kind fast prefix/suffix comparisons.
    let old = parse_markdown("Old opening.\n\nh1:Title\n\nOld ending.\n");
    let new = parse_markdown("New opening.\n\n# Title\n\nNew ending.\n");
    let report = diff_documents(&old, &new);
    assert!(!report.identical);
    assert!(
        report.changes.iter().any(|change| {
            change.kind == ChangeKind::HeadingAdded
                && change.old_index.is_none()
                && change.new_index == Some(1)
        }),
        "the new heading must not be hidden by an unrelated paragraph: {report:?}"
    );
    assert!(
        report
            .changes
            .iter()
            .any(|change| change.old_index == Some(1)),
        "the former paragraph must be accounted for: {report:?}"
    );
}

#[test]
fn literal_controls_cannot_impersonate_inline_structure() {
    for (literal, structured) in [
        ("\u{1}strong\u{2}bold\u{3}", Inline::Strong(text("bold"))),
        ("\u{1}code\u{2}code\u{3}", Inline::Code("code".to_string())),
        ("\u{1}br\u{2}", Inline::HardBreak),
        (
            "\u{1}fn\u{2}note\u{3}",
            Inline::FootnoteRef {
                id: "note".to_string(),
            },
        ),
    ] {
        changed(
            "literal control bytes",
            &document(paragraph(literal)),
            &document(Block::Paragraph(vec![structured])),
            ChangeKind::ParagraphEdited,
        );
    }
    changed(
        "literal child separator",
        &document(Block::BlockQuote(vec![paragraph("left\u{4}right")])),
        &document(Block::BlockQuote(vec![
            paragraph("left"),
            paragraph("right"),
        ])),
        ChangeKind::ParagraphEdited,
    );
    changed(
        "literal page-break prefix",
        &document(Block::BlockQuote(vec![paragraph("\u{0}pagebreak")])),
        &document(Block::BlockQuote(vec![Block::PageBreak])),
        ChangeKind::ParagraphEdited,
    );
}

#[test]
fn payload_field_boundaries_cannot_be_forged_by_delimiter_text() {
    changed(
        "code language versus source boundary",
        &document(Block::CodeBlock {
            lang: Some("rust:tag".to_string()),
            code: "value".to_string(),
        }),
        &document(Block::CodeBlock {
            lang: Some("rust".to_string()),
            code: "tag:value".to_string(),
        }),
        ChangeKind::CodeBlockChanged {
            lang: Some("rust".to_string()),
        },
    );
    let node = |dest: &str, title: Option<String>| {
        document(Block::Paragraph(vec![Inline::Link {
            dest: dest.to_string(),
            title,
            content: text("label"),
        }]))
    };
    changed(
        "link destination versus title boundary",
        &node("asset\u{1}caption", None),
        &node("asset", Some("caption".to_string())),
        ChangeKind::LinkTargetChanged {
            old: "asset\u{1}caption".to_string(),
            new: "asset".to_string(),
        },
    );
}

#[test]
fn table_rows_and_empty_cells_remain_distinct() {
    let table = |rows| {
        document(Block::Table(Table {
            align: vec![Align::None; 3],
            head: vec![text("A"), text("B"), text("C")],
            rows,
        }))
    };
    // Sparse host-created rows are supported AST input. Two source rows must
    // not become one row merely because an intervening cell is empty.
    changed(
        "table row boundaries",
        &table(vec![vec![text("left")], vec![text("right")]]),
        &table(vec![vec![text("left"), Vec::new(), text("right")]]),
        ChangeKind::TableChanged,
    );
    changed(
        "cell data containing a separator",
        &table(vec![vec![text("left\u{4}right"), Vec::new()]]),
        &table(vec![vec![text("left"), text("right\u{4}")]]),
        ChangeKind::TableChanged,
    );
}

#[test]
fn empty_list_and_definition_items_preserve_their_source_ownership() {
    changed(
        "list item versus empty child",
        &document(list(vec![vec![paragraph("first")], Vec::new()])),
        &document(list(vec![vec![
            paragraph("first"),
            Block::Paragraph(Vec::new()),
        ]])),
        ChangeKind::ListChanged,
    );
    changed(
        "definition item versus additional term",
        &document(Block::DefinitionList(vec![
            DefinitionItem {
                terms: vec![text("first")],
                definitions: Vec::new(),
            },
            DefinitionItem {
                terms: vec![text("second")],
                definitions: vec![text("meaning")],
            },
        ])),
        &document(Block::DefinitionList(vec![DefinitionItem {
            terms: vec![text("first"), text("second")],
            definitions: vec![text("meaning")],
        }])),
        ChangeKind::ListChanged,
    );
}

#[test]
fn prose_reflow_ignores_text_segmentation_and_unicode_whitespace() {
    let old = document(Block::Paragraph(vec![
        Inline::Text("  caf".to_string()),
        Inline::Text("é\u{2003}".to_string()),
        Inline::Text("世界".to_string()),
        Inline::SoftBreak,
        Inline::Text("\tnaïve\u{00a0}résumé  ".to_string()),
    ]));
    let new = document(paragraph("café 世界 naïve résumé"));
    identical("split Unicode text and soft break", &old, &new);
    identical(
        "parsed Unicode reflow",
        &parse_markdown("café 世界\nnaïve résumé\n"),
        &parse_markdown("café   世界 naïve résumé\n"),
    );
}

#[test]
fn prose_reflow_around_literal_and_styled_runs_remains_identical() {
    let old = document(Block::Paragraph(vec![
        Inline::Text("Read  ".to_string()),
        Inline::Code("two  spaces".to_string()),
        Inline::SoftBreak,
        Inline::Text("then ".to_string()),
        Inline::Strong(vec![
            Inline::Text("styled  ".to_string()),
            Inline::SoftBreak,
            Inline::Text("word".to_string()),
        ]),
        Inline::Text(" end.\t".to_string()),
    ]));
    let new = document(Block::Paragraph(vec![
        Inline::Text("Read\t".to_string()),
        Inline::Code("two  spaces".to_string()),
        Inline::Text(" then ".to_string()),
        Inline::Strong(text("styled word")),
        Inline::Text("\nend.".to_string()),
    ]));
    identical("prose around retained literals", &old, &new);
}

#[test]
fn hard_breaks_remain_semantic_when_soft_breaks_are_normalized() {
    changed(
        "hard versus soft line break",
        &document(Block::Paragraph(vec![
            Inline::Text("first".to_string()),
            Inline::HardBreak,
            Inline::Text("second".to_string()),
        ])),
        &document(Block::Paragraph(vec![
            Inline::Text("first".to_string()),
            Inline::SoftBreak,
            Inline::Text("second".to_string()),
        ])),
        ChangeKind::ParagraphEdited,
    );
}
