//! Supported HTML blocks inside table cells must survive the shared lowering
//! used by PDF and SVG. Expected native documents are written independently.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::{
    Align, Block, Document, HtmlOptions, Inline, PdfOptions, Table, parse_markdown,
    render_html_document, render_pdf_document, safe_html, verify,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.to_string())
}

fn code(value: &str) -> Inline {
    Inline::Code(value.to_string())
}

fn cell_document(inlines: Vec<Inline>) -> Document {
    Document {
        blocks: vec![Block::Table(Table {
            align: vec![Align::None],
            head: vec![vec![text("Outer")]],
            rows: vec![vec![inlines]],
        })],
    }
}

fn source_cell(content: &str) -> Document {
    parse_markdown(&format!(
        "<table><tr><th>Outer</th></tr><tr><td>{content}</td></tr></table>\n"
    ))
}

fn assert_cell(content: &str, inlines: Vec<Inline>) {
    let source = source_cell(content);
    let original = source.clone();
    let expected = cell_document(inlines);
    assert_eq!(safe_html::lower(&source).as_ref(), &expected);
    assert_eq!(safe_html::lower(&expected).as_ref(), &expected);
    assert_eq!(source, original);
}

#[test]
fn nested_tables_keep_every_row_cell_and_inline_link_in_source_order() {
    assert_cell(
        "Before<table><tr><th>Inner heading</th><th>Target</th></tr>\
         <tr><td>Inner body</td><td><a href='https://example.com/a?b=1&amp;c=2' \
         title='Read more'><strong>Open docs</strong></a></td></tr></table>After",
        vec![
            text("Before"),
            Inline::HardBreak,
            text("Inner heading | Target"),
            Inline::HardBreak,
            text("Inner body | "),
            Inline::Link {
                dest: "https://example.com/a?b=1&c=2".to_string(),
                title: Some("Read more".to_string()),
                content: vec![Inline::Strong(vec![text("Open docs")])],
            },
            Inline::HardBreak,
            text("After"),
        ],
    );
}

#[test]
fn cell_lists_keep_ordered_and_nested_markers_images_and_styles() {
    assert_cell(
        "<ol start='4'><li>First item</li><li><em>Second item</em>\
         <ul><li><img src='badge.png' alt='Badge' title='Status'></li>\
         <li><code>--flag  value</code></li></ul>Continuation</li></ol>",
        vec![
            text("4. First item"),
            Inline::HardBreak,
            text("5. "),
            Inline::Emphasis(vec![text("Second item")]),
            Inline::HardBreak,
            text("\u{2022} "),
            Inline::Image {
                dest: "badge.png".to_string(),
                title: Some("Status".to_string()),
                alt: "Badge".to_string(),
            },
            Inline::HardBreak,
            text("\u{2022} "),
            // Inline HTML code inherits ordinary HTML whitespace collapsing;
            // the preformatted-cell case below separately requires spaces.
            code("--flag value"),
            Inline::HardBreak,
            text("Continuation"),
        ],
    );
}

#[test]
fn preformatted_cells_preserve_indentation_blank_lines_and_entities() {
    // Numeric newlines keep this one CommonMark HTML block while exercising
    // actual blank lines in the preformatted HTML content.
    assert_cell(
        "<p>Before code</p><pre>\n  alpha  beta&#10;&#10;\tλ &lt; 3  &#10;</pre>\
         <p>After code</p>",
        vec![
            text("Before code"),
            Inline::HardBreak,
            code("  alpha  beta"),
            Inline::HardBreak,
            code(""),
            Inline::HardBreak,
            code("\tλ < 3  "),
            Inline::HardBreak,
            text("After code"),
        ],
    );
}

#[test]
fn paragraphs_quotes_and_disclosure_summaries_keep_their_boundaries() {
    assert_cell(
        "<p>First paragraph</p><blockquote><p><em>Quoted paragraph</em></p>\
         <p>Second quotation</p></blockquote><details><summary>Details heading</summary>\
         <p>Details body</p></details><hr><p>Last paragraph</p>",
        vec![
            text("First paragraph"),
            Inline::HardBreak,
            Inline::Emphasis(vec![text("Quoted paragraph")]),
            Inline::HardBreak,
            text("Second quotation"),
            Inline::HardBreak,
            Inline::Strong(vec![text("Details heading")]),
            Inline::HardBreak,
            text("Details body"),
            Inline::HardBreak,
            text("\u{2014}"),
            Inline::HardBreak,
            text("Last paragraph"),
        ],
    );
}

#[test]
fn empty_nested_cells_keep_column_separation_and_unclosed_cells_keep_text() {
    assert_cell(
        "<table><tr><th>A</th><th></th><th>C</th></tr>\
         <tr><td></td><td>Middle</td><td></td></tr></table>",
        vec![text("A |  | C"), Inline::HardBreak, text(" | Middle | ")],
    );
    let source = parse_markdown(
        "<table><tr><th>Outer</th></tr><tr><td>Before\
         <table><tr><th>Nested header</th></tr><tr><td>Nested body",
    );
    assert_eq!(
        safe_html::lower(&source).as_ref(),
        &cell_document(vec![
            text("Before"),
            Inline::HardBreak,
            text("Nested header"),
            Inline::HardBreak,
            text("Nested body"),
        ])
    );
}

#[test]
fn nested_unsafe_html_keeps_visible_source_and_does_not_gain_active_markup() {
    let source = source_cell(
        "<ul><li><script>alert(1)</script></li>\
         <li><a href='javascript:alert(2)'>Unsafe link text</a></li></ul>",
    );
    let lowered = safe_html::lower(&source);
    assert_eq!(safe_html::lower(&lowered).as_ref(), lowered.as_ref());
    for document in [&source, lowered.as_ref()] {
        let html = render_html_document(document, &HtmlOptions::default()).unwrap();
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("Unsafe link text"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("href=\"javascript:"));
    }
}

fn rich_cell() -> (Document, Document) {
    let source = source_cell(
        "<p>BEFOREPROOF</p><table><tr><th>NESTEDHEADER</th><th>TARGETPROOF</th></tr>\
         <tr><td>NESTEDBODY</td><td><a href='https://example.com/cell'>\
         <strong>LINKPROOF</strong></a></td></tr></table>\
         <ol start='4'><li>FIRSTPROOF</li><li><em>SECONDPROOF</em>\
         <ul><li><img src='badge.png' alt='BADGEPROOF'></li></ul></li></ol>\
         <pre>  CODEFIRST = 1;&#10;  CODESECOND = 2;</pre><p>AFTERPROOF</p>",
    );
    let expected = cell_document(vec![
        text("BEFOREPROOF"),
        Inline::HardBreak,
        text("NESTEDHEADER | TARGETPROOF"),
        Inline::HardBreak,
        text("NESTEDBODY | "),
        Inline::Link {
            dest: "https://example.com/cell".to_string(),
            title: None,
            content: vec![Inline::Strong(vec![text("LINKPROOF")])],
        },
        Inline::HardBreak,
        text("4. FIRSTPROOF"),
        Inline::HardBreak,
        text("5. "),
        Inline::Emphasis(vec![text("SECONDPROOF")]),
        Inline::HardBreak,
        text("\u{2022} "),
        Inline::Image {
            dest: "badge.png".to_string(),
            title: None,
            alt: "BADGEPROOF".to_string(),
        },
        Inline::HardBreak,
        code("  CODEFIRST = 1;"),
        Inline::HardBreak,
        code("  CODESECOND = 2;"),
        Inline::HardBreak,
        text("AFTERPROOF"),
    ]);
    (source, expected)
}

#[test]
fn pdf_retains_nested_cell_content_and_active_safe_links() {
    let (source, expected) = rich_cell();
    let options = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    let actual = render_pdf_document(&source, &options).unwrap();
    assert_eq!(actual, render_pdf_document(&expected, &options).unwrap());
    assert!(String::from_utf8_lossy(&actual).contains("https://example.com/cell"));
    let report = verify::verify_pdf(&source, &options).unwrap();
    let text = report
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for marker in [
        "BEFOREPROOF",
        "NESTEDHEADER",
        "TARGETPROOF",
        "NESTEDBODY",
        "LINKPROOF",
        "FIRSTPROOF",
        "SECONDPROOF",
        "BADGEPROOF",
        "CODEFIRST",
        "CODESECOND",
        "AFTERPROOF",
    ] {
        assert!(text.contains(marker), "missing {marker}: {text}");
    }
}

#[test]
fn svg_retains_the_same_nested_content_and_link_regions() {
    let (source, expected) = rich_cell();
    let options = SvgOptions::default();
    let actual = render_svg_with_diagnostics(&source, &options);
    assert_eq!(actual, render_svg_with_diagnostics(&expected, &options));
    assert!(actual.1.glyphs_drawn > 0);
    assert!(String::from_utf8_lossy(&actual.0).contains("href=\"https://example.com/cell\""));
}

#[test]
fn bounded_nested_tables_retain_their_text_once() {
    let source = format!(
        "<table><tr><th>Outer</th></tr><tr><td>{}DEEPPROOF{}TAILPROOF</td></tr></table>\n",
        "<table><tr><td>".repeat(100),
        "</td></tr></table>".repeat(100),
    );
    let document = parse_markdown(&source);
    let lowered = safe_html::lower(&document);
    assert_eq!(safe_html::lower(&lowered).as_ref(), lowered.as_ref());
    let html = render_html_document(&lowered, &HtmlOptions::default()).unwrap();
    assert_eq!(html.matches("DEEPPROOF").count(), 1);
    assert_eq!(html.matches("TAILPROOF").count(), 1);
}
