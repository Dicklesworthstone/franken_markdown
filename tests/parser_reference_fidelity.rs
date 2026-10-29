//! Reference definitions must obey the same label/title grammar as their uses.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.to_owned())
}

fn link(label: &str, destination: &str, title: Option<&str>) -> Inline {
    Inline::Link {
        dest: destination.to_owned(),
        title: title.map(str::to_owned),
        content: vec![text(label)],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

fn contains_literal_paragraph(blocks: &[Block], expected: &str) -> bool {
    blocks.iter().any(|block| match block {
        Block::Paragraph(inlines) => inlines == &[text(expected)],
        Block::BlockQuote(children)
        | Block::FootnoteDefinition {
            blocks: children, ..
        } => contains_literal_paragraph(children, expected),
        Block::List(list) => list
            .items
            .iter()
            .any(|item| contains_literal_paragraph(&item.blocks, expected)),
        _ => false,
    })
}

#[test]
fn overlong_reference_definitions_remain_visible_without_destination_authority() {
    for label in ["a".repeat(1000), "é".repeat(1000)] {
        let definition = format!("[{label}]: /invalid");
        let source = format!("{definition}\r\n\r\nAfter.");
        let expected = vec![
            Block::Paragraph(vec![text(&definition)]),
            Block::Paragraph(vec![text("After.")]),
        ];
        let document = parse_markdown(&source);
        assert_eq!(document.blocks, expected);
        assert_eq!(parse_markdown_profiled(&source).document, document);
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(spanned.to_document(), document);
        assert_eq!(
            spanned.blocks[0].span.slice(&source),
            Some(definition.as_str())
        );
        assert_eq!(spanned.blocks[1].span.slice(&source), Some("After."));
        assert!(destination_spans(&source).is_empty());
        assert!(fragment(&source).contains(&definition));
    }
}

#[test]
fn reference_label_limit_counts_source_characters_before_folding() {
    // These 1000 source characters normalize to one character. An invalid
    // earlier definition must neither disappear nor reserve that short label.
    let label = format!("{}x{}", " ".repeat(499), " ".repeat(500));
    let definition = format!("[{label}]: /invalid");
    let source = format!("{definition}\n\n[x]: /correct\n\n[x]");
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![
            Block::Paragraph(vec![text(&definition)]),
            Block::Paragraph(vec![link("x", "/correct", None)]),
        ]
    );
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/correct");
    assert_eq!(&source[spans[0].range.clone()], "/correct");

    // A line ending inside a multiline label also counts toward the limit.
    let first = "a".repeat(500);
    let second = "b".repeat(499);
    let source = format!("[{first}\n{second}]: /invalid\n\nAfter.");
    assert_eq!(
        parse_markdown(&source).blocks,
        vec![
            Block::Paragraph(vec![
                text(&format!("[{first}")),
                Inline::SoftBreak,
                text(&format!("{second}]: /invalid")),
            ]),
            Block::Paragraph(vec![text("After.")]),
        ]
    );
    assert!(destination_spans(&source).is_empty());
}

#[test]
fn exactly_999_character_labels_still_resolve_in_ascii_unicode_and_escaped_forms() {
    for label in [
        "a".repeat(999),
        "é".repeat(999),
        format!("{}x", "\\]".repeat(499)),
    ] {
        assert_eq!(label.chars().count(), 999);
        let source = format!("[{label}]: /valid\n\n[visible][{label}]");
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![Block::Paragraph(vec![link("visible", "/valid", None)])]
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1);
        assert_eq!(&source[spans[0].range.clone()], "/valid");
    }
}

#[test]
fn invalid_reference_definitions_stay_inside_their_original_containers() {
    let definition = format!("[{}]: /invalid", "é".repeat(1000));
    for source in [
        format!("> {definition}\n>\n> Tail."),
        format!("- {definition}\n\n  Tail."),
        format!("[^note]: {definition}\n\nUse [^note]."),
    ] {
        let document = parse_markdown(&source);
        assert!(matches!(
            document.blocks.first(),
            Some(Block::BlockQuote(_) | Block::List(_) | Block::FootnoteDefinition { .. })
        ));
        assert!(contains_literal_paragraph(&document.blocks, &definition));
        assert!(destination_spans(&source).is_empty());
        assert_eq!(parse_markdown_spanned(&source).to_document(), document);
    }
}

#[test]
fn next_line_titles_decode_entities_identically_to_same_line_titles() {
    for (encoded, decoded) in [
        ("A &amp; B", "A & B"),
        ("&quot;hi&quot; &#39;ok&#39;", "\"hi\" 'ok'"),
        ("&lpar;nested&rpar;", "(nested)"),
        ("&#x1F642; &NotEqualTilde;", "🙂 ≂̸"),
        ("&notAnEntity; &amp;amp;", "&notAnEntity; &amp;"),
    ] {
        for (open, close) in [('"', '"'), ('\'', '\''), ('(', ')')] {
            let title = format!("{open}{encoded}{close}");
            let expected = vec![Block::Paragraph(vec![
                link("id", "/url", Some(decoded)),
                text(" "),
                Inline::Image {
                    dest: "/url".into(),
                    title: Some(decoded.into()),
                    alt: "image".into(),
                },
            ])];
            for separator in [" ", "\n"] {
                let source = format!("[id]: /url{separator}{title}\n\n[id] ![image][id]");
                let document = parse_markdown(&source);
                assert_eq!(document.blocks, expected, "{source:?}");
                assert_eq!(parse_markdown_spanned(&source).to_document(), document);
                let spans = destination_spans(&source);
                assert_eq!(spans.len(), 1);
                assert_eq!(&source[spans[0].range.clone()], "/url");
            }
        }
    }
    assert_eq!(
        fragment("[id]: /url\n\"A &amp; B\"\n\n[id]"),
        "<p><a href=\"/url\" title=\"A &amp; B\">id</a></p>\n"
    );
}

#[test]
fn decoded_title_delimiters_remain_safe_attribute_content() {
    let source = "[id]: /url\n\"&quot; onmouseover=&quot;alert(1)\"\n\n[id]";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![Block::Paragraph(vec![link(
            "id",
            "/url",
            Some("\" onmouseover=\"alert(1)")
        )])]
    );
    assert_eq!(
        fragment(source),
        "<p><a href=\"/url\" title=\"&quot; onmouseover=&quot;alert(1)\">id</a></p>\n"
    );
}

#[test]
fn unescaped_open_parentheses_in_titles_do_not_consume_definitions_or_links() {
    for title in ["(bad (inner)", "(bad\n(inner)"] {
        let definition = format!("[id]: /invalid {title}");
        let source = format!("{definition}\n\n[id]");
        let document = parse_markdown(&source);
        assert_eq!(document.blocks.len(), 2, "{source:?}");
        assert_eq!(document.blocks[1], Block::Paragraph(vec![text("[id]")]));
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(spanned.to_document(), document);
        assert_eq!(
            spanned.blocks[0].span.slice(&source),
            Some(definition.as_str())
        );
        assert!(destination_spans(&source).is_empty());
        assert!(fragment(&source).contains("/invalid"));

        let inline = format!("[link](/invalid {title})");
        assert!(
            !parse_inlines(&inline)
                .iter()
                .any(|node| matches!(node, Inline::Link { .. }))
        );
        assert!(destination_spans(&inline).is_empty());
        assert_eq!(fragment(&inline), format!("<p>{inline}</p>\n"));
    }
}

#[test]
fn malformed_next_line_title_preserves_the_valid_definition_and_title_source() {
    let source = "[id]: /url\n(bad (inner)\n\n[id]";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![
            Block::Paragraph(vec![text("(bad (inner)")]),
            Block::Paragraph(vec![link("id", "/url", None)]),
        ]
    );
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.blocks[0].span.slice(source), Some("(bad (inner)"));
    assert_eq!(spanned.blocks[1].span.slice(source), Some("[id]"));
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(&source[spans[0].range.clone()], "/url");
}

#[test]
fn a_malformed_title_cannot_shadow_the_first_valid_definition() {
    let source = "[id]: /invalid (bad (inner)\n\n[id]: /first \"Good\"\n[id]: /later\n\n[id]";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![
            Block::Paragraph(vec![
                link("id", "/first", Some("Good")),
                text(": /invalid (bad (inner)"),
            ]),
            Block::Paragraph(vec![link("id", "/first", Some("Good"))]),
        ]
    );
    let spans = destination_spans(source);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/first", "/later"]
    );
}

#[test]
fn escaped_entity_and_quoted_parentheses_remain_valid_titles() {
    for (title, decoded) in [
        ("(safe \\(nested\\))", "safe (nested)"),
        ("(&lpar;nested&rpar;)", "(nested)"),
        ("\"literal (nested)\"", "literal (nested)"),
        ("'literal (nested)'", "literal (nested)"),
    ] {
        assert_eq!(
            parse_inlines(&format!("[link](/url {title})")),
            vec![link("link", "/url", Some(decoded))]
        );
        for separator in [" ", "\n"] {
            let source = format!("[id]: /url{separator}{title}\n\n[id]");
            assert_eq!(
                parse_markdown(&source).blocks,
                vec![Block::Paragraph(vec![link("id", "/url", Some(decoded))])]
            );
        }
    }
}
