//! Display mathematics owns its literal source before reference definitions are collected.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, List, ListItem, PdfOptions, parse_markdown,
    parse_markdown_profiled, parse_markdown_spanned, render_html, render_html_document, render_pdf,
    render_pdf_document,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}

fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.to_owned(),
        title: None,
        content: vec![text(label)],
    }
}

fn assert_document(source: &str, blocks: Vec<Block>) -> Document {
    let expected = Document { blocks };
    assert_eq!(parse_markdown(source), expected, "{source:?}");
    assert_eq!(parse_markdown_profiled(source).document, expected);
    assert_eq!(parse_markdown_spanned(source).to_document(), expected);
    expected
}

#[test]
fn reference_shaped_math_is_never_deleted_or_given_link_authority() {
    let source = "$$\nx + y\n\n[fake]: /hidden\n$$\n\n[fake]\n";
    let expected = assert_document(
        source,
        vec![
            Block::MathBlock("x + y\n\n[fake]: /hidden".into()),
            Block::Paragraph(vec![text("[fake]")]),
        ],
    );
    assert!(destination_spans(source).is_empty());

    let html_options = HtmlOptions::default();
    let html = render_html(source, &html_options).unwrap();
    assert_eq!(
        html,
        render_html_document(&expected, &html_options).unwrap()
    );
    assert!(!html.contains("href=\"/hidden\""));
    let browser = franken_markdown::wasm::render_html(
        source,
        &franken_markdown::wasm::WasmRenderOptions::default(),
    )
    .unwrap();
    assert!(browser.html().unwrap().contains("<p>[fake]</p>"));
    assert!(!browser.html().unwrap().contains("href=\"/hidden\""));
    let pdf_options = PdfOptions::default();
    assert_eq!(
        render_pdf(source, &pdf_options).unwrap(),
        render_pdf_document(&expected, &pdf_options).unwrap()
    );
}

#[test]
fn definitions_immediately_after_every_math_fence_form_resolve() {
    for (formula, body) in [
        ("$$ x + y $$", "x + y"),
        ("$$$$", ""),
        ("$$\nx + y\n$$", "x + y"),
        ("$$ x + y\n$$", "x + y"),
        ("$$\nx + y $$", "x + y"),
        ("   $$\nx + y\n   $$", "x + y"),
        ("\u{2003}$$ x + y $$", "x + y"),
    ] {
        let source = format!("{formula}\n[real]: /target\n\n[real]\n");
        assert_document(
            &source,
            vec![
                Block::MathBlock(body.into()),
                Block::Paragraph(vec![link("real", "/target")]),
            ],
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(spans[0].dest, "/target");
        assert_eq!(&source[spans[0].range.clone()], "/target");
    }
}

#[test]
fn math_cannot_reserve_a_reference_before_a_real_definition() {
    let source = "$$\nx\n\n[same]: /phantom\n$$\n[same]: /real\n\n[same]";
    assert_document(
        source,
        vec![
            Block::MathBlock("x\n\n[same]: /phantom".into()),
            Block::Paragraph(vec![link("same", "/real")]),
        ],
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/real");
    assert_eq!(&source[spans[0].range.clone()], "/real");
}

#[test]
fn nested_math_preserves_content_and_shared_reference_scope() {
    let body = "$$\nx + y\n\n[fake]: /hidden\n$$\n[real]: /target\n\n[fake] [real]";
    let expected_body = vec![
        Block::MathBlock("x + y\n\n[fake]: /hidden".into()),
        Block::Paragraph(vec![text("[fake] "), link("real", "/target")]),
    ];
    let quote = body
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>();
    let list = format!("- {}\n", body.replace('\n', "\n  "));
    let note = format!("[^note]: {}\n", body.replace('\n', "\n    "));
    for (container, expected) in [
        (quote, Block::BlockQuote(expected_body.clone())),
        (
            list,
            Block::List(List {
                ordered: false,
                start: 1,
                tight: false,
                items: vec![ListItem {
                    task: None,
                    blocks: expected_body.clone(),
                }],
            }),
        ),
        (
            note,
            Block::FootnoteDefinition {
                id: "note".into(),
                blocks: expected_body.clone(),
            },
        ),
    ] {
        let source = format!("{container}\nOutside [fake] [real].");
        assert_document(
            &source,
            vec![
                expected,
                Block::Paragraph(vec![
                    text("Outside [fake] "),
                    link("real", "/target"),
                    text("."),
                ]),
            ],
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(spans[0].dest, "/target");
        assert_eq!(&source[spans[0].range.clone()], "/target");
    }
}

#[test]
fn math_and_destination_spans_keep_original_unicode_crlf_coordinates() {
    let formula = "$$\nα + β\n\n[faux]: /fantôme\n$$".replace('\n', "\r\n");
    let source = format!("{formula}\r\n[real]: </café#résumé>\r\n\r\n[real]\r\n");
    assert_document(
        &source,
        vec![
            Block::MathBlock("α + β\n\n[faux]: /fantôme".into()),
            Block::Paragraph(vec![link("real", "/café#résumé")]),
        ],
    );
    let spanned = parse_markdown_spanned(&source);
    assert_eq!(
        spanned.blocks[0].span.slice(&source),
        Some(formula.as_str())
    );
    assert_eq!(spanned.blocks[1].span.slice(&source), Some("[real]"));
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/café#résumé");
    assert_eq!(&source[destinations[0].range.clone()], "</café#résumé>");
}

#[test]
fn unterminated_math_owns_its_remaining_reference_shaped_source() {
    let source = "[outside]\n\n$$\nx + y\n\n[outside]: /phantom\n";
    assert_document(
        source,
        vec![
            Block::Paragraph(vec![text("[outside]")]),
            Block::MathBlock("x + y\n\n[outside]: /phantom".into()),
        ],
    );
    assert!(destination_spans(source).is_empty());
}

#[test]
fn unclosed_nested_math_stops_at_its_container_boundary() {
    let math = Block::MathBlock("x\n\n[fake]: /hidden".into());
    for (container, expected) in [
        (
            "> $$\n> x\n>\n> [fake]: /hidden\n",
            Block::BlockQuote(vec![math.clone()]),
        ),
        (
            "- $$\n  x\n\n  [fake]: /hidden\n",
            Block::List(List {
                ordered: false,
                start: 1,
                tight: true,
                items: vec![ListItem {
                    task: None,
                    blocks: vec![math.clone()],
                }],
            }),
        ),
        (
            "[^note]: $$\n    x\n\n    [fake]: /hidden\n",
            Block::FootnoteDefinition {
                id: "note".into(),
                blocks: vec![math],
            },
        ),
    ] {
        let source = format!("{container}[real]: /target\n\n[fake] [real]");
        assert_document(
            &source,
            vec![
                expected,
                Block::Paragraph(vec![text("[fake] "), link("real", "/target")]),
            ],
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(spans[0].dest, "/target");
        assert_eq!(&source[spans[0].range.clone()], "/target");
    }
}

#[test]
fn inline_math_and_indented_code_do_not_create_display_block_boundaries() {
    for source in [
        "Before\n$$ x + y $$\n[not]: /literal\n\n[not]",
        "$$x$$ trailing\n[not]: /literal\n\n[not]",
    ] {
        let document = parse_markdown(source);
        assert!(matches!(document.blocks[0], Block::Paragraph(_)));
        assert_eq!(document.blocks[1], Block::Paragraph(vec![text("[not]")]));
        assert!(destination_spans(source).is_empty(), "{source:?}");
        let html = render_html(source, &HtmlOptions::default()).unwrap();
        assert!(html.contains("[not]: /literal"), "{source:?}");
        assert!(!html.contains("href=\"/literal\""));
    }
    let source = "    $$\n    [fake]: /literal\n    $$\n[real]: /target\n\n[fake] [real]";
    assert_document(
        source,
        vec![
            Block::CodeBlock {
                lang: None,
                code: "$$\n[fake]: /literal\n$$\n".into(),
            },
            Block::Paragraph(vec![text("[fake] "), link("real", "/target")]),
        ],
    );
}
