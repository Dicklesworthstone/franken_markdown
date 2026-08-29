//! Integration tests for `src/analyze.rs`, included standalone via `#[path]`
//! so they run before the module is wired into `lib.rs`.
//!
//! Real inputs only: every fixture is parsed with the real parser, and the
//! heading-anchor mirror is cross-checked against the real HTML renderer's
//! emitted `id` attributes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/analyze.rs"]
mod analyze;

use analyze::{analysis_json, analyze_document};
use franken_markdown::{HtmlOptions, parse_markdown, render_html};

/// The fixture's expected metrics were hand-derived from the module's
/// documented rules (see src/analyze.rs docs):
/// prose = headings + paragraphs (link text and image alt included, code
/// spans excluded); sentences end at `. ! ?` + whitespace/end; syllables via
/// the documented vowel-group heuristic (total 18 across the 15 words).
const FIXTURE: &str = r#"# Alpha

One two three.

## Beta

Four five six seven eight!

```rust
fn main() {}
```

```python
pass
```

| A | B |
|---|---|
| 1 | 2 |

![alt text](img.png)

![](missing.png)

[ok](#alpha) [bad](#nope) [ext](https://example.com)
"#;

#[test]
fn fixture_known_counts() {
    let doc = parse_markdown(FIXTURE);
    let a = analyze_document(&doc);

    assert_eq!(a.word_count, 15);
    assert_eq!(a.reading_time_secs, 5); // 15 words * 60 / 200 = 4.5, half-up -> 5
    assert_eq!(a.heading_count, 2);
    assert_eq!(a.heading_depth_histogram, [1, 1, 0, 0, 0, 0]);
    assert_eq!(a.code_blocks, 2);
    assert_eq!(
        a.code_languages.iter().collect::<Vec<_>>(),
        vec![(&"python".to_string(), &1), (&"rust".to_string(), &1)]
    );
    assert_eq!(a.table_count, 1);
    assert_eq!(a.image_count, 2);
    assert_eq!(a.images_missing_alt, 1);
    assert_eq!(a.links.total, 3);
    assert_eq!(a.links.internal, 2);
    assert_eq!(a.links.external, 1);
    assert_eq!(a.links.broken_anchor_count, 1); // #nope has no heading
    // 206.835 - 1.015*(15/2) - 84.6*(18/15) = 97.7025 -> 97.7
    assert!((a.flesch_reading_ease - 97.7).abs() < 0.05);
}

#[test]
fn json_schema_golden() {
    let doc = parse_markdown(FIXTURE);
    let json = analysis_json(&analyze_document(&doc));
    let expected = concat!(
        "{\"schema\":\"fmd-analyze-v1\",",
        "\"word_count\":15,",
        "\"reading_time_secs\":5,",
        "\"heading_count\":2,",
        "\"heading_depth_histogram\":[1,1,0,0,0,0],",
        "\"code_blocks\":2,",
        "\"code_languages\":{\"python\":1,\"rust\":1},",
        "\"table_count\":1,",
        "\"image_count\":2,",
        "\"images_missing_alt\":1,",
        "\"links\":{\"total\":3,\"internal\":2,\"external\":1,\"broken_anchor_count\":1},",
        "\"flesch_reading_ease\":97.7}"
    );
    assert_eq!(json, expected);
}

#[test]
fn empty_document_edge() {
    let doc = parse_markdown("");
    let a = analyze_document(&doc);
    assert_eq!(a.word_count, 0);
    assert_eq!(a.reading_time_secs, 0);
    assert_eq!(a.heading_count, 0);
    assert_eq!(a.heading_depth_histogram, [0; 6]);
    assert_eq!(a.code_blocks, 0);
    assert!(a.code_languages.is_empty());
    assert_eq!(a.table_count, 0);
    assert_eq!(a.image_count, 0);
    assert_eq!(a.images_missing_alt, 0);
    assert_eq!(a.links.total, 0);
    assert_eq!(a.links.broken_anchor_count, 0);
    assert_eq!(a.flesch_reading_ease, 0.0);
    assert_eq!(
        analysis_json(&a),
        "{\"schema\":\"fmd-analyze-v1\",\"word_count\":0,\"reading_time_secs\":0,\
\"heading_count\":0,\"heading_depth_histogram\":[0,0,0,0,0,0],\"code_blocks\":0,\
\"code_languages\":{},\"table_count\":0,\"image_count\":0,\"images_missing_alt\":0,\
\"links\":{\"total\":0,\"internal\":0,\"external\":0,\"broken_anchor_count\":0},\
\"flesch_reading_ease\":0.0}"
    );
}

#[test]
fn duplicate_heading_collision_suffixes_match_html_renderer() {
    let src = "# Foo\n\n# Foo\n\n# Foo\n\n[good](#foo) [also good](#foo-2) [third](#foo-3) [bad](#foo-4)\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.links.total, 4);
    assert_eq!(a.links.internal, 4);
    // foo, foo-2, foo-3 resolve; foo-4 does not.
    assert_eq!(a.links.broken_anchor_count, 1);

    // Cross-check the mirror against the REAL renderer's emitted ids.
    let html = render_html(src, &HtmlOptions::default()).unwrap();
    assert!(html.contains("<h1 id=\"foo\">"));
    assert!(html.contains("<h1 id=\"foo-2\">"));
    assert!(html.contains("<h1 id=\"foo-3\">"));
    assert!(!html.contains("id=\"foo-4\""));
}

#[test]
fn empty_slug_falls_back_to_section_like_html_renderer() {
    let src = "# !!!\n\n# ???\n\n[ok](#section) [ok2](#section-2)\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.links.broken_anchor_count, 0);

    let html = render_html(src, &HtmlOptions::default()).unwrap();
    assert!(html.contains("id=\"section\""));
    assert!(html.contains("id=\"section-2\""));
}

#[test]
fn headings_in_quotes_lists_and_footnotes_get_render_order_anchors() {
    // Body walk: blockquote heading first, then list-item heading; the
    // footnote definition's heading is rendered LAST (notes section), so it
    // picks up the collision suffix even though it appears mid-document.
    let src = "> # Deep\n\n- # Deep\n\nRef[^a].\n\n[^a]: # Deep\n\n[one](#deep) [two](#deep-2) [three](#deep-3) [bad](#deep-4)\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.heading_count, 3);
    assert_eq!(a.links.broken_anchor_count, 1); // only #deep-4 is missing

    let html = render_html(src, &HtmlOptions::default()).unwrap();
    assert!(html.contains("id=\"deep\""));
    assert!(html.contains("id=\"deep-2\""));
    assert!(html.contains("id=\"deep-3\""));
}

#[test]
fn unreferenced_footnote_heading_gets_no_anchor() {
    // html.rs omits unreferenced footnote definitions entirely, so a heading
    // inside one is never assigned an id; links to it are broken.
    let src = "Ref[^a].\n\n[ok](#used) [bad](#unused)\n\n[^a]: # Used\n\n[^b]: # Unused\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.links.internal, 2);
    assert_eq!(a.links.broken_anchor_count, 1);
}

#[test]
fn missing_alt_counts_empty_and_whitespace_alts() {
    let src = "![](a.png)\n\n![ ](b.png)\n\n![fine](c.png)\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.image_count, 3);
    assert_eq!(a.images_missing_alt, 2);
}

#[test]
fn code_and_tables_are_excluded_from_prose_metrics() {
    let src = "# Real Heading\n\nReal words here.\n\n```text\ncode words galore not prose\n```\n\n| table words |\n|---|\n| cell words |\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    // "Real Heading" (2) + "Real words here." (3) = 5 prose words; code and
    // table text contribute nothing.
    assert_eq!(a.word_count, 5);
    assert_eq!(a.code_blocks, 1);
    assert_eq!(a.table_count, 1);
}

#[test]
fn inline_code_spans_are_not_prose_words() {
    let doc = parse_markdown("alpha `code span words` omega\n");
    let a = analyze_document(&doc);
    assert_eq!(a.word_count, 2); // alpha, omega
}

#[test]
fn links_and_images_inside_tables_are_counted() {
    let src = "| [in](#nowhere) | ![i](y.png) |\n|---|---|\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.table_count, 1);
    assert_eq!(a.links.total, 1);
    assert_eq!(a.links.internal, 1);
    assert_eq!(a.links.broken_anchor_count, 1);
    assert_eq!(a.image_count, 1);
    assert_eq!(a.images_missing_alt, 0);
}

#[test]
fn sentence_split_requires_terminator_plus_boundary() {
    // "e.g." mid-sentence: '.' followed by ' ' counts by the documented
    // heuristic; "v1.2" does not split ('.' followed by '2').
    let doc = parse_markdown("Use v1.2 e.g. now. It works! Really? Yes\n");
    let a = analyze_document(&doc);
    // sentences: "now." + "works!" + "Really?" = 3 (trailing "Yes" has no
    // terminator but the sentence counter only counts terminators).
    assert_eq!(a.word_count, 8);
    let json = analysis_json(&a);
    assert!(json.contains("\"word_count\":8"));
}

#[test]
fn json_escapes_language_names() {
    let src = "```a\"b\\c\nx\n```\n";
    let doc = parse_markdown(src);
    let a = analyze_document(&doc);
    assert_eq!(a.code_blocks, 1);
    let json = analysis_json(&a);
    assert!(
        json.contains("\"code_languages\":{\"a\\\"b\\\\c\":1}"),
        "{json}"
    );
}

#[test]
fn analysis_is_deterministic_across_repeated_runs() {
    let doc = parse_markdown(FIXTURE);
    let first = analysis_json(&analyze_document(&doc));
    for _ in 0..8 {
        assert_eq!(analysis_json(&analyze_document(&doc)), first);
    }
}
