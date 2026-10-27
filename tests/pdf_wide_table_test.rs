//! Wide PDF tables retain every column through readable horizontal panels.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::pdf::verification_text_layer;
use franken_markdown::{
    Align, Block, Document, Inline, List, ListItem, PageMargins, PageSize, PdfEmitOptions,
    PdfOptions, PdfPageEmission, Table, parse_markdown, render_pdf, render_pdf_document,
    render_pdf_emitted,
};

struct Node {
    id: usize,
    tag: String,
    parent: usize,
    body: String,
}

fn number_after(text: &str, needle: &str) -> usize {
    text.split_once(needle)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn nodes(pdf: &[u8]) -> Vec<Node> {
    let text = String::from_utf8_lossy(pdf);
    let needle = " 0 obj\n<< /Type /StructElem /S /";
    let mut rest = text.as_ref();
    let mut nodes = Vec::new();
    while let Some(at) = rest.find(needle) {
        let id = rest[..at]
            .split_whitespace()
            .last()
            .unwrap()
            .parse()
            .unwrap();
        rest = &rest[at + needle.len()..];
        let body = &rest[..rest.find("\nendobj").unwrap()];
        nodes.push(Node {
            id,
            tag: body.split_whitespace().next().unwrap().to_owned(),
            parent: number_after(body, " /P "),
            body: body.to_owned(),
        });
    }
    nodes
}

fn children<'a>(nodes: &'a [Node], parent: &Node) -> Vec<&'a Node> {
    let kids = parent
        .body
        .split_once(" /K [ ")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0;
    let tokens: Vec<_> = kids.split_whitespace().collect();
    assert_eq!(tokens.len() % 3, 0);
    tokens
        .chunks_exact(3)
        .map(|reference| {
            assert_eq!(&reference[1..], ["0", "R"]);
            let id: usize = reference[0].parse().unwrap();
            let node = nodes.iter().find(|node| node.id == id).unwrap();
            assert_eq!(node.parent, parent.id);
            node
        })
        .collect()
}

fn assert_grids(nodes: &[Node], widths: &[usize], body_rows: usize) {
    let tables: Vec<_> = nodes.iter().filter(|node| node.tag == "Table").collect();
    assert_eq!(tables.len(), widths.len());
    let mut header_ids = BTreeSet::new();
    for (table, &width) in tables.iter().zip(widths) {
        let rows = children(nodes, table);
        assert_eq!(rows.len(), body_rows + 1);
        assert!(rows.iter().all(|row| row.tag == "TR"));
        let headers = children(nodes, rows[0]);
        assert_eq!(headers.len(), width);
        let ids: Vec<_> = headers
            .iter()
            .map(|header| {
                assert_eq!(header.tag, "TH");
                assert!(header.body.contains("/Scope /Column"));
                let id = header
                    .body
                    .split_once(" /ID (")
                    .unwrap()
                    .1
                    .split_once(')')
                    .unwrap()
                    .0;
                assert!(
                    header_ids.insert(id.to_owned()),
                    "header IDs must be unique across panels"
                );
                id
            })
            .collect();
        for row in rows.iter().skip(1) {
            let cells = children(nodes, row);
            assert_eq!(cells.len(), width);
            for (cell, id) in cells.iter().zip(&ids) {
                assert_eq!(cell.tag, "TD");
                assert!(
                    cell.body.contains(&format!("/Headers [ ({id}) ]")),
                    "cell must point to its own panel header: {}",
                    cell.body
                );
            }
        }
    }
}

fn source(columns: usize, rows: usize) -> String {
    let head = (0..columns)
        .map(|column| {
            if column + 1 == columns {
                format!("[H{column:02}](https://example.com/header)")
            } else {
                format!("H{column:02}")
            }
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let mut source = format!("| {head} |\n| {} |\n", vec!["---"; columns].join(" | "));
    for _ in 0..rows {
        let row = (0..columns)
            .map(|column| {
                if column + 1 == columns {
                    format!("[v{column:02}](https://example.com/last)")
                } else {
                    format!("v{column:02}")
                }
            })
            .collect::<Vec<_>>()
            .join(" | ");
        source.push_str(&format!("| {row} |\n"));
    }
    source
}

fn assert_body_contained(doc: &Document, options: &PdfOptions) -> usize {
    let layer = verification_text_layer(doc, options).unwrap();
    for run in layer.pages.iter().flat_map(|page| &page.runs) {
        if matches!(run.kind, "table_header" | "table_row") {
            assert!(run.overshoot.is_none(), "table overflow: {run:?}");
            assert!(run.size >= 5.0, "table must retain readable font floor");
        }
    }
    layer.page_count
}

#[test]
fn all_wide_columns_keep_content_links_and_panel_header_associations() {
    let source = source(24, 3);
    let options = PdfOptions::default();
    let doc = parse_markdown(&source);
    assert_body_contained(&doc, &options);
    let layer = verification_text_layer(&doc, &options).unwrap();
    let text: String = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    for column in 0..24 {
        assert!(
            text.contains(&format!("H{column:02}")),
            "missing header {column}"
        );
        assert!(
            text.contains(&format!("v{column:02}")),
            "missing body column {column}"
        );
    }
    let pdf = render_pdf(&source, &options).unwrap();
    let nodes = nodes(&pdf);
    assert_grids(&nodes, &[14, 11], 3);
    let pdf_text = String::from_utf8_lossy(&pdf);
    assert!(pdf_text.contains("https://example.com/header"));
    assert!(pdf_text.contains("https://example.com/last"));
    let final_header = nodes.iter().rfind(|node| node.tag == "TH").unwrap();
    assert!(final_header.body.contains("/Type /OBJR"));
    assert_eq!(pdf, render_pdf(&source, &options).unwrap());
    assert_eq!(
        pdf,
        render_pdf_emitted(
            &source,
            &options,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..PdfEmitOptions::default()
            }
        )
        .unwrap()
    );
}

#[test]
fn panel_pagination_repeats_headers_without_duplicating_logical_rows() {
    let source = source(24, 45);
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "wide-table-test",
        width_pt: 612.0,
        height_pt: 220.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 72.0,
        bottom_pt: 20.0,
        left_pt: 72.0,
    };
    assert!(assert_body_contained(&parse_markdown(&source), &options) > 2);
    let pdf = render_pdf(&source, &options).unwrap();
    let nodes = nodes(&pdf);
    assert_grids(&nodes, &[14, 11], 45);
    let last_header = nodes.iter().rfind(|node| node.tag == "TH").unwrap();
    assert!(
        last_header.body.matches("/Type /OBJR").count() > 1,
        "the late-column linked header must repeat across pages"
    );
}

#[test]
fn repeated_wide_tables_keep_distinct_tables_and_header_ids() {
    let table = source(24, 2);
    let source = format!("{table}\nSeparate tables.\n\n{table}");
    let options = PdfOptions::default();
    let pdf = render_pdf(&source, &options).unwrap();
    assert_grids(&nodes(&pdf), &[14, 11, 14, 11], 2);
    assert_eq!(pdf, render_pdf(&source, &options).unwrap());
}

#[test]
fn forty_point_body_keeps_single_column_panels_and_sparse_cells() {
    let table = Table {
        align: vec![Align::Left; 4],
        head: vec![vec![Inline::Text("A".into())]],
        rows: vec![
            vec![],
            vec![vec![], vec![], vec![], vec![Inline::Text("Z".into())]],
        ],
    };
    let doc = Document {
        blocks: vec![Block::Table(table)],
    };
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "narrow-table-test",
        width_pt: 80.0,
        height_pt: 300.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 20.0,
        bottom_pt: 20.0,
        left_pt: 20.0,
    };
    assert_body_contained(&doc, &options);
    let pdf = render_pdf_document(&doc, &options).unwrap();
    assert_grids(&nodes(&pdf), &[1, 1, 1, 1], 2);
    let layer = verification_text_layer(&doc, &options).unwrap();
    assert!(
        layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text == "Z")
    );
}

#[test]
fn alignment_declares_trailing_empty_columns_with_sparse_header_and_rows() {
    for (columns, widths) in [(3, vec![3]), (20, vec![14, 7])] {
        let table = Table {
            align: vec![Align::Left; columns],
            head: vec![vec![Inline::Text("ID".into())]],
            rows: vec![vec![], vec![vec![Inline::Text("A".into())]]],
        };
        let doc = Document {
            blocks: vec![Block::Table(table)],
        };
        let options = PdfOptions::default();
        assert_body_contained(&doc, &options);
        let pdf = render_pdf_document(&doc, &options).unwrap();
        // Only the alignment declaration establishes the trailing columns.
        // Ordinary and panelled tables must retain their empty logical cells.
        assert_grids(&nodes(&pdf), &widths, 2);
    }
}

#[test]
fn projected_financial_rows_keep_literal_markers_and_late_decimal_columns() {
    let text = |value: String| vec![Inline::Text(value)];
    let mut table = Table {
        align: vec![Align::Left; 20],
        head: (0..20)
            .map(|column| text(format!("H{column:02}")))
            .collect(),
        rows: vec![
            (0..20)
                .map(|column| text(format!("v{column:02}")))
                .collect(),
        ],
    };
    table.align[19] = Align::Decimal;
    let mut data = vec![Vec::new(); 20];
    data[5] = text("---".to_owned());
    data[18] = text("keep".to_owned());
    table.rows.push(data);
    let mut footing = vec![Vec::new(); 20];
    footing[19] = text("===".to_owned());
    table.rows.push(footing);
    let doc = Document {
        blocks: vec![Block::Table(table)],
    };
    let options = PdfOptions::default();
    assert_body_contained(&doc, &options);
    let layer = verification_text_layer(&doc, &options).unwrap();
    let text: String = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    assert!(text.contains("---"), "ordinary marker spelling is data");
    assert!(
        text.contains("keep"),
        "late-column data must remain visible"
    );
    assert!(
        !text.contains("==="),
        "the full-table footing is decoration"
    );
    let pdf = render_pdf_document(&doc, &options).unwrap();
    assert_grids(&nodes(&pdf), &[14, 7], 2);
}

#[test]
fn panels_keep_nested_list_and_quote_ownership() {
    let parsed = parse_markdown(&source(24, 2));
    let doc = Document {
        blocks: vec![Block::List(List {
            ordered: false,
            start: 1,
            tight: false,
            items: vec![ListItem {
                task: None,
                blocks: vec![
                    Block::Paragraph(vec![Inline::Text("Nested table".into())]),
                    Block::BlockQuote(parsed.blocks),
                ],
            }],
        })],
    };
    let options = PdfOptions::default();
    assert_body_contained(&doc, &options);
    let pdf = render_pdf_document(&doc, &options).unwrap();
    let nodes = nodes(&pdf);
    let tables: Vec<_> = nodes.iter().filter(|node| node.tag == "Table").collect();
    assert_eq!(tables.len(), 2);
    for table in tables {
        let quote = nodes.iter().find(|node| node.id == table.parent).unwrap();
        assert_eq!(quote.tag, "BlockQuote");
        let body = nodes.iter().find(|node| node.id == quote.parent).unwrap();
        assert_eq!(body.tag, "LBody");
    }
}
