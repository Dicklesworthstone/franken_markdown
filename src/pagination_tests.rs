#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::layout::ParagraphVariant;

fn paragraph(variants: &[(usize, i64)]) -> ParagraphCandidates {
    ParagraphCandidates {
        variants: variants.iter().map(|&(line_count, demerits)| ParagraphVariant {
            line_count, demerits, lines: Vec::new(),
        }).collect(),
    }
}
fn options(capacity: usize) -> PaginationOptions {
    PaginationOptions { page_capacity_lines: capacity, page_cost: 100, ..PaginationOptions::default() }
}
fn audit(input: &[ParagraphCandidates], policies: &[ParagraphPolicy], opts: PaginationOptions, plan: &PaginationPlan) {
    let mut placed = vec![0; input.len()];
    let mut page = 0;
    let mut used = opts.initial_used_lines;
    let mut last_paragraph = 0;
    let mut first_pages = vec![None; input.len()];
    let mut last_pages = vec![None; input.len()];
    let mut cost: i128 = input.iter().zip(&plan.variants)
        .map(|(p, &v)| i128::from(p.variants[v].demerits)).sum();
    for fragment in &plan.fragments {
        assert!(fragment.paragraph_index >= last_paragraph);
        last_paragraph = fragment.paragraph_index;
        if fragment.page_index != page {
            assert_eq!(fragment.page_index, page + 1, "no empty pages");
            assert!(used > 0);
            let gap = (opts.page_capacity_lines - used) as i128;
            cost += i128::from(opts.page_cost) + gap * gap * i128::from(opts.unused_line_cost);
            page += 1;
            used = 0;
        }
        assert_eq!(fragment.page_line_start, used);
        assert_eq!(fragment.line_start, placed[last_paragraph]);
        assert_eq!(fragment.variant_chosen, plan.variants[last_paragraph]);
        assert!(fragment.line_end > fragment.line_start);
        let count = fragment.line_end - fragment.line_start;
        used += count;
        assert!(used <= opts.page_capacity_lines);
        let total = input[last_paragraph].variants[fragment.variant_chosen].line_count;
        assert!(fragment.line_end <= total);
        if fragment.line_end < total {
            cost += violation(count, opts.orphans, opts.orphan_penalty).unwrap().expect("legal orphan policy");
        }
        if fragment.line_start > 0 {
            cost += violation(count, opts.widows, opts.widow_penalty).unwrap().expect("legal widow policy");
        }
        placed[last_paragraph] = fragment.line_end;
        first_pages[last_paragraph].get_or_insert(page);
        last_pages[last_paragraph] = Some(page);
    }
    for (index, p) in input.iter().enumerate() {
        assert_eq!(placed[index], p.variants[plan.variants[index]].line_count);
        let policy = policies.get(index).copied().unwrap_or_default();
        if policy.keep_together { assert_eq!(first_pages[index], last_pages[index]); }
        if index > 0 && policy.break_before { assert!(first_pages[index] > last_pages[index - 1]); }
        if index + 1 < input.len() && policy.keep_with_next { assert_eq!(last_pages[index], first_pages[index + 1]); }
    }
    if used > 0 {
        cost += i128::from(opts.page_cost);
        if opts.penalize_last_page {
            let gap = (opts.page_capacity_lines - used) as i128;
            cost += gap * gap * i128::from(opts.unused_line_cost);
        }
        page += 1;
    }
    assert_eq!(plan.page_count, page);
    assert_eq!(plan.total_demerits, cost);
}

#[test]
fn globally_shorter_variant_avoids_a_future_page() {
    let input = [paragraph(&[(6, 0), (5, 1)]), paragraph(&[(5, 0)])];
    let opts = options(10);
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.variants, vec![1, 0]);
    assert_eq!(plan.page_count, 1);
    assert_eq!(plan.total_demerits, 101);
    audit(&input, &[], opts, &plan);
    assert_eq!(plan, plan_pagination(&input, &[], opts).unwrap());
}

#[test]
fn long_paragraph_splits_without_losing_or_overflowing_lines() {
    let input = [paragraph(&[(13, -7)])];
    let opts = options(5);
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.fragments.iter().map(|f| f.line_end - f.line_start).collect::<Vec<_>>(), vec![5, 5, 3]);
    assert_eq!(plan.page_count, 3);
    assert_eq!(plan.total_demerits, 293);
    audit(&input, &[], opts, &plan);
}

#[test]
fn widow_control_moves_a_line_back_instead_of_stranding_one() {
    let input = [paragraph(&[(6, 0)])];
    let opts = options(5);
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.fragments.iter().map(|f| (f.line_start, f.line_end)).collect::<Vec<_>>(), vec![(0, 4), (4, 6)]);
    audit(&input, &[], opts, &plan);
}

#[test]
fn initial_page_orphans_and_forced_breaks_are_respected() {
    let input = [paragraph(&[(3, 0)])];
    let opts = PaginationOptions { initial_used_lines: 4, ..options(5) };
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.fragments[0].page_index, 1);
    audit(&input, &[], opts, &plan);
    let input = [paragraph(&[(2, 0)]), paragraph(&[(2, 0)])];
    let policies = [ParagraphPolicy::default(), ParagraphPolicy { break_before: true, ..ParagraphPolicy::default() }];
    let opts = options(10);
    let plan = plan_pagination(&input, &policies, opts).unwrap();
    assert_eq!(plan.page_count, 2);
    audit(&input, &policies, opts, &plan);
}

#[test]
fn keep_with_next_can_move_a_heading_before_it_is_placed() {
    let input = [paragraph(&[(3, 0)]), paragraph(&[(1, 0)]), paragraph(&[(3, 0)])];
    let policies = [ParagraphPolicy::default(), ParagraphPolicy { keep_with_next: true, ..ParagraphPolicy::default() }, ParagraphPolicy::default()];
    let opts = options(5);
    let plan = plan_pagination(&input, &policies, opts).unwrap();
    assert_eq!(plan.fragments[1].page_index, 1);
    assert_eq!(plan.fragments[2].page_index, 1);
    audit(&input, &policies, opts, &plan);
}

#[test]
fn keep_together_and_conflicting_forced_breaks_fail_closed() {
    let input = [paragraph(&[(6, 0)])];
    let policy = [ParagraphPolicy { keep_together: true, ..ParagraphPolicy::default() }];
    assert!(matches!(plan_pagination(&input, &policy, options(5)), Err(PaginationError::NoFeasibleLayout { .. })));
    let input = [paragraph(&[(6, 0), (5, 1000)])];
    let plan = plan_pagination(&input, &policy, options(5)).unwrap();
    assert_eq!(plan.variants, vec![1]);
    audit(&input, &policy, options(5), &plan);
    let input = [paragraph(&[(1, 0)]), paragraph(&[(1, 0)])];
    let policies = [ParagraphPolicy { keep_with_next: true, ..ParagraphPolicy::default() }, ParagraphPolicy { break_before: true, ..ParagraphPolicy::default() }];
    assert!(matches!(plan_pagination(&input, &policies, options(5)), Err(PaginationError::NoFeasibleLayout { .. })));
}

#[test]
fn impossible_widows_and_orphans_require_explicit_relaxation() {
    let input = [paragraph(&[(3, 0)])];
    assert!(matches!(plan_pagination(&input, &[], options(2)), Err(PaginationError::NoFeasibleLayout { .. })));
    let opts = PaginationOptions { orphan_penalty: Some(7), widow_penalty: Some(11), ..options(2) };
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.fragments[0].line_end, 1);
    assert_eq!(plan.total_demerits, 208); // 2 pages + 1 unused line + 1 missing orphan
    audit(&input, &[], opts, &plan);
}

#[test]
fn empty_input_full_initial_page_and_zero_cost_ties_do_not_add_empty_pages() {
    assert_eq!(plan_pagination(&[], &[], options(5)).unwrap().page_count, 0);
    let opts = PaginationOptions { initial_used_lines: 5, ..options(5) };
    assert_eq!(plan_pagination(&[], &[], opts).unwrap().page_count, 1);
    let input = [paragraph(&[(2, 0)])];
    let plan = plan_pagination(&input, &[], opts).unwrap();
    audit(&input, &[], opts, &plan);
    let input = [paragraph(&[(1, 0), (1, 0)]), paragraph(&[(1, 0)])];
    let opts = PaginationOptions { page_cost: 0, unused_line_cost: 0, ..options(5) };
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.page_count, 1);
    assert_eq!(plan.variants, vec![0, 0]);
}

#[test]
fn invalid_inputs_and_work_budgets_never_return_partial_plans() {
    assert!(matches!(plan_pagination(&[], &[], options(0)), Err(PaginationError::InvalidOptions(_))));
    assert!(matches!(plan_pagination(&[paragraph(&[])], &[], options(5)), Err(PaginationError::InvalidParagraph { .. })));
    assert!(matches!(plan_pagination(&[paragraph(&[(0, 0)])], &[], options(5)), Err(PaginationError::InvalidParagraph { .. })));
    let input = [paragraph(&[(3, 0)]), paragraph(&[(2, 0)])];
    assert!(matches!(plan_pagination(&input, &[ParagraphPolicy::default()], options(5)), Err(PaginationError::InvalidOptions(_))));
    for limits in [
        PaginationLimits { max_paragraphs: 1, ..PaginationLimits::default() },
        PaginationLimits { max_variants_per_paragraph: 0, ..PaginationLimits::default() },
        PaginationLimits { max_candidate_lines: 4, ..PaginationLimits::default() },
        PaginationLimits { max_page_capacity: 4, ..PaginationLimits::default() },
        PaginationLimits { max_nodes: 1, ..PaginationLimits::default() },
        PaginationLimits { max_transitions: 1, ..PaginationLimits::default() },
    ] {
        let opts = PaginationOptions { limits, ..options(5) };
        assert!(matches!(plan_pagination(&input, &[], opts), Err(PaginationError::BudgetExceeded(_))));
    }
}

#[test]
fn wide_costs_and_last_page_penalty_are_exact() {
    let input = [paragraph(&[(2, i64::MIN)]), paragraph(&[(2, i64::MIN)])];
    let opts = PaginationOptions { page_cost: u64::MAX, unused_line_cost: 0, ..options(5) };
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.total_demerits, -1);
    audit(&input, &[], opts, &plan);
    let input = [paragraph(&[(3, 0), (4, 1)])];
    let opts = PaginationOptions { penalize_last_page: true, ..options(5) };
    let plan = plan_pagination(&input, &[], opts).unwrap();
    assert_eq!(plan.variants, vec![1]);
    audit(&input, &[], opts, &plan);
}

// Independent exhaustive oracle: enumerate every variant and every legal line
// fragment, WITHOUT frontier dominance or backpointers. Small bounded inputs
// keep this exponential reference cheap. Compare objective AND page count.
fn oracle(input: &[ParagraphCandidates], policies: &[ParagraphPolicy], opts: PaginationOptions) -> Option<(i128, usize)> {
    #[allow(clippy::too_many_arguments)]
    fn visit(input: &[ParagraphCandidates], policies: &[ParagraphPolicy], opts: PaginationOptions,
        p: usize, variant: Option<usize>, start: usize, used: usize, pages: usize, cost: i128,
        best: &mut Option<(i128, usize)>,
    ) {
        let page_cost = |used: usize, last: bool| {
            let gap = (opts.page_capacity_lines - used) as i128;
            i128::from(opts.page_cost) + if !last || opts.penalize_last_page {
                gap * gap * i128::from(opts.unused_line_cost)
            } else { 0 }
        };
        if p == input.len() {
            let rank = (cost + if used > 0 { page_cost(used, true) } else { 0 }, pages + usize::from(used > 0));
            if best.is_none_or(|old| rank < old) { *best = Some(rank); }
            return;
        }
        let policy = policies.get(p).copied().unwrap_or_default();
        let keep = p > 0 && policies.get(p - 1).is_some_and(|p| p.keep_with_next);
        if variant.is_none() {
            for v in 0..input[p].variants.len() {
                let cost = cost + i128::from(input[p].variants[v].demerits);
                if !policy.break_before || used == 0 {
                    visit(input, policies, opts, p, Some(v), 0, used, pages, cost, best);
                }
                if used > 0 && !keep {
                    visit(input, policies, opts, p, Some(v), 0, 0, pages + 1, cost + page_cost(used, false), best);
                }
            }
            return;
        }
        let v = variant.unwrap();
        let total = input[p].variants[v].line_count;
        for count in 1..=(total - start).min(opts.page_capacity_lines - used) {
            let split = start + count < total;
            if split && policy.keep_together { continue; }
            let mut penalty = 0;
            let mut allowed = true;
            for (active, minimum, weight) in [(split, opts.orphans, opts.orphan_penalty), (start > 0, opts.widows, opts.widow_penalty)] {
                if active && count < minimum {
                    if let Some(weight) = weight { penalty += (minimum - count) as i128 * i128::from(weight); }
                    else { allowed = false; }
                }
            }
            if !allowed { continue; }
            if split {
                visit(input, policies, opts, p, Some(v), start + count, 0, pages + 1,
                    cost + penalty + page_cost(used + count, false), best);
            } else {
                visit(input, policies, opts, p + 1, None, 0, used + count, pages, cost + penalty, best);
            }
        }
    }
    let mut best = None;
    visit(input, policies, opts, 0, None, 0, opts.initial_used_lines, 0, 0, &mut best);
    best
}

#[test]
fn exhaustive_small_inputs_match_an_unpruned_oracle() {
    for capacity in 2..=5 {
        for first in 1..=5 {
            for second in 1..=5 {
                for flags in 0..8 {
                    let input = [paragraph(&[(first, 0), (first + 1, -1)]), paragraph(&[(second, 2)])];
                    let policies = [ParagraphPolicy { keep_together: flags & 1 != 0, keep_with_next: flags & 2 != 0, break_before: false },
                        ParagraphPolicy { break_before: flags & 4 != 0, ..ParagraphPolicy::default() }];
                    let opts = PaginationOptions { initial_used_lines: capacity - 1, ..options(capacity) };
                    let expected = oracle(&input, &policies, opts);
                    let actual = plan_pagination(&input, &policies, opts);
                    match (expected, actual) {
                        (Some(rank), Ok(plan)) => { assert_eq!((plan.total_demerits, plan.page_count), rank); audit(&input, &policies, opts, &plan); }
                        (None, Err(PaginationError::NoFeasibleLayout { .. })) => {}
                        other => panic!("oracle mismatch for capacity {capacity}, lines {first}/{second}, flags {flags}: {other:?}"),
                    }
                }
            }
        }
    }
}
