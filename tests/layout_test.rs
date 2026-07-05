#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::ast::Inline;
use franken_markdown::fonts::{FontStyle, load_body};
use franken_markdown::layout::{
    AdvanceMetrics, FORCED_BREAK_PENALTY, FitnessClass, FontSize, HyphenationOptions, Hyphenator,
    INF_PENALTY, LayoutUnit, MicrotypeOptions, PairMetrics, ParagraphItem, ParagraphLayoutScratch,
    Penalty, StyledText, TextBox, TextStyle, UNITS_PER_POINT, adjustment_to_layout_units,
    advance_to_layout_units, break_paragraph, break_paragraph_into, default_interword_glue,
    expansion_budget, hyphenated_paragraph_items_from_text,
    hyphenated_paragraph_items_from_text_into, measure_advances, measure_styled_text, measure_text,
    measure_text_with_pairs, paragraph_items_from_inlines, paragraph_items_from_text,
    protruded_fit_width, protrusion_for_text,
};
use franken_markdown::text::Font;
use franken_markdown::theme::FontFamily;

/// A deterministic, hand-computable metrics *oracle* — NOT a mock of the system
/// under test.
///
/// The system under test in this file is the layout algorithm: measurement,
/// box/glue/penalty construction, hyphenation point placement, microtypography,
/// and the Knuth-Plass line breaker. None of that is stubbed here. What
/// `StubMetrics` stands in for is purely an *input* to those functions — the
/// glyph-advance and pair-kerning table that the algorithm reads through the
/// public [`AdvanceMetrics`] / [`PairMetrics`] traits.
///
/// It is retained deliberately, for three reasons:
///
/// 1. **Exact, font-independent arithmetic.** Its advances (`i`=250, `m`=900,
///    space=250, everything else 500 ‱) and its single kern pair (`AV`=-80) are
///    chosen so every expected milli-point width, badness, demerit, and break
///    decision is computable by hand. That lets the existing certificate-style
///    tests pin the breaker's output to exact integers, which would be brittle
///    and unreadable if tied to a specific real face's hinted metrics.
/// 2. **It exercises a code path the bundled fonts cannot.** The vendored OFL
///    faces ship no legacy `kern` table, so `Font::kerning_1000` returns 0 for
///    every pair (verified by [`real_font_pair_metrics_match_advance_only_path`]
///    below). The kerning arithmetic inside [`measure_text_with_pairs`] is only
///    given a non-zero adjustment to integrate by an oracle like this one.
/// 3. **It is an oracle, not a fake of behavior we are testing.** A mock would
///    impersonate the line breaker and assert on calls into it; this only
///    answers "how wide is this glyph", which is exactly the kind of pure,
///    side-effect-free input a deterministic oracle should supply.
///
/// The real-font tests further down (`real_font_*`) run the *same* layout
/// functions against actual bundled-font metrics, proving the algorithm holds on
/// reality and not just on this synthetic table. The two approaches are
/// complementary: the oracle gives exactness, the real fonts give fidelity.
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
fn too_wide_token_keeps_optimal_breaking_instead_of_greedy_over_the_whole_paragraph() {
    // A token wider than the line used to make the WHOLE paragraph fall back to
    // greedy first-fit (states[last] == None). Overfull lines are now selectable
    // at a large finite demerit, so the short-word region keeps its Knuth-Plass
    // optimal breaking and only the too-wide token is forced onto its own line.
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(18_000);

    // A single 8-char box (no spaces) is far wider than the line.
    let items = paragraph_items_from_text(&metrics, "A A A A A AAAAAAAA", size);
    let breaks = break_paragraph(&items, width);

    let lines: Vec<String> = breaks
        .iter()
        .map(|b| line_text(&items, b.start, b.end))
        .collect();
    // The five short words are broken into two justified lines and the over-wide
    // token is isolated on its own (overfull) line — not a greedy first-fit of the
    // whole paragraph. (Greedy would pack the words maximally into the first line.)
    assert_eq!(lines, vec!["A A", "A A A", "AAAAAAAA"], "got {lines:?}");
    // The token's line is genuinely overfull yet was still selected rather than
    // discarding the whole paragraph's optimal breaking.
    assert!(
        breaks[2].natural_width > width,
        "the isolated token line is overfull"
    );
    assert_eq!(breaks[2].badness, INF_PENALTY, "overfull line badness caps");
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
fn single_forced_break_fast_path_keeps_dp_semantics_without_state_buffers() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(100_000);
    let items = paragraph_items_from_text(&metrics, "Heading", size);

    let mut scratch = ParagraphLayoutScratch::new();
    let mut breaks = Vec::new();
    break_paragraph_into(&items, width, &mut scratch, &mut breaks);

    assert_eq!(breaks.len(), 1);
    let line = breaks[0];
    assert_eq!(line.start, 0);
    assert_eq!(line.end, 1);
    assert_eq!(line.next, 2);
    assert_eq!(line_text(&items, line.start, line.end), "Heading");
    assert_eq!(line.badness, 0);
    assert_eq!(line.fitness, FitnessClass::Decent);
    assert_eq!(line.demerits, 1);

    let capacities = scratch.capacities();
    assert_eq!(capacities.forced_prefixes, 0);
    assert_eq!(capacities.states, 0);
    assert!(capacities.candidates > 0);
    assert!(capacities.prefix_widths > 0);
}

#[test]
fn paragraphs_without_hard_breaks_skip_forced_prefix_scratch() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(20_000);
    let items = paragraph_items_from_text(&metrics, "alpha beta gamma", size);

    let mut scratch = ParagraphLayoutScratch::new();
    let mut breaks = Vec::new();
    break_paragraph_into(&items, width, &mut scratch, &mut breaks);

    assert_eq!(breaks, break_paragraph(&items, width));
    let capacities = scratch.capacities();
    assert_eq!(capacities.forced_prefixes, 0);
    assert!(capacities.states > 0);
}

#[test]
fn fitting_multiword_paragraph_fast_path_skips_state_buffers() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let width = LayoutUnit::from_milli_points(100_000);
    let items = paragraph_items_from_text(&metrics, "alpha beta gamma", size);

    let mut scratch = ParagraphLayoutScratch::new();
    let mut breaks = Vec::new();
    break_paragraph_into(&items, width, &mut scratch, &mut breaks);

    assert_eq!(breaks.len(), 1);
    let line = breaks[0];
    assert_eq!(line.start, 0);
    assert_eq!(line.end, 5);
    assert_eq!(line.next, 6);
    assert_eq!(line_text(&items, line.start, line.end), "alpha beta gamma");
    assert_eq!(line.badness, 0);
    assert_eq!(line.fitness, FitnessClass::Decent);
    assert_eq!(line.demerits, 1);

    let capacities = scratch.capacities();
    assert_eq!(capacities.forced_prefixes, 0);
    assert_eq!(capacities.states, 0);
    assert!(capacities.candidates > 0);
    assert!(capacities.prefix_widths > 0);
}

#[test]
fn fitting_paragraph_with_negative_penalty_still_uses_dp_path() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let alpha = measure_text_with_pairs(&metrics, "alpha", size);
    let beta = measure_text_with_pairs(&metrics, "beta", size);
    let items = vec![
        ParagraphItem::Box(TextBox {
            text: "alpha".to_string(),
            runs: StyledText::plain("alpha"),
            width: alpha,
        }),
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: -50,
            flagged: false,
        }),
        ParagraphItem::Box(TextBox {
            text: "beta".to_string(),
            runs: StyledText::plain("beta"),
            width: beta,
        }),
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        }),
    ];

    let mut scratch = ParagraphLayoutScratch::new();
    let mut breaks = Vec::new();
    break_paragraph_into(
        &items,
        LayoutUnit::from_milli_points(100_000),
        &mut scratch,
        &mut breaks,
    );

    assert_eq!(
        breaks,
        break_paragraph(&items, LayoutUnit::from_milli_points(100_000))
    );
    assert!(
        scratch.capacities().states > 0,
        "negative non-forced penalties can change optimality, so they must keep the DP path"
    );
}

#[test]
fn interior_forced_breaks_still_use_prefix_scratch_and_split_lines() {
    let metrics = StubMetrics;
    let size = FontSize::from_points(10);
    let word_width = measure_text_with_pairs(&metrics, "alpha", size);
    let word = || {
        ParagraphItem::Box(TextBox {
            text: "alpha".to_string(),
            runs: StyledText::plain("alpha"),
            width: word_width,
        })
    };
    let forced = || {
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        })
    };
    let items = vec![word(), forced(), word(), forced()];

    let mut scratch = ParagraphLayoutScratch::new();
    let mut breaks = Vec::new();
    break_paragraph_into(
        &items,
        LayoutUnit::from_milli_points(100_000),
        &mut scratch,
        &mut breaks,
    );

    assert_eq!(breaks.len(), 2);
    assert_eq!(breaks[0].start, 0);
    assert_eq!(breaks[0].end, 1);
    assert_eq!(breaks[1].start, 2);
    assert_eq!(breaks[1].end, 3);
    assert!(scratch.capacities().forced_prefixes > 0);
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

// ---------------------------------------------------------------------------
// grn.3.1 — drive the SAME layout functions through REAL bundled-font metrics.
//
// These tests deliberately use `franken_markdown::fonts::load_body(..)` as the
// metrics source (no synthetic table), proving the measurement, item-building,
// hyphenation, and Knuth-Plass code paths behave on real glyph advances and not
// only on the hand-computable `StubMetrics` oracle documented above.
// ---------------------------------------------------------------------------

fn serif_font() -> Font {
    load_body(FontFamily::Serif, FontStyle::Regular).expect("bundled serif font parses")
}

/// A real-font advance source that intentionally does NOT override
/// [`PairMetrics::kerning_1000`], so `measure_text_with_pairs` is forced through
/// the trait-default (zero) kerning hook. The advances are 100% real bundled-font
/// data — this is a thin trait adapter to reach the default method, not a
/// synthetic metrics double.
struct AdvanceOnly<'a>(&'a Font);

impl AdvanceMetrics for AdvanceOnly<'_> {
    fn advance_1000(&self, ch: char) -> u32 {
        self.0.advance_1000(ch)
    }
}

impl PairMetrics for AdvanceOnly<'_> {}

#[test]
fn real_font_measurement_is_additive_and_real() {
    let font = serif_font();
    let size = FontSize::from_points(10);

    // Real measured advances are strictly positive and additive across glyphs.
    let m = measure_text(&font, "m", size);
    let mm = measure_text(&font, "mm", size);
    assert!(m > LayoutUnit::ZERO);
    assert_eq!(mm, m + m, "advance summation is exact for repeated glyphs");

    // A longer run measures wider than a strict prefix of it.
    assert!(measure_text(&font, "hyphenation", size) > measure_text(&font, "hyphen", size));

    // `measure_advances` must agree with `measure_text` when fed the same real
    // per-character advances the breaker would otherwise see.
    let advances: Vec<u32> = "documentation"
        .chars()
        .map(|ch| font.advance_1000(ch))
        .collect();
    assert_eq!(
        measure_advances(advances, size),
        measure_text(&font, "documentation", size)
    );
}

#[test]
fn real_font_pair_metrics_match_advance_only_path() {
    let font = serif_font();
    let size = FontSize::from_points(11);
    let text = "AVATAR Wave To.";

    // The bundled faces ship no legacy `kern` table, so the real font's
    // overriding `kerning_1000` contributes zero for every pair...
    let with_real_pairs = measure_text_with_pairs(&font, text, size);
    let no_pairs = measure_text(&font, text, size);
    assert_eq!(
        with_real_pairs, no_pairs,
        "real fonts carry no pair kerning"
    );

    // ...and the `PairMetrics::kerning_1000` *default* (exercised via AdvanceOnly,
    // which does not override it) is likewise a no-op over the same real advances.
    let with_default_pairs = measure_text_with_pairs(&AdvanceOnly(&font), text, size);
    assert_eq!(
        with_default_pairs, no_pairs,
        "the PairMetrics default kerning hook is a no-op over real advances"
    );
}

#[test]
fn real_font_paragraph_items_carry_real_box_widths() {
    let font = serif_font();
    let size = FontSize::from_points(12);
    let items = paragraph_items_from_text(&font, "alpha beta gamma", size);

    // 3 boxes + 2 interword glues + 1 trailing forced break.
    assert_eq!(items.len(), 6);
    let space = measure_text_with_pairs(&font, " ", size);
    for (item, word) in [
        (&items[0], "alpha"),
        (&items[2], "beta"),
        (&items[4], "gamma"),
    ] {
        match item {
            ParagraphItem::Box(b) => {
                assert_eq!(b.text, word);
                assert_eq!(b.width, measure_text_with_pairs(&font, word, size));
                assert!(b.width > LayoutUnit::ZERO);
            }
            other => panic!("expected real-measured box, got {other:?}"),
        }
    }
    for glue_idx in [1usize, 3] {
        match &items[glue_idx] {
            ParagraphItem::Glue(g) => assert_eq!(g.width, space),
            other => panic!("expected interword glue, got {other:?}"),
        }
    }
    assert!(matches!(
        items[5],
        ParagraphItem::Penalty(Penalty {
            penalty: FORCED_BREAK_PENALTY,
            ..
        })
    ));
}

#[test]
fn real_font_break_paragraph_wraps_and_is_monotonic() {
    let font = serif_font();
    let size = FontSize::from_points(11);
    let text = "the quick brown fox jumps over the lazy dog";
    let items = paragraph_items_from_text(&font, text, size);

    let narrow = LayoutUnit::from_milli_points(120_000);
    let breaks = break_paragraph(&items, narrow);

    // The real metrics force a wrap into more than one line.
    assert!(breaks.len() >= 2, "narrow column must wrap: {breaks:?}");

    // Every chosen line is feasible (the breaker never keeps an infinite-badness
    // edge) and cumulative demerits never decrease down the paragraph.
    let mut prev_demerits = i64::MIN;
    for line in &breaks {
        assert!(line.badness < INF_PENALTY, "feasible line: {line:?}");
        assert!(
            line.demerits >= prev_demerits,
            "demerits accumulate: {line:?}"
        );
        prev_demerits = line.demerits;
    }

    // The lines reconstruct the original word sequence, in order, with no loss.
    let reconstructed = breaks
        .iter()
        .map(|line| line_text(&items, line.start, line.end))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(reconstructed, text);

    // Metamorphic: a much wider column needs no more lines than a narrow one,
    // and a very wide column fits the whole sentence on a single line.
    let wide = break_paragraph(&items, LayoutUnit::from_points(600));
    assert!(wide.len() <= breaks.len());
    assert_eq!(wide.len(), 1, "the whole sentence fits one very wide line");
}

#[test]
fn real_font_hyphenation_inserts_real_discretionary_breaks() {
    let font = serif_font();
    let hyphenator = Hyphenator::english();
    let size = FontSize::from_points(10);
    let items = hyphenated_paragraph_items_from_text(&font, &hyphenator, "hyphenation", size);

    // Same fragmentation the exception dictates, now with real-measured widths.
    assert_eq!(line_text(&items, 0, items.len()), "hy phen ation");
    let hyphen_width = measure_text_with_pairs(&font, "-", size);
    let flagged: Vec<&Penalty> = items
        .iter()
        .filter_map(|item| match item {
            ParagraphItem::Penalty(p) if p.flagged => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(flagged.len(), 2);
    assert!(
        flagged
            .iter()
            .all(|p| p.penalty > 0 && p.width == hyphen_width)
    );

    // At a column exactly wide enough for "hyphen-", the optimal first line takes
    // the discretionary break and pays for the real hyphen glyph. Breaking after
    // "hy" is infeasible (no stretchable glue), so the breaker must choose "phen".
    let column = measure_text_with_pairs(&font, "hyphen", size) + hyphen_width;
    let breaks = break_paragraph(&items, column);
    assert_eq!(breaks.len(), 2);
    assert_eq!(line_text(&items, breaks[0].start, breaks[0].end), "hy phen");
    assert_eq!(breaks[0].natural_width, column);
}

// ---------------------------------------------------------------------------
// grn.2.7 — target previously-uncovered lines / branches.
// ---------------------------------------------------------------------------

#[test]
fn styled_text_covers_strikethrough_image_breaks_and_empty_runs() {
    let inlines = vec![
        Inline::Text(String::new()), // empty run: push_text early return
        Inline::Strikethrough(vec![Inline::Text("struck".to_string())]),
        Inline::SoftBreak, // becomes a space
        Inline::Image {
            dest: "img.png".to_string(),
            title: None,
            alt: "alt text".to_string(),
        },
        Inline::HardBreak, // also a space
        Inline::Text("end".to_string()),
    ];
    let styled = StyledText::from_inlines(&inlines);

    assert_eq!(styled.plain_text(), "struck alt text end");
    assert_eq!(styled.runs.len(), 2);
    assert_eq!(styled.runs[0].text, "struck");
    assert_eq!(styled.runs[0].style, TextStyle::BODY.with_strikethrough());
    assert_eq!(styled.runs[1].text, " alt text end");
    assert_eq!(styled.runs[1].style, TextStyle::BODY);
}

#[test]
fn styled_words_handle_runs_of_whitespace_and_trailing_space() {
    let font = serif_font();
    let size = FontSize::from_points(10);
    // Double interior spaces and a trailing space: the word splitter must not
    // emit empty words for the extra whitespace (the "current is empty" branches).
    let items = paragraph_items_from_text(&font, "A  B ", size);

    assert_eq!(items.len(), 4); // Box(A), Glue, Box(B), forced break
    assert_box_text(&items[0], "A");
    assert!(matches!(items[1], ParagraphItem::Glue(_)));
    assert_box_text(&items[2], "B");
    assert!(matches!(
        items[3],
        ParagraphItem::Penalty(Penalty {
            penalty: FORCED_BREAK_PENALTY,
            ..
        })
    ));
}

#[test]
fn left_edge_protrusion_covers_brackets_and_dashes() {
    let size = FontSize::from_points(10);
    let opts = MicrotypeOptions::CONSERVATIVE;

    // Opening bracket protrudes 120 per-mille on the left; closing 120 on right.
    let bracket = protrusion_for_text("(quote)", size, opts);
    assert_eq!(bracket.left, LayoutUnit::from_milli_points(1_200));
    assert_eq!(bracket.right, LayoutUnit::from_milli_points(1_200));

    // A leading dash protrudes 80 per-mille on the left.
    let dash = protrusion_for_text("-dash", size, opts);
    assert_eq!(dash.left, LayoutUnit::from_milli_points(800));
}

#[test]
fn protruded_fit_width_handles_nonpositive_natural_width() {
    let size = FontSize::from_points(10);
    let opts = MicrotypeOptions::CONSERVATIVE;
    // A non-positive natural width clamps to zero regardless of protrusion (the
    // `natural_width <= ZERO` arm of the guard).
    assert_eq!(
        protruded_fit_width(LayoutUnit::from_milli_points(-1_000), "(x", size, opts),
        LayoutUnit::ZERO
    );
}

#[test]
fn adjustment_conversion_saturates_in_both_directions() {
    let huge = FontSize::from_milli_points(u32::MAX);
    assert_eq!(
        adjustment_to_layout_units(i32::MAX, huge).milli_points(),
        i32::MAX
    );
    assert_eq!(
        adjustment_to_layout_units(i32::MIN, huge).milli_points(),
        i32::MIN
    );
}

#[test]
fn scratch_clear_retains_allocations() {
    let font = serif_font();
    let hyphenator = Hyphenator::english();
    let size = FontSize::from_points(10);
    let mut scratch = ParagraphLayoutScratch::new();
    let mut items = Vec::new();

    // Populate the hyphenation buffers...
    hyphenated_paragraph_items_from_text_into(
        &font,
        &hyphenator,
        "documentation typography representation",
        size,
        &mut scratch,
        &mut items,
    );
    // ...and the line-breaking buffers, reusing the same scratch.
    let mut breaks = Vec::new();
    break_paragraph_into(
        &items,
        LayoutUnit::from_milli_points(80_000),
        &mut scratch,
        &mut breaks,
    );

    let before = scratch.capacities();
    assert!(before.hyphen_points > 0);
    assert!(before.candidates > 0);
    assert!(before.prefix_widths > 0);
    assert!(before.states > 0);

    scratch.clear();
    assert_eq!(
        scratch.capacities(),
        before,
        "clear() drops live data but retains every allocation for reuse"
    );
}

#[test]
fn prohibited_penalty_is_not_a_breakpoint() {
    let font = serif_font();
    let size = FontSize::from_points(12);
    let left = measure_text_with_pairs(&font, "left", size);
    let right = measure_text_with_pairs(&font, "right", size);
    let items = vec![
        ParagraphItem::Box(TextBox {
            text: "left".to_string(),
            runs: StyledText::plain("left"),
            width: left,
        }),
        // A prohibiting penalty (>= INF_PENALTY) must NOT become a candidate.
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: INF_PENALTY,
            flagged: false,
        }),
        ParagraphItem::Box(TextBox {
            text: "right".to_string(),
            runs: StyledText::plain("right"),
            width: right,
        }),
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        }),
    ];
    // Column only wide enough for "left": with no usable breakpoint between the
    // two boxes, they stay welded and the emergency fallback emits one overfull
    // line containing both.
    let breaks = break_paragraph(&items, left);

    assert_eq!(breaks.len(), 1);
    assert_eq!(
        line_text(&items, breaks[0].start, breaks[0].end),
        "left right"
    );
    assert_eq!(breaks[0].natural_width, left + right);
    assert!(
        breaks[0].natural_width > left,
        "the welded line overflows the column"
    );
}

#[test]
fn greedy_fallback_emits_overfull_leading_line_without_forced_tail() {
    let font = serif_font();
    let size = FontSize::from_points(12);
    let space = measure_text_with_pairs(&font, " ", size);
    let lead = measure_text_with_pairs(&font, "unbreakable", size);
    let tail = measure_text_with_pairs(&font, "word", size);
    // No trailing forced break: when every DP edge is infinite the greedy
    // emergency fallback must still flush the dangling leading box as a final
    // (overfull) line.
    let items = vec![
        ParagraphItem::Box(TextBox {
            text: "unbreakable".to_string(),
            runs: StyledText::plain("unbreakable"),
            width: lead,
        }),
        ParagraphItem::Glue(default_interword_glue(space)),
        ParagraphItem::Box(TextBox {
            text: "word".to_string(),
            runs: StyledText::plain("word"),
            width: tail,
        }),
    ];
    // Column narrower than the first word forces every DP edge infinite.
    let column = LayoutUnit::from_milli_points(lead.milli_points() / 2);
    let breaks = break_paragraph(&items, column);

    assert_eq!(breaks.len(), 1);
    assert_eq!(breaks[0].start, 0);
    assert_eq!(breaks[0].end, 1);
    assert_eq!(breaks[0].natural_width, lead);
    assert!(breaks[0].natural_width > column);
}

#[test]
fn negative_non_forced_penalty_rewards_a_break() {
    let font = serif_font();
    let size = FontSize::from_points(12);
    let head = measure_text_with_pairs(&font, "encouragement", size);
    let tail = measure_text_with_pairs(&font, "go", size);
    assert!(head > tail, "precondition: head word is the wider one");
    // An "encouraged" (negative but not forced) penalty between two welded boxes.
    let items = vec![
        ParagraphItem::Box(TextBox {
            text: "encouragement".to_string(),
            runs: StyledText::plain("encouragement"),
            width: head,
        }),
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: -50,
            flagged: false,
        }),
        ParagraphItem::Box(TextBox {
            text: "go".to_string(),
            runs: StyledText::plain("go"),
            width: tail,
        }),
        ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        }),
    ];
    // Column exactly fits the first box, so breaking at the encouraged penalty is
    // feasible (badness 0) and the negative penalty lowers demerits (exercising
    // the negative-penalty demerit branch).
    let breaks = break_paragraph(&items, head);

    assert_eq!(breaks.len(), 2);
    assert_eq!(breaks[0].start, 0);
    assert_eq!(breaks[0].end, 1);
    assert_eq!(breaks[0].natural_width, head);
    assert_eq!(breaks[0].badness, 0);
    assert_eq!(line_text(&items, breaks[1].start, breaks[1].end), "go");
}

#[test]
fn large_paragraph_breaks_in_near_linear_time() {
    // Regression for the O(candidates^2) breaker (a single large paragraph could
    // take tens of seconds). With active-node deactivation this must be fast;
    // we render a big paragraph and a heavily-hyphenating single token and only
    // assert they complete (the test harness times out on regression).
    use franken_markdown::{PdfOptions, render_pdf};
    let big = "word ".repeat(8000);
    let pdf = render_pdf(&big, &PdfOptions::default()).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    let hyph = "communication".repeat(3000);
    let pdf2 = render_pdf(&hyph, &PdfOptions::default()).unwrap();
    assert!(pdf2.starts_with(b"%PDF-"));
}
