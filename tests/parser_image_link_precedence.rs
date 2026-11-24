//! Bracket-link precedence survives image descriptions being flattened to text.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::parse::{destination_spans, parse_inlines};
use franken_markdown::{
    Block, Document, HtmlOptions, Inline, PdfOptions, parse_markdown, parse_markdown_profiled,
    parse_markdown_spanned, render_pdf, render_pdf_document,
};

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}

fn image(alt: &str) -> Inline {
    Inline::Image {
        dest: "image.png".into(),
        title: None,
        alt: alt.into(),
    }
}

fn linked(content: Vec<Inline>, dest: &str) -> Inline {
    Inline::Link {
        dest: dest.into(),
        title: None,
        content,
    }
}

fn document(source: &str) -> Document {
    let parsed = parse_markdown(source);
    assert_eq!(parse_markdown_profiled(source).document, parsed);
    let spanned = parse_markdown_spanned(source);
    assert_eq!(spanned.to_document(), parsed);
    assert!(spanned.diagnostics.is_empty(), "{source:?}");
    parsed
}

fn fragment(source: &str) -> String {
    franken_markdown::html::render_fragment(&document(source).blocks, &HtmlOptions::default())
}

#[test]
fn bracket_links_inside_image_labels_deactivate_the_outer_link() {
    // The normative rule is the final step of CommonMark 0.31.2's
    // "look for link or image" algorithm: an accepted link deactivates all
    // earlier '[' openers, even when an intervening image flattens its label.
    for label in [
        "alt [inner](/inside)",
        "alt **[inner](/inside)**",
        "alt [inner](/inside \"caption ` [ ]\")",
        "alt [^inner](/inside)",
    ] {
        let expected_alt = if label.contains("^inner") {
            "alt ^inner"
        } else {
            "alt inner"
        };
        let source = format!("[![{label}](image.png)](/outside)");
        let expected = vec![text("["), image(expected_alt), text("](/outside)")];
        assert_eq!(parse_inlines(&source), expected, "{source:?}");
        assert_eq!(document(&source).blocks, [Block::Paragraph(expected)]);
        assert_eq!(
            fragment(&source),
            format!("<p>[<img src=\"image.png\" alt=\"{expected_alt}\">](/outside)</p>\n")
        );
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 1, "{source:?}: {spans:?}");
        assert_eq!(spans[0].dest, "image.png");
    }
}

#[test]
fn reference_links_and_reference_images_keep_the_same_precedence() {
    for inner in ["[inner][ref]", "[inner][]", "[inner]"] {
        for image_suffix in ["(image.png)", "[picture]"] {
            let source = format!(
                "[![alt {inner}]{image_suffix}](/outside)\n\n\
                 [ref]: /inside\n[inner]: /inside\n[picture]: image.png\n"
            );
            assert_eq!(
                document(&source).blocks,
                [Block::Paragraph(vec![
                    text("["),
                    image("alt inner"),
                    text("](/outside)"),
                ])],
                "{source:?}"
            );
            assert!(!fragment(&source).contains("href=\"/outside\""));
        }
    }
    // The rejected outer opener does not consume its following reference label.
    // That label remains a legitimate standalone shortcut reference.
    let source = "[![alt [inner][ref]][picture]][outer]\n\n\
                  [ref]: /inside\n[picture]: image.png\n[outer]: /outside\n";
    assert_eq!(
        document(source).blocks,
        [Block::Paragraph(vec![
            text("["),
            image("alt inner"),
            text("]"),
            linked(vec![text("outer")], "/outside"),
        ])]
    );
}

#[test]
fn nested_images_preserve_bracket_links_but_not_autolink_containment() {
    let source = "[![outer ![alt [inner](/inside)](nested.png)](image.png)](/outside)";
    assert_eq!(
        document(source).blocks,
        [Block::Paragraph(vec![
            text("["),
            image("outer alt inner"),
            text("](/outside)"),
        ])]
    );
    for (label, expected_alt) in [
        ("plain", "plain"),
        ("<https://inside.example>", "https://inside.example"),
        ("<inside@example.com>", "inside@example.com"),
        ("![nested](nested.png)", "nested"),
        (
            "![<https://inside.example>](nested.png)",
            "https://inside.example",
        ),
    ] {
        for image_suffix in ["(image.png)", "[picture]"] {
            let source = format!("[![{label}]{image_suffix}](/outside)\n\n[picture]: image.png\n");
            assert_eq!(
                document(&source).blocks,
                [Block::Paragraph(vec![linked(
                    vec![image(expected_alt)],
                    "/outside"
                )])],
                "{source:?}"
            );
        }
    }
}

#[test]
fn rejected_outer_destinations_and_titles_remain_ordinary_source() {
    let source = "[![alt [inner](/inside)](image.png)](/outside \"[tail](/tail)\")";
    assert_eq!(
        document(source).blocks,
        [Block::Paragraph(vec![
            text("["),
            image("alt inner"),
            text("](/outside \""),
            linked(vec![text("tail")], "/tail"),
            text("\")"),
        ])]
    );
    let spans = destination_spans(source);
    assert_eq!(
        spans
            .iter()
            .map(|span| span.dest.as_str())
            .collect::<Vec<_>>(),
        ["image.png", "/tail"]
    );
    for span in spans {
        assert_eq!(&source[span.range], span.dest);
    }
}

#[test]
fn original_unicode_destination_spans_exclude_flattened_and_rejected_links() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = format!(
            "\u{feff}# Pré{newline}{newline}\
             [![lien [café](missing.md#part)](<images/café].png>)](discard.md#x) \
             [réel](kept.md#élément){newline}"
        );
        document(&source);
        let spans = destination_spans(&source);
        assert_eq!(spans.len(), 2, "{source:?}: {spans:?}");
        assert_eq!(spans[0].dest, "images/café].png");
        assert_eq!(&source[spans[0].range.clone()], "<images/café].png>");
        assert_eq!(spans[1].dest, "kept.md#élément");
        assert_eq!(&source[spans[1].range.clone()], "kept.md#élément");
        assert_eq!(
            &source[spans[1].fragment_range(&source).unwrap()],
            "élément"
        );
    }
}

#[test]
fn label_provenance_does_not_escape_to_later_or_cached_paragraphs() {
    let rejected = "[![alt [inner](/inside)](image.png)](/outside)";
    let valid = "[![plain](image.png)](/valid) [after](/after)";
    let source = format!(
        "[before](/before) {rejected} [after](/after)\n\n\
         {valid}\n\n{rejected}\n\n{valid}\n\n{rejected}\n\n{valid}"
    );
    let parsed = document(&source);
    assert_eq!(parsed.blocks.len(), 6);
    let expected = Block::Paragraph(vec![
        linked(vec![image("plain")], "/valid"),
        text(" "),
        linked(vec![text("after")], "/after"),
    ]);
    for index in [1, 3, 5] {
        assert_eq!(parsed.blocks[index], expected);
    }
    let rendered = fragment(&source);
    assert_eq!(rendered.matches("href=\"/valid\"").count(), 3);
    assert_eq!(rendered.matches("href=\"/after\"").count(), 4);
    assert!(!rendered.contains("href=\"/outside\""));
    assert!(rendered.contains("href=\"/before\""));
}

#[test]
fn book_validation_and_pdf_consume_the_corrected_shared_ast() {
    let source = "[![alt [inner](missing-inner.md)](image.png)](missing-outer.md)";
    let expected = Document {
        blocks: vec![Block::Paragraph(vec![
            text("["),
            image("alt inner"),
            text("](missing-outer.md)"),
        ])],
    };
    assert_eq!(document(source), expected);
    let book = BookRenderer::new(&[BookInput {
        path: "chapter.md".into(),
        source: source.into(),
    }])
    .unwrap();
    let report = book.validate_links_with_sources().unwrap();
    assert_eq!(report.report().finding_count(), 0);
    assert_eq!(report.report().chapters[0].checked, 0);
    let options = PdfOptions::default();
    assert_eq!(
        render_pdf(source, &options).unwrap(),
        render_pdf_document(&expected, &options).unwrap()
    );
    // This exercises the shared native browser adapter, not generated WASM.
    let browser = franken_markdown::wasm::render_html(source, &Default::default()).unwrap();
    assert_eq!(
        browser.html(),
        Some(
            franken_markdown::render_html(source, &HtmlOptions::default())
                .unwrap()
                .as_str()
        )
    );
}
