#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn file(path: &str, source: &str) -> BookInput {
    BookInput { path: path.into(), source: source.into() }
}

fn chapters() -> Vec<BookInput> {
    vec![
        file("guide/one.md", "# One\n\n[Two](../two.md#two)\n"),
        file("two.md", "# Two\n\n[One](guide/one.md#one)\n"),
        file("three.md", "# Three\n\nUnchanged.\n"),
    ]
}

fn assert_book_eq(actual: &BookWorkspace, expected: &BookRenderer) {
    assert_eq!(actual.source_length(), expected.source_length());
    assert_eq!(actual.book().chapters.len(), expected.book().chapters.len());
    for (a, b) in actual.book().chapters.iter().zip(&expected.book().chapters) {
        assert_eq!(a.path, b.path);
        assert_eq!(a.out_name, b.out_name);
        assert_eq!(a.title, b.title);
        assert_eq!(a.frontmatter, b.frontmatter);
        assert_eq!(a.doc, b.doc);
    }
}

#[test]
fn only_changed_chapter_is_reparsed_and_other_ast_allocations_are_retained() {
    let mut sources = chapters();
    let mut workspace = BookWorkspace::new(&sources).unwrap();
    let retained = workspace.book().chapters[1].doc.blocks.as_ptr();
    sources[0].source = "---\ntitle: Revised\nlang: de\n---\n# New\n\n[Two](../two.md#two)\n".into();
    let report = workspace.update_sources(&[sources[0].clone()]).unwrap();
    assert_eq!(report.revision, 1);
    assert_eq!(report.changed_sources, 1);
    assert_eq!(report.reparsed_chapters, [0]);
    assert_eq!(workspace.book().chapters[1].doc.blocks.as_ptr(), retained);
    assert_book_eq(&workspace, &BookRenderer::new(&sources).unwrap());
    assert_eq!(workspace.book().chapters[0].title, "Revised");
}

#[test]
fn new_links_bind_against_the_entire_book_not_only_the_changed_chapter() {
    let mut workspace = BookWorkspace::new(&chapters()).unwrap();
    let source = "# New\n\n[Peer](../two.md#two) [Third](../three)\n";
    workspace.update_sources(&[file("guide/one.md", source)]).unwrap();
    let crate::Block::Paragraph(inlines) = &workspace.book().chapters[0].doc.blocks[1] else {
        panic!("expected paragraph");
    };
    let destinations: Vec<_> = inlines.iter().filter_map(|inline| {
        if let crate::Inline::Link { dest, .. } = inline { Some(dest.as_str()) } else { None }
    }).collect();
    assert_eq!(destinations, ["/two.md#two", "/three.md"]);
}

#[test]
fn transitive_shared_resources_reparse_all_and_only_changed_chapter_text() {
    let roots = vec![
        file("a.md", "# A\n\n{{#include shared/outer.md}}\n"),
        file("b.md", "# B\n\n{{#include shared/inner.md}}\n"),
        file("c.md", "# C\n\nLiteral.\n"),
    ];
    let mut resources = vec![
        file("shared/outer.md", "{{#include inner.md}}\n"),
        file("shared/inner.md", "Before.\n"),
    ];
    let mut workspace = BookWorkspace::from_sources(&roots, &resources).unwrap();
    let retained = workspace.book().chapters[2].doc.blocks.as_ptr();
    resources[1].source = "After.\n".into();
    let report = workspace.update_sources(&[resources[1].clone()]).unwrap();
    assert_eq!(report.reparsed_chapters, [0, 1]);
    assert_eq!(report.changed_sources, 1);
    assert_eq!(workspace.book().chapters[2].doc.blocks.as_ptr(), retained);
    assert_book_eq(&workspace, &BookRenderer::from_sources(&roots, &resources).unwrap());
}

#[test]
fn edits_outside_selected_snippets_commit_capture_without_reparsing() {
    let roots = [file("a.md", "# A\n\n{{#include rows.txt:2}}\n")];
    let resources = [file("rows.txt", "unused\nSelected\nunused\n"), file("spare.md", "Old")];
    let mut workspace = BookWorkspace::from_sources(&roots, &resources).unwrap();
    let retained = workspace.book().chapters[0].doc.blocks.as_ptr();
    let before = workspace.render_site().unwrap();
    let report = workspace.update_sources(&[
        file("rows.txt", "changed outside\nSelected\ndifferent outside\n"),
        file("spare.md", "New unused resource"),
    ]).unwrap();
    assert_eq!(report.revision, 1);
    assert_eq!(report.changed_sources, 2);
    assert!(report.reparsed_chapters.is_empty());
    assert_eq!(workspace.book().chapters[0].doc.blocks.as_ptr(), retained);
    assert_eq!(workspace.render_site().unwrap(), before);
    // A later root edit uses the latest retained version of the unused resource.
    let report = workspace.update_sources(&[file("a.md", "{{#include spare.md}}")]).unwrap();
    assert_eq!(report.reparsed_chapters, [0]);
    assert!(format!("{:?}", workspace.book().chapters[0].doc).contains("New unused resource"));
}

#[test]
fn parse_only_constructor_keeps_literal_includes_after_source_edits() {
    let mut workspace = BookWorkspace::new(&[file("a.md", "# A")]).unwrap();
    let new = file("a.md", "# A\n\n{{#include missing.md}}\n");
    let report = workspace.update_sources(&[new.clone()]).unwrap();
    assert_eq!(report.reparsed_chapters, [0]);
    assert_book_eq(&workspace, &BookRenderer::new(&[new]).unwrap());
}

#[test]
fn exact_noop_batches_preserve_revision_captures_and_ast_allocations() {
    let sources = chapters();
    let mut workspace = BookWorkspace::new(&sources).unwrap();
    let pointer = workspace.book().chapters[0].doc.blocks.as_ptr();
    let report = workspace.update_sources(&[file("./guide/one.md", &sources[0].source)]).unwrap();
    assert_eq!(report.revision, 0);
    assert_eq!(report.changed_sources, 0);
    assert!(report.reparsed_chapters.is_empty());
    assert_eq!(workspace.book().chapters[0].doc.blocks.as_ptr(), pointer);
}

#[test]
fn bad_second_source_rolls_back_the_entire_batch_and_allows_recovery() {
    let roots = [file("a.md", "# A\n\n{{#include part.md}}\n"), file("b.md", "# B")];
    let mut workspace = BookWorkspace::from_sources(&roots, &[file("part.md", "Before")]).unwrap();
    let before = workspace.render_site().unwrap();
    let old_len = workspace.source_length();
    for bad in [
        vec![file("b.md", "Changed"), file("part.md", "{{#include missing.md}}")],
        vec![file("b.md", "Changed"), file("part.md", "{{#include a.md}}")],
        vec![file("b.md", "Changed"), file("not-selected.md", "Forbidden")],
    ] {
        assert!(workspace.update_sources(&bad).is_err());
        assert_eq!(workspace.source_revision(), 0);
        assert_eq!(workspace.source_length(), old_len);
        assert_eq!(workspace.render_site().unwrap(), before);
        assert_eq!(workspace.book().chapters[1].title, "B");
    }
    let report = workspace.update_sources(&[file("part.md", "Recovered")]).unwrap();
    assert_eq!(report.revision, 1);
    assert_eq!(report.reparsed_chapters, [0]);
}

#[test]
fn batched_edits_resolve_against_one_complete_new_source_set() {
    let roots = [file("a.md", "{{#include x.md}}")];
    let mut workspace = BookWorkspace::from_sources(&roots, &[
        file("x.md", "{{#include y.md}}"), file("y.md", "Old"),
    ]).unwrap();
    // Applying y first would temporarily create a cycle, but the batch's final
    // graph is valid. Only the completed replacement set is expanded.
    let report = workspace.update_sources(&[
        file("y.md", "{{#include x.md}}"), file("x.md", "New"),
    ]).unwrap();
    assert_eq!(report.changed_sources, 2);
    assert_eq!(report.reparsed_chapters, [0]);
    assert_book_eq(&workspace, &BookRenderer::from_sources(&roots, &[
        file("x.md", "New"), file("y.md", "{{#include x.md}}"),
    ]).unwrap());
}

#[test]
fn unknown_duplicate_and_escaping_paths_are_not_partial_edits() {
    let mut workspace = BookWorkspace::new(&chapters()).unwrap();
    let before = workspace.book().chapters[0].doc.clone();
    for updates in [
        vec![],
        vec![file("guide/one.md", "First"), file("guide/./one.md", "Second")],
        vec![file("../outside.md", "Outside")],
        vec![file("/two.md", "Absolute")],
        vec![file("Guide/one.md", "Case change")],
        vec![file("https://host/two.md", "Network")],
        vec![file("two.md", "Changed"), file("missing.md", "Missing")],
    ] {
        assert!(workspace.update_sources(&updates).is_err());
        assert_eq!(workspace.source_revision(), 0);
        assert_eq!(workspace.book().chapters[0].doc, before);
    }
    assert!(workspace.update_sources(&vec![file("two.md", "x"); 4097]).is_err());
}

#[test]
fn final_source_budget_is_atomic_and_independent_of_batch_order() {
    let roots = [file("a.md", &"a".repeat(20)), file("b.md", "b")];
    let updates = [file("b.md", &"b".repeat(20)), file("a.md", "a")];
    let mut left = BookWorkspace::new(&roots).unwrap();
    let mut right = left.clone();
    let first = left.update_with_limit(&updates, 30).unwrap();
    let second = right.update_with_limit(&[updates[1].clone(), updates[0].clone()], 30).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.source_length, 21);
    let before = left.book().chapters[1].doc.clone();
    assert!(left.update_with_limit(&[file("b.md", &"z".repeat(25))], 30).is_err());
    assert_eq!(left.source_revision(), 1);
    assert_eq!(left.book().chapters[1].doc, before);
}

#[test]
fn unicode_bom_and_crlf_counts_are_original_utf8_bytes() {
    let initial = "\u{feff}# α\r\n\r\n😀\r\n";
    let mut workspace = BookWorkspace::new(&[file("a.md", initial)]).unwrap();
    assert_eq!(workspace.update_sources(&[file("a.md", initial)]).unwrap().revision, 0);
    let source = "\u{feff}# β\r\n\r\n😀😀\r\n";
    let report = workspace.update_sources(&[file("a.md", source)]).unwrap();
    assert_eq!(report.source_length, source.len());
    assert_eq!(workspace.chapters[0].source, source);
    assert_book_eq(&workspace, &BookRenderer::new(&[file("a.md", source)]).unwrap());
}

#[test]
fn revision_exhaustion_cannot_wrap_or_mutate_sources() {
    let roots = chapters();
    let mut workspace = BookWorkspace::new(&roots).unwrap();
    workspace.revision = u32::MAX;
    assert!(workspace.update_sources(&[file("two.md", "Changed")]).is_err());
    assert_eq!(workspace.source_revision(), u32::MAX);
    assert_eq!(workspace.book().chapters[1].title, "Two");
    assert_eq!(workspace.update_sources(&[roots[1].clone()]).unwrap().changed_sources, 0);
}

#[test]
fn options_assets_and_all_publication_surfaces_match_a_fresh_rebuild() {
    let mut roots = chapters();
    let mut workspace = BookWorkspace::new(&roots).unwrap();
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#.to_vec();
    workspace.set_image("guide/chart.svg", svg).unwrap();
    let options = workspace.options_mut();
    options.title = Some("Saved metadata".into());
    options.author = Some("Author".into());
    options.metadata_epoch_seconds = Some(0);
    options.custom_css = Some(".fmd { line-height: 1.7; }".into());
    options.toc = true;
    options.page_numbers = true;
    options.font_scale = Some(1.125);
    let asset_pointer = workspace.options().pdf_image_assets[0].bytes.as_ptr();
    roots[0].source = "# Revised\n\n![Chart](chart.svg)\n\n[Next](../two.md#two)\n".into();
    workspace.update_sources(&[roots[0].clone()]).unwrap();
    assert_eq!(workspace.options().pdf_image_assets[0].bytes.as_ptr(), asset_pointer);
    let mut expected = BookRenderer::new(&roots).unwrap();
    *expected.options_mut() = workspace.options().clone();
    assert_book_eq(&workspace, &expected);
    assert_eq!(workspace.render_pdf().unwrap(), expected.render_pdf().unwrap());
    assert_eq!(workspace.render_epub().unwrap(), expected.render_epub().unwrap());
    assert_eq!(workspace.render_site().unwrap(), expected.render_site().unwrap());
    assert_eq!(workspace.render_site_publication().unwrap(), expected.render_site_publication().unwrap());
}

#[test]
fn report_wire_shape_is_bounded_numeric_and_contains_no_source_text() {
    let mut workspace = BookWorkspace::new(&[file("a.md", "Old")]).unwrap();
    let report = workspace.update_sources(&[file("a.md", "New")]).unwrap();
    assert_eq!(report.to_json(), "{\"schema\":\"fmd-book-source-update-v1\",\"revision\":1,\"source_length\":3,\"chapter_count\":1,\"changed_sources\":1,\"reparsed_chapters\":[0]}");
}

#[test]
fn seeded_edit_sequences_match_fresh_full_book_parsing() {
    let mut sources = chapters();
    let mut workspace = BookWorkspace::new(&sources).unwrap();
    let mut seed = 1729u32;
    for round in 0..200 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let index = seed as usize % sources.len();
        let source = format!("# Heading {round}\n\n**Prose** α😀 {seed}\n\n[Peer](/two.md#two)\n");
        sources[index].source = source;
        let report = workspace.update_sources(&[sources[index].clone()]).unwrap();
        assert_eq!(report.reparsed_chapters, [index]);
        assert_eq!(report.revision as usize, round + 1);
        assert_book_eq(&workspace, &BookRenderer::new(&sources).unwrap());
    }
}
