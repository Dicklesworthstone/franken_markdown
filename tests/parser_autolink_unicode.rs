//! CommonMark absolute URIs exclude ASCII controls and spaces, not all Unicode
//! whitespace. Recognition must agree with nested-link and source ownership.
//!
//! Grammar: https://spec.commonmark.org/0.31.2/#autolinks
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, BookSourceLocation};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn link(destination: &str) -> Inline {
    Inline::Link {
        dest: destination.into(),
        title: None,
        content: vec![Inline::Text(destination.into())],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn uri_autolinks_keep_non_ascii_whitespace_and_controls_inside_the_target() {
    // Expected UTF-8 URL encoding is explicit, independent of the renderer.
    for (character, encoded) in [
        ('\u{00a0}', "%C2%A0"),
        ('\u{1680}', "%E1%9A%80"),
        ('\u{2003}', "%E2%80%83"),
        ('\u{2028}', "%E2%80%A8"),
        ('\u{2029}', "%E2%80%A9"),
        ('\u{202f}', "%E2%80%AF"),
        ('\u{3000}', "%E3%80%80"),
        ('\u{0085}', "%C2%85"),
        ('\u{009f}', "%C2%9F"),
    ] {
        let destination = format!("https://example.test/a{character}b?q=one&two=three");
        let source = format!("<{destination}>");
        assert_eq!(
            parse_inlines(&source),
            vec![link(&destination)],
            "U+{:04X}",
            character as u32,
        );
        assert_eq!(
            fragment(&source),
            format!(
                "<p><a href=\"https://example.test/a{encoded}b?q=one&amp;two=three\">https://example.test/a{character}b?q=one&amp;two=three</a></p>\n"
            ),
        );
    }
}

#[test]
fn valid_angle_autolinks_defeat_enclosing_links_and_stay_literal_in_code() {
    let destination = "https://example.test/a\u{00a0}b";
    let source = format!("[outer <{destination}>](/outside)");
    assert_eq!(
        parse_inlines(&source),
        vec![
            Inline::Text("[outer ".into()),
            link(destination),
            Inline::Text("](/outside)".into()),
        ],
    );
    assert_eq!(
        fragment(&source),
        "<p>[outer <a href=\"https://example.test/a%C2%A0b\">https://example.test/a\u{00a0}b</a>](/outside)</p>\n",
    );
    assert!(destination_spans(&source).is_empty());

    let code = format!("<{destination}>");
    assert_eq!(
        parse_inlines(&format!("`{code}`")),
        vec![Inline::Code(code)],
    );
    // Images flatten their descriptions, so a valid link in the description
    // remains visible alternative text and does not replace the image target.
    assert_eq!(
        parse_inlines(&format!("![<{destination}>](picture.png)")),
        vec![Inline::Image {
            dest: "picture.png".into(),
            title: None,
            alt: destination.into(),
        }],
    );
}

#[test]
fn autolink_unicode_does_not_steal_original_destination_or_navigation_ranges() {
    let source = "\u{feff}# Start\r\n\r\n> [outer <https://example.test/a\u{00a0}b>](/outside)\r\n> [tail](missing.md#keep)\r\n";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document);
    assert!(spanned.diagnostics.is_empty());

    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 1);
    let span = &destinations[0];
    assert_eq!(span.dest, "missing.md#keep");
    assert_eq!(&source[span.range.clone()], "missing.md#keep");

    let book = BookRenderer::new(&[BookInput {
        path: "start.md".into(),
        source: source.into(),
    }])
    .unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 1);
    let location = report.source_location(0, 0).unwrap();
    let BookSourceLocation::Mapped(ranges) = &location.source else {
        panic!("missing original-source location");
    };
    assert_eq!(ranges.len(), 1);
    assert_eq!(
        (ranges[0].start, ranges[0].end),
        (span.range.start, span.range.end),
    );

    // Shared native adapter parity; this is not generated-WASM execution.
    let options = HtmlOptions::default();
    let native = franken_markdown::render_html(source, &options).unwrap();
    let adapter = franken_markdown::wasm::render_html(source, &Default::default()).unwrap();
    assert_eq!(adapter.html(), Some(native.as_str()));
}

#[test]
fn ascii_exclusions_and_rendered_scheme_safety_remain_intact() {
    for character in [
        ' ', '\t', '\u{0001}', '\u{000b}', '\u{000c}', '\u{001f}', '\u{007f}',
    ] {
        let source = format!("<https://example.test/a{character}b>");
        assert_eq!(
            parse_inlines(&source),
            vec![Inline::Text(source.clone())],
            "ASCII U+{:04X}",
            character as u32,
        );
    }
    let dangerous = "javascript:alert\u{00a0}(1)";
    let source = format!("<{dangerous}>");
    assert_eq!(parse_inlines(&source), vec![link(dangerous)]);
    assert_eq!(fragment(&source), format!("<p>{dangerous}</p>\n"));

    // Source NUL is normalized before URI recognition, just as in prose/code.
    let source = "<https://example.test/a\0b>";
    assert_eq!(
        parse_markdown(source).blocks,
        vec![Block::Paragraph(vec![link(
            "https://example.test/a\u{fffd}b"
        )])],
    );
}
