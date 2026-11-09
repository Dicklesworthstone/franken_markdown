//! Original-source navigation uses expansion/rewrite provenance, not text search.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{
    BookInput, BookRenderer, BookSourceLocation, BookSourceRange, BookSourceRangeKind,
    BookWorkspace,
};

fn input(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn ranges(book: &BookRenderer, chapter: usize, diagnostic: usize) -> &[BookSourceRange] {
    match book
        .source_diagnostics()
        .source_location(chapter, diagnostic)
        .unwrap()
    {
        BookSourceLocation::Mapped(ranges) => ranges,
        other => panic!("expected mapped source: {other:?}"),
    }
}

#[test]
fn ordinary_source_keeps_bom_unicode_crlf_and_frontmatter_offsets() {
    let source = "\u{feff}---\r\ntitle: Présentation\r\n---\r\n\r\n# 😀\r\n\r\n[broken]:\r\n";
    let book = BookRenderer::new(&[input("notes.md", source)]).unwrap();
    let report = book.source_diagnostics();
    assert_eq!(report.diagnostic_count(), 1);
    let diagnostic = &report.chapters()[0].diagnostics[0];
    assert_eq!(diagnostic.code, "malformed_reference");
    let location = ranges(&book, 0, 0);
    assert_eq!(location.len(), 1);
    assert_eq!(location[0].path, "notes.md");
    assert_eq!(location[0].kind, BookSourceRangeKind::Exact);
    assert_eq!(location[0].start, diagnostic.span.start);
    assert_eq!(location[0].end, diagnostic.span.end);
    assert!(source[location[0].start..location[0].end].starts_with("[broken]:"));
}

#[test]
fn nested_selected_include_reports_its_full_original_file_offsets() {
    let snippet = "outside 😀\r\n<!-- ANCHOR: example -->\r\n[broken]:\r\n<!-- ANCHOR_END: example -->\r\nafter\r\n";
    let book = BookRenderer::from_sources(
        &[input(
            "manual/start.md",
            "# Start\n\n{{#include ../parts/outer.md}}\n",
        )],
        &[
            input("parts/outer.md", "{{#include inner.md:example}}\n"),
            input("parts/inner.md", snippet),
            input("parts/unused.md", "[broken]:\n"),
        ],
    )
    .unwrap();
    assert_eq!(book.source_diagnostics().diagnostic_count(), 1);
    let location = ranges(&book, 0, 0);
    assert_eq!(location.len(), 1);
    assert_eq!(location[0].path, "parts/inner.md");
    assert_eq!(location[0].start, snippet.find("[broken]").unwrap());
    assert_eq!(&snippet[location[0].start..location[0].end], "[broken]:");
    assert!(
        !book
            .source_diagnostics()
            .to_json()
            .contains("parts/unused.md")
    );
}

#[test]
fn identical_warning_text_in_different_includes_keeps_both_origins() {
    let book = BookRenderer::from_sources(
        &[input(
            "start.md",
            "{{#include one.txt}}\n\n{{#include two.txt}}\n",
        )],
        &[
            input("one.txt", "[broken]:\n"),
            input("two.txt", "[broken]:\n"),
        ],
    )
    .unwrap();
    assert_eq!(book.source_diagnostics().diagnostic_count(), 2);
    assert_eq!(ranges(&book, 0, 0)[0].path, "one.txt");
    assert_eq!(ranges(&book, 0, 1)[0].path, "two.txt");
    assert_eq!(ranges(&book, 0, 0)[0].start, 0);
    assert_eq!(ranges(&book, 0, 1)[0].start, 0);
}

#[test]
fn mapping_only_include_edit_refreshes_locations_without_reparsing() {
    let before = "<!-- ANCHOR: x -->\n[broken]:\n<!-- ANCHOR_END: x -->\n";
    let after = format!("outside 😀\r\n{before}");
    let mut book = BookWorkspace::from_sources(
        &[input("start.md", "{{#include parts/snippet.md:x}}\n")],
        &[input("parts/snippet.md", before)],
    )
    .unwrap();
    let old_diagnostics = book.source_diagnostics().chapters().to_vec();
    let old_start = ranges(&book, 0, 0)[0].start;
    let old_doc = book.book().chapters[0].doc.clone();
    let update = book
        .update_sources_at_revision(&[input("parts/snippet.md", &after)], 0)
        .unwrap();
    assert_eq!(update.revision, 1);
    assert_eq!(update.changed_sources, 1);
    assert!(update.reparsed_chapters.is_empty());
    assert_eq!(book.book().chapters[0].doc, old_doc);
    assert_eq!(book.source_diagnostics().chapters(), old_diagnostics);
    assert_eq!(
        ranges(&book, 0, 0)[0].start,
        old_start + "outside 😀\r\n".len()
    );

    let current = book.source_diagnostics().clone();
    assert!(
        book.update_sources_at_revision(&[input("parts/snippet.md", before)], 0)
            .is_err()
    );
    assert_eq!(book.source_diagnostics(), &current);
    assert!(
        book.update_sources_at_revision(&[input("parts/snippet.md", "no anchor")], 1)
            .is_err()
    );
    assert_eq!(book.source_diagnostics(), &current);
    assert_eq!(book.source_revision(), 1);
}

#[test]
fn source_set_replacement_and_profile_changes_publish_current_locations() {
    let mut book = BookWorkspace::from_sources(
        &[input("first.md", "{{#include parts/old.txt}}\n")],
        &[input("parts/old.txt", "[broken]:\n")],
    )
    .unwrap();
    book.replace_sources_at_revision(
        &[input("second.md", "{{#include moved/new.txt}}\n")],
        &[input("moved/new.txt", "α\n\n[broken]:\n")],
        0,
    )
    .unwrap();
    assert_eq!(ranges(&book, 0, 0)[0].path, "moved/new.txt");
    assert_eq!(ranges(&book, 0, 0)[0].start, "α\n\n".len());
    book.set_authoring_profile(Some(franken_markdown::Profile::CommonMarkGfm))
        .unwrap();
    assert_eq!(ranges(&book, 0, 0)[0].path, "moved/new.txt");
    assert_eq!(ranges(&book, 0, 0)[0].start, "α\n\n".len());
}

#[test]
fn a_warning_crossing_include_and_root_text_keeps_separate_source_ranges() {
    let book = BookRenderer::from_sources(
        &[input(
            "start.md",
            "# Start\n\n{{#include fence.txt}}\nroot tail\n",
        )],
        &[input("fence.txt", "```rust\nlet x = 1;")],
    )
    .unwrap();
    let report = book.source_diagnostics();
    assert_eq!(report.diagnostic_count(), 1);
    assert_eq!(report.chapters()[0].diagnostics[0].code, "unclosed_fence");
    let locations = ranges(&book, 0, 0);
    assert!(
        locations
            .iter()
            .any(|range| range.path == "fence.txt" && range.kind == BookSourceRangeKind::Exact)
    );
    assert!(locations.iter().any(|range| range.path == "fence.txt"
        && range.kind == BookSourceRangeKind::Generated
        && range.start == range.end));
    assert!(
        locations
            .iter()
            .any(|range| range.path == "start.md" && range.kind == BookSourceRangeKind::Exact)
    );
}

#[test]
fn location_metadata_does_not_change_any_publication_bytes() {
    let source = "# Start\n\n[broken]:\n";
    let ordinary = BookRenderer::new(&[input("start.md", source)]).unwrap();
    let included = BookRenderer::from_sources(
        &[input("start.md", "{{#include parts/start.txt}}\n")],
        &[input("parts/start.txt", source)],
    )
    .unwrap();
    assert_eq!(
        ordinary.book().chapters[0].doc,
        included.book().chapters[0].doc
    );
    assert_eq!(
        ordinary.render_site().unwrap(),
        included.render_site().unwrap()
    );
    assert_eq!(
        ordinary.render_epub().unwrap(),
        included.render_epub().unwrap()
    );
    assert_eq!(
        ordinary.render_pdf().unwrap(),
        included.render_pdf().unwrap()
    );
    assert_eq!(ranges(&ordinary, 0, 0)[0].path, "start.md");
    assert_eq!(ranges(&included, 0, 0)[0].path, "parts/start.txt");
}
