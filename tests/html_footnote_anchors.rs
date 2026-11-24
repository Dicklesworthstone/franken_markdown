//! Generated note links must not be captured by authored heading identifiers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use franken_markdown::{
    DocumentSourceMap, HtmlOptions, parse_markdown, parse_markdown_spanned, render_html_document,
    render_html_document_to_writer,
};

fn options() -> HtmlOptions {
    HtmlOptions {
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    }
}

fn assert_unique_ids(html: &str) {
    let mut ids = BTreeSet::new();
    for tag in html.split('<').skip(1) {
        let tag = tag.split('>').next().unwrap();
        if let Some((_, tail)) = tag.split_once(" id=\"") {
            let id = tail.split('"').next().unwrap();
            assert!(ids.insert(id), "duplicate destination {id}: {html}");
        }
    }
}

fn render(source: &str) -> String {
    let document = parse_markdown(source);
    let options = options();
    let html = render_html_document(&document, &options).unwrap();
    assert_unique_ids(&html);
    let mut streamed = Vec::new();
    let bytes = render_html_document_to_writer(&document, &options, &mut streamed).unwrap();
    assert_eq!(bytes, html.len());
    assert_eq!(
        streamed,
        html.as_bytes(),
        "streaming must use the same destinations"
    );
    let fragment = franken_markdown::html::render_fragment(&document.blocks, &options);
    assert_unique_ids(&fragment);
    assert!(html.contains(&fragment));
    assert!(!fragment.contains("epub:type="));
    assert_eq!(html, render_html_document(&document, &options).unwrap());
    html
}

#[test]
fn notes_reserve_later_headings_and_other_notes_before_choosing_suffixes() {
    let source = "Claim[^note], other[^note-3], repeat[^note].\n\n\
        # fn-note\n\n# fn-note-2\n\n# fnref-1\n\n# fnref-1-2\n\n\
        [Heading](#fn-note) [Citation heading](#fnref-1)\n\n\
        [^note]: First evidence.\n\n[^note-3]: Other evidence.\n";
    let html = render(source);
    for heading in ["fn-note", "fn-note-2", "fnref-1", "fnref-1-2"] {
        assert!(html.contains(&format!("<h1 id=\"{heading}\">{heading}</h1>")));
    }
    assert!(html.contains("id=\"fnref-1-3\"><a href=\"#fn-note-4\">1</a></sup>"));
    assert_eq!(html.matches("href=\"#fn-note-4\"").count(), 2);
    assert!(html.contains("<li id=\"fn-note-4\"><p>First evidence.</p>"));
    assert!(html.contains("<li id=\"fn-note-3\"><p>Other evidence.</p>"));
    assert!(html.contains("class=\"footnote-backref\" href=\"#fnref-1-3\""));
    assert!(html.contains("href=\"#fn-note\">Heading</a>"));
    assert!(html.contains("href=\"#fnref-1\">Citation heading</a>"));
}

#[test]
fn emitted_note_headings_are_reserved_but_unused_definition_headings_are_not() {
    let source = "# fn-other\n\nStart[^outer], then[^other].\n\n\
        [^outer]: # fn-outer\n\n    Nested[^inner] and self[^outer].\n\n\
        [^other]: Other evidence.\n\n\
        [^inner]: # fnref-3\n\n    Nested evidence.\n\n\
        [^unused]: # fn-other-2\n\n    Unused evidence.\n";
    let html = render(source);
    assert!(html.contains("<h1 id=\"fn-outer\">fn-outer</h1>"));
    assert!(html.contains("<li id=\"fn-outer-2\">"));
    assert!(html.contains("<li id=\"fn-other-2\"><p>Other evidence.</p>"));
    assert!(html.contains("<h1 id=\"fnref-3\">fnref-3</h1>"));
    assert!(html.contains("id=\"fnref-3-2\"><a href=\"#fn-inner\">3</a></sup>"));
    assert!(html.contains("class=\"footnote-backref\" href=\"#fnref-3-2\""));
    assert_eq!(html.matches("<li id=\"fn-outer-2\"").count(), 1);
    assert_eq!(html.matches("Nested evidence.").count(), 1);
    assert!(!html.contains("Unused evidence."));
    assert!(!html.contains("<h1 id=\"fn-other-2\""));
}

#[test]
fn heading_navigation_and_source_map_keep_their_original_identity() {
    let source = "[[TOC]]\n\nReference[^note].\n\n# fn-note\n\n# fnref-1\n\n\
        [^note]: Evidence.\n";
    let html = render(source);
    assert!(html.contains("<nav class=\"toc\"><ul>"));
    assert!(html.contains("href=\"#fn-note\">fn-note</a>"));
    assert!(html.contains("href=\"#fnref-1\">fnref-1</a>"));
    assert!(html.contains("<li id=\"fn-note-2\">"));
    let spanned = parse_markdown_spanned(source);
    let map = DocumentSourceMap::from_spanned_document(&spanned, source).unwrap();
    let slugs: Vec<_> = map
        .headings()
        .iter()
        .map(|heading| heading.slug.as_str())
        .collect();
    assert_eq!(slugs, ["fn-note", "fnref-1"]);
    for slug in slugs {
        let anchor = map.resolve_source_anchor(slug).unwrap();
        assert!(source[anchor.source_span.start..anchor.source_span.end].starts_with("# "));
    }
    assert!(map.resolve_source_anchor("fn-note-2").is_none());
}

#[test]
fn composed_revisions_isolate_collision_safe_note_and_heading_destinations() {
    let source = "# fn-note\n\n# fnref-1\n\nClaim[^note].\n\n[^note]: Evidence.\n";
    let old = parse_markdown(source);
    let new = parse_markdown(&source.replace("Claim", "Changed claim"));
    let diff = franken_markdown::diff::compute_diff(&old, &new, "old.md", "new.md");
    let html = diff.to_html_with_options(&options());
    assert_unique_ids(&html);
    for side in ["old", "new"] {
        assert!(html.contains(&format!("id=\"diff-{side}-fn-note\"")));
        assert!(html.contains(&format!("<li id=\"diff-{side}-fn-note-2\">")));
        assert!(html.contains(&format!("href=\"#diff-{side}-fn-note-2\"")));
        assert!(html.contains(&format!("id=\"diff-{side}-fnref-1-2\"")));
        assert!(html.contains(&format!("href=\"#diff-{side}-fnref-1-2\"")));
    }
}

#[test]
fn unicode_collision_targets_keep_literal_ids_and_encoded_fragments() {
    let html = render("# fn-café\n\nClaim[^café].\n\n[^café]: Evidence.\n");
    assert!(html.contains("<h1 id=\"fn-café\">fn-café</h1>"));
    assert!(html.contains("<li id=\"fn-café-2\"><p>Evidence.</p>"));
    assert!(html.contains("href=\"#fn-caf%C3%A9-2\">1</a>"));
    assert!(!html.contains("href=\"#fn-caf%C3%A9\">1</a>"));
}

#[test]
fn ordinary_note_identifiers_and_markup_stay_unchanged_without_collisions() {
    let source = "Claim[^note].\n\n[^note]: Evidence.\n";
    let document = parse_markdown(source);
    let html = franken_markdown::html::render_fragment(&document.blocks, &options());
    assert_eq!(
        html,
        concat!(
            "<p>Claim<sup class=\"footnote-ref\" id=\"fnref-1\"><a href=\"#fn-note\">1</a></sup>.</p>\n",
            "<section class=\"footnotes\">\n<ol>\n<li id=\"fn-note\"><p>Evidence.</p>\n",
            " <a class=\"footnote-backref\" href=\"#fnref-1\">↩</a>\n</li>\n</ol>\n</section>\n",
        )
    );
}
