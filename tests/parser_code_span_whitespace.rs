//! Code-span normalization distinguishes ASCII spaces from literal whitespace.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::parse_inlines;
use franken_markdown::{HtmlOptions, Inline, parse_markdown};

#[test]
fn code_spans_strip_outside_spaces_around_tabs_and_unicode_whitespace() {
    for content in ["\t", "\u{a0}", "\u{2003}", "\u{2028}", "\t\u{a0}"] {
        let source = format!("` {content} `");
        assert_eq!(
            parse_inlines(&source),
            vec![Inline::Code(content.to_owned())]
        );
        let document = parse_markdown(&source);
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert_eq!(html, format!("<p><code>{content}</code></p>\n"));
    }
}

#[test]
fn only_one_outside_ascii_space_is_removed_and_other_whitespace_stays_literal() {
    for (source, expected) in [
        ("`  \t  `", " \t "),
        ("`\t value \t`", "\t value \t"),
        ("` \t`", " \t"),
        ("`\t `", "\t "),
        ("`  `", "  "),
        ("` \n `", "   "),
        ("`\n\n`", "  "),
        ("` \r\n\t\r `", " \t "),
    ] {
        assert_eq!(
            parse_inlines(source),
            vec![Inline::Code(expected.to_owned())],
            "{source:?}"
        );
    }
}

#[test]
fn normalization_does_not_activate_code_contents_or_steal_following_destinations() {
    let source = "é ` \t [hidden](/hidden) \t ` [visible](/kept)";
    let document = parse_markdown(source);
    let html = franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
    assert_eq!(
        html,
        "<p>é <code>\t [hidden](/hidden) \t</code> <a href=\"/kept\">visible</a></p>\n"
    );
    let destinations = franken_markdown::parse::destination_spans(source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/kept");
    assert_eq!(&source[destinations[0].range.clone()], "/kept");
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
            let document = parse_markdown(&source);
            assert_eq!(
                document.blocks,
                vec![franken_markdown::Block::Paragraph(expected_inlines.clone())],
                "{source:?}"
            );
        }
    }
}

#[test]
fn multiline_code_keeps_content_indent_after_removing_container_markers() {
    for source in ["> `first\n>   second`", "- `first\n    second`"] {
        let document = parse_markdown(source);
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert!(
            html.contains("<code>first   second</code>"),
            "{source:?}: {html}"
        );
    }
}

#[test]
fn continuation_spaces_outside_code_still_form_normal_paragraph_breaks() {
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
        let document = parse_markdown(source);
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert_eq!(html, expected, "{source:?}");
    }
}

#[test]
fn destinations_after_multiline_code_keep_exact_source_coordinates() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = "é `first\n  [hidden](/hidden)` [visible](résumé.md)".replace('\n', newline);
        let document = parse_markdown(&source);
        let html =
            franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default());
        assert!(html.contains("<code>first   [hidden](/hidden)</code>"));
        let destinations = franken_markdown::parse::destination_spans(&source);
        assert_eq!(destinations.len(), 1);
        assert_eq!(destinations[0].dest, "résumé.md");
        assert_eq!(&source[destinations[0].range.clone()], "résumé.md");
        assert_eq!(
            franken_markdown::parse_markdown_spanned(&source).to_document(),
            document
        );
    }
}
