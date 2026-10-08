#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::book::BookRenderer;

fn file(path: &str, source: &str) -> BookInput {
    BookInput { path: path.into(), source: source.into() }
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
fn adding_removing_renaming_and_reordering_rebinds_the_complete_book() {
    let old = [
        file("guide/one.md", "# One\n\n[Other](../other.md#other)\n"),
        file("other.md", "# Other"),
        file("removed.md", "# Removed"),
    ];
    let mut workspace = BookWorkspace::new(&old).unwrap();
    let next = [
        file("new.md", "# New\n\n[One](guide/one.md#one)"),
        old[0].clone(),
        file("renamed.md", "# Other"),
    ];
    let report = workspace.replace_sources_at_revision(&next, &[], 0).unwrap();
    assert_eq!(report.revision, 1);
    assert!(report.changed);
    assert_eq!(report.chapter_count, 3);
    assert_eq!(report.resource_count, 0);
    assert_eq!(report.reparsed_chapter_count, 3);
    assert_book_eq(&workspace, &BookRenderer::new(&next).unwrap());
    assert!(workspace.update_sources(&[file("other.md", "gone")]).is_err());
    workspace.update_sources_at_revision(&[file("new.md", "# Updated")], 1).unwrap();
    assert_eq!(workspace.source_revision(), 2);
    assert_eq!(workspace.book().chapters[0].title, "Updated");
}

#[test]
fn reorder_alone_is_a_change_but_normalized_exact_capture_is_not() {
    let roots = [file("a.md", "# A"), file("b.md", "# B")];
    let mut workspace = BookWorkspace::new(&roots).unwrap();
    let ptr = workspace.book().chapters[0].doc.blocks.as_ptr();
    let report = workspace.replace_sources_at_revision(
        &[file("./a.md", "# A"), file("sub/../b.md", "# B")], &[], 0,
    ).unwrap();
    assert!(!report.changed);
    assert_eq!(report.revision, 0);
    assert_eq!(report.reparsed_chapter_count, 0);
    assert_eq!(workspace.book().chapters[0].doc.blocks.as_ptr(), ptr);
    let report = workspace.replace_sources_at_revision(
        &[roots[1].clone(), roots[0].clone()], &[], 0,
    ).unwrap();
    assert!(report.changed);
    assert_eq!(report.revision, 1);
    assert_eq!(workspace.book().chapters[0].path, "b.md");
}

#[test]
fn new_include_graph_is_admitted_together_and_future_text_edits_use_it() {
    let mut workspace = BookWorkspace::from_sources(
        &[file("old.md", "{{#include old.txt}}")], &[file("old.txt", "Before")],
    ).unwrap();
    let roots = [file("guide/new.md", "# New\n\n{{#include ../parts/new.md}}")];
    let resources = [
        file("parts/new.md", "{{#include detail.txt}}"),
        file("parts/detail.txt", "After"),
    ];
    let report = workspace.replace_sources_at_revision(&roots, &resources, 0).unwrap();
    assert_eq!(report.resource_count, 2);
    assert_book_eq(&workspace, &BookRenderer::from_sources(&roots, &resources).unwrap());
    workspace.update_sources_at_revision(&[file("parts/detail.txt", "Latest")], 1).unwrap();
    assert!(format!("{:?}", workspace.book().chapters[0].doc).contains("Latest"));
    assert!(workspace.update_sources(&[file("old.txt", "No longer selected")]).is_err());
}

#[test]
fn resources_can_be_promoted_to_chapters_and_chapters_demoted_to_resources() {
    let roots = [file("one.md", "# One")];
    let resources = [file("two.md", "# Two\n\n{{#include one.md}}")];
    let mut workspace = BookWorkspace::from_sources(&roots, &resources).unwrap();
    let report = workspace.replace_sources_at_revision(&resources, &roots, 0).unwrap();
    assert_eq!(report.chapter_count, 1);
    assert_eq!(report.resource_count, 1);
    assert_eq!(workspace.book().chapters[0].path, "two.md");
    assert_book_eq(&workspace, &BookRenderer::from_sources(&resources, &roots).unwrap());
}

#[test]
fn failed_source_sets_preserve_capture_options_assets_and_rendered_output() {
    let roots = [file("a.md", "# A\n\n{{#include part.md}}")];
    let resources = [file("part.md", "Body")];
    let mut workspace = BookWorkspace::from_sources(&roots, &resources).unwrap();
    workspace.options_mut().title = Some("Retain metadata".into());
    workspace.set_image("figure.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"2\"><rect width=\"2\" height=\"2\"/></svg>".to_vec()).unwrap();
    let pointer = workspace.options().pdf_image_assets[0].bytes.as_ptr();
    let before = workspace.render_site().unwrap();
    for (chapters, includes) in [
        (vec![], vec![]),
        (vec![file("../escape.md", "x")], vec![]),
        (vec![file("/absolute.md", "x")], vec![]),
        (vec![file("a.md", "x"), file("./a.md", "y")], vec![]),
        (vec![file("a/b.md", "x"), file("a__b.md", "y")], vec![]),
        (vec![file("a.md", "x")], vec![file("a.md", "duplicate")]),
        (roots.to_vec(), vec![]),
        (roots.to_vec(), vec![file("part.md", "{{#include a.md}}")]),
    ] {
        assert!(workspace.replace_sources_at_revision(&chapters, &includes, 0).is_err());
        assert_eq!(workspace.source_revision(), 0);
        assert_eq!(workspace.options().title.as_deref(), Some("Retain metadata"));
        assert_eq!(workspace.options().pdf_image_assets[0].bytes.as_ptr(), pointer);
        assert_eq!(workspace.render_site().unwrap(), before);
    }
    workspace.update_sources_at_revision(&[file("part.md", "Recovered")], 0).unwrap();
    assert_eq!(workspace.source_revision(), 1);
}

#[test]
fn all_outputs_match_a_fresh_book_without_cloning_retained_asset_buffers() {
    let mut workspace = BookWorkspace::from_sources(&[file("old.md", "# Old")], &[]).unwrap();
    let options = workspace.options_mut();
    options.title = Some("Publication".into());
    options.author = Some("Author".into());
    options.metadata_epoch_seconds = Some(0);
    options.toc = true;
    options.page_numbers = true;
    options.base_font_size = Some(12.0);
    options.optimal_pagination = true;
    options.custom_css = Some(".fmd { font-size: 1rem; }".into());
    workspace.set_image("figure.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"2\"><rect width=\"2\" height=\"2\"/></svg>".to_vec()).unwrap();
    let image = workspace.options().pdf_image_assets[0].bytes.as_ptr();
    let title = workspace.options().title.as_ref().unwrap().as_ptr();
    let roots = [
        file("second.md", "# Second\n\n[First](first.md#first) and a note[^1].\n\n[^1]: Second note."),
        file("first.md", "# First\n\n[Second](second.md#second) and ![Figure](figure.svg).\n\nCitation[^1].\n\n[^1]: First note."),
    ];
    let mut fresh = BookRenderer::from_sources(&roots, &[]).unwrap();
    *fresh.options_mut() = workspace.options().clone();
    workspace.replace_sources_at_revision(&roots, &[], 0).unwrap();
    assert_eq!(workspace.options().pdf_image_assets[0].bytes.as_ptr(), image);
    assert_eq!(workspace.options().title.as_ref().unwrap().as_ptr(), title);
    assert_eq!(workspace.render_site().unwrap(), fresh.render_site().unwrap());
    assert_eq!(workspace.render_epub().unwrap(), fresh.render_epub().unwrap());
    assert_eq!(workspace.render_pdf().unwrap(), fresh.render_pdf().unwrap());
}

#[test]
fn parse_only_policy_cannot_be_changed_by_replacement() {
    let mut workspace = BookWorkspace::new(&[file("old.md", "# Old")]).unwrap();
    let roots = [file("new.md", "{{#include missing.md}}")];
    assert!(workspace.replace_sources_at_revision(&roots, &[file("missing.md", "Text")], 0).is_err());
    workspace.replace_sources_at_revision(&roots, &[], 0).unwrap();
    assert_book_eq(&workspace, &BookRenderer::new(&roots).unwrap());
    assert!(workspace.expanded.is_none());
}

#[test]
fn revisions_are_shared_with_text_edits_and_stale_even_for_noops() {
    let roots = [file("a.md", "# A")];
    let mut workspace = BookWorkspace::new(&roots).unwrap();
    workspace.update_sources_at_revision(&[file("a.md", "# Changed")], 0).unwrap();
    assert!(workspace.replace_sources_at_revision(&roots, &[], 0).is_err());
    workspace.replace_sources_at_revision(&roots, &[], 1).unwrap();
    assert!(workspace.update_sources_at_revision(&[file("a.md", "Stale")], 1).is_err());
    assert!(workspace.replace_sources_at_revision(&roots, &[], 1).is_err());
    workspace.revision = u32::MAX;
    assert!(!workspace.replace_sources_at_revision(&roots, &[], u32::MAX).unwrap().changed);
    assert!(workspace.replace_sources_at_revision(&[file("b.md", "# B")], &[], u32::MAX).is_err());
    assert_eq!(workspace.source_revision(), u32::MAX);
    assert_eq!(workspace.book().chapters[0].path, "a.md");
}

#[test]
fn ingress_budget_includes_utf8_paths_resources_and_combined_count() {
    let mut workspace = BookWorkspace::from_sources(&[file("old.md", "# Old")], &[]).unwrap();
    let roots = [file("a.md", "中")];
    let resources = [file("b.txt", "😀")];
    let exact = 4 + 3 + 5 + 4;
    assert!(workspace.replace_set_with_limit(&roots, &resources, 0, exact - 1).is_err());
    assert_eq!(workspace.source_revision(), 0);
    let report = workspace.replace_set_with_limit(&roots, &resources, 0, exact).unwrap();
    assert_eq!(report.source_length, 7);
    assert_eq!(report.revision, 1);
    assert!(workspace.replace_sources_at_revision(&roots, &vec![file("x.txt", ""); 4096], 1).is_err());
    assert!(workspace.replace_sources_at_revision(&vec![file("x.md", ""); 4097], &[], 1).is_err());
    assert_eq!(workspace.source_revision(), 1);
}

#[test]
fn receipt_is_a_bounded_distinct_source_set_contract() {
    let mut workspace = BookWorkspace::new(&[file("a.md", "secret")]).unwrap();
    let report = workspace.replace_sources_at_revision(&[file("b.md", "other")], &[], 0).unwrap();
    assert_eq!(report.to_json(), "{\"schema\":\"fmd-book-source-set-v1\",\"revision\":1,\"source_length\":5,\"chapter_count\":1,\"resource_count\":0,\"changed\":true,\"reparsed_chapter_count\":1}");
    assert!(!report.to_json().contains("secret"));
    assert!(!report.to_json().contains("b.md"));
}
