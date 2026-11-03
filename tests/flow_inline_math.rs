//! Independent acceptance of mixed prose and mathematics through the shared
//! Markdown AST, real bundled fonts, Flow layout, and browser-session wire.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use fmd_math::{Engine, PathSeg, Style};
use franken_markdown::dep_invalidation::{
    BrowserFlowSession, FlowAssetReuse, FlowSession, FlowSessionError, FlowShapeCache,
    FlowShapeCacheLimits,
};
use franken_markdown::display::{
    AccessibleReadingRole, DisplayItem, DisplayList, DisplayPath, DisplayPathCommand,
    DisplayQueryMode, DisplayRect, DisplayTextRun,
};
use franken_markdown::flow_display::{
    DisplayBlock, FlowDisplayLimits, FlowInlineStyle, FlowLayoutError, FlowLayoutOptions,
    FlowMathStyle, FlowTextRole, ResumableFlowDisplay,
};
use franken_markdown::fonts::BundledFlowFonts;
use franken_markdown::{FontFamily, SourceSpan, parse_markdown_spanned};

fn options(width: f32) -> FlowLayoutOptions {
    FlowLayoutOptions {
        viewport_width: width,
        ..FlowLayoutOptions::default()
    }
}

fn fonts() -> FlowShapeCache {
    FlowShapeCache::new(FontFamily::Sans, FlowShapeCacheLimits::default()).unwrap()
}

fn session(source: &str, width: f32) -> FlowSession {
    let mut fonts = fonts();
    FlowSession::new(
        source,
        64,
        FlowDisplayLimits::default(),
        options(width),
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap()
}

fn paths(display: &DisplayList) -> Vec<&DisplayPath> {
    display
        .items()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Path(path) => Some(path),
            _ => None,
        })
        .collect()
}

fn texts(display: &DisplayList) -> Vec<&DisplayTextRun> {
    display
        .items()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn text<'a>(display: &'a DisplayList, needle: &str) -> &'a DisplayTextRun {
    texts(display)
        .into_iter()
        .find(|text| text.text.contains(needle))
        .unwrap_or_else(|| panic!("missing text {needle:?}"))
}

fn points(paths: &[&DisplayPath]) -> Vec<(f64, f64)> {
    let mut result = Vec::new();
    for path in paths {
        for command in &path.commands {
            match *command {
                DisplayPathCommand::MoveTo { x, y } | DisplayPathCommand::LineTo { x, y } => {
                    result.push((f64::from(x), f64::from(y)));
                }
                DisplayPathCommand::QuadTo { cx, cy, x, y } => {
                    result.push((f64::from(cx), f64::from(cy)));
                    result.push((f64::from(x), f64::from(y)));
                }
                DisplayPathCommand::CubicTo {
                    cx1,
                    cy1,
                    cx2,
                    cy2,
                    x,
                    y,
                } => {
                    result.push((f64::from(cx1), f64::from(cy1)));
                    result.push((f64::from(cx2), f64::from(cy2)));
                    result.push((f64::from(x), f64::from(y)));
                }
                DisplayPathCommand::Close => {}
            }
        }
    }
    result
}

fn hull(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    assert!(!points.is_empty());
    points.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |(left, top, right, bottom), &(x, y)| {
            assert!(x.is_finite() && y.is_finite());
            (left.min(x), top.min(y), right.max(x), bottom.max(y))
        },
    )
}

fn engine_points(engine: &Engine, formula: &str, style: Style) -> Vec<(f64, f64)> {
    let layout = engine.typeset(formula, style).unwrap();
    let mut result = Vec::new();
    for contour in fmd_math::paths::resolve_paths(engine, &layout).unwrap() {
        result.push(contour.start);
        for segment in contour.segments {
            match segment {
                PathSeg::Line { to } => result.push(to),
                PathSeg::Quad { ctrl, to } => {
                    result.push(ctrl);
                    result.push(to);
                }
            }
        }
    }
    result
}

fn assert_formula_baseline(
    engine: &Engine,
    formula: &str,
    style: Style,
    actual: &[&DisplayPath],
    size: f32,
    baseline: f32,
) {
    let mut expected = engine_points(engine, formula, style);
    let mut actual = points(actual);
    assert_eq!(actual.len(), expected.len(), "lost real formula contours");
    let offset_x = hull(&actual).0 - hull(&expected).0 * f64::from(size);
    for point in &mut expected {
        point.0 = offset_x + point.0 * f64::from(size);
        point.1 = f64::from(baseline) - point.1 * f64::from(size);
    }
    let order = |a: &(f64, f64), b: &(f64, f64)| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1));
    expected.sort_by(order);
    actual.sort_by(order);
    for (expected, actual) in expected.iter().zip(&actual) {
        assert!(
            (expected.0 - actual.0).abs() < 0.003,
            "math x landmark differs"
        );
        assert!(
            (expected.1 - actual.1).abs() < 0.003,
            "formula does not share the real prose baseline: {expected:?} != {actual:?}"
        );
    }
}

#[test]
fn inline_formulas_retain_real_engine_geometry_on_the_prose_baseline() {
    let engine = Engine::bundled().unwrap();
    for (delimiter, style) in [("$", Style::Text), ("$$", Style::Display)] {
        for formula in [
            r"x_2^3",
            r"\frac{1}{2}",
            r"\sqrt{x^2+1}",
            r"\sum_{i=1}^{n} i",
        ] {
            let source = format!("Before {delimiter}{formula}{delimiter} after.");
            let state = session(&source, 800.0);
            let before = text(state.display(), "Before");
            let after = text(state.display(), "after.");
            let baseline = before
                .baseline_y
                .expect("mixed text needs explicit baseline");
            assert_eq!(after.baseline_y, Some(baseline));
            assert!(
                before
                    .font_run
                    .as_ref()
                    .unwrap()
                    .vertical_metrics
                    .unwrap()
                    .is_valid()
            );
            assert_formula_baseline(
                &engine,
                formula,
                style,
                &paths(state.display()),
                before.font_size,
                baseline,
            );
            let bounds = hull(&points(&paths(state.display())));
            assert!(bounds.0 >= f64::from(before.bounds.right()) - 0.001);
            assert!(bounds.2 <= f64::from(after.bounds.x) + 0.001);
            assert!(
                texts(state.display())
                    .iter()
                    .all(|item| item.math_fallback.is_none())
            );
            assert_eq!(state.display().reading_order().len(), 1);
            assert_eq!(
                state.display().reading_order()[0].text,
                format!("Before {formula} after.")
            );
            let span = parse_markdown_spanned(&source).blocks()[0].span;
            assert!(
                paths(state.display())
                    .iter()
                    .all(|path| path.source_span == span)
            );
            assert_eq!(state.source(), source);
            assert!(state.pending_assets().is_empty());
        }
    }
}

#[test]
fn math_ranges_remain_atomic_through_styles_links_and_multiline_source() {
    let formula = "\\frac{1+\n2}{3}";
    let source = format!("**Bold *${formula}$*** [$x$](/safe) $y$ $$z$$ `$literal$`.");
    let mut engine = ResumableFlowDisplay::new(&source, 1);
    engine.process_all().unwrap();
    let DisplayBlock::Paragraph { text: reading } = &engine.blocks()[0] else {
        panic!("paragraph");
    };
    let math: Vec<_> = engine
        .inline_runs_for_block(0)
        .unwrap()
        .iter()
        .filter(|run| run.math.is_some())
        .collect();
    assert_eq!(
        math.len(),
        4,
        "adjacent formulas must not coalesce into prose"
    );
    assert_eq!(&reading[math[0].range.clone()], formula);
    assert!(math[0].style.bold && math[0].style.italic);
    assert_eq!(math[0].math, Some(FlowMathStyle::Text));
    assert_eq!(&reading[math[1].range.clone()], "x");
    assert_eq!(math[1].active_link_target(), Some("/safe"));
    assert_eq!(&reading[math[2].range.clone()], "y");
    assert_eq!(math[3].math, Some(FlowMathStyle::Display));
    assert!(
        engine
            .inline_runs_for_block(0)
            .unwrap()
            .iter()
            .any(|run| run.style.code
                && run.math.is_none()
                && &reading[run.range.clone()] == "$literal$")
    );
    let state = session(&source, 800.0);
    assert_eq!(state.display().reading_order()[0].text, *reading);
    let baselines: Vec<_> = texts(state.display())
        .iter()
        .filter_map(|item| item.baseline_y)
        .collect();
    assert!(!baselines.is_empty());
    assert!(
        baselines.iter().all(|value| *value == baselines[0]),
        "math-internal newline became a paragraph break"
    );
    assert_eq!(state.source(), source);
}

#[test]
fn tall_inline_math_expands_only_its_line_and_does_not_overlap_neighboring_prose() {
    let formula = r"\frac{1}{\frac{2}{\frac{3}{4}}}";
    let source = format!("TOP  \nBefore ${formula}$ after.  \nBOTTOM");
    let state = session(&source, 500.0);
    let ink = hull(&points(&paths(state.display())));
    let before = text(state.display(), "Before");
    let after = text(state.display(), "after.");
    let top = text(state.display(), "TOP");
    let bottom = text(state.display(), "BOTTOM");
    assert_eq!(before.baseline_y, after.baseline_y);
    assert!(
        ink.1 >= f64::from(top.bounds.bottom()) - 0.001,
        "fraction overlaps preceding line"
    );
    assert!(
        ink.3 <= f64::from(bottom.bounds.y) + 0.001,
        "fraction overlaps following line"
    );
    assert!(bottom.bounds.y >= before.bounds.bottom() - 0.001);
    let plain = session("TOP  \nBefore x after.  \nBOTTOM", 500.0);
    assert!(state.display().total_bounds().height > plain.display().total_bounds().height);
    assert_eq!(state.display().reading_order().len(), 1);
}

#[test]
fn formulas_move_as_atoms_and_shrink_only_when_wider_than_the_entire_column() {
    let formula = r"\frac{a+b+c+d+e+f}{g+h+i+j+k+l}";
    let source = format!("Some ordinary words before ${formula}$ after.");
    let mut state = session(&source, 800.0);
    let wide = state.display().clone();
    let wide_ink = hull(&points(&paths(&wide)));
    let natural_width = (wide_ink.2 - wide_ink.0) as f32;
    let middle_width = natural_width + options(800.0).body_size * 2.0;
    let mut fonts = fonts();
    state
        .reflow(options(middle_width), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    let middle_ink = hull(&points(&paths(state.display())));
    assert!(
        (middle_ink.2 - middle_ink.0 - (wide_ink.2 - wide_ink.0)).abs() < 0.003,
        "formula shrank just to fill leftover prose space"
    );
    assert!((middle_ink.3 - middle_ink.1 - (wide_ink.3 - wide_ink.1)).abs() < 0.003);
    assert!(
        middle_ink.1 > wide_ink.1,
        "fixture must force an atomic move to a later line"
    );
    let narrow_width = (natural_width * 0.6).max(40.0);
    state
        .reflow(options(narrow_width), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    let narrow_ink = hull(&points(&paths(state.display())));
    let scale_x = (narrow_ink.2 - narrow_ink.0) / (wide_ink.2 - wide_ink.0);
    let scale_y = (narrow_ink.3 - narrow_ink.1) / (wide_ink.3 - wide_ink.1);
    assert!(scale_x < 1.0 && (scale_x - scale_y).abs() < 0.0001);
    assert!(
        paths(state.display())
            .iter()
            .all(|path| path.bounds.x >= -0.001 && path.bounds.right() <= narrow_width + 0.001)
    );
    assert_eq!(paths(state.display()).len(), paths(&wide).len());
    state
        .reflow(options(800.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_eq!(state.display().items(), wide.items());
    assert_eq!(state.display().reading_order(), wide.reading_order());
    assert_eq!(state.source(), source);
}

#[test]
fn table_math_is_real_ink_inside_its_cell_and_tall_rows_keep_following_rows_clear() {
    let source = concat!(
        "| Formula $x^2$ | Result |\n| --- | --- |\n",
        "| before $\\frac{1}{\\frac{2}{3}}$ after | companion |\n",
        "| second $\\sqrt{y}$ | end |\n\nTail."
    );
    for width in [600.0, 220.0] {
        let state = session(source, width);
        let reading = state.display().reading_order();
        let rows: Vec<_> = reading
            .iter()
            .filter(|node| {
                matches!(
                    node.role,
                    AccessibleReadingRole::TableHeaderRow | AccessibleReadingRole::TableRow
                )
            })
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[1].children[0].text,
            r"before \frac{1}{\frac{2}{3}} after"
        );
        assert!(rows[1].bounds.bottom() <= rows[2].bounds.y + 0.001);
        assert!(rows[2].bounds.bottom() <= text(state.display(), "Tail.").bounds.y + 0.001);
        for row in &rows {
            let cell = &row.children[0];
            let cell_paths: Vec<_> = paths(state.display())
                .into_iter()
                .filter(|path| {
                    path.bounds.y >= cell.bounds.y - 0.001
                        && path.bounds.bottom() <= cell.bounds.bottom() + 0.001
                })
                .collect();
            assert!(
                !cell_paths.is_empty(),
                "table formula stayed literal at width {width}"
            );
            assert!(
                cell_paths
                    .iter()
                    .all(|path| path.bounds.x >= cell.bounds.x - 0.001
                        && path.bounds.right() <= cell.bounds.right() + 0.001)
            );
        }
        assert!(
            !texts(state.display())
                .iter()
                .any(|item| item.text.contains("\\frac") || item.text.contains("\\sqrt"))
        );
        assert!(state.pending_assets().is_empty());
        assert_eq!(state.source(), source);
    }
}

#[test]
fn nested_math_uses_truthful_container_spans_and_keeps_markers_and_prose() {
    let source =
        "# Heading $x^2$\r\n\r\n> Quote $\\sqrt{x}$ tail.\r\n\r\n- List $\\frac{1}{2}$ tail.\r\n";
    let state = session(source, 450.0);
    let parsed = parse_markdown_spanned(source);
    assert_eq!(parsed.blocks().len(), 3);
    for block in parsed.blocks() {
        let owned: Vec<_> = paths(state.display())
            .into_iter()
            .filter(|path| path.source_span == block.span)
            .collect();
        assert!(
            !owned.is_empty(),
            "nested inline math did not reach the shared display"
        );
        assert_eq!(block.span.slice(state.source()), block.span.slice(source));
    }
    // Marker is a shaping role; its established ink role is ordinary text.
    // Compare a real non-math list, and assert the bullet itself and gutter
    // geometry rather than inventing a new display color category.
    let plain = session(
        "# Heading x\r\n\r\n> Quote x tail.\r\n\r\n- List x tail.\r\n",
        450.0,
    );
    let markers: Vec<_> = texts(state.display())
        .into_iter()
        .filter(|text| text.text == "•")
        .collect();
    assert_eq!(markers.len(), 1, "list bullet omitted or duplicated");
    let marker = markers[0];
    let plain_marker = text(plain.display(), "•");
    assert_eq!(marker.color_role, plain_marker.color_role);
    assert_eq!(marker.font_run, plain_marker.font_run);
    assert_eq!(marker.bounds.x, plain_marker.bounds.x);
    assert_eq!(marker.bounds.width, plain_marker.bounds.width);
    assert_eq!(marker.source_span, parsed.blocks()[2].span);
    assert!(marker.bounds.right() < text(state.display(), "List").bounds.x);
    assert_eq!(
        text(state.display(), "List").bounds.x,
        text(plain.display(), "List").bounds.x
    );
    assert!(text(state.display(), "Quote").bounds.x > text(state.display(), "Heading").bounds.x);
    assert!(text(state.display(), "List").bounds.x > text(state.display(), "Heading").bounds.x);
    assert_eq!(state.source(), source);
}

#[test]
fn code_escaped_delimiters_and_ordinary_prose_remain_literal_without_math_metadata() {
    let source = "Literal `$x^2$` and \\$x^2\\$ with **bold** and [link](/safe).\n\n```text\n$\\frac{1}{2}$\n```";
    let state = session(source, 600.0);
    assert!(paths(state.display()).is_empty());
    assert!(
        texts(state.display())
            .iter()
            .all(|text| text.math_fallback.is_none())
    );
    // A measured text baseline is now shared by ordinary and math-bearing
    // lines. It does not turn literal code or escaped delimiters into math.
    assert!(
        texts(state.display())
            .iter()
            .all(|text| { text.baseline_y.is_some() && text.has_valid_placement() })
    );
    assert!(
        state.display().reading_order()[0]
            .text
            .contains("$x^2$ and $x^2$")
    );
    assert_eq!(state.display().reading_order()[1].text, "$\\frac{1}{2}$\n");
    assert!(
        state
            .display()
            .anchors()
            .any(|anchor| anchor.anchor_id == "/safe")
    );
    for index in 0..state.engine().blocks().len() {
        assert!(
            state
                .engine()
                .inline_runs_for_block(index)
                .unwrap()
                .iter()
                .all(|run| run.math.is_none())
        );
    }
    assert_eq!(state.source(), source);
}

#[test]
fn unsupported_or_unmeasured_inline_formulas_preserve_complete_visible_source() {
    let formula = r"\notARealMathCommand{x}";
    assert!(
        Engine::bundled()
            .unwrap()
            .typeset(formula, Style::Text)
            .is_err()
    );
    let source = format!("Before ${formula}$ after.");
    let state = session(&source, 130.0);
    assert!(paths(state.display()).is_empty());
    let fallback: Vec<_> = texts(state.display())
        .into_iter()
        .filter(|text| text.math_fallback.is_some())
        .collect();
    assert!(!fallback.is_empty());
    assert_eq!(
        fallback
            .iter()
            .map(|text| text.text.as_str())
            .collect::<String>(),
        formula
    );
    assert!(fallback.iter().all(|text| {
        text.math_fallback
            .as_ref()
            .is_some_and(|reason| !reason.is_empty() && reason.chars().count() <= 160)
    }));
    assert_eq!(
        state.display().reading_order()[0].text,
        format!("Before {formula} after.")
    );
    assert_eq!(state.source(), source);

    let formula = r"\frac{1}{2}";
    let source = format!("Before ${formula}$ after.");
    let mut fonts = fonts();
    let unmeasured = FlowSession::new(
        &source,
        8,
        FlowDisplayLimits::default(),
        options(300.0),
        |text, size, role, style| {
            let mut shaped = fonts.shape(text, size, role, style)?;
            shaped.vertical_metrics = None;
            Ok(shaped)
        },
    )
    .unwrap();
    assert!(
        paths(unmeasured.display()).is_empty(),
        "missing host metrics must not invent a math baseline"
    );
    let fallback: String = texts(unmeasured.display())
        .into_iter()
        .filter(|text| text.math_fallback.is_some())
        .map(|text| text.text.as_str())
        .collect();
    assert_eq!(fallback, formula);
    assert_eq!(unmeasured.source(), source);
}

#[test]
fn large_formula_fallback_is_complete_and_does_not_publish_partial_ink() {
    let formula = format!("{}x", "x+".repeat(2500));
    let source = format!("Before ${formula}$ after.");
    let state = session(&source, 300.0);
    assert!(paths(state.display()).is_empty());
    let fallback: String = texts(state.display())
        .into_iter()
        .filter(|text| text.math_fallback.is_some())
        .map(|text| text.text.as_str())
        .collect();
    assert_eq!(
        fallback, formula,
        "complexity fallback dropped equation source"
    );
    assert_eq!(
        state.display().reading_order()[0].text,
        format!("Before {formula} after.")
    );
    assert_eq!(state.source(), source);
}

#[test]
fn mixed_layout_budgets_and_failed_edits_preserve_the_last_complete_publication() {
    let source = "Before $\\sqrt{\\frac{1}{2}}$ after.";
    let mut state = session(source, 400.0);
    let expected = state.display().clone();
    assert!(!paths(&expected).is_empty());
    let mut fonts = fonts();
    for limited in [
        FlowLayoutOptions {
            max_items: 1,
            ..options(400.0)
        },
        FlowLayoutOptions {
            max_total_shape_bytes: 1,
            ..options(400.0)
        },
    ] {
        assert!(matches!(
            state.reflow(limited, |text, size, role, style| fonts
                .shape(text, size, role, style)),
            Err(FlowSessionError::Layout(FlowLayoutError::BudgetExceeded(_)))
        ));
        assert_eq!(state.revision(), 1);
        assert_eq!(state.layout_revision(), 1);
        assert_eq!(state.display().items(), expected.items());
        assert_eq!(state.display().reading_order(), expected.reading_order());
        assert_eq!(state.source(), source);
    }
    let failed = state.replace_source(
        1,
        "Changed $x^2$ prose.",
        FlowAssetReuse::Invalidate,
        |_, _, _, _| Err("host refused shaping".to_owned()),
    );
    assert!(matches!(
        failed,
        Err(FlowSessionError::Layout(FlowLayoutError::Shaping(_)))
    ));
    assert_eq!(state.display().items(), expected.items());
    assert_eq!(state.source(), source);
    assert_eq!(state.revision(), 1);
    assert_eq!(state.layout_revision(), 1);
}

#[test]
fn batch_sizes_and_formula_edits_preserve_exact_source_and_deterministic_geometry() {
    let source = "Before $x^2$ after.\n\n> Nested $\\sqrt{y}$ prose.\n\n| math |\n| --- |\n| $\\frac{1}{2}$ |";
    let mut whole = ResumableFlowDisplay::new(source, usize::MAX);
    whole.process_all().unwrap();
    let mut fonts = fonts();
    let expected = fonts.render(&whole, options(350.0)).unwrap();
    for batch in [1, 2, 5] {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        let actual = fonts.render(&engine, options(350.0)).unwrap();
        assert_eq!(actual.items(), expected.items());
        assert_eq!(actual.reading_order(), expected.reading_order());
        assert_eq!(actual.total_bounds(), expected.total_bounds());
    }
    let mut state = session(source, 350.0);
    let start = source.find("x^2").unwrap();
    let replacement = r"\frac{x}{3}";
    state
        .edit(
            1,
            SourceSpan::new(start, start + 3),
            replacement,
            FlowAssetReuse::Invalidate,
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
    let edited = source.replacen("x^2", replacement, 1);
    assert_eq!(state.source(), edited);
    assert_eq!(state.revision(), 2);
    let fresh = session(&edited, 350.0);
    assert_eq!(state.display().items(), fresh.display().items());
    assert_eq!(
        state.display().reading_order(),
        fresh.display().reading_order()
    );
}

#[test]
fn bundled_metrics_enclose_real_positioned_glyphs_across_styles_and_faces() {
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let fonts = BundledFlowFonts::new(family).unwrap();
        for style in [
            FlowInlineStyle::default(),
            FlowInlineStyle {
                bold: true,
                italic: true,
                ..FlowInlineStyle::default()
            },
        ] {
            let run = fonts
                .shape("AV office e\u{301} →", 17.0, FlowTextRole::Body, style)
                .unwrap();
            let metrics = run
                .vertical_metrics
                .expect("bundled shaping must expose real vertical metrics");
            assert!(metrics.is_valid());
            for glyph in &run.glyphs {
                let font = franken_markdown::text::Font::parse(
                    fonts.font_bytes(glyph.font_id).unwrap().to_vec(),
                )
                .unwrap();
                let scale = 17.0 / f64::from(font.units_per_em);
                let outline = font.glyph_outline(glyph.glyph_id).unwrap();
                for contour in &outline.contours {
                    let mut coordinates = vec![contour.start.y];
                    for segment in &contour.segments {
                        coordinates.push(segment.to().y);
                    }
                    for y in coordinates {
                        let placed = y * scale + f64::from(glyph.y_offset);
                        assert!(placed <= f64::from(metrics.ascent) + 0.001);
                        assert!(placed >= -f64::from(metrics.descent) - 0.001);
                    }
                }
            }
        }
    }
}

#[test]
fn bounded_windows_preserve_real_prose_and_table_geometry_with_intermittent_formulas() {
    let words = "office AV café beta plain words ".repeat(16);
    let paragraph_unit = format!("{words}**bold office** $\\frac{{1+\n2}}{{3}}$ *end* ");
    let cell_unit = format!("{words}[**bold office**](#destination) $x^2$ *end* ");
    let paragraph = format!("{}\n\nTail.", paragraph_unit.repeat(40));
    let table = format!(
        "| ID | Description |\n| --- | --- |\n| 7 | {} |\n\nTail.",
        cell_unit.repeat(40)
    );
    for (name, source, width) in [("paragraph", paragraph, 170.0), ("table", table, 210.0)] {
        assert!(source.len() > FlowLayoutOptions::default().max_shape_bytes);
        let large_window = source.len();
        let mut fonts = fonts();
        let expected = FlowSession::new(
            &source,
            64,
            FlowDisplayLimits::default(),
            FlowLayoutOptions {
                max_shape_bytes: large_window,
                ..options(width)
            },
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
        assert!(!paths(expected.display()).is_empty());
        for window in [64, 128, FlowLayoutOptions::default().max_shape_bytes] {
            let mut charged = 0usize;
            let mut calls = 0usize;
            let actual = FlowSession::new(
                &source,
                3,
                FlowDisplayLimits::default(),
                FlowLayoutOptions {
                    max_shape_bytes: window,
                    ..options(width)
                },
                |text, size, role, style| {
                    assert!(text.len() <= window, "{name}: unbounded callback");
                    charged += text.len();
                    calls += 1;
                    fonts.shape(text, size, role, style)
                },
            )
            .unwrap_or_else(|error| panic!("{name}, window={window}: {error}"));
            assert!(calls > 0 && charged >= words.len());
            assert_eq!(actual.source(), source);
            assert!(
                actual.display().reading_order() == expected.display().reading_order(),
                "{name}, window={window}: scratch boundary changed reading geometry"
            );
            assert_eq!(
                actual.display().items().len(),
                expected.display().items().len(),
                "{name}, window={window}: scratch boundary changed final fragments"
            );
            for (index, (actual, expected)) in actual
                .display()
                .items()
                .iter()
                .zip(expected.display().items())
                .enumerate()
            {
                assert_eq!(
                    actual, expected,
                    "{name}, window={window}, item={index}: scratch boundary changed actual glyph or formula placement"
                );
            }
            assert_eq!(
                actual.display().total_bounds(),
                expected.display().total_bounds()
            );
        }
    }
}

#[test]
fn measured_rtl_prose_requires_bidi_for_math_but_formula_only_needs_no_text_probe() {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    // This explicit custom host presents real bundled Latin glyphs in an RTL
    // run. Its metrics remain those of the actual face; only direction and
    // visual cluster placement are supplied by the host. It is not a claim
    // that bundled Flow has acquired a Hebrew/Arabic or bidi shaper.
    let rtl = |text: &str, size, role, style| {
        let mut run = fonts.shape(text, size, role, style)?;
        assert!(run.vertical_metrics.unwrap().is_valid());
        run.context.direction = franken_markdown::text::Direction::RightToLeft;
        for cluster in &mut run.clusters {
            let left = cluster.x_start;
            cluster.x_start = run.total_advance - cluster.x_end;
            cluster.x_end = run.total_advance - left;
        }
        Ok(run)
    };
    let plain = FlowSession::new(
        "abc def",
        8,
        FlowDisplayLimits::default(),
        options(300.0),
        rtl,
    )
    .unwrap();
    assert!(texts(plain.display()).iter().all(|text| {
        text.font_run.as_ref().unwrap().context.direction
            == franken_markdown::text::Direction::RightToLeft
    }));
    let mixed = FlowSession::new(
        "abc $x^2$ def",
        8,
        FlowDisplayLimits::default(),
        options(300.0),
        rtl,
    );
    assert!(
        matches!(mixed, Err(FlowSessionError::Layout(FlowLayoutError::Shaping(message))) if message.contains("bidi")),
        "mixed RTL mathematics must refuse unsupported ordering explicitly"
    );
    let mut called = false;
    let formula_only = FlowSession::new(
        "$\\frac{1}{2}$",
        8,
        FlowDisplayLimits::default(),
        options(300.0),
        |_, _, _, _| {
            called = true;
            Err("formula-only line must not request invented text metrics".to_owned())
        },
    )
    .unwrap();
    assert!(!called);
    assert!(!paths(formula_only.display()).is_empty());
    assert_eq!(
        formula_only.display().reading_order()[0].text,
        r"\frac{1}{2}"
    );
}

const BROWSER_SOURCE: &str = concat!(
    "Before **bold $\\textcolor{red}{\\frac{1}{2}}$** and [$\\sqrt{x^2+1}$](/guide) after.  \n",
    "Below $\\frac{1}{\\frac{2}{3}}$ end.\n\n",
    "Literal `$x^2$`; unsupported $\\notARealMathCommand{x}$ remains readable.\n\n",
    "| Formula | Text |\n| --- | --- |\n| $\\sqrt{y}$ | cell |"
);

fn browser_payload(state: &BrowserFlowSession, source: &str) -> String {
    let native = session(source, 500.0);
    assert!(native.display().items().len() < 256);
    let revision = state.revision();
    let layout_revision = state.layout_revision();
    let viewport = DisplayRect::new(0.0, 0.0, 500.0, 600.0);
    let snapshot = state
        .snapshot_json(revision, layout_revision, 0, 2048, true)
        .unwrap();
    let visible = state
        .viewport_json(revision, layout_revision, viewport, 0, 256, true)
        .unwrap();
    let reading = state
        .reading_json(revision, layout_revision, 0, 2048)
        .unwrap();
    let mut by_font: BTreeMap<u64, BTreeSet<u16>> = BTreeMap::new();
    for text in texts(native.display()) {
        let run = text.font_run.as_ref().unwrap();
        by_font.entry(run.context.font_id.0).or_default();
        for glyph in &run.glyphs {
            by_font
                .entry(glyph.font_id.0)
                .or_default()
                .insert(glyph.glyph_id);
        }
    }
    let outlines: Vec<_> = by_font
        .into_iter()
        .map(|(font, ids)| {
            let ids: Vec<_> = ids.into_iter().collect();
            state.glyph_outlines_json(font, &ids).unwrap()
        })
        .collect();
    let point = paths(native.display())[0].bounds;
    let x = point.x + point.width / 2.0;
    let y = point.y + point.height / 2.0;
    let hit = state
        .hit_test_json(revision, layout_revision, x, y)
        .unwrap();
    // Fixed ASCII fixture strings use only JSON-compatible Debug escapes;
    // every display, reading, hit, and outline payload comes from production.
    assert!(source.is_ascii());
    format!(
        "{{\"source\":{source:?},\"snapshot\":{snapshot},\"viewport\":{visible},\"viewportRequest\":{{\"x\":0,\"y\":0,\"width\":500,\"height\":600,\"afterIndex\":0,\"limit\":256,\"glyphs\":true}},\"reading\":{reading},\"glyphOutlines\":[{}],\"hits\":[{{\"x\":{x},\"y\":{y},\"response\":{hit}}}]}}",
        outlines.join(",")
    )
}

#[test]
fn browser_wire_keeps_real_baselines_math_links_source_hits_and_glyph_outlines() {
    let mut state = BrowserFlowSession::new(BROWSER_SOURCE, "sans", options(500.0)).unwrap();
    let native = session(BROWSER_SOURCE, 500.0);
    let snapshot = state.snapshot_json(1, 1, 0, 2048, true).unwrap();
    assert!(snapshot.contains("\"kind\":\"path\""));
    assert!(snapshot.contains("\"baselineY\":"));
    assert!(snapshot.contains("\"mathFallback\":"));
    let reading = state.reading_json(1, 1, 0, 2048).unwrap();
    assert!(reading.contains("\\\\frac") && reading.contains("\\\\sqrt"));
    assert_eq!(
        state
            .copy_source(1, SourceSpan::new(0, BROWSER_SOURCE.len()))
            .unwrap(),
        BROWSER_SOURCE
    );
    assert!(
        state
            .pending_assets_json(1, 0, 2048)
            .unwrap()
            .contains("\"requests\":[]")
    );
    for (index, item) in native.display().items().iter().enumerate() {
        let DisplayItem::Path(path) = item else {
            continue;
        };
        let hit = state
            .hit_test_json(
                1,
                1,
                path.bounds.x + path.bounds.width / 2.0,
                path.bounds.y + path.bounds.height / 2.0,
            )
            .unwrap();
        assert!(hit.contains("\"enclosingSourceSpan\":"));
        assert!(
            !hit.contains("\"fragment-utf16\""),
            "formula paths invented a glyph caret: {hit}"
        );
        assert!(state.select_item_json(1, 1, index, 0, 1).is_err());
        assert_eq!(
            state.copy_source(1, path.source_span).unwrap(),
            path.source_span.slice(BROWSER_SOURCE).unwrap()
        );
    }
    let link = native
        .display()
        .anchors()
        .find(|anchor| anchor.anchor_id == "/guide")
        .unwrap();
    let linked_path = paths(native.display())
        .into_iter()
        .find(|path| {
            path.bounds.x >= link.bounds.x
                && path.bounds.right() <= link.bounds.right()
                && path.bounds.y >= link.bounds.y
                && path.bounds.bottom() <= link.bounds.bottom()
        })
        .unwrap();
    let hit = state
        .hit_test_json(
            1,
            1,
            linked_path.bounds.x + linked_path.bounds.width / 2.0,
            linked_path.bounds.y + linked_path.bounds.height / 2.0,
        )
        .unwrap();
    assert!(hit.contains("\"linkTarget\":\"/guide\""));
    let viewport = DisplayRect::new(0.0, 0.0, 500.0, 600.0);
    let mut seen = Vec::new();
    let mut cursor = 0;
    loop {
        let page = native
            .display()
            .viewport_page(viewport, cursor, 3, DisplayQueryMode::Visible)
            .unwrap();
        seen.extend(
            page.items.iter().filter_map(|entry| {
                matches!(entry.item, DisplayItem::Path(_)).then_some(entry.index)
            }),
        );
        let Some(next) = page.next_index else { break };
        cursor = next;
    }
    let expected: Vec<_> = native
        .display()
        .items()
        .iter()
        .enumerate()
        .filter_map(|(index, item)| matches!(item, DisplayItem::Path(_)).then_some(index))
        .collect();
    assert_eq!(seen, expected);
    if let Some(path) = std::env::var_os("FMD_FLOW_INLINE_MATH_FIXTURE") {
        let original = browser_payload(&state, BROWSER_SOURCE);
        let source = "Updated $\\textcolor{blue}{\\sqrt{\\frac{a^2+b^2}{c}}}$ and **bold** prose.\n\nLiteral `$x$`.";
        state
            .replace_source(1, source, FlowAssetReuse::Invalidate)
            .unwrap();
        let updated = browser_payload(&state, source);
        assert!(state.snapshot_json(1, 1, 0, 2048, true).is_err());
        std::fs::write(path, format!("{{\"schema\":\"fmd-flow-inline-math-native-v1\",\"original\":{original},\"updated\":{updated}}}\n")).unwrap();
    }
}
