//! GFM short rows retain table, container, and editable-source ownership.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::flow_display::{DisplayBlock, ResumableFlowDisplay};
use franken_markdown::parse::{ReferenceLabelForm, destination_spans, reference_label_spans};
use franken_markdown::source_map::{DocumentSourceMap, TextSelectionRange};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, Table, parse_markdown, parse_markdown_spanned,
};

const HEADER: &str = "| Name | Value |\n| --- | --- |\n";

fn text(value: &str) -> Vec<Inline> {
    vec![Inline::Text(value.into())]
}

fn first_table(document: &Document) -> &Table {
    let Block::Table(table) = &document.blocks[0] else {
        panic!("expected table: {document:?}");
    };
    table
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn unchanged_gfm_example_202_keeps_the_unpiped_body_row_in_the_table() {
    // https://github.github.com/gfm/#example-202. The blank line, not a missing
    // pipe, separates the final paragraph from the second body row.
    let source = "| abc | def |\n| --- | --- |\n| bar | baz |\nbar\n\nbar\n";
    let document = parse_markdown(source);
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(
        first_table(&document).rows,
        [vec![text("bar"), text("baz")], vec![text("bar"), vec![]]]
    );
    assert_eq!(document.blocks[1], Block::Paragraph(text("bar")));
    let html = fragment(source);
    assert!(html.contains("<tr><td>bar</td><td>baz</td></tr>\n<tr><td>bar</td><td></td></tr>"));
    assert_eq!(html.matches("<p>bar</p>").count(), 1);
}

#[test]
fn sparse_rows_can_alternate_with_full_rows_without_ending_the_table() {
    let source = format!("{HEADER}first\nleft | right\nlast\n| a | b | ignored |\n");
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 1);
    assert_eq!(
        first_table(&document).rows,
        [
            vec![text("first"), vec![]],
            vec![text("left"), text("right")],
            vec![text("last"), vec![]],
            vec![text("a"), text("b")],
        ]
    );
    assert!(!fragment(&source).contains("ignored"));
}

#[test]
fn single_column_tables_keep_links_images_code_and_formatted_rows() {
    let source = "| Status |\n| --- |\nQueued\n[guide](/guide)\n![badge](badge.png)\n`a  b`\n**ready**\n[known]\n\n[known]: /resolved\n";
    let document = parse_markdown(source);
    let table = first_table(&document);
    assert_eq!(document.blocks.len(), 1);
    assert_eq!(table.rows.len(), 6);
    assert_eq!(table.rows[0], [text("Queued")]);
    assert!(matches!(&table.rows[1][0][0], Inline::Link { dest, .. } if dest == "/guide"));
    assert!(
        matches!(&table.rows[2][0][0], Inline::Image { dest, alt, .. } if dest == "badge.png" && alt == "badge")
    );
    assert_eq!(table.rows[3], [vec![Inline::Code("a  b".into())]]);
    assert_eq!(table.rows[4], [vec![Inline::Strong(text("ready"))]]);
    assert!(matches!(&table.rows[5][0][0], Inline::Link { dest, .. } if dest == "/resolved"));
    assert_eq!(
        destination_spans(source)
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/guide", "badge.png", "/resolved"]
    );
}

#[test]
fn either_header_or_delimiter_can_supply_the_explicit_pipe() {
    for source in [
        "Status\n| --- |\nready\nqueued",
        "| Status |\n---\nready\nqueued",
    ] {
        let document = parse_markdown(source);
        assert_eq!(document.blocks.len(), 1, "{source}");
        assert_eq!(first_table(&document).head, [text("Status")]);
        assert_eq!(
            first_table(&document).rows,
            [vec![text("ready")], vec![text("queued")]]
        );
    }
    let setext = parse_markdown("Status\n---\nready");
    assert!(matches!(&setext.blocks[0], Block::Heading { level: 2, .. }));
    assert_eq!(setext.blocks[1], Block::Paragraph(text("ready")));
    for source in [
        "Status\n| nope |\nready",
        "Status\n| --- | --- |\nready",
        "Status\n| :-x |\nready",
    ] {
        assert!(
            matches!(&parse_markdown(source).blocks[..], [Block::Paragraph(_)]),
            "{source}"
        );
    }
}

#[test]
fn sparse_table_rows_cannot_authorize_dedented_lazy_container_continuations() {
    for source in [
        "> | Name | Value |\n> | --- | --- |\n> short\noutside\n",
        "- | Name | Value |\n  | --- | --- |\n  short\noutside\n",
        "> Status\n> | --- |\n> short\noutside\n",
        "- Status\n  | --- |\n  short\noutside\n",
    ] {
        let document = parse_markdown(source);
        assert_eq!(document.blocks.len(), 2, "{source}: {document:?}");
        let children = match &document.blocks[0] {
            Block::BlockQuote(children) => children,
            Block::List(list) => &list.items[0].blocks,
            other => panic!("expected container: {other:?}"),
        };
        assert!(
            matches!(&children[..], [Block::Table(table)] if table.rows.len() == 1),
            "{source}: {children:?}"
        );
        assert_eq!(document.blocks[1], Block::Paragraph(text("outside")));
        assert_eq!(parse_markdown_spanned(source).to_document(), document);
    }
}

#[test]
fn nested_containers_and_footnotes_share_short_row_ownership() {
    for source in [
        "> - Status\n>   | --- |\n>   [guide](/guide)\noutside\n",
        "- > Status\n  > | --- |\n  > [guide](/guide)\noutside\n",
        "[^note]: Status\n    | --- |\n    [guide](/guide)\noutside\n",
    ] {
        let document = parse_markdown(source);
        assert_eq!(document.blocks.len(), 2, "{source}: {document:?}");
        assert_eq!(document.blocks[1], Block::Paragraph(text("outside")));
        let html = fragment(source);
        if !source.starts_with("[^note]") {
            assert!(
                html.contains("<td><a href=\"/guide\">guide</a></td>"),
                "{html}"
            );
        } else {
            let Block::FootnoteDefinition { blocks, .. } = &document.blocks[0] else {
                panic!("{document:?}");
            };
            assert!(matches!(&blocks[..], [Block::Table(table)] if table.rows.len() == 1));
        }
        let destinations = destination_spans(source);
        assert_eq!(destinations.len(), 1);
        assert_eq!(&source[destinations[0].range.clone()], "/guide");
    }
}

#[test]
fn supported_blocks_still_terminate_a_table_after_short_rows() {
    for suffix in [
        "# Heading\n",
        "---\n",
        "- item\n",
        "2. item\n",
        "> quote\n",
        "```md\n[ref]: /hidden\n```\n",
        "    [ref]: /hidden\n",
        "<custom-tag>\n[ref]: /hidden\n</custom-tag>\n",
        "$$\nx+y\n$$\n",
        "[^note]: note\n",
    ] {
        let source = format!("{HEADER}short\n{suffix}\n[ref]\n");
        let document = parse_markdown(&source);
        assert_eq!(
            first_table(&document).rows,
            [vec![text("short"), vec![]]],
            "{suffix}"
        );
        assert!(document.blocks.len() >= 3, "{suffix}: {document:?}");
        assert!(
            destination_spans(&source).is_empty(),
            "literal definitions cannot leak: {source}"
        );
    }
}

#[test]
fn reference_definitions_keep_their_boundary_after_short_rows() {
    let source = "Status\n| --- |\nready\n[ref]: /target\nplain after\n\nUse [ref].\n";
    let document = parse_markdown(source);
    assert_eq!(document.blocks.len(), 3, "{document:?}");
    assert_eq!(first_table(&document).rows, [vec![text("ready")]]);
    assert_eq!(document.blocks[1], Block::Paragraph(text("plain after")));
    assert!(fragment(source).contains("Use <a href=\"/target\">ref</a>."));
    let source = format!("{HEADER}short\n[ref]: /first\nStatus\n| --- |\nlast\n\n[ref]\n");
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 3, "{document:?}");
    assert!(
        matches!(&document.blocks[1], Block::Table(table) if table.rows == [vec![text("last")]])
    );
}

#[test]
fn multiline_definitions_after_tables_keep_exact_definition_and_use_spans() {
    let source = "> Status\n> | --- |\n> ready\n> [two\n> words]:\n> /target\n> \"long\n> title\"\n>\n> [two words]\n";
    let document = parse_markdown(source);
    let Block::BlockQuote(blocks) = &document.blocks[0] else {
        panic!("{document:?}");
    };
    assert!(matches!(&blocks[0], Block::Table(table) if table.rows == [vec![text("ready")]]));
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    assert!(fragment(source).contains("href=\"/target\" title=\"long\ntitle\""));
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(&source[destinations[0].range.clone()], "/target");
    let labels = reference_label_spans(source);
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert_eq!(labels[0].form, ReferenceLabelForm::Definition);
    assert_eq!(labels[1].form, ReferenceLabelForm::Shortcut);
    assert_eq!(&source[labels[1].label.clone().unwrap()], "two words");
}

#[test]
fn pipe_bearing_definition_shapes_and_malformed_inline_shapes_remain_cells() {
    let source =
        format!("{HEADER}short\n[ref]: /hidden|right\n[not](a b)\n[closed] junk\n\n[ref]\n");
    let document = parse_markdown(&source);
    assert_eq!(
        first_table(&document).rows,
        [
            vec![text("short"), vec![]],
            vec![text("[ref]: /hidden"), text("right")],
            vec![text("[not](a b)"), vec![]],
            vec![text("[closed] junk"), vec![]],
        ]
    );
    assert_eq!(document.blocks[1], Block::Paragraph(text("[ref]")));
    assert!(destination_spans(&source).is_empty());
    assert!(reference_label_spans(&source).is_empty());
}

#[test]
fn mixed_unicode_crlf_cr_and_bom_keep_exact_source_and_destination_coordinates() {
    let source = "\u{feff}État\r\n| --- |\r😀 [go](<dir/图.png>)\r\n![徽章](image.png)\r# After\n";
    let document = parse_markdown_spanned(source);
    assert_eq!(document.blocks.len(), 2, "{document:?}");
    assert_eq!(
        document.blocks[0].span.slice(source),
        Some("État\r\n| --- |\r😀 [go](<dir/图.png>)\r\n![徽章](image.png)")
    );
    assert_eq!(document.blocks[1].span.slice(source), Some("# After"));
    assert_eq!(document.to_document(), parse_markdown(source));
    assert_eq!(
        destination_spans(source)
            .iter()
            .map(|span| &source[span.range.clone()])
            .collect::<Vec<_>>(),
        ["<dir/图.png>", "image.png"]
    );
}

#[test]
fn source_maps_and_resumable_native_display_keep_the_same_table_rows() {
    let source = "Name | Value\n--- | ---\n短い\nleft | right\n\nOutside\n";
    let document = parse_markdown_spanned(source);
    let map = DocumentSourceMap::from_spanned_document(&document, source).unwrap();
    let start = map.rendered_text().find("短い").unwrap();
    let selection = TextSelectionRange::new(start, start + "短い".len());
    assert_eq!(
        map.copy_exact_source_slices(selection, source).unwrap(),
        ["短い"]
    );
    assert_eq!(
        map.copy_enclosing_source(selection, source).unwrap(),
        "Name | Value\n--- | ---\n短い\nleft | right"
    );
    for batch in [1, 3, usize::MAX] {
        let mut display = ResumableFlowDisplay::new(source, batch);
        assert_eq!(
            display.process_all().unwrap(),
            [
                DisplayBlock::TableHeader {
                    cells: vec!["Name".into(), "Value".into()]
                },
                DisplayBlock::TableRow {
                    cells: vec!["短い".into(), "".into()]
                },
                DisplayBlock::TableRow {
                    cells: vec!["left".into(), "right".into()]
                },
                DisplayBlock::Paragraph {
                    text: "Outside".into()
                },
            ]
        );
    }
}

#[test]
fn thousands_of_short_rows_keep_linear_source_ownership_and_late_references() {
    let source = format!(
        "{HEADER}{}[ref]: /target\n\n[ref]\n",
        "short\n".repeat(4096)
    );
    let document = parse_markdown_spanned(&source);
    assert_eq!(document.blocks.len(), 2);
    assert!(matches!(&document.blocks[0].node, Block::Table(table) if table.rows.len() == 4096));
    assert_eq!(
        document.blocks[0].span.end,
        source.find("\n[ref]:").unwrap()
    );
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/target");
}

fn padding_boundary_source() -> String {
    // 512 rows * 1024 absent cells reaches, but does not exceed, the shared
    // 512K padding budget. The following complete row costs no padding.
    let columns = 1025;
    format!(
        "|{}\n|{}\n{}|{}\n",
        "H|".repeat(columns),
        "---|".repeat(columns),
        "short\n".repeat(512),
        "full|".repeat(columns),
    )
}

#[test]
fn sparse_padding_is_bounded_without_losing_source_or_promoting_definitions() {
    let prefix = padding_boundary_source();
    let fallback = "[hidden]: /wrong|right\n[also]: /wrong-too\n";
    let source = format!("{prefix}{fallback}\n[real]: /target\n\n[hidden] [also] [real]\n");
    let document = parse_markdown_spanned(&source);
    assert_eq!(
        document.blocks.len(),
        3,
        "table, literal fallback, visible uses"
    );
    let Block::Table(table) = &document.blocks[0].node else {
        panic!("expected bounded table");
    };
    assert_eq!(table.head.len(), 1025);
    assert_eq!(
        table.rows.len(),
        513,
        "a complete row remains admissible after padding is spent"
    );
    assert_eq!(
        table
            .rows
            .iter()
            .flatten()
            .filter(|cell| cell.is_empty())
            .count(),
        512 * 1024
    );
    assert!(
        table
            .rows
            .last()
            .unwrap()
            .iter()
            .all(|cell| cell == &text("full"))
    );
    assert_eq!(
        document.blocks[0].span.slice(&source),
        Some(prefix.trim_end_matches('\n'))
    );
    assert_eq!(
        document.blocks[1].span.slice(&source),
        Some(fallback.trim_end_matches('\n'))
    );
    assert!(matches!(&document.blocks[1].node, Block::Paragraph(_)));
    let destinations = destination_spans(&source);
    assert_eq!(
        destinations
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/target"]
    );
    let labels = reference_label_spans(&source);
    assert_eq!(labels.len(), 2);
    assert!(labels.iter().all(|label| label.key == "real"));
}

#[test]
fn container_laziness_and_reference_collection_share_the_padding_fallback() {
    let body = format!("{}[hidden]: /wrong|right\n", padding_boundary_source());
    let quoted = body
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>();
    let listed = format!("- {}\n", body.trim_end_matches('\n').replace('\n', "\n  "));
    for container in [quoted, listed] {
        let source = format!("{container}outside [hidden]\n");
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks.len(),
            1,
            "the fallback is an actual lazy paragraph"
        );
        let children = match &document.blocks[0] {
            Block::BlockQuote(children) => children,
            Block::List(list) => &list.items[0].blocks,
            other => panic!("expected container, got {other:?}"),
        };
        assert_eq!(children.len(), 2);
        assert!(matches!(&children[0], Block::Table(table) if table.rows.len() == 513));
        assert_eq!(
            children[1],
            Block::Paragraph(vec![
                Inline::Text("[hidden]: /wrong|right".into()),
                Inline::SoftBreak,
                Inline::Text("outside [hidden]".into()),
            ])
        );
        assert!(destination_spans(&source).is_empty());
    }
}

#[test]
fn padding_fallback_first_line_is_not_a_setext_underline_for_the_previous_table() {
    let prefix = padding_boundary_source();
    let fallback = "===\n[hidden]: /wrong";
    let source = format!("{prefix}{fallback}\n\n[hidden]\n");
    let document = parse_markdown_spanned(&source);
    assert_eq!(document.blocks.len(), 3);
    assert_eq!(document.blocks[1].span.slice(&source), Some(fallback));
    assert_eq!(
        document.blocks[1].node,
        Block::Paragraph(vec![
            Inline::Text("===".into()),
            Inline::SoftBreak,
            Inline::Text("[hidden]: /wrong".into()),
        ])
    );
    assert_eq!(document.blocks[2].node, Block::Paragraph(text("[hidden]")));
    assert!(destination_spans(&source).is_empty());
    assert!(reference_label_spans(&source).is_empty());

    // An actual second-line underline still closes the fallback paragraph.
    // Its following definition is at a real boundary and retains authority.
    let source = format!("{prefix}first\n===\n[visible]: /right\n\n[visible]\n");
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 3);
    assert_eq!(
        document.blocks[1],
        Block::Heading {
            level: 1,
            inlines: text("first")
        }
    );
    assert_eq!(
        destination_spans(&source)
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/right"]
    );
}
