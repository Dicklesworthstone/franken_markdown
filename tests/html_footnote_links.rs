//! Footnote marks preserve link ownership and exact fragment identifiers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, Document, HtmlOptions, Inline, parse_markdown, render_html_document,
    render_html_document_to_writer,
};

fn options() -> HtmlOptions {
    HtmlOptions {
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    }
}

fn assert_no_nested_anchors(html: &str) {
    let mut open = false;
    for tag in html.split('<').skip(1) {
        if tag.starts_with("a ") || tag.starts_with("a>") {
            assert!(!open, "nested anchor in {html}");
            open = true;
        } else if tag.starts_with("/a>") {
            assert!(open, "unmatched anchor close in {html}");
            open = false;
        }
    }
    assert!(!open, "unclosed anchor in {html}");
}

fn render(source: &str) -> String {
    let doc = parse_markdown(source);
    render_document(&doc)
}

fn render_document(doc: &Document) -> String {
    let opts = options();
    let html = render_html_document(doc, &opts).unwrap();
    let mut streamed = Vec::new();
    let count = render_html_document_to_writer(doc, &opts, &mut streamed).unwrap();
    assert_eq!(count, html.len());
    assert_eq!(streamed, html.as_bytes());
    assert_no_nested_anchors(&html);

    let fragment = franken_markdown::html::render_fragment(&doc.blocks, &opts);
    assert_no_nested_anchors(&fragment);
    assert!(html.contains(&fragment));
    html
}

#[test]
fn citation_in_a_markdown_link_keeps_the_enclosing_destination_and_return_anchor() {
    let html = render(
        "[Before **claim[^note]** after](/source \"Source\") outside[^note].\n\n\
         [^note]: Supporting evidence.\n",
    );
    assert!(html.contains(
        "<a href=\"/source\" title=\"Source\">Before <strong>claim\
         <sup class=\"footnote-ref\" id=\"fnref-1\">1</sup></strong> after</a>",
    ));
    assert!(html.contains("outside<sup class=\"footnote-ref\"><a href=\"#fn-note\">1</a></sup>"));
    assert_eq!(html.matches("id=\"fnref-1\"").count(), 1);
    assert_eq!(html.matches("<li id=\"fn-note\"").count(), 1);
    assert!(html.contains("href=\"#fnref-1\">"));
    assert!(html.contains("Supporting evidence."));
}

#[test]
fn unsafe_outer_links_do_not_disable_valid_citations_or_later_links() {
    let html = render(
        "[Claim[^a]](javascript:alert) [Safe[^b]](/safe) after[^a] [Last](/last).\n\n\
         [^a]: First note.\n\n[^b]: Second note.\n",
    );
    assert!(!html.contains("href=\"javascript:"));
    assert!(
        html.contains(
            "Claim<sup class=\"footnote-ref\" id=\"fnref-1\"><a href=\"#fn-a\">1</a></sup>",
        )
    );
    assert!(
        html.contains(
            "<a href=\"/safe\">Safe<sup class=\"footnote-ref\" id=\"fnref-2\">2</sup></a>",
        )
    );
    assert!(html.contains("<a href=\"/last\">Last</a>"));
    assert_eq!(html.matches("class=\"footnote-backref\"").count(), 2);
}

#[test]
fn sanitized_html_anchors_own_their_note_marks_and_nested_markdown_labels() {
    let html = render(
        "<a href=\"/source\">claim[^a] **more[^b]** [label](/inner)</a> outside[^a].\n\n\
         [^a]: First note.\n\n[^b]: Second note.\n",
    );
    assert!(
        html.contains(
            "<a href=\"/source\">claim<sup class=\"footnote-ref\" id=\"fnref-1\">1</sup>",
        )
    );
    assert!(
        html.contains("<strong>more<sup class=\"footnote-ref\" id=\"fnref-2\">2</sup></strong>")
    );
    assert!(!html.contains("href=\"/inner\""));
    assert!(html.contains(" label</a> outside<sup class=\"footnote-ref\"><a href=\"#fn-a\">"));
}

#[test]
fn sanitized_anchors_inside_markdown_and_raw_links_cannot_steal_outer_text() {
    for source in [
        "[before <a href=\"/inner\">inside[^a]</a> after](/outer) tail[^a].\n\n\
         [^a]: Supporting note.\n",
        "<a href=\"/outer\">before <a href=\"/inner\">inside[^a]</a> after</a> tail[^a].\n\n\
         [^a]: Supporting note.\n",
    ] {
        let html = render(source);
        assert!(html.contains(
            "<a href=\"/outer\">before inside<sup class=\"footnote-ref\" id=\"fnref-1\">1</sup> after</a>",
        ));
        assert!(!html.contains("href=\"/inner\""));
        assert!(html.contains(" tail<sup class=\"footnote-ref\"><a href=\"#fn-a\">"));
        assert!(html.contains("href=\"#fnref-1\">"));
    }
}

#[test]
fn literal_percent_sequences_in_note_ids_do_not_navigate_to_another_note() {
    let html = render(
        "First[^x%41], second[^xA], Unicode[^%CE%B2], real Unicode[^β].\n\n\
         [^x%41]: Literal escape.\n\n[^xA]: Decoded neighbor.\n\n\
         [^%CE%B2]: Encoded-looking source.\n\n[^β]: Unicode source.\n",
    );
    for (id, fragment) in [
        ("x%41", "x%2541"),
        ("xA", "xA"),
        ("%CE%B2", "%25CE%25B2"),
        ("β", "%CE%B2"),
    ] {
        assert_eq!(html.matches(&format!("id=\"fn-{id}\"")).count(), 1);
        assert_eq!(html.matches(&format!("href=\"#fn-{fragment}\"")).count(), 1);
        assert_eq!(
            franken_markdown::anchors::decode_fragment(&format!("fn-{fragment}")),
            Some(format!("fn-{id}")),
        );
    }
    assert!(!html.contains("href=\"#fn-x%41\""));
    assert!(html.contains(
        "Unicode<sup class=\"footnote-ref\" id=\"fnref-3\"><a href=\"#fn-%25CE%25B2\">3</a></sup>",
    ));
    assert!(html.contains(
        "real Unicode<sup class=\"footnote-ref\" id=\"fnref-4\"><a href=\"#fn-%CE%B2\">4</a></sup>",
    ));
}

#[test]
fn nested_note_references_and_undefined_marks_preserve_note_ownership() {
    let html = render(
        "Start[^outer].\n\n\
         [^outer]: [Nested[^inner] and missing[^absent]](/source).\n\n\
         [^inner]: Inner evidence.\n",
    );
    assert!(html.contains(
        "<a href=\"/source\">Nested<sup class=\"footnote-ref\" id=\"fnref-2\">2</sup> \
         and missing[^absent]</a>",
    ));
    assert_eq!(html.matches("<li id=\"fn-inner\"").count(), 1);
    assert!(html.contains("href=\"#fnref-2\">"));
    assert!(html.contains("Inner evidence."));
}

#[test]
fn caller_constructed_nested_links_do_not_close_the_outer_link_early() {
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![
            Inline::Link {
                dest: "/outer".into(),
                title: None,
                content: vec![
                    Inline::Text("before ".into()),
                    Inline::Link {
                        dest: "/inner".into(),
                        title: None,
                        content: vec![Inline::Text("inside".into())],
                    },
                    Inline::Text(" after".into()),
                ],
            },
            Inline::Link {
                dest: "/next".into(),
                title: None,
                content: vec![Inline::Text("next".into())],
            },
        ])],
    };
    let html = render_document(&doc);
    assert!(
        html.contains(
            "<p><a href=\"/outer\">before inside after</a><a href=\"/next\">next</a></p>",
        )
    );
}
