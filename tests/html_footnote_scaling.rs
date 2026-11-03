//! Citation-heavy documents retain first-use numbering and complete note bodies
//! when definitions are indexed instead of scanned for every reference.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{Block, Document, HtmlOptions, Inline, render_html_document};

fn reference(id: &str) -> Inline {
    Inline::FootnoteRef { id: id.to_owned() }
}

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.to_owned())])
}

fn note(id: &str, blocks: Vec<Block>) -> Block {
    Block::FootnoteDefinition {
        id: id.to_owned(),
        blocks,
    }
}

fn render(blocks: Vec<Block>) -> String {
    render_html_document(
        &Document { blocks },
        &HtmlOptions {
            custom_css: Some("body { font-family: sans-serif; }".into()),
            ..HtmlOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn thousands_of_repeated_citations_keep_every_body_and_first_use_number() {
    const COUNT: usize = 4096;
    let mut inlines = Vec::with_capacity(COUNT * 4 + 1);
    for _ in 0..2 {
        for index in (0..COUNT).rev() {
            inlines.push(reference(&format!("note-{index:06}")));
            inlines.push(Inline::Text(" ".into()));
        }
    }
    inlines.push(reference("undefined"));
    let mut blocks = vec![Block::Paragraph(inlines)];
    for index in 0..COUNT {
        blocks.push(note(
            &format!("note-{index:06}"),
            vec![paragraph(&format!("Definition {index:06}."))],
        ));
    }
    let html = render(blocks);
    assert_eq!(html.matches("class=\"footnote-ref\"").count(), 2 * COUNT);
    assert_eq!(html.matches("id=\"fnref-").count(), COUNT);
    assert_eq!(html.matches("class=\"footnote-backref\"").count(), COUNT);
    assert!(html.contains("[^undefined]"));
    let entries: Vec<_> = html.split("<li id=\"fn-").skip(1).collect();
    assert_eq!(entries.len(), COUNT);
    for (number, entry) in entries.into_iter().enumerate() {
        let index = COUNT - number - 1;
        let expected = format!("note-{index:06}\"><p>Definition {index:06}.</p>");
        assert!(
            entry.starts_with(&expected),
            "incorrect note at number {}",
            number + 1
        );
        let own = entry.split_once("</li>").unwrap().0;
        assert!(own.contains(&format!("href=\"#fnref-{}\"", number + 1)));
    }
    assert!(html.contains("id=\"fnref-1\"><a href=\"#fn-note-004095\">1</a>"));
    assert!(html.contains("id=\"fnref-4096\"><a href=\"#fn-note-000000\">4096</a>"));
}

#[test]
fn first_definition_nested_definitions_and_citation_cycles_keep_original_ownership() {
    let html = render(vec![
        Block::Paragraph(vec![reference("b"), reference("a"), reference("b")]),
        note("a", vec![paragraph("First A.")]),
        note(
            "b",
            vec![Block::Paragraph(vec![
                Inline::Text("Body B cites ".into()),
                reference("a"),
                reference("nested"),
            ])],
        ),
        note(
            "a",
            vec![
                paragraph("Shadowed A must not render."),
                note(
                    "nested",
                    vec![Block::Paragraph(vec![
                        Inline::Text("Nested body cites ".into()),
                        reference("b"),
                    ])],
                ),
            ],
        ),
        note(
            "unused",
            vec![paragraph("Unreferenced body must not render.")],
        ),
    ]);
    assert_eq!(html.matches("<li id=\"fn-").count(), 3);
    assert_eq!(html.matches("First A.").count(), 1);
    assert_eq!(html.matches("Body B cites ").count(), 1);
    assert_eq!(html.matches("Nested body cites ").count(), 1);
    assert!(!html.contains("Shadowed A"));
    assert!(!html.contains("Unreferenced body"));
    let b = html.find("<li id=\"fn-b\"").unwrap();
    let a = html.find("<li id=\"fn-a\"").unwrap();
    let nested = html.find("<li id=\"fn-nested\"").unwrap();
    assert!(
        b < a && a < nested,
        "definitions follow first references, including note-to-note references"
    );
    for number in 1..=3 {
        assert_eq!(html.matches(&format!("id=\"fnref-{number}\"")).count(), 1);
    }
}

#[test]
fn definition_lookup_preserves_exact_ids_escaping_and_undefined_literals() {
    let html = render(vec![
        Block::Paragraph(vec![
            reference("id<&\""),
            reference("Case"),
            reference("case"),
            reference("<img src=x>"),
        ]),
        note("id<&\"", vec![paragraph("Escaped identifier body.")]),
        note("Case", vec![paragraph("Exact case body.")]),
    ]);
    assert!(html.contains("id=\"fn-id&lt;&amp;&quot;\""));
    assert!(html.contains("href=\"#fn-id&lt;&amp;&quot;\""));
    assert!(html.contains("id=\"fn-Case\""));
    assert!(html.contains("[^case]"));
    assert!(html.contains("[^&lt;img src=x&gt;]"));
    assert!(!html.contains("<img src=x>"));
    assert_eq!(html.matches("<li id=\"fn-").count(), 2);
}

#[test]
fn reference_free_documents_do_not_emit_definitions() {
    let mut blocks = vec![paragraph("Body without citations.")];
    for index in 0..2048 {
        blocks.push(note(
            &format!("unused-{index}"),
            vec![paragraph("Hidden note.")],
        ));
    }
    let html = render(blocks);
    assert!(html.contains("<p>Body without citations.</p>"));
    assert!(!html.contains("Hidden note."));
    assert!(!html.contains("<section class=\"footnotes\">"));
}
