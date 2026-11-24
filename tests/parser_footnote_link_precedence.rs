//! Explicit inline/reference links take precedence over footnote references.
//!
//! GitHub's reference parser recognizes footnotes only after link matching has
//! failed: github/cmark-gfm, commit 7fe8904ee31fa24b772ae565c4908b1394850de6,
//! src/inlines.c::handle_close_bracket.
//! https://github.com/github/cmark-gfm/blob/7fe8904ee31fa24b772ae565c4908b1394850de6/src/inlines.c
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer, BookSourceLocation};
use franken_markdown::parse::{
    ReferenceLabelForm, destination_spans, parse_inlines, reference_label_spans,
};
use franken_markdown::{
    Block, HtmlOptions, Inline, parse_markdown, parse_markdown_profiled, parse_markdown_spanned,
};

fn link(label: &str, dest: &str, title: Option<&str>) -> Inline {
    Inline::Link {
        dest: dest.into(),
        title: title.map(str::to_owned),
        content: vec![Inline::Text(label.into())],
    }
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&parse_markdown(source).blocks, &HtmlOptions::default())
}

#[test]
fn caret_labels_keep_explicit_destinations_and_titles() {
    for label in ["^note", "^é中🙂", "^a\"b"] {
        for (target, dest, title) in [
            ("(/chosen)", "/chosen", None),
            ("(</route with spaces>)", "/route with spaces", None),
            (
                "(/route_(v2) \"Title &amp; more\")",
                "/route_(v2)",
                Some("Title & more"),
            ),
            ("()", "", None),
        ] {
            let source = format!("[{label}]{target}");
            assert_eq!(
                parse_inlines(&source),
                vec![link(label, dest, title)],
                "{source:?}"
            );
            let destinations = destination_spans(&source);
            if dest.is_empty() {
                // Empty targets are valid links but have no editable token.
                assert!(destinations.is_empty(), "{source:?}");
            } else {
                assert_eq!(destinations.len(), 1, "{source:?}");
                assert_eq!(destinations[0].dest, dest, "{source:?}");
            }
        }
    }
}

#[test]
fn caret_links_win_even_when_a_real_note_has_the_same_identifier() {
    let source = "[^note](/chosen) then actual[^note].\n\n[^note]: Note body.";
    let document = parse_markdown(source);
    assert_eq!(
        document.blocks[0],
        Block::Paragraph(vec![
            link("^note", "/chosen", None),
            Inline::Text(" then actual".into()),
            Inline::FootnoteRef { id: "note".into() },
            Inline::Text(".".into()),
        ])
    );
    let html = franken_markdown::render_html(source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("<a href=\"/chosen\">^note</a> then actual"));
    assert_eq!(html.matches("class=\"footnote-ref\"").count(), 1);
    assert!(html.contains("Note body."));
}

#[test]
fn caret_labels_resolve_full_references_and_keep_editable_source_tokens() {
    let source = "[^é中][STRASSE] and note[^é中].\n\n[Straße]: /chosen \"Title\"\n\n[^é中]: body";
    let document = parse_markdown(source);
    assert!(matches!(&document.blocks[0], Block::Paragraph(inlines)
        if inlines[0] == link("^é中", "/chosen", Some("Title"))));
    let labels = reference_label_spans(source);
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0].form, ReferenceLabelForm::Full);
    assert_eq!(labels[0].key, "strasse");
    assert_eq!(&source[labels[0].label.clone().unwrap()], "STRASSE");
    assert_eq!(labels[1].form, ReferenceLabelForm::Definition);
    assert_eq!(&source[labels[1].label.clone().unwrap()], "Straße");

    // Whitespace excludes a footnote identifier, but is legal in an ordinary
    // reference label. Both collapsed and shortcut references retain it.
    for use_site in ["[^Two Words][]", "[^Two Words]"] {
        let source = format!("{use_site}\n\n[^two words]: /ordinary-reference");
        assert_eq!(
            parse_markdown(&source).blocks,
            vec![Block::Paragraph(vec![link(
                "^Two Words",
                "/ordinary-reference",
                None,
            )])]
        );
    }
}

#[test]
fn nested_caret_links_defeat_outer_links_but_images_keep_their_target() {
    for (inner, definitions) in [
        ("[^note](/inside)", ""),
        ("[^note][ref]", "\n\n[ref]: /inside"),
    ] {
        let source = format!("[outside {inner}](/outside){definitions}");
        let html = fragment(&source);
        assert!(
            html.contains("[outside <a href=\"/inside\">^note</a>](/outside)"),
            "{source:?}: {html}"
        );
        assert!(!html.contains("href=\"/outside\""));
        let image = format!("![outside {inner}](picture.png){definitions}");
        assert!(fragment(&image).contains("<img src=\"picture.png\" alt=\"outside ^note\">"));
    }
    assert_eq!(
        parse_inlines("[outside[^note]](/outside)"),
        vec![Inline::Link {
            dest: "/outside".into(),
            title: None,
            content: vec![
                Inline::Text("outside".into()),
                Inline::FootnoteRef { id: "note".into() },
            ],
        }]
    );
}

#[test]
fn actual_citations_keep_opaque_punctuation_without_stealing_outer_brackets() {
    for id in ["a\"b", "a<b>", "a<https://e>", "a`", "a$b"] {
        let source = format!("[outside[^{id}]](/chosen) trailing ` $");
        let inlines = parse_inlines(&source);
        assert_eq!(
            inlines,
            vec![
                Inline::Link {
                    dest: "/chosen".into(),
                    title: None,
                    content: vec![
                        Inline::Text("outside".into()),
                        Inline::FootnoteRef { id: id.into() },
                    ],
                },
                Inline::Text(" trailing ` $".into()),
            ],
            "{source:?}"
        );
        let destinations = destination_spans(&source);
        assert_eq!(destinations.len(), 1, "{source:?}");
        assert_eq!(destinations[0].dest, "/chosen", "{source:?}");
    }
}

#[test]
fn rejected_targets_preserve_citations_and_do_not_acquire_destination_authority() {
    for tail in ["", "(/unterminated", "(<bad\nurl>)", "[missing]", "[]"] {
        let source = format!("[^note]{tail}");
        let inlines = parse_inlines(&source);
        assert!(
            matches!(&inlines[0], Inline::FootnoteRef { id } if id == "note"),
            "{source:?}: {inlines:?}"
        );
        assert!(destination_spans(&source).is_empty(), "{source:?}");
        assert!(reference_label_spans(&source).is_empty(), "{source:?}");
    }
    assert_eq!(
        fragment("[^missing](/unterminated"),
        "<p>[^missing](/unterminated</p>\n"
    );
    assert_eq!(fragment("[^note](javascript:alert(1))"), "<p>^note</p>\n");
}

#[test]
fn code_escaped_brackets_and_html_attributes_do_not_create_caret_links() {
    for source in [
        "`[^note](/hidden)`",
        "\\[^note](/hidden)",
        "<span title='[^note](/hidden)'>visible</span>",
    ] {
        assert!(destination_spans(source).is_empty(), "{source:?}");
        assert!(!fragment(source).contains("href=\"/hidden\""), "{source:?}");
    }
    // Code retains ownership even when its closing delimiter follows a
    // bracket that resembles the end of a caret-label link.
    for (source, code) in [
        (
            "[^before `inside](/hidden)` after](/chosen)",
            "inside](/hidden)",
        ),
        ("[^a`]` more](/chosen)", "]"),
    ] {
        let inlines = parse_inlines(source);
        assert!(
            matches!(&inlines[..], [Inline::Link { dest, content, .. }]
            if dest == "/chosen" && content.iter().any(|inline|
                matches!(inline, Inline::Code(text) if text == code))),
            "{source:?}: {inlines:?}"
        );
        assert_eq!(
            destination_spans(source)
                .iter()
                .map(|span| span.dest.as_str())
                .collect::<Vec<_>>(),
            ["/chosen"]
        );
    }
}

#[test]
fn container_caret_links_keep_original_ranges_and_publication_findings() {
    let source =
        "\u{feff}# Start\r\n\r\n> [^é中](missing.md#keep)\r\n\r\n- [^next](other.md#next)\r\n";
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), document);
    assert!(spanned.diagnostics.is_empty());
    let destinations = destination_spans(source);
    assert_eq!(destinations.len(), 2);
    for (span, expected) in destinations
        .iter()
        .zip(["missing.md#keep", "other.md#next"])
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
    assert_eq!(report.report().finding_count(), 2);
    let mut findings = Vec::new();
    for index in 0..2 {
        let location = report.source_location(0, index).unwrap();
        let BookSourceLocation::Mapped(ranges) = &location.source else {
            panic!("expected source range");
        };
        assert_eq!(ranges.len(), 1);
        findings.push((ranges[0].start, ranges[0].end));
    }
    findings.sort_unstable();
    assert_eq!(
        findings,
        destinations
            .iter()
            .map(|span| (span.range.start, span.range.end))
            .collect::<Vec<_>>()
    );
    // Native Rust adapter parity, not a generated-WASM execution claim.
    let native = franken_markdown::render_html(source, &HtmlOptions::default()).unwrap();
    let adapter = franken_markdown::wasm::render_html(source, &Default::default()).unwrap();
    assert_eq!(adapter.html(), Some(native.as_str()));
}

#[test]
fn repeated_caret_links_remain_bounded_and_do_not_turn_into_citations() {
    let source = "[^note](/chosen) ".repeat(4096);
    let inlines = parse_inlines(&source);
    assert_eq!(
        inlines
            .iter()
            .filter(|inline| matches!(inline, Inline::Link { .. }))
            .count(),
        4096
    );
    assert!(
        !inlines
            .iter()
            .any(|inline| matches!(inline, Inline::FootnoteRef { .. }))
    );
    let hostile = format!("{}leaf{}", "[^".repeat(256), "](/target)".repeat(256));
    assert_eq!(parse_inlines(&hostile), vec![Inline::Text(hostile.clone())]);
    assert!(destination_spans(&hostile).is_empty());
}
