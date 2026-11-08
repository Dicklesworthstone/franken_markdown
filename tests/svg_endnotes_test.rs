//! Endnote preservation through the public AST and SVG APIs, not a second
//! implementation of footnote collection or numbering.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{
    Align, Block, DefinitionItem, Document, Inline, List, ListItem, Table,
};
use franken_markdown::svg::{
    SvgOptions, SvgReport, SvgWarning, render_svg, render_svg_with_diagnostics,
    render_svg_with_resources,
};
use franken_markdown::{
    FontAssets, PdfImageAsset, PdfOptions, parse_markdown, render_pdf_document,
};
use std::borrow::Cow;
use std::collections::BTreeSet;

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}
fn reference(id: &str) -> Inline {
    Inline::FootnoteRef { id: id.to_owned() }
}
fn paragraph(value: &str) -> Block {
    Block::Paragraph(vec![text(value)])
}
fn definition(id: &str, blocks: Vec<Block>) -> Block {
    Block::FootnoteDefinition {
        id: id.to_owned(),
        blocks,
    }
}
fn notes_heading() -> Block {
    Block::Heading {
        level: 2,
        inlines: vec![text("Notes")],
    }
}
fn doc(blocks: Vec<Block>) -> Document {
    Document { blocks }
}

fn note_identifier(id: &str) -> bool {
    id.strip_prefix("fmd-note-ref-")
        .or_else(|| id.strip_prefix("fmd-note-"))
        .is_some_and(|number| {
            !number.is_empty()
                && !number.starts_with('0')
                && number.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn new_note_navigation(line: &str) -> bool {
    let value = line
        .strip_prefix("<view id=\"")
        .or_else(|| line.strip_prefix("<a href=\"#"));
    value
        .and_then(|value| value.split_once('"'))
        .is_some_and(|(id, _)| note_identifier(id))
}

fn assert_unchanged_paint(actual: &[u8], legacy: &[u8]) {
    let actual = std::str::from_utf8(actual).unwrap();
    let legacy = std::str::from_utf8(legacy).unwrap();
    // Remove only the precisely named additive note views/hit regions. Every
    // external/heading link, glyph, image, formula and paint attribute remains.
    let actual: Vec<_> = actual
        .lines()
        .filter(|line| !new_note_navigation(line))
        .collect();
    let expected: Vec<_> = legacy.lines().collect();
    let mismatch = actual.iter().zip(&expected).position(|(a, b)| a != b);
    assert!(
        mismatch.is_none() && actual.len() == expected.len(),
        "paint/existing-link mismatch at line {:?}; actual {} lines, expected {} lines",
        mismatch,
        actual.len(),
        expected.len()
    );
}

fn assert_added_note_links(bytes: &[u8], numbers: &[usize]) {
    let svg = std::str::from_utf8(bytes).unwrap();
    let mut notes = BTreeSet::new();
    let mut returns = BTreeSet::new();
    for line in svg
        .lines()
        .filter(|line| line.starts_with("<a href=\"#fmd-note-"))
    {
        let (id, _) = line
            .strip_prefix("<a href=\"#")
            .unwrap()
            .split_once('"')
            .unwrap();
        let (set, label, number) = if let Some(number) = id.strip_prefix("fmd-note-ref-") {
            (
                &mut returns,
                format!("Return to footnote {number} citation"),
                number,
            )
        } else {
            let number = id.strip_prefix("fmd-note-").unwrap();
            (&mut notes, format!("Footnote {number}"), number)
        };
        set.insert(number.parse::<usize>().unwrap());
        assert!(line.contains(&format!("aria-label=\"{label}\"")));
        assert_eq!(svg.matches(&format!("<view id=\"{id}\"")).count(), 1);
    }
    let expected: BTreeSet<_> = numbers.iter().copied().collect();
    assert_eq!(notes, expected);
    assert_eq!(returns, expected);
    assert_eq!(
        svg.lines()
            .filter(|line| line.starts_with("<view id=\"fmd-note-"))
            .count(),
        numbers.len() * 2
    );
}

type SvgResult = (Vec<u8>, SvgReport, Vec<SvgWarning>);

fn assert_render_equivalent(actual: &SvgResult, legacy: &SvgResult, numbers: &[usize]) {
    assert_unchanged_paint(&actual.0, &legacy.0);
    assert_added_note_links(&actual.0, numbers);
    assert_eq!(actual.1, legacy.1);
    assert_eq!(actual.2, legacy.2);
}

fn assert_svg_equivalent(source: &Document, expected: &Document, numbers: &[usize]) {
    let options = SvgOptions::default();
    assert_eq!(source.with_endnotes().as_ref(), expected);
    let actual = render_svg_with_diagnostics(source, &options);
    assert_render_equivalent(
        &actual,
        &render_svg_with_diagnostics(expected, &options),
        numbers,
    );
    assert!(actual.1.glyphs_drawn > 0);
    assert_eq!(render_svg(source, &options), actual.0);
}

#[test]
fn ordinary_documents_and_prepared_notes_are_borrowed() {
    let source = parse_markdown("# Plain\n\nLiteral `[^not-a-note]` and normal prose.");
    let Cow::Borrowed(borrowed) = source.with_endnotes() else {
        panic!("unexpected AST clone")
    };
    assert!(std::ptr::eq(borrowed, &source));
    let source = doc(vec![
        Block::Paragraph(vec![reference("a")]),
        definition("a", vec![paragraph("Note")]),
    ]);
    let prepared = source.with_endnotes();
    assert!(matches!(prepared, Cow::Owned(_)));
    assert!(matches!(prepared.with_endnotes(), Cow::Borrowed(_)));
    let actual = render_svg(&source, &Default::default());
    assert_unchanged_paint(&actual, &render_svg(&prepared, &Default::default()));
    assert_added_note_links(&actual, &[1]);
}

#[test]
fn svg_resolves_forward_references_in_first_use_order() {
    let source = doc(vec![
        definition("a", vec![paragraph("Alpha")]),
        Block::Paragraph(vec![
            text("Body"),
            reference("b"),
            reference("a"),
            reference("b"),
        ]),
        definition("b", vec![paragraph("Beta")]),
    ]);
    let expected = doc(vec![
        Block::Paragraph(vec![text("Body"), text("¹"), text("²"), text("¹")]),
        notes_heading(),
        Block::Paragraph(vec![text("¹ "), text("Beta")]),
        Block::Paragraph(vec![text("² "), text("Alpha")]),
    ]);
    assert_svg_equivalent(&source, &expected, &[1, 2]);
}

#[test]
fn rich_notes_retain_code_lists_tables_math_and_styles_in_order() {
    let rich = vec![
        Block::CodeBlock {
            lang: Some("rust".into()),
            code: "let evidence = 42;".into(),
        },
        Block::BlockQuote(vec![paragraph("Quoted evidence")]),
        Block::List(List {
            ordered: true,
            start: 3,
            tight: true,
            items: vec![ListItem {
                task: Some(true),
                blocks: vec![paragraph("List evidence")],
            }],
        }),
        Block::Table(Table {
            align: vec![Align::Right],
            head: vec![vec![text("Value")]],
            rows: vec![vec![vec![text("42")]]],
        }),
        Block::MathBlock(r"\frac{1}{2}".into()),
        Block::DefinitionList(vec![DefinitionItem {
            terms: vec![vec![text("Term")]],
            definitions: vec![vec![text("Meaning")]],
        }]),
        Block::Paragraph(vec![
            Inline::Strong(vec![text("Bold evidence")]),
            text(" "),
            Inline::Emphasis(vec![text("italic evidence")]),
        ]),
        Block::HtmlBlock("<aside>Inert evidence</aside>".into()),
        Block::Heading {
            level: 3,
            inlines: vec![text("Appendix")],
        },
        Block::ThematicBreak,
        paragraph("Last evidence"),
    ];
    let source = doc(vec![
        Block::Paragraph(vec![reference("rich")]),
        definition("rich", rich.clone()),
    ]);
    let mut expected = vec![paragraph("¹"), notes_heading(), paragraph("¹")];
    // Paged output lowers safe raw HTML: the `<aside>` keeps its text.
    expected.extend(rich.into_iter().map(|block| match block {
        Block::HtmlBlock(_) => paragraph("Inert evidence"),
        other => other,
    }));
    assert_svg_equivalent(&source, &doc(expected), &[1]);
}

#[test]
fn nested_definitions_and_reference_cycles_are_emitted_once() {
    let source = doc(vec![
        Block::Paragraph(vec![reference("a")]),
        definition(
            "a",
            vec![
                Block::Paragraph(vec![text("A"), reference("b")]),
                definition("b", vec![Block::Paragraph(vec![text("B"), reference("a")])]),
            ],
        ),
    ]);
    assert_svg_equivalent(
        &source,
        &doc(vec![
            paragraph("¹"),
            notes_heading(),
            Block::Paragraph(vec![text("¹ "), text("A"), text("²")]),
            Block::Paragraph(vec![text("² "), text("B"), text("¹")]),
        ]),
        &[1, 2],
    );
}

#[test]
fn notes_in_list_and_quote_containers_are_not_lost() {
    let source = doc(vec![Block::BlockQuote(vec![Block::List(List {
        ordered: false,
        start: 1,
        tight: true,
        items: vec![ListItem {
            task: None,
            blocks: vec![
                Block::Paragraph(vec![reference("a")]),
                definition("a", vec![paragraph("Nested note")]),
            ],
        }],
    })])]);
    let expected = doc(vec![
        Block::BlockQuote(vec![Block::List(List {
            ordered: false,
            start: 1,
            tight: true,
            items: vec![ListItem {
                task: None,
                blocks: vec![paragraph("¹")],
            }],
        })]),
        notes_heading(),
        Block::Paragraph(vec![text("¹ "), text("Nested note")]),
    ]);
    assert_svg_equivalent(&source, &expected, &[1]);
}

#[test]
fn undefined_duplicate_and_unreferenced_notes_have_explicit_policy() {
    let source = doc(vec![
        Block::Paragraph(vec![reference("missing")]),
        definition("a", vec![paragraph("First wins")]),
        definition("a", vec![paragraph("Duplicate omitted")]),
        definition("unused", vec![]),
    ]);
    assert_svg_equivalent(
        &source,
        &doc(vec![
            paragraph("[^missing]"),
            notes_heading(),
            Block::Paragraph(vec![text("¹ "), text("First wins")]),
            paragraph("²"),
        ]),
        &[],
    );
}

#[test]
fn note_images_use_resources_and_report_failures_once() {
    let image = Inline::Image {
        dest: "plot.svg".into(),
        title: None,
        alt: "Evidence plot".into(),
    };
    let source = doc(vec![
        Block::Paragraph(vec![reference("plot"), reference("plot")]),
        definition("plot", vec![Block::Paragraph(vec![image.clone()])]),
    ]);
    let expected = doc(vec![
        Block::Paragraph(vec![text("¹"), text("¹")]),
        notes_heading(),
        paragraph("¹"),
        Block::Paragraph(vec![image]),
    ]);
    // Keep the generated label in its own paragraph so shared endnote
    // preparation preserves the original image-only block.
    assert_eq!(source.with_endnotes().as_ref(), &expected);
    let options = SvgOptions::default();
    let fonts = FontAssets::default();
    let asset = PdfImageAsset::new("plot.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>"#.to_vec());
    let actual =
        render_svg_with_resources(&source, &options, &fonts, std::slice::from_ref(&asset)).unwrap();
    assert_render_equivalent(
        &actual,
        &render_svg_with_resources(&expected, &options, &fonts, &[asset]).unwrap(),
        &[1],
    );
    let xml = String::from_utf8(actual.0).unwrap();
    assert_eq!(xml.matches("aria-label=\"Evidence plot\"").count(), 1);
    assert!(actual.2.is_empty());
    let (_, _, warnings) = render_svg_with_diagnostics(&source, &options);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].code, "svg_image_missing");
}

#[test]
fn parsed_markdown_and_explicit_preparation_match_all_svg_entry_points() {
    let source =
        parse_markdown("Evidence[^x].\n\n[^x]: A **complete** note.\n\n    A second paragraph.\n");
    let before = source.clone();
    let prepared = source.with_endnotes();
    assert_ne!(prepared.as_ref(), &source);
    let options = SvgOptions::default();
    let expected = render_svg_with_diagnostics(&prepared, &options);
    let actual = render_svg_with_diagnostics(&source, &options);
    assert_render_equivalent(&actual, &expected, &[1]);
    assert_eq!(
        render_svg_with_resources(&source, &options, &FontAssets::default(), &[]).unwrap(),
        actual
    );
    assert_eq!(source, before);
}

#[test]
fn public_endnote_preparation_opts_pdf_into_endnotes_without_changing_the_source() {
    let source =
        parse_markdown("# Evidence\n\nCitations[^b] and[^a].\n\n[^a]: Alpha.\n\n[^b]: Beta.\n");
    let before = source.clone();
    let options = PdfOptions::default();
    // The PDF places notes at the page foot by default; a host that prepares
    // endnotes gets the trailing Notes section the SVG renderer also draws.
    let page_notes = render_pdf_document(&source, &options).unwrap();
    let prepared = source.with_endnotes();
    let endnotes = render_pdf_document(&prepared, &options).unwrap();
    assert_ne!(endnotes, page_notes);
    assert_eq!(render_pdf_document(&prepared, &options).unwrap(), endnotes);
    let outline = |pdf: &[u8]| {
        String::from_utf8_lossy(pdf)
            .matches("/Title (Notes)")
            .count()
    };
    assert_eq!(
        outline(&endnotes),
        1,
        "endnotes keep their bookmarked heading"
    );
    assert_eq!(outline(&page_notes), 0);
    assert_eq!(source, before);
}
