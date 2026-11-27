//! End-to-end publication and diagnostics for GitHub-authored separator anchors.
#![allow(clippy::unwrap_used)]

use franken_markdown::book::validation::check_book_links;
use franken_markdown::book::{BookInput, build_book, render_book_pdf};
use franken_markdown::doc_stats::compute_doc_stats;
use franken_markdown::verify::verify_pdf_result;
use franken_markdown::{
    HtmlOptions, PdfOptions, parse_markdown, render_html_document, render_pdf_document,
};

const REPRO: &str =
    "- [PART 0 — DOCUMENT CONTROL](#part-0--document-control)\n\n# PART 0 — DOCUMENT CONTROL\n";

fn html_options() -> HtmlOptions {
    HtmlOptions {
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    }
}

#[test]
fn reported_github_toc_reproducer_renders_and_verifies_without_changing_ids() {
    let doc = parse_markdown(REPRO);
    let original = doc.clone();
    let canonical = parse_markdown(&REPRO.replace("#part-0--", "#part-0-"));
    let html = render_html_document(&doc, &html_options()).unwrap();
    assert!(html.contains("href=\"#part-0-document-control\""));
    assert!(html.contains("id=\"part-0-document-control\""));
    assert!(!html.contains("id=\"part-0--document-control\""));
    assert_eq!(
        html,
        render_html_document(&canonical, &html_options()).unwrap()
    );
    let stats = compute_doc_stats(REPRO, &doc);
    assert!(
        stats
            .findings
            .iter()
            .all(|finding| finding.code != "broken_internal_anchor")
    );
    let options = PdfOptions::default();
    let verification = verify_pdf_result(&doc, &options).unwrap();
    assert!(verification.anchors_unresolved.is_empty());
    assert_eq!(verification.anchors_resolved, 1);
    let pdf = render_pdf_document(&doc, &options).unwrap();
    assert_eq!(pdf, render_pdf_document(&canonical, &options).unwrap());
    assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Link"));
    assert_eq!(doc, original);
}

#[test]
fn html_queries_safe_html_and_percent_escapes_share_source_owned_aliases() {
    let source = "# A B\n\n# A — B\n\n\
        [encoded](?view=print#a%2D%2Db) \
        [exact](#a-b) \
        [external](https://example.test/#a--b)\n\n\
        <a href=\"#a--b\">Raw anchor</a>\n";
    let doc = parse_markdown(source);
    let html = render_html_document(&doc, &html_options()).unwrap();
    assert!(html.contains("href=\"?view=print#a-b-2\""));
    assert!(html.contains("href=\"#a-b\">exact</a>"));
    assert!(html.contains("href=\"#a-b-2\">Raw anchor</a>"));
    assert!(html.contains("href=\"https://example.test/#a--b\""));
}

#[test]
fn api_identifiers_and_free_duplicate_suffixes_resolve_across_publication() {
    let source = "[api](#read_value) [again](?q=1#read%5Fvalue-1) [repeat](#usage-1) \
        [start](#-getting-started)\n\n# Read Value\n\n# `read_value`\n\n# `read_value`\n\n\
        # Usage\n\n# Usage\n\n# 🚀 Getting started";
    let canonical = source
        .replace("#read_value)", "#read-value-2)")
        .replace("#read%5Fvalue-1)", "#read-value-3)")
        .replace("#usage-1)", "#usage-2)")
        .replace("#-getting-started)", "#getting-started)");
    let doc = parse_markdown(source);
    let expected = parse_markdown(&canonical);
    let html = render_html_document(&doc, &html_options()).unwrap();
    assert_eq!(
        html,
        render_html_document(&expected, &html_options()).unwrap()
    );
    assert!(html.contains("href=\"#read-value-2\">api</a>"));
    assert!(html.contains("href=\"?q=1#read-value-3\">again</a>"));
    assert!(html.contains("href=\"#usage-2\">repeat</a>"));
    let options = PdfOptions::default();
    assert_eq!(
        render_pdf_document(&doc, &options).unwrap(),
        render_pdf_document(&expected, &options).unwrap()
    );
    let report = verify_pdf_result(&doc, &options).unwrap();
    assert!(report.anchors_unresolved.is_empty());
    assert_eq!(report.anchors_resolved, 4);
}

#[test]
fn pdf_note_link_binding_keeps_heading_alias_resolution() {
    let source = "# A B\n\n# A — B\n\n\
        [heading](#a--b) [note](#fn-n) cited[^n].\n\n[^n]: Note body.";
    let canonical = source.replace("#a--b", "#a-b-2");
    let doc = parse_markdown(source);
    let options = PdfOptions::default();
    let verification = verify_pdf_result(&doc, &options).unwrap();
    assert!(verification.anchors_unresolved.is_empty());
    assert_eq!(
        render_pdf_document(&doc, &options).unwrap(),
        render_pdf_document(&parse_markdown(&canonical), &options).unwrap()
    );
}

#[test]
fn cross_chapter_aliases_work_in_validation_and_the_direct_pdf_book_api() {
    let mut book = build_book(&[
        BookInput {
            path: "intro.md".into(),
            source: "[next](guide.md?view=print#a--b-1)".into(),
        },
        BookInput {
            path: "guide.md".into(),
            source: "# A B\n\n# A — B\n\n# A — B".into(),
        },
    ])
    .unwrap();
    assert_eq!(check_book_links(&book).unwrap().finding_count(), 0);
    let html = render_html_document(&book.chapters[0].doc, &html_options()).unwrap();
    assert!(html.contains("href=\"/guide.md?view=print#a-b-3\""));

    // Public Book fields also accept a source-owned AST constructed by hosts.
    // Exercise the writer's source map without build_book's URL normalization.
    book.chapters[0].doc = parse_markdown("[next](guide.md?view=print#a--b-1)");
    assert_eq!(check_book_links(&book).unwrap().finding_count(), 0);
    let mut canonical = book.clone();
    canonical.chapters[0].doc = parse_markdown("[next](guide.md?view=print#a-b-3)");
    let options = PdfOptions::default();
    let pdf = render_book_pdf(&book, &options).unwrap();
    assert_eq!(pdf, render_book_pdf(&canonical, &options).unwrap());
    let wire = String::from_utf8_lossy(&pdf);
    assert!(wire.contains("/Subtype /Link"));
    assert!(!wire.contains("fmd-book-pdf-nav/"));
}

#[test]
fn endnote_heading_aliases_preserve_source_ordinals_through_both_transforms() {
    let source = parse_markdown(
        "[^unused]:\n    # A B\n\n# A B\n\n<h2>A B</h2>\n\n# A — B\n\n\
         Published[^n].\n\n[^n]:\n    # A — B\n",
    );
    let prepared = franken_markdown::PreparedEndnotes::new(&source);
    let aliases: std::collections::BTreeMap<_, _> = prepared.heading_aliases().collect();
    // Three body headings precede the generated Notes heading. First-use
    // ordering puts the referenced definition before the earlier unused one.
    assert_eq!(aliases.get("a--b"), Some(&Some(2)));
    assert_eq!(aliases.get("a-b-2"), Some(&Some(2)));
    assert_eq!(aliases.get("a--b-1"), Some(&Some(4)));
    assert_eq!(aliases.get("a-b-3"), Some(&Some(4)));
}
