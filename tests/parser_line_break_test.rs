//! Source delimiters, rather than decoded text, determine Markdown line breaks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::parse_inlines;
use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn expected_break(before: &str, hard: bool) -> Vec<Inline> {
    vec![
        Inline::Text(before.to_owned()),
        if hard {
            Inline::HardBreak
        } else {
            Inline::SoftBreak
        },
        Inline::Text("next".to_owned()),
    ]
}

#[test]
fn escaped_backslashes_are_preserved_and_only_unescaped_backslashes_break() {
    for count in 1..=12 {
        let source = format!("text{}\nnext", "\\".repeat(count));
        let before = format!("text{}", "\\".repeat(count / 2));
        assert_eq!(
            parse_inlines(&source),
            expected_break(&before, count % 2 == 1),
            "source backslash count: {count}"
        );
    }
}

#[test]
fn character_references_cannot_create_hard_break_delimiters() {
    for (source, before) in [
        ("text&#92;\nnext", "text\\"),
        ("text&#x5c;\nnext", "text\\"),
        ("text&bsol;\nnext", "text\\"),
        ("text&#32;&#32;\nnext", "text  "),
        ("text&#x20;&#x20;\nnext", "text  "),
        ("text&#32; \nnext", "text "),
        ("text &#32;\nnext", "text  "),
    ] {
        assert_eq!(
            parse_inlines(source),
            expected_break(before, false),
            "{source:?}"
        );
    }
}

#[test]
fn character_reference_spaces_survive_the_end_of_an_inline_run() {
    for (source, expected) in [
        ("text&#32;", "text "),
        ("text&#x20;", "text "),
        ("text&#32;&#32;", "text  "),
        ("text &#32;", "text  "),
        ("text&#32; ", "text "),
        ("text&#32;  ", "text "),
    ] {
        assert_eq!(
            parse_inlines(source),
            vec![Inline::Text(expected.to_owned())],
            "{source:?}"
        );
    }
}

#[test]
fn nested_inline_trailing_entities_keep_visible_word_separation() {
    let document = parse_markdown("[link&#32;](/target)next ~~word&#32;~~next");
    let rendered =
        franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert_eq!(
        rendered,
        "<p><a href=\"/target\">link </a>next <del>word </del>next</p>\n"
    );
}

#[test]
fn trailing_source_spaces_do_not_consume_literal_backslashes_or_entity_spaces() {
    for (prefix, before) in [
        ("text\\", "text\\"),
        ("text\\\\", "text\\"),
        ("text&#92;", "text\\"),
        ("text&#32;", "text "),
        ("text&#32;&#32;", "text  "),
    ] {
        for spaces in 1..=4 {
            let source = format!("{prefix}{}\nnext", " ".repeat(spaces));
            assert_eq!(
                parse_inlines(&source),
                expected_break(before, spaces >= 2),
                "{source:?}"
            );
        }
    }
}

#[test]
fn hard_breaks_consume_only_their_own_source_marker() {
    for (source, before) in [
        ("text  \\\nnext", "text  "),
        ("text&#32;\\\nnext", "text "),
        ("text&#92;\\\nnext", "text\\"),
        ("text\\\\\\\nnext", "text\\"),
    ] {
        assert_eq!(
            parse_inlines(source),
            expected_break(before, true),
            "{source:?}"
        );
    }
}

#[test]
fn nested_inline_and_block_parses_preserve_literal_line_end_content() {
    let source = "[text\\\\\nnext](/target)";
    assert_eq!(
        parse_inlines(source),
        vec![Inline::Link {
            dest: "/target".to_owned(),
            title: None,
            content: expected_break("text\\", false),
        }]
    );

    let source = "**text&#92;\nnext**";
    assert_eq!(
        parse_inlines(source),
        vec![Inline::Strong(expected_break("text\\", false))]
    );

    for source in [
        "text\\\\\nnext",
        "text&#32; \nnext",
        "> text&#92;\n> next",
        "- text&#32;\n  next",
        "[text\\\\\nnext](/target)",
    ] {
        let document = parse_markdown(source);
        assert_eq!(parse_markdown_profiled(source).document, document);
        assert_eq!(
            parse_markdown_spanned(source)
                .blocks
                .into_iter()
                .map(|block| block.node)
                .collect::<Vec<_>>(),
            document.blocks,
            "spanned parse: {source:?}"
        );
    }

    assert_eq!(
        parse_markdown("> text&#92;\n> next").blocks,
        vec![Block::BlockQuote(vec![Block::Paragraph(expected_break(
            "text\\", false
        ))])]
    );
}

#[test]
fn html_retains_literal_content_while_rendering_actual_hard_breaks() {
    let source = "text\\\\\nnext\n\ntext&#32; \nnext\n\ntext&#92;  \nnext";
    let document = parse_markdown(source);
    let rendered =
        franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert_eq!(
        rendered,
        "<p>text\\\nnext</p>\n<p>text \nnext</p>\n<p>text\\<br>\nnext</p>\n"
    );
}
