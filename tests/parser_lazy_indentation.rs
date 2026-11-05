//! CommonMark 5.1/example 238: indentation on an admitted lazy paragraph line
//! must not become a block opener when quote/list containers are stripped.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, Document, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
    parse_markdown_spanned_profiled,
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
        assert_eq!(
            blocks.len(),
            1,
            "expected a single container/paragraph: {blocks:?}"
        );
        match &blocks[0] {
            Block::BlockQuote(inner) => blocks = inner,
            Block::List(list) => {
                assert_eq!(list.items.len(), 1, "unexpected sibling item: {list:?}");
                blocks = &list.items[0].blocks;
            }
            Block::Paragraph(inlines) => return inlines,
            other => panic!("lazy paragraph became a block: {other:?}"),
        }
    }
}

fn visible_link() -> Inline {
    Inline::Link {
        dest: "/réal#heading".into(),
        title: None,
        content: vec![Inline::Text("visible".into())],
    }
}

fn code_paragraph() -> Vec<Inline> {
    vec![
        Inline::Text("alpha".into()),
        Inline::SoftBreak,
        Inline::Code("β [literal](/not-a-link)".into()),
        Inline::SoftBreak,
        visible_link(),
    ]
}

fn assert_visible_destination(source: &str) {
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
    assert_eq!(destinations[0].dest, "/réal#heading");
    assert_eq!(&source[destinations[0].range.clone()], "/réal#heading");
}

#[test]
fn four_column_lazy_markers_keep_paragraph_ownership() {
    for indent in ["    ", "        ", "\t", " \t", "  \t", "   \t"] {
        for marker in [
            "```",
            "~~~",
            "# heading",
            "> nested",
            "***",
            "---",
            "=",
            "2. item",
            "- item",
        ] {
            let source = format!("> alpha\n{indent}{marker}\n> beta\n");
            let parsed = document(&source);
            assert_eq!(
                only_paragraph(&parsed.blocks),
                [
                    Inline::Text("alpha".into()),
                    Inline::SoftBreak,
                    Inline::Text(marker.into()),
                    Inline::SoftBreak,
                    Inline::Text("beta".into()),
                ],
                "{source:?}"
            );
        }
    }
}

#[test]
fn lazy_html_stays_inline_and_does_not_swallow_following_markdown() {
    let source = "> alpha\n    <div>\n> [visible](/réal#heading)\n";
    let parsed = document(source);
    assert_eq!(
        only_paragraph(&parsed.blocks),
        [
            Inline::Text("alpha".into()),
            Inline::SoftBreak,
            Inline::Html("<div>".into()),
            Inline::SoftBreak,
            visible_link(),
        ]
    );
    assert_visible_destination(source);
}

#[test]
fn lazy_backticks_remain_inline_code_through_nested_quotes_and_lists() {
    for (opening, continuation) in [
        ("alpha", ""),
        ("- alpha", "  "),
        ("1. alpha", "   "),
        ("- 1. alpha", "     "),
        ("> - alpha", ">   "),
        ("- > alpha", "  > "),
    ] {
        let source = format!(
            "> {opening}\n    ```\n> {continuation}β [literal](/not-a-link)\n    ```\n> {continuation}[visible](/réal#heading)\n"
        );
        let parsed = document(&source);
        assert_eq!(
            only_paragraph(&parsed.blocks),
            code_paragraph(),
            "{source:?}"
        );
        assert_visible_destination(&source);
        let spanned = parse_markdown_spanned(&source);
        assert_eq!(
            spanned.blocks[0].span.slice(&source),
            Some(source.trim_end_matches('\n'))
        );
    }
}

#[test]
fn lazy_list_and_setext_markers_do_not_inject_escapes_into_inline_code() {
    for opening in ["> alpha", "> - alpha", "1234. alpha"] {
        let source = format!("{opening}\n    ```\n    - item\n    2. item\n    =\n    ```\n");
        let parsed = document(&source);
        assert_eq!(
            only_paragraph(&parsed.blocks),
            [
                Inline::Text("alpha".into()),
                Inline::SoftBreak,
                Inline::Code("- item 2. item =".into()),
            ],
            "{source:?}"
        );
    }
}

#[test]
fn low_indent_lazy_setext_lines_do_not_escape_inline_code() {
    for (opening, closing) in [
        ("> `alpha", "> beta`"),
        ("> - `alpha", ">   beta`"),
        ("1234. `alpha", "      beta`"),
    ] {
        for indent in ["", " ", "  ", "   "] {
            for marker in ["=", "=="] {
                let source = format!("{opening}\n{indent}{marker}\n{closing}\n");
                let parsed = document(&source);
                assert_eq!(
                    only_paragraph(&parsed.blocks),
                    [Inline::Code(format!("alpha {marker} beta"))],
                    "{source:?}"
                );
            }
        }
    }
}

#[test]
fn wide_ordered_items_preserve_the_same_lazy_inline_code() {
    for opening in ["1234. alpha", "123456789. alpha", "1234. - alpha"] {
        let source = format!(
            "{opening}\n    ```\n    β [literal](/not-a-link)\n    ```\n    [visible](/réal#heading)\n"
        );
        let parsed = document(&source);
        assert!(matches!(parsed.blocks[0], Block::List(_)), "{source:?}");
        assert_eq!(
            only_paragraph(&parsed.blocks),
            code_paragraph(),
            "{source:?}"
        );
        assert_visible_destination(&source);
    }
}

#[test]
fn lazy_marker_lines_cannot_create_reference_definition_boundaries() {
    for marker in ["```", "# heading", "> nested", "<div>"] {
        let source = format!("> alpha\n    {marker}\n[ref]: /not-a-definition\n\n[ref]\n");
        let parsed = document(&source);
        assert_eq!(parsed.blocks.len(), 2, "{source:?}: {parsed:?}");
        assert!(
            matches!(&parsed.blocks[0], Block::BlockQuote(blocks) if matches!(blocks.as_slice(), [Block::Paragraph(_)])),
            "{source:?}: {parsed:?}"
        );
        assert_eq!(
            parsed.blocks[1],
            Block::Paragraph(vec![Inline::Text("[ref]".into())])
        );
        assert!(destination_spans(&source).is_empty(), "{source:?}");
    }
}

#[test]
fn padded_lazy_lines_keep_unicode_crlf_tab_source_ranges() {
    let source =
        "> - 12345. café\r\n\t# [visible](/réal#heading)\r\n>          tail\r\n\r\noutside\r\n";
    let parsed = document(source);
    assert_eq!(parsed.blocks.len(), 2, "{parsed:?}");
    assert_eq!(
        only_paragraph(&parsed.blocks[..1]),
        [
            Inline::Text("café".into()),
            Inline::SoftBreak,
            Inline::Text("# ".into()),
            visible_link(),
            Inline::SoftBreak,
            Inline::Text("tail".into()),
        ]
    );
    assert_visible_destination(source);
    let spanned = parse_markdown_spanned(source);
    let end = source.find("\r\n\r\n").unwrap();
    assert_eq!(spanned.blocks[0].span.slice(source), Some(&source[..end]));
    assert_eq!(spanned.blocks[1].span.slice(source), Some("outside"));
}

#[test]
fn deeply_nested_lists_leave_lazy_backticks_in_the_paragraph() {
    let source = format!(
        "> {}alpha\n    ```\n    β [literal](/not-a-link)\n    ```\n    [visible](/réal#heading)\n",
        "123456789. ".repeat(48)
    );
    let parsed = document(&source);
    assert_eq!(only_paragraph(&parsed.blocks), code_paragraph());
    assert_visible_destination(&source);
}

#[test]
fn explicit_and_outside_fences_keep_their_real_block_boundaries() {
    let paragraph = Block::Paragraph(vec![Inline::Text("alpha".into())]);
    let code = Block::CodeBlock {
        lang: None,
        code: "literal\n".into(),
    };
    let quoted = document("> alpha\n> ```\n> literal\n> ```\n");
    assert_eq!(
        quoted.blocks,
        [Block::BlockQuote(vec![paragraph.clone(), code.clone()])]
    );
    let outside = document("> alpha\n```\nliteral\n```\n");
    assert_eq!(
        outside.blocks,
        [Block::BlockQuote(vec![paragraph.clone()]), code.clone()]
    );
    let indented = document("1234. alpha\n      ```\n      literal\n      ```\n");
    let Block::List(list) = &indented.blocks[0] else {
        panic!("{indented:?}")
    };
    assert_eq!(list.items[0].blocks, [paragraph, code]);
}
