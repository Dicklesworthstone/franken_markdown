//! Natural-width fitting through the production paragraph optimizer.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    FORCED_BREAK_PENALTY, FitnessClass, Glue, INF_PENALTY, LayoutUnit, LineBreak,
    ParagraphItem, ParagraphLayoutScratch, Penalty, Protrusion, StyledText, TextBox,
    break_paragraph_into,
};

fn u(value: i32) -> LayoutUnit {
    LayoutUnit::from_milli_points(value)
}

fn word(name: &str, width: i32) -> ParagraphItem {
    ParagraphItem::Box(TextBox {
        text: name.to_owned(),
        runs: StyledText::plain(name),
        width: u(width),
        protrusion: Protrusion::default(),
    })
}

fn glue(width: i32, stretch: i32, shrink: i32) -> ParagraphItem {
    ParagraphItem::Glue(Glue { width: u(width), stretch: u(stretch), shrink: u(shrink) })
}

fn forced() -> ParagraphItem {
    ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    })
}

fn pressure() -> Vec<ParagraphItem> {
    vec![
        word("alpha", 60_000), glue(6_000, 3_000, 2_000),
        word("beta", 35_000), glue(6_000, 3_000, 2_000),
        word("gamma", 40_000), forced(),
    ]
}

fn run(items: &[ParagraphItem], width: i32, justified: bool, expansion: u16) -> Vec<LineBreak> {
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_justified(justified);
    scratch.set_expansion_permilli(expansion);
    let mut lines = Vec::new();
    break_paragraph_into(items, u(width), &mut scratch, &mut lines);
    lines
}

fn content(items: &[ParagraphItem], lines: &[LineBreak]) -> Vec<Vec<String>> {
    lines.iter().map(|line| {
        items[line.start..line.end].iter().filter_map(|item| match item {
            ParagraphItem::Box(item) => Some(item.text.clone()),
            _ => None,
        }).collect()
    }).collect()
}

fn assert_partition(items: &[ParagraphItem], lines: &[LineBreak]) {
    assert!(!lines.is_empty());
    assert_eq!(lines[0].start, 0);
    assert_eq!(lines.last().unwrap().next, items.len());
    for pair in lines.windows(2) {
        assert_eq!(pair[0].next, pair[1].start);
    }
    for line in lines {
        assert!(line.start <= line.end && line.end < line.next && line.next <= items.len());
        let content_width: i32 = items[line.start..line.end].iter().map(|item| match item {
            ParagraphItem::Box(item) => item.width.milli_points(),
            ParagraphItem::Glue(item) => item.width.milli_points(),
            ParagraphItem::Penalty(_) => 0,
        }).sum();
        let penalty = match items.get(line.end) {
            Some(ParagraphItem::Penalty(item)) => item.width.milli_points(),
            _ => 0,
        };
        assert_eq!(line.natural_width, u(content_width + penalty));
    }
    let expected: Vec<_> = items.iter().filter_map(|item| match item {
        ParagraphItem::Box(item) => Some(item.text.clone()), _ => None,
    }).collect();
    assert_eq!(content(items, lines).into_iter().flatten().collect::<Vec<_>>(), expected);
}

#[test]
fn nonfinal_ragged_line_does_not_borrow_space_compression() {
    let items = pressure();
    let lines = run(&items, 100_000, false, 0);
    assert_eq!(content(&items, &lines), vec![vec!["alpha".to_owned()], vec!["beta".to_owned(), "gamma".to_owned()]]);
    assert!(lines.iter().all(|line| line.natural_width <= u(100_000)));
    assert_partition(&items, &lines);
}

#[test]
fn justified_control_keeps_real_inner_line_compression() {
    let items = pressure();
    let lines = run(&items, 100_000, true, 0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].natural_width, u(101_000));
    assert!(lines[0].badness < INF_PENALTY);
    assert_eq!(content(&items, &lines)[0], ["alpha", "beta"]);
    assert_partition(&items, &lines);
}

#[test]
fn ragged_fit_ignores_configured_glyph_elasticity_without_destroying_it() {
    let items = pressure();
    let reference = run(&items, 100_000, false, 0);
    for expansion in [1, 15, 100, u16::MAX] {
        assert_eq!(run(&items, 100_000, false, expansion), reference);
    }
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_expansion_permilli(15);
    scratch.set_justified(false);
    assert_eq!(scratch.expansion_permilli(), 15);
    assert!(!scratch.is_justified());
}

#[test]
fn short_ragged_lines_have_finite_natural_width_quality() {
    let items = vec![word("one", 50_000), glue(4_000, 0, 0), word("two", 50_000), forced()];
    let lines = run(&items, 100_000, false, 0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].badness, 12); // 100 * (1/2)^3, integer truncation
    assert_eq!(lines[0].fitness, FitnessClass::Decent);
    assert_eq!(lines[0].fitness_milli, 0);
    assert!(lines.last().unwrap().demerits < 1_000);
}

#[test]
fn unpainted_glue_stretch_cannot_change_ragged_breaks_or_scores() {
    let mut items = pressure();
    let reference = run(&items, 100_000, false, 15);
    for item in &mut items {
        if let ParagraphItem::Glue(glue) = item {
            glue.stretch = u(i32::MAX);
            glue.shrink = glue.width;
        }
    }
    assert_eq!(run(&items, 100_000, false, 15), reference);
}

#[test]
fn every_optimizer_mode_honors_the_ragged_paint_contract() {
    let items = pressure();
    for flags in 0..8 {
        let mut scratch = ParagraphLayoutScratch::new();
        scratch.set_justified(false);
        scratch.set_expansion_permilli(100);
        scratch.set_gradual_demerits(flags & 1 != 0);
        scratch.set_river_penalty(flags & 2 != 0);
        scratch.set_pareto_breaking(flags & 4 != 0);
        let mut lines = Vec::new();
        break_paragraph_into(&items, u(100_000), &mut scratch, &mut lines);
        assert_partition(&items, &lines);
        assert!(lines.iter().all(|line| line.natural_width <= u(100_000)), "mode {flags}");
        assert!(lines.iter().all(|line| line.fitness_milli == 0));
    }
}

#[test]
fn scratch_can_alternate_ragged_and_justified_blocks_in_one_document() {
    let items = pressure();
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_expansion_permilli(15);
    let mut lines = Vec::new();
    for justified in [false, true, false, true] {
        scratch.set_justified(justified);
        break_paragraph_into(&items, u(100_000), &mut scratch, &mut lines);
        assert_eq!(lines, run(&items, 100_000, justified, 15));
        scratch.clear();
        assert_eq!(scratch.is_justified(), justified);
        assert_eq!(scratch.expansion_permilli(), 15);
        break_paragraph_into(&[], u(100_000), &mut scratch, &mut lines);
        assert!(lines.is_empty());
    }
}

#[test]
fn hard_breaks_and_terminal_lines_remain_naturally_fitted() {
    let mut items = pressure();
    let first_end = items.len() - 1;
    items.extend(pressure());
    for justified in [false, true] {
        let lines = run(&items, 100_000, justified, 15);
        assert_partition(&items, &lines);
        assert!(lines.iter().any(|line| line.end == first_end));
        assert!(lines.iter().all(|line| !(line.start < first_end && line.end > first_end)));
        for line in &lines {
            if matches!(items.get(line.end), Some(ParagraphItem::Penalty(p)) if p.penalty == FORCED_BREAK_PENALTY) {
                assert!(line.natural_width <= u(100_000));
            }
        }
    }
}

#[test]
fn indivisible_content_is_preserved_as_explicit_overflow() {
    let items = vec![word("unbreakable", 150_000), forced()];
    let lines = run(&items, 100_000, false, 100);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].natural_width, u(150_000));
    assert_eq!(lines[0].badness, INF_PENALTY);
    assert_partition(&items, &lines);
}

#[test]
fn deterministic_varied_ragged_paragraphs_preserve_content_and_fit() {
    let mut state = 0x5eed_u64;
    for case in 0..512 {
        let mut items = Vec::new();
        for index in 0..8 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let width = 10_000 + ((state >> 32) % 80_001) as i32;
            if index > 0 { items.push(glue(4_000, 2_000, 1_333)); }
            items.push(word(&format!("w{index}"), width));
        }
        items.push(forced());
        let lines = run(&items, 100_000, false, 15);
        assert_partition(&items, &lines);
        assert!(lines.iter().all(|line| line.natural_width <= u(100_000)), "case {case}");
    }
}
