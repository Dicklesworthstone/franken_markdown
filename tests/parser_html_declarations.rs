//! Declaration atoms own their contents regardless of ASCII letter case.
//!
//! CommonMark 0.31.2 section 6.6 starts a declaration with <! followed by
//! any ASCII letter. It does not require an uppercase name or whitespace.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::{destination_spans, parse_inlines, reference_label_spans};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
    parse_markdown_spanned_profiled,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn link(label: &str, dest: &str) -> Inline {
    Inline::Link {
        dest: dest.into(),
        title: None,
        content: vec![text(label)],
    }
}

fn fragment(source: &str, allow_raw_html: bool) -> String {
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    assert_eq!(
        parse_markdown_spanned_profiled(source).document.to_document(),
        document
    );
    franken_markdown::html::render_fragment(
        &document.blocks,
        &HtmlOptions {
            allow_raw_html,
            ..HtmlOptions::default()
        },
    )
}

#[test]
fn every_ascii_letter_can_start_a_declaration_without_name_or_space_requirements() {
    for first in ('a'..='z').chain('A'..='Z') {
        for tail in ["", " [hidden](/hidden)", "=[hidden](/hidden)", "-x"] {
            let raw = format!("<!{first}{tail}>");
            let source = format!("before {raw} [visible](/visible)");
            assert_eq!(
                parse_inlines(&source),
                [
                    text("before "),
                    Inline::Html(raw.clone()),
                    text(" "),
                    link("visible", "/visible"),
                ],
                "{source:?}"
            );
            assert_eq!(
                fragment(&source, true),
                format!("<p>before {raw} <a href=\"/visible\">visible</a></p>\n"),
                "{source:?}"
            );
        }
    }
}

#[test]
fn declarations_do_not_activate_markup_or_break_their_enclosing_link() {
    for raw in [
        "<!doctype [hidden](/hidden)>",
        "<!dOcTyPe **hidden** `hidden]` $x]$>",
        "<!a=[hidden](/hidden)>",
    ] {
        let source = format!("[before {raw} after](/chosen) [later](/later)");
        assert_eq!(
            parse_inlines(&source),
            [
                Inline::Link {
                    dest: "/chosen".into(),
                    title: None,
                    content: vec![text("before "), Inline::Html(raw.into()), text(" after")],
                },
                text(" "),
                link("later", "/later"),
            ],
            "{source:?}"
        );
        for allow_raw_html in [false, true] {
            let html = fragment(&source, allow_raw_html);
            assert!(html.contains("href=\"/chosen\""), "{source:?}: {html}");
            assert!(html.contains("href=\"/later\""), "{source:?}: {html}");
            assert!(!html.contains("href=\"/hidden\""), "{source:?}: {html}");
        }
    }
}

#[test]
fn declaration_contents_never_receive_editable_destination_or_reference_authority() {
    let source = "[ref]: /reference\n\né <!doctype [hidden](/hidden) [hidden][ref]> [visible][ref] [café](/kept#part)\n";
    let destinations = destination_spans(source);
    assert_eq!(
        destinations
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["/reference", "/kept#part"]
    );
    for destination in destinations {
        assert_eq!(&source[destination.range], destination.dest);
    }
    let labels = reference_label_spans(source);
    assert_eq!(labels.len(), 2, "{labels:?}");
    assert_eq!(
        labels[1].label.as_ref().unwrap().start,
        source.find("[visible][ref]").unwrap() + 10
    );
    let html = fragment(source, false);
    assert!(html.contains("href=\"/reference\">visible</a>"), "{html}");
    assert!(!html.contains("href=\"/hidden\""), "{html}");
}

#[test]
fn multiline_declarations_preserve_literals_and_following_unicode_source_ranges() {
    for newline in ["\n", "\r\n", "\r"] {
        for (prefix, continuation) in [("", ""), ("> ", "> "), ("- ", "  ")] {
            let source = format!(
                "{prefix}café <!doctype [hidden](/hidden){newline}{continuation}**literal**> [visible](/réal#part){newline}"
            );
            let destinations = destination_spans(&source);
            assert_eq!(destinations.len(), 1, "{source:?}: {destinations:?}");
            assert_eq!(destinations[0].dest, "/réal#part");
            assert_eq!(&source[destinations[0].range.clone()], "/réal#part");
            let html = fragment(&source, true);
            assert!(
                html.contains("<!doctype [hidden](/hidden)\n**literal**>"),
                "{html}"
            );
            assert!(!html.contains("<strong>literal</strong>"), "{html}");
        }
    }
}

#[test]
fn non_ascii_or_nonletter_openers_remain_markdown_with_live_links() {
    for opener in ["<!1", "<!_", "<!é", "<! ", "<!\t"] {
        let source = format!("before {opener}[visible](/kept)> after");
        assert_eq!(
            parse_inlines(&source),
            [
                text(&format!("before {opener}")),
                link("visible", "/kept"),
                text("> after"),
            ],
            "{source:?}"
        );
        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 1, "{source:?}");
        assert_eq!(&source[destinations[0].range.clone()], "/kept");
        assert!(fragment(&source, false).contains("href=\"/kept\""));
    }
}

#[test]
fn unclosed_declarations_keep_live_markdown_and_closed_declarations_stop_at_first_angle() {
    for declaration in ["<!doctype", "<!Doctype", "<!DOCTYPE"] {
        let source = format!("before {declaration} [visible](/kept)");
        assert_eq!(
            parse_inlines(&source),
            [
                text(&format!("before {declaration} ")),
                link("visible", "/kept"),
            ],
            "{source:?}"
        );
    }
    let source = "before <!doctype [hidden](/hidden)> [visible](/kept)> after";
    assert_eq!(
        parse_inlines(source),
        [
            text("before "),
            Inline::Html("<!doctype [hidden](/hidden)>".into()),
            text(" "),
            link("visible", "/kept"),
            text("> after"),
        ]
    );
}

#[test]
fn lowercase_unclosed_declaration_scans_remain_bounded_and_recover_at_next_paragraph() {
    let hostile = format!("[discard](/discard) {}end", "<!doctype ".repeat(12_000));
    let source = format!("{hostile}\n\n[good](/good)");
    let document = parse_markdown(&source);
    assert_eq!(
        document.blocks,
        [
            Block::Paragraph(vec![text(&hostile)]),
            Block::Paragraph(vec![link("good", "/good")]),
        ]
    );
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 1, "{destinations:?}");
    assert_eq!(destinations[0].dest, "/good");
}
