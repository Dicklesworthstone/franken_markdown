//! Regression coverage for final/forced lines that are painted without glue
//! compression. The public paragraph APIs exercise the actual production DP.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    FORCED_BREAK_PENALTY, FontSize, Glue, INF_PENALTY, LayoutUnit, LineBreak,
    ParagraphItem, ParagraphLayoutScratch, Penalty, Protrusion, StyledText, TextBox,
    break_paragraph_candidates, break_paragraph_into, paragraph_items_from_text,
};

fn units(value: i32) -> LayoutUnit { LayoutUnit::from_milli_points(value) }

fn word(text: &str, width: i32) -> ParagraphItem {
    ParagraphItem::Box(TextBox {
        text: text.to_owned(), runs: StyledText::plain(text),
        width: units(width), protrusion: Protrusion::default(),
    })
}

fn glue(width: i32, stretch: i32, shrink: i32) -> ParagraphItem {
    ParagraphItem::Glue(Glue { width: units(width), stretch: units(stretch), shrink: units(shrink) })
}

fn forced(width: i32) -> ParagraphItem {
    ParagraphItem::Penalty(Penalty { width: units(width), penalty: FORCED_BREAK_PENALTY, flagged: false })
}

fn pressure() -> Vec<ParagraphItem> {
    // Natural 101pt, nominally compressible to 99pt. Terminal painting does
    // not perform that compression, so a one-line layout would overhang.
    vec![word("alpha", 60_000), glue(6_000, 3_000, 2_000), word("beta", 35_000), forced(0)]
}

fn layout(items: &[ParagraphItem], width: i32, expansion: u16) -> Vec<LineBreak> {
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_expansion_permilli(expansion);
    let mut lines = Vec::new();
    break_paragraph_into(items, units(width), &mut scratch, &mut lines);
    lines
}

fn texts(items: &[ParagraphItem], lines: &[LineBreak]) -> Vec<String> {
    lines.iter().map(|line| items[line.start..line.end].iter().filter_map(|item| {
        match item { ParagraphItem::Box(item) => Some(item.text.as_str()), _ => None }
    }).collect()).collect()
}

#[test]
fn final_line_must_not_borrow_interword_shrink() {
    let items = pressure();
    for expansion in [0, 15, 100] {
        let lines = layout(&items, 100_000, expansion);
        assert_eq!(texts(&items, &lines), ["alpha", "beta"]);
        assert!(lines.iter().all(|line| line.natural_width <= units(100_000)));
        assert_eq!(lines.last().unwrap().next, items.len());
    }
}

#[test]
fn interior_hard_break_must_not_borrow_glue_or_glyph_shrink() {
    let mut items = pressure();
    items.extend([word("after", 10_000), forced(0)]);
    for expansion in [0, 15, 100] {
        let lines = layout(&items, 100_000, expansion);
        assert_eq!(texts(&items, &lines), ["alpha", "beta", "after"]);
        assert!(lines.iter().all(|line| line.natural_width <= units(100_000)));
        assert!(lines.iter().any(|line| line.end == 3));
    }
}

#[test]
fn forced_lines_after_a_prefix_are_not_misclassified_as_adjustable() {
    let mut items = vec![word("lead", 10_000), forced(0)];
    items.extend(pressure());
    items.extend([word("after", 10_000), forced(0)]);
    let lines = layout(&items, 100_000, 15);
    assert_eq!(texts(&items, &lines), ["lead", "alpha", "beta", "after"]);
    assert!(lines.iter().all(|line| line.natural_width <= units(100_000)));
}

#[test]
fn final_penalty_width_is_part_of_the_unadjusted_measure() {
    let items = vec![word("alpha", 58_000), glue(6_000, 3_000, 2_000), word("beta", 35_000), forced(2_000)];
    let lines = layout(&items, 100_000, 0);
    assert_eq!(texts(&items, &lines), ["alpha", "beta"]);
    assert_eq!(lines.last().unwrap().natural_width, units(37_000));
}

#[test]
fn adjusted_inner_lines_keep_their_existing_shrink_budget() {
    // The 101pt inner line is followed by a zero-content terminal line, so
    // the first line really is eligible for glyph contraction. This pins the
    // distinction instead of disabling shrink globally to hide the defect.
    let items = vec![word("alpha", 95_000), glue(0, 0, 0), word("beta", 6_000),
        glue(2_000, 3_000, 1_000), forced(0)];
    let lines = layout(&items, 100_000, 15);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].end, 3);
    assert_eq!(lines[0].natural_width, units(101_000));
    assert!(lines[0].badness < INF_PENALTY);
}

#[test]
fn all_optimizer_modes_obey_the_terminal_contraction_rule() {
    let items = pressure();
    for flags in 0..8 {
        let mut scratch = ParagraphLayoutScratch::new();
        scratch.set_expansion_permilli(15);
        scratch.set_gradual_demerits(flags & 1 != 0);
        scratch.set_river_penalty(flags & 2 != 0);
        scratch.set_pareto_breaking(flags & 4 != 0);
        let mut lines = Vec::new();
        break_paragraph_into(&items, units(100_000), &mut scratch, &mut lines);
        assert_eq!(texts(&items, &lines), ["alpha", "beta"], "mode {flags}");
        assert_eq!(lines.last().unwrap().natural_width, units(35_000));
    }
}

#[test]
fn line_count_variants_do_not_call_an_unpainted_compression_feasible() {
    let items = pressure();
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_expansion_permilli(15);
    let candidates = break_paragraph_candidates(&items, units(100_000), &mut scratch);
    assert!(!candidates.variants.is_empty());
    for variant in candidates.variants {
        assert!(variant.line_count >= 2);
        let last = variant.lines.last().unwrap();
        assert!(last.natural_width <= units(100_000) || last.badness >= INF_PENALTY);
        assert_eq!(last.next, items.len());
    }
}

#[test]
fn a_genuinely_indivisible_overwide_box_remains_visible_and_overfull() {
    let items = vec![word("unbreakable", 101_000), forced(0)];
    let lines = layout(&items, 100_000, 100);
    assert_eq!(texts(&items, &lines), ["unbreakable"]);
    assert_eq!(lines[0].natural_width, units(101_000));
    assert_eq!(lines[0].badness, INF_PENALTY);
}

#[test]
fn scratch_reuse_fast_paths_and_empty_input_do_not_restore_stale_flexibility() {
    let items = pressure();
    let expected = layout(&items, 100_000, 15);
    let mut scratch = ParagraphLayoutScratch::new();
    let mut lines = Vec::new();
    for _ in 0..4 {
        break_paragraph_into(&items, units(200_000), &mut scratch, &mut lines);
        assert_eq!(lines.len(), 1);
        break_paragraph_into(&items, units(100_000), &mut scratch, &mut lines);
        assert_eq!(lines, expected);
        break_paragraph_into(&[], units(100_000), &mut scratch, &mut lines);
        assert!(lines.is_empty());
    }
}

#[test]
fn bundled_font_paragraphs_fit_without_phantom_terminal_space_compression() {
    use franken_markdown::{FontFamily, fonts::{FontStyle, load_body}};
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let font = load_body(family, FontStyle::Regular).unwrap();
        let items = paragraph_items_from_text(&font, "short tails", FontSize::from_points(12));
        let natural: i32 = items.iter().map(|item| item.width().milli_points()).sum();
        let shrink: i32 = items.iter().filter_map(|item| match item {
            ParagraphItem::Glue(glue) => Some(glue.shrink.milli_points()), _ => None,
        }).sum();
        assert!(shrink > 1, "fixture needs genuine interword shrink");
        let width = natural - 1;
        let lines = layout(&items, width, 0);
        assert_eq!(texts(&items, &lines), ["short", "tails"]);
        assert!(lines.iter().all(|line| line.natural_width <= units(width)));
    }
}
