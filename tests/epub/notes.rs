//! Inspect actual archived XHTML, including XML attributes and link targets.

use std::collections::{BTreeMap, BTreeSet};

use super::{assert_well_formed_xml, entry_text, find_tag_end};
use franken_markdown::{
    Block, BookInput, Document, HtmlOptions, Inline, build_book, epub, parse_markdown,
};

fn tags<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find('<') {
        let start = cursor + offset;
        let end = find_tag_end(source, start);
        let tag = &source[start + 1..end];
        if tag.strip_prefix(name).is_some_and(|rest| {
            rest.is_empty() || rest.starts_with(char::is_whitespace) || rest == "/"
        }) {
            result.push(tag);
        }
        cursor = end + 1;
    }
    result
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    tag.split_once(&format!(" {name}=\""))?.1.split('"').next()
}

fn xml_text(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn fragment(href: &str) -> String {
    let source = xml_text(href).strip_prefix('#').unwrap().to_string();
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < source.len() {
        if source.as_bytes()[i] == b'%' {
            bytes.push(u8::from_str_radix(&source[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            bytes.push(source.as_bytes()[i]);
            i += 1;
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn assert_notes(chapter: &str, notes: usize, references: usize) {
    assert_well_formed_xml(chapter);
    assert!(
        tags(chapter, "html")
            .iter()
            .any(|tag| { attr(tag, "xmlns:epub") == Some("http://www.idpf.org/2007/ops") })
    );
    let mut ids = BTreeMap::new();
    for name in ["h1", "h2", "h3", "h4", "h5", "h6", "li", "sup"] {
        for tag in tags(chapter, name) {
            if let Some(id) = attr(tag, "id") {
                assert!(
                    ids.insert(xml_text(id), tag).is_none(),
                    "duplicate id: {id}"
                );
            }
        }
    }
    let sections = tags(chapter, "section");
    assert_eq!(
        sections
            .iter()
            .filter(|t| attr(t, "epub:type") == Some("endnotes"))
            .count(),
        1
    );
    assert!(sections.iter().any(|tag| {
        attr(tag, "epub:type") == Some("endnotes")
            && attr(tag, "role") == Some("doc-endnotes")
            && attr(tag, "aria-label") == Some("Notes")
    }));
    let items = tags(chapter, "li");
    assert_eq!(
        items
            .iter()
            .filter(|t| attr(t, "epub:type") == Some("endnote"))
            .count(),
        notes
    );
    assert!(!chapter.contains("role=\"doc-endnote\""));
    assert!(!chapter.contains("role=\"doc-footnote\""));
    let links = tags(chapter, "a");
    let mut note_targets = BTreeSet::new();
    let mut count = 0;
    let mut backrefs = 0;
    for link in links {
        match attr(link, "epub:type") {
            Some("noteref") => {
                count += 1;
                assert_eq!(attr(link, "role"), Some("doc-noteref"));
                let target = fragment(attr(link, "href").unwrap());
                assert_eq!(attr(ids[&target], "epub:type"), Some("endnote"));
                note_targets.insert(target);
            }
            Some("backlink") => {
                backrefs += 1;
                assert_eq!(attr(link, "role"), Some("doc-backlink"));
                assert!(
                    attr(link, "aria-label")
                        .unwrap()
                        .starts_with("Back to reference ")
                );
                let target = fragment(attr(link, "href").unwrap());
                assert_eq!(attr(ids[&target], "class"), Some("footnote-ref"));
            }
            _ => {}
        }
    }
    assert_eq!(count, references);
    assert_eq!(backrefs, notes);
    assert!(note_targets.len() <= notes);
}

fn publications(source: &str, opts: &HtmlOptions) -> Vec<Vec<u8>> {
    let book = build_book(&[BookInput {
        path: "chapter.md".into(),
        source: source.into(),
    }])
    .unwrap();
    vec![
        epub::render_epub(&parse_markdown(source), opts).unwrap(),
        epub::render_book_epub(&book, opts).unwrap(),
    ]
}

#[test]
fn repeated_nested_and_cyclic_notes_have_source_owned_publication_semantics() {
    let source = "# Evidence\n\nSecond[^second], first[^first], repeat[^second], missing[^missing].\n\n[^first]: First evidence.\n\n[^second]: Second evidence cites[^third].\n\n[^third]: Third evidence returns[^second].\n\n[^first]: Duplicate must be absent.\n\n[^unused]: Unused must be absent.\n";
    for custom_css in [None, Some(".fmd { color: navy; }".into())] {
        let opts = HtmlOptions {
            custom_css,
            ..HtmlOptions::default()
        };
        let archives = publications(source, &opts);
        assert_eq!(archives, publications(source, &opts), "byte determinism");
        for bytes in archives {
            let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");
            assert_notes(&chapter, 3, 5);
            let labels: Vec<_> = tags(&chapter, "li")
                .iter()
                .filter(|t| attr(t, "epub:type") == Some("endnote"))
                .map(|t| attr(t, "id").unwrap())
                .collect();
            assert_eq!(labels, ["fn-second", "fn-first", "fn-third"]);
            assert!(chapter.contains("[^missing]"));
            assert!(!chapter.contains("Duplicate must be absent"));
            assert!(!chapter.contains("Unused must be absent"));
        }
    }
}

#[test]
fn headings_and_other_note_names_cannot_capture_generated_links() {
    let source = "<h1>fn-note</h1>\n\n# fn-note\n\n# fnref-1\n\n# fnref-1-2\n\nReference[^note], other[^note-3], repeat[^note].\n\n[^note]: ## fn-note-4\n\n    Evidence.\n\n[^note-3]: Other evidence.\n\n[Authored heading](#fn-note)\n";
    for bytes in publications(source, &HtmlOptions::default()) {
        let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");
        assert_notes(&chapter, 2, 3);
        assert!(chapter.contains("<li id=\"fn-note-5\" epub:type=\"endnote\">"));
        assert!(chapter.contains("<li id=\"fn-note-3\" epub:type=\"endnote\">"));
        assert!(chapter.contains("id=\"fnref-1-3\""));
        assert!(chapter.contains("href=\"#fn-note-5\" epub:type=\"noteref\""));
        assert!(chapter.contains("href=\"#fnref-1-3\" epub:type=\"backlink\""));
        assert!(chapter.contains("href=\"#fn-note\">Authored heading</a>"));
    }
}

#[test]
fn links_own_their_labels_and_authored_lookalikes_gain_no_note_authority() {
    let source = "[Claim[^note] after](https://example.com/source) outside[^note].\n\n[^note]: **Strong** evidence and [an ordinary link](#heading).\n\n# Heading\n\n<a class=\"footnote-backref\" epub:type=\"backlink\" href=\"#heading\">Authored</a>\n\n<span class=\"footnote-ref\" epub:type=\"noteref\">Lookalike</span>\n\n```html\n<section class=\"footnotes\">literal</section>\n```\n";
    for bytes in publications(
        source,
        &HtmlOptions {
            allow_raw_html: true,
            ..HtmlOptions::default()
        },
    ) {
        let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");
        assert_notes(&chapter, 1, 1);
        assert!(chapter.contains("href=\"https://example.com/source\">Claim<sup class=\"footnote-ref\" id=\"fnref-1\">1</sup> after</a>"));
        assert!(chapter.contains("href=\"#heading\">Authored</a>"));
        assert!(chapter.contains("<strong>Strong</strong> evidence"));
        assert!(chapter.contains("href=\"#heading\">an ordinary link</a>"));
    }
    let html = franken_markdown::html::render(
        &parse_markdown("Text[^n].\n\n[^n]: Note.\n"),
        &HtmlOptions::default(),
    );
    assert!(
        !html.contains("epub:type="),
        "ordinary HTML keeps its format contract"
    );
    assert!(html.contains("<section class=\"footnotes\">"));
}

#[test]
fn unicode_quotes_ampersands_and_literal_percent_targets_decode_once() {
    let id = "café&\"%41";
    let document = Document {
        blocks: vec![
            Block::Paragraph(vec![Inline::FootnoteRef { id: id.into() }]),
            Block::FootnoteDefinition {
                id: id.into(),
                blocks: vec![Block::Paragraph(vec![Inline::Text("Evidence.".into())])],
            },
        ],
    };
    let bytes = epub::render_epub(&document, &HtmlOptions::default()).unwrap();
    let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");
    assert_notes(&chapter, 1, 1);
    assert!(chapter.contains("id=\"fn-café&amp;&quot;%41\""));
    assert!(chapter.contains("href=\"#fn-caf%C3%A9%26%22%2541\""));
}

#[test]
fn book_notes_and_namespaces_are_scoped_to_their_own_chapters() {
    let book = build_book(&[
        BookInput {
            path: "one.md".into(),
            source: "# One\n\nFirst[^same].\n\n[^same]: One evidence.\n".into(),
        },
        BookInput {
            path: "nested/two.md".into(),
            source: "# Two\n\nSecond[^same].\n\n[^same]: Two evidence.\n".into(),
        },
        BookInput {
            path: "plain.md".into(),
            source: "# Plain\n\nNo references.\n\n[^unused]: Omitted.\n".into(),
        },
    ])
    .unwrap();
    let bytes = epub::render_book_epub(&book, &HtmlOptions::default()).unwrap();
    for (index, evidence) in [(1, "One evidence."), (2, "Two evidence.")] {
        let chapter = entry_text(&bytes, &format!("OEBPS/chapter-{index}.xhtml"));
        assert_notes(&chapter, 1, 1);
        assert!(chapter.contains("href=\"#fn-same\" epub:type=\"noteref\""));
        assert!(chapter.contains(evidence));
    }
    let plain = entry_text(&bytes, "OEBPS/chapter-3.xhtml");
    assert_well_formed_xml(&plain);
    assert!(!plain.contains("xmlns:epub="));
    assert!(!plain.contains("epub:type="));
    assert!(!plain.contains("Omitted."));
}
