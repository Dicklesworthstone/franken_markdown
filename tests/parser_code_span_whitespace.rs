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
