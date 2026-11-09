#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::source_origins::{
    rebase_destinations_mapped_with_profile, rebase_destinations_with_profile,
};
use crate::transclude::{ExpansionSpan, expand_includes_mapped};

fn expanded(root: &str, part: &str) -> ExpandedDocument {
    expand_includes_mapped(root, "manual/start.md", &|_, _| {
        Ok(Some((part.to_owned(), "parts/body.txt".into())))
    })
    .unwrap()
}

fn mapped(location: BookSourceLocation) -> Vec<BookSourceRange> {
    match location {
        BookSourceLocation::Mapped(ranges) => ranges,
        other => panic!("expected mapped range: {other:?}"),
    }
}

#[test]
fn composed_url_mapping_preserves_original_token_and_later_unicode_offsets() {
    let root = "# Root\n\n{{#include part}}\n";
    let part = "[Link](next.md?x=1#end)\r\n\r\n[broken]: α\r\n";
    for profile in [
        None,
        Some(crate::Profile::CommonMarkGfm),
        Some(crate::Profile::GfmPlus),
    ] {
        let expansion = expanded(root, part);
        let expected = rebase_destinations_with_profile(
            expansion.clone(),
            "manual/start.md",
            |origin| Some(origin.into()),
            4096,
            profile,
        )
        .unwrap();
        let (text, map) = rebase_destinations_mapped_with_profile(
            expansion,
            "manual/start.md",
            |origin| Some(origin.into()),
            4096,
            profile,
        )
        .unwrap();
        assert_eq!(text, expected);
        let token = "<../parts/next.md?x=1#end>";
        let start = text.find(token).unwrap();
        // An interior published position maps to the complete original token,
        // never a guessed byte position after a length-changing rewrite.
        let ranges = mapped(map.locate(start + 2..start + 3));
        assert_eq!(
            ranges,
            [BookSourceRange {
                path: "parts/body.txt".into(),
                start: part.find("next.md").unwrap(),
                end: part.find(')').unwrap(),
                kind: BookSourceRangeKind::Rewritten,
            }]
        );
        let start = text.find('α').unwrap();
        let ranges = mapped(map.locate(start..start + 'α'.len_utf8()));
        assert_eq!(ranges[0].kind, BookSourceRangeKind::Exact);
        assert_eq!(ranges[0].start, part.find('α').unwrap());
        assert_eq!(&part[ranges[0].start..ranges[0].end], "α");
    }
}

#[test]
fn selectors_and_removed_markers_do_not_invent_contiguous_original_ranges() {
    let part = "outside\n<!-- ANCHOR: x -->\nα\n// ANCHOR: nested\nβ\n// ANCHOR_END: nested\n<!-- ANCHOR_END: x -->\n";
    let expansion = expanded("{{#include part:x}}", part);
    assert_eq!(expansion.text, "α\nβ\n");
    let map = SourceMap::from_expanded(&expansion, &|origin| Some(origin.into()));
    let ranges = mapped(map.locate(0..expansion.text.len()));
    assert_eq!(ranges.len(), 2);
    assert_eq!(&part[ranges[0].start..ranges[0].end], "α\n");
    assert_eq!(&part[ranges[1].start..ranges[1].end], "β\n");
    assert!(ranges[0].end < ranges[1].start);
}

#[test]
fn repeated_inclusions_remain_distinct_and_generated_separators_are_boundaries() {
    let part = "β";
    let expansion = expanded("{{#include part}}\n{{#include part}}\n", part);
    let map = SourceMap::from_expanded(&expansion, &|origin| Some(origin.into()));
    let ranges = mapped(map.locate(0..expansion.text.len()));
    assert_eq!(
        ranges
            .iter()
            .filter(|range| range.kind == BookSourceRangeKind::Exact)
            .count(),
        2
    );
    for range in ranges
        .iter()
        .filter(|range| range.kind == BookSourceRangeKind::Generated)
    {
        assert_eq!(range.start, part.len());
        assert_eq!(range.end, part.len());
    }
    let end = mapped(map.locate(map.len()..map.len()));
    assert_eq!(end[0].start, part.len());
    assert_eq!(end[0].end, part.len());
}

#[test]
fn many_fragments_refuse_the_whole_location_instead_of_truncating() {
    let text = "x".repeat(MAX_RANGES + 1);
    let expansion = ExpandedDocument {
        text: text.clone(),
        sources: vec!["part.txt".into()],
        spans: (0..text.len())
            .map(|index| ExpansionSpan {
                expanded: index..index + 1,
                source_id: 0,
                original: index * 2..index * 2 + 1,
                generated: false,
            })
            .collect(),
    };
    let map = SourceMap::from_expanded(&expansion, &|origin| Some(origin.into()));
    assert_eq!(
        map.locate(0..text.len()),
        BookSourceLocation::Unavailable("range_limit")
    );
    assert_eq!(mapped(map.locate(0..1))[0].start, 0);
}

#[test]
fn complete_book_metadata_budget_degrades_to_explicit_unavailability() {
    let path = format!("{}.txt", "a".repeat(MAX_PATH - 4));
    let mut budget = SourceMapBudget::default();
    for _ in 0..MAX_PATH_BYTES / MAX_PATH {
        assert!(matches!(
            budget.admit(SourceMap::identity(&path, 1)).locate(0..1),
            BookSourceLocation::Mapped(_)
        ));
    }
    assert_eq!(
        budget.admit(SourceMap::identity(&path, 1)).locate(0..1),
        BookSourceLocation::Unavailable("provenance_limit")
    );
}

#[test]
fn missing_or_inconsistent_origin_metadata_never_produces_a_location() {
    let mut expansion = expanded("{{#include part}}", "abc\n");
    assert_eq!(
        SourceMap::from_expanded(&expansion, &|_| None).locate(0..1),
        BookSourceLocation::Unavailable("source_unavailable")
    );
    expansion.spans[0].original.end += 1;
    assert_eq!(
        SourceMap::from_expanded(&expansion, &|origin| Some(origin.into())).locate(0..1),
        BookSourceLocation::Unavailable("invalid_provenance")
    );
}
