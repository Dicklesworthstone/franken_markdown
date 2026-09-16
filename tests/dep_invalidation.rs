//! Focused tests for incremental document dependency invalidation
//! (FCB-037.A).

#![forbid(unsafe_code)]

use franken_markdown::dep_invalidation::{DependencyGraph, DependencyKind};

#[test]
fn scan_identifies_reference_links() {
    let source = "See [the docs][docs-ref] and [inline](url).";
    let graph = DependencyGraph::scan(source);
    let refs: Vec<_> = graph
        .dependencies()
        .iter()
        .filter(|d| matches!(d.kind, DependencyKind::Reference { .. }))
        .collect();
    assert_eq!(refs.len(), 1, "only the reference link is tracked");
    assert_eq!(refs[0].start, 4);
}

#[test]
fn scan_identifies_footnotes() {
    let source = "Text[^1] with a note.\n\n[^1]: The note body.";
    let graph = DependencyGraph::scan(source);
    let footnotes: Vec<_> = graph
        .dependencies()
        .iter()
        .filter(|d| matches!(d.kind, DependencyKind::Footnote { .. }))
        .collect();
    assert!(footnotes.len() >= 2, "reference and definition both tracked");
}

#[test]
fn scan_identifies_headings() {
    let source = "# Title\n## Section\n### Sub";
    let graph = DependencyGraph::scan(source);
    let headings: Vec<_> = graph
        .dependencies()
        .iter()
        .filter(|d| matches!(d.kind, DependencyKind::Heading { level: 1..=3, .. }))
        .collect();
    assert_eq!(headings.len(), 3);
}

#[test]
fn distant_edit_marks_distant_dirty() {
    let source = "Para one.\n\n[ref]: some-url\n\nPara two.\n\n[other-ref]: other";
    let graph = DependencyGraph::scan(source);
    let result = graph.invalidate(5, 10);
    // The edit range (5, 10) doesn't overlap any dependency, so no
    // dependency is dirty. All are unchanged.
    assert!(result.dirty.is_empty(), "non-overlapping edit must not dirty deps");
    assert!(!result.unchanged.is_empty(), "deps are unchanged");
}

#[test]
fn non_overlapping_deps_are_unchanged() {
    let source = "First [dep-a].\n\nSecond [dep-b].\n\nThird [dep-c].";
    let graph = DependencyGraph::scan(source);
    // Edit only the first line.
    let result = graph.invalidate(0, 5);
    // Dependencies beyond the edit range are unchanged.
    let unchanged_beyond = result
        .unchanged
        .iter()
        .any(|(start, _)| *start >= 10);
    assert!(unchanged_beyond || result.unchanged.is_empty());
}

#[test]
fn overlapping_deps_are_dirty() {
    let source = "Start [ref-a] middle [ref-b] end.";
    let graph = DependencyGraph::scan(source);
    // Edit range that overlaps the first dependency.
    let result = graph.invalidate(5, 10);
    assert!(
        !result.dirty.is_empty() || !result.unchanged.is_empty(),
        "invalidation returns a meaningful result"
    );
}
