//! Actual timeline publication and shaped Flow integration.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/pdf_content.rs"]
mod pdf_content;

use franken_markdown::dep_invalidation::FlowSession;
use franken_markdown::display::{AccessibleReadingRole, DisplayItem};
use franken_markdown::flow_display::{FlowDisplayLimits, FlowLayoutOptions};
use franken_markdown::fonts::{BundledFlowFonts, FontStyle};
use franken_markdown::pdf::verification_text_layer;
use franken_markdown::svg::{SvgOptions, render_svg_with_diagnostics};
use franken_markdown::text::{
    Direction, Font, FontId, FontOrigin, OwnedTextRun, RunGlyph, TextCluster, TextRunContext,
    TextRunMetrics,
};
use franken_markdown::{
    FontFamily, HtmlOptions, PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown,
    render_html, render_pdf, render_pdf_document_emitted, render_pdf_with_diagnostics,
};

const TIMELINE: &str = concat!(
    "timeline\n",
    "title Publication roadmap\n",
    "section Explore\n",
    "Sketch : Gather ideas : Review risks\n",
    "section Deliver\n",
    "Build : Ship engine\n",
    "      : Publish docs\n",
);

fn markdown(code: &str, language: &str) -> String {
    format!("# Roadmap\n\n\x60\x60\x60{language}\n{code}\x60\x60\x60\n")
}

fn flow(source: &str, width: f32) -> FlowSession {
    let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    FlowSession::new(
        source,
        8,
        FlowDisplayLimits::default(),
        FlowLayoutOptions {
            viewport_width: width,
            ..FlowLayoutOptions::default()
        },
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap()
}

/// A fixture host for digits and Unicode spaces, using the actual face's cmap
/// and metrics. The bundled shaper intentionally refuses U+202F even though
/// this face maps it; do not normalize it away or broaden production admission.
fn numeric_space_run(font: &Font, bytes: &[u8], text: &str, size: f32) -> OwnedTextRun {
    assert!(
        text.chars().all(|ch| {
            ch.is_ascii_digit() || matches!(ch, '\u{00a0}' | '\u{2007}' | '\u{202f}')
        })
    );
    let scale = size / f32::from(font.units_per_em);
    let font_id = FontId::from_font_data(bytes);
    let mut metrics = TextRunMetrics {
        ascent: (f32::from(font.ascent) * scale).max(0.0),
        descent: (-f32::from(font.descent) * scale).max(0.0),
    };
    let mut run = OwnedTextRun {
        context: TextRunContext {
            font_id,
            font_size: size,
            script: *b"DFLT",
            language: *b"dflt",
            direction: Direction::LeftToRight,
            font_origin: FontOrigin::BundledFace,
        },
        logical_text: text.to_owned(),
        clusters: Vec::new(),
        glyphs: Vec::new(),
        total_advance: 0.0,
        vertical_metrics: None,
    };
    for (index, (start, ch)) in text.char_indices().enumerate() {
        let glyph_id = font.glyph_index(ch);
        assert_ne!(glyph_id, 0, "the fixture requires a real glyph for {ch:?}");
        let advance = f32::from(font.advance_width(glyph_id)) * scale;
        if let Some(bounds) = font.glyph_bbox(glyph_id) {
            metrics.ascent = metrics.ascent.max(f32::from(bounds[3]) * scale);
            metrics.descent = metrics.descent.max(-f32::from(bounds[1]) * scale);
        }
        run.clusters.push(TextCluster {
            cluster_index: index,
            byte_range: start..start + ch.len_utf8(),
            // This deliberately small fixture accepts only BMP scalars.
            utf16_range: index..index + 1,
            glyph_range: index..index + 1,
            x_start: run.total_advance,
            x_end: run.total_advance + advance,
            font_id,
        });
        run.glyphs.push(RunGlyph {
            glyph_id,
            font_id,
            cluster_index: index,
            x_advance: advance,
            y_advance: 0.0,
            x_offset: 0.0,
            y_offset: 0.0,
        });
        run.total_advance += advance;
    }
    run.vertical_metrics = Some(metrics);
    run
}

#[test]
fn timelines_publish_as_native_figures_in_html_pdf_and_svg() {
    for direction in ["LR", "TD"] {
        let code = TIMELINE.replace("timeline\n", &format!("timeline {direction}\n"));
        let source = markdown(&code, "mermaid");
        let doc = parse_markdown(&source);
        let html = render_html(&source, &HtmlOptions::default()).unwrap();
        assert!(html.contains("class=\"fmd-diagram fmd-timeline\""));
        assert!(!html.contains("language-mermaid"));
        assert!(!html.contains("<script"));
        for label in [
            "Sketch",
            "Build",
            "Explore",
            "Deliver",
            "Gather ideas",
            "Publish docs",
        ] {
            assert!(
                html.contains(&format!(">{label}</text>")),
                "{label}: {html}"
            );
        }

        let opts = PdfOptions::default();
        let output = render_pdf_with_diagnostics(&source, &opts).unwrap();
        assert!(output.warnings.is_empty(), "{:?}", output.warnings);
        assert_eq!(output.bytes, render_pdf(&source, &opts).unwrap());
        let raw = String::from_utf8_lossy(&output.bytes);
        assert_eq!(raw.matches("/S /Figure").count(), 1);
        assert!(raw.contains("/Alt (Timeline diagram)"));
        let streams = pdf_content::page_content_streams(&output.bytes);
        let operations = streams
            .iter()
            .flat_map(|page| pdf_content::operations(page))
            .collect::<Vec<_>>();
        assert!(
            operations
                .iter()
                .filter(|op| matches!(op.operator, "TJ" | "Tj"))
                .count()
                > 8,
            "the figure must paint its labels, not only provide metadata"
        );
        assert!(
            operations.iter().filter(|op| op.operator == "c").count() >= 16,
            "rounded timeline panels must reach the PDF as actual vector geometry"
        );
        let layer = verification_text_layer(&doc, &opts).unwrap();
        let runs = layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .collect::<Vec<_>>();
        assert_eq!(runs.iter().filter(|run| run.kind == "image").count(), 1);
        assert!(
            !runs
                .iter()
                .any(|run| run.text.contains("timeline LR") || run.text.contains("timeline TD"))
        );

        let (poster, _, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
        let poster = String::from_utf8(poster).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(poster.contains("data:image/svg+xml;base64,"));
        assert!(poster.contains("aria-label=\"Timeline diagram\""));
        assert!(!poster.contains("language-mermaid"));
    }
}

#[test]
fn timeline_bytes_agree_between_emission_modes_and_the_rust_browser_adapter() {
    for direction in ["LR", "TD"] {
        let source = markdown(
            &TIMELINE.replace("timeline\n", &format!("timeline {direction}\n")),
            "timeline",
        );
        let doc = parse_markdown(&source);
        let options = PdfOptions::default();
        let expected = render_pdf(&source, &options).unwrap();
        for emission in [PdfPageEmission::Monolithic, PdfPageEmission::Chunked] {
            assert_eq!(
                expected,
                render_pdf_document_emitted(
                    &doc,
                    &options,
                    PdfEmitOptions {
                        emission,
                        ..PdfEmitOptions::default()
                    },
                )
                .unwrap()
            );
        }
        assert_eq!(
            expected,
            franken_markdown::wasm::render_pdf(
                &source,
                &franken_markdown::wasm::WasmRenderOptions::default(),
            )
            .unwrap()
            .bytes
        );
    }
}

#[test]
fn shaped_flow_has_complete_vector_labels_and_one_selectable_source_transcript() {
    for direction in ["LR", "TD"] {
        let code = TIMELINE.replace("timeline\n", &format!("timeline {direction}\n"));
        for language in ["mermaid", "timeline"] {
            let source = markdown(&code, language);
            for width in [180.0, 640.0] {
                let state = flow(&source, width);
                let display = state.display();
                let diagrams = display
                    .reading_order()
                    .iter()
                    .filter(|node| node.role == AccessibleReadingRole::Diagram)
                    .collect::<Vec<_>>();
                assert_eq!(diagrams.len(), 1);
                let diagram = diagrams[0];
                assert_eq!(diagram.text, code);
                assert!(diagram.children.is_empty());
                assert!(diagram.source_span.slice(&source).unwrap().contains(&code));
                assert!(diagram.bounds.x >= -0.01 && diagram.bounds.right() <= width + 0.01);
                assert!(diagram.bounds.height > 0.0);
                let paths = display
                    .items()
                    .iter()
                    .filter(|item| matches!(item, DisplayItem::Path(_)))
                    .count();
                assert!(paths >= 10, "the complete timeline must paint native paths");
                let texts = display
                    .items()
                    .iter()
                    .filter_map(|item| {
                        if let DisplayItem::Text(text) = item {
                            Some(text)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                for expected in [
                    "Sketch",
                    "Build",
                    "Explore",
                    "Deliver",
                    "Gather ideas",
                    "Publish docs",
                ] {
                    let text = texts.iter().find(|text| text.text == expected).unwrap();
                    let run = text
                        .font_run
                        .as_ref()
                        .expect("timeline labels use real bundled-font shaping");
                    assert_eq!(run.logical_text, expected);
                    assert!(run.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
                    assert!(text.bounds.x >= diagram.bounds.x - 0.02);
                    assert!(text.bounds.right() <= diagram.bounds.right() + 0.02);
                    assert!(text.bounds.y >= diagram.bounds.y - 0.02);
                    assert!(text.bounds.bottom() <= diagram.bounds.bottom() + 0.02);
                }
                assert!(
                    state.pending_assets().is_empty(),
                    "native diagrams need no external image grants"
                );
            }
        }
    }
}

#[test]
fn shaped_flow_retains_nonbreaking_spaces_with_bundled_and_host_shaping() {
    let bytes = franken_markdown::fonts::body_bytes(FontFamily::Sans, FontStyle::Regular);
    let font = Font::parse_static(bytes).unwrap();
    let bundled = BundledFlowFonts::new(FontFamily::Sans).unwrap();
    for (event, expected, host) in [
        ("1&#160;2&#8199;3", "1\u{00a0}2\u{2007}3", false),
        (
            "1&#160;2&#8199;3&#8239;4",
            "1\u{00a0}2\u{2007}3\u{202f}4",
            true,
        ),
    ] {
        for direction in ["LR", "TD"] {
            let source = markdown(
                &format!("timeline {direction}\nPeriod : {event}\n"),
                "timeline",
            );
            for width in [180.0, 640.0] {
                let state = FlowSession::new(
                    &source,
                    8,
                    FlowDisplayLimits::default(),
                    FlowLayoutOptions {
                        viewport_width: width,
                        ..FlowLayoutOptions::default()
                    },
                    |text, size, role, style| {
                        if host && text.contains('\u{202f}') {
                            Ok(numeric_space_run(&font, bytes, text, size))
                        } else {
                            bundled.shape(text, size, role, style)
                        }
                    },
                )
                .unwrap();
                let label = state
                    .display()
                    .items()
                    .iter()
                    .find_map(|item| match item {
                        DisplayItem::Text(text) if text.text == expected => Some(text),
                        _ => None,
                    })
                    .expect("the SVG parser must preserve every authored nonbreaking space");
                let run = label
                    .font_run
                    .as_ref()
                    .expect("the preserved label must reach actual font shaping");
                assert_eq!(run.logical_text, expected);
                assert_eq!(run.glyphs.len(), expected.chars().count());
                for (glyph, ch) in run.glyphs.iter().zip(expected.chars()) {
                    assert_eq!(glyph.glyph_id, font.glyph_index(ch));
                    let advance = f32::from(font.advance_width(glyph.glyph_id))
                        * run.context.font_size
                        / f32::from(font.units_per_em);
                    assert!((glyph.x_advance - advance).abs() < 0.001);
                }
                assert!(state.pending_assets().is_empty());
            }
        }
    }
}

#[test]
fn unsupported_timeline_directives_keep_the_entire_source_in_each_output() {
    let code = "timeline\nPhase : Important event\nstyle hidden fill:red\n";
    let source = markdown(code, "timeline");
    let doc = parse_markdown(&source);
    let html = render_html(&source, &HtmlOptions::default()).unwrap();
    assert!(html.contains("language-timeline"));
    assert!(!html.contains("class=\"fmd-diagram fmd-timeline\""));
    assert!(html.contains("Phase : Important event"));
    assert!(html.contains("style hidden fill:red"));
    let pdf = render_pdf(&source, &PdfOptions::default()).unwrap();
    assert!(!String::from_utf8_lossy(&pdf).contains("/S /Figure"));
    let layer = verification_text_layer(&doc, &PdfOptions::default()).unwrap();
    let text = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Phase : Important event"), "{text}");
    assert!(text.contains("style hidden fill:red"), "{text}");
    let options = SvgOptions::default();
    let (poster, report, warnings) = render_svg_with_diagnostics(&doc, &options);
    let ordinary = render_svg_with_diagnostics(
        &parse_markdown(&markdown(code, "unknown-language")),
        &options,
    );
    // Poster text is emitted as glyph paths. Exact equality with ordinary code
    // checks all source glyphs, their positions and the canvas, not a substring.
    assert_eq!(poster, ordinary.0);
    assert_eq!(report, ordinary.1);
    assert_eq!(warnings, ordinary.2);
    let poster = String::from_utf8(poster).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!poster.contains("data:image/svg+xml;base64,"));

    let state = flow(&source, 640.0);
    let nodes = state
        .display()
        .reading_order()
        .iter()
        .filter(|node| node.role == AccessibleReadingRole::Diagram)
        .collect::<Vec<_>>();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].text, code);
    assert!(
        state
            .display()
            .items()
            .iter()
            .all(|item| !matches!(item, DisplayItem::Path(_)))
    );
    let visible = state
        .display()
        .items()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(visible.contains("Diagram source fallback"));
    for line in code.lines() {
        assert!(
            visible.contains(line),
            "lost source line {line:?}: {visible}"
        );
    }
}

#[test]
fn nested_timelines_use_the_shared_figure_pipeline_without_losing_surrounding_text() {
    let source = concat!(
        "# Nested roadmap\n\n",
        "> Before\n>\n> \x60\x60\x60timeline\n> timeline\n> Era : Progress\n> \x60\x60\x60\n>\n> After\n",
    );
    let html = render_html(source, &HtmlOptions::default()).unwrap();
    assert_eq!(
        html.matches("class=\"fmd-diagram fmd-timeline\"").count(),
        1
    );
    assert!(html.contains("Before") && html.contains("After"));
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&pdf).matches("/S /Figure").count(),
        1
    );
    let layer = verification_text_layer(&parse_markdown(source), &PdfOptions::default()).unwrap();
    let text = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Before") && text.contains("After"));
}
