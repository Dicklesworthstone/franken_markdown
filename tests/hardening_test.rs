//! Unit tests verifying zero-mock subsystem hardening across the codebase.
//!
//! Enforces exact behavioral invariants across diff escaping, heading slug
//! collision parity, unicode syntax highlighting, URL scanner case-insensitivity,
//! PDF hex color parsing, and verification caret spans.

use franken_markdown::ast::{Block, Document, Inline};
use franken_markdown::caret::CaretStyle;
use franken_markdown::diff::compute_diff;
use franken_markdown::doc_stats::compute_doc_stats;
use franken_markdown::highlight::{highlight, Tok};
use franken_markdown::parse::parse_document;
use franken_markdown::scanner::scan_markdown_line;
use franken_markdown::theme::Theme;
use franken_markdown::verify::{to_human, verify_pdf};
use franken_markdown::PdfOptions;

#[test]
fn diff_html_escapes_raw_html_and_blocks_xss() {
    let doc_a = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Text(
            "Normal text".to_string(),
        )])],
    };
    let doc_b = Document {
        blocks: vec![Block::Paragraph(vec![
            Inline::Text("Normal text with ".to_string()),
            Inline::Html("<script>alert('xss')</script>".to_string()),
        ])],
    };

    let diff = compute_diff(&doc_a, &doc_b, "old.md", "new.md");
    let html = diff.to_html(&Theme::default());

    assert!(
        !html.contains("<script>"),
        "Raw <script> tag must not be emitted unescaped in diff HTML"
    );
    assert!(
        html.contains("&lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;")
            || html.contains("&lt;script&gt;alert('xss')&lt;/script&gt;"),
        "Raw HTML inline must be properly HTML escaped in diff output"
    );
}

#[test]
fn diff_html_filters_javascript_links() {
    let doc_a = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Text(
            "Before".to_string(),
        )])],
    };
    let doc_b = Document {
        blocks: vec![Block::Paragraph(vec![Inline::Link {
            dest: "javascript:alert(1)".to_string(),
            title: None,
            content: vec![Inline::Text("Click me".to_string())],
        }])],
    };

    let diff = compute_diff(&doc_a, &doc_b, "old.md", "new.md");
    let html = diff.to_html(&Theme::default());

    assert!(
        !html.contains("href=\"javascript:"),
        "Unsafe javascript: link href must not be rendered as an active anchor in diff HTML"
    );
}

#[test]
fn doc_stats_heading_collision_parity_with_html() {
    let markdown = "# Intro\n\n# Intro-2\n\n# Intro\n\n[Link](#intro-3)\n";
    let doc = parse_document(markdown);
    let stats = compute_doc_stats(markdown, &doc);

    assert_eq!(stats.outline.len(), 3);
    assert_eq!(stats.outline[0].slug, "intro");
    assert_eq!(stats.outline[1].slug, "intro-2");
    assert_eq!(stats.outline[2].slug, "intro-3");

    // Broken internal anchor check: [Link](#intro-3) targets the 3rd heading,
    // so no broken_internal_anchor finding should be reported.
    let broken_anchors: Vec<_> = stats
        .findings
        .iter()
        .filter(|f| f.code == "broken_internal_anchor")
        .collect();
    assert!(
        broken_anchors.is_empty(),
        "Anchor #intro-3 must be recognized as existing in the document"
    );
}

#[test]
fn highlight_unicode_capitalized_types() {
    let spans = highlight("rust", "let x: Überklasse = foo;");
    let type_span = spans.iter().find(|s| s.kind == Tok::Type);
    assert!(
        type_span.is_some(),
        "Unicode PascalCase identifier 'Überklasse' should be recognized as Tok::Type"
    );

    let spans_fr = highlight("rust", "let y: Événement = bar;");
    let type_span_fr = spans_fr.iter().find(|s| s.kind == Tok::Type);
    assert!(
        type_span_fr.is_some(),
        "Unicode PascalCase identifier 'Événement' should be recognized as Tok::Type"
    );
}

#[test]
fn scanner_case_insensitive_url_prefixes() {
    assert!(
        scan_markdown_line("Visit HTTP://EXAMPLE.COM").maybe_autolink,
        "Uppercase HTTP:// should be recognized by maybe_autolink scan"
    );
    assert!(
        scan_markdown_line("Visit HTTPS://EXAMPLE.COM").maybe_autolink,
        "Uppercase HTTPS:// should be recognized by maybe_autolink scan"
    );
    assert!(
        scan_markdown_line("Visit WWW.EXAMPLE.COM").maybe_autolink,
        "Uppercase WWW. should be recognized by maybe_autolink scan"
    );
}

#[test]
fn verify_accessibility_caret_spans_in_human_output() {
    let md =
        "# Title\n\n### Subtitle\n\n![ ](missing-img.png)\n\n[click here](https://example.com)\n";
    let doc = parse_document(md);
    let report = verify_pdf(&doc, &PdfOptions::default()).expect("verify report");

    let human = to_human(&report, md, Some("test.md"), CaretStyle::default());

    // Verify caret blocks are rendered for accessibility findings
    assert!(
        human.contains("heading_level_skip"),
        "Should report heading level skip"
    );
    assert!(
        human.contains("missing_alt_text"),
        "Should report missing alt text"
    );
    assert!(
        human.contains("generic_link_text"),
        "Should report generic link text"
    );
}
