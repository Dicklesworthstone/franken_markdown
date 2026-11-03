//! Reference precollection must follow the real paragraph's continuation rules.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn continued_paragraph() -> Block {
    Block::Paragraph(vec![
        text("Intro"),
        Inline::SoftBreak,
        text("continued"),
        Inline::SoftBreak,
        text("[id]: /literal"),
    ])
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn hanging_indent_does_not_turn_paragraph_text_into_a_reference_definition() {
    for indent in ["    ", "        ", "\t", " \t"] {
        let source = format!("Intro\n{indent}continued\n[id]: /literal\n\n[id]\n");
        let document = parse_markdown(&source);
        assert_eq!(
            document.blocks,
            [continued_paragraph(), Block::Paragraph(vec![text("[id]")])],
            "{source:?}"
        );
        assert_eq!(parse_markdown_profiled(&source).document, document);
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
        assert!(destination_spans(&source).is_empty());
        assert_eq!(
            fragment(&source),
            "<p>Intro\ncontinued\n[id]: /literal</p>\n<p>[id]</p>\n"
        );
    }
}

#[test]
fn quoted_list_and_footnote_paragraphs_keep_reference_looking_continuations() {
    for source in [
        "> Intro\n>     continued\n> [id]: /literal\n\n[id]\n",
        "- Intro\n      continued\n  [id]: /literal\n\n[id]\n",
        "[^note]: Intro\n        continued\n    [id]: /literal\n\nUse [^note].\n\n[id]\n",
    ] {
        let document = parse_markdown(source);
        let inner = match &document.blocks[0] {
            Block::BlockQuote(blocks) | Block::FootnoteDefinition { blocks, .. } => blocks,
            Block::List(list) => {
                assert!(list.tight);
                assert_eq!(list.items.len(), 1);
                &list.items[0].blocks
            }
            other => panic!("expected original container, got {other:?}"),
        };
        assert_eq!(inner, &[continued_paragraph()], "{source:?}");
        assert_eq!(
            document.blocks.last(),
            Some(&Block::Paragraph(vec![text("[id]")]))
        );
        assert_eq!(parse_markdown_profiled(source).document, document);
        assert_eq!(parse_markdown_spanned(source).to_document(), document);
        assert!(destination_spans(source).is_empty());
        assert!(fragment(source).contains("[id]: /literal"), "{source:?}");
    }
}

#[test]
fn hanging_reference_text_cannot_shadow_a_real_later_definition() {
    let source = "Intro\n    continued\n[id]: /literal\n\n[id]: /real\n\n[id]\n";
    let document = parse_markdown(source);
    assert_eq!(document.blocks.len(), 2);
    let Block::Paragraph(inlines) = &document.blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(
        inlines,
        &[
            text("Intro"),
            Inline::SoftBreak,
            text("continued"),
            Inline::SoftBreak,
            Inline::Link {
                dest: "/real".into(),
                title: None,
                content: vec![text("id")],
            },
            text(": /literal"),
        ]
    );
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/real");
    assert_eq!(&source[spans[0].range.clone()], "/real");
    assert!(fragment(source).contains("<a href=\"/real\">id</a>: /literal"));
}

#[test]
fn indented_code_at_a_real_boundary_still_leaves_following_definitions_in_scope() {
    for code in ["    code", "    <div>", "    [code]: /literal", "\tcode"] {
        let source = format!("Intro\n\n{code}\n[id]: /real\n\n[id]\n");
        let document = parse_markdown(&source);
        assert!(matches!(&document.blocks[1], Block::CodeBlock { .. }));
        assert!(matches!(
            document.blocks.last(),
            Some(Block::Paragraph(inlines))
                if matches!(&inlines[..], [Inline::Link { dest, .. }] if dest == "/real")
        ));
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}");
        assert_eq!(&source[spans[0].range.clone()], "/real");
    }
}

#[test]
fn preserved_paragraphs_keep_original_crlf_unicode_source_spans() {
    let source = "\u{feff}Intro\r\n\tcontinued\r\n[id]: /literal\r\n\r\ncafé [id]\r\n";
    let document = parse_markdown(source);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document);
    assert_eq!(spanned.blocks.len(), 2);
    assert_eq!(
        spanned.blocks[0].span.slice(source),
        Some("Intro\r\n\tcontinued\r\n[id]: /literal")
    );
    assert_eq!(spanned.blocks[1].span.slice(source), Some("café [id]"));
    assert!(destination_spans(source).is_empty());
}
