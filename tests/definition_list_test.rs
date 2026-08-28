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
            assert_eq!(items[0].definitions[0], vec![Inline::Text("Definition 1".to_string())]);
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
            assert_eq!(items[0].terms[0], vec![Inline::Text("Markdown".to_string())]);
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
