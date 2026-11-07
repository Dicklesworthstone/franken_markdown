//! Accessible PDF tables retain their complete grid and resolve column headers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::pdf::verification_text_layer;
use franken_markdown::{
    Align, Block, Document, Inline, List, ListItem, PageMargins, PageSize, PdfASettings,
    PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPageEmission, Table, parse_markdown, render_pdf,
    render_pdf_document, render_pdf_emitted, render_pdf_pdfa,
};

#[derive(Debug)]
struct Node {
    id: usize,
    tag: String,
    parent: usize,
    body: String,
}

fn number_after(text: &str, needle: &str) -> usize {
    text.split_once(needle)
        .and_then(|(_, tail)| tail.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("missing number after {needle:?}: {text}"))
}

fn nodes(pdf: &[u8]) -> Vec<Node> {
    let text = String::from_utf8_lossy(pdf);
    let needle = " 0 obj\n<< /Type /StructElem /S /";
    let mut rest = text.as_ref();
    let mut result = Vec::new();
    while let Some(at) = rest.find(needle) {
        let id = rest[..at]
            .split_whitespace()
            .last()
            .unwrap()
            .parse()
            .unwrap();
        rest = &rest[at + needle.len()..];
        let end = rest.find("\nendobj").unwrap();
        let body = &rest[..end];
        result.push(Node {
            id,
            tag: body.split_whitespace().next().unwrap().to_owned(),
            parent: number_after(body, " /P "),
            body: body.to_owned(),
        });
        rest = &rest[end..];
    }
    result
}

fn node_by_id(elements: &[Node], id: usize) -> &Node {
    elements.iter().find(|node| node.id == id).unwrap()
}

/// Tables and rows contain only indirect child-node references in /K.
fn children<'a>(elements: &'a [Node], node: &Node) -> Vec<&'a Node> {
    let kids = node
        .body
        .split_once(" /K [ ")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0;
    let tokens: Vec<_> = kids.split_whitespace().collect();
    assert_eq!(tokens.len() % 3, 0, "{kids}");
    tokens
        .as_chunks::<3>()
        .0
        .iter()
        .map(|reference| {
            assert_eq!(&reference[1..], ["0", "R"]);
            let child = node_by_id(elements, reference[0].parse().unwrap());
            assert_eq!(child.parent, node.id);
            child
        })
        .collect()
}

fn header_id(node: &Node) -> &str {
    node.body
        .split_once(" /ID (")
        .unwrap()
        .1
        .split_once(')')
        .unwrap()
        .0
}

fn associated_headers(node: &Node) -> Vec<&str> {
    node.body
        .split_once(" /Headers [ ")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0
        .split_whitespace()
        .map(|value| value.strip_prefix('(').unwrap().strip_suffix(')').unwrap())
        .collect()
}

fn id_tree(pdf: &[u8]) -> Vec<(String, usize)> {
    let text = String::from_utf8_lossy(pdf);
    let names = text
        .split_once(" /IDTree << /Names [ ")
        .unwrap()
        .1
        .split_once(']')
        .unwrap()
        .0;
    let tokens: Vec<_> = names.split_whitespace().collect();
    assert_eq!(tokens.len() % 4, 0);
    tokens
        .as_chunks::<4>()
        .0
        .iter()
        .map(|entry| {
            assert_eq!(&entry[2..], ["0", "R"]);
            let name = entry[0]
                .strip_prefix('(')
                .unwrap()
                .strip_suffix(')')
                .unwrap();
            (name.to_owned(), entry[1].parse().unwrap())
        })
        .collect()
}

/// Follow real /Contents references and /Length, including compressed pages.
fn contents(pdf: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(pdf)
        .split("/Contents ")
        .skip(1)
        .map(|reference| {
            let number = reference.split_whitespace().next().unwrap();
            let header = format!("\n{number} 0 obj\n");
            let start = pdf
                .windows(header.len())
                .position(|bytes| bytes == header.as_bytes())
                .unwrap()
                + header.len();
            let object = &pdf[start..];
            let stream = object
                .windows(8)
                .position(|bytes| bytes == b"\nstream\n")
                .unwrap();
            let dictionary = std::str::from_utf8(&object[..stream]).unwrap();
            let length = number_after(dictionary, "/Length ");
            let encoded = &object[stream + 8..stream + 8 + length];
            let decoded = if dictionary.contains("/Filter /FlateDecode") {
                franken_markdown::zlib_decompress(encoded, 16 << 20).unwrap()
            } else {
                assert!(!dictionary.contains("/Filter"));
                encoded.to_vec()
            };
            String::from_utf8(decoded).unwrap()
        })
        .collect()
}

fn assert_grid(elements: &[Node], table: &Node, columns: usize, body_rows: usize) {
    let rows = children(elements, table);
    assert_eq!(rows.len(), body_rows + 1);
    assert!(rows.iter().all(|row| row.tag == "TR"));
    let headers = children(elements, rows[0]);
    assert_eq!(headers.len(), columns);
    for header in &headers {
        assert_eq!(header.tag, "TH");
        assert!(header.body.contains("/A << /O /Table /Scope /Column >>"));
        assert!(!header_id(header).is_empty());
    }
    for row in rows.iter().skip(1) {
        let cells = children(elements, row);
        assert_eq!(
            cells.len(),
            columns,
            "blank columns must retain their slots"
        );
        for (column, cell) in cells.iter().enumerate() {
            assert_eq!(cell.tag, "TD");
            assert_eq!(associated_headers(cell), [header_id(headers[column])]);
        }
    }
}

fn assert_lookup_and_content_ownership(pdf: &[u8], elements: &[Node]) {
    let names = id_tree(pdf);
    assert!(
        names.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "name-tree keys must be unique and byte-sorted"
    );
    let expected: BTreeMap<_, _> = elements
        .iter()
        .filter(|node| node.tag == "TH")
        .map(|node| (header_id(node).to_owned(), node.id))
        .collect();
    assert_eq!(names, expected.into_iter().collect::<Vec<_>>());
    assert_eq!(
        names.len(),
        elements.iter().filter(|node| node.tag == "TH").count()
    );

    let mut owners = BTreeMap::new();
    for node in elements {
        for tail in node.body.split("/Type /MCR /Pg ").skip(1) {
            let page: usize = tail.split_whitespace().next().unwrap().parse().unwrap();
            let mcid = number_after(tail, " /MCID ");
            assert!(owners.insert((page, mcid), node.id).is_none());
        }
    }
    let pages: BTreeSet<_> = owners.keys().map(|&(page, _)| page).collect();
    for page in pages {
        let mcids: Vec<_> = owners
            .keys()
            .filter_map(|&(p, mcid)| (p == page).then_some(mcid))
            .collect();
        assert_eq!(mcids, (0..mcids.len()).collect::<Vec<_>>());
    }
    let streams = contents(pdf);
    assert_eq!(
        owners.len(),
        streams
            .iter()
            .map(|stream| stream.matches("<</MCID ").count())
            .sum::<usize>()
    );
    for stream in streams {
        assert_eq!(
            stream.matches("BDC").count() + stream.matches("BMC").count(),
            stream.matches("EMC").count()
        );
    }
}

fn assert_annotation_owners(pdf: &[u8], elements: &[Node]) -> usize {
    let raw = String::from_utf8_lossy(pdf);
    let nums = raw
        .split_once("/Nums [ ")
        .unwrap()
        .1
        .split_once(" ] >>")
        .unwrap()
        .0;
    let mut annotations = BTreeSet::new();
    for node in elements {
        for reference in node.body.split("/Type /OBJR /Pg ").skip(1) {
            let id = number_after(reference, " /Obj ");
            assert_eq!(
                node.tag, "Link",
                "annotation {id} needs a semantic Link owner"
            );
            assert!(annotations.insert(id));
            let object = raw
                .split_once(&format!("\n{id} 0 obj\n"))
                .unwrap()
                .1
                .split_once("\nendobj")
                .unwrap()
                .0;
            let key = number_after(object, " /StructParent ");
            assert!(
                nums.contains(&format!(" {key} {} 0 R", node.id)),
                "annotation {id} must map back to its owner"
            );
        }
    }
    assert_eq!(annotations.len(), raw.matches("/Subtype /Link").count());
    annotations.len()
}

fn small_pages() -> PdfOptions {
    let mut opts = PdfOptions::default();
    opts.theme.page.size = PageSize {
        name: "table-accessibility",
        width_pt: 260.0,
        height_pt: 160.0,
    };
    opts.theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 20.0,
        bottom_pt: 20.0,
        left_pt: 20.0,
    };
    opts
}

#[test]
fn blank_headers_cells_and_entire_rows_preserve_column_positions() {
    let source = "| Name | | Q3 |\n|---|---|---|\n| ALPHA | | THREE |\n| | TWO | |\n| | | |\n";
    let options = PdfOptions::default();
    let pdf = render_pdf(source, &options).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 3, 3);
    assert_lookup_and_content_ownership(&pdf, &elements);
    let rows = children(&elements, table);
    let streams = contents(&pdf);
    assert_eq!(streams.len(), 1);
    for (row, column) in [(0, 1), (1, 1), (2, 0), (2, 2), (3, 0), (3, 1), (3, 2)] {
        let cell = children(&elements, rows[row])[column];
        let mcid = number_after(&cell.body, " /MCID ");
        assert!(streams[0].contains(&format!("/{} <</MCID {mcid}>> BDC\nEMC\n", cell.tag)));
    }
    let layer = verification_text_layer(&parse_markdown(source), &options).unwrap();
    let rows: Vec<_> = layer.pages[0]
        .runs
        .iter()
        .filter(|run| run.kind == "table_row")
        .collect();
    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        ["ALPHATHREE", "TWO", ""]
    );
    assert_eq!(
        rows[2].x, 0.0,
        "blank semantics must not introduce fake paint segments"
    );
    assert!(rows.iter().all(|row| row.overshoot.is_none()));
}

#[test]
fn wholly_blank_table_is_still_a_tagged_rectangular_grid() {
    let pdf = render_pdf("| | | |\n|---|---|---|\n| | | |\n", &PdfOptions::default()).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 3, 1);
    assert_lookup_and_content_ownership(&pdf, &elements);
    let streams = contents(&pdf);
    assert_eq!(
        streams
            .iter()
            .map(|stream| stream.matches("<</MCID ").count())
            .sum::<usize>(),
        6
    );
    assert!(streams.iter().all(|stream| !stream.contains("BT /F")));
    assert!(String::from_utf8_lossy(&pdf).contains("/MarkInfo << /Marked true >>"));
}

#[test]
fn ragged_host_tables_backfill_missing_cells_to_the_rendered_grid_width() {
    let doc = Document {
        blocks: vec![Block::Table(Table {
            align: vec![Align::Left; 3],
            head: vec![vec![Inline::Text("Only supplied header".into())]],
            rows: vec![
                vec![],
                vec![vec![], vec![], vec![Inline::Text("last column".into())]],
            ],
        })],
    };
    let pdf = render_pdf_document(&doc, &PdfOptions::default()).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 3, 2);
    assert_lookup_and_content_ownership(&pdf, &elements);
}

#[test]
fn cached_identical_tables_have_distinct_headers_and_lexically_sorted_id_lookup() {
    let head = (0..12)
        .map(|column| format!("H{column}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let table = format!(
        "| {head} |\n| {} |\n| {} |\n",
        ["---"; 12].join(" | "),
        [""; 12].join(" | ")
    );
    let source = format!("{table}\nSeparate tables.\n\n{table}");
    let options = PdfOptions::default();
    let pdf = render_pdf(&source, &options).unwrap();
    let elements = nodes(&pdf);
    let tables: Vec<_> = elements.iter().filter(|node| node.tag == "Table").collect();
    assert_eq!(tables.len(), 2);
    for table in tables {
        assert_grid(&elements, table, 12, 1);
    }
    assert_lookup_and_content_ownership(&pdf, &elements);
    assert_eq!(id_tree(&pdf).len(), 24);
    assert_eq!(pdf, render_pdf(&source, &options).unwrap());
    let monolithic = render_pdf_emitted(
        &source,
        &options,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(pdf, monolithic);
}

#[test]
fn wrapped_and_repeated_cells_keep_one_header_association_and_link_owner() {
    let mut source = format!(
        "| [Name](https://example.com/header) | | Detail |\n|---|---|---|\n| First | | {} |\n",
        "wrapping detail ".repeat(65)
    );
    for row in 0..20 {
        source.push_str(&format!("| Row{row} | | tail |\n"));
    }
    let options = small_pages();
    let pdf = render_pdf(&source, &options).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 3, 21);
    assert_lookup_and_content_ownership(&pdf, &elements);
    let rows = children(&elements, table);
    let long_cell = children(&elements, rows[1])[2];
    let pages: BTreeSet<_> = long_cell
        .body
        .split("/Type /MCR /Pg ")
        .skip(1)
        .map(|tail| tail.split_whitespace().next().unwrap())
        .collect();
    assert!(
        pages.len() > 1,
        "the cell must actually continue across pages"
    );
    let header = children(&elements, rows[0])[0];
    let links: Vec<_> = elements
        .iter()
        .filter(|node| node.tag == "Link" && node.parent == header.id)
        .collect();
    assert_eq!(links.len(), 1);
    assert!(links[0].body.matches("/Type /OBJR").count() > 1);
    assert!(!header.body.contains("/Type /OBJR"));
    assert!(assert_annotation_owners(&pdf, &elements) > 1);
    assert!(
        contents(&pdf)
            .iter()
            .skip(1)
            .any(|stream| stream.contains("/Artifact <</Type /Pagination /Subtype /Header>> BDC"))
    );
    let monolithic = render_pdf_emitted(
        &source,
        &options,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(pdf, monolithic);
}

#[test]
fn separate_same_target_cell_links_preserve_reading_order_through_wrapping_and_repetition() {
    let target = "https://example.com/shared";
    let source = format!(
        "| [one]({target}) middle [two]({target}) | Side |\n|---|---|\n\
         | [**{}**]({target}) between [last]({target}) | [neighbor]({target}) |\n",
        ["wrapping words"; 45].join(" ")
    );
    let options = small_pages();
    let pdf = render_pdf(&source, &options).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 2, 1);
    assert_lookup_and_content_ownership(&pdf, &elements);
    let rows = children(&elements, table);
    for row in &rows {
        let cell = children(&elements, row)[0];
        let links: Vec<_> = elements
            .iter()
            .filter(|node| node.tag == "Link" && node.parent == cell.id)
            .collect();
        assert_eq!(
            links.len(),
            2,
            "intervening prose separates same-target links"
        );
        let kids = cell
            .body
            .split_once(" /K [")
            .unwrap()
            .1
            .split_once(']')
            .unwrap()
            .0;
        let first = kids.find(&format!(" {} 0 R", links[0].id)).unwrap();
        let middle = kids.find("/Type /MCR ").unwrap();
        let last = kids.find(&format!(" {} 0 R", links[1].id)).unwrap();
        assert!(
            first < middle && middle < last,
            "cell reading order: {kids}"
        );
        assert!(!cell.body.contains("/Type /OBJR"));
        if cell.tag == "TH" {
            for link in links {
                let pages: BTreeSet<_> = link
                    .body
                    .split("/Type /OBJR /Pg ")
                    .skip(1)
                    .map(|tail| tail.split_whitespace().next().unwrap())
                    .collect();
                assert!(
                    pages.len() > 1,
                    "each original header link repeats across pages"
                );
            }
        } else {
            let pages: BTreeSet<_> = links[0]
                .body
                .split("/Type /MCR /Pg ")
                .skip(1)
                .map(|tail| tail.split_whitespace().next().unwrap())
                .collect();
            assert!(
                pages.len() > 1,
                "one link continues through wrapping and page breaks"
            );
        }
    }
    assert_eq!(elements.iter().filter(|node| node.tag == "Link").count(), 5);
    assert!(assert_annotation_owners(&pdf, &elements) > 5);
    assert_eq!(
        pdf,
        render_pdf_emitted(&source, &options, PdfEmitOptions::monolithic()).unwrap()
    );
}

#[test]
fn blank_tables_keep_list_and_footnote_container_ownership() {
    let table = Block::Table(Table {
        align: vec![Align::Left; 2],
        head: vec![vec![], vec![]],
        rows: vec![vec![vec![], vec![]]],
    });
    let doc = Document {
        blocks: vec![
            Block::List(List {
                ordered: false,
                start: 1,
                tight: false,
                items: vec![ListItem {
                    task: None,
                    blocks: vec![
                        Block::Paragraph(vec![Inline::Text("Body list".into())]),
                        table.clone(),
                    ],
                }],
            }),
            Block::Paragraph(vec![
                Inline::Text("Reference".into()),
                Inline::FootnoteRef { id: "note".into() },
            ]),
            Block::FootnoteDefinition {
                id: "note".into(),
                blocks: vec![
                    Block::Paragraph(vec![Inline::Text("Footnote".into())]),
                    Block::BlockQuote(vec![table]),
                ],
            },
        ],
    };
    let pdf = render_pdf_document(&doc, &PdfOptions::default()).unwrap();
    let elements = nodes(&pdf);
    let tables: Vec<_> = elements.iter().filter(|node| node.tag == "Table").collect();
    assert_eq!(tables.len(), 2);
    assert_eq!(node_by_id(&elements, tables[0].parent).tag, "LBody");
    assert_eq!(node_by_id(&elements, tables[1].parent).tag, "BlockQuote");
    for table in tables {
        assert_grid(&elements, table, 2, 1);
    }
    assert_lookup_and_content_ownership(&pdf, &elements);
    assert_eq!(assert_annotation_owners(&pdf, &elements), 1);
}

#[test]
fn image_cells_keep_alt_text_and_header_associations_without_repeated_figures() {
    let mut options = small_pages();
    options.image_assets = vec![PdfImageAsset::new("status.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#007ec6"/></svg>"##.to_vec())];
    let mut source = String::from(
        "| [![Status heading](status.svg)](https://example.com/status) | | Value |\n|---|---|---|\n| ![Ready](status.svg) | | first |\n",
    );
    for row in 0..20 {
        source.push_str(&format!("| State{row} | | value |\n"));
    }
    let pdf = render_pdf(&source, &options).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 3, 21);
    assert_lookup_and_content_ownership(&pdf, &elements);
    let figures: Vec<_> = elements
        .iter()
        .filter(|node| node.tag == "Figure")
        .collect();
    assert_eq!(
        figures.len(),
        2,
        "repeated visual headers must not repeat image alt text"
    );
    assert!(figures[0].body.contains("/Alt (Status heading)"));
    let header_link = node_by_id(&elements, figures[0].parent);
    assert_eq!(header_link.tag, "Link");
    assert_eq!(node_by_id(&elements, header_link.parent).tag, "TH");
    assert!(header_link.body.matches("/Type /OBJR").count() > 1);
    assert!(!figures[0].body.contains("/Type /OBJR"));
    assert!(figures[1].body.contains("/Alt (Ready)"));
    assert_eq!(node_by_id(&elements, figures[1].parent).tag, "TD");
    assert!(assert_annotation_owners(&pdf, &elements) > 1);
}

#[test]
fn linked_table_formulas_and_images_keep_semantics_across_repeated_headers() {
    let mut options = small_pages();
    options.image_assets = vec![PdfImageAsset::new(
        "status.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#007ec6"/></svg>"##.to_vec(),
    )];
    let mut source = String::from(
        "| [**Rule** $x^2$](#details) | [Status](https://example.com/shared) |\n|---|---|\n\
         | [$y_1$](https://example.com/shared) | [**Ready** ![Status icon](status.svg) $z$](https://example.com/shared) |\n",
    );
    for row in 0..20 {
        source.push_str(&format!("| Rule{row} | Ready |\n"));
    }
    source.push_str("\n# Details\n");
    let pdf = render_pdf(&source, &options).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 2, 21);
    assert_lookup_and_content_ownership(&pdf, &elements);
    let links: Vec<_> = elements.iter().filter(|node| node.tag == "Link").collect();
    assert_eq!(
        links.len(),
        4,
        "same-target links in separate cells stay distinct"
    );
    for link in &links {
        assert!(matches!(
            node_by_id(&elements, link.parent).tag.as_str(),
            "TH" | "TD"
        ));
        assert!(link.body.contains("/Type /OBJR"));
    }
    let formulas: Vec<_> = elements
        .iter()
        .filter(|node| node.tag == "Formula")
        .collect();
    assert_eq!(
        formulas.len(),
        3,
        "repeated headers do not duplicate formula content"
    );
    for (formula, source) in formulas.iter().zip(["x^2", "y_1", "z"]) {
        assert!(formula.body.contains(&format!("/Alt ({source})")));
        assert!(formula.body.contains("/BBox ["));
        assert!(!formula.body.contains("/Type /OBJR"));
        assert_eq!(node_by_id(&elements, formula.parent).tag, "Link");
    }
    let header_link = node_by_id(&elements, formulas[0].parent);
    assert_eq!(node_by_id(&elements, header_link.parent).tag, "TH");
    let pages: BTreeSet<_> = header_link
        .body
        .split("/Type /OBJR /Pg ")
        .skip(1)
        .map(|tail| tail.split_whitespace().next().unwrap())
        .collect();
    assert!(
        pages.len() > 1,
        "header links remain accessible on repeated pages"
    );
    let figures: Vec<_> = elements
        .iter()
        .filter(|node| node.tag == "Figure")
        .collect();
    assert_eq!(figures.len(), 1);
    assert!(figures[0].body.contains("/Alt (Status icon)"));
    assert!(!figures[0].body.contains("/Type /OBJR"));
    assert_eq!(
        figures[0].parent, formulas[2].parent,
        "mixed link shares one owner"
    );
    assert!(assert_annotation_owners(&pdf, &elements) > 4);
    assert!(
        contents(&pdf)
            .iter()
            .skip(1)
            .any(|stream| stream.contains("/Artifact <</Type /Pagination /Subtype /Header>> BDC"))
    );
    assert_eq!(
        pdf,
        render_pdf_emitted(&source, &options, PdfEmitOptions::monolithic()).unwrap()
    );
}

#[test]
fn archival_tables_keep_the_same_header_lookup_and_non_tables_add_no_id_tree() {
    let source = "| Name | Value |\n|---|---|\n| | [value](https://example.com/value) |\n";
    let options = PdfOptions {
        title: Some("Accessible table".into()),
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let pdf = render_pdf_pdfa(source, &options, PdfASettings::a2b()).unwrap();
    let elements = nodes(&pdf);
    let table = elements.iter().find(|node| node.tag == "Table").unwrap();
    assert_grid(&elements, table, 2, 1);
    assert_lookup_and_content_ownership(&pdf, &elements);
    assert_eq!(assert_annotation_owners(&pdf, &elements), 1);
    let ordinary = render_pdf("# Heading\n\nAn ordinary paragraph.", &options).unwrap();
    assert!(!String::from_utf8_lossy(&ordinary).contains("/IDTree"));
}
