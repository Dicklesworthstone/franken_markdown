//! Pagination alternatives must preserve the actual baseline and fit measure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    FORCED_BREAK_PENALTY, Glue, INF_PENALTY, LayoutUnit, ParagraphItem,
    ParagraphLayoutScratch, Penalty, Protrusion, StyledText, TextBox,
    break_paragraph_candidates, break_paragraph_into,
};

fn u(value: i32) -> LayoutUnit { LayoutUnit::from_milli_points(value) }
fn word(width: i32, left: i32, right: i32) -> ParagraphItem {
    ParagraphItem::Box(TextBox {
        text: "word".to_owned(), runs: StyledText::plain("word"), width: u(width),
        protrusion: Protrusion { left: u(left), right: u(right) },
    })
}
fn forced() -> ParagraphItem {
    ParagraphItem::Penalty(Penalty { width: u(0), penalty: FORCED_BREAK_PENALTY, flagged: false })
}
fn spaces(widths: &[i32]) -> Vec<ParagraphItem> {
    let mut items = Vec::new();
    for &width in widths {
        if !items.is_empty() {
            items.push(ParagraphItem::Glue(Glue { width: u(10_000), stretch: u(50_000), shrink: u(3_000) }));
        }
        items.push(word(width, 0, 0));
    }
    items.push(forced());
    items
}

#[test]
fn single_line_optical_fit_retains_the_actual_natural_width() {
    let items = vec![word(103_000, 1_000, 2_000), forced()];
    for justified in [false, true] {
        let mut scratch = ParagraphLayoutScratch::new();
        scratch.set_justified(justified);
        let mut lines = Vec::new();
        break_paragraph_into(&items, u(100_000), &mut scratch, &mut lines);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].natural_width, u(103_000));
        assert_eq!(lines[0].badness, 0);
    }
}

#[test]
fn optical_credit_cannot_make_natural_width_negative() {
    let items = vec![word(1_000, 2_000, 2_000), forced()];
    let mut scratch = ParagraphLayoutScratch::new();
    let mut lines = Vec::new();
    break_paragraph_into(&items, u(10_000), &mut scratch, &mut lines);
    assert_eq!(lines[0].natural_width, u(1_000));
}

#[test]
fn alternatives_never_replace_the_production_baseline() {
    for items in [spaces(&[45_000, 45_000, 65_000, 45_000]),
                  spaces(&[50_000, 30_000, 20_000, 70_000]),
                  vec![word(103_000, 1_000, 2_000), forced()]] {
        for justified in [false, true] {
            let mut scratch = ParagraphLayoutScratch::new();
            scratch.set_justified(justified);
            scratch.set_expansion_permilli(0);
            let mut baseline = Vec::new();
            break_paragraph_into(&items, u(100_000), &mut scratch, &mut baseline);
            let variants = break_paragraph_candidates(&items, u(100_000), &mut scratch);
            let same_count: Vec<_> = variants.variants.iter()
                .filter(|variant| variant.line_count == baseline.len()).collect();
            assert_eq!(same_count.len(), 1);
            assert_eq!(same_count[0].lines, baseline);
            assert_eq!(same_count[0].demerits, baseline.last().unwrap().demerits);
        }
    }
}

#[test]
fn counted_solver_does_not_offer_overfull_terminal_lines_to_save_a_page() {
    // Two lines could be [45+10+45] [65+10+45], but the second is 120pt
    // and terminal painting cannot compress it. It is not a quality variant.
    let items = spaces(&[45_000, 45_000, 65_000, 45_000]);
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_expansion_permilli(0);
    let mut baseline = Vec::new();
    break_paragraph_into(&items, u(100_000), &mut scratch, &mut baseline);
    assert_eq!(baseline.len(), 3, "fixture must keep the tail on separate lines");
    let variants = break_paragraph_candidates(&items, u(100_000), &mut scratch);
    assert!(!variants.variants.iter().any(|variant| variant.line_count == 2));
    assert_eq!(variants.variants.iter().find(|v| v.line_count == baseline.len()).unwrap().lines, baseline);
}

#[test]
fn ragged_adjacent_variants_are_natural_width_layouts() {
    let items = spaces(&[35_000, 25_000, 40_000, 20_000, 55_000, 30_000]);
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_justified(false);
    scratch.set_expansion_permilli(100);
    let mut baseline = Vec::new();
    break_paragraph_into(&items, u(100_000), &mut scratch, &mut baseline);
    let variants = break_paragraph_candidates(&items, u(100_000), &mut scratch);
    assert!(variants.variants.len() >= 2, "retain genuine same-measure alternatives");
    for variant in variants.variants {
        assert!(variant.line_count.abs_diff(baseline.len()) <= 1);
        assert_eq!(variant.lines.len(), variant.line_count);
        assert!(variant.lines.iter().all(|line| line.natural_width <= u(100_000)));
        assert!(variant.lines.iter().all(|line| line.badness < INF_PENALTY));
        assert_eq!(variant.lines.last().unwrap().next, items.len());
    }
}

#[test]
fn unavoidable_baseline_overflow_is_not_lost_or_relabelled() {
    let items = vec![word(150_000, 0, 0), forced()];
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_justified(false);
    let mut baseline = Vec::new();
    break_paragraph_into(&items, u(100_000), &mut scratch, &mut baseline);
    let candidates = break_paragraph_candidates(&items, u(100_000), &mut scratch);
    assert_eq!(candidates.variants.len(), 1);
    assert_eq!(candidates.variants[0].lines, baseline);
    assert_eq!(candidates.variants[0].lines[0].badness, INF_PENALTY);
}
