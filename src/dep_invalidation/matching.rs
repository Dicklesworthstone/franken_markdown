//! Verified, monotone block matching for snapshots with several distant edits.
//!
//! One patience-diff pass finds unique source anchors; exact AST equality is
//! still required because reference definitions can change distant semantics.
//! HashMap hashes only accelerate exact string lookup, never prove equality.
//! We never iterate a map to choose matches. An increasing subsequence of old
//! indices, visited in new-source order, makes the result deterministic and
//! prevents crossing or reusing an occurrence twice. Common edges of each gap
//! recover repeated boilerplate without guessing an ambiguous middle identity.
//!
//! There is no recursive diff or quadratic LCS matrix: auxiliary storage is
//! O(old blocks + new blocks), with O(anchors log anchors) sequence selection.

use super::DependencyGraph;
use std::collections::HashMap;
use std::ops::Range;

pub(super) fn reusable_indices(old: &DependencyGraph, new: &DependencyGraph) -> Vec<(usize, usize)> {
    let old_len = old.document.block_count();
    let new_len = new.document.block_count();
    let equal = |a: usize, b: usize| {
        let before = &old.document.blocks()[a];
        let after = &new.document.blocks()[b];
        before.node == after.node
            && before.span.slice(old.source()).is_some()
            && before.span.slice(old.source()) == after.span.slice(new.source())
    };
    let (prefix, suffix) = common_edges(0..old_len, 0..new_len, &equal);
    let old_end = old_len - suffix;
    let new_end = new_len - suffix;
    let mut matched: Vec<_> = (0..prefix).map(|i| (i, i)).collect();
    let old_unique = unique_sources(old, prefix..old_end);
    let new_unique = unique_sources(new, prefix..new_end);
    let mut candidates = Vec::new();
    for b in prefix..new_end {
        let Some(text) = new.document.blocks()[b].span.slice(new.source()) else {
            continue;
        };
        if new_unique.get(text).copied().flatten() != Some(b) {
            continue;
        }
        if let Some(a) = old_unique.get(text).copied().flatten() {
            if equal(a, b) {
                candidates.push((a, b));
            }
        }
    }

    let mut old_cursor = prefix;
    let mut new_cursor = prefix;
    for (a, b) in increasing_anchors(&candidates) {
        append_edges(&mut matched, old_cursor..a, new_cursor..b, &equal);
        matched.push((a, b));
        old_cursor = a + 1;
        new_cursor = b + 1;
    }
    append_edges(&mut matched, old_cursor..old_end, new_cursor..new_end, &equal);
    matched.extend((0..suffix).map(|i| (old_end + i, new_end + i)));
    matched
}

fn unique_sources(graph: &DependencyGraph, range: Range<usize>) -> HashMap<&str, Option<usize>> {
    let mut unique = HashMap::new();
    for index in range {
        if let Some(text) = graph.document.blocks()[index].span.slice(graph.source()) {
            unique.entry(text).and_modify(|entry| *entry = None).or_insert(Some(index));
        }
    }
    unique
}

fn common_edges(
    old: Range<usize>,
    new: Range<usize>,
    equal: &impl Fn(usize, usize) -> bool,
) -> (usize, usize) {
    let mut prefix = 0;
    let mut suffix = 0;
    while prefix < old.len().min(new.len()) && equal(old.start + prefix, new.start + prefix) {
        prefix += 1;
    }
    while suffix < old.len() - prefix && suffix < new.len() - prefix
        && equal(old.end - suffix - 1, new.end - suffix - 1)
    {
        suffix += 1;
    }
    (prefix, suffix)
}

fn append_edges(
    matched: &mut Vec<(usize, usize)>,
    old: Range<usize>,
    new: Range<usize>,
    equal: &impl Fn(usize, usize) -> bool,
) {
    let (prefix, suffix) = common_edges(old.clone(), new.clone(), equal);
    matched.extend((0..prefix).map(|i| (old.start + i, new.start + i)));
    matched.extend((0..suffix).map(|i| (old.end - suffix + i, new.end - suffix + i)));
}

// Candidates arrive in new-source order with distinct old indices. Keep the
// smallest tail for each subsequence length; links reconstruct real anchors,
// not the (possibly mutually incompatible) final tail entries themselves.
fn increasing_anchors(candidates: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; candidates.len()];
    for (index, &(old, _)) in candidates.iter().enumerate() {
        let slot = tails.partition_point(|&tail| candidates[tail].0 < old);
        if slot > 0 {
            previous[index] = Some(tails[slot - 1]);
        }
        if slot == tails.len() {
            tails.push(index);
        } else {
            tails[slot] = index;
        }
    }
    let mut anchors = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(index) = cursor {
        anchors.push(candidates[index]);
        cursor = previous[index];
    }
    anchors.reverse();
    anchors
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::dep_invalidation::{FlowAssetReuse, FlowSession};
    use crate::flow_display::{AssetResult, FlowDisplayLimits, FlowLayoutOptions};
    use crate::fonts::BundledFlowFonts;
    use crate::FontFamily;

    fn check_partition(old: &DependencyGraph, new: &DependencyGraph) {
        let change = old.compare(new);
        let mut old_indices = change.removed_blocks.clone();
        let mut new_indices = change.dirty_blocks.clone();
        for pair in &change.reusable {
            old_indices.push(pair.old_index);
            new_indices.push(pair.new_index);
            assert_eq!(pair.old_span.slice(old.source()), pair.new_span.slice(new.source()));
            assert_eq!(old.document.blocks()[pair.old_index].node, new.document.blocks()[pair.new_index].node);
        }
        for window in change.reusable.windows(2) {
            assert!(window[0].old_index < window[1].old_index);
            assert!(window[0].new_index < window[1].new_index);
        }
        old_indices.sort_unstable();
        new_indices.sort_unstable();
        assert_eq!(old_indices, (0..old.document.block_count()).collect::<Vec<_>>());
        assert_eq!(new_indices, (0..new.document.block_count()).collect::<Vec<_>>());
        assert_eq!(change, old.compare(new), "matching must be deterministic");
    }

    #[test]
    fn distant_edits_preserve_the_verified_middle() {
        let old = DependencyGraph::scan("old start\n\nstable one\n\nstable two\n\nold end\n");
        let new = DependencyGraph::scan("東京 start\n\nstable one\n\nstable two\n\nnew end\n");
        let change = old.compare(&new);
        assert_eq!(change.dirty_blocks, vec![0, 3]);
        assert_eq!(change.removed_blocks, vec![0, 3]);
        assert_eq!(change.reusable.iter().map(|p| (p.old_index, p.new_index)).collect::<Vec<_>>(), vec![(1, 1), (2, 2)]);
        assert_ne!(change.reusable[0].old_span, change.reusable[0].new_span);
        check_partition(&old, &new);
    }

    #[test]
    fn repeated_gap_edges_are_recovered_but_ambiguous_middles_are_not_guessed() {
        let old = DependencyGraph::scan("old left\n\nsame\n\nanchor\n\nsame\n\nold right\n");
        let new = DependencyGraph::scan("new left\n\nsame\n\nanchor\n\nsame\n\nnew right\n");
        assert_eq!(old.compare(&new).reusable.len(), 3);
        check_partition(&old, &new);
        let old = DependencyGraph::scan("old left\n\nsame\n\nsame\n\nold right\n");
        let new = DependencyGraph::scan("new left\n\nsame\n\nsame\n\nnew right\n");
        assert!(old.compare(&new).reusable.is_empty());
        check_partition(&old, &new);
    }

    #[test]
    fn insertions_deletions_and_reordering_never_cross_occurrences() {
        let snapshots = [
            "", "a\n", "a\n\nb\n\nc\n", "c\n\nb\n\na\n",
            "a\n\na\n\nb\n", "new\n\na\n\nb\n\nc\n\ntail\n",
            "left\n\na\n\na\n\nb\n\nright\n",
        ];
        for a in snapshots {
            for b in snapshots {
                check_partition(&DependencyGraph::scan(a), &DependencyGraph::scan(b));
            }
        }
        assert_eq!(increasing_anchors(&[(4, 0), (1, 1), (3, 2), (2, 3), (5, 4)]), vec![(1, 1), (2, 3), (5, 4)]);
    }

    #[test]
    fn exact_source_is_not_a_proof_when_reference_semantics_change() {
        let old = DependencyGraph::scan("old start\n\n[use][r]\n\nstable\n\nold end\n\n[r]: old\n");
        let new = DependencyGraph::scan("new start\n\n[use][r]\n\nstable\n\nnew end\n\n[r]: new\n");
        let change = old.compare(&new);
        assert!(!change.global_context_changed);
        assert!(change.dirty_blocks.contains(&1));
        assert!(change.reusable.iter().any(|p| p.old_index == 2 && p.new_index == 2));
        check_partition(&old, &new);
    }

    #[test]
    fn global_render_context_still_refuses_middle_reuse() {
        let old = DependencyGraph::scan("# Before\n\nstable\n\nold tail\n");
        let new = DependencyGraph::scan("# After\n\nstable\n\nnew tail\n");
        let change = old.compare(&new);
        assert!(change.global_context_changed);
        assert!(change.reusable.is_empty());
        check_partition(&old, &new);
    }

    #[test]
    fn session_retains_middle_assets_only_with_explicit_host_authorization() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        for reuse in [FlowAssetReuse::Invalidate, FlowAssetReuse::HostVerifiedUnchanged] {
            let mut session = FlowSession::new(
                "old start\n\n![plot](plot.png)\n\nstable\n\nold tail\n", 2,
                FlowDisplayLimits::default(), FlowLayoutOptions::default(),
                |t, s, r, i| fonts.shape(t, s, r, i),
            ).unwrap();
            let request = session.pending_assets()[0].clone();
            session.provide_asset(AssetResult {
                request_id: request.id, generation: request.generation,
                width: 20, height: 10, bytes: Some(vec![1, 2]),
            }, |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
            let update = session.replace_source(
                1, "new start\n\n![plot](plot.png)\n\nstable\n\nnew tail\n", reuse,
                |t, s, r, i| fonts.shape(t, s, r, i),
            ).unwrap();
            assert_eq!(update.changes.reusable.len(), 2);
            if reuse == FlowAssetReuse::HostVerifiedUnchanged {
                assert_eq!(update.reused_assets.len(), 1);
                assert!(session.pending_assets().is_empty());
                assert_eq!(session.engine().resolved_assets()[0].bytes, Some(vec![1, 2]));
                assert_eq!(session.engine().resolved_assets()[0].generation, 2);
            } else {
                assert!(update.reused_assets.is_empty());
                assert_eq!(session.pending_assets().len(), 1);
            }
        }
    }
}
