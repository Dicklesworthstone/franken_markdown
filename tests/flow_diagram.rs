//! Public Flow acceptance for actual compiled diagrams, source ownership,
//! measured labels, vector geometry, and transactional resource limits.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::dep_invalidation::{
    BrowserFlowSession, FlowAssetReuse, FlowSession, FlowSessionError,
};
use franken_markdown::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayItem, DisplayList, DisplayPath,
    DisplayPathCommand, DisplayRect, DisplayTextRun,
};
use franken_markdown::flow_display::{FlowDisplayLimits, FlowLayoutError, FlowLayoutOptions};
use franken_markdown::fonts::{BundledFlowFonts, FontStyle};
use franken_markdown::text::{Font, FontId, outline::Segment};
use franken_markdown::{FontFamily, SourceSpan, parse_markdown_spanned};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const FLOW: &str = concat!(
    "flowchart LR\n",
    "A([Start]) -. Review .-> B{Ready?}\n",
    "B ==> C((Ship))\nC --> A\n",
    "A --- D[\"Literal <tag> & value\"]\n",
);
const SEQUENCE: &str = concat!(
    "sequenceDiagram\nautonumber\nactor Client\nparticipant API as Service\n",
    "Client->>+API: Request & verify\nNote right of API: Keep <raw>\n",
    "API-->>-Client: Done\n",
);
const PIE: &str = concat!(
    "pie showData title Costs\n",
    "\"Engineering\" : 42.25\n\"Operations\" : 31.50\n\"Support\" : 26.25\n",
);
const STATE: &str = concat!(
    "stateDiagram-v2\n[*] --> Idle\n",
    "state \"Waiting for input\" as Idle\n",
    "Idle --> Choice: submit\nstate Choice <<choice>>\n",
    "Choice --> Working: if n >= 0\nChoice --> Idle: retry\n",
    "Working --> [*]: complete\n",
);
const CLASS: &str = concat!(
    "classDiagram\ndirection LR\n",
    "class Document {\n  +String title\n  +render() String\n}\n",
    "class Renderer {\n  <<interface>>\n  +render() String\n}\n",
    "Renderer <|.. HtmlRenderer : implements\n",
    "Document \"1\" *-- \"1\" HtmlRenderer : renders & caches\n",
);
// Keep the unsupported-dialect control when class diagrams become native figures.
const UNSUPPORTED: &str =
    "gantt\ntitle Publication schedule\nsection Rendering\nCompile :2026-01-01, 1d\n";
const ASCII: &str = "+--------+\n| source |\n+--------+\n";

fn options(width: f32) -> FlowLayoutOptions {
    FlowLayoutOptions {
        viewport_width: width,
        ..FlowLayoutOptions::default()
    }
}
fn fence(language: &str, code: &str) -> String {
    assert!(code.ends_with('\n'));
    format!("```{language}\n{code}```\n")
}
fn fixture_source(language: &str, code: &str) -> String {
    // The parser's LF code body and the original document's BOM/CRLF bytes
    // are distinct contracts. Neither is normalized by this fixture adapter.
    format!("\u{feff}```{language}\r\n{code}```\r\n")
}
fn build(
    source: &str,
    options: FlowLayoutOptions,
    batch: usize,
    family: FontFamily,
) -> FlowSession {
    let fonts = BundledFlowFonts::new(family).unwrap();
    FlowSession::new(
        source,
        batch,
        FlowDisplayLimits::default(),
        options,
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap()
}
fn session(source: &str, width: f32) -> FlowSession {
    build(source, options(width), 8, FontFamily::Sans)
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
fn diagram<'a>(display: &'a DisplayList, code: &str) -> &'a AccessibleReadingNode {
    let nodes: Vec<_> = display
        .reading_order()
        .iter()
        .filter(|node| node.role == AccessibleReadingRole::Diagram)
        .collect();
    assert_eq!(nodes.len(), 1, "one complete source transcript per diagram");
    assert_eq!(nodes[0].text, code);
    assert!(
        nodes[0].children.is_empty(),
        "labels must not duplicate the source transcript"
    );
    nodes[0]
}
fn flattened(display: &DisplayList) -> String {
    texts(display)
        .iter()
        .map(|text| text.text.as_str())
        .collect()
}
fn assert_scene_bounds(
    display: &DisplayList,
    source: &str,
    code: &str,
    width: f32,
    family: FontFamily,
) {
    let node = diagram(display, code);
    let bounds = node.bounds;
    assert!(
        bounds.x.is_finite() && bounds.y.is_finite() && bounds.width > 0.0 && bounds.height > 0.0
    );
    assert!(bounds.x >= -0.01 && bounds.right() <= width + 0.01);
    assert!(
        node.source_span
            .slice(source)
            .unwrap()
            .contains(code.lines().next().unwrap())
    );
    let actual = paths(display);
    assert!(!actual.is_empty());
    assert_eq!(
        actual[0].fill,
        Some([1.0; 4]),
        "the figure owns its white backdrop"
    );
    assert!(actual[0].stroke.is_none());
    for path in actual {
        assert!(path.is_valid());
        assert_eq!(path.source_span, node.source_span);
        assert!(path.bounds.x >= bounds.x - 0.01 && path.bounds.right() <= bounds.right() + 0.01);
        assert!(path.bounds.y >= bounds.y - 0.01 && path.bounds.bottom() <= bounds.bottom() + 0.01);
        if let Some(stroke) = &path.stroke {
            assert!(stroke.width.is_finite() && stroke.width > 0.0);
            assert!(
                stroke
                    .dash
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0)
            );
            assert!(
                stroke
                    .color
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            );
        }
    }
    let fonts = BundledFlowFonts::new(family).unwrap();
    let mut decoded = BTreeMap::<u64, Font>::new();
    let mut labels = 0;
    for text in texts(display)
        .into_iter()
        .filter(|text| text.fill.is_some())
    {
        labels += 1;
        assert_eq!(text.source_span, node.source_span);
        let run = text
            .font_run
            .as_ref()
            .expect("diagram labels require real glyph runs");
        assert_eq!(run.logical_text, text.text);
        assert_eq!(run.context.font_size, text.font_size);
        assert!(run.vertical_metrics.unwrap().is_valid());
        let baseline = text
            .baseline_y
            .expect("diagram text owns a measured baseline");
        for cluster in &run.clusters {
            let mut pen = cluster.x_start;
            for glyph in &run.glyphs[cluster.glyph_range.clone()] {
                assert_ne!(glyph.glyph_id, 0);
                let font = decoded.entry(glyph.font_id.0).or_insert_with(|| {
                    Font::parse(fonts.font_bytes(glyph.font_id).unwrap().to_vec()).unwrap()
                });
                let scale = f64::from(text.font_size) / f64::from(font.units_per_em);
                let check = |point: franken_markdown::text::outline::Point| {
                    let x = f64::from(text.bounds.x + pen + glyph.x_offset) + point.x * scale;
                    let y = f64::from(baseline - glyph.y_offset) - point.y * scale;
                    assert!(
                        x >= f64::from(bounds.x) - 0.02 && x <= f64::from(bounds.right()) + 0.02,
                        "real label ink escaped the fitted diagram horizontally: {:?}",
                        text.text
                    );
                    assert!(
                        y >= f64::from(bounds.y) - 0.02 && y <= f64::from(bounds.bottom()) + 0.02,
                        "real label ink escaped the fitted diagram vertically: {:?}",
                        text.text
                    );
                };
                for contour in font.glyph_outline(glyph.glyph_id).unwrap().contours {
                    check(contour.start);
                    for segment in contour.segments {
                        if let Segment::Quad { ctrl, .. } = segment {
                            check(ctrl);
                        }
                        check(segment.to());
                    }
                }
                pen += glyph.x_advance;
            }
        }
    }
    assert!(labels > 0);
}

#[test]
fn flowcharts_keep_real_shapes_arrowheads_dashes_thickness_and_literal_labels() {
    let source = fence("mermaid", FLOW);
    let state = session(&source, 400.0);
    assert_scene_bounds(state.display(), &source, FLOW, 400.0, FontFamily::Sans);
    let labels = texts(state.display());
    for label in ["Start", "Ready?", "Ship", "Review", "Literal <tag> & value"] {
        assert!(
            labels.iter().any(|text| text.text == label),
            "missing actual label {label:?}"
        );
    }
    let drawn = paths(state.display());
    assert!(drawn.iter().any(|path| {
        path.commands
            .iter()
            .any(|op| matches!(op, DisplayPathCommand::CubicTo { .. }))
    }));
    assert!(drawn.iter().any(|path| {
        path.stroke.as_ref().is_some_and(|stroke| {
            stroke.dash.len() == 2 && stroke.dash[0] > 0.0 && stroke.dash[0] == stroke.dash[1]
        })
    }));
    let widths: Vec<_> = drawn
        .iter()
        .filter_map(|path| path.stroke.as_ref().map(|stroke| stroke.width))
        .collect();
    let min = widths.iter().copied().reduce(f32::min).unwrap();
    let max = widths.iter().copied().reduce(f32::max).unwrap();
    assert!(
        max >= min * 1.9,
        "thick connectors must retain distinct stroke width"
    );
    let directed = session(&fence("mermaid", "graph LR\nA[One] --> B[Two]\n"), 600.0);
    let open = session(&fence("mermaid", "graph LR\nA[One] --- B[Two]\n"), 600.0);
    assert_eq!(
        paths(directed.display()).len(),
        paths(open.display()).len() + 1,
        "a real arrowhead must be added to the same open connector"
    );
    assert!(
        paths(directed.display())
            .iter()
            .any(|path| path.fill.is_some()
                && path.stroke.is_none()
                && path.commands.len() == 4
                && matches!(path.commands.last(), Some(DisplayPathCommand::Close)))
    );
    assert!(state.pending_assets().is_empty());
}

#[test]
fn state_machines_keep_native_geometry_labels_and_accessible_source_at_each_width() {
    for language in ["mermaid", "state"] {
        let source = fence(language, STATE);
        for width in [180.0, 400.0, 900.0] {
            let state = session(&source, width);
            assert_scene_bounds(state.display(), &source, STATE, width, FontFamily::Sans);
            let visible = flattened(state.display());
            for label in [
                "Waiting for input",
                "submit",
                "if n >= 0",
                "Working",
                "retry",
                "complete",
            ] {
                assert!(
                    visible.contains(label),
                    "missing state-machine label {label:?}"
                );
            }
            assert!(!paths(state.display()).is_empty());
            assert!(state.pending_assets().is_empty());
            assert_eq!(diagram(state.display(), STATE).text, STATE);
        }
    }
}

#[test]
fn sequence_actors_activation_notes_and_dashed_returns_keep_every_label() {
    let source = fence("mermaid", SEQUENCE);
    let state = session(&source, 400.0);
    assert_scene_bounds(state.display(), &source, SEQUENCE, 400.0, FontFamily::Sans);
    let visible = flattened(state.display());
    for label in [
        "Client",
        "Service",
        "Request & verify",
        "Keep <raw>",
        "Done",
    ] {
        assert!(visible.contains(label), "lost sequence label {label:?}");
    }
    assert!(paths(state.display()).iter().any(|path| {
        path.stroke
            .as_ref()
            .is_some_and(|stroke| !stroke.dash.is_empty())
    }));
    assert!(
        paths(state.display()).iter().any(|path| path.fill.is_none()
            && path
                .commands
                .iter()
                .filter(|op| matches!(op, DisplayPathCommand::MoveTo { .. }))
                .count()
                > 1),
        "actor limbs must remain separate stroked contours"
    );
    assert!(
        paths(state.display()).iter().any(|path| path
            .fill
            .is_some_and(|fill| fill[0] > 0.9 && fill[1] > 0.9 && fill[2] < 0.9)),
        "the note retains its fill"
    );
    assert!(state.pending_assets().is_empty());
}

#[test]
fn pie_wedges_keep_real_curves_colors_and_measured_white_percentage_labels() {
    let source = fence("mermaid", PIE);
    let state = session(&source, 400.0);
    assert_scene_bounds(state.display(), &source, PIE, 400.0, FontFamily::Sans);
    let colors: BTreeSet<_> = paths(state.display())
        .iter()
        .filter_map(|path| path.fill)
        .map(|fill| fill.map(f32::to_bits))
        .collect();
    assert!(
        colors.len() >= 4,
        "three categories plus the white backdrop need distinct paint"
    );
    assert!(paths(state.display()).iter().any(|path| {
        path.commands
            .iter()
            .any(|op| matches!(op, DisplayPathCommand::CubicTo { .. }))
    }));
    let bold = FontId::from_font_data(franken_markdown::fonts::body_bytes(
        FontFamily::Sans,
        FontStyle::Bold,
    ));
    let percentage = texts(state.display())
        .into_iter()
        .find(|text| text.text == "42.3%")
        .unwrap();
    assert_eq!(percentage.fill, Some([1.0; 4]));
    assert_eq!(percentage.font_run.as_ref().unwrap().context.font_id, bold);
    let visible = flattened(state.display());
    for label in [
        "Costs",
        "Engineering",
        "Operations",
        "Support",
        "42.25",
        "31.5",
        "26.25",
    ] {
        assert!(
            visible.contains(label),
            "lost pie category or value {label:?}"
        );
    }
}

#[test]
fn fitted_diagrams_preserve_container_indentation_markers_and_following_block_separation() {
    let code = "graph TD\nA[Widest label WWWWWWWWWWWWWWWWWWWW] --> B[Next]\n";
    let top = fence("mermaid", code);
    let quoted = top
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>();
    let list = format!(
        "- ```mermaid\n{}  ```\n",
        code.lines()
            .map(|line| format!("  {line}\n"))
            .collect::<String>()
    );
    let nested = list
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>();
    for (container, has_marker) in [(top, false), (quoted, false), (list, true), (nested, true)] {
        let source = format!("Before.\n\n{container}\nAfter.");
        let parsed = parse_markdown_spanned(&source);
        assert_eq!(
            parsed.blocks().len(),
            3,
            "fixture must have one container between two paragraphs"
        );
        for family in [FontFamily::Sans, FontFamily::Serif] {
            for width in [500.0, 120.0] {
                let state = build(&source, options(width), 8, family);
                assert_scene_bounds(state.display(), &source, code, width, family);
                let node = diagram(state.display(), code);
                assert_eq!(node.source_span, parsed.blocks()[1].span);
                let before = texts(state.display())
                    .into_iter()
                    .find(|text| text.text == "Before.")
                    .unwrap();
                let after = texts(state.display())
                    .into_iter()
                    .find(|text| text.text == "After.")
                    .unwrap();
                assert!(before.bounds.bottom() <= node.bounds.y + 0.01);
                assert!(
                    after.bounds.y >= node.bounds.bottom() - 0.01,
                    "following prose overlaps diagram ink"
                );
                let markers: Vec<_> = texts(state.display())
                    .into_iter()
                    .filter(|text| text.text == "•")
                    .collect();
                assert_eq!(markers.len(), usize::from(has_marker));
                if has_marker {
                    assert!(markers[0].bounds.x < node.bounds.x);
                }
                assert_eq!(state.source(), source);
            }
        }
    }
}

#[test]
fn unsupported_diagrams_show_complete_source_without_partial_paths_and_ascii_stays_code() {
    for code in [
        UNSUPPORTED,
        "graph TD\nA-->B\nsubgraph omitted\nC-->D\nend\n",
        "%%{init: {}}%%\ngraph TD\nA-->B\n",
    ] {
        let source = fence("mermaid", code);
        let state = session(&source, 400.0);
        assert!(
            paths(state.display()).is_empty(),
            "failed figure published partial paths"
        );
        diagram(state.display(), code);
        let visible = flattened(state.display());
        assert!(visible.contains("Diagram source fallback:"));
        assert!(
            visible.contains(&code.replace('\n', "")),
            "literal fallback lost code: {visible:?}"
        );
        assert_eq!(state.source(), source);
        assert!(state.pending_assets().is_empty());
    }
    for language in ["text", "ascii", "ditaa"] {
        let source = fence(language, ASCII);
        let state = session(&source, 400.0);
        assert!(paths(state.display()).is_empty());
        assert!(!flattened(state.display()).contains("Diagram source fallback:"));
        let reading = state.display().reading_order();
        assert_eq!(reading.len(), 1);
        assert_eq!(reading[0].role, AccessibleReadingRole::CodeBlock);
        assert_eq!(reading[0].text, ASCII);
    }
}

#[test]
fn diagram_global_budgets_are_typed_and_transactional_and_shape_calls_stay_bounded() {
    let source = format!("{}\n{}", fence("mermaid", FLOW), fence("mermaid", PIE));
    for limited in [
        FlowLayoutOptions {
            max_lines: 1,
            ..options(400.0)
        },
        FlowLayoutOptions {
            max_items: 2,
            ..options(400.0)
        },
        FlowLayoutOptions {
            max_total_shape_bytes: 64,
            ..options(400.0)
        },
    ] {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut state = build("Saved", limited, 8, FontFamily::Sans);
        let before = state.display().clone();
        let error = state
            .replace_source(
                1,
                &source,
                FlowAssetReuse::Invalidate,
                |text, size, role, style| fonts.shape(text, size, role, style),
            )
            .unwrap_err();
        assert!(
            matches!(
                error,
                FlowSessionError::Layout(FlowLayoutError::BudgetExceeded(_))
            ),
            "global host exhaustion must remain a typed error: {error}"
        );
        assert_eq!(state.source(), "Saved");
        assert_eq!((state.revision(), state.layout_revision()), (1, 1));
        assert_eq!(state.display().items(), before.items());
        assert_eq!(state.display().reading_order(), before.reading_order());
    }
    let source = fence("mermaid", FLOW);
    let expected = session(&source, 400.0);
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let mut calls = 0;
    let bounded = FlowSession::new(
        &source,
        3,
        FlowDisplayLimits::default(),
        FlowLayoutOptions {
            max_shape_bytes: 64,
            ..options(400.0)
        },
        |text, size, role, style| {
            assert!(
                text.len() <= 64,
                "diagram labels escaped the host shaping-call limit"
            );
            calls += 1;
            fonts.shape(text, size, role, style)
        },
    )
    .unwrap();
    assert!(calls > 0);
    assert_eq!(bounded.display().items(), expected.display().items());
    assert_eq!(
        bounded.display().reading_order(),
        expected.display().reading_order()
    );
}

#[test]
fn source_edits_width_reflow_and_incremental_admission_match_fresh_diagram_output() {
    let source = fixture_source("mermaid", FLOW);
    let mut state = build(&source, options(400.0), 1, FontFamily::Sans);
    let before = state.display().clone();
    assert_eq!(
        before.items(),
        build(&source, options(400.0), 64, FontFamily::Sans)
            .display()
            .items()
    );
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    state
        .reflow(options(120.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_scene_bounds(state.display(), &source, FLOW, 120.0, FontFamily::Sans);
    assert_eq!(state.source(), source);
    state
        .reflow(options(400.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_eq!(state.display().items(), before.items());
    assert_eq!(state.display().reading_order(), before.reading_order());
    let at = source.find("Start").unwrap();
    state
        .edit(
            1,
            SourceSpan::new(at, at + "Start".len()),
            "Launch",
            FlowAssetReuse::Invalidate,
            |text, size, role, style| fonts.shape(text, size, role, style),
        )
        .unwrap();
    let mut edited = source.clone();
    edited.replace_range(at..at + "Start".len(), "Launch");
    assert_eq!(state.source(), edited);
    let fresh = session(&edited, 400.0);
    assert_eq!(state.display().items(), fresh.display().items());
    assert_eq!(
        state.display().reading_order(),
        fresh.display().reading_order()
    );
    assert!(
        state
            .edit(
                1,
                SourceSpan::new(0, 0),
                "stale",
                FlowAssetReuse::Invalidate,
                |text, size, role, style| fonts.shape(text, size, role, style)
            )
            .is_err()
    );
    assert_eq!(state.source(), edited);
    assert_eq!(state.display().items(), fresh.display().items());
}

fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{1f}' => write!(&mut out, "\\u{:04x}", ch as u32).unwrap(),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
fn browser_payload(state: &BrowserFlowSession, name: &str, code: &str, supported: bool) -> String {
    let source = state.source();
    let native = session(source, 400.0);
    assert!(native.display().items().len() < 256);
    let revision = state.revision();
    let layout = state.layout_revision();
    let snapshot = state
        .snapshot_json(revision, layout, 0, 2048, true)
        .unwrap();
    let viewport = state
        .viewport_json(
            revision,
            layout,
            DisplayRect::new(0.0, 0.0, 400.0, 800.0),
            0,
            256,
            true,
        )
        .unwrap();
    let reading = state.reading_json(revision, layout, 0, 2048).unwrap();
    let mut by_font = BTreeMap::<u64, BTreeSet<u16>>::new();
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
            state
                .glyph_outlines_json(font, &ids.into_iter().collect::<Vec<_>>())
                .unwrap()
        })
        .collect();
    let point = paths(native.display())
        .first()
        .map(|path| path.bounds)
        .unwrap_or_else(|| texts(native.display())[0].bounds);
    let x = point.x + point.width.min(1.0) / 2.0;
    let y = point.y + point.height.min(1.0) / 2.0;
    let hit = state.hit_test_json(revision, layout, x, y).unwrap();
    assert_eq!(
        state
            .copy_source(revision, SourceSpan::new(0, source.len()))
            .unwrap(),
        source
    );
    format!(
        concat!(
            "{{\"name\":{},\"supported\":{},\"codeSource\":{},\"source\":{},",
            "\"options\":{{\"font\":\"sans\",\"viewportWidth\":400}},",
            "\"snapshot\":{},\"viewport\":{},\"viewportRequest\":{{\"x\":0,\"y\":0,\"width\":400,\"height\":800,\"afterIndex\":0,\"limit\":256,\"glyphs\":true}},",
            "\"reading\":{},\"glyphOutlines\":[{}],\"hits\":[{{\"x\":{},\"y\":{},\"response\":{}}}]}}"
        ),
        quoted(name),
        supported,
        quoted(code),
        quoted(source),
        snapshot,
        viewport,
        reading,
        outlines.join(","),
        x,
        y,
        hit
    )
}

#[test]
fn browser_wire_preserves_paths_strokes_real_label_outlines_and_source_only_path_selection() {
    let source = fixture_source("mermaid", FLOW);
    let mut state = BrowserFlowSession::new(&source, "sans", options(400.0)).unwrap();
    let native = session(&source, 400.0);
    let snapshot = state.snapshot_json(1, 1, 0, 2048, true).unwrap();
    assert!(
        snapshot.contains("\"kind\":\"path\"")
            && snapshot.contains("\"stroke\":")
            && snapshot.contains("\"dash\":")
    );
    assert!(snapshot.contains("\"fill\":") && snapshot.contains("\"baselineY\":"));
    assert!(
        state
            .reading_json(1, 1, 0, 2048)
            .unwrap()
            .contains("\"role\":\"diagram\"")
    );
    assert_eq!(
        state
            .copy_source(1, SourceSpan::new(0, source.len()))
            .unwrap(),
        source
    );
    assert!(
        state
            .pending_assets_json(1, 0, 2048)
            .unwrap()
            .contains("\"requests\":[]")
    );
    for (index, item) in native.display().items().iter().enumerate() {
        if let DisplayItem::Path(path) = item {
            assert!(
                state.select_item_json(1, 1, index, 0, 1).is_err(),
                "vector path invented a text caret"
            );
            assert_eq!(
                state.copy_source(1, path.source_span).unwrap(),
                path.source_span.slice(&source).unwrap()
            );
        }
    }
    let bounds = paths(native.display())[0].bounds;
    let hit = state
        .hit_test_json(1, 1, bounds.x + 0.5, bounds.y + 0.5)
        .unwrap();
    assert!(hit.contains("\"enclosingSourceSpan\":"));
    assert!(
        !hit.contains("\"fragment-utf16\""),
        "background path invented a glyph caret"
    );
    let label = texts(native.display())
        .into_iter()
        .find(|text| text.fill.is_some())
        .unwrap();
    let run = label.font_run.as_ref().unwrap();
    let ids: Vec<_> = run
        .glyphs
        .iter()
        .map(|glyph| glyph.glyph_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let outlines = state
        .glyph_outlines_json(run.context.font_id.0, &ids)
        .unwrap();
    assert!(outlines.contains("\"commands\":[[") && outlines.contains("\"unitsPerEm\":"));
    if let Some(path) = std::env::var_os("FMD_FLOW_DIAGRAM_FIXTURE") {
        let original = browser_payload(&state, "flowchart", FLOW, true);
        let mut scenes = vec![original.clone()];
        for (name, language, code, supported) in [
            ("class", "mermaid", CLASS, true),
            ("sequence", "mermaid", SEQUENCE, true),
            ("pie", "mermaid", PIE, true),
            ("unsupported", "mermaid", UNSUPPORTED, false),
            ("ascii", "ditaa", ASCII, false),
        ] {
            let scene_source = fixture_source(language, code);
            let scene = BrowserFlowSession::new(&scene_source, "sans", options(400.0)).unwrap();
            scenes.push(browser_payload(&scene, name, code, supported));
        }
        let updated_source = fixture_source("mermaid", SEQUENCE);
        state
            .replace_source(1, &updated_source, FlowAssetReuse::Invalidate)
            .unwrap();
        let updated = browser_payload(&state, "sequence", SEQUENCE, true);
        assert!(state.snapshot_json(1, 1, 0, 2048, true).is_err());
        std::fs::write(path, format!("{{\"schema\":\"fmd-flow-diagram-native-v1\",\"scenes\":[{}],\"original\":{},\"updated\":{}}}\n", scenes.join(","), original, updated)).unwrap();
    }
}

#[test]
fn recognized_diagrams_and_literal_fallback_coexist_with_real_code_highlighting() {
    let rust = "let answer = 42; // retain all ordinary code\n";
    let source = format!(
        "{}\n{}\n{}",
        fence("mermaid", FLOW),
        fence("mermaid", UNSUPPORTED),
        fence("rust", rust)
    );
    let mut engine = franken_markdown::flow_display::ResumableFlowDisplay::new(&source, 8);
    engine.process_all().unwrap();
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    let styled = engine
        .to_styled_display_list(options(400.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert!(
        !paths(&styled).is_empty(),
        "control must contain a real rendered diagram"
    );
    let highlighted = engine
        .to_highlighted_display_list(options(400.0), |text, size, role, style| {
            fonts.shape(text, size, role, style)
        })
        .unwrap();
    assert_eq!(
        paths(&highlighted),
        paths(&styled),
        "syntax paint changed diagram paths or strokes"
    );
    assert_eq!(
        texts(&highlighted)
            .into_iter()
            .filter(|text| text.fill.is_some())
            .collect::<Vec<_>>(),
        texts(&styled)
            .into_iter()
            .filter(|text| text.fill.is_some())
            .collect::<Vec<_>>(),
        "generated labels are already shaped and painted, not code tokens"
    );
    assert_eq!(highlighted.total_bounds(), styled.total_bounds());
    assert_eq!(highlighted.reading_order(), styled.reading_order());
    let reading = highlighted.reading_order();
    let rendered = reading
        .iter()
        .find(|node| node.role == AccessibleReadingRole::Diagram && node.text == FLOW)
        .unwrap();
    let fallback = reading
        .iter()
        .find(|node| node.role == AccessibleReadingRole::Diagram && node.text == UNSUPPORTED)
        .unwrap();
    let ordinary = reading
        .iter()
        .find(|node| node.role == AccessibleReadingRole::CodeBlock && node.text == rust)
        .unwrap();
    assert!(
        !texts(&highlighted)
            .iter()
            .any(|text| text.source_span == rendered.source_span
                && (text.color_role == "code" || text.color_role.starts_with("tok-")))
    );
    assert!(
        texts(&highlighted)
            .iter()
            .any(|text| text.source_span == fallback.source_span
                && text.color_role.starts_with("tok-")),
        "unsupported Mermaid still owns syntax-colored literal source"
    );
    let fallback_visible: String = texts(&highlighted)
        .into_iter()
        .filter(|text| text.source_span == fallback.source_span)
        .map(|text| text.text.as_str())
        .collect();
    assert!(fallback_visible.contains("Diagram source fallback:"));
    assert!(fallback_visible.contains(&UNSUPPORTED.replace('\n', "")));
    assert!(
        paths(&highlighted)
            .iter()
            .all(|path| path.source_span != fallback.source_span)
    );
    let ordinary_visible: String = texts(&highlighted)
        .into_iter()
        .filter(|text| text.source_span == ordinary.source_span)
        .map(|text| text.text.as_str())
        .collect();
    assert_eq!(ordinary_visible, rust.replace('\n', ""));
    assert!(
        texts(&highlighted)
            .iter()
            .any(|text| text.source_span == ordinary.source_span && text.color_role == "tok-kw")
    );
}

#[test]
fn browser_highlighting_mode_survives_diagram_fallback_edits_and_width_changes() {
    let rust = "let answer = 42;\n";
    let source = format!(
        "{}\n{}",
        fixture_source("mermaid", FLOW),
        fence("rust", rust)
    );
    let mut state = BrowserFlowSession::new(&source, "sans", options(400.0)).unwrap();
    state.set_code_highlighting(1, 1, true).unwrap();
    assert!(state.code_highlighting());
    assert_eq!((state.revision(), state.layout_revision()), (1, 2));
    let highlighted = state.snapshot_json(1, 2, 0, 2048, true).unwrap();
    assert!(highlighted.contains("\"kind\":\"path\"") && highlighted.contains("\"tok-kw\""));
    assert_eq!(
        state
            .copy_source(1, SourceSpan::new(0, source.len()))
            .unwrap(),
        source
    );
    let changed = source.replace(FLOW, UNSUPPORTED);
    state
        .replace_source(1, &changed, FlowAssetReuse::Invalidate)
        .unwrap();
    assert!(state.code_highlighting());
    assert_eq!(state.revision(), 2);
    assert!(state.snapshot_json(1, 2, 0, 2048, true).is_err());
    state
        .reflow(2, state.layout_revision(), options(120.0))
        .unwrap();
    assert!(state.code_highlighting());
    assert_eq!(state.source(), changed);
    let current = state
        .snapshot_json(2, state.layout_revision(), 0, 2048, true)
        .unwrap();
    assert!(
        !current.contains("\"kind\":\"path\""),
        "unsupported source must not publish partial diagram paths"
    );
    assert!(
        current.contains("\"tok-kw\""),
        "editing/reflow must retain ordinary-code highlighting"
    );
    let reading = state
        .reading_json(2, state.layout_revision(), 0, 2048)
        .unwrap();
    assert!(reading.contains(&quoted(UNSUPPORTED)) && reading.contains(&quoted(rust)));
    assert!(reading.contains("\"role\":\"diagram\""));
    assert_eq!(
        state
            .copy_source(2, SourceSpan::new(0, changed.len()))
            .unwrap(),
        changed
    );
    state
        .set_code_highlighting(2, state.layout_revision(), false)
        .unwrap();
    assert!(!state.code_highlighting());
    let ordinary = state
        .snapshot_json(2, state.layout_revision(), 0, 2048, true)
        .unwrap();
    assert!(!ordinary.contains("\"tok-kw\""));
    assert_eq!(state.source(), changed);
}

#[test]
fn class_diagrams_keep_members_relationships_and_exact_source_in_narrow_flow() {
    let source = fence("mermaid", CLASS);
    for width in [180.0, 400.0, 900.0] {
        let state = session(&source, width);
        assert_scene_bounds(state.display(), &source, CLASS, width, FontFamily::Sans);
        let visible = flattened(state.display());
        for label in [
            "Document",
            "String title",
            "render() String",
            "interface",
            "HtmlRenderer",
            "implements",
            "renders & caches",
        ] {
            assert!(visible.contains(label), "lost class diagram label {label:?}");
        }
        assert!(!visible.contains("Diagram source fallback:"));
        assert!(
            paths(state.display()).iter().any(|path| {
                path.stroke
                    .as_ref()
                    .is_some_and(|stroke| !stroke.dash.is_empty())
            }),
            "realization retains its dashed relationship"
        );
        assert!(state.pending_assets().is_empty());
        assert_eq!(state.source(), source);
    }
}
