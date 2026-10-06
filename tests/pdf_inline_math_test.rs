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
fn an_over_wide_inline_formula_overflows_whole_and_is_named() {
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
    // Still one formula, drawn whole: it never splits across lines.
    let (_, content) = render(&markdown);
    assert_eq!(content.matches("/Formula <</MCID ").count(), 1);
}
