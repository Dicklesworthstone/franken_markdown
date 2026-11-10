//! GFM autolinks must own their complete target and release trailing prose.
//!
//! Exact protocol/resource and entity-suffix examples come from GFM 6.9:
//! https://github.github.com/gfm/#autolinks-extension-
//! No reference parser is a runtime or test dependency.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, BookSourceLocation};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, HtmlOptions, Inline, PdfOptions, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn link(label: &str, destination: &str) -> Inline {
    Inline::Link {
        dest: destination.into(),
        title: None,
        content: vec![text(label)],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn official_gfm_entity_suffix_stays_outside_the_url() {
    // GFM example 627: the internal query separator stays literal in the URL;
    // the terminal entity-shaped suffix becomes ordinary prose instead.
    assert_eq!(
        fragment(
            "www.google.com/search?q=commonmark&hl=en\n\nwww.google.com/search?q=commonmark&hl;"
        ),
        concat!(
            "<p><a href=\"http://www.google.com/search?q=commonmark&amp;hl=en\">www.google.com/search?q=commonmark&amp;hl=en</a></p>\n",
            "<p><a href=\"http://www.google.com/search?q=commonmark\">www.google.com/search?q=commonmark</a>&amp;hl;</p>\n",
        )
    );
}

#[test]
fn real_and_unknown_entity_suffixes_return_to_the_prose_tokenizer() {
    for (source_prefix, destination) in [
        ("https://example.test/", "https://example.test/"),
        ("HTTP://example.test/", "HTTP://example.test/"),
        ("www.example.test/", "http://www.example.test/"),
    ] {
        for (encoded, decoded) in [
            ("&copy;", "©"),
            ("&amp;", "&"),
            ("&frac34;", "¾"),
            ("&NotEqualTilde;", "≂̸"),
            ("&unknown9;", "&unknown9;"),
            ("&123;", "&123;"),
        ] {
            let source = format!("{source_prefix}{encoded}");
            assert_eq!(
                parse_inlines(&source),
                vec![link(source_prefix, destination), text(decoded)],
                "{source:?}"
            );
        }
    }
}

#[test]
fn punctuation_and_delimiters_cannot_drag_an_entity_into_the_link() {
    for (source, expected) in [
        (
            "**https://example.test/a&copy;**.",
            "<p><strong><a href=\"https://example.test/a\">https://example.test/a</a>©</strong>.</p>\n",
        ),
        (
            "(https://example.test/a&copy;).",
            "<p>(<a href=\"https://example.test/a\">https://example.test/a</a>©).</p>\n",
        ),
        (
            "\"https://example.test/a&copy;\"",
            "<p>\"<a href=\"https://example.test/a\">https://example.test/a</a>©\"</p>\n",
        ),
        (
            "https://example.test/a&copy;&amp;",
            "<p><a href=\"https://example.test/a\">https://example.test/a</a>©&amp;</p>\n",
        ),
        (
            "https://example.test/a(b))&copy;.)",
            "<p><a href=\"https://example.test/a(b)\">https://example.test/a(b)</a>)©.)</p>\n",
        ),
    ] {
        assert_eq!(fragment(source), expected, "{source:?}");
    }
}

#[test]
fn internal_entities_numeric_references_and_explicit_targets_keep_literal_url_ownership() {
    for (source, destination, tail) in [
        (
            "https://example.test/?a=1&amp;b=2",
            "https://example.test/?a=1&amp;b=2",
            "",
        ),
        (
            "https://example.test/&#123;",
            "https://example.test/&#123",
            ";",
        ),
        (
            "https://example.test/&a-b;",
            "https://example.test/&a-b",
            ";",
        ),
        ("https://example.test/&;", "https://example.test/&", ";"),
        (
            "https://example.test/a\\&copy;",
            "https://example.test/a\\",
            "©",
        ),
        (
            "https://example.test/a(b)&copy;",
            "https://example.test/a(b)",
            "©",
        ),
    ] {
        let mut expected = vec![link(destination, destination)];
        if !tail.is_empty() {
            expected.push(text(tail));
        }
        assert_eq!(parse_inlines(source), expected, "{source:?}");
    }
    assert_eq!(
        parse_inlines("<https://example.test/&copy;>"),
        vec![link(
            "https://example.test/&copy;",
            "https://example.test/&copy;"
        )]
    );
    assert_eq!(
        parse_inlines("[chosen](https://example.test/&copy;)"),
        vec![link("chosen", "https://example.test/©")]
    );
}

#[test]
fn official_mailto_and_xmpp_protocols_keep_the_complete_scheme_and_resource() {
    // GFM examples 633–635. An extra resource slash remains ordinary text.
    for source in [
        "mailto:foo@bar.baz",
        "mailto:a.b-c_d@a.b",
        "xmpp:foo@bar.baz",
        "xmpp:foo@bar.baz/txt",
        "xmpp:foo@bar.baz/txt@bin",
        "xmpp:foo@bar.baz/txt@bin.com",
    ] {
        assert_eq!(parse_inlines(source), vec![link(source, source)]);
        assert_eq!(
            fragment(source),
            format!("<p><a href=\"{source}\">{source}</a></p>\n")
        );
    }
    for (source, destination, tail) in [
        ("mailto:a.b-c_d@a.b.", "mailto:a.b-c_d@a.b", "."),
        ("mailto:a.b-c_d@a.b/", "mailto:a.b-c_d@a.b", "/"),
        ("xmpp:foo@bar.baz.", "xmpp:foo@bar.baz", "."),
        ("xmpp:foo@bar.baz/txt/bin", "xmpp:foo@bar.baz/txt", "/bin"),
        ("xmpp:foo@bar.baz/txt...", "xmpp:foo@bar.baz/txt", "..."),
        ("xmpp:foo@bar.baz/", "xmpp:foo@bar.baz", "/"),
        ("xmpp:foo@bar.baz/猫", "xmpp:foo@bar.baz", "/猫"),
    ] {
        assert_eq!(
            parse_inlines(source),
            vec![link(destination, destination), text(tail)],
            "{source:?}"
        );
    }
}

#[test]
fn protocol_autolinks_respect_case_boundaries_invalid_domains_and_literal_contexts() {
    for source in ["MAILTO:User@Example.TEST", "XmPp:User@Example.TEST/Phone"] {
        assert_eq!(parse_inlines(source), vec![link(source, source)]);
        assert_eq!(
            parse_inlines(&format!("({source})")),
            vec![text("("), link(source, source), text(")")]
        );
    }
    for source in [
        "xmailto:foo@bar.baz",
        "prefixxmpp:foo@bar.baz",
        "mailto:foo@localhost",
        "mailto:a.b-c_d@a.b-",
        "mailto:a.b-c_d@a.b_",
        "xmpp:foo@bar.baz-/txt",
        "xmpp:foo@bar..baz/txt",
        "mailto:@bar.baz",
        "mailto:",
        "xmpp:",
        "< mailto:foo@bar.baz >",
        "< xmpp:foo@bar.baz/txt >",
    ] {
        assert_eq!(parse_inlines(source), vec![text(source)], "{source:?}");
    }
    let label = "mailto:team@example.test xmpp:team@example.test/chat";
    assert_eq!(
        parse_inlines(&format!("`{label}`")),
        vec![Inline::Code(label.into())]
    );
    assert_eq!(
        parse_inlines(&format!("[{label}](/chosen)")),
        vec![link(label, "/chosen")]
    );
    assert_eq!(
        parse_markdown(&format!("[{label}][ref]\n\n[ref]: /chosen")).blocks,
        vec![Block::Paragraph(vec![link(label, "/chosen")])]
    );
    assert_eq!(
        parse_inlines("mailto\\:foo@bar.baz"),
        vec![text("mailto:foo@bar.baz")]
    );
    assert_eq!(
        fragment("*mailto:team@example.test* ~~xmpp:team@example.test/chat~~"),
        "<p><em><a href=\"mailto:team@example.test\">mailto:team@example.test</a></em> <del><a href=\"xmpp:team@example.test/chat\">xmpp:team@example.test/chat</a></del></p>\n"
    );
}

#[test]
fn autolinks_do_not_capture_neighboring_editable_destination_or_navigation_ranges() {
    let source = "\u{feff}# Start\r\n\r\n> café https://example.test/a&copy; [one](missing.md#one)\r\n\r\n- MAILTO:team@example.test [two](missing.md#two)\r\n\r\n- xmpp:team@example.test/chat [three](missing.md#three)\r\n";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document);
    assert!(spanned.diagnostics.is_empty());
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 3);
    for (span, expected) in
        destinations
            .iter()
            .zip(["missing.md#one", "missing.md#two", "missing.md#three"])
    {
        assert_eq!(span.dest, expected);
        assert_eq!(&source[span.range.clone()], expected);
    }
    let book = BookRenderer::new(&[BookInput {
        path: "start.md".into(),
        source: source.into(),
    }])
    .unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 3);
    for (index, span) in destinations.iter().enumerate() {
        let location = report.source_location(0, index).unwrap();
        let BookSourceLocation::Mapped(ranges) = &location.source else {
            panic!("missing original-source location")
        };
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            (ranges[0].start, ranges[0].end),
            (span.range.start, span.range.end)
        );
    }
    // This checks the shared native adapter, not a generated WASM runtime.
    let wasm = franken_markdown::wasm::render_html(source, &Default::default()).unwrap();
    assert_eq!(
        wasm.html(),
        Some(
            franken_markdown::render_html(source, &HtmlOptions::default())
                .unwrap()
                .as_str()
        )
    );
}

#[test]
fn long_entity_tails_and_protocol_resources_keep_linear_bounded_source_ownership() {
    let tail = "&unknown;".repeat(16_000);
    let source = format!("https://example.test/{tail}");
    assert_eq!(
        parse_inlines(&source),
        vec![
            link("https://example.test/", "https://example.test/"),
            text(&tail)
        ]
    );
    let address = format!("mailto:{}z@example.test", "a_".repeat(16_000));
    assert_eq!(parse_inlines(&address), vec![link(&address, &address)]);
    let resource = format!("xmpp:team@example.test/{}end", "a@b.".repeat(16_000));
    assert_eq!(parse_inlines(&resource), vec![link(&resource, &resource)]);

    let hostile = format!("[discard](/discard) mailto:{}@invalid", "a_".repeat(4096));
    let source = format!("{hostile}\n\n[kept](/kept) mailto:team@example.test\n\n{hostile}");
    let parsed = parse_markdown_spanned(&source);
    assert_eq!(
        parsed.blocks[0].node,
        Block::Paragraph(vec![text(&hostile)])
    );
    assert_eq!(
        parsed.blocks[2].node,
        Block::Paragraph(vec![text(&hostile)])
    );
    let destinations = destination_spans(&source);
    assert_eq!(destinations.len(), 1);
    assert_eq!(destinations[0].dest, "/kept");
    assert_eq!(&source[destinations[0].range.clone()], "/kept");
    assert!(fragment(&source).contains("href=\"mailto:team@example.test\""));
}

#[test]
fn protocol_autolinks_keep_each_output_formats_existing_url_policy() {
    let source = "mailto:team@example.test xmpp:team@example.test/chat https://example.test/&copy; [unsafe](javascript:alert(1))";
    let html = fragment(source);
    assert!(html.contains("href=\"mailto:team@example.test\""));
    assert!(html.contains("href=\"xmpp:team@example.test/chat\""));
    assert!(!html.contains("href=\"javascript:"));
    let pdf = franken_markdown::render_pdf(source, &PdfOptions::default()).unwrap();
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("/URI (mailto:team@example.test)"));
    assert!(raw.contains("/URI (https://example.test/)"));
    assert!(!raw.contains("/URI (xmpp:"));
    assert!(!raw.contains("/URI (javascript:"));
}
