#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn options(capacity: i64) -> Options {
    Options {
        capacity,
        page_cost: 50_000,
        unused_height_cost: 10_000,
        limits: Limits {
            max_blocks: 100,
            max_fragments: 1_000,
            max_capacity: 100_000,
            max_nodes: 100_000,
            max_work: 1_000_000,
            max_retained_ranges: 1_000_000,
        },
    }
}

fn input(heights: &[i64], notes: &[(usize, &[i64])]) -> Input {
    Input {
        rows: heights
            .iter()
            .enumerate()
            .map(|(block, &height)| Row {
                height,
                continuation_prefix: 0,
                block,
            })
            .collect(),
        blocks: (0..heights.len())
            .map(|index| Block {
                range: index..index + 1,
                min_before_break: 1,
                min_after_break: 1,
            })
            .collect(),
        break_costs: vec![Some(0); heights.len() + 1],
        forced: vec![false; heights.len() + 1],
        notes: notes
            .iter()
            .map(|&(first_reference, heights)| Note {
                first_reference,
                units: heights
                    .iter()
                    .map(|&height| Unit {
                        height,
                        prefix: 0,
                        suffix: 0,
                    })
                    .collect(),
            })
            .collect(),
        separator: 5,
    }
}

/// Independent small-instance oracle: enumerate complete body partitions,
/// then account for note progress using one counter per source note. It does
/// not use the production frontier, state key, cumulative heights, legality
/// helper, allocator, cost helper, dominance rule, or backpointer walk.
fn fragment_height(input: &Input, note: usize, start: usize, end: usize) -> i64 {
    let units = &input.notes[note].units;
    units[start..end]
        .iter()
        .map(|unit| unit.height)
        .sum::<i64>()
        + units[start].prefix
        + units[end - 1].suffix
}

fn oracle_page(
    input: &Input,
    options: Options,
    body: Range<usize>,
    progress: &mut [usize],
) -> Option<Page> {
    let body_height = if body.is_empty() {
        0
    } else {
        if input.break_costs[body.end].is_none()
            || input.forced[body.start + 1..body.end]
                .iter()
                .any(|&forced| forced)
        {
            return None;
        }
        for block in &input.blocks {
            let first = body.start.max(block.range.start);
            let last = body.end.min(block.range.end);
            if first >= last {
                continue;
            }
            let count = last - first;
            if (last < block.range.end && count < block.min_before_break)
                || (first > block.range.start && count < block.min_after_break)
            {
                return None;
            }
        }
        input.rows[body.clone()]
            .iter()
            .map(|row| row.height)
            .sum::<i64>()
            + input.rows[body.start].continuation_prefix
    };
    if body_height > options.capacity {
        return None;
    }
    let budget = if body.is_empty() {
        options.capacity
    } else {
        (options.capacity * 2 / 5).min(options.capacity - body_height)
    };
    let available = (budget - input.separator).max(0);
    let mut ranges = vec![None; input.notes.len()];
    let mut used = 0;

    // First-citation fragments are mandatory even above the preferred share.
    for (note, source) in input.notes.iter().enumerate() {
        if body.contains(&source.first_reference) {
            assert_eq!(progress[note], 0);
            ranges[note] = Some((0, 1));
            progress[note] = 1;
            used += fragment_height(input, note, 0, 1);
        }
    }

    // Older notes grow in source citation order. A partially consumed old
    // note prevents a younger old note from overtaking it on this page.
    for (note, source) in input.notes.iter().enumerate() {
        if source.first_reference >= body.start || progress[note] == source.units.len() {
            continue;
        }
        let first = progress[note];
        let mut last = first;
        for candidate in first + 1..=source.units.len() {
            if fragment_height(input, note, first, candidate) > (available - used).max(0) {
                break;
            }
            last = candidate;
        }
        if first == last {
            break;
        }
        ranges[note] = Some((first, last));
        progress[note] = last;
        used += fragment_height(input, note, first, last);
        if last != source.units.len() {
            break;
        }
    }

    // Grow new fragments only after older continuations used their share.
    for (note, source) in input.notes.iter().enumerate() {
        if !body.contains(&source.first_reference) {
            continue;
        }
        let before = fragment_height(input, note, 0, 1);
        let limit = before + (available - used).max(0);
        let mut last = 1;
        for candidate in 2..=source.units.len() {
            if fragment_height(input, note, 0, candidate) > limit {
                break;
            }
            last = candidate;
        }
        ranges[note] = Some((0, last));
        progress[note] = last;
        used += fragment_height(input, note, 0, last) - before;
    }
    let fragments: Vec<_> = ranges
        .into_iter()
        .enumerate()
        .filter_map(|(note, range)| range.map(|(start, end)| Fragment { note, start, end }))
        .collect();
    if body.is_empty() && fragments.is_empty() {
        return None;
    }
    let reserve = if fragments.is_empty() {
        0
    } else {
        input.separator + used
    };
    (body_height + reserve <= options.capacity).then_some(Page {
        body,
        fragments,
        reserve,
    })
}

fn oracle_partition(input: &Input, options: Options, ends: &[usize]) -> Option<Vec<Page>> {
    let mut pages = Vec::new();
    let mut progress = vec![0; input.notes.len()];
    let mut start = 0;
    for &end in ends {
        pages.push(oracle_page(input, options, start..end, &mut progress)?);
        start = end;
    }
    assert_eq!(start, input.rows.len());
    while input
        .notes
        .iter()
        .zip(&progress)
        .any(|(note, &next)| next < note.units.len())
    {
        pages.push(oracle_page(input, options, start..start, &mut progress)?);
    }
    Some(pages)
}

fn oracle_cost(input: &Input, options: Options, pages: &[Page]) -> i128 {
    let capacity = i128::from(options.capacity);
    pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            let body_height = if page.body.is_empty() {
                0
            } else {
                input.rows[page.body.clone()]
                    .iter()
                    .map(|row| row.height)
                    .sum::<i64>()
                    + input.rows[page.body.start].continuation_prefix
            };
            let gap = capacity - i128::from(body_height + page.reserve);
            let ragged = if index + 1 == pages.len() {
                0
            } else {
                gap * gap * i128::from(options.unused_height_cost) / (capacity * capacity)
            };
            let boundary = if page.body.is_empty() {
                0
            } else {
                input.break_costs[page.body.end].unwrap()
            };
            i128::from(options.page_cost) + ragged + i128::from(boundary)
        })
        .sum()
}

fn oracle(input: &Input, options: Options) -> Option<(i128, usize)> {
    assert!(
        input.rows.len() <= 8,
        "oracle enumerates only small instances"
    );
    if input.rows.is_empty() {
        return Some((0, 0));
    }
    let mut best = None;
    for cuts in 0..(1_usize << (input.rows.len() - 1)) {
        let ends: Vec<_> = (1..=input.rows.len())
            .filter(|&end| end == input.rows.len() || cuts & (1 << (end - 1)) != 0)
            .collect();
        if let Some(pages) = oracle_partition(input, options, &ends) {
            let rank = (oracle_cost(input, options, &pages), pages.len());
            if best.is_none_or(|previous| rank < previous) {
                best = Some(rank);
            }
        }
    }
    best
}

fn assert_oracle(input: &Input, options: Options) -> Option<Plan> {
    let expected = oracle(input, options);
    let result = plan(input, options);
    match expected {
        None => {
            assert!(
                matches!(result, Err(Error::Infeasible { .. })),
                "{result:?}"
            );
            None
        }
        Some(expected) => {
            let result = result.expect("oracle found a feasible complete publication");
            assert_eq!((result.total_demerits, result.pages.len()), expected);
            let ends: Vec<_> = result
                .pages
                .iter()
                .filter(|page| !page.body.is_empty())
                .map(|page| page.body.end)
                .collect();
            assert_eq!(
                result.pages,
                oracle_partition(input, options, &ends).unwrap(),
                "the retained plan must replay its exact body and note allocation"
            );
            let mut seen = vec![0; input.notes.len()];
            for page in &result.pages {
                for fragment in &page.fragments {
                    assert_eq!(fragment.start, seen[fragment.note]);
                    assert!(fragment.start < fragment.end);
                    seen[fragment.note] = fragment.end;
                }
                for (note, source) in input.notes.iter().enumerate() {
                    if page.body.contains(&source.first_reference) {
                        assert!(
                            page.fragments
                                .iter()
                                .any(|fragment| fragment.note == note && fragment.start == 0),
                            "each citation page must retain its first note fragment"
                        );
                    }
                }
            }
            for (note, seen) in input.notes.iter().zip(seen) {
                assert_eq!(seen, note.units.len(), "all note units appear exactly once");
            }
            if !input.rows.is_empty() {
                assert!(result.search_nodes > 0 && result.search_work > 0);
            }
            Some(result)
        }
    }
}

#[test]
fn exhaustive_partitions_match_the_global_body_boundary_minimum() {
    for count in 2..=8 {
        for seed in 0..12 {
            let heights: Vec<_> = (0..count)
                .map(|index| 9 + ((seed * 7 + index * 11) % 19) as i64)
                .collect();
            let mut input = input(&heights, &[]);
            for (note, reference) in [0, count / 2, count - 1].into_iter().enumerate() {
                input.notes.push(Note {
                    first_reference: reference,
                    units: (0..2 + (seed + note) % 5)
                        .map(|unit| Unit {
                            height: 3 + ((seed + unit * 3 + note) % 7) as i64,
                            prefix: ((unit + note) % 3) as i64,
                            suffix: ((seed + unit) % 2) as i64,
                        })
                        .collect(),
                });
            }
            for boundary in 1..count {
                input.break_costs[boundary] =
                    Some(((boundary * 31 + seed * 17) % 9) as i64 * 1_000 - 2_000);
            }
            if seed % 3 == 0 {
                input.forced[count / 2] = true;
            }
            if seed % 4 == 0 && count > 3 {
                input.break_costs[1] = None;
            }
            let options = options(45 + (seed * 5 % 31) as i64);
            assert_oracle(&input, options);
        }
    }
}

#[test]
fn continuation_minimum_counts_only_its_own_block_fragment() {
    let mut input = input(&[10, 10, 10, 10, 10], &[(0, &[3, 3, 3])]);
    input.separator = 2;
    input.blocks = vec![
        Block {
            range: 0..4,
            min_before_break: 2,
            min_after_break: 2,
        },
        Block {
            range: 4..5,
            min_before_break: 1,
            min_after_break: 1,
        },
    ];
    for row in &mut input.rows[..4] {
        row.block = 0;
    }
    input.rows[4].block = 1;
    let result = assert_oracle(&input, options(35)).unwrap();
    assert_eq!(
        result
            .pages
            .iter()
            .map(|page| page.body.clone())
            .collect::<Vec<_>>(),
        [0..2, 2..5]
    );
    assert_eq!(result.total_demerits, 100_130);
}

#[test]
fn forced_boundaries_and_indivisible_keeps_survive_note_reservations() {
    let mut input = input(&[10; 6], &[(0, &[5; 6])]);
    input.break_costs[2] = None;
    input.forced[3] = true;
    let result = assert_oracle(&input, options(50)).unwrap();
    assert_eq!(
        result
            .pages
            .iter()
            .map(|page| page.body.clone())
            .collect::<Vec<_>>(),
        [0..3, 3..6]
    );
}

#[test]
fn body_and_note_continuation_prefixes_are_charged_on_every_physical_page() {
    let mut input = input(&[20, 20], &[(0, &[10; 12])]);
    input.forced[1] = true;
    input.rows[1].continuation_prefix = 7;
    input.separator = 4;
    for unit in &mut input.notes[0].units {
        unit.prefix = 3;
        unit.suffix = 2;
    }
    let result = assert_oracle(&input, options(60)).unwrap();
    assert_eq!(result.pages.len(), 4);
    assert_eq!(result.pages[0].reserve, 19);
    assert_eq!(result.pages[1].reserve, 19);
    assert_eq!(result.pages[2].reserve, 59);
    assert_eq!(result.pages[3].reserve, 59);
}

#[test]
fn extending_a_note_replaces_its_last_suffix_without_repeating_internal_prefixes() {
    let mut input = input(&[10], &[(0, &[10, 10, 10])]);
    input.notes[0].units[0].prefix = 2;
    input.notes[0].units[0].suffix = 20;
    input.notes[0].units[1].prefix = 10;
    input.notes[0].units[1].suffix = 1;
    input.notes[0].units[2].prefix = 4;
    input.notes[0].units[2].suffix = 1;
    let result = assert_oracle(&input, options(100)).unwrap();
    assert_eq!(result.pages.len(), 1);
    assert_eq!(result.pages[0].reserve, 38);
    assert_eq!(
        result.pages[0].fragments,
        [Fragment {
            note: 0,
            start: 0,
            end: 3
        }]
    );
}

#[test]
fn simultaneous_first_fragments_can_exceed_the_preferred_share() {
    let input = input(&[15], &[(0, &[30, 20]), (0, &[25, 20])]);
    let result = assert_oracle(&input, options(100)).unwrap();
    assert_eq!(result.pages.len(), 2);
    assert_eq!(result.pages[0].reserve, 60);
    assert_eq!(
        result.pages[0].fragments,
        [
            Fragment {
                note: 0,
                start: 0,
                end: 1
            },
            Fragment {
                note: 1,
                start: 0,
                end: 1
            },
        ]
    );
}

#[test]
fn a_new_citation_reserves_its_first_unit_before_older_pending_work() {
    let mut input = input(&[20, 20], &[(0, &[10; 10]), (1, &[35, 5])]);
    input.forced[1] = true;
    let result = assert_oracle(&input, options(100)).unwrap();
    assert_eq!(result.pages.len(), 3);
    assert_eq!(
        result.pages[1].fragments,
        [Fragment {
            note: 1,
            start: 0,
            end: 1
        }]
    );
    assert_eq!(
        result.pages[2].fragments,
        [
            Fragment {
                note: 0,
                start: 3,
                end: 10
            },
            Fragment {
                note: 1,
                start: 1,
                end: 2
            },
        ]
    );
}

#[test]
fn only_the_last_physical_page_receives_the_last_page_cost_exemption() {
    let input = input(&[10], &[(0, &[10; 8])]);
    let result = assert_oracle(&input, options(50)).unwrap();
    assert_eq!(
        result
            .pages
            .iter()
            .map(|page| page.body.clone())
            .collect::<Vec<_>>(),
        [0..1, 1..1, 1..1]
    );
    assert_eq!(result.total_demerits, 152_600);
}

#[test]
fn an_impossible_tail_does_not_discard_a_feasible_alternative_body_partition() {
    let mut input = input(&[20, 15], &[(0, &[10, 5])]);
    input.notes[0].units[1].prefix = 100;
    let result = assert_oracle(&input, options(50)).unwrap();
    assert_eq!(
        result
            .pages
            .iter()
            .map(|page| page.body.clone())
            .collect::<Vec<_>>(),
        [0..1, 1..2]
    );
    assert_eq!(result.total_demerits, 100_400);
}

#[test]
fn impossible_widow_orphan_and_mandatory_note_requests_return_infeasible() {
    let mut paragraph = input(&[10, 10, 10], &[(0, &[2])]);
    paragraph.separator = 1;
    paragraph.blocks = vec![Block {
        range: 0..3,
        min_before_break: 2,
        min_after_break: 2,
    }];
    for row in &mut paragraph.rows {
        row.block = 0;
    }
    assert!(assert_oracle(&paragraph, options(25)).is_none());
    let notes = input(&[10], &[(0, &[20]), (0, &[20])]);
    assert!(assert_oracle(&notes, options(50)).is_none());
}

#[test]
fn empty_input_has_an_empty_zero_cost_plan() {
    let result = assert_oracle(&input(&[], &[]), options(100)).unwrap();
    assert!(result.pages.is_empty());
    assert_eq!(result.total_demerits, 0);
}

#[test]
fn work_nodes_fragments_blocks_and_retained_ranges_have_typed_budget_failures() {
    let input = input(&[10, 10], &[(0, &[5, 5, 5])]);
    for (label, limits) in [
        (
            "blocks",
            Limits {
                max_blocks: 0,
                ..options(50).limits
            },
        ),
        (
            "fragments",
            Limits {
                max_fragments: 4,
                ..options(50).limits
            },
        ),
        (
            "nodes",
            Limits {
                max_nodes: 0,
                ..options(50).limits
            },
        ),
        (
            "work",
            Limits {
                max_work: 0,
                ..options(50).limits
            },
        ),
        (
            "ranges",
            Limits {
                max_retained_ranges: 0,
                ..options(50).limits
            },
        ),
        (
            "capacity",
            Limits {
                max_capacity: 49,
                ..options(50).limits
            },
        ),
    ] {
        let error = plan(
            &input,
            Options {
                limits,
                ..options(50)
            },
        )
        .unwrap_err();
        assert!(matches!(error, Error::Budget(_)), "{label}: {error:?}");
    }
}

#[test]
fn malformed_partitions_heights_and_citation_order_are_rejected() {
    let original = input(&[10, 10, 10], &[(0, &[5]), (2, &[5])]);
    for case in 0..8 {
        let mut bad = original.clone();
        match case {
            0 => {
                bad.break_costs.pop();
            }
            1 => {
                bad.forced.pop();
            }
            2 => bad.blocks[1].range.start = 0,
            3 => bad.rows[0].height = 0,
            4 => bad.rows[1].continuation_prefix = -1,
            5 => bad.notes[0].units[0].suffix = -1,
            6 => bad.notes.swap(0, 1),
            _ => bad.separator = -1,
        }
        assert!(
            matches!(plan(&bad, options(50)), Err(Error::Invalid(_))),
            "case {case}"
        );
    }
}

#[test]
fn cost_overflow_is_an_error_instead_of_a_wrapped_minimum() {
    let mut input = input(&[1, 1], &[(0, &[1])]);
    input.forced[1] = true;
    input.separator = 0;
    let options = Options {
        capacity: i64::MAX - 10,
        unused_height_cost: u64::MAX,
        limits: Limits {
            max_capacity: i64::MAX,
            ..options(50).limits
        },
        ..options(50)
    };
    assert_eq!(plan(&input, options).unwrap_err(), Error::CostOverflow);
}

#[test]
fn failed_candidates_consume_the_shared_search_budget() {
    // No body partition can place this mandatory first note unit, but finding
    // that out costs work. A caller trying another paragraph shape must charge
    // this failed attempt rather than resetting its document allowance.
    let impossible = input(&[8, 8, 8], &[(0, &[50])]);
    let (result, usage) = plan_with_usage(&impossible, options(20));
    assert!(matches!(result, Err(Error::Infeasible { .. })));
    assert!(usage.search_work > 1);

    let remaining = usage.search_work - 1;
    let mut bounded = options(20);
    bounded.limits.max_work = remaining;
    let (result, refused) = plan_with_usage(&impossible, bounded);
    assert_eq!(result.unwrap_err(), Error::Budget("continuation work"));
    assert!(refused.search_work > remaining);

    // Successful candidates expose the same complete plan and cost together
    // with their live-range high-water mark, including pending note tails.
    let feasible = input(&[8, 8, 8], &[(0, &[5, 5, 5, 5])]);
    let (result, usage) = plan_with_usage(&feasible, options(20));
    let result = result.unwrap();
    let ordinary = plan(&feasible, options(20)).unwrap();
    assert_eq!(result.pages, ordinary.pages);
    assert_eq!(result.total_demerits, ordinary.total_demerits);
    assert_eq!(usage.search_nodes, result.search_nodes);
    assert_eq!(usage.search_work, result.search_work);
    assert!(usage.retained_ranges > 0);
}
