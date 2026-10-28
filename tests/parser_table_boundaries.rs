//! Table delimiters and following blocks retain their actual source ownership.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, Table, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned,
};

const HEADER: &str = "Name | Value\n--- | ---\n";

fn table(document: &Document) -> &Table {
    let Block::Table(table) = &document.blocks[0] else {
        panic!("expected table: {document:?}");
    };
    table
}

fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text(value.into())]
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn gfm_example_200_decodes_escaped_pipes_in_all_inline_spans() {
    // https://github.github.com/gfm/#example-200 — source remains unchanged.
    let source = "| f\\|oo  |\n| ------ |\n| b `\\|` az |\n| b **\\|** im |\n";
    let document = parse_markdown(source);
    let table = table(&document);
    assert_eq!(table.head, [text("f|oo")]);
    assert_eq!(
        table.rows,
        [
            vec![vec![
                Inline::Text("b ".into()),
                Inline::Code("|".into()),
                Inline::Text(" az".into())
            ]],
            vec![vec![
                Inline::Text("b ".into()),
                Inline::Strong(text("|")),
                Inline::Text(" im".into())
            ]],
        ]
    );
    let html = fragment(source);
    assert!(html.contains("<th>f|oo</th>"));
    assert!(html.contains("<td>b <code>|</code> az</td>"));
    assert!(html.contains("<td>b <strong>|</strong> im</td>"));
}

#[test]
fn escaped_final_pipes_belong_to_cells_and_headers() {
    for (row, expected) in [
        (r"| x | y\|", "y|"),
        (r"| x | y\\|", "y\\"),
        (r"| x | y\\\|", "y\\|"),
        (r"| x | y\|   ", "y|"),
    ] {
        let document = parse_markdown(&format!("{HEADER}{row}\n"));
        assert_eq!(
            table(&document).rows,
            [vec![text("x"), text(expected)]],
            "{row}"
        );
    }
    let document = parse_markdown("A | B\\|\n--- | ---\nx | y\n");
    assert_eq!(table(&document).head, [text("A"), text("B|")]);
}

#[test]
fn unmatched_backtick_runs_cannot_capture_later_cells() {
    for literal in ["`a", "``a`", "a ` b `` c", "a ``` b ``"] {
        let document = parse_markdown(&format!("{HEADER}| {literal} | right |\n"));
        assert_eq!(
            table(&document).rows[0],
            [text(literal), text("right")],
            "{literal}"
        );
    }
}

#[test]
fn matched_code_spans_keep_internal_pipes_and_use_code_escape_rules() {
    for (literal, expected) in [
        (r"`a|b`", vec![Inline::Code("a|b".into())]),
        (r"``a`|b``", vec![Inline::Code("a`|b".into())]),
        (r"`left\`", vec![Inline::Code("left\\".into())]),
        (
            r"\``left|right`",
            vec![Inline::Text("`".into()), Inline::Code("left|right".into())],
        ),
        (r"`a\\|b`", vec![Inline::Code("a\\|b".into())]),
    ] {
        let document = parse_markdown(&format!("{HEADER}| {literal} | right |\n"));
        assert_eq!(
            table(&document).rows[0],
            [expected, text("right")],
            "{literal}"
        );
    }
}

#[test]
fn nested_link_and_image_labels_inherit_table_code_pipe_decoding() {
    let source = format!("{HEADER}| [**`a\\|b`**](/link) | ![`c\\|d`](image.png) |\n");
    let document = parse_markdown(&source);
    assert_eq!(
        table(&document).rows[0][0],
        [Inline::Link {
            dest: "/link".into(),
            title: None,
            content: vec![Inline::Strong(vec![Inline::Code("a|b".into())])],
        }]
    );
    assert_eq!(
        table(&document).rows[0][1],
        [Inline::Image {
            dest: "image.png".into(),
            title: None,
            alt: "c|d".into(),
        }]
    );
    let destinations = destination_spans(&source);
    assert_eq!(
        destinations
            .iter()
            .map(|item| item.dest.as_str())
            .collect::<Vec<_>>(),
        ["/link", "image.png"]
    );
}

#[test]
fn table_decoding_does_not_reuse_or_pollute_the_prose_inline_cache() {
    let cell = "repeated long prefix `a\\|b`";
    let before = format!("{cell}\n\n").repeat(3);
    let source = format!(
        "{before}{cell} | right\n--- | ---\n{cell} | a\n{cell} | b\n{cell} | c\n\n{before}"
    );
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 7);
    for block in &document.blocks {
        match block {
            Block::Paragraph(inlines) => {
                assert_eq!(inlines.last(), Some(&Inline::Code("a\\|b".into())))
            }
            Block::Table(table) => {
                assert_eq!(table.head[0].last(), Some(&Inline::Code("a|b".into())));
                for row in &table.rows {
                    assert_eq!(row[0].last(), Some(&Inline::Code("a|b".into())));
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(parse_inlines("`a\\|b`"), [Inline::Code("a\\|b".into())]);
    assert_eq!(parse_markdown_profiled(&source).document, document);
    assert_eq!(parse_markdown_spanned(&source).to_document(), document);
}

#[test]
fn pipe_bearing_block_openers_end_a_table() {
    for (next, kind) in [
        ("# title | text\n", "heading"),
        ("> quote | text\n", "quote"),
        ("- item | text\n", "list"),
        ("2. item | text\n", "list"),
        ("    literal | code\n", "code"),
        ("~~~lang|pipe\nliteral\n~~~\n", "code"),
        ("<div>| literal\n</div>\n", "html"),
        ("$$ a|b $$\n", "math"),
        ("[^note]: note | text\n", "note"),
    ] {
        let source = format!("{HEADER}row | value\n{next}");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{next:?}: {document:?}");
        assert_eq!(table(&document).rows, [vec![text("row"), text("value")]]);
        let actual = match &document.blocks[1] {
            Block::Heading { .. } => "heading",
            Block::BlockQuote(_) => "quote",
            Block::List(_) => "list",
            Block::CodeBlock { .. } => "code",
            Block::HtmlBlock(_) => "html",
            Block::MathBlock(_) => "math",
            Block::FootnoteDefinition { .. } => "note",
            other => panic!("unexpected following block {other:?}: {next:?}"),
        };
        assert_eq!(actual, kind, "{next:?}");
        assert_eq!(parse_markdown_profiled(&source).document, document);
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    }
}

#[test]
fn escaped_and_malformed_block_markers_still_remain_rows() {
    let rows = [
        r"\# heading | value",
        r"\> quote | value",
        r"\- list | value",
        "#not-a-heading | value",
        "9999999999. not-a-list | value",
        "```bad`info | value",
    ];
    let source = format!("{HEADER}{}\n", rows.join("\n"));
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 1);
    assert_eq!(table(&document).rows.len(), rows.len());
    for row in &table(&document).rows {
        assert_eq!(row[1], text("value"));
    }
}

#[test]
fn list_markers_own_nested_table_headers_instead_of_becoming_header_text() {
    for marker in ["- ", "+ ", "* ", "1. ", "2) "] {
        let indent = " ".repeat(marker.len());
        let source =
            format!("{marker}Name | Value\n{indent}--- | ---\n{indent}row | value\n\noutside\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{marker}: {document:?}");
        let Block::List(list) = &document.blocks[0] else {
            panic!("list marker became a table header: {document:?}");
        };
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].blocks.len(), 1);
        let Block::Table(nested) = &list.items[0].blocks[0] else {
            panic!("missing nested table: {list:?}");
        };
        assert_eq!(nested.head, [text("Name"), text("Value")]);
        assert_eq!(nested.rows, [vec![text("row"), text("value")]]);
        assert_eq!(document.blocks[1], Block::Paragraph(text("outside")));
        assert_eq!(parse_markdown_profiled(&source).document, document);
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    }
}

#[test]
fn header_admission_preserves_real_block_markers_and_literal_cell_markers() {
    let heading = parse_markdown("# Name | Value\n--- | ---\n");
    assert!(matches!(
        &heading.blocks[0],
        Block::Heading { level: 1, .. }
    ));

    let quote = parse_markdown("> Name | Value\n> --- | ---\n> row | value\n");
    let Block::BlockQuote(blocks) = &quote.blocks[0] else {
        panic!("quote marker became header text: {quote:?}");
    };
    assert_eq!(blocks.len(), 1);
    assert!(
        matches!(&blocks[0], Block::Table(nested) if nested.head == [text("Name"), text("Value")])
    );

    for fence in ["```", "~~~"] {
        let source = format!("{fence}lang|pipe\n--- | ---\nrow | value\n{fence}\n");
        assert_eq!(
            parse_markdown(&source).blocks,
            [Block::CodeBlock {
                lang: Some("lang|pipe".into()),
                code: "--- | ---\nrow | value\n".into(),
            }]
        );
    }
    for marker in ["- ", "+ ", "* ", "# ", "> "] {
        for header in [
            format!("| {marker}Name | Value |"),
            format!("\\{marker}Name | Value"),
        ] {
            let document = parse_markdown(&format!("{header}\n--- | ---\n"));
            assert_eq!(document.blocks.len(), 1, "{header}: {document:?}");
            assert_eq!(
                table(&document).head,
                [text(&format!("{marker}Name")), text("Value")]
            );
        }
    }
}

#[test]
fn a_fence_after_a_table_keeps_literal_definitions_and_following_prose() {
    for fence in ["```", "~~~"] {
        let source =
            format!("{HEADER}row | value\n{fence}lang|pipe\n[ref]: /hidden\n{fence}\n\n[ref]\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 3, "{document:?}");
        assert_eq!(
            document.blocks[1],
            Block::CodeBlock {
                lang: Some("lang|pipe".into()),
                code: "[ref]: /hidden\n".into()
            }
        );
        assert_eq!(document.blocks[2], Block::Paragraph(text("[ref]")));
        assert!(destination_spans(&source).is_empty());
        let html = fragment(&source);
        assert!(html.contains("[ref]: /hidden\n</code></pre>"));
        assert!(html.ends_with("<p>[ref]</p>\n"));
    }
}

#[test]
fn real_definitions_after_a_table_and_fence_keep_their_own_authority() {
    let source = format!(
        "{HEADER}row | value\n```lang|pipe\n[ref]: /hidden\n```\n[ref]: /visible\n\n[ref]\n"
    );
    let html = fragment(&source);
    assert!(html.contains("[ref]: /hidden\n</code></pre>"));
    assert!(html.ends_with("<p><a href=\"/visible\">ref</a></p>\n"));
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/visible");
    assert_eq!(&source[spans[0].range.clone()], "/visible");
}

#[test]
fn reference_shaped_pipe_rows_remain_cells_without_defining_links() {
    let source = format!("{HEADER}[ref]: /hidden|left\n\n[ref]\n");
    let document = parse_markdown(&source);
    assert_eq!(
        table(&document).rows,
        [vec![text("[ref]: /hidden"), text("left")]]
    );
    assert_eq!(document.blocks[1], Block::Paragraph(text("[ref]")));
    assert!(destination_spans(&source).is_empty());
}

#[test]
fn nested_quote_list_and_note_tables_share_fence_boundaries() {
    let body = format!("{HEADER}```lang|pipe\n[ref]: /hidden\n```\n");
    let quote: String = body.lines().map(|line| format!("> {line}\n")).collect();
    let list = format!("- {}", body.replace('\n', "\n  ").trim_end());
    let note = format!("[^note]: {}", body.replace('\n', "\n    ").trim_end());
    for (nested, kind) in [(quote, "quote"), (list, "list"), (note, "note")] {
        let source = format!("{nested}\n\noutside [ref]\n");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{kind}: {document:?}");
        let blocks = match &document.blocks[0] {
            Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => blocks,
            Block::List(list) => &list.items[0].blocks,
            other => panic!("{kind}: {other:?}"),
        };
        assert_eq!(blocks.len(), 2, "{kind}: {blocks:?}");
        assert!(matches!(&blocks[0], Block::Table(table) if table.rows.is_empty()));
        assert_eq!(
            blocks[1],
            Block::CodeBlock {
                lang: Some("lang|pipe".into()),
                code: "[ref]: /hidden\n".into()
            }
        );
        assert_eq!(document.blocks[1], Block::Paragraph(text("outside [ref]")));
        assert!(destination_spans(&source).is_empty(), "{kind}");
    }

    // Once a list opens after a table, its paragraph can again continue
    // lazily across a missing outer quote marker. A stale table tail would
    // eject that continuation from both containers.
    let source = "> Name | Value\n> --- | ---\n> - item | text\ncontinuation\n";
    let document = parse_markdown(source);
    assert_eq!(document.blocks.len(), 1, "{document:?}");
    let Block::BlockQuote(blocks) = &document.blocks[0] else {
        panic!("expected quote: {document:?}");
    };
    assert_eq!(blocks.len(), 2);
    assert!(matches!(&blocks[0], Block::Table(table) if table.rows.is_empty()));
    let Block::List(list) = &blocks[1] else {
        panic!("expected list: {blocks:?}");
    };
    assert_eq!(
        list.items[0].blocks,
        [Block::Paragraph(vec![
            Inline::Text("item | text".into()),
            Inline::SoftBreak,
            Inline::Text("continuation".into()),
        ])]
    );
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
}

#[test]
fn source_and_destination_spans_remain_exact_with_unicode_crlf_and_pipe_escapes() {
    let source = "Pré | Δ\r\n--- | ---\r\n`a\\|b` [go](<dir/café\\|图.png>) | end\\|\r\n# Next | heading\r\n";
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks.len(), 2);
    let end = source.find("\r\n# Next").unwrap();
    assert_eq!(spanned.blocks[0].span.slice(source), Some(&source[..end]));
    assert_eq!(
        spanned.blocks[1].span.slice(source),
        Some("# Next | heading")
    );
    assert_eq!(spanned.to_document(), parse_markdown(source));
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "dir/café|图.png");
    assert_eq!(
        &source[destinations[0].range.clone()],
        "<dir/café\\|图.png>"
    );
}

#[test]
fn many_distinct_unmatched_backtick_runs_preserve_every_cell() {
    let count = 256;
    let head = vec!["h"; count].join(" | ");
    let delimiter = vec!["---"; count].join(" | ");
    let row: Vec<_> = (1..=count)
        .map(|n| format!("x {}", "`".repeat(n)))
        .collect();
    let source = format!(
        "{head}\n{delimiter}\n{}\n\nAfter [ok](/target)\n",
        row.join(" | ")
    );
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 2);
    let parsed = table(&document);
    assert_eq!(parsed.rows.len(), 1);
    assert_eq!(parsed.rows[0].len(), count);
    for (cell, expected) in parsed.rows[0].iter().zip(row) {
        assert_eq!(cell, &text(&expected));
    }
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/target");
}

#[test]
fn header_admission_and_ragged_rows_use_the_same_actual_cell_boundaries() {
    let source = "A `unmatched | B\n--- | ---\nleft | right\nshort |\nlong | row | ignored\n";
    let document = parse_markdown(source);
    let parsed = table(&document);
    assert_eq!(parsed.head, [text("A `unmatched"), text("B")]);
    assert_eq!(
        parsed.rows,
        [
            vec![text("left"), text("right")],
            vec![text("short"), vec![]],
            vec![text("long"), text("row")]
        ]
    );
    assert!(!fragment(source).contains("ignored"));
}
