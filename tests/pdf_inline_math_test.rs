//! Inline `$…$` in PDF prose is typeset by fmd-math and drawn as outlines on
//! the text baseline (fm-djcw), like display equations: an unbreakable box
//! that grows its line, a tagged `/Formula` with the source as `/Alt`, and a
//! named, visible fallback for anything fmd-math cannot typeset.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    PdfOptions, RenderWarning, parse_markdown, render_pdf_document, render_warnings,
};

/// The concatenated, inflated page content streams.
fn page_content(pdf: &[u8]) -> String {
    let text = String::from_utf8_lossy(pdf);
    let mut result = String::new();
    for reference in text.split("/Contents ").skip(1) {
        let number = reference.split_whitespace().next().unwrap();
        let header = format!("\n{number} 0 obj\n");
        let offset = pdf
            .windows(header.len())
            .position(|s| s == header.as_bytes())
            .unwrap()
            + header.len();
        let object = &pdf[offset..];
        let stream = object.windows(8).position(|s| s == b"\nstream\n").unwrap();
        let dictionary = std::str::from_utf8(&object[..stream]).unwrap();
        let len: usize = dictionary
            .split_once("/Length ")
            .unwrap()
            .1
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let payload = &object[stream + 8..stream + 8 + len];
        if dictionary.contains("/Filter /FlateDecode") {
            let decoded = franken_markdown::zlib_decompress(payload, 16 * 1024 * 1024).unwrap();
            result.push_str(std::str::from_utf8(&decoded).unwrap());
        } else {
            result.push_str(std::str::from_utf8(payload).unwrap());
        }
    }
    assert!(!result.is_empty());
    result
}

/// The y of every text matrix (`1 0 0 1 x y Tm`) in content order.
fn text_baselines(content: &str) -> Vec<f32> {
    content
        .lines()
        .filter_map(|line| {
            let at = line.find(" Tm")?;
            let numbers: Vec<f32> = line[..at]
                .split_whitespace()
                .rev()
                .take(6)
                .map(|value| value.parse().ok())
                .collect::<Option<_>>()?;
            // Reversed: y, x, d, c, b, a.
            Some(numbers[0])
        })
        .collect()
}

fn render(markdown: &str) -> (Vec<u8>, String) {
    let document = parse_markdown(markdown);
    let pdf = render_pdf_document(&document, &PdfOptions::default()).unwrap();
    let content = page_content(&pdf);
    (pdf, content)
}

#[test]
fn inline_formula_draws_outlines_not_its_tex_source() {
    let source = r"\frac{a}{b} + x^2";
    let document = parse_markdown(&format!("Let ${source}$ be given."));
    assert!(
        render_warnings(&document, &PdfOptions::default()).is_empty(),
        "{:?}",
        render_warnings(&document, &PdfOptions::default())
    );
    let (pdf, content) = render(&format!("Let ${source}$ be given."));
    // The formula is its own tagged element, carrying its source.
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("/S /Formula "),
        "a /Formula structure element"
    );
    assert!(
        text.contains(r"/Alt (\\frac{a}{b} + x^2)"),
        "the source as /Alt"
    );
    let formula = content
        .split("/Formula <</MCID ")
        .nth(1)
        .expect("the formula's own marked content")
        .split("EMC\n")
        .collect::<Vec<_>>();
    // Outlines (curves and fills) inside it, then the /ActualText anchor span.
    assert!(formula[0].contains(" c\n"), "curved glyph outlines");
    assert!(formula[0].contains("f\n"), "filled contours");
    assert!(formula[0].contains("/ActualText "), "extractable source");
    // Planted negative: the old path set the source in the monospace slot.
    assert!(
        !content.contains("/F4 "),
        "TeX source must not be shown in the monospace face"
    );
}

#[test]
fn a_tall_inline_formula_grows_its_line_by_its_height_and_depth() {
    let size = PdfOptions::default().type_scale().body;
    let plain = text_baselines(&render("Above the gap.\n\nBelow it.").1);
    let source = r"\frac{\frac{a}{b}}{\frac{c}{d}}";
    let tall = text_baselines(&render(&format!("Above ${source}$ gap.\n\nBelow it.")).1);
    // Both pages start at the same top margin; the last text matrix is the
    // second paragraph's baseline. The formula's height lowers its own line
    // and its depth lowers the next, so the whole growth shows there.
    let growth = plain[plain.len() - 1] - tall[tall.len() - 1];
    let engine = franken_markdown::math::Engine::bundled().unwrap();
    let layout = engine
        .typeset(source, franken_markdown::math::Style::Text)
        .unwrap();
    // The line reserves at least the formula box's overflow above 1.0 em and
    // below 0.3 em of the body size (its ink can only add to that).
    let overflow = ((layout.height - 1.0).max(0.0) + (layout.depth - 0.3).max(0.0)) as f32 * size;
    assert!(overflow > 1.0, "the probe formula is genuinely tall");
    assert!(
        growth >= overflow - 0.05,
        "the line grew by {growth} for a box overflow of {overflow}"
    );
}

#[test]
fn an_unsupported_inline_construct_keeps_visible_tex_and_names_it() {
    let markdown = r"Before $\fmnNotACommand{x}$ after.";
    let document = parse_markdown(markdown);
    let warnings = render_warnings(&document, &PdfOptions::default());
    assert!(
        warnings.iter().any(|warning| matches!(
            warning,
            RenderWarning::MathFallback { source, .. } if source == r"\fmnNotACommand{x}"
        )),
        "{warnings:?}"
    );
    assert_eq!(warnings[0].code(), "math_fallback");
    let (pdf, content) = render(markdown);
    assert!(
        content.contains("/F4 "),
        "the fallback shows the source in the monospace face"
    );
    assert!(!String::from_utf8_lossy(&pdf).contains("/S /Formula "));
}

#[test]
fn inline_formulas_in_headings_and_table_cells_draw_outlines() {
    for markdown in [
        "# Heading with $x^2$\n",
        "| a | b |\n|---|---|\n| $\\sqrt{2}$ | text |\n",
    ] {
        let document = parse_markdown(markdown);
        assert!(
            render_warnings(&document, &PdfOptions::default()).is_empty(),
            "{markdown}"
        );
        let (_, content) = render(markdown);
        assert!(content.contains("/ActualText "), "{markdown}");
        assert!(content.contains(" c\n"), "outlines: {markdown}");
        assert!(!content.contains("/F4 "), "no monospace TeX: {markdown}");
    }
}

#[test]
fn an_over_wide_inline_formula_fits_whole_and_is_named() {
    let source = (0..120)
        .map(|index| format!("x_{{{index}}}"))
        .collect::<Vec<_>>()
        .join("+");
    let markdown = format!("Sum ${source}$ end.");
    let document = parse_markdown(&markdown);
    let warnings = render_warnings(&document, &PdfOptions::default());
    let overflow = warnings
        .iter()
        .find(|warning| warning.code() == "math_overflow")
        .unwrap_or_else(|| panic!("{warnings:?}"));
    assert!(matches!(
        overflow,
        RenderWarning::MathOverflow { width_pt, measure_pt, .. } if width_pt > measure_pt
    ));
    assert!(overflow.message().contains("scaled down to fit"));
    // Still one formula, drawn whole: fitting never splits its source.
    let (_, content) = render(&markdown);
    assert_eq!(content.matches("/Formula <</MCID ").count(), 1);
}

/// Inspect painted path endpoints and extraction anchors, without consulting
/// PDF layout internals or its verification text layer.
struct PaintedFormula {
    bounds: [f64; 4],
    x: f64,
    y: f64,
    size: f64,
}

fn painted_formulas(content: &str) -> Vec<PaintedFormula> {
    content
        .match_indices("/Span <</ActualText ")
        .filter_map(|(at, _)| {
            let start = content[..at].rfind(">> BDC\n")? + ">> BDC\n".len();
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for line in content[start..at].lines() {
                let parts: Vec<_> = line.split_whitespace().collect();
                if !matches!(parts.last(), Some(&"m" | &"l" | &"c")) || parts.len() < 3 {
                    continue;
                }
                let x: f64 = parts[parts.len() - 3].parse().unwrap();
                let y: f64 = parts[parts.len() - 2].parse().unwrap();
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x);
                bounds[3] = bounds[3].max(y);
            }
            if !bounds.iter().all(|v| v.is_finite()) {
                return None;
            }
            let matrix = content[at..].lines().find(|line| line.contains(" Tm"))?;
            let numbers: Vec<f64> = matrix
                .split_once(" Tm")?
                .0
                .split_whitespace()
                .rev()
                .take(6)
                .map(|v| v.parse().unwrap())
                .collect();
            let size = matrix
                .split_once(" Tf")?
                .0
                .split_whitespace()
                .last()?
                .parse()
                .unwrap();
            Some(PaintedFormula {
                bounds,
                x: numbers[1],
                y: numbers[0],
                size,
            })
        })
        .collect()
}

fn assert_painted_style(formula: &PaintedFormula, source: &str, display: bool) {
    use franken_markdown::math::{Engine, PathSeg, Style, paths};
    let engine = Engine::bundled().unwrap();
    let layout = engine
        .typeset(source, if display { Style::Display } else { Style::Text })
        .unwrap();
    let mut expected = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut visit = |(x, y): (f64, f64)| {
        let x = formula.x + x * formula.size;
        let y = formula.y + y * formula.size;
        expected[0] = expected[0].min(x);
        expected[1] = expected[1].min(y);
        expected[2] = expected[2].max(x);
        expected[3] = expected[3].max(y);
    };
    for contour in paths::resolve_paths(&engine, &layout).unwrap() {
        visit(contour.start);
        for segment in contour.segments {
            match segment {
                PathSeg::Line { to } | PathSeg::Quad { to, .. } => visit(to),
            }
        }
    }
    for (actual, expected) in formula.bounds.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.03,
            "display={display}, source={source:?}: painted {:?}, expected coordinate {expected}",
            formula.bounds
        );
    }
}

#[test]
fn inline_display_math_preserves_display_operators_limits_and_fractions() {
    for source in [r"\sum_{i=1}^{n} i", r"\int_0^1 x^2 dx", r"\frac{a+b}{c+d}"] {
        let (text_pdf, text) = render(&format!("Before ${source}$ after."));
        let (display_pdf, display) = render(&format!("Before $${source}$$ after."));
        assert_ne!(
            display_pdf, text_pdf,
            "the two math styles must not collapse"
        );
        let text_formulas = painted_formulas(&text);
        let display_formulas = painted_formulas(&display);
        assert_eq!(text_formulas.len(), 1);
        assert_eq!(display_formulas.len(), 1);
        assert_painted_style(&text_formulas[0], source, false);
        assert_painted_style(&display_formulas[0], source, true);
        let escaped = source.replace('\\', "\\\\");
        assert!(String::from_utf8_lossy(&display_pdf).contains(&format!("/Alt ({escaped})")));
        assert!(display.contains(&format!("/ActualText ({escaped})")));
        assert!(
            !display.contains("/F10 "),
            "display math is not a font slot"
        );
    }
}

#[test]
fn identical_sources_keep_separate_styles_in_repeated_table_layouts() {
    let source = r"\sum_{i=1}^{n} i";
    for displays in [[false, true], [true, false]] {
        let mut markdown = String::new();
        for display in displays {
            let delimiter = if display { "$$" } else { "$" };
            markdown.push_str(&format!(
                "| Formula |\n| --- |\n| {delimiter}{source}{delimiter} |\n\nSeparator.\n\n"
            ));
        }
        let (_, content) = render(&markdown);
        let formulas = painted_formulas(&content);
        assert_eq!(formulas.len(), 2);
        for (formula, display) in formulas.iter().zip(displays) {
            assert_painted_style(formula, source, display);
        }
    }
}

#[test]
fn display_math_style_reaches_headings_lists_quotes_definitions_and_footnotes() {
    let source = r"\sum_{i=1}^{n} i";
    let math = format!("$${source}$$");
    for markdown in [
        format!("# Heading {math}\n"),
        format!("> Before {math} after.\n"),
        format!("- Before {math} after.\n"),
        format!("Term {math}\n: Definition {math}\n"),
        format!("Evidence[^n].\n\n[^n]: Before {math} after.\n"),
        format!("Before [{math}](https://example.com/equation) after.\n"),
    ] {
        let (_, content) = render(&markdown);
        let formulas = painted_formulas(&content);
        assert!(!formulas.is_empty(), "missing formula in {markdown:?}");
        for formula in formulas {
            assert_painted_style(&formula, source, true);
        }
    }
}

#[test]
fn display_math_expands_grid_rows_and_keeps_emission_modes_identical() {
    use franken_markdown::{
        BaselineGrid, PdfEmitOptions, PdfPageEmission, render_pdf_document_emitted,
    };
    let source = r"\sum_{i=1}^{n} \frac{1}{i^2}";
    let document = parse_markdown(&format!("Before $${source}$$ after.\n\nNext paragraph."));
    let mut options = PdfOptions {
        strict_pagination: true,
        ..PdfOptions::default()
    };
    options.theme.page.baseline_grid = Some(BaselineGrid::from_points(12.0).unwrap());
    let pdf = render_pdf_document(&document, &options).unwrap();
    let monolithic = render_pdf_document_emitted(
        &document,
        &options,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(pdf, monolithic);
    let content = page_content(&pdf);
    let formulas = painted_formulas(&content);
    assert_eq!(formulas.len(), 1);
    assert_painted_style(&formulas[0], source, true);
    let top = options.theme.page.size.height_pt - options.theme.page.margins.top_pt;
    for y in text_baselines(&content) {
        let steps = (top - y) / 12.0;
        assert!(
            (steps - steps.round()).abs() < 0.002,
            "off-grid baseline: {y}"
        );
    }
}

#[test]
fn display_math_warning_measurements_use_the_authored_style() {
    use franken_markdown::math::{Engine, Style};
    let source = [r"\sum_{i=1}^{n} i"; 4].join("+");
    let engine = Engine::bundled().unwrap();
    let mut options = PdfOptions::default();
    let size = f64::from(options.type_scale().body);
    let widths = [Style::Text, Style::Display]
        .map(|style| engine.typeset(&source, style).unwrap().width * size);
    assert!(
        (widths[0] - widths[1]).abs() > 1.0,
        "probe must distinguish styles"
    );
    let measure = ((widths[0] + widths[1]) / 2.0) as f32;
    options.theme.page.size.width_pt =
        measure + options.theme.page.margins.left_pt + options.theme.page.margins.right_pt;
    for (display, width) in [(false, widths[0]), (true, widths[1])] {
        let delimiter = if display { "$$" } else { "$" };
        let document = parse_markdown(&format!("Before {delimiter}{source}{delimiter} after."));
        let warnings = render_warnings(&document, &options);
        let overflow = warnings
            .iter()
            .any(|warning| matches!(warning, RenderWarning::MathOverflow { .. }));
        assert_eq!(
            overflow,
            width > f64::from(measure),
            "display={display}: {warnings:?}"
        );
    }
    let unsupported = parse_markdown(r"Before $$\fmnNotACommand{x}$$ after.");
    assert!(render_warnings(&unsupported, &PdfOptions::default()).iter().any(|warning| matches!(
        warning, RenderWarning::MathFallback { source, .. } if source == r"\fmnNotACommand{x}"
    )));
    let (_, content) = render(r"Before $$\fmnNotACommand{x}$$ after.");
    assert!(
        content.contains("/F4 "),
        "unsupported display math retains visible TeX"
    );
}
