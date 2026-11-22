//! Semantic Markdown AST diffing and visual change rendering.
//!
//! Provides structural LCS diffing between two Markdown ASTs at the block and
//! inline levels, calculating similarity scores and rendering visual green/red
//! side-by-side or unified change reports in HTML and PDF.

use crate::ast::{Block, Document, Inline};
use crate::theme::Theme;
use std::collections::{BTreeMap, VecDeque};

mod canonical;
mod html;

use canonical::canonical_block;

/// High-level change metrics between two Markdown documents.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DiffStats {
    /// Number of unchanged blocks.
    pub unchanged_blocks: usize,
    /// Number of inserted blocks.
    pub inserted_blocks: usize,
    /// Number of deleted blocks.
    pub deleted_blocks: usize,
    /// Number of modified blocks.
    pub modified_blocks: usize,
    /// Word-level insertion count.
    pub words_inserted: usize,
    /// Word-level deletion count.
    pub words_deleted: usize,
    /// Structural similarity coefficient (0.0 = completely different, 1.0 = identical).
    pub similarity_ratio: f32,
}

/// A semantic diff element at the block level.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffBlock {
    /// Block is identical in both documents.
    Unchanged(Block),
    /// Block was added in the new document.
    Inserted(Block),
    /// Block was removed from the old document.
    Deleted(Block),
    /// Block was modified between old and new versions.
    Modified {
        old: Box<Block>,
        new: Box<Block>,
        inline_diff: Vec<DiffInline>,
    },
}

/// A fine-grained diff element at the inline/phrase level.
#[derive(Debug, Clone, PartialEq)]
pub enum DiffInline {
    /// Inline text/element is unchanged.
    Unchanged(Inline),
    /// Inline text/element was inserted.
    Inserted(Inline),
    /// Inline text/element was deleted.
    Deleted(Inline),
}

/// Aggregated semantic diff between two Markdown documents.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentDiff {
    pub old_name: String,
    pub new_name: String,
    pub stats: DiffStats,
    pub blocks: Vec<DiffBlock>,
}

#[derive(Debug, Clone, PartialEq)]
enum RawDiffItem<T> {
    Unchanged(T),
    Inserted(T),
    Deleted(T),
}

/// Compute semantic structural diff between two parsed Markdown documents.
#[must_use]
pub fn compute_diff(
    doc_a: &Document,
    doc_b: &Document,
    name_a: &str,
    name_b: &str,
) -> DocumentDiff {
    let raw_diff = lcs_diff(&doc_a.blocks, &doc_b.blocks);
    let mut blocks = Vec::new();
    let mut stats = DiffStats::default();

    let mut i = 0;
    while i < raw_diff.len() {
        match &raw_diff[i] {
            RawDiffItem::Unchanged(b) => {
                stats.unchanged_blocks += 1;
                blocks.push(DiffBlock::Unchanged(b.clone()));
                i += 1;
            }
            RawDiffItem::Deleted(_) | RawDiffItem::Inserted(_) => {
                // Collect contiguous cluster of Deleted and Inserted items
                let mut dels = Vec::new();
                let mut inss = Vec::new();
                while i < raw_diff.len() {
                    match &raw_diff[i] {
                        RawDiffItem::Deleted(b) => {
                            dels.push(b.clone());
                            i += 1;
                        }
                        RawDiffItem::Inserted(b) => {
                            inss.push(b.clone());
                            i += 1;
                        }
                        RawDiffItem::Unchanged(_) => break,
                    }
                }

                // Keep the first unused insertion of each compatible kind.
                // Scanning every insertion again for each deletion made the
                // bounded LCS fallback quadratic on large replacement edits.
                let mut candidates: [VecDeque<usize>; 5] = Default::default();
                for (index, block) in inss.iter().enumerate() {
                    if let Some(kind) = pairable_block_kind(block) {
                        candidates[kind].push_back(index);
                    }
                }
                let mut used_inss = vec![false; inss.len()];
                for del in dels {
                    let paired =
                        pairable_block_kind(&del).and_then(|kind| candidates[kind].pop_front());
                    if let Some(ins_idx) = paired {
                        let ins = &inss[ins_idx];
                        used_inss[ins_idx] = true;
                        let inline_diff = diff_block_inlines(&del, ins, &mut stats);
                        stats.modified_blocks += 1;
                        blocks.push(DiffBlock::Modified {
                            old: Box::new(del),
                            new: Box::new(ins.clone()),
                            inline_diff,
                        });
                    } else {
                        stats.deleted_blocks += 1;
                        count_block_words(&del, &mut stats.words_deleted);
                        blocks.push(DiffBlock::Deleted(del));
                    }
                }

                for (ins_idx, ins) in inss.into_iter().enumerate() {
                    if !used_inss[ins_idx] {
                        stats.inserted_blocks += 1;
                        count_block_words(&ins, &mut stats.words_inserted);
                        blocks.push(DiffBlock::Inserted(ins));
                    }
                }
            }
        }
    }

    let total_operations = stats.unchanged_blocks
        + stats.inserted_blocks
        + stats.deleted_blocks
        + stats.modified_blocks;
    stats.similarity_ratio = if total_operations == 0 {
        1.0
    } else {
        (stats.unchanged_blocks as f32) / (total_operations as f32)
    };

    DocumentDiff {
        old_name: name_a.to_string(),
        new_name: name_b.to_string(),
        stats,
        blocks,
    }
}

#[inline(always)]
fn pairable_block_kind(block: &Block) -> Option<usize> {
    match block {
        Block::Paragraph(_) => Some(0),
        Block::Heading { .. } => Some(1),
        Block::CodeBlock { .. } => Some(2),
        Block::Table(_) => Some(3),
        Block::MathBlock(_) => Some(4),
        _ => None,
    }
}

fn diff_block_inlines(old: &Block, new: &Block, stats: &mut DiffStats) -> Vec<DiffInline> {
    match (old, new) {
        (Block::Paragraph(in_a), Block::Paragraph(in_b))
        | (Block::Heading { inlines: in_a, .. }, Block::Heading { inlines: in_b, .. }) => {
            let raw = lcs_diff(in_a, in_b);
            raw.into_iter()
                .map(|item| match item {
                    RawDiffItem::Unchanged(inl) => DiffInline::Unchanged(inl),
                    RawDiffItem::Inserted(inl) => {
                        count_inline_words(&inl, &mut stats.words_inserted);
                        DiffInline::Inserted(inl)
                    }
                    RawDiffItem::Deleted(inl) => {
                        count_inline_words(&inl, &mut stats.words_deleted);
                        DiffInline::Deleted(inl)
                    }
                })
                .collect()
        }
        (Block::CodeBlock { code: c_a, .. }, Block::CodeBlock { code: c_b, .. }) => {
            let words_a: Vec<&str> = c_a.split_whitespace().collect();
            let words_b: Vec<&str> = c_b.split_whitespace().collect();
            let raw = lcs_diff(&words_a, &words_b);
            for item in raw {
                match item {
                    RawDiffItem::Inserted(_) => stats.words_inserted += 1,
                    RawDiffItem::Deleted(_) => stats.words_deleted += 1,
                    RawDiffItem::Unchanged(_) => {}
                }
            }
            Vec::new()
        }
        (Block::MathBlock(m_a), Block::MathBlock(m_b)) => {
            let words_a: Vec<&str> = m_a.split_whitespace().collect();
            let words_b: Vec<&str> = m_b.split_whitespace().collect();
            let raw = lcs_diff(&words_a, &words_b);
            for item in raw {
                match item {
                    RawDiffItem::Inserted(_) => stats.words_inserted += 1,
                    RawDiffItem::Deleted(_) => stats.words_deleted += 1,
                    RawDiffItem::Unchanged(_) => {}
                }
            }
            Vec::new()
        }
        (Block::Table(t_a), Block::Table(t_b)) => {
            let mut words_a = Vec::new();
            let mut words_b = Vec::new();
            for cell in &t_a.head {
                for inl in cell {
                    collect_inline_words(inl, &mut words_a);
                }
            }
            for row in &t_a.rows {
                for cell in row {
                    for inl in cell {
                        collect_inline_words(inl, &mut words_a);
                    }
                }
            }
            for cell in &t_b.head {
                for inl in cell {
                    collect_inline_words(inl, &mut words_b);
                }
            }
            for row in &t_b.rows {
                for cell in row {
                    for inl in cell {
                        collect_inline_words(inl, &mut words_b);
                    }
                }
            }
            let raw = lcs_diff(&words_a, &words_b);
            for item in raw {
                match item {
                    RawDiffItem::Inserted(_) => stats.words_inserted += 1,
                    RawDiffItem::Deleted(_) => stats.words_deleted += 1,
                    RawDiffItem::Unchanged(_) => {}
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

#[inline(always)]
fn collect_inline_words<'a>(inline: &'a Inline, words: &mut Vec<&'a str>) {
    match inline {
        Inline::Text(t)
        | Inline::Code(t)
        | Inline::Image { alt: t, .. }
        | Inline::Math(t)
        | Inline::DisplayMath(t) => {
            words.extend(t.split_whitespace());
        }
        Inline::Emphasis(inner)
        | Inline::Strong(inner)
        | Inline::Strikethrough(inner)
        | Inline::Link { content: inner, .. } => {
            for inl in inner {
                collect_inline_words(inl, words);
            }
        }
        _ => {}
    }
}

/// Maximum DP cells before trying a bounded edit-frontier search.
/// 16M cells = 64 MB of u32; beyond that, the quadratic cost is a DoS vector.
const LCS_MAX_CELLS: usize = 16 * 1024 * 1024;

fn lcs_diff<T: PartialEq + Clone>(old: &[T], new: &[T]) -> Vec<RawDiffItem<T>> {
    let m = old.len();
    let n = new.len();
    if m == 0 && n == 0 {
        return Vec::new();
    }
    if m == 0 {
        return new.iter().cloned().map(RawDiffItem::Inserted).collect();
    }
    if n == 0 {
        return old.iter().cloned().map(RawDiffItem::Deleted).collect();
    }

    // Fast-path: trim common prefix
    let mut start = 0;
    while start < m && start < n && old[start] == new[start] {
        start += 1;
    }

    // Fast-path: trim common suffix
    let mut end_old = m;
    let mut end_new = n;
    while end_old > start && end_new > start && old[end_old - 1] == new[end_new - 1] {
        end_old -= 1;
        end_new -= 1;
    }

    let mut out = Vec::with_capacity(m + n);
    for item in &old[..start] {
        out.push(RawDiffItem::Unchanged(item.clone()));
    }

    let sub_old = &old[start..end_old];
    let sub_new = &new[start..end_new];
    let sub_m = sub_old.len();
    let sub_n = sub_new.len();

    if sub_m > 0 || sub_n > 0 {
        if sub_m == 0 {
            out.extend(sub_new.iter().cloned().map(RawDiffItem::Inserted));
        } else if sub_n == 0 {
            out.extend(sub_old.iter().cloned().map(RawDiffItem::Deleted));
        } else if sub_m
            .saturating_add(1)
            .saturating_mul(sub_n.saturating_add(1))
            > LCS_MAX_CELLS
        {
            if let Some(edits) = bounded_myers_edits(sub_old, sub_new) {
                append_sequence_edits(&mut out, sub_old, sub_new, &edits);
            } else {
                out.extend(sub_old.iter().cloned().map(RawDiffItem::Deleted));
                out.extend(sub_new.iter().cloned().map(RawDiffItem::Inserted));
            }
        } else {
            let stride = sub_n + 1;
            let mut dp = vec![0u32; (sub_m + 1) * stride];

            for i in 0..sub_m {
                for j in 0..sub_n {
                    if sub_old[i] == sub_new[j] {
                        dp[(i + 1) * stride + (j + 1)] = dp[i * stride + j] + 1;
                    } else {
                        let up = dp[i * stride + (j + 1)];
                        let left = dp[(i + 1) * stride + j];
                        dp[(i + 1) * stride + (j + 1)] = up.max(left);
                    }
                }
            }

            let mut mid_diff = Vec::with_capacity(sub_m + sub_n);
            let mut i = sub_m;
            let mut j = sub_n;

            while i > 0 || j > 0 {
                if i > 0 && j > 0 && sub_old[i - 1] == sub_new[j - 1] {
                    mid_diff.push(RawDiffItem::Unchanged(sub_old[i - 1].clone()));
                    i -= 1;
                    j -= 1;
                } else if j > 0 && (i == 0 || dp[i * stride + (j - 1)] >= dp[(i - 1) * stride + j])
                {
                    mid_diff.push(RawDiffItem::Inserted(sub_new[j - 1].clone()));
                    j -= 1;
                } else if i > 0 && (j == 0 || dp[i * stride + (j - 1)] < dp[(i - 1) * stride + j]) {
                    mid_diff.push(RawDiffItem::Deleted(sub_old[i - 1].clone()));
                    i -= 1;
                }
            }

            mid_diff.reverse();
            out.extend(mid_diff);
        }
    }

    for item in &old[end_old..] {
        out.push(RawDiffItem::Unchanged(item.clone()));
    }

    out
}

/// A single edit at the predecessor coordinates of an edit-graph edge.
/// Matching snakes are implicit between edges, so reconstruction stores O(D)
/// edits rather than allocating an index pair for every unchanged element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SequenceEdit {
    old: usize,
    new: usize,
    insert: bool,
}

const MYERS_MAX_WORK: usize = 4 * 1024 * 1024;
const MYERS_MAX_TRACE_CELLS: usize = 1024 * 1024;
const UNREACHABLE: usize = usize::MAX;

/// Exact shortest edit paths for large, lightly edited documents. Both the
/// frontier walk and comparisons inside matching snakes spend the same work
/// budget. Packed parity frontiers cap the trace at 1M usize cells (8 MiB on
/// 64-bit hosts); exhaustion returns no partial alignment or guessed matches.
fn bounded_myers_edits<T: PartialEq>(old: &[T], new: &[T]) -> Option<Vec<SequenceEdit>> {
    let mut remaining = MYERS_MAX_WORK;
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let mut trace_cells = 0usize;
    for depth in 0usize.. {
        let row_len = depth.checked_add(1)?;
        trace_cells = trace_cells.checked_add(row_len)?;
        if trace_cells > MYERS_MAX_TRACE_CELLS {
            return None;
        }
        let mut frontier = vec![UNREACHABLE; row_len];
        for (index, furthest) in frontier.iter_mut().enumerate() {
            remaining = remaining.checked_sub(1)?;
            let (mut x, mut y) = if depth == 0 {
                (0, 0)
            } else {
                let Some(step) = myers_step(&trace[depth - 1], depth, index, old.len(), new.len())
                else {
                    continue;
                };
                (
                    step.old + usize::from(!step.insert),
                    step.new + usize::from(step.insert),
                )
            };
            while x < old.len() && y < new.len() {
                remaining = remaining.checked_sub(1)?;
                if old[x] != new[y] {
                    break;
                }
                x += 1;
                y += 1;
            }
            *furthest = x;
            if x == old.len() && y == new.len() {
                let mut edits = Vec::with_capacity(depth);
                // The completed snake's predecessor belongs to the previous
                // frontier. The final partially-filled row is never needed.
                for distance in (1..=depth).rev() {
                    let index = if x >= y {
                        (distance + x - y) / 2
                    } else {
                        (distance - (y - x)) / 2
                    };
                    let step =
                        myers_step(&trace[distance - 1], distance, index, old.len(), new.len())?;
                    x = step.old;
                    y = step.new;
                    edits.push(step);
                }
                edits.reverse();
                return Some(edits);
            }
        }
        trace.push(frontier);
    }
    None
}

/// Choose the furthest reachable predecessor on one Myers diagonal. On equal
/// post-edit x coordinates insertion wins, matching the standard frontier rule.
fn myers_step(
    previous: &[usize],
    depth: usize,
    index: usize,
    old_len: usize,
    new_len: usize,
) -> Option<SequenceEdit> {
    let position = |index: usize| {
        let &x = previous.get(index)?;
        if x == UNREACHABLE {
            return None;
        }
        let twice = index * 2;
        let y = if twice >= depth - 1 {
            x.checked_sub(twice - (depth - 1))?
        } else {
            x.checked_add(depth - 1 - twice)?
        };
        Some((x, y))
    };
    let deletion = index.checked_sub(1).and_then(position).and_then(|(x, y)| {
        (x < old_len && y <= new_len).then_some(SequenceEdit {
            old: x,
            new: y,
            insert: false,
        })
    });
    let insertion = position(index).and_then(|(x, y)| {
        (x <= old_len && y < new_len).then_some(SequenceEdit {
            old: x,
            new: y,
            insert: true,
        })
    });
    match (deletion, insertion) {
        (Some(del), Some(ins)) if ins.old > del.old => Some(ins),
        (Some(del), _) => Some(del),
        (_, insertion) => insertion,
    }
}

fn append_sequence_edits<T: Clone>(
    out: &mut Vec<RawDiffItem<T>>,
    old: &[T],
    new: &[T],
    edits: &[SequenceEdit],
) {
    let (mut old_cursor, mut new_cursor) = (0, 0);
    for edit in edits {
        debug_assert_eq!(edit.old - old_cursor, edit.new - new_cursor);
        out.extend(
            old[old_cursor..edit.old]
                .iter()
                .cloned()
                .map(RawDiffItem::Unchanged),
        );
        old_cursor = edit.old;
        new_cursor = edit.new;
        if edit.insert {
            out.push(RawDiffItem::Inserted(new[new_cursor].clone()));
            new_cursor += 1;
        } else {
            out.push(RawDiffItem::Deleted(old[old_cursor].clone()));
            old_cursor += 1;
        }
    }
    debug_assert_eq!(old.len() - old_cursor, new.len() - new_cursor);
    out.extend(
        old[old_cursor..]
            .iter()
            .cloned()
            .map(RawDiffItem::Unchanged),
    );
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod sequence_tests {
    use super::*;
    use std::cell::Cell;

    /// Independent insert/delete edit-distance oracle. This deliberately uses
    /// the ordinary distance recurrence, not Myers frontiers or backtracking.
    fn edit_distance(old: &[u8], new: &[u8]) -> usize {
        let mut previous: Vec<usize> = (0..=new.len()).collect();
        for (i, left) in old.iter().enumerate() {
            let mut current = vec![i + 1];
            for (j, right) in new.iter().enumerate() {
                current.push(if left == right {
                    previous[j]
                } else {
                    previous[j + 1].min(current[j]) + 1
                });
            }
            previous = current;
        }
        previous[new.len()]
    }

    #[test]
    fn bounded_myers_matches_independent_optimal_distance_and_replays_both_inputs() {
        let mut cases = vec![Vec::new()];
        for length in 1..=4 {
            for mut encoded in 0..3usize.pow(length) {
                let mut sequence = Vec::new();
                for _ in 0..length {
                    sequence.push((encoded % 3) as u8);
                    encoded /= 3;
                }
                cases.push(sequence);
            }
        }
        for old in &cases {
            for new in &cases {
                let edits = bounded_myers_edits(old, new).unwrap();
                assert_eq!(edits.len(), edit_distance(old, new), "{old:?} -> {new:?}");
                let mut diff = Vec::new();
                append_sequence_edits(&mut diff, old, new, &edits);
                let before: Vec<_> = diff
                    .iter()
                    .filter_map(|item| match item {
                        RawDiffItem::Unchanged(value) | RawDiffItem::Deleted(value) => Some(*value),
                        RawDiffItem::Inserted(_) => None,
                    })
                    .collect();
                let after: Vec<_> = diff
                    .iter()
                    .filter_map(|item| match item {
                        RawDiffItem::Unchanged(value) | RawDiffItem::Inserted(value) => {
                            Some(*value)
                        }
                        RawDiffItem::Deleted(_) => None,
                    })
                    .collect();
                assert_eq!(&before, old);
                assert_eq!(&after, new);
            }
        }
    }

    #[test]
    fn small_lcs_keeps_the_established_ambiguous_tie_choice() {
        assert_eq!(
            lcs_diff(&['a', 'b'], &['b', 'a']),
            [
                RawDiffItem::Deleted('a'),
                RawDiffItem::Unchanged('b'),
                RawDiffItem::Inserted('a'),
            ]
        );
    }

    #[test]
    fn disjoint_myers_search_is_bounded_and_never_returns_a_partial_path() {
        struct Counted<'a> {
            value: usize,
            comparisons: &'a Cell<usize>,
        }
        impl PartialEq for Counted<'_> {
            fn eq(&self, other: &Self) -> bool {
                self.comparisons.set(self.comparisons.get() + 1);
                self.value == other.value
            }
        }
        let comparisons = Cell::new(0);
        let values = |range: std::ops::Range<usize>| {
            range
                .map(|value| Counted {
                    value,
                    comparisons: &comparisons,
                })
                .collect::<Vec<_>>()
        };
        let old = values(0..12_000);
        let new = values(12_000..24_000);
        assert!(bounded_myers_edits(&old, &new).is_none());
        assert!(comparisons.get() > 0);
        assert!(comparisons.get() <= MYERS_MAX_WORK);
    }
}

fn count_block_words(block: &Block, count: &mut usize) {
    match block {
        Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
            for inl in inlines {
                count_inline_words(inl, count);
            }
        }
        Block::BlockQuote(inner) => {
            for b in inner {
                count_block_words(b, count);
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for b in &item.blocks {
                    count_block_words(b, count);
                }
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                for inl in cell {
                    count_inline_words(inl, count);
                }
            }
            for row in &table.rows {
                for cell in row {
                    for inl in cell {
                        count_inline_words(inl, count);
                    }
                }
            }
        }
        Block::CodeBlock { code, .. } => {
            *count += code.split_whitespace().count();
        }
        Block::MathBlock(math) => {
            *count += math.split_whitespace().count();
        }
        Block::DefinitionList(items) => {
            for item in items {
                for term in &item.terms {
                    for inl in term {
                        count_inline_words(inl, count);
                    }
                }
                for def in &item.definitions {
                    for inl in def {
                        count_inline_words(inl, count);
                    }
                }
            }
        }
        Block::FootnoteDefinition { blocks, .. } => {
            for b in blocks {
                count_block_words(b, count);
            }
        }
        _ => {}
    }
}

#[inline(always)]
fn count_inline_words(inline: &Inline, count: &mut usize) {
    match inline {
        Inline::Text(t)
        | Inline::Code(t)
        | Inline::Image { alt: t, .. }
        | Inline::Math(t)
        | Inline::DisplayMath(t) => {
            *count += t.split_whitespace().count();
        }
        Inline::Emphasis(inner)
        | Inline::Strong(inner)
        | Inline::Strikethrough(inner)
        | Inline::Link { content: inner, .. } => {
            for inl in inner {
                count_inline_words(inl, count);
            }
        }
        _ => {}
    }
}

impl DocumentDiff {
    /// Render both revisions with highlighted changes and complete document context.
    #[must_use]
    pub fn to_html(&self, theme: &Theme) -> String {
        self.to_html_with_options(&crate::HtmlOptions {
            theme: theme.clone(),
            ..Default::default()
        })
    }

    /// Render a self-contained comparison using the ordinary HTML renderer's
    /// styles, host-supplied image/font assets, language, and trust policy.
    /// Each revision retains its own heading and footnote namespace.
    #[must_use]
    pub fn to_html_with_options(&self, options: &crate::HtmlOptions) -> String {
        html::render(self, options)
    }

    /// Render machine-readable JSON representation of diff.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(4096);
        out.push('{');
        out.push_str("\"schema\":\"fmd-diff-v1\",");
        out.push_str(&format!(
            "\"old_name\":\"{}\",",
            json_escape(&self.old_name)
        ));
        out.push_str(&format!(
            "\"new_name\":\"{}\",",
            json_escape(&self.new_name)
        ));
        out.push_str("\"stats\":{");
        out.push_str(&format!(
            "\"unchanged_blocks\":{},",
            self.stats.unchanged_blocks
        ));
        out.push_str(&format!(
            "\"inserted_blocks\":{},",
            self.stats.inserted_blocks
        ));
        out.push_str(&format!(
            "\"deleted_blocks\":{},",
            self.stats.deleted_blocks
        ));
        out.push_str(&format!(
            "\"modified_blocks\":{},",
            self.stats.modified_blocks
        ));
        out.push_str(&format!(
            "\"words_inserted\":{},",
            self.stats.words_inserted
        ));
        out.push_str(&format!("\"words_deleted\":{},", self.stats.words_deleted));
        out.push_str(&format!(
            "\"similarity_ratio\":{:.3}",
            self.stats.similarity_ratio
        ));
        out.push_str("},");
        out.push_str(&format!("\"total_diff_blocks\":{}", self.blocks.len()));
        out.push('}');
        out
    }
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut clean_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0..=0x1f => {
                if clean_start < i {
                    out.push_str(&s[clean_start..i]);
                }
                use std::fmt::Write;
                let _ = write!(out, "\\u{b:04x}");
                clean_start = i + 1;
                continue;
            }
            _ => continue,
        };
        if clean_start < i {
            out.push_str(&s[clean_start..i]);
        }
        out.push_str(esc);
        clean_start = i + 1;
    }
    if clean_start < s.len() {
        out.push_str(&s[clean_start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_markdown;

    #[test]
    fn compute_diff_detects_block_and_inline_modifications() {
        let md_a = "# Title\n\nOriginal paragraph text.\n\n```rust\nlet x = 1;\n```\n";
        let md_b =
            "# Title\n\nUpdated paragraph text with additions.\n\n```rust\nlet x = 2;\n```\n";

        let doc_a = parse_markdown(md_a);
        let doc_b = parse_markdown(md_b);

        let diff = compute_diff(&doc_a, &doc_b, "v1.md", "v2.md");

        assert_eq!(diff.stats.unchanged_blocks, 1); // Heading is unchanged
        assert_eq!(diff.stats.modified_blocks, 2); // Paragraph and CodeBlock are modified
        assert!(diff.stats.words_inserted > 0);

        let html = diff.to_html(&Theme::default());
        assert!(html.contains("Comparing <code>v1.md</code> &rarr; <code>v2.md</code>"));
        assert!(html.contains("<ins class=\"diff-inline\">"));

        let json = diff.to_json();
        assert!(json.contains("\"schema\":\"fmd-diff-v1\""));
        assert!(json.contains("\"modified_blocks\":2"));
    }

    #[test]
    fn compute_diff_tracks_code_math_table_words() {
        let md_a = "```rust\nlet count = 10;\n```\n\n$$\nx + y\n$$\n\n| Item | Val |\n|---|---|\n| foo | 1 |\n";
        let md_b = "```rust\nlet count = 20;\n```\n\n$$\nx + z\n$$\n\n| Item | Val |\n|---|---|\n| foo | 2 |\n";

        let doc_a = parse_markdown(md_a);
        let doc_b = parse_markdown(md_b);

        let diff = compute_diff(&doc_a, &doc_b, "a.md", "b.md");

        assert_eq!(diff.stats.modified_blocks, 3);
        assert_eq!(diff.stats.words_inserted, 3); // "20;", "z", "2"
        assert_eq!(diff.stats.words_deleted, 3); // "10;", "y", "1"
    }
}
// ===========================================================================
// Semantic AST diff report: `diff_documents` / `DiffReport` / `report_json` /
// `report_text`.
//
// A complementary, machine-oriented surface next to the visual `DocumentDiff`
// above: it classifies each top-level block change into a stable `ChangeKind`
// taxonomy and renders it as compact per-change JSON or plain text. Alignment
// is heading-aware: blocks are keyed by (kind, canonical text) and aligned
// with the shared `lcs_diff` core over those keys, so a renamed heading is
// reported as `HeadingChanged`, never as remove+add.
//
// Two JSON surfaces live in this module, with distinct schemas:
// - `DocumentDiff::to_json` — schema `fmd-diff-v1`: aggregate stats payload
//   (block/word counters, similarity ratio) for the visual diff.
// - `report_json` — schema `fmd-diff-changes-v1`: per-change taxonomy payload
//   (kind, summary, old/new block indices) from `diff_documents`.
//
// Semantics and deliberate choices:
// - Reflow-insensitive: whitespace runs in prose are collapsed before
//   comparison, so re-wrapping a paragraph is not a change (unlike the
//   `PartialEq`-on-`Block` alignment in `compute_diff`). Code, math, and raw
//   HTML payloads are compared verbatim — their whitespace is meaningful.
// - Ordering is deterministic: changes are emitted in document order; move
//   detection scans old blocks in order and takes the first exact-key match
//   on the new side.
// - Moves are detected by an exact-key match among otherwise unmatched
//   blocks (bounded to 4M candidate pairs; past that the blocks fall back to
//   add/remove reporting). The LCS middle is bounded to 16M DP cells, with an
//   indexed greedy in-order scan beyond that. Candidate pairing is indexed
//   too, so a memory fallback cannot reopen a quadratic whole-document scan.
// - The AST carries no frontmatter block, so `FrontmatterChanged` is
//   reserved and never emitted.
// - Block kinds without a dedicated `ChangeKind` (block quote, thematic
//   break, raw HTML, footnote definition, math block, definition list) are
//   reported under the closest category; `summary` always names the true
//   block kind.

/// The category of a semantic change between two document versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// A heading exists only in the new document.
    HeadingAdded,
    /// A heading exists only in the old document.
    HeadingRemoved,
    /// A heading's text and/or level changed; `level` is the new level.
    HeadingChanged { level: u8 },
    /// Paragraph (or prose container) text changed, or a prose block was
    /// added/removed; inspect `old_index`/`new_index` for the direction.
    ParagraphEdited,
    /// A code-like block changed; `lang` is the new fence info word, if any.
    CodeBlockChanged { lang: Option<String> },
    /// A table's cells, alignment, or shape changed.
    TableChanged,
    /// Link/image text is unchanged but a destination changed.
    LinkTargetChanged { old: String, new: String },
    /// A block is unchanged but its position changed.
    BlockMoved,
    /// A list's items or structure changed.
    ListChanged,
    /// Reserved: the AST has no frontmatter block, so this is never emitted.
    FrontmatterChanged,
}

/// One semantic change, addressed by top-level block indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The change category.
    pub kind: ChangeKind,
    /// Short, deterministic human-readable description.
    pub summary: String,
    /// Index into `old.blocks` (`None` for pure additions).
    pub old_index: Option<usize>,
    /// Index into `new.blocks` (`None` for pure removals).
    pub new_index: Option<usize>,
}

/// The result of [`diff_documents`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffReport {
    /// All changes, in deterministic document order.
    pub changes: Vec<Change>,
    /// True when no semantic differences were found.
    pub identical: bool,
}

/// A top-level block reduced to its diff-relevant fingerprint.
struct KeyedBlock {
    /// Coarse block-kind tag (see `block_tag`).
    tag: u8,
    /// Canonical key: kind prefix + structural inline markers + link dests.
    key: String,
    /// Same as `key` but with link/image destinations and titles elided;
    /// used to spot `LinkTargetChanged`.
    key_no_dest: String,
    /// Link/image destinations in document order.
    dests: Vec<String>,
    /// Whitespace-normalized plain text for human summaries.
    plain: String,
    /// Heading level (0 for non-headings).
    level: u8,
    /// Code fence info word (None for non-code blocks).
    lang: Option<String>,
}

impl KeyedBlock {
    fn from_block(block: &Block) -> Self {
        let (level, lang) = match block {
            Block::Heading { level, .. } => (*level, None),
            Block::CodeBlock { lang, .. } => (0, lang.clone()),
            _ => (0, None),
        };
        KeyedBlock {
            tag: block_tag(block),
            key: canonical_block(block, true),
            key_no_dest: canonical_block(block, false),
            dests: block_link_dests(block),
            plain: plain_block(block),
            level,
            lang,
        }
    }

    fn same_block(&self, other: &KeyedBlock) -> bool {
        self.tag == other.tag && self.key == other.key
    }
}

/// Coarse, stable block-kind tags.
#[inline(always)]
fn block_tag(block: &Block) -> u8 {
    match block {
        Block::Heading { .. } => 0,
        Block::Paragraph(_) => 1,
        Block::CodeBlock { .. } => 2,
        Block::BlockQuote(_) => 3,
        Block::List(_) => 4,
        Block::Table(_) => 5,
        Block::ThematicBreak => 6,
        Block::HtmlBlock(_) => 7,
        Block::FootnoteDefinition { .. } => 8,
        Block::MathBlock(_) => 9,
        Block::DefinitionList(_) => 10,
        Block::PageBreak => 11,
    }
}

#[inline(always)]
fn block_name(tag: u8) -> &'static str {
    match tag {
        0 => "heading",
        1 => "paragraph",
        2 => "code block",
        3 => "block quote",
        4 => "list",
        5 => "table",
        6 => "thematic break",
        7 => "HTML block",
        8 => "footnote definition",
        9 => "math block",
        10 => "definition list",
        11 => "page break",
        _ => "block",
    }
}

/// Collapse every whitespace run to a single space and trim the ends.
/// This is what makes paragraph reflow a no-op for the differ.
fn normalize_prose(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Link/image destinations of a block, in document order.
fn block_link_dests(block: &Block) -> Vec<String> {
    let mut out = Vec::new();
    collect_block_dests(block, &mut out);
    out
}

fn collect_block_dests(block: &Block, out: &mut Vec<String>) {
    match block {
        Block::PageBreak => {}
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            collect_inline_dests(inlines, out);
        }
        Block::BlockQuote(inner) => {
            for child in inner {
                collect_block_dests(child, out);
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for child in &item.blocks {
                    collect_block_dests(child, out);
                }
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                collect_inline_dests(cell, out);
            }
            for row in &table.rows {
                for cell in row {
                    collect_inline_dests(cell, out);
                }
            }
        }
        Block::FootnoteDefinition { blocks, .. } => {
            for child in blocks {
                collect_block_dests(child, out);
            }
        }
        Block::DefinitionList(items) => {
            for item in items {
                for term in &item.terms {
                    collect_inline_dests(term, out);
                }
                for def in &item.definitions {
                    collect_inline_dests(def, out);
                }
            }
        }
        Block::CodeBlock { .. }
        | Block::ThematicBreak
        | Block::HtmlBlock(_)
        | Block::MathBlock(_) => {}
    }
}

fn collect_inline_dests(inlines: &[Inline], out: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Link { dest, content, .. } => {
                out.push(dest.clone());
                collect_inline_dests(content, out);
            }
            Inline::Image { dest, .. } => out.push(dest.clone()),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => collect_inline_dests(children, out),
            _ => {}
        }
    }
}

/// Whitespace-normalized plain text of a block, for human summaries.
fn plain_block(block: &Block) -> String {
    let mut s = String::new();
    plain_block_into(block, &mut s);
    normalize_prose(&s)
}

fn plain_block_into(block: &Block, out: &mut String) {
    match block {
        Block::PageBreak => out.push_str("[page break]"),
        Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
            plain_inlines(inlines, out);
        }
        Block::CodeBlock { code, .. } => out.push_str(code),
        Block::BlockQuote(inner) => {
            for child in inner {
                plain_block_into(child, out);
                out.push(' ');
            }
        }
        Block::List(list) => {
            for item in &list.items {
                for child in &item.blocks {
                    plain_block_into(child, out);
                    out.push(' ');
                }
            }
        }
        Block::Table(table) => {
            for cell in &table.head {
                plain_inlines(cell, out);
                out.push(' ');
            }
            for row in &table.rows {
                for cell in row {
                    plain_inlines(cell, out);
                    out.push(' ');
                }
            }
        }
        Block::ThematicBreak => {}
        Block::HtmlBlock(html) => out.push_str(html),
        Block::FootnoteDefinition { id, blocks } => {
            out.push_str(id);
            out.push(' ');
            for child in blocks {
                plain_block_into(child, out);
                out.push(' ');
            }
        }
        Block::MathBlock(math) => out.push_str(math),
        Block::DefinitionList(items) => {
            for item in items {
                for term in &item.terms {
                    plain_inlines(term, out);
                    out.push(' ');
                }
                for def in &item.definitions {
                    plain_inlines(def, out);
                    out.push(' ');
                }
            }
        }
    }
}

fn plain_inlines(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strikethrough(children) => plain_inlines(children, out),
            Inline::Code(code) => out.push_str(code),
            Inline::Link { content, .. } => plain_inlines(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(html) => out.push_str(html),
            Inline::FootnoteRef { id } => out.push_str(id),
            Inline::Math(math) | Inline::DisplayMath(math) => out.push_str(math),
        }
    }
}

/// First `MAX_CHARS` chars of `text`, plus `...` when truncated.
fn snippet(text: &str) -> String {
    const MAX_CHARS: usize = 40;
    let mut iter = text.chars();
    let mut s: String = (&mut iter).take(MAX_CHARS).collect();
    if iter.next().is_some() {
        s.push_str("...");
    }
    s
}

/// Alignment between two keyed block sequences: anchor pairs plus matched
/// flags. Common prefixes/suffixes are trimmed first; the middle is aligned
/// with the shared `lcs_diff` core over canonical keys, bounded to 16M DP
/// cells with an indexed greedy in-order scan beyond that.
struct Alignment {
    anchors: Vec<(usize, usize)>,
    matched_old: Vec<bool>,
    matched_new: Vec<bool>,
}

fn align_blocks(old: &[KeyedBlock], new: &[KeyedBlock]) -> Alignment {
    let mut matched_old = vec![false; old.len()];
    let mut matched_new = vec![false; new.len()];
    let mut anchors: Vec<(usize, usize)> = Vec::new();

    let mut start = 0usize;
    while start < old.len() && start < new.len() && old[start].same_block(&new[start]) {
        matched_old[start] = true;
        matched_new[start] = true;
        anchors.push((start, start));
        start += 1;
    }
    let mut end_old = old.len();
    let mut end_new = new.len();
    let mut suffix_anchors: Vec<(usize, usize)> = Vec::new();
    while end_old > start && end_new > start && old[end_old - 1].same_block(&new[end_new - 1]) {
        end_old -= 1;
        end_new -= 1;
        matched_old[end_old] = true;
        matched_new[end_new] = true;
        suffix_anchors.push((end_old, end_new));
    }

    let om = end_old - start;
    let nm = end_new - start;
    const MAX_CELLS: usize = 16_000_000;
    if om > 0 && nm > 0 && om.saturating_mul(nm) <= MAX_CELLS {
        let old_mid: Vec<String> = old[start..end_old].iter().map(|k| k.key.clone()).collect();
        let new_mid: Vec<String> = new[start..end_new].iter().map(|k| k.key.clone()).collect();
        let raw = lcs_diff(&old_mid, &new_mid);
        let (mut oi, mut ni) = (start, start);
        for item in raw {
            match item {
                RawDiffItem::Unchanged(_) => {
                    matched_old[oi] = true;
                    matched_new[ni] = true;
                    anchors.push((oi, ni));
                    oi += 1;
                    ni += 1;
                }
                RawDiffItem::Deleted(_) => oi += 1,
                RawDiffItem::Inserted(_) => ni += 1,
            }
        }
    } else if om > 0 && nm > 0 {
        let mut positions: BTreeMap<(u8, &str), Vec<usize>> = BTreeMap::new();
        for (index, block) in new.iter().enumerate().take(end_new).skip(start) {
            positions
                .entry((block.tag, block.key.as_str()))
                .or_default()
                .push(index);
        }
        let mut cursor = start;
        for (i, block) in old.iter().enumerate().take(end_old).skip(start) {
            if let Some(indices) = positions.get(&(block.tag, block.key.as_str()))
                && let Some(&j) = indices.get(indices.partition_point(|&index| index < cursor))
            {
                matched_old[i] = true;
                matched_new[j] = true;
                anchors.push((i, j));
                cursor = j + 1;
            }
        }
    }

    anchors.extend(suffix_anchors);
    anchors.sort_unstable();
    Alignment {
        anchors,
        matched_old,
        matched_new,
    }
}

fn first_dest_diff(old: &[String], new: &[String]) -> (String, String) {
    let max = old.len().max(new.len());
    for i in 0..max {
        let o = old.get(i);
        let n = new.get(i);
        if o != n {
            return (
                o.cloned().unwrap_or_default(),
                n.cloned().unwrap_or_default(),
            );
        }
    }
    (String::new(), String::new())
}

/// Build the change for two paired blocks of the same kind whose keys differ.
fn changed_change(old: &KeyedBlock, new: &KeyedBlock, oi: usize, ni: usize) -> Change {
    let (kind, summary) = match old.tag {
        0 => {
            let summary = if old.plain == new.plain && old.level != new.level {
                format!(
                    "heading level changed {} -> {}: \"{}\"",
                    old.level,
                    new.level,
                    snippet(&new.plain)
                )
            } else if old.level != new.level {
                format!(
                    "heading changed (level {} -> {}): \"{}\" -> \"{}\"",
                    old.level,
                    new.level,
                    snippet(&old.plain),
                    snippet(&new.plain)
                )
            } else if old.plain == new.plain {
                format!(
                    "heading content or formatting changed: \"{}\"",
                    snippet(&new.plain)
                )
            } else {
                format!(
                    "heading changed: \"{}\" -> \"{}\"",
                    snippet(&old.plain),
                    snippet(&new.plain)
                )
            };
            (ChangeKind::HeadingChanged { level: new.level }, summary)
        }
        1 => {
            if old.key_no_dest == new.key_no_dest && old.dests != new.dests {
                let (old_dest, new_dest) = first_dest_diff(&old.dests, &new.dests);
                (
                    ChangeKind::LinkTargetChanged {
                        old: old_dest.clone(),
                        new: new_dest.clone(),
                    },
                    format!("link target changed: {old_dest} -> {new_dest}"),
                )
            } else if old.plain == new.plain {
                (
                    ChangeKind::ParagraphEdited,
                    format!(
                        "paragraph content or formatting changed: \"{}\"",
                        snippet(&new.plain)
                    ),
                )
            } else {
                (
                    ChangeKind::ParagraphEdited,
                    format!(
                        "paragraph edited: \"{}\" -> \"{}\"",
                        snippet(&old.plain),
                        snippet(&new.plain)
                    ),
                )
            }
        }
        2 => (
            ChangeKind::CodeBlockChanged {
                lang: new.lang.clone(),
            },
            format!(
                "code block changed ({})",
                new.lang.as_deref().unwrap_or("plain")
            ),
        ),
        3 => (
            ChangeKind::ParagraphEdited,
            format!(
                "block quote changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        4 => (
            ChangeKind::ListChanged,
            format!(
                "list changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        5 => (
            ChangeKind::TableChanged,
            format!(
                "table changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        7 => (
            ChangeKind::CodeBlockChanged { lang: None },
            String::from("HTML block changed"),
        ),
        8 => (
            ChangeKind::ParagraphEdited,
            format!(
                "footnote definition changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        9 => (
            ChangeKind::CodeBlockChanged { lang: None },
            String::from("math block changed"),
        ),
        10 => (
            ChangeKind::ListChanged,
            format!(
                "definition list changed: \"{}\" -> \"{}\"",
                snippet(&old.plain),
                snippet(&new.plain)
            ),
        ),
        _ => (
            ChangeKind::ParagraphEdited,
            format!("{} changed", block_name(old.tag)),
        ),
    };
    Change {
        kind,
        summary,
        old_index: Some(oi),
        new_index: Some(ni),
    }
}

/// Build the change for an unpaired block (pure addition or removal).
fn add_remove_change(block: &KeyedBlock, index: usize, added: bool) -> Change {
    let kind = match (block.tag, added) {
        (0, true) => ChangeKind::HeadingAdded,
        (0, false) => ChangeKind::HeadingRemoved,
        (2, _) => ChangeKind::CodeBlockChanged {
            lang: block.lang.clone(),
        },
        (4, _) => ChangeKind::ListChanged,
        (5, _) => ChangeKind::TableChanged,
        (7, _) | (9, _) => ChangeKind::CodeBlockChanged { lang: None },
        (10, _) => ChangeKind::ListChanged,
        _ => ChangeKind::ParagraphEdited,
    };
    let mut summary = format!(
        "{} {}",
        block_name(block.tag),
        if added { "added" } else { "removed" }
    );
    if block.tag == 2
        && let Some(lang) = &block.lang
    {
        summary.push_str(&format!(" ({lang})"));
    }
    if !block.plain.is_empty() {
        summary.push_str(&format!(": \"{}\"", snippet(&block.plain)));
    }
    Change {
        kind,
        summary,
        old_index: if added { None } else { Some(index) },
        new_index: if added { Some(index) } else { None },
    }
}

/// Pair unmatched blocks inside one gap between two anchors: same-kind
/// in-order pairing becomes a typed change, leftovers become add/remove.
fn pair_gap(
    okeys: &[KeyedBlock],
    nkeys: &[KeyedBlock],
    matched_old: &[bool],
    matched_new: &[bool],
    olds: std::ops::Range<usize>,
    news: std::ops::Range<usize>,
    changes: &mut Vec<Change>,
) {
    let gap_olds: Vec<usize> = olds.filter(|&i| !matched_old[i]).collect();
    let gap_news: Vec<usize> = news.filter(|&i| !matched_new[i]).collect();
    let mut candidates: BTreeMap<u8, VecDeque<usize>> = BTreeMap::new();
    for (index, &ni) in gap_news.iter().enumerate() {
        candidates
            .entry(nkeys[ni].tag)
            .or_default()
            .push_back(index);
    }
    let mut used_new = vec![false; gap_news.len()];
    for &oi in &gap_olds {
        let paired = candidates
            .get_mut(&okeys[oi].tag)
            .and_then(VecDeque::pop_front);
        match paired {
            Some(k) => {
                used_new[k] = true;
                changes.push(changed_change(
                    &okeys[oi],
                    &nkeys[gap_news[k]],
                    oi,
                    gap_news[k],
                ));
            }
            None => changes.push(add_remove_change(&okeys[oi], oi, false)),
        }
    }
    for (k, &ni) in gap_news.iter().enumerate() {
        if !used_new[k] {
            changes.push(add_remove_change(&nkeys[ni], ni, true));
        }
    }
}

/// Compute the semantic diff between two parsed Markdown documents.
///
/// Block-level, heading-aware, reflow-insensitive, and fully deterministic:
/// identical inputs always produce a byte-identical report.
#[must_use]
pub fn diff_documents(old: &Document, new: &Document) -> DiffReport {
    if old == new {
        return DiffReport {
            changes: Vec::new(),
            identical: true,
        };
    }
    let okeys: Vec<KeyedBlock> = old.blocks.iter().map(KeyedBlock::from_block).collect();
    let nkeys: Vec<KeyedBlock> = new.blocks.iter().map(KeyedBlock::from_block).collect();
    let alignment = align_blocks(&okeys, &nkeys);
    let mut matched_old = alignment.matched_old;
    let mut matched_new = alignment.matched_new;

    let mut changes: Vec<Change> = Vec::new();

    // Global move pass: exact-key matches among otherwise unmatched blocks.
    const MAX_MOVE_CELLS: usize = 4_000_000;
    let rem_count = matched_old.iter().filter(|&&m| !m).count();
    let add_count = matched_new.iter().filter(|&&m| !m).count();
    if rem_count.saturating_mul(add_count) <= MAX_MOVE_CELLS {
        let mut candidates: BTreeMap<(u8, &str), VecDeque<usize>> = BTreeMap::new();
        for (ni, nkey) in nkeys.iter().enumerate() {
            if !matched_new[ni] {
                candidates
                    .entry((nkey.tag, nkey.key.as_str()))
                    .or_default()
                    .push_back(ni);
            }
        }
        for (oi, okey) in okeys.iter().enumerate() {
            if matched_old[oi] {
                continue;
            }
            if let Some(ni) = candidates
                .get_mut(&(okey.tag, okey.key.as_str()))
                .and_then(VecDeque::pop_front)
            {
                matched_old[oi] = true;
                matched_new[ni] = true;
                changes.push(Change {
                    kind: ChangeKind::BlockMoved,
                    summary: format!(
                        "{} moved from block {} to block {}: \"{}\"",
                        block_name(okey.tag),
                        oi,
                        ni,
                        snippet(&okey.plain)
                    ),
                    old_index: Some(oi),
                    new_index: Some(ni),
                });
            }
        }
    }

    // Pair leftovers inside each gap between consecutive anchors.
    let mut prev = (0usize, 0usize);
    for &(ai, aj) in &alignment.anchors {
        pair_gap(
            &okeys,
            &nkeys,
            &matched_old,
            &matched_new,
            prev.0..ai,
            prev.1..aj,
            &mut changes,
        );
        prev = (ai + 1, aj + 1);
    }
    pair_gap(
        &okeys,
        &nkeys,
        &matched_old,
        &matched_new,
        prev.0..okeys.len(),
        prev.1..nkeys.len(),
        &mut changes,
    );

    // Deterministic document order; stable sort preserves emission order on
    // position ties.
    changes.sort_by_key(|c| c.old_index.or(c.new_index).unwrap_or(0));
    DiffReport {
        identical: changes.is_empty(),
        changes,
    }
}

#[inline(always)]
fn change_kind_str(kind: &ChangeKind) -> &'static str {
    match kind {
        ChangeKind::HeadingAdded => "heading_added",
        ChangeKind::HeadingRemoved => "heading_removed",
        ChangeKind::HeadingChanged { .. } => "heading_changed",
        ChangeKind::ParagraphEdited => "paragraph_edited",
        ChangeKind::CodeBlockChanged { .. } => "code_block_changed",
        ChangeKind::TableChanged => "table_changed",
        ChangeKind::LinkTargetChanged { .. } => "link_target_changed",
        ChangeKind::BlockMoved => "block_moved",
        ChangeKind::ListChanged => "list_changed",
        ChangeKind::FrontmatterChanged => "frontmatter_changed",
    }
}

#[inline(always)]
fn push_opt_index(out: &mut String, index: Option<usize>) {
    match index {
        Some(i) => {
            use std::fmt::Write;
            let _ = write!(out, "{i}");
        }
        None => out.push_str("null"),
    }
}

/// Render a [`DiffReport`] as compact JSON: schema `fmd-diff-changes-v1`,
/// snake_case keys, fixed key order, no timestamps. (The aggregate-stats
/// JSON surface on [`DocumentDiff::to_json`] keeps schema `fmd-diff-v1`.)
#[must_use]
pub fn report_json(report: &DiffReport) -> String {
    let mut out = String::with_capacity(64 + report.changes.len() * 96);
    out.push_str("{\"schema\":\"fmd-diff-changes-v1\",\"identical\":");
    out.push_str(if report.identical { "true" } else { "false" });
    out.push_str(",\"changes\":[");
    for (i, change) in report.changes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"kind\":\"");
        out.push_str(change_kind_str(&change.kind));
        out.push('"');
        match &change.kind {
            ChangeKind::HeadingChanged { level } => {
                out.push_str(",\"level\":");
                out.push_str(&level.to_string());
            }
            ChangeKind::CodeBlockChanged { lang } => {
                out.push_str(",\"lang\":");
                match lang {
                    Some(lang) => {
                        out.push('"');
                        out.push_str(&json_escape(lang));
                        out.push('"');
                    }
                    None => out.push_str("null"),
                }
            }
            ChangeKind::LinkTargetChanged { old, new } => {
                out.push_str(",\"old\":\"");
                out.push_str(&json_escape(old));
                out.push_str("\",\"new\":\"");
                out.push_str(&json_escape(new));
                out.push('"');
            }
            _ => {}
        }
        out.push_str(",\"summary\":\"");
        out.push_str(&json_escape(&change.summary));
        out.push_str("\",\"old_index\":");
        push_opt_index(&mut out, change.old_index);
        out.push_str(",\"new_index\":");
        push_opt_index(&mut out, change.new_index);
        out.push('}');
    }
    out.push_str("]}");
    out
}

/// Render a [`DiffReport`] as a plain-text, human-readable change list.
#[must_use]
pub fn report_text(report: &DiffReport) -> String {
    if report.identical {
        return String::from("fmd diff: documents identical\n");
    }
    let n = report.changes.len();
    let mut out = format!("fmd diff: {n} change{}\n", if n == 1 { "" } else { "s" });
    for change in &report.changes {
        let marker = match (change.old_index, change.new_index) {
            (Some(_), Some(_)) => '~',
            (None, Some(_)) => '+',
            (Some(_), None) => '-',
            (None, None) => '?',
        };
        out.push(marker);
        out.push_str(" [");
        if let Some(i) = change.old_index {
            out.push_str(&i.to_string());
        }
        out.push_str("->");
        if let Some(i) = change.new_index {
            out.push_str(&i.to_string());
        }
        out.push_str("] ");
        out.push_str(&change.summary);
        out.push('\n');
    }
    out
}
