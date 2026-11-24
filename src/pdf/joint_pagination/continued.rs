//! Exact paragraph selection under the continued-note allocation policy.
//!
//! A bounded complete product of measured paragraph shapes is small enough to
//! reuse the physical continuation planner directly. Each candidate includes
//! the unchanged note region, so first citations, pending fragments, repeated
//! headers and note-only tail pages all contribute to the same objective.
//! Search exhaustion rejects the whole optional selection; an incomplete best
//! candidate is never reported as an exact joint plan.

use super::*;
use crate::pagination::continuation as solver;
use crate::pagination::height::{HeightPaginationError, HeightPaginationLimits};

const MAX_COMBINATIONS: usize = 1024;

#[derive(Clone, Copy)]
struct Variant {
    rows: usize,
    demerits: i64,
}

struct Candidate<T> {
    value: T,
    demerits: i128,
    pages: usize,
}

struct Attempt<T> {
    result: std::result::Result<Candidate<T>, PdfPaginationFailure>,
    usage: solver::Usage,
}

struct Winner<T> {
    variants: Vec<usize>,
    value: T,
    rank: (i128, usize),
}

fn budget(reason: &'static str) -> PdfPaginationFailure {
    PdfPaginationFailure::Planner(HeightPaginationError::BudgetExceeded(reason))
}

fn overflow() -> PdfPaginationFailure {
    PdfPaginationFailure::Planner(HeightPaginationError::CostOverflow)
}

/// Bound all repeated materialization before cloning any document. Each
/// variant participates in product / block_width complete layouts. Counting
/// the unchanged note region each time also bounds the repeated note scans.
fn admit(
    blocks: &[Vec<Variant>],
    note_rows: usize,
    limits: HeightPaginationLimits,
) -> std::result::Result<usize, PdfPaginationFailure> {
    if blocks.len() > limits.max_blocks {
        return Err(budget("continued paragraph blocks"));
    }
    let mut combinations = 1_usize;
    for variants in blocks {
        if variants.is_empty() {
            return Err(PdfPaginationFailure::InvalidPlan(
                "continued paragraph search received an empty block",
            ));
        }
        if variants.len() > limits.max_variants_per_block {
            return Err(budget("continued paragraph variants"));
        }
        combinations = combinations
            .checked_mul(variants.len())
            .filter(|&count| count <= MAX_COMBINATIONS)
            .ok_or_else(|| budget("continued paragraph combinations"))?;
    }
    let mut rows = note_rows
        .checked_mul(combinations)
        .ok_or_else(|| budget("continued paragraph materialized rows"))?;
    for variants in blocks {
        let sum = variants.iter().try_fold(0_usize, |sum, variant| {
            sum.checked_add(variant.rows)
                .ok_or_else(|| budget("continued paragraph materialized rows"))
        })?;
        let contribution = sum
            .checked_mul(combinations / variants.len())
            .ok_or_else(|| budget("continued paragraph materialized rows"))?;
        rows = rows
            .checked_add(contribution)
            .ok_or_else(|| budget("continued paragraph materialized rows"))?;
    }
    if rows > limits.max_candidate_fragments {
        return Err(budget("continued paragraph materialized rows"));
    }
    Ok(combinations)
}

fn charge(
    used: &mut usize,
    amount: usize,
    limit: usize,
    reason: &'static str,
) -> std::result::Result<(), PdfPaginationFailure> {
    *used = used
        .checked_add(amount)
        .filter(|&total| total <= limit)
        .ok_or_else(|| budget(reason))?;
    Ok(())
}

/// The callback evaluates the actual flattened layout. It reports work even
/// when that layout is infeasible, so unsuccessful combinations cannot evade
/// the document-wide budgets. Ties retain the earlier, baseline-first choice.
fn search<T>(
    blocks: &[Vec<Variant>],
    note_rows: usize,
    limits: HeightPaginationLimits,
    mut evaluate: impl FnMut(&[usize], solver::Limits) -> Attempt<T>,
) -> std::result::Result<Winner<T>, PdfPaginationFailure> {
    let combinations = admit(blocks, note_rows, limits)?;
    let mut selected = vec![0; blocks.len()];
    let mut work = 0;
    let mut nodes = 0;
    let mut ranges = 0;
    let mut best: Option<Winner<T>> = None;
    let mut infeasible = None;
    for _ in 0..combinations {
        let paragraph_cost =
            blocks
                .iter()
                .zip(&selected)
                .try_fold(0_i128, |cost, (variants, &selected)| {
                    cost.checked_add(i128::from(variants[selected].demerits))
                        .ok_or_else(overflow)
                })?;
        let attempt = evaluate(
            &selected,
            solver::Limits {
                max_blocks: limits.max_blocks,
                max_fragments: limits.max_candidate_fragments,
                max_capacity: limits.max_page_capacity_mpt as i64,
                max_nodes: limits.max_nodes - nodes,
                max_work: limits.max_transitions - work,
                max_retained_ranges: limits.max_candidate_fragments - ranges,
            },
        );
        charge(
            &mut work,
            attempt.usage.search_work,
            limits.max_transitions,
            "continued paragraph search work",
        )?;
        charge(
            &mut nodes,
            attempt.usage.search_nodes,
            limits.max_nodes,
            "continued paragraph search nodes",
        )?;
        // Sum each solve's peak live ranges. This conservative cumulative
        // bound includes failed attempts and never replenishes between shapes.
        charge(
            &mut ranges,
            attempt.usage.retained_ranges,
            limits.max_candidate_fragments,
            "continued paragraph search ranges",
        )?;
        match attempt.result {
            Ok(candidate) => {
                let rank = (
                    candidate
                        .demerits
                        .checked_add(paragraph_cost)
                        .ok_or_else(overflow)?,
                    candidate.pages,
                );
                if best.as_ref().is_none_or(|best| rank < best.rank) {
                    best = Some(Winner {
                        variants: selected.clone(),
                        value: candidate.value,
                        rank,
                    });
                }
            }
            Err(
                reason @ PdfPaginationFailure::Planner(HeightPaginationError::NoFeasibleLayout {
                    ..
                }),
            ) => {
                infeasible.get_or_insert(reason);
            }
            Err(reason) => return Err(reason),
        }
        for (selected, variants) in selected.iter_mut().zip(blocks).rev() {
            *selected += 1;
            if *selected < variants.len() {
                break;
            }
            *selected = 0;
        }
    }
    best.ok_or_else(|| {
        infeasible.unwrap_or(PdfPaginationFailure::InvalidPlan(
            "continued paragraph search produced no complete candidate",
        ))
    })
}

pub(super) fn select(
    lines: &[Line],
    page: PageGeom,
    body_end: usize,
    choices: &[Choices],
    blocks: &[BlockCandidates],
    report: &mut PdfParagraphVariantReport,
) -> std::result::Result<Option<Vec<Line>>, PdfPaginationFailure> {
    let variants: Vec<Vec<Variant>> = blocks
        .iter()
        .map(|block| {
            block
                .variants
                .iter()
                .map(|variant| Variant {
                    rows: variant.fragment_heights.len(),
                    demerits: variant.demerits,
                })
                .collect()
        })
        .collect();
    let winner = search(
        &variants,
        lines.len() - body_end,
        options(page).limits,
        |selected, limits| {
            // Borrow the original for the baseline assignment. Every other
            // assignment owns its exact complete layout; only the best one
            // survives the next evaluation, rather than retaining the product.
            let candidate = if selected.iter().all(|&variant| variant == 0) {
                None
            } else {
                let mut candidate = Vec::with_capacity(lines.len());
                for (choice, &variant) in choices.iter().zip(selected) {
                    let shape = if variant == 0 {
                        &lines[choice.start..choice.end]
                    } else {
                        &choice.alternatives[variant - 1]
                    };
                    candidate.extend(shape.iter().cloned());
                }
                candidate.extend(lines[body_end..].iter().cloned());
                Some(candidate)
            };
            let measured = candidate.as_deref().unwrap_or(lines);
            let Some(notes) = PageNotes::scan(measured) else {
                return Attempt {
                    result: Err(PdfPaginationFailure::InvalidPlan(
                        "continued paragraph selection lost the note region",
                    )),
                    usage: solver::Usage::default(),
                };
            };
            let evaluated = split_footnotes::exact::evaluate(measured, &notes, page, Some(limits));
            Attempt {
                result: evaluated.result.map(|evaluation| Candidate {
                    value: candidate,
                    demerits: evaluation.total_demerits,
                    pages: evaluation.page_count,
                }),
                usage: evaluated.usage,
            }
        },
    )?;
    // Unlike the block-variant planner, evaluate already ran on these exact
    // flattened lines and certified the physical body/note allocation. They
    // remain untouched after evaluation, so the shared final planner receives
    // the identical input; no second translation or certificate solve is needed.
    report.status = PdfParagraphVariantStatus::Planned;
    report.selected_alternatives = winner
        .variants
        .iter()
        .filter(|&&variant| variant != 0)
        .count();
    Ok(winner.value)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn variants(costs: &[&[i64]]) -> Vec<Vec<Variant>> {
        costs
            .iter()
            .map(|costs| {
                costs
                    .iter()
                    .map(|&demerits| Variant { rows: 4, demerits })
                    .collect()
            })
            .collect()
    }

    fn success<T>(value: T, demerits: i128, pages: usize) -> Attempt<T> {
        Attempt {
            result: Ok(Candidate {
                value,
                demerits,
                pages,
            }),
            usage: solver::Usage {
                search_work: 2,
                search_nodes: 1,
                retained_ranges: 1,
            },
        }
    }

    #[test]
    fn complete_choice_objective_includes_paragraph_costs_and_note_tail_pages() {
        let blocks = variants(&[&[0, 70], &[0, 30]]);
        // The complete continuation costs include differing pending-note
        // tails. Neither independently cheapest paragraph nor page cost alone
        // determines the winning complete combination.
        let costs = [[(200, 4), (190, 3)], [(150, 3), (90, 2)]];
        let mut visited = Vec::new();
        let winner = search(
            &blocks,
            8,
            HeightPaginationLimits::default(),
            |selected, _| {
                visited.push(selected.to_vec());
                let (cost, pages) = costs[selected[0]][selected[1]];
                success(selected.to_vec(), cost, pages)
            },
        )
        .unwrap();
        assert_eq!(visited, [[0, 0], [0, 1], [1, 0], [1, 1]]);
        assert_eq!(winner.variants, [1, 1]);
        assert_eq!(winner.value, [1, 1]);
        assert_eq!(winner.rank, (190, 2));

        let expensive = variants(&[&[0, 500]]);
        let winner = search(
            &expensive,
            8,
            HeightPaginationLimits::default(),
            |selected, _| success((), if selected[0] == 0 { 200 } else { 90 }, 2),
        )
        .unwrap();
        assert_eq!(winner.variants, [0]);
    }

    #[test]
    fn ties_prefer_fewer_physical_pages_then_baseline_order() {
        let blocks = variants(&[&[0, 0], &[0, 0]]);
        let winner = search(
            &blocks,
            4,
            HeightPaginationLimits::default(),
            |selected, _| success((), 100, if selected == [0, 0] { 3 } else { 2 }),
        )
        .unwrap();
        assert_eq!(winner.variants, [0, 1]);
    }

    #[test]
    fn infeasible_assignments_still_consume_the_shared_search_budget() {
        let blocks = variants(&[&[0, 1, 2]]);
        let limits = HeightPaginationLimits {
            max_transitions: 9,
            max_nodes: 8,
            max_candidate_fragments: 100,
            ..HeightPaginationLimits::default()
        };
        let mut remaining = Vec::new();
        let winner = search(&blocks, 4, limits, |selected, limits| {
            remaining.push((
                limits.max_work,
                limits.max_nodes,
                limits.max_retained_ranges,
            ));
            let mut attempt = success((), 100, 2);
            if selected[0] == 0 {
                attempt.result = Err(PdfPaginationFailure::Planner(
                    HeightPaginationError::NoFeasibleLayout { block_index: 0 },
                ));
            }
            attempt
        })
        .unwrap();
        assert_eq!(winner.variants, [1]);
        assert_eq!(remaining, [(9, 8, 100), (7, 7, 99), (5, 6, 98)]);
    }

    #[test]
    fn later_budget_failure_discards_an_earlier_feasible_winner() {
        let blocks = variants(&[&[0, 0]]);
        let error = search(
            &blocks,
            4,
            HeightPaginationLimits::default(),
            |selected, _| {
                if selected[0] == 0 {
                    success((), 100, 2)
                } else {
                    Attempt {
                        result: Err(budget("continuation work")),
                        usage: solver::Usage::default(),
                    }
                }
            },
        )
        .err()
        .unwrap();
        assert_eq!(error, budget("continuation work"));
    }

    #[test]
    fn combination_and_materialization_bounds_refuse_before_evaluation() {
        let blocks = variants(&[&[0_i64, 0][..]; 11]);
        let error = search::<()>(&blocks, 4, HeightPaginationLimits::default(), |_, _| {
            panic!("over-budget product must not materialize")
        })
        .err()
        .unwrap();
        assert_eq!(error, budget("continued paragraph combinations"));
        let blocks = variants(&[&[0, 0], &[0, 0]]);
        let limits = HeightPaginationLimits {
            // Four layouts, each with eight body and three note rows.
            max_candidate_fragments: 43,
            ..HeightPaginationLimits::default()
        };
        assert_eq!(
            admit(&blocks, 3, limits),
            Err(budget("continued paragraph materialized rows"))
        );
        assert_eq!(
            admit(
                &blocks,
                3,
                HeightPaginationLimits {
                    max_candidate_fragments: 44,
                    ..limits
                }
            ),
            Ok(4)
        );
    }

    /// Exhaust all body partitions without calling the production pager.
    /// This fixture has two paragraphs and one ordinary continued note: every
    /// body row is ten units, every note row five, and the separator two.
    fn partition_oracle(
        first_rows: usize,
        second_rows: usize,
        note_rows: usize,
        capacity: i64,
    ) -> Option<(i128, usize)> {
        let total = first_rows + second_rows;
        let reference = first_rows - 1;
        let paragraphs = [0..first_rows, first_rows..total];
        let mut best = None;
        for cuts in 0..(1_usize << (total - 1)) {
            let mut ends: Vec<usize> = (1..total)
                .filter(|&end| cuts & (1 << (end - 1)) != 0)
                .collect();
            ends.push(total);
            let mut start = 0;
            let mut pending = 0_usize;
            let mut used_pages = Vec::new();
            let mut feasible = true;
            for end in ends {
                for paragraph in &paragraphs {
                    let intersection_start = start.max(paragraph.start);
                    let intersection_end = end.min(paragraph.end);
                    if intersection_start < intersection_end
                        && (intersection_start != paragraph.start
                            || intersection_end != paragraph.end)
                        && intersection_end - intersection_start < 2
                    {
                        feasible = false;
                    }
                }
                let body = ((end - start) * 10) as i64;
                let fresh = (start..end).contains(&reference);
                if fresh {
                    pending = note_rows;
                }
                let note_budget = (capacity * 2 / 5).min(capacity - body);
                let optional_rows = ((note_budget - 2).max(0) / 5) as usize;
                let take = pending.min(optional_rows.max(usize::from(fresh)));
                pending -= take;
                let reserve = if take == 0 { 0 } else { 2 + take as i64 * 5 };
                let used = body + reserve;
                if used > capacity {
                    feasible = false;
                }
                used_pages.push(used);
                start = end;
            }
            if !feasible {
                continue;
            }
            while pending > 0 {
                let take = pending.min(((capacity - 2) / 5) as usize);
                assert!(take > 0);
                pending -= take;
                used_pages.push(2 + take as i64 * 5);
            }
            let mut cost = used_pages.len() as i128 * 100;
            for &used in &used_pages[..used_pages.len() - 1] {
                let gap = i128::from(capacity - used);
                cost += gap * gap * 100 / i128::from(capacity).pow(2);
            }
            let rank = (cost, used_pages.len());
            if best.is_none_or(|previous| rank < previous) {
                best = Some(rank);
            }
        }
        best
    }

    #[test]
    fn actual_continuation_solver_and_selector_match_complete_partition_oracle() {
        let blocks = vec![
            vec![
                Variant {
                    rows: 4,
                    demerits: 0,
                },
                Variant {
                    rows: 5,
                    demerits: -13,
                },
                Variant {
                    rows: 6,
                    demerits: -21,
                },
            ],
            vec![
                Variant {
                    rows: 4,
                    demerits: 0,
                },
                Variant {
                    rows: 5,
                    demerits: -17,
                },
            ],
        ];
        for capacity in [50, 60, 70] {
            for note_rows in [1, 5, 13] {
                let expected = (0..blocks[0].len())
                    .flat_map(|first| (0..blocks[1].len()).map(move |second| [first, second]))
                    .filter_map(|chosen| {
                        let first = blocks[0][chosen[0]];
                        let second = blocks[1][chosen[1]];
                        partition_oracle(first.rows, second.rows, note_rows, capacity).map(
                            |(cost, pages)| {
                                (
                                    cost + i128::from(first.demerits) + i128::from(second.demerits),
                                    pages,
                                    chosen,
                                )
                            },
                        )
                    })
                    .min()
                    .unwrap();
                let winner = search(
                    &blocks,
                    note_rows + 1,
                    HeightPaginationLimits::default(),
                    |chosen, limits| {
                        let first = blocks[0][chosen[0]].rows;
                        let second = blocks[1][chosen[1]].rows;
                        let total = first + second;
                        let input = solver::Input {
                            rows: (0..total)
                                .map(|index| solver::Row {
                                    height: 10,
                                    continuation_prefix: 0,
                                    suffix: 0,
                                    block: usize::from(index >= first),
                                })
                                .collect(),
                            blocks: [0..first, first..total]
                                .into_iter()
                                .map(|range| solver::Block {
                                    range,
                                    min_before_break: 2,
                                    min_after_break: 2,
                                })
                                .collect(),
                            break_costs: vec![Some(0); total + 1],
                            forced: vec![false; total + 1],
                            notes: vec![solver::Note {
                                first_reference: first - 1,
                                constraints: None,
                                units: vec![
                                    solver::Unit {
                                        height: 5,
                                        prefix: 0,
                                        suffix: 0,
                                        minimum_units: 1,
                                    };
                                    note_rows
                                ],
                            }],
                            separator: 2,
                        };
                        let (result, usage) = solver::plan_with_usage(
                            &input,
                            solver::Options {
                                capacity,
                                page_cost: 100,
                                unused_height_cost: 100,
                                limits,
                            },
                        );
                        Attempt {
                            result: result
                                .map(|plan| Candidate {
                                    value: chosen.to_vec(),
                                    demerits: plan.total_demerits,
                                    pages: plan.pages.len(),
                                })
                                .map_err(|error| {
                                    PdfPaginationFailure::Planner(match error {
                                        solver::Error::Invalid(reason) => {
                                            HeightPaginationError::InvalidOptions(reason)
                                        }
                                        solver::Error::Budget(reason) => {
                                            HeightPaginationError::BudgetExceeded(reason)
                                        }
                                        solver::Error::Infeasible { body_line } => {
                                            HeightPaginationError::NoFeasibleLayout {
                                                block_index: body_line,
                                            }
                                        }
                                        solver::Error::CostOverflow => {
                                            HeightPaginationError::CostOverflow
                                        }
                                    })
                                }),
                            usage,
                        }
                    },
                )
                .unwrap();
                assert_eq!(
                    (winner.rank.0, winner.rank.1, winner.variants.as_slice()),
                    (expected.0, expected.1, expected.2.as_slice()),
                    "capacity {capacity}, note rows {note_rows}"
                );
            }
        }
    }

    #[test]
    fn overflowing_complete_objective_is_not_an_exact_winner() {
        let blocks = variants(&[&[1]]);
        let error = search(&blocks, 1, HeightPaginationLimits::default(), |_, _| {
            success((), i128::MAX, 1)
        })
        .err()
        .unwrap();
        assert_eq!(error, overflow());
    }
}
