#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    AdvanceMetrics, FORCED_BREAK_PENALTY, FitnessClass, FontSize, HyphenationOptions, Hyphenator,
    LayoutUnit, PairMetrics, ParagraphItem, UNITS_PER_POINT, adjustment_to_layout_units,
    advance_to_layout_units, break_paragraph, hyphenated_paragraph_items_from_text,
    measure_advances, measure_text, measure_text_with_pairs, paragraph_items_from_text,
};

struct StubMetrics;

impl AdvanceMetrics for StubMetrics {
    fn advance_1000(&self, ch: char) -> u32 {
        match ch {
            'i' => 250,
            'm' => 900,
            ' ' => 250,
            _ => 500,
        }
    }
}

impl PairMetrics for StubMetrics {
    fn kerning_1000(&self, left: char, right: char) -> i32 {
        match (left, right) {
            ('A', 'V') => -80,
            _ => 0,
        }
    }
}

#[test]
fn layout_units_are_milli_points() {
    assert_eq!(UNITS_PER_POINT, 1000);
    assert_eq!(LayoutUnit::from_points(72).milli_points(), 72_000);
    assert_eq!(LayoutUnit::from_milli_points(1_250).to_points_f32(), 1.25);
}

#[test]
fn advance_conversion_uses_integer_milli_points() {
    let size = FontSize::from_points(11);

    assert_eq!(
        advance_to_layout_units(500, size),
        LayoutUnit::from_milli_points(5_500)
    );
    assert_eq!(
        advance_to_layout_units(250, size),
        LayoutUnit::from_milli_points(2_750)
    );
}

#[test]
fn fractional_font_sizes_are_represented_without_floats() {
    let size = FontSize::from_milli_points(9_500);

    assert_eq!(
        advance_to_layout_units(600, size),
        LayoutUnit::from_milli_points(5_700)
    );
}

#[test]
fn pair_adjustments_can_be_negative_fixed_point_values() {
    let size = FontSize::from_points(10);

    assert_eq!(
        adjustment_to_layout_units(-80, size),
        LayoutUnit::from_milli_points(-800)
    );
}

#[test]
fn text_measurement_is_deterministic_and_ordered() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);

    assert_eq!(
        measure_text(&metrics, "mi mi", size),
        LayoutUnit::from_milli_points(25_500)
    );
    assert_eq!(
        measure_advances([900, 250, 250, 900, 250], size),
        LayoutUnit::from_milli_points(25_500)
    );
}

#[test]
fn pair_kerning_contributes_to_text_measurement() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);

    assert_eq!(
        measure_text(&metrics, "AV", size),
        LayoutUnit::from_milli_points(10_000)
    );
    assert_eq!(
        measure_text_with_pairs(&metrics, "AV", size),
        LayoutUnit::from_milli_points(9_200)
    );
}

#[test]
fn paragraph_items_use_boxes_glue_and_final_forced_break() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&metrics, "mi AV", size);

    assert_eq!(items.len(), 4);
    match &items[0] {
        ParagraphItem::Box(item) => {
            assert_eq!(item.text, "mi");
            assert_eq!(item.width, LayoutUnit::from_milli_points(11_500));
        }
        other => panic!("expected first box, got {other:?}"),
    }
    match &items[1] {
        ParagraphItem::Glue(glue) => {
            assert_eq!(glue.width, LayoutUnit::from_milli_points(2_500));
            assert_eq!(glue.stretch, LayoutUnit::from_milli_points(1_250));
            assert_eq!(glue.shrink, LayoutUnit::from_milli_points(833));
        }
        other => panic!("expected interword glue, got {other:?}"),
    }
    match &items[2] {
        ParagraphItem::Box(item) => {
            assert_eq!(item.text, "AV");
            assert_eq!(item.width, LayoutUnit::from_milli_points(9_200));
        }
        other => panic!("expected second box, got {other:?}"),
    }
    match &items[3] {
        ParagraphItem::Penalty(penalty) => {
            assert_eq!(penalty.width, LayoutUnit::ZERO);
            assert_eq!(penalty.penalty, FORCED_BREAK_PENALTY);
            assert!(!penalty.flagged);
        }
        other => panic!("expected final forced break, got {other:?}"),
    }
}

#[test]
fn paragraph_item_width_returns_natural_width() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&metrics, "A B", size);

    assert_eq!(items[0].width(), LayoutUnit::from_milli_points(5_000));
    assert_eq!(items[1].width(), LayoutUnit::from_milli_points(2_500));
    assert_eq!(items[3].width(), LayoutUnit::ZERO);
}

#[test]
fn break_paragraph_optimizes_across_the_whole_paragraph() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&metrics, "A A A A A", size);
    let breaks = break_paragraph(&items, LayoutUnit::from_milli_points(18_000));

    assert_eq!(breaks.len(), 2);
    assert_eq!(line_text(&items, breaks[0].start, breaks[0].end), "A A A");
    assert_eq!(line_text(&items, breaks[1].start, breaks[1].end), "A A");
    assert_eq!(breaks[0].fitness, FitnessClass::Tight);
    assert_eq!(breaks[1].fitness, FitnessClass::Decent);
    assert!(breaks[0].demerits < breaks[1].demerits);
    assert_eq!(breaks[1].badness, 0);
    assert!(breaks[0].badness < 10_000);
}

#[test]
fn break_paragraph_returns_empty_for_no_candidates() {
    let breaks = break_paragraph(&[], LayoutUnit::from_points(72));

    assert!(breaks.is_empty());
}

#[test]
fn english_hyphenator_uses_exceptions_and_minima() {
    let hyphenator = Hyphenator::english();

    assert_eq!(
        hyphenator.hyphenation_points("hyphenation", HyphenationOptions::default()),
        vec![2, 6]
    );
    assert_eq!(
        hyphenator.hyphenation_points(
            "hyphenation",
            HyphenationOptions {
                min_left: 3,
                min_right: 6,
            },
        ),
        Vec::<usize>::new()
    );
    assert!(
        hyphenator
            .hyphenation_points("not-a-word", HyphenationOptions::default())
            .is_empty()
    );
}

#[test]
fn hyphenated_paragraph_items_emit_flagged_discretionary_penalties() {
    let metrics = StubMetrics;
    let hyphenator = Hyphenator::english();
    let size = FontSize::from_points(10);
    let items = hyphenated_paragraph_items_from_text(&metrics, &hyphenator, "hyphenation", size);

    assert_eq!(line_text(&items, 0, items.len()), "hy phen ation");
    let flagged = items
        .iter()
        .filter_map(|item| match item {
            ParagraphItem::Penalty(p) if p.flagged => Some(p),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(flagged.len(), 2);
    assert!(flagged.iter().all(|p| p.penalty > 0));
    assert!(flagged.iter().all(|p| p.width > LayoutUnit::ZERO));
}

#[test]
fn hyphen_penalty_width_applies_only_when_the_break_is_chosen() {
    let metrics = StubMetrics;
    let hyphenator = Hyphenator::english();
    let size = FontSize::from_points(10);
    let items = hyphenated_paragraph_items_from_text(&metrics, &hyphenator, "hyphenation", size);
    let breaks = break_paragraph(&items, LayoutUnit::from_milli_points(35_000));

    assert_eq!(breaks.len(), 2);
    assert_eq!(line_text(&items, breaks[0].start, breaks[0].end), "hy phen");
    assert!(breaks[0].natural_width > measure_text_with_pairs(&metrics, "hyphen", size));
}

fn line_text(items: &[ParagraphItem], start: usize, end: usize) -> String {
    let mut words = Vec::new();
    for item in &items[start..end] {
        if let ParagraphItem::Box(b) = item {
            words.push(b.text.as_str());
        }
    }
    words.join(" ")
}

#[test]
fn metric_sums_saturate_instead_of_overflowing() {
    let huge = FontSize::from_milli_points(u32::MAX);
    let measured = measure_advances([u32::MAX, u32::MAX, u32::MAX], huge);

    assert_eq!(measured.milli_points(), i32::MAX);
}
