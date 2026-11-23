//! Source ownership is independent of the prepared text's glyph boundaries.
//!
//! A link or generated citation can begin inside a ligature/combining cluster.
//! Retain its exact UTF-8 range while shaping the complete same-paint run;
//! wrapping clips these ranges only at actual source cuts.

use super::{RStyle, Word};
use std::ops::Range;

#[derive(Debug, Clone)]
pub(in super::super) struct NavigationSpan {
    pub(super) bytes: Range<usize>,
    pub(super) link: Option<usize>,
    pub(super) note_reference: Option<usize>,
}

impl NavigationSpan {
    fn new(bytes: Range<usize>, style: RStyle) -> Self {
        Self {
            bytes,
            link: style.link,
            note_reference: style.note_reference,
        }
    }

    fn same_owner(&self, other: &Self) -> bool {
        self.link == other.link && self.note_reference == other.note_reference
    }
}

fn text_run(run: &Word) -> bool {
    !run.text.is_empty() && run.formula.is_none() && run.image.is_none() && run.warning.is_none()
}

fn can_join(left: &Word, right: &Word) -> bool {
    text_run(left) && text_run(right) && right.gap == 0.0 && left.style.same_paint(right.style)
}

fn append_span(spans: &mut Vec<NavigationSpan>, span: NavigationSpan) {
    if let Some(last) = spans
        .last_mut()
        .filter(|last| last.bytes.end == span.bytes.start && last.same_owner(&span))
    {
        last.bytes.end = span.bytes.end;
    } else {
        spans.push(span);
    }
}

/// Join only text with identical painting. Math, images, diagnostics and real
/// face/color/decoration changes remain boundaries. A compact range inventory
/// is needed only when an actual navigation boundary is crossed.
pub(super) fn coalesce(runs: &mut Vec<Word>) {
    // Ordinary words have one run. Also preserve the existing allocation when
    // every boundary is a real paint/object/diagnostic change.
    if runs.len() < 2 || !runs.windows(2).any(|pair| can_join(&pair[0], &pair[1])) {
        return;
    }
    let mut joined: Vec<Word> = Vec::with_capacity(runs.len());
    for run in std::mem::take(runs) {
        let Some(last) = joined.last_mut().filter(|last| can_join(last, &run)) else {
            joined.push(run);
            continue;
        };
        let offset = last.text.len();
        if !last.navigation.is_empty() || !run.navigation.is_empty() || last.style != run.style {
            if last.navigation.is_empty() {
                last.navigation
                    .push(NavigationSpan::new(0..offset, last.style));
            }
            if run.navigation.is_empty() {
                append_span(
                    &mut last.navigation,
                    NavigationSpan::new(offset..offset + run.text.len(), run.style),
                );
            } else {
                for mut span in run.navigation {
                    span.bytes.start += offset;
                    span.bytes.end += offset;
                    append_span(&mut last.navigation, span);
                }
            }
        }
        last.text.push_str(&run.text);
        last.shaped = None;
        last.w = 0.0;
    }
    *runs = joined;
}

/// Preserve exact ownership on a physical or discretionary fragment. A slice
/// contained in one owner returns to the allocation-free homogeneous form.
pub(super) fn slice(source: &Word, part: &mut Word, bytes: Range<usize>) {
    if source.navigation.is_empty() {
        return;
    }
    debug_assert!(part.navigation.is_empty());
    debug_assert!(source.text.is_char_boundary(bytes.start));
    debug_assert!(source.text.is_char_boundary(bytes.end));
    debug_assert_eq!(part.text.len(), bytes.len());
    let first = source
        .navigation
        .partition_point(|span| span.bytes.end <= bytes.start);
    for span in source.navigation[first..]
        .iter()
        .take_while(|span| span.bytes.start < bytes.end)
    {
        part.navigation.push(NavigationSpan {
            bytes: span.bytes.start.max(bytes.start) - bytes.start
                ..span.bytes.end.min(bytes.end) - bytes.start,
            link: span.link,
            note_reference: span.note_reference,
        });
    }
    if let Some(first) = part.navigation.first() {
        debug_assert_eq!(first.bytes.start, 0);
        part.style.link = first.link;
        part.style.note_reference = first.note_reference;
    }
    debug_assert_eq!(
        part.navigation.last().map(|span| span.bytes.end),
        Some(part.text.len())
    );
    if part.navigation.len() == 1 {
        part.navigation.clear();
    }
}

/// Coalescing must not let metadata-heavy input bypass the existing paragraph
/// run budget. Every retained ownership range still counts as a source run.
pub(super) fn source_runs(run: &Word) -> usize {
    run.navigation.len().max(1)
}
