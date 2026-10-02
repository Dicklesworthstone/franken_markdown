//! Bounded Knuth-Plass adapter for already-shaped, ragged poster paragraphs.
//!
//! It changes break selection, not text shaping or painting. One box contains
//! all contiguous style/math/image runs of a source word. Only source spaces
//! create glue. Final lines, code spans and nonbreaking spaces retain their
//! existing semantics; the painter never has to honor fictitious shrink.

use super::{Poster, RStyle, Shaper, SvgWarning, TextFlow, Word};
use franken_markdown::layout::{
    FORCED_BREAK_PENALTY, Glue, LayoutUnit, ParagraphItem, ParagraphLayoutScratch, Penalty,
    TextBox, break_paragraph_into,
};
use std::ops::Range;

const MAX_WORDS: usize = 2048;
const MAX_RUNS: usize = 8192;
const MAX_TEXT_BYTES: usize = 256 * 1024;

struct Group {
    runs: Vec<Word>,
    gap: f64,
}

#[derive(Default)]
pub(super) struct Paragraph {
    groups: Vec<Group>,
    runs: usize,
    text_bytes: usize,
    greedy: bool,
}

impl Paragraph {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn push(
        &mut self,
        poster: &Poster,
        flow: &mut TextFlow,
        shaper: &Shaper<'_>,
        word: Vec<Word>,
        gap: f64,
        size: f64,
        width: f64,
    ) {
        let mut group = Group { runs: word, gap };
        if !self.greedy {
            self.runs = self.runs.saturating_add(group.runs.len());
            for run in &group.runs {
                self.text_bytes = self.text_bytes.saturating_add(run.text.len());
            }
            if self.groups.len() == MAX_WORDS
                || self.runs > MAX_RUNS
                || self.text_bytes > MAX_TEXT_BYTES
            {
                // Stream the rest through the existing wrapper. Never retain
                // an unbounded DP graph or silently omit the remaining source.
                self.greedy = true;
                limit_warning(&mut group);
                for prior in self.groups.drain(..) {
                    poster.place_prepared(flow, shaper, prior.runs, prior.gap, size, width);
                }
            }
        }
        if self.greedy {
            poster.place_prepared(flow, shaper, group.runs, group.gap, size, width);
        } else {
            self.groups.push(group);
        }
    }

    pub(super) fn finish(
        &mut self,
        poster: &Poster,
        flow: &mut TextFlow,
        shaper: &Shaper<'_>,
        size: f64,
        width: f64,
    ) {
        let plan = if self.greedy {
            None
        } else {
            plan(&self.groups, width)
        };
        let mut boundaries = plan.as_deref().unwrap_or_default().iter().peekable();
        for (index, group) in self.groups.drain(..).enumerate() {
            if boundaries.peek().is_some_and(|line| line.end == index) {
                flow.new_line();
                boundaries.next();
            }
            poster.place_prepared(flow, shaper, group.runs, group.gap, size, width);
        }
        self.runs = 0;
        self.text_bytes = 0;
        self.greedy = false;
    }
}

/// None means the existing cluster-safe wrapper must handle the paragraph.
/// In particular, an overwide indivisible word must not enter a DP that can
/// only break at source whitespace. Its prepared data is reused by fallback.
fn plan(groups: &[Group], width: f64) -> Option<Vec<Range<usize>>> {
    if groups.is_empty() {
        return Some(Vec::new());
    }
    if groups.len() > MAX_WORDS || !width.is_finite() || width <= 0.0 {
        return None;
    }
    let measure = (width * 1000.0).floor();
    if measure < 1.0 || measure > f64::from(i32::MAX) {
        return None;
    }
    let mut items = Vec::with_capacity(groups.len() * 2);
    for (index, group) in groups.iter().enumerate() {
        if group.runs.is_empty() {
            return None;
        }
        if index != 0 {
            items.push(ParagraphItem::Glue(Glue {
                width: upper_units(group.gap)?,
                stretch: LayoutUnit::ZERO,
                shrink: LayoutUnit::ZERO,
            }));
        }
        let mut natural = 0.0;
        for run in &group.runs {
            if !run.w.is_finite() || run.w < 0.0 {
                return None;
            }
            natural += run.w;
        }
        let units = upper_units(natural)?;
        if units.milli_points() > measure as i32 {
            return None;
        }
        items.push(ParagraphItem::Box(TextBox {
            // Source ownership stays in Group. The shared breaker needs only
            // geometry here, not another copy of each styled source string.
            text: String::new(),
            runs: Default::default(),
            width: units,
            protrusion: Default::default(),
        }));
    }
    items.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    }));
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_justified(false);
    scratch.set_expansion_permilli(0);
    let mut lines = Vec::new();
    break_paragraph_into(
        &items,
        LayoutUnit::from_milli_points(measure as i32),
        &mut scratch,
        &mut lines,
    );
    let mut ranges = Vec::with_capacity(lines.len());
    let mut next = 0;
    for line in lines {
        // Validate a complete, ordered partition before consuming any runs.
        // This also rejects the shared breaker's permitted overfull fallback.
        if line.start != next * 2
            || line.end % 2 != 1
            || line.end >= items.len()
            || line.next != line.end + 1
        {
            return None;
        }
        let end = line.end.div_ceil(2);
        if end <= next || end > groups.len() {
            return None;
        }
        let mut used = 0.0;
        for (offset, group) in groups[next..end].iter().enumerate() {
            if offset != 0 {
                used += group.gap;
            }
            for run in &group.runs {
                used += run.w;
            }
        }
        if !used.is_finite() || used > width {
            return None;
        }
        ranges.push(next..end);
        next = end;
    }
    (next == groups.len()).then_some(ranges)
}

/// Round OUTWARD: fitting must not borrow a fraction of a point that painting
/// will spend. The page measure is rounded inward. Original f64 geometry stays
/// on the prepared runs and is never replaced with these decision units.
fn upper_units(value: f64) -> Option<LayoutUnit> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let units = (value * 1000.0).ceil();
    if !units.is_finite() || units < 0.0 || units > f64::from(i32::MAX) {
        return None;
    }
    Some(LayoutUnit::from_milli_points(units as i32))
}

fn limit_warning(group: &mut Group) {
    let warning = SvgWarning {
        code: "svg_paragraph_limit",
        message: "Paragraph exceeds the optimal-wrap budget (2048 words, 8192 runs or 256 KiB of text); complete text uses cluster-safe greedy wrapping.".to_owned(),
    };
    if let Some(run) = group.runs.iter_mut().find(|run| run.warning.is_none()) {
        run.warning = Some(warning);
    } else {
        // Preserve resource/shape warnings on the real runs as well. A zero-ink
        // run remains inside this word, so it creates no space or word break.
        group.runs.push(Word {
            text: String::new(),
            style: RStyle::BODY,
            w: 0.0,
            gap: 0.0,
            formula: None,
            image: None,
            warning: Some(warning),
            shaped: None,
        });
    }
}

#[cfg(test)]
#[path = "paragraph_plan_tests.rs"]
mod tests;
