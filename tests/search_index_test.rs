//! Standalone integration tests for `src/search_index.rs`.
//!
//! The module is included directly (not via the crate root) so these tests
//! run before the module is registered in `lib.rs`. The critical guarantee
//! under test: heading anchors produced by `build_search_index` match the
//! `id="..."` attributes the HTML emitter produces, byte-for-byte, including
//! duplicate-heading collision suffixes.
//!
//! Tests may use `unwrap` for brevity, so opt out of the crate-wide
//! restriction lints here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/search_index.rs"]
mod search_index;

use franken_markdown::ast::{Block, Document, Inline};
use franken_markdown::html;
use franken_markdown::{HtmlOptions, parse};
use search_index::{EntryKind, build_search_index, search_index_json};

/// Extract every `<hN id="...">` id from rendered HTML, in document order.
fn html_heading_ids(rendered: &str) -> Vec<String> {
    let bytes = rendered.as_bytes();
    let mut ids = Vec::new();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'h' && bytes[i + 2].is_ascii_digit() {
            // Scan this tag for id="...".
            let tag_end = rendered[i..]
                .find('>')
                .map(|off| i + off)
                .expect("heading tag must close");
            let tag = &rendered[i..tag_end];
            let id_start = tag
                .find(" id=\"")
                .map(|off| off + " id=\"".len())
                .expect("heading tag must carry an id");
            let id_end = tag[id_start..]
                .find('"')
                .map(|off| id_start + off)
                .expect("id attribute must close");
            ids.push(tag[id_start..id_end].to_string());
            i = tag_end;
        }
        i += 1;
    }
    ids
}

/// Anchors assigned by the index to heading entries, in document order.
fn index_heading_anchors(doc: &Document) -> Vec<String> {
    build_search_index(doc)
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Heading)
        .map(|e| e.anchor.clone())
        .collect()
}

#[test]
fn anchors_match_rendered_html_with_duplicate_headings() {
    let md = "# Alpha\n\npara one\n\n## Beta\n\n# Alpha\n\n> # Alpha\n>\n> quoted para\n\n1. # Alpha\n\n   list para\n\n# Alpha\n";
    let doc = parse(md);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    let html_ids = html_heading_ids(&rendered);
    let index_anchors = index_heading_anchors(&doc);

    assert_eq!(
        index_anchors,
        vec![
            "alpha".to_string(),
            "beta".to_string(),
            "alpha-2".to_string(),
            "alpha-3".to_string(),
            "alpha-4".to_string(),
            "alpha-5".to_string(),
        ],
        "index anchors must mirror the collision-suffix state machine"
    );
    assert_eq!(
        index_anchors, html_ids,
        "index anchors must match the rendered HTML heading ids byte-for-byte\nHTML: {rendered}"
    );
}

#[test]
fn anchors_match_rendered_html_for_slug_edge_cases() {
    let md = "# Hello, World!\n\n# Under_score - Test\n\n# 中文 Unicode\n\n# !!!\n\n# !!!\n";
    let doc = parse(md);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    let expected = vec![
        "hello-world".to_string(),
        "under-score-test".to_string(),
        "unicode".to_string(),
        "section".to_string(),
        "section-2".to_string(),
    ];
    assert_eq!(index_heading_anchors(&doc), expected);
    assert_eq!(
        index_heading_anchors(&doc),
        html_heading_ids(&rendered),
        "slug edge cases must match rendered HTML ids\nHTML: {rendered}"
    );
}

#[test]
fn anchors_match_rendered_html_when_base_collides_with_suffix_text() {
    // Adversarial chain: the literal heading text "A-2" slugs to the same id
    // the collision suffix assigns to the second "A".
    let md = "# A\n\n# A\n\n# A-2\n\n# A\n";
    let doc = parse(md);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    let expected = vec![
        "a".to_string(),
        "a-2".to_string(),
        "a-2-2".to_string(),
        "a-3".to_string(),
    ];
    assert_eq!(index_heading_anchors(&doc), expected);
    assert_eq!(
        index_heading_anchors(&doc),
        html_heading_ids(&rendered),
        "collision chains must match rendered HTML ids\nHTML: {rendered}"
    );
}

#[test]
fn anchors_match_rendered_html_with_inline_formatting_in_headings() {
    let md = "# The *Quick* `Brown` [Fox](https://example.com) ![Alt Text](x.png)\n\n# The Quick Brown Fox Alt Text\n";
    let doc = parse(md);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    // Both headings slug identically; the second takes the collision suffix.
    let expected = vec![
        "the-quick-brown-fox-alt-text".to_string(),
        "the-quick-brown-fox-alt-text-2".to_string(),
    ];
    assert_eq!(index_heading_anchors(&doc), expected);
    assert_eq!(
        index_heading_anchors(&doc),
        html_heading_ids(&rendered),
        "inline-formatting slugs must match rendered HTML ids\nHTML: {rendered}"
    );
}

#[test]
fn footnote_definitions_do_not_consume_heading_ids() {
    // The footnote definition sits between duplicate body headings; the HTML
    // emitter renders definitions after the body, so body heading ids must be
    // assigned as if the definition were not there.
    let md = "# Dup\n\nbody[^n]\n\n[^n]: # Dup\n\n# Dup\n";
    let doc = parse(md);
    let rendered = html::render_fragment(&doc.blocks, &HtmlOptions::default());

    let index_anchors = index_heading_anchors(&doc);
    assert_eq!(
        index_anchors,
        vec!["dup".to_string(), "dup-2".to_string()],
        "the footnote-definition heading must not appear in the body index"
    );
    // The rendered body walk assigns the same ids to the two body headings.
    assert_eq!(
        index_anchors,
        html_heading_ids(&rendered)[..2],
        "body heading ids must match the rendered HTML\nHTML: {rendered}"
    );
}

#[test]
fn entries_follow_document_order_through_containers() {
    let md = "# One\n\nfirst para\n\n> quoted para\n\n- list para\n\n## Two\n\nlast para\n";
    let doc = parse(md);
    let index = build_search_index(&doc);

    let summary: Vec<(&str, Option<u8>, &str, &str)> = index
        .entries
        .iter()
        .map(|e| {
            (
                e.kind.as_str_pub(),
                e.level,
                e.anchor.as_str(),
                e.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("heading", Some(1), "one", "One"),
            ("paragraph", None, "one", "first para"),
            ("paragraph", None, "one", "quoted para"),
            ("paragraph", None, "one", "list para"),
            ("heading", Some(2), "two", "Two"),
            ("paragraph", None, "two", "last para"),
        ],
        "entries must appear in document order, descending into containers"
    );
}

#[test]
fn paragraphs_before_first_heading_have_empty_anchor() {
    let doc = parse("preamble paragraph\n\n# Later\n");
    let index = build_search_index(&doc);

    assert_eq!(index.entries.len(), 2);
    assert_eq!(index.entries[0].kind, EntryKind::Paragraph);
    assert_eq!(index.entries[0].anchor, "");
    assert_eq!(index.entries[1].anchor, "later");
}

#[test]
fn text_is_whitespace_normalized() {
    let md = "# Heading\n\nFirst  paragraph\ncontinues   here.\n\n\ttabbed   indent stays text?\n";
    let doc = parse(md);
    let index = build_search_index(&doc);

    let para = index
        .entries
        .iter()
        .find(|e| e.kind == EntryKind::Paragraph)
        .expect("fixture has a paragraph");
    assert_eq!(para.text, "First paragraph continues here.");
    assert_eq!(index.entries[0].text, "Heading");
}

#[test]
fn heading_text_normalization_strips_formatting_but_keeps_words() {
    let doc = parse("# The  *Emphasized*   `Code` Heading\n");
    let index = build_search_index(&doc);

    assert_eq!(index.entries.len(), 1);
    assert_eq!(index.entries[0].text, "The Emphasized Code Heading");
    assert_eq!(index.entries[0].anchor, "the-emphasized-code-heading");
}

#[test]
fn json_golden_output() {
    let md = "# Title\n\nFirst  paragraph\ncontinues here.\n\n## Second\n\n- list paragraph\n";
    let doc = parse(md);
    let json = search_index_json(&build_search_index(&doc));

    let expected = "{\"schema\":\"fmd-search-index-v1\",\"entries\":[\
{\"kind\":\"heading\",\"level\":1,\"anchor\":\"title\",\"text\":\"Title\"},\
{\"kind\":\"paragraph\",\"anchor\":\"title\",\"text\":\"First paragraph continues here.\"},\
{\"kind\":\"heading\",\"level\":2,\"anchor\":\"second\",\"text\":\"Second\"},\
{\"kind\":\"paragraph\",\"anchor\":\"second\",\"text\":\"list paragraph\"}\
]}";
    assert_eq!(json, expected);
}

#[test]
fn json_escapes_quotes_backslashes_and_control_characters() {
    let doc = Document {
        blocks: vec![
            Block::Heading {
                level: 1,
                inlines: vec![Inline::Text("Quote \" \\ \n \u{1}".to_string())],
            },
            Block::Paragraph(vec![Inline::Text("tab\there".to_string())]),
        ],
    };
    let json = search_index_json(&build_search_index(&doc));

    let expected = "{\"schema\":\"fmd-search-index-v1\",\"entries\":[\
{\"kind\":\"heading\",\"level\":1,\"anchor\":\"quote\",\"text\":\"Quote \\\" \\\\ \\u0001\"},\
{\"kind\":\"paragraph\",\"anchor\":\"quote\",\"text\":\"tab here\"}\
]}";
    assert_eq!(json, expected);
}

#[test]
fn empty_document_produces_empty_index() {
    let doc = parse("");
    let index = build_search_index(&doc);

    assert!(index.entries.is_empty());
    assert_eq!(
        search_index_json(&index),
        "{\"schema\":\"fmd-search-index-v1\",\"entries\":[]}"
    );
}

#[test]
fn index_is_deterministic_across_runs() {
    let md = "# A\n\n# A\n\n# A-2\n\npara *one*  two\n\n> # A\n";
    let first = search_index_json(&build_search_index(&parse(md)));
    let second = search_index_json(&build_search_index(&parse(md)));
    assert_eq!(first, second);
}

#[test]
fn non_indexed_blocks_contribute_no_entries() {
    let md =
        "# H\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n$$x^2$$\n";
    let doc = parse(md);
    let index = build_search_index(&doc);

    assert_eq!(
        index.entries.len(),
        1,
        "code blocks, tables, thematic breaks, and math blocks produce no entries"
    );
    assert_eq!(index.entries[0].anchor, "h");
}

// Test-local extension trait so the order test can read the wire spelling of
// EntryKind without making the field public API beyond the contract.
#[allow(clippy::wrong_self_convention)]
trait EntryKindExt {
    fn as_str_pub(self) -> &'static str;
}

impl EntryKindExt for EntryKind {
    fn as_str_pub(self) -> &'static str {
        match self {
            EntryKind::Heading => "heading",
            EntryKind::Paragraph => "paragraph",
        }
    }
}
