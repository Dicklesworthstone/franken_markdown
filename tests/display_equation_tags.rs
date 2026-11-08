//! Explicit equation labels survive publication and browser Flow as complete
//! displays, while inline and malformed uses retain their source fallback.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::dep_invalidation::{FlowSession, FlowShapeCache, FlowShapeCacheLimits};
use franken_markdown::display::{AccessibleReadingRole, DisplayItem, DisplayPath};
use franken_markdown::flow_display::{FlowDisplayLimits, FlowLayoutOptions};
use franken_markdown::{
    Block, Document, FontFamily, HtmlOptions, Inline, PageMargins, PageSize, PdfEmitOptions,
    PdfOptions, RenderWarning, parse_markdown, render_html, render_html_document,
    render_html_document_to_writer, render_pdf, render_pdf_document_emitted, render_warnings,
};

const FORMULA: &str = r"{\color{blue}x}\tag*{\textcolor{red}{Energy}}";

fn markdown(formula: &str) -> String {
    format!("$$\n{formula}\n$$\n")
}

fn flow(source: &str, width: f32) -> FlowSession {
    let mut fonts = FlowShapeCache::new(FontFamily::Sans, FlowShapeCacheLimits::default()).unwrap();
    FlowSession::new(
        source,
        32,
        FlowDisplayLimits::default(),
        FlowLayoutOptions {
            viewport_width: width,
            ..FlowLayoutOptions::default()
        },
        |text, size, role, style| fonts.shape(text, size, role, style),
    )
    .unwrap()
}

fn paths(session: &FlowSession) -> Vec<&DisplayPath> {
    session
        .display()
        .items()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Path(path) => Some(path),
            _ => None,
        })
        .collect()
}

#[test]
fn html_labels_are_upright_escaped_and_do_not_leak_through_the_formula_cache() {
    let formula = r"x^2\tag{Energy \& <safety>}";
    let document = Document {
        blocks: vec![
            Block::MathBlock(formula.into()),
            Block::Paragraph(vec![Inline::DisplayMath(formula.into())]),
            Block::MathBlock(formula.into()),
            Block::MathBlock(format!("  {formula}  ")),
        ],
    };
    let options = HtmlOptions::default();
    let html = render_html_document(&document, &options).unwrap();
    assert_eq!(html.matches("class=\"fmd-tagged-equation\"").count(), 3);
    assert_eq!(html.matches("class=\"fmd-equation-tag\"").count(), 3);
    assert!(html.contains("data-tex=\"  x^2\\tag{Energy \\&amp; &lt;safety&gt;}  \""));
    assert!(html.contains("<mtext>"));
    assert!(html.contains("&amp;") && html.contains("&lt;safety&gt;"));
    assert!(!html.contains("<safety>"));
    assert!(
        html.contains("<span class=\"math display\">\\["),
        "inline display style still has no label column"
    );
    let mut streamed = Vec::new();
    render_html_document_to_writer(&document, &options, &mut streamed).unwrap();
    assert_eq!(html.as_bytes(), streamed);
    let source = markdown(formula);
    let native = render_html(&source, &options).unwrap();
    let browser = franken_markdown::wasm::render_html(&source, &Default::default()).unwrap();
    assert_eq!(native.as_bytes(), browser.bytes);
}

#[test]
fn pdf_labels_are_complete_vector_formulas_in_wide_and_narrow_pages() {
    let source = markdown(FORMULA);
    let doc = parse_markdown(&source);
    for width in [160.0, 420.0] {
        let mut options = PdfOptions::default();
        options.theme.page.size = PageSize {
            name: "labels",
            width_pt: width,
            height_pt: 180.0,
        };
        options.theme.page.margins = PageMargins {
            top_pt: 20.0,
            right_pt: 20.0,
            bottom_pt: 20.0,
            left_pt: 20.0,
        };
        assert!(render_warnings(&doc, &options).is_empty());
        let pdf = render_pdf(&source, &options).unwrap();
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&doc, &options, PdfEmitOptions::monolithic()).unwrap()
        );
        assert_eq!(pdf, render_pdf(&source, &options).unwrap());
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/S /Formula "));
        let bbox: Vec<f32> = text
            .split("/BBox [")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap()
            .split_whitespace()
            .map(|value| value.parse().unwrap())
            .collect();
        assert!(
            bbox[0] >= 19.98 && bbox[2] <= width - 19.98,
            "label must remain inside the column: {bbox:?}"
        );
        assert!(bbox[1] >= 19.98 && bbox[3] <= 160.02);
        assert!(
            (bbox[2] - (width - 20.0)).abs() < 0.03,
            "tagged formula occupies its full containing width"
        );
        assert_pdf_text_contains(&pdf, FORMULA);
    }
    let native = render_pdf(&source, &PdfOptions::default()).unwrap();
    let browser = franken_markdown::wasm::render_pdf(&source, &Default::default()).unwrap();
    assert_eq!(native, browser.bytes);
}

fn assert_pdf_text_contains(pdf: &[u8], source: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if Command::new("pdftotext").arg("-v").output().is_err() {
        return;
    }
    let mut child = Command::new("pdftotext")
        .args(["-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(pdf).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(
        text.contains(source),
        "source or explicit label lost in independent PDF extraction: {text}"
    );
}

#[test]
fn svg_draws_real_label_glyphs_and_reports_no_unsupported_math() {
    let document = parse_markdown(&markdown(FORMULA));
    let (bytes, report, warnings) =
        franken_markdown::svg::render_svg_with_diagnostics(&document, &Default::default());
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        report.glyphs_drawn, 7,
        "one equation glyph and six upright label glyphs"
    );
    let svg = String::from_utf8(bytes).unwrap();
    assert_eq!(svg.matches("<use href=").count(), 7);
    assert!(!svg.contains("foreignObject") && !svg.contains("<text"));
}

#[test]
fn flow_labels_reflow_with_their_column_and_keep_one_exact_reading_source() {
    let source = markdown(FORMULA);
    let mut previous_right = 0.0;
    for width in [80.0, 360.0, 720.0] {
        let state = flow(&source, width);
        let paths = paths(&state);
        assert_eq!(
            paths.len(),
            7,
            "every equation and label glyph has a real outline"
        );
        let body_right = paths
            .iter()
            .filter(|path| path.fill == Some([0.0, 0.0, 1.0, 1.0]))
            .map(|path| path.bounds.right())
            .fold(0.0_f32, f32::max);
        let label_left = paths
            .iter()
            .filter(|path| path.fill == Some([1.0, 0.0, 0.0, 1.0]))
            .map(|path| path.bounds.x)
            .fold(f32::INFINITY, f32::min);
        let label_right = paths
            .iter()
            .map(|path| path.bounds.right())
            .fold(0.0_f32, f32::max);
        assert!(
            body_right > 0.0 && body_right < label_left,
            "label overlaps the equation"
        );
        assert!(label_right <= width + 0.002 && label_right > previous_right);
        assert!(paths.iter().all(|path| path.bounds.x >= -0.002));
        previous_right = label_right;
        let reading = state.display().reading_order();
        assert_eq!(reading.len(), 1);
        assert_eq!(reading[0].role, AccessibleReadingRole::Math);
        assert_eq!(reading[0].text, FORMULA);
        assert!(
            paths
                .iter()
                .all(|path| path.source_span == reading[0].source_span)
        );
        assert!(state.pending_assets().is_empty());
    }
    let nested = format!("> $$\n> {FORMULA}\n> $$\n");
    let state = flow(&nested, 180.0);
    assert!(
        paths(&state)
            .iter()
            .all(|path| path.bounds.x >= 16.0 && path.bounds.right() <= 180.002)
    );
}

#[test]
fn invalid_and_nested_labels_keep_complete_source_fallback_in_every_renderer() {
    for formula in [r"x\tag{A}\tag{B}", r"{x\tag{nested}}", r"x\tag{unclosed"] {
        let source = markdown(formula);
        let document = parse_markdown(&source);
        let html = render_html(&source, &Default::default()).unwrap();
        assert!(!html.contains("class=\"fmd-tagged-equation\""));
        assert!(html.contains(formula));
        assert!(
            render_warnings(&document, &Default::default())
                .iter()
                .any(|warning| matches!(warning, RenderWarning::MathFallback { .. }))
        );
        let (_, _, warnings) =
            franken_markdown::svg::render_svg_with_diagnostics(&document, &Default::default());
        assert!(
            warnings
                .iter()
                .any(|warning| warning.code == "svg_math_unsupported")
        );
        let state = flow(&source, 400.0);
        assert!(paths(&state).is_empty());
        assert_eq!(state.display().reading_order()[0].text, formula);
    }
}
