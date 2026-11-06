//! Inline containers must preserve source newlines until token ownership is known.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::parse::destination_spans;
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, PdfOptions, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, render_pdf_document,
};

#[derive(Clone, Copy, Debug)]
enum Container {
    Definition,
    Footnote,
}

impl Container {
    fn source(self, body: &str) -> String {
        match self {
            Self::Definition => format!("Term\n: {}", body.replace('\n', "\n  ")),
            Self::Footnote => format!("[^note]: {}", body.replace('\n', "\n    ")),
        }
    }
}

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn document(source: &str) -> Document {
    let document = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, document);
    assert_eq!(parse_markdown_spanned(source).to_document(), document);
    document
}

fn body(document: &Document) -> &[Inline] {
    match &document.blocks[0] {
        Block::DefinitionList(items) => &items[0].definitions[0],
        Block::FootnoteDefinition { blocks, .. } => match &blocks[0] {
            Block::Paragraph(inlines) => inlines,
            other => panic!("expected inline note body, got {other:?}"),
        },
        other => panic!("expected inline container, got {other:?}"),
    }
}

fn html(document: &Document) -> String {
    let mut document = document.clone();
    if matches!(document.blocks[0], Block::FootnoteDefinition { .. }) {
        // Unreferenced notes are omitted from HTML. Give the parsed fixture a
        // normal body reference so its actual note renderer is exercised.
        document
            .blocks
            .push(Block::Paragraph(vec![Inline::FootnoteRef {
                id: "note".into(),
            }]));
    }
    franken_markdown::html::render_fragment(&document.blocks, &HtmlOptions::default())
}

#[test]
fn explicit_breaks_survive_every_container_line_and_source_line_ending() {
    for container in [Container::Definition, Container::Footnote] {
        for (first, second) in [("  ", "  "), ("\\", "  "), ("  ", "\\")] {
            for newline in ["\n", "\r\n", "\r"] {
                let source = container
                    .source(&format!("Café{first}\nSecond{second}\nThird"))
                    .replace('\n', newline);
                let document = document(&source);
                assert_eq!(
                    body(&document),
                    [
                        text("Café"),
                        Inline::HardBreak,
                        text("Second"),
                        Inline::HardBreak,
                        text("Third"),
                    ],
                    "{source:?}"
                );
                assert!(
                    html(&document).contains("Café<br>\nSecond<br>\nThird"),
                    "{source:?}"
                );
                let spanned = parse_markdown_spanned(&source);
                assert_eq!(spanned.blocks[0].span.slice(&source), Some(source.as_str()));
            }
        }
    }
}

#[test]
fn physical_line_breaks_cannot_become_link_or_image_destination_spaces() {
    for container in [Container::Definition, Container::Footnote] {
        for marker in ["", "!"] {
            let source = container.source(&format!("{marker}[bad](<docs/\nsecret.md>)"));
            let document = document(&source);
            assert_eq!(
                body(&document),
                [
                    text(&format!("{marker}[bad](<docs/")),
                    Inline::SoftBreak,
                    text("secret.md>)"),
                ],
                "a forbidden destination newline must stay visible: {source:?}"
            );
            assert!(destination_spans(&source).is_empty(), "{source:?}");
            let html = html(&document);
            assert!(html.contains("(&lt;docs/\nsecret.md&gt;)"), "{html}");
            assert!(!html.contains("href=\"docs/"), "{html}");
            assert!(!html.contains("src=\"docs/"), "{html}");
        }
    }
}

#[test]
fn code_spans_preserve_spaces_before_first_and_later_physical_newlines() {
    for container in [Container::Definition, Container::Footnote] {
        for (source_body, expected) in [
            ("`one \ntwo`", vec![Inline::Code("one  two".into())]),
            (
                "prefix\n`one \ntwo`",
                vec![
                    text("prefix"),
                    Inline::SoftBreak,
                    Inline::Code("one  two".into()),
                ],
            ),
        ] {
            let source = container.source(source_body);
            assert_eq!(body(&document(&source)), expected, "{source:?}");
        }
    }
}

#[test]
fn mathematics_and_html_keep_their_literal_physical_newlines() {
    for container in [Container::Definition, Container::Footnote] {
        let source = container.source("$x % comment\ny$");
        let parsed = document(&source);
        assert_eq!(body(&parsed), [Inline::Math("x % comment\ny".into())]);
        let html = html(&parsed);
        assert!(
            html.contains("<mi>y</mi>"),
            "the comment must end at its newline: {html}"
        );

        let source = container.source("before <span title=\"first\nsecond\">after</span>");
        assert_eq!(
            body(&document(&source)),
            [
                text("before "),
                Inline::Html("<span title=\"first\nsecond\">".into()),
                text("after"),
                Inline::Html("</span>".into()),
            ],
            "{source:?}"
        );
    }
}

#[test]
fn valid_multiline_titles_and_following_destinations_keep_exact_source_ownership() {
    for container in [Container::Definition, Container::Footnote] {
        for newline in ["\n", "\r\n", "\r"] {
            let source = container
                .source("[café](guide.md\n\"Guide\") and [bad](<docs/\nmissing.md>) [next](/kept)")
                .replace('\n', newline);
            let document = document(&source);
            assert!(
                matches!(&body(&document)[0], Inline::Link { dest, title, .. }
                if dest == "guide.md" && title.as_deref() == Some("Guide"))
            );
            let spans = destination_spans(&source);
            assert_eq!(
                spans
                    .iter()
                    .map(|span| span.dest.as_str())
                    .collect::<Vec<_>>(),
                ["guide.md", "/kept"],
                "{source:?}: {spans:?}"
            );
            for span in spans {
                assert_eq!(&source[span.range], span.dest, "{source:?}");
            }
        }
    }
}

#[test]
fn empty_definition_openers_and_plain_continuations_keep_their_existing_presentation() {
    assert_eq!(
        body(&document("Term\n:\n  **first**  \n  second")),
        [
            Inline::Strong(vec![text("first")]),
            Inline::HardBreak,
            text("second")
        ]
    );
    assert_eq!(
        body(&document("Term\n: First line\n  second line\n  third line")),
        [text("First line second line third line")]
    );
    assert_eq!(
        body(&document("[^note]: First line \n        extra indentation")),
        [text("First line     extra indentation")]
    );
}

#[test]
fn pdf_definitions_respect_hard_break_baselines_and_do_not_invent_links() {
    let parsed = document("Term\n: FIRSTLINE  \n  SECONDLINE\\\n  THIRDLINE");
    let report = franken_markdown::verify::verify_pdf(&parsed, &PdfOptions::default()).unwrap();
    let baseline = |needle: &str| {
        report
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .find(|run| run.text.contains(needle))
            .unwrap()
            .y
    };
    assert!(baseline("FIRSTLINE") > baseline("SECONDLINE"));
    assert!(baseline("SECONDLINE") > baseline("THIRDLINE"));

    for container in [Container::Definition, Container::Footnote] {
        let source = container.source("[bad](<docs/\nmissing.md>)");
        let pdf = render_pdf_document(&document(&source), &PdfOptions::default()).unwrap();
        assert!(
            !String::from_utf8_lossy(&pdf).contains("/Subtype /Link"),
            "a malformed source link must not acquire a PDF annotation: {source:?}"
        );
    }
}
