//! Footnote names are source identifiers, not already-encoded URL fragments.
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

#[test]
fn percent_spelling_in_a_note_name_cannot_select_another_notes_anchor() {
    let doc = parse_markdown(
        "First.[^chapter%41] Second.[^chapterA] First again.[^chapter%41]\n\n\
         [^chapter%41]: Percent-named evidence.\n\n\
         [^chapterA]: Literal-letter evidence.\n",
    );
    let html = render_html_document(&doc, &options()).unwrap();
    assert!(html.contains("<li id=\"fn-chapter%41\"><p>Percent-named evidence.</p>"));
    assert!(html.contains("<li id=\"fn-chapterA\"><p>Literal-letter evidence.</p>"));
    assert_eq!(html.matches("href=\"#fn-chapter%2541\"").count(), 2);
    assert_eq!(html.matches("href=\"#fn-chapterA\"").count(), 1);
    assert!(!html.contains("href=\"#fn-chapter%41\""));
    assert_eq!(html.matches("id=\"fnref-1\"").count(), 1);
    assert_eq!(html.matches("href=\"#fnref-1\"").count(), 1);
}

#[test]
fn generated_fragments_encode_source_bytes_once_including_uri_and_markup_characters() {
    for (id, anchor, fragment) in [
        ("space%20name", "space%20name", "space%2520name"),
        ("nested%2520name", "nested%2520name", "nested%252520name"),
        ("malformed%zz", "malformed%zz", "malformed%25zz"),
        ("ending%", "ending%", "ending%25"),
        ("café", "café", "caf%C3%A9"),
        ("a&b", "a&amp;b", "a%26b"),
        ("quoted\"name", "quoted&quot;name", "quoted%22name"),
        ("a#b?c/d", "a#b?c/d", "a%23b%3Fc%2Fd"),
        (":~:text=claim", ":~:text=claim", "%3A~%3Atext%3Dclaim"),
        ("plain-._~123", "plain-._~123", "plain-._~123"),
    ] {
        let doc = Document {
            blocks: vec![
                Block::Paragraph(vec![Inline::FootnoteRef { id: id.into() }]),
                Block::FootnoteDefinition {
                    id: id.into(),
                    blocks: vec![Block::Paragraph(vec![Inline::Text("Evidence.".into())])],
                },
            ],
        };
        let html = render_html_document(&doc, &options()).unwrap();
        assert!(
            html.contains(&format!("<li id=\"fn-{anchor}\">")),
            "{id}: {html}"
        );
        assert!(
            html.contains(&format!("href=\"#fn-{fragment}\"")),
            "{id}: {html}"
        );
        assert!(html.contains("href=\"#fnref-1\""));
        let mut streamed = Vec::new();
        render_html_document_to_writer(&doc, &options(), &mut streamed).unwrap();
        assert_eq!(
            streamed,
            html.as_bytes(),
            "streaming changed note identity: {id}"
        );
    }
}

#[test]
fn nested_note_links_retain_the_same_source_identity_and_numbering() {
    let doc = parse_markdown(
        "Claim.[^outer]\n\n\
         [^outer]: See nested.[^inner%CE%B2]\n\n\
         [^inner%CE%B2]: Nested evidence.\n",
    );
    let html = render_html_document(&doc, &options()).unwrap();
    assert!(html.contains("id=\"fnref-2\"><a href=\"#fn-inner%25CE%25B2\">2</a>"));
    assert!(html.contains("<li id=\"fn-inner%CE%B2\"><p>Nested evidence.</p>"));
    assert!(html.contains("href=\"#fnref-2\""));
}

#[test]
fn diff_footnote_links_keep_revision_namespaces_and_literal_source_ids() {
    let old = parse_markdown("Old claim.[^n%41]\n\n[^n%41]: Old evidence.\n");
    let new = parse_markdown("New claim.[^n%41]\n\n[^n%41]: New evidence.\n");
    let diff = franken_markdown::diff::compute_diff(&old, &new, "old.md", "new.md");
    let html = diff.to_html_with_options(&options());
    for revision in ["old", "new"] {
        assert!(
            html.contains(&format!("id=\"diff-{revision}-fn-n%41\"")),
            "{html}"
        );
        assert!(
            html.contains(&format!("href=\"#diff-{revision}-fn-n%2541\"")),
            "{html}"
        );
        assert!(
            html.contains(&format!("href=\"#diff-{revision}-fnref-1\"")),
            "{html}"
        );
    }
}
