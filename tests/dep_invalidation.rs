//! Public-contract tests for dependency-aware document change analysis.

#![forbid(unsafe_code)]

use franken_markdown::dep_invalidation::{DependencyGraph, DependencyKind};

#[test]
fn scan_identifies_reference_links() {
    let source = "See [the docs][docs-ref] and [inline](url).";
    let graph = DependencyGraph::scan(source);
    let refs: Vec<_> = graph.dependencies().iter()
        .filter(|d| matches!(d.kind, DependencyKind::Reference { .. })).collect();
    assert_eq!(refs.len(), 1, "only the reference candidate is tracked");
    assert_eq!(refs[0].start, 4);
}

#[test]
fn scan_identifies_footnotes() {
    let graph = DependencyGraph::scan("Text[^1] with a note.\n\n[^1]: The note body.");
    let footnotes: Vec<_> = graph.dependencies().iter()
        .filter(|d| matches!(d.kind, DependencyKind::Footnote { .. })).collect();
    assert_eq!(footnotes.len(), 2);
}

#[test]
fn scan_identifies_headings() {
    let graph = DependencyGraph::scan("# Title\n## Section\n### Sub");
    let headings: Vec<_> = graph.dependencies().iter()
        .filter(|d| matches!(d.kind, DependencyKind::Heading { level: 1..=3, .. })).collect();
    assert_eq!(headings.len(), 3);
}

#[test]
fn distant_edit_marks_distant_dirty() {
    let source = "Para one.\n\n[ref]: some-url\n\nPara two.\n\n[other-ref]: other";
    let graph = DependencyGraph::scan(source);
    let result = graph.invalidate(5, 10);
    // Without the replacement text an edit can open a fence or a new reference
    // definition. Range non-overlap is NOT proof of unchanged semantics.
    assert_eq!(result.dirty, vec![(0, source.len())]);
    assert!(result.unchanged.is_empty());
    assert!(result.distant_dirty);
}

#[test]
fn non_overlapping_deps_are_unchanged_only_after_snapshot_comparison() {
    let a = DependencyGraph::scan("First [dep-a].\n\nSecond [dep-b].\n\nThird [dep-c].");
    let b = DependencyGraph::scan("Edited [dep-a].\n\nSecond [dep-b].\n\nThird [dep-c].");
    let change = a.compare(&b);
    assert_eq!(change.dirty_blocks, vec![0]);
    assert_eq!(change.reusable.len(), 2);
    assert_eq!(change.reusable[0].new_index, 1);
    assert_eq!(change.reusable[1].new_index, 2);
}

#[test]
fn overlapping_deps_are_dirty() {
    let source = "Start [ref-a] middle [ref-b] end.";
    let result = DependencyGraph::scan(source).invalidate(5, 10);
    assert_eq!(result.dirty, vec![(0, source.len())]);
    assert!(result.unchanged.is_empty());
}
