//! Literal inline contents retain source indentation after container padding.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, parse_markdown_spanned_profiled,
};

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(
        parse_markdown_profiled(source).document,
        parsed,
        "{source:?}"
    );
    assert_eq!(
        parse_markdown_spanned(source).to_document(),
        parsed,
        "{source:?}"
    );
    assert_eq!(
        parse_markdown_spanned_profiled(source)
            .document
            .to_document(),
        parsed,
        "{source:?}"
    );
    parsed
}

fn only_paragraph(mut blocks: &[Block]) -> &[Inline] {
    loop {
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        match &blocks[0] {
            Block::Paragraph(inlines) => return inlines,
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                blocks = inner;
            }
            Block::List(list) => {
                assert_eq!(list.items.len(), 1, "{list:?}");
                blocks = &list.items[0].blocks;
            }
            other => panic!("expected a paragraph container, got {other:?}"),
        }
    }
}

#[test]
fn multiline_code_spans_preserve_continuation_indentation() {
    for (source, expected) in [
        ("`first\n  second`", "first   second"),
        ("`first\n    second`", "first     second"),
        ("`first\n   second\n last`", "first    second  last"),
        ("` \n   value\n `", "    value "),
        ("``first\n  `second` ``", "first   `second` "),
    ] {
        let expected_inlines = vec![Inline::Code(expected.to_owned())];
        assert_eq!(parse_inlines(source), expected_inlines, "{source:?}");
        for newline in ["\n", "\r\n", "\r"] {
            let source = source.replace('\n', newline);
            let parsed = document(&source);
            assert_eq!(
                only_paragraph(&parsed.blocks),
                expected_inlines,
                "{source:?}"
            );
            let html =
                franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
            assert_eq!(html, format!("<p><code>{expected}</code></p>\n"));
        }
    }
}

#[test]
fn explicit_container_lines_preserve_literal_indent_after_their_markers() {
    for source in [
        "> `first\n>   second`",
        "- `first\n    second`",
        "> - `first\n>     second`",
        "1234. `first\n        second`",
        "[^note]: `first\n      second`",
    ] {
        let parsed = document(source);
        assert_eq!(
            only_paragraph(&parsed.blocks),
            [Inline::Code("first   second".into())],
            "{source:?}"
        );
    }
}

#[test]
fn mixed_lazy_and_explicit_lines_keep_literal_tabs_and_exact_destination_ranges() {
    let source =
        "> `α\n=\n>   β\n\t2. item\n> \t[hidden](/hidden)\n>   ω` [visible](/réal#heading)\n";
    for newline in ["\n", "\r\n", "\r"] {
        let source = source.replace('\n', newline);
        let parsed = document(&source);
        assert_eq!(
            only_paragraph(&parsed.blocks),
            [
                Inline::Code("α =   β 2. item \t[hidden](/hidden)   ω".into()),
                Inline::Text(" ".into()),
                Inline::Link {
                    dest: "/réal#heading".into(),
                    title: None,
                    content: vec![Inline::Text("visible".into())],
                },
            ],
            "{source:?}"
        );
        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 1, "{destinations:?}");
        assert_eq!(destinations[0].dest, "/réal#heading");
        assert_eq!(&source[destinations[0].range.clone()], "/réal#heading");
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(
            spanned.blocks[0].span.slice(&source),
            Some(source.trim_end_matches(['\r', '\n']))
        );
    }
}

#[test]
fn lazy_padding_scope_survives_nested_items_and_ends_before_later_blocks() {
    let mut source = String::new();
    for index in 0..128 {
        source.push_str(&format!(
            "1234. > `one\n=\n      >   two` [item](/item-{index})\n"
        ));
    }
    source.push_str("\n`outer\n    preserved`\n");
    let parsed = document(&source);
    assert_eq!(parsed.blocks.len(), 2, "{parsed:?}");
    let Block::List(list) = &parsed.blocks[0] else {
        panic!("expected the ordered list: {parsed:?}");
    };
    assert_eq!(list.items.len(), 128);
    for (index, item) in list.items.iter().enumerate() {
        assert_eq!(
            only_paragraph(&item.blocks),
            [
                Inline::Code("one =   two".into()),
                Inline::Text(" ".into()),
                Inline::Link {
                    dest: format!("/item-{index}"),
                    title: None,
                    content: vec![Inline::Text("item".into())],
                },
            ],
            "item {index}"
        );
    }
    assert_eq!(
        parsed.blocks[1],
        Block::Paragraph(vec![Inline::Code("outer     preserved".into())])
    );
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 128);
    for (index, destination) in destinations.iter().enumerate() {
        assert_eq!(destination.dest, format!("/item-{index}"));
        assert_eq!(&source[destination.range.clone()], destination.dest);
    }
}

#[test]
fn continuation_spaces_in_prose_still_form_normal_paragraph_breaks() {
    for (source, expected) in [
        (
            "first\n  **second**",
            "<p>first\n<strong>second</strong></p>\n",
        ),
        (
            "first  \n  **second**",
            "<p>first<br>\n<strong>second</strong></p>\n",
        ),
        ("`first`\n  second", "<p><code>first</code>\nsecond</p>\n"),
    ] {
        let parsed = document(source);
        let html = franken_markdown::html::render_fragment(&parsed.blocks, &HtmlOptions::default());
        assert_eq!(html, expected, "{source:?}");
    }
}

#[test]
fn other_literal_inline_contents_keep_continuation_spaces() {
    let math = document("$x % comment\n  y$");
    assert_eq!(
        only_paragraph(&math.blocks),
        [Inline::Math("x % comment\n  y".into())]
    );
    let html = document("before <span title=\"first\n  second\">after</span>");
    assert_eq!(
        only_paragraph(&html.blocks),
        [
            Inline::Text("before ".into()),
            Inline::Html("<span title=\"first\n  second\">".into()),
            Inline::Text("after".into()),
            Inline::Html("</span>".into()),
        ]
    );
}
