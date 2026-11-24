//! Measured-PDF bridge to the integer continuation planner.

use super::*;
use crate::pagination::continuation as solver;
use crate::pagination::height::HeightPaginationError;

fn measured(points: f32) -> std::result::Result<i64, PdfPaginationFailure> {
    if !points.is_finite() || points < 0.0 {
        return Err(PdfPaginationFailure::InvalidPlan(
            "continued-note measurement must be finite and nonnegative",
        ));
    }
    Ok(milli_height(points))
}

fn row_height(line: &Line) -> std::result::Result<i64, PdfPaginationFailure> {
    line.grid.map_or_else(
        || measured(line_leading(line) + line.gap_after),
        |grid| Ok(i64::from(grid.height().milli_points())),
    )
}

fn sum_rows(lines: &[Line]) -> std::result::Result<i64, PdfPaginationFailure> {
    lines.iter().try_fold(0_i64, |height, line| {
        height
            .checked_add(row_height(line)?)
            .ok_or(PdfPaginationFailure::InvalidPlan(
                "continued-note measurement overflow",
            ))
    })
}

pub(super) fn plan(
    lines: &[Line],
    notes: &PageNotes,
    page: PageGeom,
) -> std::result::Result<PageBreakPlan, PdfPaginationFailure> {
    let options = joint_pagination::options(page);
    let limits = options.limits;
    if lines.len() > limits.max_candidate_fragments {
        return Err(PdfPaginationFailure::Planner(
            HeightPaginationError::BudgetExceeded("continuation measured rows"),
        ));
    }
    let body = &lines[..notes.body_end];
    let body_headers = TableHeaders::scan(body);
    let mut header_heights = BTreeMap::new();
    for (&group, header) in &body_headers.0 {
        header_heights.insert(group, sum_rows(&body[header.range.clone()])?);
    }
    let mut input = solver::Input {
        rows: Vec::with_capacity(body.len()),
        blocks: Vec::new(),
        break_costs: vec![Some(0); body.len() + 1],
        forced: vec![false; body.len() + 1],
        notes: Vec::with_capacity(notes.first_refs.len()),
        separator: measured(fragment_height(std::slice::from_ref(
            &lines[notes.separator],
        )))?,
    };
    let mut start = 0;
    let mut previous_keeps_next = false;
    while start < body.len() {
        if input.blocks.len() >= limits.max_blocks {
            return Err(PdfPaginationFailure::Planner(
                HeightPaginationError::BudgetExceeded("continuation blocks"),
            ));
        }
        let end = pdf_height_block_end(body, start);
        let policy = joint_pagination::block_policy(body, start);
        let block = input.blocks.len();
        input.blocks.push(solver::Block {
            range: start..end,
            min_before_break: policy.min_before_break,
            min_after_break: policy.min_after_break,
        });
        input.forced[start] = policy.break_before;
        input.break_costs[start] = if previous_keeps_next {
            None
        } else {
            Some(policy.break_before_cost)
        };
        for candidate in (start + 1)..end {
            input.break_costs[candidate] =
                if policy.keep_together || pdf_split_is_hard_forbidden(body, candidate) {
                    None
                } else {
                    Some(break_penalty(body, candidate).round() as i64)
                };
        }
        for (index, line) in body.iter().enumerate().take(end).skip(start) {
            // Charge the complete source-owned header. The greedy painter can
            // suppress a header that does not fit; an exact plan never uses
            // that suppression to admit an otherwise impossible page.
            let continuation_prefix = body_headers.at(body, index).map_or(0, |_| {
                header_heights.get(&line.flow.group).copied().unwrap_or(0)
            });
            input.rows.push(solver::Row {
                height: row_height(line)?.max(1),
                continuation_prefix,
                block,
            });
        }
        previous_keeps_next = policy.keep_with_next;
        start = end;
    }

    let headers = TableHeaders::scan(lines);
    let mut ranges = Vec::with_capacity(notes.first_refs.len());
    for (first_reference, range) in &notes.first_refs {
        let mut units = Vec::new();
        let mut source = Vec::new();
        let mut start = range.start;
        while start < range.end {
            let end = if lines[range.start].flow.split_note {
                unit_end(lines, start, range.end)
            } else {
                range.end
            };
            units.push(solver::Unit {
                height: sum_rows(&lines[start..end])?.max(1),
                prefix: measured(top_guard(&lines[start]) + headers.extra_height(lines, start))?,
                suffix: measured(bottom_guard(&lines[end - 1]))?,
            });
            source.push(start..end);
            start = end;
        }
        input.notes.push(solver::Note {
            first_reference: *first_reference,
            units,
        });
        ranges.push(source);
    }

    let plan = solver::plan(
        &input,
        solver::Options {
            capacity: i64::from(options.page_capacity.milli_points()),
            page_cost: options.page_cost,
            unused_height_cost: options.unused_height_cost,
            limits: solver::Limits {
                max_blocks: limits.max_blocks,
                max_fragments: limits.max_candidate_fragments,
                max_capacity: limits.max_page_capacity_mpt as i64,
                max_nodes: limits.max_nodes,
                max_work: limits.max_transitions,
                max_retained_ranges: limits.max_candidate_fragments,
            },
        },
    )
    .map_err(|error| {
        PdfPaginationFailure::Planner(match error {
            solver::Error::Invalid(reason) => HeightPaginationError::InvalidOptions(reason),
            solver::Error::Budget(reason) => HeightPaginationError::BudgetExceeded(reason),
            solver::Error::Infeasible { body_line } => HeightPaginationError::NoFeasibleLayout {
                block_index: input
                    .rows
                    .get(body_line)
                    .map_or_else(|| input.blocks.len().saturating_sub(1), |row| row.block),
            },
            solver::Error::CostOverflow => HeightPaginationError::CostOverflow,
        })
    })?;
    debug_assert!(plan.search_nodes >= plan.pages.len());
    debug_assert!(plan.search_work > 0);
    debug_assert!(plan.total_demerits >= 0);

    let grid = page.grid.is_some();
    let separator = fragment_height(std::slice::from_ref(&lines[notes.separator]));
    let capacity = page.top_y() - page.bottom;
    let mut pages = Vec::with_capacity(plan.pages.len());
    let mut ends = Vec::new();
    for planned in plan.pages {
        let mut fragments = Vec::with_capacity(planned.fragments.len());
        for fragment in planned.fragments {
            let source = &ranges[fragment.note];
            fragments.push(source[fragment.start].start..source[fragment.end - 1].end);
        }
        // Reconstruct the actual painter's reservation from the same borrowed
        // ranges. This final certificate also catches fixed-point conversion
        // discrepancies instead of silently painting an overfull exact page.
        let reserve = if fragments.is_empty() {
            0.0
        } else {
            fragments.iter().fold(separator, |height, range| {
                add_height(height, headers.fragment_height(lines, range.clone()), grid)
            })
        };
        let height = if planned.body.is_empty() {
            0.0
        } else {
            let geometry = page.with_foot_reserve(reserve);
            if table_header_prefix(body, planned.body.start).len()
                != repeated_table_header_lines(body, planned.body.start, geometry).len()
            {
                return Err(PdfPaginationFailure::InvalidPlan(
                    "continued-note plan could not retain the required body table header",
                ));
            }
            body_height(body, planned.body.start, planned.body.end, geometry, None)
        };
        if !reserve.is_finite() || !height.is_finite() || height + reserve > capacity + 0.001 {
            return Err(PdfPaginationFailure::InvalidPlan(
                "continued-note plan exceeded the measured physical page",
            ));
        }
        if !planned.body.is_empty() {
            ends.push(planned.body.end);
        }
        pages.push(PlannedPage {
            body: planned.body,
            allocation: Allocation {
                fragments,
                consumed_pending: 0,
                continued_pending: None,
                new_pending: Vec::new(),
                reserve,
            },
        });
    }
    Ok(PageBreakPlan {
        ends,
        continuation: Some(Plan { pages }),
    })
}
