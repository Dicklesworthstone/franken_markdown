//! Raw-HTML boundaries must not consume live Markdown or its editable targets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn fragment(source: &str, allow_raw_html: bool) -> String {
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    franken_markdown::html::render_fragment(
        &document.blocks,
        &HtmlOptions {
            allow_raw_html,
            ..HtmlOptions::default()
        },
    )
}

#[test]
fn official_invalid_self_closing_tag_is_literal_text() {
    // CommonMark 0.31.2 example 621, preserving the official expected markup.
    let source = "< a><\nfoo><bar/ >\n<foo bar=baz\nbim!bop />\n";
    assert_eq!(
        fragment(source, true),
        "<p>&lt; a&gt;&lt;\nfoo&gt;&lt;bar/ &gt;\n&lt;foo bar=baz\nbim!bop /&gt;</p>\n"
    );
}

#[test]
fn official_short_comments_end_before_the_later_comment_closer() {
    // CommonMark 0.31.2 example 626. Neither short form owns the later '-->'.
    let source = "foo <!--> foo -->\n\nfoo <!---> foo -->\n";
    assert_eq!(
        fragment(source, true),
        "<p>foo <!--> foo --&gt;</p>\n<p>foo <!---> foo --&gt;</p>\n"
    );
}

#[test]
fn short_comments_leave_following_formatting_and_destinations_live() {
    for comment in ["<!-->", "<!--->"] {
        let source = format!("café {comment} **visible** [réad](/kept) --> after");
        assert_eq!(
            fragment(&source, true),
            format!(
                "<p>café {comment} <strong>visible</strong> <a href=\"/kept\">réad</a> --&gt; after</p>\n"
            )
        );
        assert_eq!(
            fragment(&source, false),
            "<p>café  <strong>visible</strong> <a href=\"/kept\">réad</a> --&gt; after</p>\n"
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].dest, "/kept");
        assert_eq!(&source[spans[0].range.clone()], "/kept");
    }
}

#[test]
fn short_comments_inside_link_labels_preserve_both_link_owners() {
    for comment in ["<!-->", "<!--->"] {
        let source = format!("[{comment} *visible*](/outer) --> [later](/later)");
        assert_eq!(
            fragment(&source, true),
            format!(
                "<p><a href=\"/outer\">{comment} <em>visible</em></a> --&gt; <a href=\"/later\">later</a></p>\n"
            )
        );
        let spans = destination_spans(&source);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            vec!["/outer", "/later"]
        );
        for span in spans {
            assert_eq!(&source[span.range], span.dest);
        }
    }
}

#[test]
fn whitespace_cannot_separate_the_self_closing_slash_from_its_bracket() {
    for whitespace in [" ", "\t", "\n", "\r", "\r\n", "\u{c}"] {
        for prefix in ["<span/", "<span /", "<span title='x'/"] {
            let source = format!("before {prefix}{whitespace}> *visible*");
            let inlines = parse_inlines(&source);
            assert!(
                !inlines
                    .iter()
                    .any(|inline| matches!(inline, Inline::Html(_))),
                "invalid tag became HTML: {source:?}: {inlines:?}"
            );
            assert!(
                inlines
                    .iter()
                    .any(|inline| matches!(inline, Inline::Emphasis(_))),
                "following Markdown was hidden: {source:?}: {inlines:?}"
            );
        }
    }
}

#[test]
fn rejected_tags_cannot_hide_links_in_their_apparent_attributes() {
    let source = "café <span title=\"[visible](/kept)\" / > after";
    let spans = destination_spans(source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/kept");
    assert_eq!(&source[spans[0].range.clone()], "/kept");
    assert!(fragment(source, true).contains("<a href=\"/kept\">visible</a>"));
    assert!(fragment(source, false).contains("<a href=\"/kept\">visible</a>"));
}

#[test]
fn valid_tags_and_complete_comments_keep_their_literal_contents() {
    for raw in [
        "<span/>",
        "<span />",
        "<span title='[hidden](/hidden)' />",
        "<span title='[hidden](/hidden)'\n/>",
        // An unquoted attribute may contain a slash; this is not '/ >'.
        "<span title=value/ >",
        "<!-- *hidden* [hidden](/hidden) -->",
        "<!--\n[hidden](/hidden)\n-->",
    ] {
        let source = format!("before {raw} [visible](/visible)");
        assert_eq!(
            fragment(&source, true),
            format!("<p>before {raw} <a href=\"/visible\">visible</a></p>\n")
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}");
        assert_eq!(spans[0].dest, "/visible");
    }
}

#[test]
fn unclosed_inline_comments_remain_visible_markdown() {
    for start in ["<!--", "<!---", "<!-- note"] {
        let source = format!("before {start} **visible** [link](/kept)");
        let rendered = fragment(&source, true);
        assert!(rendered.contains("&lt;!--"), "{source:?}: {rendered}");
        assert!(rendered.contains("<strong>visible</strong>"));
        assert!(rendered.contains("<a href=\"/kept\">link</a>"));
    }
}
