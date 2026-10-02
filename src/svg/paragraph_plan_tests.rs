//! The oracle enumerates every source-word partition, not the shared DP.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::single_range_in_vec_init
)]

use super::*;

fn group(width: f64, gap: f64) -> Group {
    Group {
        runs: vec![Word {
            text: "word".to_owned(),
            style: RStyle::BODY,
            w: width,
            gap: 0.0,
            formula: None,
            image: None,
            warning: None,
            shaped: None,
        }],
        gap,
    }
}

fn used(groups: &[Group], range: Range<usize>) -> i64 {
    let mut width = 0;
    for (offset, item) in groups[range].iter().enumerate() {
        if offset != 0 {
            width += i64::from(upper_units(item.gap).unwrap().milli_points());
        }
        let natural: f64 = item.runs.iter().map(|run| run.w).sum();
        width += i64::from(upper_units(natural).unwrap().milli_points());
    }
    width
}

fn line_cost(natural: i64, measure: i64, last: bool) -> i64 {
    assert!(natural <= measure);
    let shortfall = measure - natural;
    let ratio = shortfall * 1000 / measure;
    let badness = if last {
        0
    } else {
        100 * ratio * ratio * ratio / 1_000_000_000
    };
    (badness + 1).pow(2)
}

fn score(groups: &[Group], lines: &[Range<usize>], measure: i64) -> i64 {
    let mut cursor = 0;
    let mut result = 0;
    for line in lines {
        assert_eq!(line.start, cursor);
        assert!(line.end > line.start && line.end <= groups.len());
        result += line_cost(
            used(groups, line.clone()),
            measure,
            line.end == groups.len(),
        );
        cursor = line.end;
    }
    assert_eq!(cursor, groups.len());
    result
}

fn exhaustive(groups: &[Group], measure: i64, start: usize) -> i64 {
    if start == groups.len() {
        return 0;
    }
    let mut best = i64::MAX;
    for end in start + 1..=groups.len() {
        let width = used(groups, start..end);
        if width <= measure {
            let rest = exhaustive(groups, measure, end);
            best = best.min(line_cost(width, measure, end == groups.len()) + rest);
        }
    }
    best
}

#[test]
fn moves_a_word_back_to_avoid_the_greedy_short_middle_line() {
    let groups = [
        group(30.0, 0.0),
        group(20.0, 10.0),
        group(20.0, 10.0),
        group(50.0, 10.0),
    ];
    let lines = plan(&groups, 60.0).unwrap();
    assert_eq!(lines, [0..1, 1..3, 3..4]);
    assert_eq!(score(&groups, &lines, 60_000), 171);
    assert_eq!(score(&groups, &[0..2, 2..3, 3..4], 60_000), 902);
}

#[test]
fn shared_ragged_plans_match_an_independent_exhaustive_oracle() {
    let mut state = 0x3ca8_f011_u32;
    for length in 1..=8 {
        for _ in 0..64 {
            let mut groups = Vec::new();
            for _ in 0..length {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let width = 1.0 + f64::from(state % 85);
                let gap = f64::from((state >> 8) % 9);
                groups.push(group(width, gap));
            }
            let lines =
                plan(&groups, 100.0).expect("individually fitting words have a feasible partition");
            assert_eq!(
                score(&groups, &lines, 100_000),
                exhaustive(&groups, 100_000, 0)
            );
        }
    }
}

#[test]
fn decisions_round_boxes_outward_and_measure_inward_without_changing_runs() {
    assert_eq!(upper_units(0.000_001).unwrap().milli_points(), 1);
    assert_eq!(upper_units(1.234_001).unwrap().milli_points(), 1235);
    let groups = [group(10.000_1, 999.0), group(10.000_1, 0.0)];
    // The first gap is not rendered and does not consume the measure.
    assert_eq!(plan(&groups, 20.000_3).unwrap(), [0..1, 1..2]);
    assert_eq!(plan(&groups, 20.002).unwrap(), [0..2]);
    assert_eq!(groups[0].runs[0].w, 10.000_1);
    assert_eq!(groups[0].gap, 999.0);
}

#[test]
fn fitting_never_borrows_space_shrink_or_glyph_contraction() {
    let groups = [group(48.0, 0.0), group(48.0, 5.0)];
    let lines = plan(&groups, 100.0).unwrap();
    assert_eq!(lines, [0..1, 1..2]);
    for line in lines {
        assert!(used(&groups, line) <= 100_000);
    }
}

#[test]
fn adjacent_styled_runs_are_one_indivisible_box() {
    let mut styled = group(12.0, 2.0);
    let mut bold = group(13.0, 0.0).runs.pop().unwrap();
    bold.style.bold = true;
    styled.runs.push(bold);
    let groups = [group(10.0, 0.0), styled, group(15.0, 2.0)];
    let lines = plan(&groups, 30.0).unwrap();
    assert_eq!(lines, [0..1, 1..2, 2..3]);
    assert_eq!(groups[1].runs.len(), 2);
}

#[test]
fn overwide_groups_return_to_emergency_wrapping_not_a_partial_plan() {
    let groups = [group(10.0, 0.0), group(100.1, 3.0), group(10.0, 3.0)];
    assert!(plan(&groups, 100.0).is_none());
    assert_eq!(groups.iter().map(|g| g.runs.len()).sum::<usize>(), 3);
}

#[test]
fn invalid_geometry_and_excess_candidates_are_not_cast_or_partially_planned() {
    for width in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.000_1, 9e12] {
        assert!(plan(&[group(1.0, 0.0)], width).is_none());
    }
    for width in [f64::NAN, f64::INFINITY, -1.0, 9e12] {
        assert!(plan(&[group(width, 0.0)], 100.0).is_none());
    }
    for gap in [f64::NAN, f64::INFINITY, -1.0, -0.000_001, 9e12] {
        assert!(plan(&[group(1.0, 0.0), group(1.0, gap)], 100.0).is_none());
    }
    let many: Vec<_> = (0..=MAX_WORDS).map(|_| group(1.0, 1.0)).collect();
    assert!(plan(&many, 100.0).is_none());
    assert!(
        plan(
            &[Group {
                runs: Vec::new(),
                gap: 0.0
            }],
            100.0
        )
        .is_none()
    );
}

#[test]
fn empty_and_zero_advance_inputs_have_complete_deterministic_plans() {
    assert!(plan(&[], 100.0).unwrap().is_empty());
    let groups = [group(0.0, 7.0), group(0.0, 0.0), group(0.0, 0.0)];
    assert_eq!(plan(&groups, 100.0).unwrap(), [0..3]);
    assert_eq!(plan(&groups, 100.0), plan(&groups, 100.0));
}

#[test]
fn optimizer_limit_does_not_replace_an_existing_resource_warning() {
    let mut g = group(12.0, 2.0);
    g.runs[0].warning = Some(SvgWarning {
        code: "original",
        message: "original".into(),
    });
    limit_warning(&mut g);
    assert_eq!(g.runs[0].warning.as_ref().unwrap().code, "original");
    assert_eq!(
        g.runs[1].warning.as_ref().unwrap().code,
        "svg_paragraph_limit"
    );
    assert_eq!(g.runs[1].w, 0.0);
    assert_eq!(g.runs[1].gap, 0.0);
    assert!(g.runs[1].text.is_empty());
}
