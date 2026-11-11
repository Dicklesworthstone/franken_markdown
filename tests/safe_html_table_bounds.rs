//! Sparse raw HTML tables must not synthesize an unbounded rectangular grid.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{
    Align, Block, Document, HtmlOptions, Inline, PdfOptions, Table, parse_markdown, render_epub,
    render_pdf_document, safe_html,
};

const WIDE_COLUMNS: usize = 1025;
const NARROW_ROWS: usize = 513;

fn sparse_table() -> String {
    let mut source = String::from("<table><tr>");
    for column in 0..WIDE_COLUMNS {
        source.push_str(&format!("<th>H{column}</th>"));
    }
    source.push_str("</tr>");
    for row in 0..NARROW_ROWS {
        source.push_str(&format!("<tr><td>ROW{row}</td></tr>"));
    }
    source.push_str("</table>\n");
    source
}

fn plain(value: impl Into<String>) -> Inline {
    Inline::Text(value.into())
}

fn expected_rows() -> Vec<Block> {
    let header = (0..WIDE_COLUMNS)
        .map(|index| format!("H{index}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let mut blocks = vec![Block::Paragraph(vec![Inline::Strong(vec![plain(header)])])];
    blocks.extend((0..NARROW_ROWS).map(|row| Block::Paragraph(vec![plain(format!("ROW{row}"))])));
    blocks
}

fn text(inlines: &[Inline]) -> String {
    let mut result = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(value) | Inline::Code(value) => result.push_str(value),
            Inline::Strong(inner) | Inline::Emphasis(inner) => result.push_str(&text(inner)),
            _ => panic!("unexpected node in the plain table fixture: {inline:?}"),
        }
    }
    result
}

#[test]
fn raw_html_sparse_grid_falls_back_before_allocating_quadratic_padding() {
    let source = sparse_table();
    // This is a small source fixture; the original rectangular expansion
    // creates 525,312 empty cell Vecs (over 12 MiB on a 64-bit host).
    assert!(source.len() < 32 * 1024);
    let document = parse_markdown(&source);
    let original = document.clone();
    let lowered = safe_html::lower(&document);
    assert!(
        lowered
            .blocks
            .iter()
            .all(|block| matches!(block, Block::Paragraph(_))),
        "an over-budget sparse table must keep its rows without allocating an expanded grid"
    );
    assert_eq!(lowered.blocks.len(), NARROW_ROWS + 1);
    let Block::Paragraph(header) = &lowered.blocks[0] else {
        unreachable!()
    };
    assert_eq!(
        text(header),
        (0..WIDE_COLUMNS)
            .map(|index| format!("H{index}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    for (index, block) in lowered.blocks[1..].iter().enumerate() {
        let Block::Paragraph(inlines) = block else {
            unreachable!()
        };
        assert_eq!(text(inlines), format!("ROW{index}"));
    }
    assert_eq!(
        document, original,
        "lowering must not mutate the source AST"
    );
    assert_eq!(safe_html::lower(&lowered).as_ref(), lowered.as_ref());
}

#[test]
fn sparse_html_table_admits_the_exact_padding_budget() {
    let source = sparse_table().replacen("<tr><td>ROW512</td></tr>", "", 1);
    let document = parse_markdown(&source);
    let lowered = safe_html::lower(&document);
    let [Block::Table(table)] = lowered.blocks.as_slice() else {
        panic!("a table at the exact limit must remain a table");
    };
    assert_eq!(table.head.len(), WIDE_COLUMNS);
    assert_eq!(table.rows.len(), NARROW_ROWS - 1);
    let mut generated = 0;
    for (row, cells) in table.rows.iter().enumerate() {
        assert_eq!(cells.len(), WIDE_COLUMNS);
        assert_eq!(cells[0], vec![plain(format!("ROW{row}"))]);
        assert!(cells[1..].iter().all(Vec::is_empty));
        generated += cells[1..].len();
    }
    assert_eq!(generated, 512 * 1024);
}

#[test]
fn ordinary_ragged_html_tables_retain_the_existing_grid_and_inline_content() {
    let source = parse_markdown(
        "<table><tr><th>Heading</th></tr>\
         <tr><td><em>First</em></td><td></td><td><code>x y</code></td></tr>\
         <tr><td><a href='https://example.com/ordinary'>Last</a></td></tr></table>\n",
    );
    let expected = Document {
        blocks: vec![Block::Table(Table {
            align: vec![Align::None; 3],
            head: vec![vec![plain("Heading")], vec![], vec![]],
            rows: vec![
                vec![
                    vec![Inline::Emphasis(vec![plain("First")])],
                    vec![],
                    vec![Inline::Code("x y".into())],
                ],
                vec![
                    vec![Inline::Link {
                        dest: "https://example.com/ordinary".into(),
                        title: None,
                        content: vec![plain("Last")],
                    }],
                    vec![],
                    vec![],
                ],
            ],
        })],
    };
    assert_eq!(safe_html::lower(&source).as_ref(), &expected);
}

#[test]
fn a_late_wide_row_cannot_expand_the_earlier_rows_unboundedly() {
    let source = sparse_table();
    let (wide, narrow) = source
        .strip_prefix("<table>")
        .unwrap()
        .split_once("</tr>")
        .unwrap();
    let source = format!(
        "<table>{}{wide}</tr></table>\n",
        narrow.trim_end().strip_suffix("</table>").unwrap()
    );
    let document = parse_markdown(&source);
    let lowered = safe_html::lower(&document);
    assert_eq!(lowered.blocks.len(), NARROW_ROWS + 1);
    for (index, block) in lowered.blocks[..NARROW_ROWS].iter().enumerate() {
        let Block::Paragraph(inlines) = block else {
            panic!("over-budget row remained a table")
        };
        assert_eq!(text(inlines), format!("ROW{index}"));
    }
    let Block::Paragraph(inlines) = &lowered.blocks[NARROW_ROWS] else {
        unreachable!()
    };
    assert!(text(inlines).starts_with("H0 | H1 | "));
    assert!(text(inlines).ends_with(" | H1024"));
}

fn rich_sparse_table() -> (Document, Document) {
    let source = sparse_table().replace(
        "<td>ROW512</td>",
        "<td><a href='https://example.com/sparse?x=1&amp;y=2' title='Keep link'>\
         <strong>LINKPROOF</strong></a></td><td></td>\
         <td><em>STYLEPROOF</em><br><code>CODEPROOF</code></td>\
         <td><img src='badge.png' alt='IMAGEPROOF'></td>",
    );
    let mut blocks = expected_rows();
    *blocks.last_mut().unwrap() = Block::Paragraph(vec![
        Inline::Link {
            dest: "https://example.com/sparse?x=1&y=2".into(),
            title: Some("Keep link".into()),
            content: vec![Inline::Strong(vec![plain("LINKPROOF")])],
        },
        plain(" |  | "),
        Inline::Emphasis(vec![plain("STYLEPROOF")]),
        Inline::HardBreak,
        Inline::Code("CODEPROOF".into()),
        plain(" | "),
        Inline::Image {
            dest: "badge.png".into(),
            title: None,
            alt: "IMAGEPROOF".into(),
        },
    ]);
    (parse_markdown(&source), Document { blocks })
}

#[test]
fn sparse_fallback_preserves_styles_empty_cells_images_links_and_nested_rows() {
    let (source, expected) = rich_sparse_table();
    assert_eq!(safe_html::lower(&source).as_ref(), &expected);
    let Block::HtmlBlock(raw) = &source.blocks[0] else {
        unreachable!()
    };
    let nested = parse_markdown(&format!(
        "<table><tr><th>Outer</th></tr><tr><td>BEFOREPROOF{raw}AFTERPROOF</td></tr></table>\n"
    ));
    let mut inlines = vec![plain("BEFOREPROOF")];
    for block in expected.blocks {
        let Block::Paragraph(row) = block else {
            unreachable!()
        };
        inlines.push(Inline::HardBreak);
        inlines.extend(row);
    }
    inlines.push(Inline::HardBreak);
    inlines.push(plain("AFTERPROOF"));
    let expected = Document {
        blocks: vec![Block::Table(Table {
            align: vec![Align::None],
            head: vec![vec![plain("Outer")]],
            rows: vec![vec![inlines]],
        })],
    };
    assert_eq!(safe_html::lower(&nested).as_ref(), &expected);
    assert_eq!(safe_html::lower(&expected).as_ref(), &expected);
}

#[test]
fn bounded_rows_reach_pdf_svg_and_epub_without_content_or_link_loss() {
    let (source, expected) = rich_sparse_table();
    // Fail before invoking an emitter on the old quadratic grid.
    assert_eq!(safe_html::lower(&source).as_ref(), &expected);

    let options = PdfOptions::default();
    let pdf = render_pdf_document(&source, &options).unwrap();
    assert_eq!(pdf, render_pdf_document(&expected, &options).unwrap());
    assert!(String::from_utf8_lossy(&pdf).contains("https://example.com/sparse?x=1&y=2"));

    let options = SvgOptions::default();
    let svg = render_svg_with_diagnostics(&source, &options);
    assert_eq!(svg, render_svg_with_diagnostics(&expected, &options));
    assert!(
        String::from_utf8_lossy(&svg.0).contains("href=\"https://example.com/sparse?x=1&amp;y=2\"")
    );

    let options = HtmlOptions::default();
    let epub = render_epub(&source, &options).unwrap();
    assert_eq!(epub, render_epub(&expected, &options).unwrap());
}
