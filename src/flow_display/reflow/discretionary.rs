//! Source-preserving projection of authored prose break hints.
//!
//! The host shapes visible text, then its real glyph clusters are mapped back
//! onto the original UTF-8/UTF-16 source. Only a chosen soft-hyphen boundary
//! inserts ink. Measurement may expose an authored boundary inside a ligature;
//! those candidates are always reshaped before publication and are never
//! exported as fabricated glyph clusters.

use super::{FlowInlineRun, FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow, breaks};
use crate::text::{OwnedTextRun, TextCluster};
use std::ops::Range;

pub(super) fn is_hint(ch: char) -> bool {
    matches!(ch, '\u{00ad}' | '\u{200b}')
}

pub(super) fn enabled(role: FlowTextRole, style: FlowInlineStyle) -> bool {
    role != FlowTextRole::Code && !style.code
}

fn prose_at(runs: &[FlowInlineRun], at: usize, role: FlowTextRole) -> bool {
    if role == FlowTextRole::Code {
        return false;
    }
    let index = runs.partition_point(|run| run.range.end <= at);
    runs.get(index)
        .is_none_or(|run| !run.style.code && run.math.is_none())
}

fn before_hints(
    text: &str,
    runs: &[FlowInlineRun],
    at: usize,
    role: FlowTextRole,
) -> Option<char> {
    text.get(..at)?.char_indices().rev().find_map(|(offset, ch)| {
        (!is_hint(ch) || !prose_at(runs, offset, role)).then_some(ch)
    })
}

fn bound_mark(ch: char) -> bool {
    matches!(ch as u32,
        0x0300..=0x036f | 0x0483..=0x0489 | 0x0591..=0x05bd |
        0x05bf | 0x05c1..=0x05c2 | 0x05c4..=0x05c5 | 0x05c7 |
        0x0610..=0x061a | 0x064b..=0x065f | 0x0670 | 0x06d6..=0x06dc |
        0x06df..=0x06e4 | 0x06e7..=0x06e8 | 0x06ea..=0x06ed | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff |
        0x20d0..=0x20ff | 0x3099..=0x309a | 0xfe00..=0xfe0f |
        0xfe20..=0xfe2f | 0x1f3fb..=0x1f3ff | 0xe0100..=0xe01ef
    ) || matches!(ch, '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}')
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Hint {
    pub(super) end: usize,
    /// Source byte of the first soft hyphen in a coalesced hint sequence.
    pub(super) hyphen: Option<usize>,
}

/// One complete, meaningful hint sequence immediately before the boundary.
pub(super) fn at_boundary(
    text: &str,
    runs: &[FlowInlineRun],
    at: usize,
    role: FlowTextRole,
) -> Option<Hint> {
    let right = text.get(at..)?.chars().next()?;
    if right.is_whitespace()
        || bound_mark(right)
        || (is_hint(right) && prose_at(runs, at, role))
    {
        return None;
    }
    let mut start = at;
    let mut hyphen = None;
    for (offset, ch) in text.get(..at)?.char_indices().rev() {
        if !is_hint(ch) || !prose_at(runs, offset, role) {
            break;
        }
        start = offset;
        if ch == '\u{00ad}' {
            hyphen = Some(offset);
        }
    }
    if start == at {
        return None;
    }
    let left = text[..start].chars().next_back()?;
    if left.is_whitespace()
        || matches!(left, '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}')
        || crate::layout::cjk_break_prohibited(left, right)
    {
        return None;
    }
    Some(Hint { end: at, hyphen })
}

/// Bind hidden prefixes/suffixes to real content, coalesce a hint sequence,
/// and preserve a base with its marks even across an inline style boundary.
pub(super) fn prohibited_boundary(
    text: &str,
    runs: &[FlowInlineRun],
    at: usize,
    role: FlowTextRole,
) -> bool {
    if role == FlowTextRole::Code {
        return false;
    }
    let Some(right) = text.get(at..).and_then(|tail| tail.chars().next()) else {
        return false;
    };
    if is_hint(right) && prose_at(runs, at, role) {
        return true;
    }
    let Some((left_at, left)) = text.get(..at).and_then(|head| head.char_indices().next_back())
    else {
        return false;
    };
    if is_hint(left) && prose_at(runs, left_at, role) {
        // Hidden controls after whitespace cannot erase its existing break.
        // They still cannot detach a following mark or CJK nonstarter.
        let before = before_hints(text, runs, at, role);
        if before.is_some_and(|ch| {
            ch.is_whitespace()
                && !bound_mark(right)
                && !crate::layout::cjk_break_prohibited(ch, right)
        }) {
            return false;
        }
        return at_boundary(text, runs, at, role).is_none();
    }
    breaks::prohibited_boundary(text, at)
}

pub(super) fn word_boundary(
    text: &str,
    runs: &[FlowInlineRun],
    bytes: Range<usize>,
    role: FlowTextRole,
) -> bool {
    if at_boundary(text, runs, bytes.end, role).is_some() {
        return true;
    }
    let mut whitespace = false;
    let separator = text[bytes.clone()].char_indices().all(|(offset, ch)| {
        if matches!(ch, ' ' | '\t') {
            whitespace = true;
            true
        } else {
            is_hint(ch) && prose_at(runs, bytes.start + offset, role)
        }
    });
    (separator
        && (whitespace
            || before_hints(text, runs, bytes.end, role)
                .is_some_and(|ch| matches!(ch, ' ' | '\t'))))
        || breaks::cjk_boundary(text, bytes.end)
}

/// Source candidates in a single bounded measurement window.
fn hints(
    text: &str,
    runs: &[FlowInlineRun],
    range: Range<usize>,
    role: FlowTextRole,
) -> Vec<Hint> {
    let mut hints = Vec::new();
    if role == FlowTextRole::Code {
        return hints;
    }
    let mut chars = text[range.clone()].char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if !is_hint(ch) || !prose_at(runs, range.start + offset, role) {
            continue;
        }
        let mut end = range.start + offset + ch.len_utf8();
        while let Some(&(next, ch)) = chars.peek() {
            if !is_hint(ch) || !prose_at(runs, range.start + next, role) {
                break;
            }
            end = range.start + next + ch.len_utf8();
            chars.next();
        }
        if let Some(hint) = at_boundary(text, runs, end, role) {
            hints.push(hint);
        }
    }
    hints
}

/// A complete host cluster containing a hint can be committed to intrinsic
/// measurement once another cluster remains as right-hand shaping context.
pub(super) fn contains_boundary(
    text: &str,
    runs: &[FlowInlineRun],
    bytes: Range<usize>,
    role: FlowTextRole,
) -> bool {
    !hints(text, runs, bytes, role).is_empty()
}

pub(super) struct MeasuredCluster {
    pub(super) bytes: Range<usize>,
    pub(super) advance: f32,
    /// A selected boundary inside a host cluster requires fresh tail metrics.
    pub(super) continuation: Option<usize>,
}

/// Keep the measured no-break width, while admitting source-owned candidates
/// inside a ligature. An unfinished prefix contributes no speculative width;
/// the complete original cluster carries its actual measured advance. Every
/// selected prefix is measured again with its optional hyphen before it can
/// become a line, so this never invents visible metrics or splits a glyph.
pub(super) fn measured_clusters(
    text: &str,
    runs: &[FlowInlineRun],
    offset: usize,
    run: &OwnedTextRun,
    role: FlowTextRole,
    split_hints: bool,
) -> Vec<MeasuredCluster> {
    let hints: Vec<_> = if split_hints {
        hints(text, runs, offset..offset + run.logical_text.len(), role)
    } else {
        Vec::new()
    }
        .into_iter()
        .filter(|hint| {
            // ASCII letter/digit ligatures may be reshaped at an authored
            // boundary. Unknown-script host clusters remain indivisible;
            // a source hint is not evidence that their grapheme can be split.
            let right = text[hint.end..].chars().next();
            let left = text[..hint.end].chars().rev().find(|ch| !is_hint(*ch));
            left.is_some_and(|ch| ch.is_ascii_alphanumeric())
                && right.is_some_and(|ch| ch.is_ascii_alphanumeric())
        })
        .collect();
    let mut logical: Vec<_> = run.clusters.iter().collect();
    logical.sort_by_key(|cluster| cluster.byte_range.start);
    let mut out = Vec::with_capacity(logical.len() + hints.len());
    let mut next_hint = 0;
    for cluster in logical {
        let original_start = offset + cluster.byte_range.start;
        let mut start = original_start;
        let end = offset + cluster.byte_range.end;
        while next_hint < hints.len() && hints[next_hint].end <= start {
            next_hint += 1;
        }
        while next_hint < hints.len() && hints[next_hint].end < end {
            let boundary = hints[next_hint].end;
            out.push(MeasuredCluster {
                bytes: start..boundary,
                advance: 0.0,
                continuation: (start > original_start).then_some(end),
            });
            start = boundary;
            next_hint += 1;
        }
        out.push(MeasuredCluster {
            bytes: start..end,
            advance: cluster.advance(),
            continuation: (start > original_start).then_some(end),
        });
    }
    out
}

/// A work cut still needs right-hand context and is never a published break.
/// Hints add bounded cuts to long words without making code into prose.
pub(super) fn measurement_boundary(
    text: &str,
    runs: &[FlowInlineRun],
    role: FlowTextRole,
    start: usize,
    end: usize,
    limit: usize,
) -> Option<usize> {
    let cap = start.saturating_add(limit).min(end);
    let mut capped = cap;
    while !text.is_char_boundary(capped) {
        capped -= 1;
    }
    let has_hints = text.get(start..capped)?.chars().any(is_hint)
        || text.get(capped..end)?.chars().next().is_some_and(is_hint);
    if role == FlowTextRole::Code || !has_hints {
        return breaks::measurement_boundary(text, start, end, limit);
    }
    let mut latest = None;
    let mut previous = None;
    for (offset, ch) in text.get(start..end)?.char_indices() {
        let at = start + offset;
        if at > cap {
            break;
        }
        if at > start
            && ((at < cap && matches!(ch, ' ' | '\t'))
                || (previous.is_some_and(|left| {
                    left != '\u{200d}' && crate::layout::cjk_break_allowed(left, ch)
                }) && !prohibited_boundary(text, runs, at, role)))
        {
            latest = Some(at);
        }
        previous = Some(ch);
    }
    for hint in hints(text, runs, start..capped, role) {
        if hint.end > start {
            latest = Some(latest.map_or(hint.end, |old| old.max(hint.end)));
        }
    }
    latest
}

/// Minimum width of a stream of measured atomic units with optional break
/// costs. Taking every currently fitting cut leaves the shortest possible
/// unresolved suffix. When that suffix exceeds the current minimum, either
/// its no-break width or an earlier complete cut must become admissible.
/// Already settled prefixes never need to be retained or measured again.
pub(super) struct Intrinsic {
    minimum: f64,
    unbroken: f64,
    pending: std::collections::VecDeque<(f64, Option<f64>)>,
    maximum_pending: usize,
}

impl Intrinsic {
    pub(super) fn new(maximum_pending: usize) -> Self {
        Self {
            minimum: 1.0,
            unbroken: 0.0,
            pending: std::collections::VecDeque::new(),
            maximum_pending,
        }
    }

    pub(super) fn minimum(&self) -> f64 {
        self.minimum
    }

    pub(super) fn finish_line(&mut self) {
        // A physical line end needs no discretionary hyphen. The unresolved
        // suffix is already no wider than the established minimum.
        self.pending.clear();
        self.unbroken = 0.0;
    }

    pub(super) fn push(
        &mut self,
        advance: f64,
        cost: Option<f64>,
        work: &mut usize,
        limit: usize,
    ) -> Result<(), FlowLayoutError> {
        fn charge(work: &mut usize, limit: usize) -> Result<(), FlowLayoutError> {
            *work = work.checked_add(1)
                .filter(|count| *count <= limit)
                .ok_or(FlowLayoutError::BudgetExceeded("total shaping bytes"))?;
            Ok(())
        }

        charge(work, limit)?;
        if self.pending.len() >= self.maximum_pending {
            return Err(FlowLayoutError::BudgetExceeded(
                "table shaping window cannot settle a unit",
            ));
        }
        self.pending.push_back((advance, cost));
        self.unbroken += advance;
        if cost.is_some_and(|cost| self.unbroken + cost <= self.minimum) {
            self.finish_line();
            return Ok(());
        }
        while self.unbroken > self.minimum {
            let mut prefix = 0.0;
            let mut next = self.unbroken;
            for (advance, cost) in &self.pending {
                charge(work, limit)?;
                prefix += advance;
                if let Some(cost) = cost {
                    next = next.min(prefix + cost);
                }
            }
            self.minimum = self.minimum.max(next);
            let mut suffix = 0.0;
            let mut consumed = 0;
            for (index, (advance, cost)) in self.pending.iter().enumerate() {
                charge(work, limit)?;
                suffix += advance;
                if cost.is_some_and(|cost| suffix + cost <= self.minimum) {
                    consumed = index + 1;
                    suffix = 0.0;
                }
            }
            drop(self.pending.drain(..consumed));
            self.unbroken = suffix;
        }
        Ok(())
    }
}

impl<F: ?Sized> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    /// Extra measured ink when a table's current atomic unit ends at SHY.
    /// An unused hint contributes no preferred width. Same-face prefixes are
    /// reshaped with the actual chosen hyphen; an isolated styled hint uses
    /// its own face, as the final fragment renderer does.
    pub(super) fn table_hint_cost(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        unit: Range<usize>,
        advance: f64,
        role: FlowTextRole,
    ) -> Result<Option<f64>, FlowLayoutError> {
        if prohibited_boundary(text, runs, unit.end, role) {
            return Ok(None);
        }
        let Some(marker) = at_boundary(text, runs, unit.end, role)
            .and_then(|hint| hint.hyphen)
        else {
            return Ok(Some(0.0));
        };
        let index = runs.partition_point(|spec| spec.range.end <= marker);
        let spec = runs.get(index);
        let style = spec.map_or(FlowInlineStyle::default(), |spec| spec.style);
        let size = self.inline_size(self.options.body_size, style);
        let same_style = spec.is_none_or(|spec| {
            spec.range.start <= unit.start && spec.range.end >= unit.end
        });
        let extra = if same_style && unit.len() <= self.options.max_shape_bytes {
            let shaped = self.shaped_discretionary(
                &text[unit.clone()], size, role, style, Some(marker - unit.start),
            )?;
            (f64::from(shaped.total_advance) - advance).max(0.0)
        } else {
            // A preceding formula or another face cannot join to this glyph.
            // Its atomic width is already included in the unit's advance.
            f64::from(self.shaped_with_style("-", size, role, style)?.total_advance)
        };
        Ok(Some(extra))
    }
}

/// Retry earlier preferred opportunities first. If none can be shaped to fit,
/// retain the original complete-cluster emergency candidate; a discretionary
/// prefix plus hyphen can be wider than the unused whole ligature.
pub(super) fn next_line_end(
    minimum: usize,
    end: usize,
    preferred: &mut bool,
    emergency: &mut Option<usize>,
    word_break: impl Fn(usize) -> bool,
) -> Option<usize> {
    if *preferred {
        if let Some(previous) = (minimum..end).rev().find(|candidate| word_break(*candidate)) {
            return Some(previous);
        }
        *preferred = false;
        if let Some(original) = emergency.take() {
            return Some(original);
        }
    }
    if end > minimum {
        Some(end - 1)
    } else {
        None
    }
}

pub(super) struct Projection {
    pub(super) visible: String,
    /// Visible scalar boundary -> original byte and UTF-16 boundary.
    boundaries: Vec<(usize, usize, usize)>,
}

impl Projection {
    pub(super) fn new(source: &str, selected: Option<usize>) -> Self {
        let mut visible = String::with_capacity(source.len());
        let mut boundaries = vec![(0, 0, 0)];
        let mut utf16 = 0;
        for (at, ch) in source.char_indices() {
            utf16 += ch.len_utf16();
            if is_hint(ch) && selected != Some(at) {
                // Leading hints belong to the first real cluster. Elsewhere
                // an unused hint belongs to the preceding real cluster.
                if !visible.is_empty()
                    && let Some(last) = boundaries.last_mut()
                {
                    last.1 = at + ch.len_utf8();
                    last.2 = utf16;
                }
                continue;
            }
            visible.push(if selected == Some(at) { '-' } else { ch });
            boundaries.push((visible.len(), at + ch.len_utf8(), utf16));
        }
        Self { visible, boundaries }
    }

    pub(super) fn restore(
        self,
        source: &str,
        mut run: OwnedTextRun,
    ) -> Result<OwnedTextRun, FlowLayoutError> {
        if self.visible.is_empty() && !source.is_empty() {
            // An empty host run supplies truthful font/direction/vertical
            // context. Source-only clusters have no glyphs and no ink.
            run.clusters.push(TextCluster {
                cluster_index: 0,
                byte_range: 0..source.len(),
                utf16_range: 0..source.encode_utf16().count(),
                glyph_range: 0..0,
                x_start: 0.0,
                x_end: 0.0,
                font_id: run.context.font_id,
            });
        } else {
            for cluster in &mut run.clusters {
                let start = self.boundaries.binary_search_by_key(&cluster.byte_range.start, |b| b.0)
                    .map_err(|_| FlowLayoutError::InvalidShapedRun)?;
                let end = self.boundaries.binary_search_by_key(&cluster.byte_range.end, |b| b.0)
                    .map_err(|_| FlowLayoutError::InvalidShapedRun)?;
                cluster.byte_range = self.boundaries[start].1..self.boundaries[end].1;
                cluster.utf16_range = self.boundaries[start].2..self.boundaries[end].2;
            }
        }
        run.logical_text = source.to_owned();
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use super::super::{FlowLayoutOptions, Reflow, tests::measured};
    use crate::display::{DisplayItem, DisplayTextRun};
    use crate::flow_display::ResumableFlowDisplay;
    use crate::text::{CaretAffinity, Direction};

    fn engine(source: &str) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, 4);
        engine.process_all().unwrap();
        engine
    }

    #[test]
    fn source_projection_keeps_ligatures_supplementary_offsets_and_hidden_edges() {
        let source = "\u{00ad}f\u{00ad}i\u{200b}😀\u{00ad}";
        let projection = Projection::new(source, None);
        assert_eq!(projection.visible, "fi😀");
        let original = measured(&projection.visible, 14.0, FlowTextRole::Body).unwrap();
        let run = projection.restore(source, original.clone()).unwrap();
        assert_eq!(run.logical_text, source);
        assert_eq!(run.glyphs, original.glyphs);
        assert_eq!(run.total_advance, original.total_advance);
        assert_eq!(run.clusters.len(), 2);
        assert_eq!(run.clusters[0].byte_range, 0..9);
        assert_eq!(run.clusters[0].utf16_range, 0..5);
        assert_eq!(run.clusters[1].byte_range, 9..15);
        assert_eq!(run.clusters[1].utf16_range, 5..8);
        for offset in [0, 2, 3, 5, 6, 9, 13, 15] {
            for affinity in [CaretAffinity::Leading, CaretAffinity::Trailing] {
                let caret = run.caret_at_byte(offset, affinity).unwrap();
                assert_eq!(
                    source[..caret.byte_offset].encode_utf16().count(),
                    caret.utf16_offset,
                );
                assert_eq!(
                    run.caret_at_utf16(caret.utf16_offset, caret.affinity),
                    Some(caret),
                );
            }
        }
    }

    #[test]
    fn selected_hyphen_belongs_to_its_original_control_not_a_synthetic_source_byte() {
        let source = "a\u{200b}\u{00ad}";
        let projection = Projection::new(source, Some(4));
        assert_eq!(projection.visible, "a-");
        let shaped = measured(&projection.visible, 14.0, FlowTextRole::Body).unwrap();
        let run = projection.restore(source, shaped).unwrap();
        assert_eq!(run.logical_text, source);
        assert_eq!(run.clusters[0].byte_range, 0..4);
        assert_eq!(run.clusters[1].byte_range, 4..6);
        assert_eq!(run.clusters[1].utf16_range, 2..3);
        assert_eq!(run.total_advance, 20.0);
    }

    #[test]
    fn empty_projection_has_real_source_carets_and_no_glyph_or_selection_ink() {
        let source = "\u{00ad}\u{200b}";
        let projection = Projection::new(source, None);
        let shaped = measured("", 14.0, FlowTextRole::Body).unwrap();
        let run = projection.restore(source, shaped).unwrap();
        assert!(run.glyphs.is_empty());
        assert_eq!(run.clusters[0].glyph_range, 0..0);
        assert_eq!(run.total_advance, 0.0);
        assert_eq!(
            run.caret_at_utf16(2, CaretAffinity::Trailing).unwrap().byte_offset,
            source.len(),
        );
        assert!(run.selection_rects(0..source.len(), 0.0, 20.0).is_empty());
    }

    #[test]
    fn custom_complex_script_graphemes_are_not_split_as_ascii_ligatures() {
        // Musical combining stem is intentionally outside the supported
        // Latin mark profile. The host owns its complete atomic cluster.
        let source = "x\u{00ad}\u{1d165}";
        let projection = Projection::new(source, None);
        let mut shaped = measured(&projection.visible, 14.0, FlowTextRole::Body).unwrap();
        let bytes = shaped.logical_text.len();
        let units = shaped.logical_text.encode_utf16().count();
        let glyphs = shaped.glyphs.len();
        shaped.clusters.truncate(1);
        let cluster = &mut shaped.clusters[0];
        cluster.byte_range = 0..bytes;
        cluster.utf16_range = 0..units;
        cluster.glyph_range = 0..glyphs;
        cluster.x_end = shaped.total_advance;
        for glyph in &mut shaped.glyphs {
            glyph.cluster_index = 0;
        }
        let run = projection.restore(source, shaped).unwrap();
        let parts = measured_clusters(source, &[], 0, &run, FlowTextRole::Body, true);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].bytes, 0..source.len());
        assert_eq!(parts[0].advance, run.total_advance);
    }

    #[test]
    fn hints_preserve_whitespace_opportunities_and_respect_bound_marks_and_joiners() {
        let role = FlowTextRole::Body;
        for hint in ['\u{00ad}', '\u{200b}'] {
            let source = format!("foo {hint}bar");
            let boundary = source.find('b').unwrap();
            assert!(at_boundary(&source, &[], boundary, role).is_none());
            assert!(word_boundary(&source, &[], 3..boundary, role));
            assert!(!prohibited_boundary(&source, &[], boundary, role));
            for mark in ['\u{0301}', '\u{0483}', '\u{05b0}', '\u{064e}', '\u{fe0f}'] {
                let source = format!("a{hint}{mark}b");
                assert!(at_boundary(&source, &[], 1 + hint.len_utf8(), role).is_none());
            }
            for joiner in ['\u{200c}', '\u{200d}', '\u{2060}', '\u{feff}'] {
                let before = format!("{joiner}{hint}a");
                let after = format!("a{hint}{joiner}");
                assert!(at_boundary(
                    &before, &[], joiner.len_utf8() + hint.len_utf8(), role,
                ).is_none());
                assert!(at_boundary(&after, &[], 1 + hint.len_utf8(), role).is_none());
            }
        }
    }

    #[test]
    fn source_bytes_are_charged_even_when_the_host_would_shape_nothing() {
        let mut calls = 0;
        let mut shape = |text: &str, size, role, _style| {
            calls += 1;
            measured(text, size, role)
        };
        let mut reflow = Reflow::new(
            FlowLayoutOptions {
                max_total_shape_bytes: 1,
                ..FlowLayoutOptions::default()
            },
            &mut shape,
        ).unwrap();
        assert_eq!(
            reflow.shaped("\u{00ad}", 14.0, FlowTextRole::Body),
            Err(FlowLayoutError::BudgetExceeded("total shaping bytes")),
        );
        assert_eq!(calls, 0);
    }

    #[test]
    fn a_hyphen_that_does_not_fit_uses_an_earlier_authored_opportunity() {
        let source = "ab\u{00ad}cd\u{00ad}ef";
        let list = engine(source).to_shaped_display_list(
            FlowLayoutOptions {
                viewport_width: 40.0,
                ..FlowLayoutOptions::default()
            },
            measured,
        ).unwrap();
        let text: Vec<_> = list.items().iter().filter_map(|item| match item {
            DisplayItem::Text(text) => Some(text),
            _ => None,
        }).collect();
        assert_eq!(text[0].text, "ab\u{00ad}");
        assert_eq!(text[0].bounds.width, 30.0);
        assert_eq!(text.iter().map(|part| part.text.as_str()).collect::<String>(), source);
        assert!(text.iter().all(|part| part.bounds.width <= 40.0));
    }

    #[test]
    fn plain_adapter_preserves_inline_code_hints_without_changing_its_size_or_ink() {
        let markdown = "co\u{00ad}de \u{0060}raw\u{200b}code\u{0060}";
        let mut seen_literal = false;
        let list = engine(markdown).to_shaped_display_list(
            FlowLayoutOptions::default(),
            |text, size, role| {
                if text.contains("raw\u{200b}code") {
                    seen_literal = true;
                } else {
                    assert!(!text.chars().any(is_hint));
                }
                measured(text, size, role)
            },
        ).unwrap();
        assert!(seen_literal);
        let literal = list.items().iter().find_map(|item| match item {
            DisplayItem::Text(text) if text.text == "raw\u{200b}code" => Some(text),
            _ => None,
        }).unwrap();
        assert_eq!(literal.font_size, 14.0);
        assert_eq!(literal.color_role, "text");
    }

    #[test]
    fn source_only_style_fragments_do_not_invent_a_direction_conflict() {
        let list = engine("a**\u{00ad}**b").to_styled_display_list(
            FlowLayoutOptions::default(),
            |text, size, role, _style| {
                let mut run = measured(text, size, role)?;
                if !text.is_empty() {
                    // One visible scalar per fragment makes this a truthful
                    // synthetic RTL run without reversing any glyph arrays.
                    run.context.direction = Direction::RightToLeft;
                }
                Ok(run)
            },
        ).unwrap();
        let text: Vec<&DisplayTextRun> = list.items().iter().filter_map(|item| match item {
            DisplayItem::Text(text) => Some(text),
            _ => None,
        }).collect();
        assert_eq!(text.iter().map(|part| part.text.as_str()).collect::<String>(), "a\u{00ad}b");
        assert_eq!(text.iter().filter(|part| part.bounds.width == 0.0).count(), 1);
    }
    #[test]
    fn a_fitting_whole_ligature_survives_an_unfittable_authored_prefix() {
        let source = "f\u{00ad}iX";
        for styled in [false, true] {
            let engine = engine(source);
            let options = FlowLayoutOptions {
                viewport_width: 10.0,
                ..FlowLayoutOptions::default()
            };
            let list = if styled {
                engine.to_styled_display_list(options, |text, size, role, _style| {
                    measured(text, size, role)
                })
            } else {
                engine.to_shaped_display_list(options, measured)
            }.unwrap();
            let first = list.items().iter().find_map(|item| match item {
                DisplayItem::Text(text) => Some(text),
                _ => None,
            }).unwrap();
            assert_eq!(first.text, "f\u{00ad}i");
            assert_eq!(first.bounds.width, 10.0);
            assert_eq!(first.font_run.as_ref().unwrap().glyphs.len(), 1);
        }
    }

    #[test]
    fn intrinsic_minimum_matches_exhaustive_legal_partitions() {
        let costs = [None, Some(0.0), Some(1.0), Some(2.0), Some(5.0), Some(8.0)];
        for length in 1..=5_usize {
            for mut seed in 0..6_usize.pow(length as u32) {
                let mut units = Vec::new();
                for index in 0..length {
                    units.push((1.0 + (index % 3) as f64, costs[seed % 6]));
                    seed /= 6;
                }
                let mut best = f64::INFINITY;
                for mask in 0..(1_usize << length) {
                    let mut maximum = 1.0_f64;
                    let mut width = 0.0;
                    let mut valid = true;
                    for (index, (advance, cost)) in units.iter().enumerate() {
                        width += advance;
                        if mask & (1 << index) != 0 {
                            let Some(cost) = cost else {
                                valid = false;
                                break;
                            };
                            maximum = maximum.max(width + cost);
                            width = 0.0;
                        }
                    }
                    if valid {
                        best = best.min(maximum.max(width));
                    }
                }
                let mut intrinsic = Intrinsic::new(64);
                let mut work = 0;
                for (advance, cost) in &units {
                    intrinsic.push(*advance, *cost, &mut work, 10_000).unwrap();
                }
                intrinsic.finish_line();
                assert_eq!(intrinsic.minimum(), best, "{units:?}");
            }
        }
    }

    #[test]
    fn table_minima_include_real_hyphen_ink_and_keep_no_break_alternatives() {
        for hyphen in [2.0, 25.0] {
            let mut shape = |text: &str, size, role, _style| {
                let mut run = measured(text, size, role)?;
                let mut x = 0.0;
                for cluster in &mut run.clusters {
                    let width = if &run.logical_text[cluster.byte_range.clone()] == "-" {
                        hyphen
                    } else {
                        10.0
                    };
                    cluster.x_start = x;
                    cluster.x_end = x + width;
                    run.glyphs[cluster.glyph_range.start].x_advance = width;
                    x += width;
                }
                run.total_advance = x;
                Ok(run)
            };
            let mut reflow = Reflow::new(FlowLayoutOptions::default(), &mut shape).unwrap();
            let cases = [
                ("a\u{00ad}b", if hyphen < 10.0 { 12.0 } else { 20.0 }, 20.0),
                ("a\u{00ad}b\u{00ad}c", if hyphen < 10.0 { 12.0 } else { 30.0 }, 30.0),
                ("a\u{00ad}bcd", if hyphen < 10.0 { 12.0 } else { 20.0 }, 40.0),
                ("f\u{00ad}i", 10.0, 10.0),
            ];
            for (source, minimum, preferred) in cases {
                assert_eq!(
                    reflow.cell_measures(source, &[], FlowTextRole::TableCell).unwrap(),
                    (minimum, preferred),
                    "{source:?}, hyphen={hyphen}",
                );
            }
        }
    }

    #[test]
    fn unresolved_intrinsic_units_and_candidate_scans_obey_shared_limits() {
        let mut intrinsic = Intrinsic::new(2);
        let mut work = 0;
        intrinsic.push(0.0, None, &mut work, 10).unwrap();
        intrinsic.push(0.0, None, &mut work, 10).unwrap();
        assert_eq!(
            intrinsic.push(0.0, None, &mut work, 10),
            Err(FlowLayoutError::BudgetExceeded("table shaping window cannot settle a unit")),
        );
        let mut intrinsic = Intrinsic::new(8);
        let mut work = 0;
        assert_eq!(
            intrinsic.push(10.0, Some(10.0), &mut work, 1),
            Err(FlowLayoutError::BudgetExceeded("total shaping bytes")),
        );
    }

}
