//! Bare URL/email speculation participates in the same source-preserving budget.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::{Block, Inline};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    HtmlOptions, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.to_owned(),
        title: None,
        content: vec![Inline::Text(label.to_owned())],
    }
}

#[test]
fn repeated_invalid_email_suffixes_preserve_the_complete_paragraph() {
    // An underscore admits another email candidate. Each rejected candidate
    // previously scanned the whole remaining suffix without charging the work
    // budget, even if the only '@' belonged to a later, unrelated word.
    for tail in ["@invalid", "@example.test_", " other@example.test"] {
        let source = format!("**prefix** {}{tail} suffix", "a_".repeat(4096));
        assert_eq!(parse_inlines(&source), vec![Inline::Text(source.clone())]);
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![Block::Paragraph(vec![Inline::Text(source)])]
        );
    }
}

#[test]
fn exhausted_email_scans_roll_back_source_authority_and_reset_at_paragraphs() {
    let hostile = format!("[discard](/discard) {}@invalid", "a_".repeat(4096));
    let source = format!("{hostile}\n\n[good](/ok) alice@example.test\n\n{hostile}");
    let document = parse_markdown(&source);
    assert_eq!(
        document.blocks,
        vec![
            Block::Paragraph(vec![Inline::Text(hostile.clone())]),
            Block::Paragraph(vec![
                link("good", "/ok"),
                Inline::Text(" ".into()),
                link("alice@example.test", "mailto:alice@example.test"),
            ]),
            Block::Paragraph(vec![Inline::Text(hostile)]),
        ]
    );
    assert_eq!(parse_markdown_profiled(&source).document, document);
    let spanned = parse_markdown_spanned(&source);
    assert_eq!(
        spanned
            .blocks
            .iter()
            .map(|block| &block.node)
            .collect::<Vec<_>>(),
        document.blocks.iter().collect::<Vec<_>>()
    );
    for block in &spanned.blocks {
        assert!(block.span.slice(&source).is_some());
    }
    let spans = destination_spans(&source);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].dest, "/ok");
    assert_eq!(&source[spans[0].range.clone()], "/ok");
}

#[test]
fn formatted_autolinks_keep_delimiter_and_punctuation_boundaries() {
    for (source, expected) in [
        (
            "*alice@example.test*",
            "<p><em><a href=\"mailto:alice@example.test\">alice@example.test</a></em></p>\n",
        ),
        (
            "**alice_smith+tag@example.test**.",
            "<p><strong><a href=\"mailto:alice_smith+tag@example.test\">alice_smith+tag@example.test</a></strong>.</p>\n",
        ),
        (
            "~~alice@example.test~~",
            "<p><del><a href=\"mailto:alice@example.test\">alice@example.test</a></del></p>\n",
        ),
        (
            "_https://example.test/a_b_",
            "<p><em><a href=\"https://example.test/a_b\">https://example.test/a_b</a></em></p>\n",
        ),
        (
            "~~https://example.test/a(b)~~.",
            "<p><del><a href=\"https://example.test/a(b)\">https://example.test/a(b)</a></del>.</p>\n",
        ),
        (
            "[alice@example.test](/contact)",
            "<p><a href=\"/contact\">alice@example.test</a></p>\n",
        ),
    ] {
        assert_eq!(
            franken_markdown::html::render_fragment(
                &parse_markdown(source).blocks,
                &HtmlOptions::default()
            ),
            expected,
            "{source}"
        );
    }
}

#[test]
fn angle_attempts_remain_literal_and_escaped_angles_allow_bare_links() {
    for address in ["https://example.test/path", "alice@example.test"] {
        let source = format!("< {address} >");
        assert_eq!(parse_inlines(&source), vec![Inline::Text(source.clone())]);
        let destination = if address.contains('@') {
            format!("mailto:{address}")
        } else {
            address.to_owned()
        };
        assert_eq!(
            parse_inlines(&format!("\\< {address} >")),
            vec![
                Inline::Text("< ".into()),
                link(address, &destination),
                Inline::Text(" >".into()),
            ]
        );
    }
}

#[test]
fn long_valid_addresses_and_many_ordinary_autolinks_retain_their_structure() {
    let address = format!("{}z@example.test", "a_".repeat(16_000));
    assert_eq!(
        parse_inlines(&address),
        vec![link(&address, &format!("mailto:{address}"))]
    );

    let url = format!("https://example.test/{}end", "a_".repeat(16_000));
    assert_eq!(parse_inlines(&url), vec![link(&url, &url)]);

    let source =
        "alice_smith@example.test https://example.test/a(b) www.example.test ".repeat(4096);
    let inlines = parse_inlines(&source);
    assert_eq!(
        inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::Link { .. }))
            .count(),
        4096 * 3
    );
}
