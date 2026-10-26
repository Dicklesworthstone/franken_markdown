//! Same-measure paragraph alternatives for the opt-in PDF page planner.
//!
//! Shaping happens while the paragraph's tokens and fonts are available; the
//! choice happens after block layout has finalized gaps and chapter boundaries.
//! Ordinary and blockquoted paragraphs with at least four lines in every
//! offered shape participate. That keeps the short-caption, list, and heading
//! boundary costs independent of choices in preceding blocks. Prepared page
//! notes reserve their unchanged measured bodies at each alternative's first
//! citation; selecting a shape never turns the note region into body content.

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
    report: PdfParagraphVariantReport,
}

impl Candidates {
    pub fn new(opts: &PdfOptions, blocks: &[Block]) -> Self {
        let status = if !opts.optimal_pagination {
            PdfParagraphVariantStatus::NotRequested
        } else if opts.gradual_demerits || opts.river_penalty || opts.pareto_line_breaking {
            PdfParagraphVariantStatus::UnsupportedTypography
        } else if collect_pdf_footnote_defs(blocks)
            .iter()
            .any(|(id, _)| page_note_number(id).is_none())
        {
            // The public PDF path prepares numbered, linked citations.
            PdfParagraphVariantStatus::UnpreparedFootnotes
        } else {
            PdfParagraphVariantStatus::NoAlternatives
        };
        Self {
            enabled: status == PdfParagraphVariantStatus::NoAlternatives,
            report: PdfParagraphVariantReport {
                status,
                ..PdfParagraphVariantReport::default()
            },
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
        self.report = PdfParagraphVariantReport {
            status: self.report.status,
            ..PdfParagraphVariantReport::default()
        };
    }

    pub fn begin_measure(&mut self, items: usize, baseline_count: usize) -> bool {
        let work = items
            .saturating_mul(items)
            .saturating_mul(baseline_count.saturating_add(2));
        if !self.enabled || baseline_count < 4 {
            return false;
        }
        let item_limit = items > MAX_PARAGRAPH_ITEMS;
        let line_limit = self
            .retained_lines
            .saturating_add(baseline_count.saturating_mul(2))
            > MAX_ALTERNATIVE_LINES;
        let work_limit = self.measurement_work.saturating_add(work) > MAX_MEASUREMENT_WORK;
        if item_limit || line_limit || work_limit {
            self.report.limited_paragraphs += 1;
            self.report.paragraph_item_limit |= item_limit;
            self.report.retained_line_limit |= line_limit;
            self.report.measurement_work_limit |= work_limit;
            return false;
        }
        self.measurement_work += work;
        self.report.attempted_paragraphs += 1;
        true
    }

    pub fn candidate_state_limit(&mut self) {
        self.report.candidate_state_limit = true;
        self.report.limited_paragraphs += 1;
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
            self.report.retained_line_limit = true;
            self.report.limited_paragraphs += 1;
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
                // A quote decorates the already measured paragraph; it does
                // not change its glyph positions here. All physical lines
                // must belong to the same complete ancestry, including nested
                // quote IDs and gutter coordinates, before transferring it.
                && line.quote_bars == lines[0].quote_bars
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
        // Quotes stamp their ancestor bars after paragraph shaping. Carry
        // that full path to every replacement line: it owns both the painted
        // gutter/background and the tagged-PDF BlockQuote structure. The
        // alternative was measured at this quote's inset, so never translate
        // or reshape its segments when installing the decoration.
        line.quote_bars.clone_from(&first.quote_bars);
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
        table_source_row: None,
        table_footing: Vec::new(),
        heading_source: None,
        text_ink: (0.0, 0.0),
        segs: Vec::new(),
        image: None,
    }
}

/// The exact linked citation sequence, including repeated references. A
/// different sequence cannot safely reuse the original first-reference map.
fn note_references(lines: &[Line]) -> impl Iterator<Item = u32> + '_ {
    lines
        .iter()
        .flat_map(|line| &line.segs)
        .filter_map(seg_page_note)
}

/// Re-anchor first-cited note bodies to the physical lines of one measured
/// alternative. Notes first cited in another block do not reserve space again.
/// Preserve the production reserve's summation order and separator-per-citing-
/// line policy: regrouping citations can change separator cost, not note text.
fn shape_note_reservations(
    lines: &[Line],
    notes: &PageNotes,
    start: usize,
    end: usize,
    shape: &[Line],
) -> Option<Vec<f32>> {
    if !note_references(&lines[start..end]).eq(note_references(shape)) {
        return None;
    }
    let first_refs = notes.notes_in(start, end);
    if first_refs.is_empty() {
        return Some(Vec::new());
    }
    let mut pending: BTreeMap<u32, std::ops::Range<usize>> = first_refs
        .iter()
        .map(|(_, range)| (lines[range.start].flow.note, range.clone()))
        .collect();
    let height = |line: &Line| line_leading(line) + line.gap_after;
    let separator = height(&lines[notes.separator]);
    let mut reservations = vec![0.0; shape.len()];
    for (index, line) in shape.iter().enumerate() {
        let first_here: Vec<_> = line
            .segs
            .iter()
            .filter_map(seg_page_note)
            .filter_map(|number| pending.remove(&number))
            .collect();
        if !first_here.is_empty() {
            reservations[index] = separator
                + first_here
                    .iter()
                    .flat_map(|range| &lines[range.clone()])
                    .map(height)
                    .sum::<f32>();
        }
    }
    // A missing first citation must never silently drop its reserved body.
    pending.is_empty().then_some(reservations)
}

/// Atomically select and install paragraph shapes before every consumer of
/// layout, including the TOC fixpoint, page-budget fitting and text verification.
pub(super) fn apply(
    lines: &mut Vec<Line>,
    page: PageGeom,
    candidates: &Candidates,
) -> PdfParagraphVariantReport {
    let mut report = candidates.report.clone();
    if !candidates.paragraphs.is_empty() {
        match select_reported(lines, page, candidates, &mut report) {
            Ok(Some(selected)) => *lines = selected,
            Ok(None) => {}
            Err(reason) => {
                report.status = PdfParagraphVariantStatus::Rejected;
                report.failure = Some(reason);
            }
        }
    }
    report
}

#[cfg(test)]
fn select(lines: &[Line], page: PageGeom, candidates: &Candidates) -> Option<Vec<Line>> {
    select_reported(lines, page, candidates, &mut candidates.report.clone())
        .ok()
        .flatten()
}

fn select_reported(
    lines: &[Line],
    page: PageGeom,
    candidates: &Candidates,
    report: &mut PdfParagraphVariantReport,
) -> std::result::Result<Option<Vec<Line>>, PdfPaginationFailure> {
    let notes = PageNotes::scan(lines);
    let body_end = notes.as_ref().map_or(lines.len(), |notes| notes.body_end);
    let body = &lines[..body_end];
    let mut choices = Vec::<Choices>::new();
    let mut blocks = Vec::<BlockCandidates>::new();
    let mut policies = Vec::new();
    let mut start = 0;
    let mut any_alternatives = false;
    while start < body.len() {
        let end = pdf_height_block_end(body, start);
        if end <= start {
            return Err(PdfPaginationFailure::InvalidPlan(
                "joint block span did not advance",
            ));
        }
        let mut variants = vec![block_variant(body, start, end, 0, |line| {
            notes
                .as_ref()
                .map_or(0.0, |notes| notes.line_reserve(lines, line))
        })];
        let mut alternatives = Vec::new();
        if let Some(paragraph) = candidates.paragraphs.get(&body[start].flow.group)
            && eligible(&body[start..end], paragraph)
        {
            for alternative in &paragraph.alternatives {
                if alternative.lines.len() < 4 {
                    continue;
                }
                let shape = finalized_shape(&body[start..end], alternative);
                let reservations = if let Some(notes) = &notes {
                    let Some(reservations) =
                        shape_note_reservations(lines, notes, start, end, &shape)
                    else {
                        // Losing/reordering a citation must not make an
                        // apparently cheaper but semantically different shape.
                        continue;
                    };
                    reservations
                } else {
                    Vec::new()
                };
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
                if let Some(next) = body.get(end) {
                    contextual.push(cost_context(next));
                }
                variants.push(block_variant(
                    &contextual,
                    prefix_len,
                    shape_end,
                    alternative.extra_demerits,
                    |line| reservations.get(line - prefix_len).copied().unwrap_or(0.0),
                ));
                alternatives.push(shape);
            }
        }
        any_alternatives |= !alternatives.is_empty();
        report.offered_paragraphs += usize::from(!alternatives.is_empty());
        blocks.push(BlockCandidates { variants });
        policies.push(block_policy(body, start));
        choices.push(Choices {
            start,
            end,
            alternatives,
        });
        start = end;
    }
    if !any_alternatives {
        return Ok(None);
    }
    let plan =
        plan_blocks(&blocks, &policies, options(page)).map_err(PdfPaginationFailure::Planner)?;
    report.status = PdfParagraphVariantStatus::Planned;
    if plan.variants.iter().all(|&variant| variant == 0) {
        return Ok(None);
    }

    let mut selected = Vec::with_capacity(lines.len());
    let mut offsets = Vec::with_capacity(choices.len());
    for (index, choice) in choices.iter_mut().enumerate() {
        offsets.push(selected.len());
        let variant = *plan
            .variants
            .get(index)
            .ok_or(PdfPaginationFailure::InvalidPlan(
                "joint plan omitted a block",
            ))?;
        if variant == 0 {
            selected.extend(lines[choice.start..choice.end].iter().cloned());
        } else {
            selected.append(choice.alternatives.get_mut(variant - 1).ok_or(
                PdfPaginationFailure::InvalidPlan("joint plan selected an unknown variant"),
            )?);
        }
    }

    // Measured note bodies and their separator remain in the trailing region.
    // They are reserved by body citations, never offered as normal page blocks.
    selected.extend(lines[body_end..].iter().cloned());

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
            let offset =
                offsets
                    .get(fragment.block_index)
                    .ok_or(PdfPaginationFailure::InvalidPlan(
                        "joint fragment selected an unknown block",
                    ))?;
            expected_ends.push(offset.checked_add(fragment.fragment_end).ok_or(
                PdfPaginationFailure::InvalidPlan("joint fragment offset overflow"),
            )?);
        }
    }
    // Re-scan after substitution: both body indices and note-region offsets
    // may have moved. Use the emitter's footnote-aware planner, not the
    // no-reservation bridge, to certify the chosen boundaries.
    let reproduced = if notes.is_some() {
        let selected_notes = PageNotes::scan(&selected).ok_or(
            PdfPaginationFailure::InvalidPlan("joint selection lost the note region"),
        )?;
        exact_height_page_breaks_reserving(&selected[..selected_notes.body_end], page, |line| {
            selected_notes.line_reserve(&selected, line)
        })?
    } else {
        exact_height_page_breaks(&selected, page)?
    };
    if reproduced.ends != expected_ends {
        return Err(PdfPaginationFailure::InvalidPlan(
            "joint page boundaries could not be reproduced",
        ));
    }
    report.selected_alternatives = plan
        .variants
        .iter()
        .filter(|&&variant| variant != 0)
        .count();
    Ok(Some(selected))
}

#[cfg(test)]
#[path = "joint_pagination_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "joint_pagination_note_tests.rs"]
mod note_tests;
