#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::Inline;
use franken_markdown::layout::{
    AdvanceMetrics, FORCED_BREAK_PENALTY, FitnessClass, FontSize, HyphenationOptions, Hyphenator,
    LayoutUnit, MicrotypeOptions, PairMetrics, ParagraphItem, StyledText, TextStyle,
    UNITS_PER_POINT, adjustment_to_layout_units, advance_to_layout_units, break_paragraph,
    expansion_budget, hyphenated_paragraph_items_from_text, measure_advances, measure_styled_text,
    measure_text, measure_text_with_pairs, paragraph_items_from_inlines, paragraph_items_from_text,
    protruded_fit_width, protrusion_for_text,
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
fn styled_text_preserves_markdown_inline_boundaries() {
    let inlines = vec![
        Inline::Text("plain ".to_string()),
        Inline::Strong(vec![
            Inline::Text("bold".to_string()),
            Inline::Emphasis(vec![Inline::Text(" both".to_string())]),
        ]),
        Inline::Text(" ".to_string()),
        Inline::Code("code".to_string()),
        Inline::Text(" ".to_string()),
        Inline::Link {
            dest: "https://example.com".to_string(),
            title: None,
            content: vec![Inline::Text("link".to_string())],
        },
    ];
    let styled = StyledText::from_inlines(&inlines);

    assert_eq!(styled.plain_text(), "plain bold both code link");
    assert_eq!(styled.runs.len(), 7);
    assert_eq!(styled.runs[0].text, "plain ");
    assert_eq!(styled.runs[0].style, TextStyle::BODY);
    assert_eq!(styled.runs[1].text, "bold");
    assert_eq!(styled.runs[1].style, TextStyle::BODY.with_bold());
    assert_eq!(styled.runs[2].text, " both");
    assert_eq!(
        styled.runs[2].style,
        TextStyle::BODY.with_bold().with_italic()
    );
    assert_eq!(styled.runs[4].text, "code");
    assert_eq!(styled.runs[4].style, TextStyle::BODY.with_code());
    assert_eq!(styled.runs[6].text, "link");
    assert_eq!(styled.runs[6].style, TextStyle::BODY.with_link());
}

#[test]
fn paragraph_items_from_inlines_keep_styles_inside_boxes() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let inlines = vec![
        Inline::Text("plain ".to_string()),
        Inline::Strong(vec![Inline::Text("bold".to_string())]),
        Inline::Text(" ".to_string()),
        Inline::Code("code".to_string()),
        Inline::Text(" ".to_string()),
        Inline::Link {
            dest: "#target".to_string(),
            title: None,
            content: vec![Inline::Text("link".to_string())],
        },
    ];
    let items = paragraph_items_from_inlines(&metrics, &inlines, size);

    assert_eq!(items.len(), 8);
    assert_box_style(&items[0], "plain", TextStyle::BODY);
    assert_box_style(&items[2], "bold", TextStyle::BODY.with_bold());
    assert_box_style(&items[4], "code", TextStyle::BODY.with_code());
    assert_box_style(&items[6], "link", TextStyle::BODY.with_link());
    assert_eq!(
        measure_styled_text(&metrics, &StyledText::from_inlines(&inlines), size),
        measure_text_with_pairs(&metrics, "plain ", size)
            + measure_text_with_pairs(&metrics, "bold", size)
            + measure_text_with_pairs(&metrics, " ", size)
            + measure_text_with_pairs(&metrics, "code", size)
            + measure_text_with_pairs(&metrics, " ", size)
            + measure_text_with_pairs(&metrics, "link", size)
    );
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
fn english_hyphenator_uses_full_tex_pattern_corpus() {
    let hyphenator = Hyphenator::english();

    assert_eq!(hyphenator.encoded_pattern_count(), 4_938);
    assert_eq!(
        hyphenator.hyphenation_points("representation", HyphenationOptions::default()),
        vec![3, 5, 8, 10]
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

#[test]
fn microtype_hooks_are_disabled_by_default() {
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(20_000);

    assert_eq!(
        protrusion_for_text("\"Hello.\"", size, MicrotypeOptions::default()).total(),
        LayoutUnit::ZERO
    );
    assert_eq!(
        protruded_fit_width(width, "\"Hello.\"", size, MicrotypeOptions::default()),
        width
    );
    assert_eq!(
        expansion_budget(width, MicrotypeOptions::default()),
        LayoutUnit::ZERO
    );
}

#[test]
fn microtype_protrusion_and_expansion_are_integer_deterministic() {
    let size = FontSize::from_points(10);
    let options = MicrotypeOptions::CONSERVATIVE;
    let protrusion = protrusion_for_text("\"Hello.\"", size, options);

    assert_eq!(protrusion.left, LayoutUnit::from_milli_points(3_500));
    assert_eq!(protrusion.right, LayoutUnit::from_milli_points(3_500));
    assert_eq!(
        protruded_fit_width(
            LayoutUnit::from_milli_points(50_000),
            "\"Hello.\"",
            size,
            options
        ),
        LayoutUnit::from_milli_points(43_000)
    );
    assert_eq!(
        expansion_budget(LayoutUnit::from_milli_points(50_000), options),
        LayoutUnit::from_milli_points(750)
    );
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

fn assert_box_style(item: &ParagraphItem, text: &str, style: TextStyle) {
    match item {
        ParagraphItem::Box(b) => {
            assert_eq!(b.text, text);
            assert_eq!(b.runs.runs.len(), 1);
            assert_eq!(b.runs.runs[0].text, text);
            assert_eq!(b.runs.runs[0].style, style);
        }
        other => panic!("expected styled box, got {other:?}"),
    }
}

#[test]
fn metric_sums_saturate_instead_of_overflowing() {
    let huge = FontSize::from_milli_points(u32::MAX);
    let measured = measure_advances([u32::MAX, u32::MAX, u32::MAX], huge);

    assert_eq!(measured.milli_points(), i32::MAX);
}
