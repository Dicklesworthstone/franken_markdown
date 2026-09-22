//! Endnote preservation through the public AST and SVG APIs, not a second
//! implementation of footnote collection or numbering.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::borrow::Cow;
use franken_markdown::ast::{Align, Block, DefinitionItem, Document, Inline, List, ListItem, Table};
use franken_markdown::svg::{SvgOptions, render_svg, render_svg_with_diagnostics, render_svg_with_resources};
use franken_markdown::{FontAssets, PdfImageAsset, PdfOptions, parse_markdown, render_pdf_document};

fn text(value: &str) -> Inline { Inline::Text(value.to_owned()) }
fn reference(id: &str) -> Inline { Inline::FootnoteRef { id: id.to_owned() } }
fn paragraph(value: &str) -> Block { Block::Paragraph(vec![text(value)]) }
fn definition(id: &str, blocks: Vec<Block>) -> Block {
    Block::FootnoteDefinition { id: id.to_owned(), blocks }
}
fn notes_heading() -> Block { Block::Heading { level: 2, inlines: vec![text("Notes")] } }
fn doc(blocks: Vec<Block>) -> Document { Document { blocks } }

fn assert_svg_equivalent(source: &Document, expected: &Document) {
    let options = SvgOptions::default();
    assert_eq!(source.with_endnotes().as_ref(), expected);
    let actual = render_svg_with_diagnostics(source, &options);
    assert_eq!(actual, render_svg_with_diagnostics(expected, &options));
    assert!(actual.1.glyphs_drawn > 0);
    assert_eq!(render_svg(source, &options), actual.0);
}

#[test]
fn ordinary_documents_and_prepared_notes_are_borrowed() {
    let source = parse_markdown("# Plain\n\nLiteral `[^not-a-note]` and normal prose.");
    let Cow::Borrowed(borrowed) = source.with_endnotes() else { panic!("unexpected AST clone") };
    assert!(std::ptr::eq(borrowed, &source));
    let source = doc(vec![Block::Paragraph(vec![reference("a")]), definition("a", vec![paragraph("Note")])]);
    let prepared = source.with_endnotes();
    assert!(matches!(prepared, Cow::Owned(_)));
    assert!(matches!(prepared.with_endnotes(), Cow::Borrowed(_)));
    assert_eq!(render_svg(&source, &Default::default()), render_svg(&prepared, &Default::default()));
}

#[test]
fn svg_resolves_forward_references_in_first_use_order() {
    let source = doc(vec![
        definition("a", vec![paragraph("Alpha")]),
        Block::Paragraph(vec![text("Body"), reference("b"), reference("a"), reference("b")]),
        definition("b", vec![paragraph("Beta")]),
    ]);
    let expected = doc(vec![
        Block::Paragraph(vec![text("Body"), text("[1]"), text("[2]"), text("[1]")]),
        notes_heading(),
        Block::Paragraph(vec![text("[1] "), text("Beta")]),
        Block::Paragraph(vec![text("[2] "), text("Alpha")]),
    ]);
    assert_svg_equivalent(&source, &expected);
}

#[test]
fn rich_notes_retain_code_lists_tables_math_and_styles_in_order() {
    let rich = vec![
        Block::CodeBlock { lang: Some("rust".into()), code: "let evidence = 42;".into() },
        Block::BlockQuote(vec![paragraph("Quoted evidence")]),
        Block::List(List { ordered: true, start: 3, tight: true, items: vec![ListItem {
            task: Some(true), blocks: vec![paragraph("List evidence")],
        }] }),
        Block::Table(Table { align: vec![Align::Right], head: vec![vec![text("Value")]],
            rows: vec![vec![vec![text("42")]]] }),
        Block::MathBlock(r"\frac{1}{2}".into()),
        Block::DefinitionList(vec![DefinitionItem { terms: vec![vec![text("Term")]],
            definitions: vec![vec![text("Meaning")]] }]),
        Block::Paragraph(vec![Inline::Strong(vec![text("Bold evidence")]),
            text(" "), Inline::Emphasis(vec![text("italic evidence")])]),
        Block::HtmlBlock("<aside>Inert evidence</aside>".into()),
        Block::Heading { level: 3, inlines: vec![text("Appendix")] },
        Block::ThematicBreak,
        paragraph("Last evidence"),
    ];
    let source = doc(vec![Block::Paragraph(vec![reference("rich")]), definition("rich", rich.clone())]);
    let mut expected = vec![paragraph("[1]"), notes_heading(), paragraph("[1]")];
    expected.extend(rich);
    assert_svg_equivalent(&source, &doc(expected));
}

#[test]
fn nested_definitions_and_reference_cycles_are_emitted_once() {
    let source = doc(vec![
        Block::Paragraph(vec![reference("a")]),
        definition("a", vec![Block::Paragraph(vec![text("A"), reference("b")]),
            definition("b", vec![Block::Paragraph(vec![text("B"), reference("a")])])]),
    ]);
    assert_svg_equivalent(&source, &doc(vec![
        paragraph("[1]"), notes_heading(),
        Block::Paragraph(vec![text("[1] "), text("A"), text("[2]")]),
        Block::Paragraph(vec![text("[2] "), text("B"), text("[1]")]),
    ]));
}

#[test]
fn notes_in_list_and_quote_containers_are_not_lost() {
    let source = doc(vec![Block::BlockQuote(vec![Block::List(List {
        ordered: false, start: 1, tight: true, items: vec![ListItem { task: None, blocks: vec![
            Block::Paragraph(vec![reference("a")]), definition("a", vec![paragraph("Nested note")]),
        ] }],
    })])]);
    let expected = doc(vec![Block::BlockQuote(vec![Block::List(List {
        ordered: false, start: 1, tight: true, items: vec![ListItem { task: None, blocks: vec![paragraph("[1]")] }],
    })]), notes_heading(), Block::Paragraph(vec![text("[1] "), text("Nested note")])]);
    assert_svg_equivalent(&source, &expected);
}

#[test]
fn undefined_duplicate_and_unreferenced_notes_have_explicit_policy() {
    let source = doc(vec![Block::Paragraph(vec![reference("missing")]),
        definition("a", vec![paragraph("First wins")]),
        definition("a", vec![paragraph("Duplicate omitted")]),
        definition("unused", vec![]),
    ]);
    assert_svg_equivalent(&source, &doc(vec![paragraph("[^missing]"), notes_heading(),
        Block::Paragraph(vec![text("[1] "), text("First wins")]), paragraph("[2]"),
    ]));
}

#[test]
fn note_images_use_resources_and_report_failures_once() {
    let image = Inline::Image { dest: "plot.svg".into(), title: None, alt: "Evidence plot".into() };
    let source = doc(vec![Block::Paragraph(vec![reference("plot"), reference("plot")]),
        definition("plot", vec![Block::Paragraph(vec![image.clone()])])]);
    let expected = doc(vec![Block::Paragraph(vec![text("[1]"), text("[1]")]), notes_heading(),
        Block::Paragraph(vec![text("[1] "), image])]);
    let options = SvgOptions::default();
    let fonts = FontAssets::default();
    let asset = PdfImageAsset::new("plot.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec());
    let actual = render_svg_with_resources(&source, &options, &fonts, std::slice::from_ref(&asset)).unwrap();
    assert_eq!(actual, render_svg_with_resources(&expected, &options, &fonts, &[asset]).unwrap());
    let xml = String::from_utf8(actual.0).unwrap();
    assert_eq!(xml.matches("aria-label=\"Evidence plot\"").count(), 1);
    assert!(actual.2.is_empty());
    let (_, _, warnings) = render_svg_with_diagnostics(&source, &options);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "svg_image_missing");
}

#[test]
fn parsed_markdown_and_explicit_preparation_match_all_svg_entry_points() {
    let source = parse_markdown("Evidence[^x].\n\n[^x]: A **complete** note.\n\n    A second paragraph.\n");
    let before = source.clone();
    let prepared = source.with_endnotes();
    assert_ne!(prepared.as_ref(), &source);
    let options = SvgOptions::default();
    let expected = render_svg_with_diagnostics(&prepared, &options);
    assert_eq!(render_svg_with_diagnostics(&source, &options), expected);
    assert_eq!(render_svg_with_resources(&source, &options, &FontAssets::default(), &[]).unwrap(), expected);
    assert_eq!(source, before);
}

#[test]
fn public_endnote_preparation_matches_pdf_without_changing_the_source() {
    let source = parse_markdown("# Evidence\n\nCitations[^b] and[^a].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n");
    let before = source.clone();
    let options = PdfOptions::default();
    let expected = render_pdf_document(&source, &options).unwrap();
    let prepared = source.with_endnotes();
    assert_eq!(render_pdf_document(&prepared, &options).unwrap(), expected);
    assert_eq!(source, before);
}
