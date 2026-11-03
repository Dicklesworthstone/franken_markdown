//! End-to-end display mathematics through the shared parser, real bundled
//! fonts, renderer-neutral paths, and native browser-session wire contract.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{Engine, PathSeg, Placed, Style};
use franken_markdown::dep_invalidation::{
    BrowserFlowSession, FlowAssetReuse, FlowSession, FlowSessionError, FlowShapeCache,
    FlowShapeCacheLimits,
};
use franken_markdown::display::{
    AccessibleReadingRole, DisplayItem, DisplayList, DisplayPath, DisplayPathCommand,
    DisplayQueryMode, DisplayRect, VectorShapeType,
};
use franken_markdown::flow_display::{
    FlowDisplayLimits, FlowLayoutError, FlowLayoutOptions, ResumableFlowDisplay,
};
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

fn points(path: &DisplayPath) -> Vec<(f64, f64)> {
    let mut result = Vec::new();
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

fn assert_path_bounds(path: &DisplayPath) {
    let bounds = path.bounds;
    assert!(bounds.x.is_finite() && bounds.y.is_finite());
    assert!(bounds.width.is_finite() && bounds.width > 0.0);
    assert!(bounds.height.is_finite() && bounds.height > 0.0);
    assert!(matches!(
        path.commands.first(),
        Some(DisplayPathCommand::MoveTo { .. })
    ));
    assert!(matches!(
        path.commands.last(),
        Some(DisplayPathCommand::Close)
    ));
    for (x, y) in points(path) {
        assert!(x >= f64::from(bounds.x) - 0.001);
        assert!(y >= f64::from(bounds.y) - 0.001);
        assert!(x <= f64::from(bounds.right()) + 0.001);
        assert!(y <= f64::from(bounds.bottom()) + 0.001);
    }
    if let Some(fill) = path.fill {
        assert!(
            fill.iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        );
    }
}

#[test]
fn formulas_retain_real_glyph_outlines_rules_radicals_and_drawn_accents() {
    let engine = Engine::bundled().unwrap();
    for formula in [
        r"\frac{1}{2}",
        r"\sqrt{\frac{x^2+1}{y_2}}",
        r"\left(\begin{matrix}a & b \\ c & d \\ e & f\end{matrix}\right)",
        r"\overrightarrow{AB}",
    ] {
        let reference = engine.typeset(formula, Style::Display).unwrap();
        let contours = fmd_math::paths::resolve_paths(&engine, &reference).unwrap();
        let source = format!("$$\n{formula}\n$$\n");
        let state = session(&source, 360.0);
        let expected_span = parse_markdown_spanned(&source).blocks()[0].span;
        let actual = paths(state.display());
        assert!(!actual.is_empty(), "formula stayed plain text: {formula}");
        assert!(state.pending_assets().is_empty());
        assert!(
            state
                .display()
                .items()
                .iter()
                .all(|item| !matches!(item, DisplayItem::Text(_)))
        );

        let mut reference_points = Vec::new();
        let mut quadratic_count = 0;
        for contour in &contours {
            reference_points.push((contour.start.0, -contour.start.1));
            for segment in &contour.segments {
                match *segment {
                    PathSeg::Line { to } => reference_points.push((to.0, -to.1)),
                    PathSeg::Quad { ctrl, to } => {
                        quadratic_count += 1;
                        reference_points.push((ctrl.0, -ctrl.1));
                        reference_points.push((to.0, -to.1));
                    }
                }
            }
        }
        assert!(
            quadratic_count > 0,
            "fixture must exercise actual curved glyph ink"
        );
        if formula.contains("overrightarrow") {
            assert!(
                !reference.paths.is_empty(),
                "fixture must exercise drawn math paths"
            );
        }
        assert_eq!(
            actual
                .iter()
                .flat_map(|path| &path.commands)
                .filter(|command| matches!(command, DisplayPathCommand::QuadTo { .. }))
                .count(),
            quadratic_count,
            "glyph or radical curves disappeared: {formula}"
        );
        assert_eq!(
            actual
                .iter()
                .flat_map(|path| &path.commands)
                .filter(|command| matches!(command, DisplayPathCommand::Close))
                .count(),
            contours.len(),
            "glyph holes, fraction bars or drawn contours disappeared: {formula}"
        );

        let mut actual_points: Vec<_> = actual.iter().flat_map(|path| points(path)).collect();
        assert_eq!(actual_points.len(), reference_points.len());
        let expected_hull = hull(&reference_points);
        let actual_hull = hull(&actual_points);
        let scale = (actual_hull.2 - actual_hull.0) / (expected_hull.2 - expected_hull.0);
        assert!(scale > 0.0 && scale <= f64::from(options(360.0).body_size) + 0.001);
        // Compare the shared engine's actual contour landmarks after one
        // uniform scale and translation, independent of path grouping/order.
        for point in &mut reference_points {
            point.0 = actual_hull.0 + (point.0 - expected_hull.0) * scale;
            point.1 = actual_hull.1 + (point.1 - expected_hull.1) * scale;
        }
        let order = |a: &(f64, f64), b: &(f64, f64)| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1));
        reference_points.sort_by(order);
        actual_points.sort_by(order);
        for (expected, actual) in reference_points.iter().zip(&actual_points) {
            assert!(
                (expected.0 - actual.0).abs() < 0.002,
                "x landmark differs: {formula}"
            );
            assert!(
                (expected.1 - actual.1).abs() < 0.002,
                "y landmark differs: {formula}"
            );
        }
        for path in actual {
            assert_path_bounds(path);
            assert_eq!(path.source_span, expected_span);
        }
        assert!(
            state
                .display()
                .reading_order()
                .iter()
                .any(|node| node.text == formula)
        );
    }
}

#[test]
fn formula_colors_follow_the_shared_engines_paint_order() {
    let formula = r"\textcolor{red}{\frac{1}{\textcolor{blue}{2}}}+\textcolor{green}{x}";
    let engine = Engine::bundled().unwrap();
    let reference = engine.typeset(formula, Style::Display).unwrap();
    let expected: Vec<_> = reference
        .order
        .iter()
        .map(|primitive| {
            let color = match *primitive {
                Placed::Glyph(index) => reference.glyphs[index].color,
                Placed::Rule(index) => reference.rules[index].color,
                Placed::Path(index) => reference.paths[index].color,
            };
            color.map(|color| [color.r as f32, color.g as f32, color.b as f32, 1.0])
        })
        .collect();
    assert!(expected.iter().any(Option::is_some));
    assert!(
        reference
            .order
            .iter()
            .any(|primitive| matches!(primitive, Placed::Rule(_)))
    );
    let state = session(&format!("$$\n{formula}\n$$"), 400.0);
    let actual: Vec<_> = paths(state.display())
        .iter()
        .map(|path| path.fill)
        .collect();
    assert_eq!(
        actual, expected,
        "fraction bar and denominator must retain emission order and color"
    );
}

#[test]
fn nested_math_fits_its_column_and_keeps_enclosing_original_source() {
    let source = concat!(
        "\u{feff}---\r\ntitle: Source\r\n---\r\n# Café\r\n\r\n",
        "> Before.\r\n>\r\n> $$\r\n> \\frac{1}{2}+\\sqrt{x}\r\n> $$\r\n>\r\n> After.\r\n\r\n",
        "1. Item\r\n\r\n   $$\r\n   \\sum_{i=1}^{20} i\r\n   $$\r\n\r\nTail.\r\n"
    );
    let state = session(source, 160.0);
    assert_eq!(state.source(), source);
    let parsed = parse_markdown_spanned(source);
    let expected_spans: Vec<_> = parsed
        .blocks()
        .iter()
        .filter_map(|block| {
            let text = block.span.slice(source).unwrap();
            text.contains("$$").then_some(block.span)
        })
        .collect();
    assert_eq!(expected_spans.len(), 2);
    let math = paths(state.display());
    assert!(!math.is_empty());
    for span in expected_spans {
        let group: Vec<_> = math
            .iter()
            .filter(|path| path.source_span == span)
            .collect();
        assert!(
            !group.is_empty(),
            "nested formula lost its own enclosing block"
        );
        for path in group {
            assert_path_bounds(path);
            assert!(path.bounds.x > 0.0, "container indentation must survive");
            assert!(path.bounds.right() <= 160.001);
        }
    }
    assert!(
        math.iter()
            .all(|path| path.source_span.slice(source).is_some())
    );
    let tail = state
        .display()
        .items()
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text(text) if text.text == "Tail." => Some(text),
            _ => None,
        })
        .unwrap();
    assert!(math.iter().all(|path| path.bounds.bottom() < tail.bounds.y));
    assert!(state.pending_assets().is_empty());
}

#[test]
fn a_first_math_list_child_keeps_exactly_one_marker_and_an_indented_body() {
    for (source, marker, task) in [
        ("- $$\n  x^2\n  $$\n", Some("•"), None),
        ("7. ```math\n   \\frac{1}{2}\n   ```\n", Some("7."), None),
        ("- [x]\n\n  $$\n  x^2\n  $$\n", None, Some(true)),
        ("- [ ]\n\n  $$\n  x^2\n  $$\n", None, Some(false)),
    ] {
        let parsed = parse_markdown_spanned(source);
        assert_eq!(parsed.blocks().len(), 1);
        let block = &parsed.blocks()[0];
        let franken_markdown::ast::Block::List(list) = &block.node else {
            panic!("fixture must be admitted as a list: {source:?}");
        };
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].task, task);
        assert!(
            matches!(
                list.items[0].blocks.first(),
                Some(franken_markdown::ast::Block::MathBlock(_))
            ),
            "fixture must start with actual block math: {source:?}"
        );
        let state = session(source, 160.0);
        let math = paths(state.display());
        assert!(!math.is_empty());
        let marker_bounds = if let Some(marker) = marker {
            let markers: Vec<_> = state
                .display()
                .items()
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Text(text) if text.text == marker => Some(text.bounds),
                    _ => None,
                })
                .collect();
            assert_eq!(
                markers.len(),
                1,
                "list marker omitted or duplicated: {source:?}"
            );
            markers[0]
        } else {
            let boxes: Vec<_> = state
                .display()
                .items()
                .iter()
                .filter_map(|item| match item {
                    DisplayItem::Vector(path) if path.shape == VectorShapeType::CheckboxOutline => {
                        Some(path.bounds)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(boxes.len(), 1, "task box omitted or duplicated");
            let checks = state.display().items().iter().filter(|item| matches!(item, DisplayItem::Vector(path) if path.shape == VectorShapeType::CheckboxCheck)).count();
            assert_eq!(checks, usize::from(task == Some(true)));
            boxes[0]
        };
        for path in math {
            assert_eq!(path.source_span, block.span);
            assert!(path.bounds.x >= marker_bounds.right());
            assert!(
                path.bounds.y >= marker_bounds.bottom(),
                "first math child keeps the established separate marker row"
            );
            assert!(path.bounds.right() <= 160.001);
        }
        let reading = state.display().reading_order();
        assert_eq!(reading.len(), state.engine().blocks().len());
        assert_eq!(reading.len(), 2);
        assert_eq!(reading[0].role, AccessibleReadingRole::ListItem);
        assert_eq!(reading[1].role, AccessibleReadingRole::Math);
        assert_eq!(reading[1].source_span, block.span);
        assert_eq!(state.source(), source);
    }
    // An empty marker followed by a blank line closes the empty item. These
    // original probe forms must not acquire invented nested-math ownership.
    for source in [
        "-\n\n  $$\n  x^2\n  $$\n",
        "7.\n\n   ```math\n   \\frac{1}{2}\n   ```\n",
    ] {
        let parsed = parse_markdown_spanned(source);
        assert_eq!(parsed.blocks().len(), 2);
        let franken_markdown::ast::Block::List(list) = &parsed.blocks()[0].node else {
            panic!("negative control begins with an empty list");
        };
        assert_eq!(list.items.len(), 1);
        assert!(list.items[0].blocks.is_empty());
        let math_block = &parsed.blocks()[1];
        assert!(matches!(
            math_block.node,
            franken_markdown::ast::Block::MathBlock(_)
        ));
        let state = session(source, 160.0);
        let math = paths(state.display());
        assert!(!math.is_empty());
        assert!(math.iter().all(|path| path.source_span == math_block.span));
        assert_eq!(
            state.display().reading_order()[1].source_span,
            math_block.span
        );
        assert_eq!(state.source(), source);
    }
}

#[test]
fn inline_math_and_table_cells_gain_paths_without_changing_literal_code_or_reading_text() {
    let source = concat!(
        "Before **bold** and [link](/safe).\n\n",
        "Inline $x^2$ and $$y$$ remain inline.\n\n",
        "| formula | literal |\n| --- | --- |\n| $x+1$ | `$$x$$` |\n\n",
        "```text\n$$\\frac{1}{2}$$\n```\n\nAfter.\n"
    );
    let state = session(source, 600.0);
    assert!(!paths(state.display()).is_empty());
    assert_eq!(state.source(), source);
    let reading = state.display().reading_order();
    assert!(
        reading
            .iter()
            .any(|node| node.text.contains("Before bold and link."))
    );
    assert!(
        reading
            .iter()
            .any(|node| node.text.contains("Inline x^2 and y remain inline."))
    );
    assert!(
        reading
            .iter()
            .flat_map(|node| &node.children)
            .any(|cell| cell.text == "x+1")
    );
    assert!(
        reading
            .iter()
            .flat_map(|node| &node.children)
            .any(|cell| cell.text == "$$x$$")
    );
    assert!(reading.iter().any(|node| node.text == "$$\\frac{1}{2}$$\n"));
    let literal = reading
        .iter()
        .find(|node| node.text == "$$\\frac{1}{2}$$\n")
        .unwrap();
    assert!(
        paths(state.display())
            .iter()
            .all(|path| path.source_span != literal.source_span)
    );
    let literal_cell = reading
        .iter()
        .flat_map(|node| &node.children)
        .find(|cell| cell.text == "$$x$$")
        .unwrap();
    assert!(paths(state.display()).iter().all(|path| {
        path.bounds.right() <= literal_cell.bounds.x
            || path.bounds.x >= literal_cell.bounds.right()
            || path.bounds.bottom() <= literal_cell.bounds.y
            || path.bounds.y >= literal_cell.bounds.bottom()
    }));
    assert!(
        state
            .display()
            .anchors()
            .any(|anchor| anchor.anchor_id == "/safe")
    );
}

#[test]
fn malformed_and_unsupported_formulas_remain_visible_without_partial_math() {
    let engine = Engine::bundled().unwrap();
    for formula in [r"\frac{a}^2", r"\notARealMathCommand{x}", r"\sqrt[3}"] {
        assert!(
            engine.typeset(formula, Style::Display).is_err(),
            "fallback oracle requires an actual shared-engine rejection"
        );
        let source = format!("Before.\n\n$$\n{formula}\n$$\n\nAfter.");
        let state = session(&source, 400.0);
        assert!(
            paths(state.display()).is_empty(),
            "partial formula published: {formula}"
        );
        assert_eq!(state.source(), source);
        let text: String = state
            .display()
            .items()
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            text.contains(formula),
            "fallback hid original TeX: {text:?}"
        );
        assert!(text.contains("Before.") && text.contains("After."));
        assert!(
            state
                .display()
                .reading_order()
                .iter()
                .any(|node| node.text.contains(formula))
        );
        assert!(state.pending_assets().is_empty());
    }
    // The shared engine deliberately admits unfinished fragments at EOF.
    // They are valid positive controls, not stricter syntax invented by Flow.
    for formula in [r"\frac{1}", r"x^{"] {
        assert!(engine.typeset(formula, Style::Display).is_ok());
        let source = format!("$$\n{formula}\n$$");
        let state = session(&source, 400.0);
        assert!(
            !paths(state.display()).is_empty(),
            "admitted fragment lost its real math: {formula}"
        );
        assert!(
            state
                .display()
                .items()
                .iter()
                .all(|item| !matches!(item, DisplayItem::Text(_)))
        );
        assert!(
            state
                .display()
                .reading_order()
                .iter()
                .any(|node| node.text == formula)
        );
        assert_eq!(state.source(), source);
    }
}

#[test]
fn per_formula_complexity_fallback_keeps_the_complete_formula() {
    let formula = format!("{}x", "x+".repeat(2500));
    let source = format!("$$\n{formula}\n$$\n");
    let state = session(&source, 400.0);
    assert!(
        paths(state.display()).is_empty(),
        "fixture exceeds the bounded vector primitive inventory"
    );
    assert_eq!(state.source(), source);
    assert!(
        state
            .display()
            .reading_order()
            .iter()
            .any(|node| node.text.contains(&formula))
    );
    let text: String = state
        .display()
        .items()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        text.contains(&formula),
        "bounded fallback must not truncate formula source"
    );
}

#[test]
fn resumable_math_batches_produce_identical_geometry_and_reading_order() {
    let source = "Before.\n\n$$\n\\frac{1}{2}\n$$\n\n> $$\n> \\sqrt{x}\n> $$\n\nAfter.";
    let mut fonts = fonts();
    let mut whole = ResumableFlowDisplay::new(source, usize::MAX);
    whole.process_all().unwrap();
    let expected = fonts.render(&whole, options(320.0)).unwrap();
    assert!(!paths(&expected).is_empty());
    for batch in [1, 2, 3, 7] {
        let mut stepped = ResumableFlowDisplay::new(source, batch);
        while let Some(step) = stepped.step().unwrap() {
            assert!(step.blocks.len() <= batch);
        }
        let actual = fonts.render(&stepped, options(320.0)).unwrap();
        assert_eq!(actual.items(), expected.items(), "batch={batch}");
        assert_eq!(
            actual.reading_order(),
            expected.reading_order(),
            "batch={batch}"
        );
        assert_eq!(actual.total_bounds(), expected.total_bounds());
        assert_eq!(stepped.source(), source);
    }
}

#[test]
fn narrow_reflow_scales_math_uniformly_and_restores_identical_wide_geometry() {
    let formula = r"\sum_{i=1}^{100} i^2 = \frac{100(100+1)(2\cdot100+1)}{6}";
    let source = format!("$$\n{formula}\n$$\n\nTail.");
    let mut state = session(&source, 600.0);
    let expected = state.display().clone();
    let original_hull = hull(
        &paths(&expected)
            .iter()
            .flat_map(|path| points(path))
            .collect::<Vec<_>>(),
    );
    let mut fonts = fonts();
    state
        .reflow(options(75.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    let narrowed = paths(state.display());
    assert!(!narrowed.is_empty());
    assert!(
        narrowed
            .iter()
            .all(|path| path.bounds.x >= 0.0 && path.bounds.right() <= 75.001)
    );
    let narrow_hull = hull(
        &narrowed
            .iter()
            .flat_map(|path| points(path))
            .collect::<Vec<_>>(),
    );
    let horizontal = (narrow_hull.2 - narrow_hull.0) / (original_hull.2 - original_hull.0);
    let vertical = (narrow_hull.3 - narrow_hull.1) / (original_hull.3 - original_hull.1);
    assert!(horizontal < 1.0 && (horizontal - vertical).abs() < 0.0001);
    assert_eq!(state.source(), source);
    assert_eq!(state.revision(), 1);
    state
        .reflow(options(600.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_eq!(state.display().items(), expected.items());
    assert_eq!(state.display().reading_order(), expected.reading_order());
}

#[test]
fn global_math_output_budget_refusal_preserves_the_last_complete_session() {
    let source = "$$\n\\frac{1}{2}+\\sqrt{x}\n$$";
    let mut state = session(source, 400.0);
    let expected = state.display().clone();
    assert!(paths(&expected).len() > 1);
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
        let error = state
            .reflow(limited, |text, size, role, style| {
                fonts.shape(text, size, role, style)
            })
            .unwrap_err();
        assert!(
            matches!(
                error,
                FlowSessionError::Layout(FlowLayoutError::BudgetExceeded(_))
            ),
            "unexpected refusal: {error}"
        );
        assert_eq!(state.source(), source);
        assert_eq!(state.revision(), 1);
        assert_eq!(state.layout_revision(), 1);
        assert_eq!(state.display().items(), expected.items());
        assert_eq!(state.display().reading_order(), expected.reading_order());
    }
}

#[test]
fn viewport_pages_retain_every_formula_path_once_in_paint_order() {
    let source = "$$\n\\sqrt{\\frac{1}{2}}\n$$\n\n$$\n\\sum_{i=1}^{4} i\n$$";
    let state = session(source, 400.0);
    let display = state.display();
    assert!(!paths(display).is_empty());
    let bounds = display.total_bounds();
    let viewport = DisplayRect::new(0.0, 0.0, 400.0, bounds.bottom() + 1.0);
    let expected: Vec<_> = display
        .items()
        .iter()
        .enumerate()
        .filter_map(|(index, item)| matches!(item, DisplayItem::Path(_)).then_some(index))
        .collect();
    for limit in [1, 3, 7] {
        let mut after = 0;
        let mut found = Vec::new();
        loop {
            let page = display
                .viewport_page(viewport, after, limit, DisplayQueryMode::Visible)
                .unwrap();
            assert!(page.items.len() <= limit);
            for entry in page.items {
                if matches!(entry.item, DisplayItem::Path(_)) {
                    found.push(entry.index);
                }
            }
            let Some(next) = page.next_index else { break };
            assert!(next > after);
            after = next;
        }
        assert_eq!(
            found, expected,
            "page size {limit} omitted or reordered formula ink"
        );
    }
    let last = paths(display).last().unwrap().bounds;
    let page = display
        .viewport_page(
            DisplayRect::new(0.0, last.y, 400.0, last.height),
            0,
            100,
            DisplayQueryMode::Visible,
        )
        .unwrap();
    assert!(
        page.items
            .iter()
            .any(|entry| matches!(entry.item, DisplayItem::Path(path) if path.bounds == last))
    );
}

#[test]
fn phantom_math_reserves_layout_without_inventing_visible_ink_or_hits() {
    let formula = r"\phantom{\frac{x^2+1}{y}}";
    let reference = Engine::bundled()
        .unwrap()
        .typeset(formula, Style::Display)
        .unwrap();
    assert!(
        reference.glyphs.is_empty() && reference.rules.is_empty() && reference.paths.is_empty()
    );
    assert!(reference.width > 0.0 && reference.height + reference.depth > 0.0);
    let source = format!("$$\n{formula}\n$$");
    let state = session(&source, 400.0);
    assert!(
        state.display().items().is_empty(),
        "phantom must reserve space without fake paint or anchors"
    );
    let bounds = state.display().total_bounds();
    assert!(f64::from(bounds.width) >= reference.width * f64::from(options(400.0).body_size));
    assert!(
        f64::from(bounds.height)
            >= (reference.height + reference.depth) * f64::from(options(400.0).body_size)
    );
    let reading = state.display().reading_order();
    assert_eq!(reading.len(), 1);
    assert_eq!(reading[0].text, formula);
    assert!(reading[0].bounds.bottom() <= bounds.bottom() + 0.001);
    assert!(
        state
            .display()
            .hit_test(
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height / 2.0
            )
            .is_none()
    );
    assert_eq!(state.source(), source);
}

#[test]
fn code_highlighting_preserves_math_ink_and_trailing_phantom_scroll_extent() {
    let source = concat!(
        "```rust\nlet answer = 42;\n```\n\n",
        "```math\n\\frac{1}{2}\n```\n\n",
        "$$\n\\phantom{\\begin{matrix}a \\\\ b \\\\ c\\end{matrix}}\n$$\n"
    );
    let mut state = session(source, 400.0);
    let before = state.display().clone();
    let expected_paths: Vec<_> = paths(&before).into_iter().cloned().collect();
    assert!(
        !expected_paths.is_empty(),
        "the shared fenced-math AST must reach vector reflow"
    );
    assert!(expected_paths.iter().all(|path| {
        path.source_span
            .slice(source)
            .unwrap()
            .starts_with("```math")
    }));
    let last_ink = expected_paths
        .iter()
        .map(|path| path.bounds.bottom())
        .fold(0.0_f32, f32::max);
    assert!(before.total_bounds().bottom() > last_ink + options(400.0).line_height);
    let mut fonts = fonts();
    state
        .set_code_highlighting(true, |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert!(
        state
            .display()
            .items()
            .iter()
            .any(|item| matches!(item, DisplayItem::Text(text) if text.color_role == "tok-kw"))
    );
    assert_eq!(
        paths(state.display())
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
        expected_paths
    );
    assert_eq!(state.display().reading_order(), before.reading_order());
    assert_eq!(
        state.display().total_bounds(),
        before.total_bounds(),
        "syntax paint must retain invisible measured math extents"
    );
    assert_eq!(state.source(), source);
    assert_eq!(state.revision(), 1);
    assert_eq!(state.layout_revision(), 2);
}

const BROWSER_SOURCE: &str = concat!(
    "$$\n\\textcolor{red}{\\frac{1}{2}} + \\sqrt{x^2+1} + \\sum_{i=1}^{n} i\n$$\n\n",
    "$$\n\\left(\\begin{matrix}a & b \\\\ c & d \\\\ e & f\\end{matrix}\\right)\n$$\n"
);

fn browser_payload(state: &BrowserFlowSession, source: &str) -> String {
    let native = session(source, 400.0);
    assert!(native.display().items().len() < 256);
    let revision = state.revision();
    let layout_revision = state.layout_revision();
    let viewport = DisplayRect::new(0.0, 0.0, 400.0, 600.0);
    let snapshot = state
        .snapshot_json(revision, layout_revision, 0, 2048, true)
        .unwrap();
    let visible = state
        .viewport_json(revision, layout_revision, viewport, 0, 256, true)
        .unwrap();
    let reading = state
        .reading_json(revision, layout_revision, 0, 2048)
        .unwrap();
    let point = paths(native.display())[0].bounds;
    let x = point.x + point.width / 2.0;
    let y = point.y + point.height / 2.0;
    let hit = state
        .hit_test_json(revision, layout_revision, x, y)
        .unwrap();
    // Fixtures are ASCII source; Rust's Debug escaping is valid JSON for this
    // fixed source vocabulary. All display/reading/hit payloads are emitted by
    // the production codec, without rewriting or fabricating primitives.
    assert!(source.is_ascii());
    format!(
        "{{\"source\":{source:?},\"snapshot\":{snapshot},\"viewport\":{visible},\"viewportRequest\":{{\"x\":0,\"y\":0,\"width\":400,\"height\":600,\"afterIndex\":0,\"limit\":256,\"glyphs\":true}},\"reading\":{reading},\"hits\":[{{\"x\":{x},\"y\":{y},\"response\":{hit}}}]}}"
    )
}

#[test]
fn native_browser_wire_exposes_math_paths_reading_and_source_only_hits() {
    let mut state = BrowserFlowSession::new(BROWSER_SOURCE, "sans", options(400.0)).unwrap();
    let native = session(BROWSER_SOURCE, 400.0);
    assert_eq!(state.source(), BROWSER_SOURCE);
    let snapshot = state.snapshot_json(1, 1, 0, 2048, true).unwrap();
    assert!(snapshot.contains("\"kind\":\"path\""));
    assert!(snapshot.contains("\"commands\":["));
    assert!(!snapshot.contains("\"kind\":\"text\""));
    let reading = state.reading_json(1, 1, 0, 2048).unwrap();
    assert!(reading.contains("\\\\frac") && reading.contains("\\\\sqrt"));
    assert!(
        state
            .pending_assets_json(1, 0, 2048)
            .unwrap()
            .contains("\"requests\":[]")
    );
    for path in paths(native.display()).iter().step_by(3) {
        let hit = state
            .hit_test_json(
                1,
                1,
                path.bounds.x + path.bounds.width / 2.0,
                path.bounds.y + path.bounds.height / 2.0,
            )
            .unwrap();
        assert!(
            hit.contains("\"enclosingSourceSpan\":"),
            "formula cannot navigate to source: {hit}"
        );
        assert!(
            !hit.contains("\"fragment-utf16\""),
            "vector ink must not invent text caret offsets"
        );
        assert!(hit.contains("\"linkTarget\":null"));
        assert_eq!(
            state.copy_source(1, path.source_span).unwrap(),
            path.source_span.slice(BROWSER_SOURCE).unwrap()
        );
    }
    assert_eq!(
        state
            .copy_source(1, SourceSpan::new(0, BROWSER_SOURCE.len()))
            .unwrap(),
        BROWSER_SOURCE
    );

    if let Some(path) = std::env::var_os("FMD_FLOW_MATH_FIXTURE") {
        let original = browser_payload(&state, BROWSER_SOURCE);
        let updated_source = "$$\n\\textcolor{blue}{\\sqrt{\\frac{a^2+b^2}{c}}}\n$$\n";
        state
            .replace_source(1, updated_source, FlowAssetReuse::Invalidate)
            .unwrap();
        let updated = browser_payload(&state, updated_source);
        assert!(state.snapshot_json(1, 1, 0, 2048, true).is_err());
        let artifact = format!(
            "{{\"schema\":\"fmd-flow-math-native-v1\",\"original\":{original},\"updated\":{updated}}}\n"
        );
        std::fs::write(path, artifact).unwrap();
    }
}
