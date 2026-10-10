//! Same-measure paragraph alternatives for the opt-in PDF page planner.
//!
//! Shaping happens while the paragraph's tokens and fonts are available; the
//! choice happens after block layout has finalized gaps and chapter boundaries.
//! Only ordinary paragraphs with at least four lines in every offered shape
//! participate. That keeps the existing short-caption, list, and heading
//! boundary costs independent of choices in preceding blocks.

use super::*;
use crate::pagination::height::{
    BlockCandidates, BlockPolicy, BlockVariant, HeightPaginationOptions, plan_blocks,
};

/// The counted KP search already has a state-cell bound. Bound its input here
/// too, because its predecessor scan is quadratic in the number of breakpoints.
const MAX_PARAGRAPH_ITEMS: usize = 1024;
/// Retaining alternate positioned segments is optional quality work. Stop
/// collecting before large documents can multiply their whole layout storage.
const MAX_ALTERNATIVE_LINES: usize = 16_384;
/// A conservative bound on the counted search's breakpoint × predecessor ×
/// line-count work. Charge every attempt, including ones with no alternatives
/// and shapes later rejected by block metadata, so repeated paragraphs cannot
/// turn a per-paragraph limit into unbounded optional document work.
const MAX_MEASUREMENT_WORK: usize = 64 * 1024 * 1024;

pub(super) struct Alternative {
    pub lines: Vec<Line>,
    /// Difference from this paragraph's production baseline, in the original
    /// KP cost units. Subtracting that constant preserves the joint objective.
    pub extra_demerits: i64,
}

struct Paragraph {
    baseline_count: usize,
    alternatives: Vec<Alternative>,
}

#[derive(Default)]
pub(super) struct Candidates {
    enabled: bool,
    paragraphs: BTreeMap<u32, Paragraph>,
    retained_lines: usize,
    measurement_work: usize,
}

impl Candidates {
    pub fn new(opts: &PdfOptions, blocks: &[Block]) -> Self {
        Self {
            enabled: opts.optimal_pagination
                && !opts.gradual_demerits
                && !opts.river_penalty
                && !opts.pareto_line_breaking
                // Notes retain their measured baseline: a reflowed citation
                // would need a correspondingly re-anchored foot reservation.
                && collect_pdf_footnote_defs(blocks).is_empty(),
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn clear(&mut self) {
        self.paragraphs.clear();
        self.retained_lines = 0;
        self.measurement_work = 0;
    }

    pub fn begin_measure(&mut self, items: usize, baseline_count: usize) -> bool {
        let work = items
            .saturating_mul(items)
            .saturating_mul(baseline_count.saturating_add(2));
        let admitted = self.enabled
            && items <= MAX_PARAGRAPH_ITEMS
            && baseline_count >= 4
            && self
                .retained_lines
                .saturating_add(baseline_count.saturating_mul(2))
                <= MAX_ALTERNATIVE_LINES
            && self.measurement_work.saturating_add(work) <= MAX_MEASUREMENT_WORK;
        if admitted {
            self.measurement_work += work;
        }
        admitted
    }

    pub fn insert(&mut self, group: u32, baseline_count: usize, alternatives: Vec<Alternative>) {
        if alternatives.is_empty() {
            return;
        }
        let lines = alternatives
            .iter()
            .map(|shape| shape.lines.len())
            .sum::<usize>();
        if self.retained_lines.saturating_add(lines) > MAX_ALTERNATIVE_LINES {
            return;
        }
        self.retained_lines += lines;
        self.paragraphs.insert(
            group,
            Paragraph {
                baseline_count,
                alternatives,
            },
        );
    }
}

/// Common conversion used by both the original single-shape bridge and the
/// joint planner. In particular, final gaps, forbidden hyphen splits, table
/// prefixes and note reservations use exactly the same admission rules.
pub(super) fn block_variant(
    lines: &[Line],
    start: usize,
    end: usize,
    demerits: i64,
    reserve: impl Fn(usize) -> f32,
) -> BlockVariant {
    let fragment_heights = lines[start..end]
        .iter()
        .map(|line| lu_from_points_f32((line_leading(line) + line.gap_after).max(0.001)))
        .collect();
    let reservations: Vec<f32> = (start..end).map(reserve).collect();
    let fragment_reservations = if reservations.iter().all(|&points| points == 0.0) {
        Vec::new()
    } else {
        reservations.into_iter().map(lu_from_points_f32).collect()
    };
    let split_costs = ((start + 1)..end)
        .map(|candidate| {
            if pdf_split_is_hard_forbidden(lines, candidate) {
                None
            } else {
                Some(break_penalty(lines, candidate).round() as i64)
            }
        })
        .collect();
    BlockVariant {
        demerits,
        fragment_heights,
        continuation_prefix: pdf_table_continuation_prefix(lines, start, end),
        fragment_reservations,
        split_costs,
    }
}

pub(super) fn block_policy(lines: &[Line], start: usize) -> BlockPolicy {
    let first = &lines[start];
    let mut policy = if first.flow.kind == FlowKind::Paragraph {
        BlockPolicy::paragraph()
    } else {
        BlockPolicy::default()
    };
    policy.break_before = first.page_break_before;
    policy.break_before_cost = if start == 0 || first.page_break_before {
        0
    } else {
        break_penalty(lines, start).round() as i64
    };
    if first.flow.kind == FlowKind::Heading {
        policy.keep_together = true;
        policy.keep_with_next = true;
    }
    policy
}

pub(super) fn options(page: PageGeom) -> HeightPaginationOptions {
    HeightPaginationOptions {
        page_capacity: lu_from_points_f32((page.top_y() - page.bottom).max(MIN_CONTENT_DIM)),
        page_cost: PAGE_COUNT_DEMERITS.round() as u64,
        unused_height_cost: 10_000,
        penalize_last_page: false,
        ..HeightPaginationOptions::default()
    }
}

fn eligible(lines: &[Line], paragraph: &Paragraph) -> bool {
    lines.len() == paragraph.baseline_count
        && lines.len() >= 4
        && lines.iter().enumerate().all(|(index, line)| {
            line.flow.kind == FlowKind::Paragraph
                && line.flow.index == index
                && line.flow.count == lines.len()
                && line.flow.note == 0
                && !line.flow.list_start
                && !line.rule
                && line.bg == 0
                && !line.shade
                && line.quote_bars.is_empty()
                && line.list_path.is_empty()
                && line.table_cols.is_empty()
                && line.image.is_none()
                && (index == 0 || !line.page_break_before)
                && line.segs.iter().all(|seg| !is_inline_object(seg.slot))
        })
}

/// Transfer metadata stamped by the containing block after paragraph shaping.
/// Segment text, links, style, font metrics and justification remain those of
/// the selected measured shape; they are never rebuilt from flattened text.
fn finalized_shape(baseline: &[Line], alternative: &Alternative) -> Vec<Line> {
    let mut result = alternative.lines.clone();
    let count = result.len();
    let first = &baseline[0];
    let gap_after = baseline[baseline.len() - 1].gap_after;
    for (index, line) in result.iter_mut().enumerate() {
        line.flow = FlowMark {
            index,
            count,
            ..first.flow
        };
        line.page_break_before = index == 0 && first.page_break_before;
        line.gap_after = if index + 1 == count { gap_after } else { 0.0 };
    }
    result
}

struct Choices {
    start: usize,
    end: usize,
    alternatives: Vec<Vec<Line>>,
}

/// Internal paragraph split costs inspect preceding flow kinds, not the image
/// bytes or text of neighboring blocks. Retain structural context without
/// copying a potentially large figure payload for every candidate paragraph.
fn cost_context(line: &Line) -> Line {
    Line {
        size: line.size,
        gap_after: line.gap_after,
        rule: line.rule,
        rule_x: line.rule_x,
        quote_bars: Vec::new(),
        bg: line.bg,
        shade: line.shade,
        flow: line.flow,
        page_break_before: line.page_break_before,
        list_path: Vec::new(),
        table_cols: Vec::new(),
        segs: Vec::new(),
        image: None,
    }
}

/// Atomically select and install paragraph shapes before every consumer of
/// layout, including the TOC fixpoint, page-budget fitting and text verification.
pub(super) fn apply(lines: &mut Vec<Line>, page: PageGeom, candidates: &Candidates) {
    if candidates.paragraphs.is_empty() || PageNotes::scan(lines).is_some() {
        return;
    }
    if let Some(selected) = select(lines, page, candidates) {
        *lines = selected;
    }
}

fn select(lines: &[Line], page: PageGeom, candidates: &Candidates) -> Option<Vec<Line>> {
    let mut choices = Vec::<Choices>::new();
    let mut blocks = Vec::<BlockCandidates>::new();
    let mut policies = Vec::new();
    let mut start = 0;
    let mut any_alternatives = false;
    while start < lines.len() {
        let end = pdf_height_block_end(lines, start);
        if end <= start {
            return None;
        }
        let mut variants = vec![block_variant(lines, start, end, 0, |_| 0.0)];
        let mut alternatives = Vec::new();
        if let Some(paragraph) = candidates.paragraphs.get(&lines[start].flow.group)
            && eligible(&lines[start..end], paragraph)
        {
            for alternative in &paragraph.alternatives {
                if alternative.lines.len() < 4 {
                    continue;
                }
                let shape = finalized_shape(&lines[start..end], alternative);
                // Heading penalties inspect the three preceding physical
                // lines. Include that real context when pricing each shape.
                // The >=4-line eligibility rule keeps costs at the *next*
                // block boundary independent of which shape this one takes.
                let prefix_start = start.saturating_sub(3);
                let prefix_len = start - prefix_start;
                let mut contextual: Vec<Line> = lines[prefix_start..start]
                    .iter()
                    .map(cost_context)
                    .collect();
                contextual.extend(shape.iter().cloned());
                let shape_end = contextual.len();
                if let Some(next) = lines.get(end) {
                    contextual.push(cost_context(next));
                }
                variants.push(block_variant(
                    &contextual,
                    prefix_len,
                    shape_end,
                    alternative.extra_demerits,
                    |_| 0.0,
                ));
                alternatives.push(shape);
            }
        }
        any_alternatives |= !alternatives.is_empty();
        blocks.push(BlockCandidates { variants });
        policies.push(block_policy(lines, start));
        choices.push(Choices {
            start,
            end,
            alternatives,
        });
        start = end;
    }
    if !any_alternatives {
        return None;
    }
    let plan = plan_blocks(&blocks, &policies, options(page)).ok()?;
    if plan.variants.iter().all(|&variant| variant == 0) {
        return None;
    }

    let mut selected = Vec::with_capacity(lines.len());
    let mut offsets = Vec::with_capacity(choices.len());
    for (index, choice) in choices.iter_mut().enumerate() {
        offsets.push(selected.len());
        let variant = *plan.variants.get(index)?;
        if variant == 0 {
            selected.extend(lines[choice.start..choice.end].iter().cloned());
        } else {
            selected.append(choice.alternatives.get_mut(variant - 1)?);
        }
    }

    // The remaining PDF pipeline takes flattened lines. Certify that its
    // existing planner reproduces the same page boundaries after substitution;
    // a new context-sensitive policy cannot silently invalidate this bridge.
    let mut expected_ends = Vec::with_capacity(plan.page_count);
    for (index, fragment) in plan.fragments.iter().enumerate() {
        if plan
            .fragments
            .get(index + 1)
            .is_none_or(|next| next.page_index != fragment.page_index)
        {
            expected_ends.push(offsets[fragment.block_index] + fragment.fragment_end);
        }
    }
    let reproduced = exact_height_page_breaks(&selected, page)?;
    if reproduced.ends != expected_ends {
        return None;
    }
    Some(selected)
}

#[cfg(test)]
#[path = "joint_pagination_tests.rs"]
mod tests;
