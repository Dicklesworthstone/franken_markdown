//! Narrow recovery for an impossible heading or short-introduction keep in best-effort PDF.
//!
//! The hard planner always runs first. Measure the chain of headings and short
//! introductions, then the smallest legal first fragment of their subject.
//! Each must fit alone, while their combined heights and reservations cannot.
//! The final block's keep is then priced by the existing boundary penalty;
//! block interiors, the rest of the chain and every other hard rule survive.
//! One bounded replan covers all admitted boundaries. Strict requests and the
//! continued-footnote allocator never enter this recovery path.

use super::*;
use crate::pagination::height::{BlockPolicy, BlockVariant};

#[derive(Clone, Copy, Default)]
struct Measured {
    height: i64,
    reserved: i64,
}

impl Measured {
    fn prefix(variant: &BlockVariant, count: usize) -> Option<Self> {
        let sum = |values: &[LayoutUnit]| {
            values.iter().take(count).try_fold(0i64, |total, value| {
                let value = i64::from(value.milli_points());
                if value < 0 {
                    None
                } else {
                    total.checked_add(value)
                }
            })
        };
        Some(Self {
            height: sum(&variant.fragment_heights)?,
            reserved: sum(&variant.fragment_reservations)?,
        })
    }

    fn add(self, other: Self) -> Option<Self> {
        Some(Self {
            height: self.height.checked_add(other.height)?,
            reserved: self.reserved.checked_add(other.reserved)?,
        })
    }

    fn occupied(self, prefix: i64) -> Option<i64> {
        self.height
            .checked_add(self.reserved)?
            .checked_add(if self.reserved == 0 { 0 } else { prefix })
    }
}

/// Earliest internal break that preserves the block's hard split rules and
/// leaves enough rows for a legal continuation. This is a lower bound. The
/// full planner must still find and validate all subsequent pages.
fn first_fragment_len(variant: &BlockVariant, policy: BlockPolicy) -> usize {
    let total = variant.fragment_heights.len();
    if policy.keep_together {
        return total;
    }
    (1..total)
        .find(|&count| {
            (policy.before_break_penalty.is_some() || count >= policy.min_before_break)
                && (policy.after_break_penalty.is_some() || total - count >= policy.min_after_break)
                && (variant.split_costs.is_empty() || variant.split_costs[count - 1].is_some())
        })
        .unwrap_or(total)
}

/// Starts of the final introducing blocks in independently impossible keeps.
/// Fixed-point admission uses exactly the same variants as the production
/// bridge, including final gaps, baseline-grid heights and page-foot reserves.
fn boundaries(
    lines: &[Line],
    page: PageGeom,
    reservation_prefix: LayoutUnit,
    reserve: impl Fn(usize) -> f32,
) -> Option<Vec<usize>> {
    let capacity = i64::from(joint_pagination::options(page).page_capacity.milli_points());
    let prefix = i64::from(reservation_prefix.milli_points());
    let mut result = Vec::new();
    let mut chain: Option<Measured> = None;
    let mut last_intro = 0;
    let mut start = 0;
    while start < lines.len() {
        let end = pdf_height_block_end(lines, start);
        let policy = joint_pagination::block_policy(lines, start);
        if policy.break_before {
            chain = None;
        }
        // Headings and short introductions are the only measured prose
        // blocks kept both internally intact and bonded to their subject.
        if matches!(
            lines[start].flow.kind,
            FlowKind::Heading | FlowKind::Paragraph
        ) && policy.keep_together
            && policy.keep_with_next
        {
            let variant = joint_pagination::block_variant(lines, start, end, 0, &reserve);
            let measured = Measured::prefix(&variant, end - start)?;
            chain = Some(chain.unwrap_or_default().add(measured)?);
            last_intro = start;
        } else {
            if lines[start].flow.kind != FlowKind::Heading
                && !policy.break_before
                // A label, definition term or hyphenated word may independently
                // forbid this boundary; recovery cannot weaken it.
                && !pdf_split_is_hard_forbidden(lines, start)
                && let Some(intros) = chain
                && intros.occupied(prefix)? <= capacity
            {
                let variant = joint_pagination::block_variant(lines, start, end, 0, &reserve);
                let subject = Measured::prefix(&variant, first_fragment_len(&variant, policy))?;
                if subject.occupied(prefix)? <= capacity
                    && intros.add(subject)?.occupied(prefix)? > capacity
                {
                    result.push(last_intro);
                }
            }
            chain = None;
        }
        start = end;
    }
    Some(result)
}

/// Failure keeps the original hard-planner reason and its existing fallback.
/// Optional recovery must never advertise an incomplete or unchecked plan.
pub(super) fn plan(
    lines: &[Line],
    page: PageGeom,
    reservation_prefix: LayoutUnit,
    reserve: impl Fn(usize) -> f32,
) -> Option<PageBreakPlan> {
    let relaxed = boundaries(lines, page, reservation_prefix, &reserve)?;
    if relaxed.is_empty() {
        return None;
    }
    height_page_breaks_reserving(lines, page, reservation_prefix, reserve, &relaxed).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn page(capacity: f32) -> PageGeom {
        PageGeom {
            width: 400.0,
            height: capacity,
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
            content_w: 400.0,
            grid: None,
        }
    }

    fn line(kind: FlowKind, group: u32, height: f32) -> Line {
        Line {
            size: height,
            gap_after: 0.0,
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark {
                kind,
                group,
                ..FlowMark::default()
            },
            page_break_before: false,
            list_path: Vec::new(),
            table_cols: Vec::new(),
            table_source_row: None,
            table_footing: Vec::new(),
            heading_source: None,
            text_ink: (0.0, 0.0),
            spacing: spacing::LineSpacing {
                leading: Some(1.0),
                ..Default::default()
            },
            grid: None,
            hyphenated: false,
            right_protrusion: 0.0,
            segs: Vec::new(),
            image: None,
        }
    }

    fn fixture() -> Vec<Line> {
        let mut lines = vec![
            line(FlowKind::Heading, 1, 20.0),
            line(FlowKind::Heading, 2, 20.0),
            line(FlowKind::Image, 3, 70.0),
        ];
        lines.extend((0..10).map(|index| {
            let mut row = line(FlowKind::Paragraph, 4, 10.0);
            row.flow.index = index;
            row.flow.count = 10;
            row
        }));
        lines
    }

    #[test]
    fn impossible_heading_chain_recovers_without_relaxing_paragraph_fragments() {
        let lines = fixture();
        let page = page(100.0);
        assert!(exact_height_page_breaks(&lines, page).is_err());
        assert_eq!(
            boundaries(&lines, page, LayoutUnit::ZERO, |_| 0.0),
            Some(vec![1])
        );
        let opts = PdfOptions {
            optimal_pagination: true,
            ..PdfOptions::default()
        };
        let (plan, report) = validated_page_break_plan(&lines, page, &opts).unwrap();
        let plan = plan.unwrap();
        assert_eq!(report.strategy, PdfPaginationStrategy::RelaxedHeight);
        assert!(!report.is_exact());
        assert_eq!(report.strategy.as_str(), "relaxed_height");
        assert!(matches!(
            report.fallback,
            Some(PdfPaginationFailure::Planner(
                crate::pagination::height::HeightPaginationError::NoFeasibleLayout { .. }
            ))
        ));
        assert_eq!(plan.ends[0], 2, "both headings remain together");
        let mut start = 0;
        for &end in &plan.ends {
            let height: i32 = lines[start..end]
                .iter()
                .map(|line| lu_from_points_f32(line_leading(line)).milli_points())
                .sum();
            assert!(height <= 100_000, "{start}..{end}: {height}");
            let paragraph_count = lines[start..end]
                .iter()
                .filter(|line| line.flow.kind == FlowKind::Paragraph)
                .count();
            assert!(paragraph_count == 0 || paragraph_count >= 2);
            start = end;
        }
        assert_eq!(start, lines.len());
        assert_eq!(
            plan.ends,
            validated_page_break_plan(&lines, page, &opts)
                .unwrap()
                .0
                .unwrap()
                .ends
        );
    }

    #[test]
    fn strict_requests_keep_the_original_typed_infeasibility() {
        let lines = fixture();
        let page = page(100.0);
        let original = exact_height_page_breaks(&lines, page).err().unwrap();
        let opts = PdfOptions {
            strict_pagination: true,
            ..PdfOptions::default()
        };
        assert!(matches!(
            validated_page_break_plan(&lines, page, &opts),
            Err(RenderError::Pagination(reason)) if reason == original
        ));
    }

    #[test]
    fn feasible_keeps_and_explicit_boundaries_are_unchanged() {
        let mut lines = fixture();
        lines[2].size = 60.0;
        let page = page(100.0);
        assert!(
            boundaries(&lines, page, LayoutUnit::ZERO, |_| 0.0)
                .unwrap()
                .is_empty()
        );
        let exact = exact_height_page_breaks(&lines, page).unwrap();
        let opts = PdfOptions {
            optimal_pagination: true,
            ..PdfOptions::default()
        };
        let (actual, report) = page_break_plan_and_report(&lines, page, &opts);
        assert!(report.is_exact());
        assert_eq!(actual.unwrap().ends, exact.ends);

        lines[2].size = 70.0;
        lines[2].page_break_before = true;
        assert!(
            boundaries(&lines, page, LayoutUnit::ZERO, |_| 0.0)
                .unwrap()
                .is_empty()
        );
        assert!(page_break_plan_and_report(&lines, page, &opts).1.is_exact());
    }

    #[test]
    fn single_millipoint_and_note_separator_reservations_use_exact_admission() {
        let lines = vec![
            line(FlowKind::Heading, 1, 20.0),
            line(FlowKind::Image, 2, 80.0),
        ];
        let page = page(100.0);
        assert!(
            boundaries(&lines, page, LayoutUnit::ZERO, |_| 0.0)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            boundaries(&lines, page, LayoutUnit::ZERO, |index| {
                if index == 1 { 0.001 } else { 0.0 }
            }),
            Some(vec![0])
        );
        let prefix = LayoutUnit::from_points(5);
        let reserved = |index| if index == 0 { 10.0 } else { 0.0 };
        assert_eq!(boundaries(&lines, page, prefix, reserved), Some(vec![0]));
        let recovered = plan(&lines, page, prefix, reserved).unwrap();
        assert_eq!(recovered.ends, [1, 2]);
        assert!(
            boundaries(&lines, page, prefix, |index| {
                if index == 1 { 16.0 } else { 0.0 }
            })
            .unwrap()
            .is_empty(),
            "a subject that cannot fit even alone must retain the original fallback"
        );
    }

    #[test]
    fn other_hard_associations_and_fitting_first_fragments_are_never_relaxed() {
        let page = page(100.0);
        let mut cases = Vec::new();
        let mut oversized = fixture();
        oversized[2].size = 101.0;
        cases.push(oversized);
        let mut headings = fixture();
        headings[0].size = 81.0;
        cases.push(headings);
        let mut definition = fixture();
        definition[1].flow.note_definition_keep_next = Some(true);
        cases.push(definition);
        let mut hyphen = fixture();
        hyphen[1].hyphenated = true;
        cases.push(hyphen);
        let mut caption = fixture();
        caption[0].flow.kind = FlowKind::Paragraph;
        caption.remove(1);
        caption[1].size = 90.0;
        cases.push(caption);
        let mut code = fixture();
        code[2].flow.kind = FlowKind::Code;
        code[2].size = 60.0;
        code[2].flow.count = 2;
        let mut continuation = code[2].clone();
        continuation.flow.index = 1;
        code.insert(3, continuation);
        cases.push(code);
        for lines in cases {
            assert!(
                boundaries(&lines, page, LayoutUnit::ZERO, |_| 0.0)
                    .unwrap()
                    .is_empty()
            );
            assert!(plan(&lines, page, LayoutUnit::ZERO, |_| 0.0).is_none());
        }
    }

    #[test]
    fn recovery_keeps_the_table_header_and_wrapped_first_row_together() {
        let mut lines = vec![
            line(FlowKind::Heading, 1, 40.0),
            line(FlowKind::TableHeader, 2, 15.0),
            line(FlowKind::TableRule, 2, 5.0),
            line(FlowKind::TableRow, 2, 25.0),
            line(FlowKind::TableRow, 2, 25.0),
            line(FlowKind::TableRow, 2, 25.0),
        ];
        lines[3].flow.count = 2;
        lines[4].flow.count = 2;
        lines[4].flow.index = 1;
        let opts = PdfOptions {
            optimal_pagination: true,
            ..PdfOptions::default()
        };
        let (plan, report) = validated_page_break_plan(&lines, page(100.0), &opts).unwrap();
        assert_eq!(report.strategy, PdfPaginationStrategy::RelaxedHeight);
        assert_eq!(plan.unwrap().ends, [1, 6]);

        // At exactly 110pt the first complete table fragment fits with its
        // heading, even though the whole table still needs another page.
        assert!(
            boundaries(&lines, page(110.0), LayoutUnit::ZERO, |_| 0.0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unrelated_infeasibility_still_uses_the_original_fallback() {
        let mut lines = fixture();
        lines.truncate(6);
        for (index, row) in lines[3..].iter_mut().enumerate() {
            row.size = 40.0;
            row.flow.index = index;
            row.flow.count = 3;
        }
        let opts = PdfOptions {
            optimal_pagination: true,
            ..PdfOptions::default()
        };
        let (plan, report) = page_break_plan_and_report(&lines, page(100.0), &opts);
        assert!(plan.is_some());
        assert_eq!(report.strategy, PdfPaginationStrategy::Legacy);
        assert!(!report.is_exact());
        assert!(report.fallback.is_some());
    }
}
