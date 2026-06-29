#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::Inline;
use franken_markdown::layout::{
    AdvanceMetrics, FORCED_BREAK_PENALTY, FitnessClass, FontSize, HyphenationOptions, Hyphenator,
    LayoutUnit, MicrotypeOptions, PairMetrics, ParagraphItem, ParagraphLayoutScratch, Penalty,
    StyledText, TextBox, TextStyle, UNITS_PER_POINT, adjustment_to_layout_units,
    advance_to_layout_units, break_paragraph, break_paragraph_into, expansion_budget,
    hyphenated_paragraph_items_from_text, hyphenated_paragraph_items_from_text_into,
    measure_advances, measure_styled_text, measure_text, measure_text_with_pairs,
    paragraph_items_from_inlines, paragraph_items_from_text, protruded_fit_width,
    protrusion_for_text,
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
    let first = &items[0];
    assert!(
        matches!(first, ParagraphItem::Box(_)),
        "expected first box, got {first:?}"
    );
    if let ParagraphItem::Box(item) = first {
        assert_eq!(item.text, "mi");
        assert_eq!(item.width, LayoutUnit::from_milli_points(11_500));
    }

    let interword = &items[1];
    assert!(
        matches!(interword, ParagraphItem::Glue(_)),
        "expected interword glue, got {interword:?}"
    );
    if let ParagraphItem::Glue(glue) = interword {
        assert_eq!(glue.width, LayoutUnit::from_milli_points(2_500));
        assert_eq!(glue.stretch, LayoutUnit::from_milli_points(1_250));
        assert_eq!(glue.shrink, LayoutUnit::from_milli_points(833));
    }

    let second = &items[2];
    assert!(
        matches!(second, ParagraphItem::Box(_)),
        "expected second box, got {second:?}"
    );
    if let ParagraphItem::Box(item) = second {
        assert_eq!(item.text, "AV");
        assert_eq!(item.width, LayoutUnit::from_milli_points(9_200));
    }

    let final_break = &items[3];
    assert!(
        matches!(final_break, ParagraphItem::Penalty(_)),
        "expected final forced break, got {final_break:?}"
    );
    if let ParagraphItem::Penalty(penalty) = final_break {
        assert_eq!(penalty.width, LayoutUnit::ZERO);
        assert_eq!(penalty.penalty, FORCED_BREAK_PENALTY);
        assert!(!penalty.flagged);
    }
}

#[test]
fn paragraph_items_keep_no_break_spaces_inside_words() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&metrics, "A\u{00a0}B C", size);

    assert_box_text(&items[0], "A\u{00a0}B");
    assert!(matches!(items[1], ParagraphItem::Glue(_)));
    assert_box_text(&items[2], "C");
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
fn styled_text_preserves_raw_html_source_for_layout_text() {
    let inlines = vec![
        Inline::Text("before ".to_string()),
        Inline::Html("<i>raw</i>".to_string()),
        Inline::Text(" ".to_string()),
        Inline::Emphasis(vec![Inline::Html("<span>styled</span>".to_string())]),
        Inline::Text(" after".to_string()),
    ];
    let styled = StyledText::from_inlines(&inlines);

    assert_eq!(
        styled.plain_text(),
        "before <i>raw</i> <span>styled</span> after"
    );
    assert_eq!(styled.runs.len(), 3);
    assert_eq!(styled.runs[0].text, "before <i>raw</i> ");
    assert_eq!(styled.runs[0].style, TextStyle::BODY);
    assert_eq!(styled.runs[1].text, "<span>styled</span>");
    assert_eq!(styled.runs[1].style, TextStyle::BODY.with_italic());
    assert_eq!(styled.runs[2].text, " after");
    assert_eq!(styled.runs[2].style, TextStyle::BODY);
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
fn line_break_certificate_locks_prefix_metric_behavior() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&metrics, "A A A A A", size);
    let breaks = break_paragraph(&items, LayoutUnit::from_milli_points(18_000));

    let certificate = breaks
        .iter()
        .map(|line| {
            (
                line.start,
                line.end,
                line.next,
                line.natural_width.milli_points(),
                line.badness,
                line.fitness,
                line.demerits,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        certificate,
        vec![
            (0, 5, 6, 20_000, 172, FitnessClass::Tight, 29_929),
            (6, 9, 10, 12_500, 0, FitnessClass::Decent, 29_930),
        ]
    );
}

#[test]
fn break_paragraph_never_spans_an_interior_forced_break() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let box_a = || {
        ParagraphItem::Box(TextBox {
            text: "A".to_string(),
            runs: StyledText::plain("A"),
            width: measure_text_with_pairs(&metrics, "A", size),
        })
    };
    let forced = || {
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        })
    };
    let items = vec![box_a(), forced(), box_a(), forced()];
    let breaks = break_paragraph(&items, LayoutUnit::from_milli_points(100_000));

    assert_eq!(breaks.len(), 2);
    assert_eq!(line_text(&items, breaks[0].start, breaks[0].end), "A");
    assert_eq!(breaks[0].next, 2);
    assert_eq!(line_text(&items, breaks[1].start, breaks[1].end), "A");
}

#[test]
fn break_paragraph_falls_back_when_every_dp_edge_has_infinite_badness() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(40_000);
    let items = paragraph_items_from_text(&metrics, "mmmm mmmm mmmm mmmm", size);
    let breaks = break_paragraph(&items, width);

    assert_eq!(
        breaks.len(),
        4,
        "saturated-badness DP must not collapse the whole paragraph into one overfull line"
    );
    for line in &breaks {
        assert_eq!(line_text(&items, line.start, line.end), "mmmm");
        assert!(line.natural_width <= width);
    }
}

#[test]
fn break_paragraph_into_matches_wrapper_and_reuses_scratch() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(18_000);
    let items = paragraph_items_from_text(&metrics, "a aa aaa aaaa aa a", size);
    let expected = break_paragraph(&items, width);

    let mut scratch = ParagraphLayoutScratch::new();
    let mut breaks = Vec::new();
    break_paragraph_into(&items, width, &mut scratch, &mut breaks);
    assert_eq!(breaks, expected);

    let first_capacities = scratch.capacities();
    assert!(first_capacities.candidates > 0);
    assert!(first_capacities.states > 0);
    assert!(first_capacities.prefix_widths > 0);

    break_paragraph_into(&items, width, &mut scratch, &mut breaks);
    assert_eq!(breaks, expected);
    assert_eq!(
        scratch.capacities(),
        first_capacities,
        "second same-sized layout should reuse retained scratch capacity"
    );
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
fn hyphenation_points_into_matches_allocating_api() {
    let hyphenator = Hyphenator::english();
    let opts = HyphenationOptions::default();
    let mut points = vec![99, 100];

    hyphenator.hyphenation_points_into("documentation", opts, &mut points);
    assert_eq!(points, hyphenator.hyphenation_points("documentation", opts));

    hyphenator.hyphenation_points_into("not-a-word", opts, &mut points);
    assert!(points.is_empty());
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
fn hyphenated_paragraph_items_keep_no_break_spaces_inside_words() {
    let metrics = StubMetrics;
    let hyphenator = Hyphenator::english();
    let size = FontSize::from_points(10);
    let items = hyphenated_paragraph_items_from_text(
        &metrics,
        &hyphenator,
        "alpha\u{00a0}beta gamma",
        size,
    );

    assert_box_text(&items[0], "alpha\u{00a0}beta");
    assert!(matches!(items[1], ParagraphItem::Glue(_)));
    assert_box_text(&items[2], "gamma");
}

#[test]
fn hyphenated_paragraph_items_into_matches_wrapper_and_reuses_scratch() {
    let metrics = StubMetrics;
    let hyphenator = Hyphenator::english();
    let size = FontSize::from_points(10);
    let text = "Documentation typography representation deterministic optimization";
    let expected = hyphenated_paragraph_items_from_text(&metrics, &hyphenator, text, size);
    let mut scratch = ParagraphLayoutScratch::new();
    let mut items = Vec::new();

    hyphenated_paragraph_items_from_text_into(
        &metrics,
        &hyphenator,
        text,
        size,
        &mut scratch,
        &mut items,
    );
    assert_eq!(items, expected);

    let first_capacities = scratch.capacities();
    assert!(first_capacities.hyphen_lower_bytes > 0);
    assert!(first_capacities.hyphen_dotted_bytes > 0);
    assert!(first_capacities.hyphen_scores > 0);
    assert!(first_capacities.hyphen_points > 0);

    hyphenated_paragraph_items_from_text_into(
        &metrics,
        &hyphenator,
        text,
        size,
        &mut scratch,
        &mut items,
    );
    assert_eq!(items, expected);
    assert_eq!(
        scratch.capacities(),
        first_capacities,
        "second same-sized item build should reuse retained hyphenation buffers"
    );
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
    assert_eq!(
        breaks[0].natural_width,
        LayoutUnit::from_milli_points(35_000)
    );
    assert_eq!(
        breaks[0].natural_width,
        measure_text_with_pairs(&metrics, "hyphen", size)
            + measure_text_with_pairs(&metrics, "-", size)
    );
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

#[test]
fn microtype_fit_width_clamps_to_zero_when_protrusion_exceeds_natural_width() {
    let size = FontSize::from_points(10);
    let options = MicrotypeOptions::CONSERVATIVE;

    assert_eq!(
        protruded_fit_width(LayoutUnit::from_milli_points(2_000), ".", size, options),
        LayoutUnit::ZERO
    );
}

#[test]
fn microtype_protrusion_changes_a_line_fit_decision_deterministically() {
    // The microtypography cost hook's intended effect on a line-fit decision: a
    // line whose natural width slightly overruns the column does NOT fit with
    // protrusion disabled, but DOES fit once its trailing period is allowed to
    // hang into the margin. The delta is exact integer math (no float), so the
    // decision is deterministic. (Hooks are off by default; this is the opt-in
    // behavior a renderer would get by enabling MicrotypeOptions.)
    let size = FontSize::from_points(10);
    let column = LayoutUnit::from_milli_points(50_000);
    let natural = LayoutUnit::from_milli_points(52_000); // 2 pt over the column

    // Disabled: fit width == natural width, which overruns the column.
    let disabled = protruded_fit_width(natural, "sentence.", size, MicrotypeOptions::DISABLED);
    assert_eq!(disabled, natural);
    assert!(
        disabled > column,
        "without protrusion the line must not fit"
    );

    // Enabled: the trailing '.' protrudes 550‰ × 10 pt = 5_500 milli-points, so
    // the fit width drops to 46_500 and the line now fits the column.
    let enabled = protruded_fit_width(natural, "sentence.", size, MicrotypeOptions::CONSERVATIVE);
    assert_eq!(enabled, LayoutUnit::from_milli_points(46_500));
    assert!(
        enabled <= column,
        "with protrusion the trailing period lets the line fit: {enabled:?} <= {column:?}"
    );
}

#[test]
fn microtype_protrusion_table_is_stable() {
    // Golden fixture for the intended optical-margin deltas: each protruding
    // character's fit-width reduction at 1000 pt (so milli-points read directly
    // as per-mille). Pins the table so any change to the deltas is a reviewed,
    // explained change.
    let size = FontSize::from_points(1000);
    let opts = MicrotypeOptions::CONSERVATIVE;
    let right = |c: &str| {
        (LayoutUnit::from_milli_points(1_000_000).milli_points()
            - protruded_fit_width(LayoutUnit::from_milli_points(1_000_000), c, size, opts)
                .milli_points())
            / 1000
    };
    // Right-edge protrusion per-mille (trailing character).
    assert_eq!(right("a."), 550);
    assert_eq!(right("a,"), 550);
    assert_eq!(right("a:"), 420);
    assert_eq!(right("a;"), 420);
    assert_eq!(right("a!"), 250);
    assert_eq!(right("a?"), 250);
    assert_eq!(right("a)"), 120);
    assert_eq!(right("a-"), 80);
    assert_eq!(right("ax"), 0);
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
    assert!(
        matches!(item, ParagraphItem::Box(_)),
        "expected styled box, got {item:?}"
    );
    if let ParagraphItem::Box(b) = item {
        assert_eq!(b.text, text);
        assert_eq!(b.runs.runs.len(), 1);
        assert_eq!(b.runs.runs[0].text, text);
        assert_eq!(b.runs.runs[0].style, style);
    }
}

fn assert_box_text(item: &ParagraphItem, text: &str) {
    assert!(
        matches!(item, ParagraphItem::Box(_)),
        "expected text box {text:?}, got {item:?}"
    );
    if let ParagraphItem::Box(b) = item {
        assert_eq!(b.text, text);
    }
}

#[test]
fn metric_sums_saturate_instead_of_overflowing() {
    let huge = FontSize::from_milli_points(u32::MAX);
    let measured = measure_advances([u32::MAX, u32::MAX, u32::MAX], huge);

    assert_eq!(measured.milli_points(), i32::MAX);
}
