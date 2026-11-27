#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfOptions, Profile,
    ast::{Block, Inline},
    parse_markdown, render_html_document, render_pdf_document,
};

#[test]
fn test_parse_simple_definition_list() {
    let md = "\
Term 1
: Definition 1
";
    let doc = parse_markdown(md);
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::DefinitionList(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].terms.len(), 1);
            assert_eq!(items[0].terms[0], vec![Inline::Text("Term 1".to_string())]);
            assert_eq!(items[0].definitions.len(), 1);
            assert_eq!(
                items[0].definitions[0],
                vec![Inline::Text("Definition 1".to_string())]
            );
        }
        other => panic!("Expected DefinitionList, got {other:?}"),
    }
}

#[test]
fn test_parse_multi_term_multi_def() {
    let md = "\
Rust
C++
: Systems programming language
: Statically typed language

Python
: Dynamically typed language
";
    let doc = parse_markdown(md);
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::DefinitionList(items) => {
            assert_eq!(items.len(), 2);
            // Item 1
            assert_eq!(items[0].terms.len(), 2);
            assert_eq!(items[0].terms[0], vec![Inline::Text("Rust".to_string())]);
            assert_eq!(items[0].terms[1], vec![Inline::Text("C++".to_string())]);
            assert_eq!(items[0].definitions.len(), 2);
            assert_eq!(
                items[0].definitions[0],
                vec![Inline::Text("Systems programming language".to_string())]
            );
            assert_eq!(
                items[0].definitions[1],
                vec![Inline::Text("Statically typed language".to_string())]
            );
            // Item 2
            assert_eq!(items[1].terms.len(), 1);
            assert_eq!(items[1].terms[0], vec![Inline::Text("Python".to_string())]);
            assert_eq!(items[1].definitions.len(), 1);
            assert_eq!(
                items[1].definitions[0],
                vec![Inline::Text("Dynamically typed language".to_string())]
            );
        }
        other => panic!("Expected DefinitionList, got {other:?}"),
    }
}

#[test]
fn test_definition_list_continuation_lines() {
    let md = "\
Markdown
: A lightweight markup language with plain-text
  formatting syntax designed so that it can be converted
  to HTML.
";
    let doc = parse_markdown(md);
    assert_eq!(doc.blocks.len(), 1);
    match &doc.blocks[0] {
        Block::DefinitionList(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].terms[0],
                vec![Inline::Text("Markdown".to_string())]
            );
            assert_eq!(
                items[0].definitions[0],
                vec![Inline::Text(
                    "A lightweight markup language with plain-text formatting syntax designed so that it can be converted to HTML.".to_string()
                )]
            );
        }
        other => panic!("Expected DefinitionList, got {other:?}"),
    }
}

#[test]
fn test_definition_list_html_rendering() {
    let md = "\
**CPU**
: Central Processing Unit
";
    let doc = parse_markdown(md);
    let opts = HtmlOptions {
        profile: Some(Profile::GfmPlus),
        ..Default::default()
    };
    let html = render_html_document(&doc, &opts).expect("render HTML");
    assert!(html.contains("<dl>"));
    assert!(html.contains("<dt><strong>CPU</strong></dt>"));
    assert!(html.contains("<dd>Central Processing Unit</dd>"));
    assert!(html.contains("</dl>"));
}

#[test]
fn test_definition_list_pdf_rendering() {
    let md = "\
# Glossary

Term
: Definition text that explains the term in detail.
";
    let doc = parse_markdown(md);
    let opts = PdfOptions {
        profile: Some(Profile::GfmPlus),
        ..Default::default()
    };
    let pdf = render_pdf_document(&doc, &opts).expect("render PDF");
    assert!(pdf.starts_with(b"%PDF-1.7"));
    assert!(pdf.len() > 1000);
}

#[test]
fn a_glossary_after_a_setext_heading_keeps_its_heading_and_anchor() {
    use franken_markdown::{parse_markdown_spanned, parse_markdown_with_profile};

    for (underline, level) in [("=", 1), ("===", 1), ("   == ", 1), ("--", 2), ("---", 2)] {
        let source = format!(
            "First\n: old meaning\n\nAPI *Guide*\r\n{underline}\r\nTerm\r\n: new meaning\r\n\n[Jump](#api-guide)\n"
        );
        let document = parse_markdown(&source);
        assert_eq!(
            document,
            parse_markdown_with_profile(&source, Some(Profile::GfmPlus))
        );
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(document, spanned.to_document());
        assert!(
            matches!(
                &document.blocks[..],
                [Block::DefinitionList(_), Block::Heading {level: heading, ..}, Block::DefinitionList(_), Block::Paragraph(_)] if *heading == level
            ),
            "{source:?}: {document:?}"
        );
        assert_eq!(
            spanned.blocks[1].span.slice(&source),
            Some(format!("API *Guide*\r\n{underline}").as_str())
        );
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert!(html.contains(&format!("<h{level} id=\"api-guide\">API <em>Guide</em></h{level}>")));
        assert!(html.contains("<a href=\"#api-guide\">Jump</a>"));
    }
}

#[test]
fn an_underline_term_immediately_before_its_definition_is_preserved() {
    let source = "Term\n===\n: meaning";
    let document = parse_markdown(source);
    let html = franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert_eq!(
        html,
        "<dl>\n<dt>Term</dt>\n<dt>===</dt>\n<dd>meaning</dd>\n</dl>\n"
    );
}
