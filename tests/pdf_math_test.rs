//! Public PDF equations use the math engine's vector geometry and keep source
//! semantics across pagination, archived output, and the browser-safe adapter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    Block, Document, Inline, List, ListItem, PageMargins, PageSize, PdfASettings, PdfEmitOptions,
    PdfOptions, RenderWarning, parse_markdown, render_pdf, render_pdf_document,
    render_pdf_document_emitted, render_pdf_document_pdfa, render_pdf_document_profiled,
    render_warnings,
};

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
            assert!(!dictionary.contains("/Filter"));
            result.push_str(std::str::from_utf8(payload).unwrap());
        }
    }
    assert!(!result.is_empty());
    result
}

fn bounds(pdf: &[u8]) -> Vec<[f32; 4]> {
    String::from_utf8_lossy(pdf)
        .split("/BBox [")
        .skip(1)
        .map(|part| {
            part.split_once(']')
                .unwrap()
                .0
                .split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        })
        .collect()
}

fn equation(source: &str) -> Document {
    Document {
        blocks: vec![Block::MathBlock(source.to_string())],
    }
}

#[test]
fn fractions_roots_scripts_operators_and_matrices_draw_real_curves() {
    let options = PdfOptions::default();
    for source in [
        r"\frac{a+b}{c+d}",
        r"\sqrt{x^2+y^2}",
        r"\sum_{i=1}^{n} i^2",
        r"\begin{bmatrix}a & b \\ c & d\end{bmatrix}",
        r"\left\{\frac{1}{\frac{a}{b}}\right\}",
        r"\overbrace{x+y}^{n}",
    ] {
        let document = equation(source);
        assert!(render_warnings(&document, &options).is_empty(), "{source}");
        let pdf = render_pdf_document(&document, &options).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        let content = page_content(&pdf);
        assert!(text.contains("/S /Formula "), "semantic equation: {source}");
        assert!(text.contains("/Alt "), "source alternative: {source}");
        assert!(
            content.contains("/ActualText "),
            "extractable source: {source}"
        );
        let drawing = content
            .split(" BDC\n")
            .nth(1)
            .unwrap()
            .split("EMC")
            .next()
            .unwrap();
        assert!(
            drawing.contains(" c"),
            "actual curved glyph outlines: {source}\n{drawing}"
        );
        assert!(
            drawing.contains("f\n") || drawing.contains("f Q"),
            "filled contours: {source}"
        );
        assert!(
            !drawing.contains(" Tf") || drawing.contains("3 Tr"),
            "TeX must not be visibly typeset as text"
        );
        let bbox = bounds(&pdf);
        assert_eq!(bbox.len(), 1);
        assert!(bbox[0][2] > bbox[0][0] && bbox[0][3] > bbox[0][1]);
    }
}

#[test]
fn math_fences_single_line_and_multiline_dollars_share_the_same_equation() {
    let source = r"\frac{x^2}{\sqrt{y}}";
    let options = PdfOptions::default();
    let expected = render_pdf_document(&equation(source), &options).unwrap();
    for markdown in [
        format!("$${source}$$"),
        format!("$$\n{source}\n$$"),
        format!("```math\n{source}\n```"),
    ] {
        assert_eq!(render_pdf(&markdown, &options).unwrap(), expected);
    }
}

#[test]
fn math_is_centered_fits_page_and_preserves_tall_ink() {
    let mut options = PdfOptions::default();
    options.theme.page.size = PageSize {
        name: "equations",
        width_pt: 160.0,
        height_pt: 120.0,
    };
    options.theme.page.margins = PageMargins {
        top_pt: 15.0,
        right_pt: 20.0,
        bottom_pt: 15.0,
        left_pt: 20.0,
    };
    let small = bounds(&render_pdf_document(&equation("x"), &options).unwrap())[0];
    let tall =
        bounds(&render_pdf_document(&equation(r"\frac{1}{\frac{a}{b}}"), &options).unwrap())[0];
    assert!(
        tall[3] - tall[1] > small[3] - small[1],
        "fractions reserve real vertical space"
    );
    for source in [
        "x".to_string(),
        "x+y+".repeat(150),
        format!(r"\begin{{matrix}}{}\end{{matrix}}", "x \\\\ ".repeat(30)),
    ] {
        let document = equation(&source);
        assert!(render_warnings(&document, &options).is_empty());
        let pdf = render_pdf_document(&document, &options).unwrap();
        let bbox = bounds(&pdf)[0];
        assert!(
            bbox[0] >= 19.98 && bbox[2] <= 140.02,
            "horizontal bounds: {bbox:?}"
        );
        assert!(
            bbox[1] >= 14.98 && bbox[3] <= 105.02,
            "vertical bounds: {bbox:?}"
        );
        assert!(((bbox[0] + bbox[2]) / 2.0 - 80.0).abs() < 0.02);
    }
}

#[test]
fn equations_use_theme_ink_and_preserve_source_in_verification() {
    let mut options = PdfOptions::default();
    options.theme.colors.fg = "#234567".to_string();
    let source = r"\alpha + \frac{b}{c}";
    let document = equation(source);
    let pdf = render_pdf_document(&document, &options).unwrap();
    assert!(page_content(&pdf).contains("0.137 0.271 0.404 rg"));
    let report = franken_markdown::verify::verify_pdf(&document, &options).unwrap();
    assert!(report.findings.is_empty());
    assert!(
        report
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text == source)
    );
}

#[test]
fn math_font_coverage_does_not_use_the_document_font_cmap() {
    // U+03D5 exists in the bundled math face but not in the default sans
    // document faces: it must not produce a false missing-glyph warning.
    let document = equation("ϕ");
    let options = PdfOptions::default();
    assert!(render_warnings(&document, &options).is_empty());
    let pdf = render_pdf_document(&document, &options).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/S /Formula "));
    assert!(page_content(&pdf).contains("/ActualText <FEFF03D5>"));
}

#[test]
fn fitted_subpoint_equations_keep_a_unit_matrix_in_the_emitted_pdf() {
    let document = equation(&"x".repeat(2000));
    let options = PdfOptions::default();
    assert!(render_warnings(&document, &options).is_empty());
    let pdf = render_pdf_document(&document, &options).unwrap();
    let content = page_content(&pdf);
    assert!(
        content.contains("q 1 0 0 -1 "),
        "formula scaling must not round to zero"
    );
    let bbox = bounds(&pdf)[0];
    assert!((bbox[2] - bbox[0] - 468.0).abs() < 0.02);
    assert!((bbox[3] - bbox[1] - 1.0).abs() < 0.02);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn independent_pdf_text_extractor_recovers_vector_equation_sources() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if Command::new("pdftotext").arg("-v").output().is_err() {
        return;
    }
    let source = r"\frac{a+b}{\sqrt{c}}";
    let document = Document {
        blocks: vec![
            Block::Paragraph(vec![Inline::Text("Before".into())]),
            Block::MathBlock(source.into()),
            Block::MathBlock("ϕ".into()),
            Block::Paragraph(vec![Inline::Text("After".into())]),
        ],
    };
    let pdf = render_pdf_document(&document, &PdfOptions::default()).unwrap();
    let mut child = Command::new("pdftotext")
        .args(["-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&pdf).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let extracted = String::from_utf8(output.stdout).unwrap();
    assert!(
        extracted.contains(source),
        "vector formula source was lost: {extracted}"
    );
    assert!(
        extracted.contains('ϕ'),
        "Unicode formula source was lost: {extracted}"
    );
    assert!(extracted.find("Before") < extracted.find(source));
    assert!(extracted.find(source) < extracted.find("After"));
}

#[test]
fn unsupported_and_excessive_equations_retain_source_with_typed_warning() {
    let options = PdfOptions::default();
    for source in [r"\notacommand{x}".to_string(), "x".repeat(65_537)] {
        let document = equation(&source);
        let warnings = render_warnings(&document, &options);
        assert!(matches!(
            warnings.as_slice(),
            [RenderWarning::MathFallback { .. }]
        ));
        assert_eq!(warnings[0].code(), "math_fallback");
        assert!(warnings[0].message().contains("rendered as TeX source"));
        if source.len() < 100 {
            let pdf = render_pdf_document(&document, &options).unwrap();
            assert!(!String::from_utf8_lossy(&pdf).contains("/S /Formula "));
            let layer =
                franken_markdown::pdf::verification_text_layer(&document, &options).unwrap();
            assert!(
                layer
                    .pages
                    .iter()
                    .flat_map(|page| &page.runs)
                    .any(|run| run.text == source)
            );
            let report = franken_markdown::verify::verify_pdf(&document, &options).unwrap();
            assert!(
                report
                    .findings
                    .iter()
                    .any(|finding| finding.code == "math_fallback")
            );
        }
    }
}

#[test]
fn quoted_list_equations_keep_structure_and_invalid_nested_math_is_reported() {
    let document = Document {
        blocks: vec![Block::BlockQuote(vec![Block::List(List {
            ordered: false,
            start: 1,
            tight: false,
            items: vec![ListItem {
                task: Some(true),
                blocks: vec![Block::MathBlock(r"\sqrt{x}".into())],
            }],
        })])],
    };
    let options = PdfOptions::default();
    let pdf = render_pdf_document(&document, &options).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    for tag in ["/S /BlockQuote ", "/S /L ", "/S /LI ", "/S /Formula "] {
        assert!(text.contains(tag), "{tag}");
    }
    let layer = franken_markdown::pdf::verification_text_layer(&document, &options).unwrap();
    assert!(
        layer
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text.contains("[x]"))
    );
    let invalid = Document {
        blocks: vec![Block::FootnoteDefinition {
            id: "note".into(),
            blocks: vec![Block::BlockQuote(vec![Block::MathBlock(
                r"\notacommand{x}".into(),
            )])],
        }],
    };
    assert!(
        render_warnings(&invalid, &options)
            .iter()
            .any(|warning| warning.code() == "math_fallback")
    );
}

#[test]
fn all_pdf_entrypoints_and_browser_adapter_agree_on_equations() {
    let source = "# Equations\n\n$$\\frac{a+b}{c+d}$$\n\nAfter the equation.";
    let document = parse_markdown(source);
    let options = PdfOptions::default();
    let original = document.clone();
    let pdf = render_pdf_document(&document, &options).unwrap();
    assert_eq!(pdf, render_pdf_document(&document, &options).unwrap());
    assert_eq!(
        pdf,
        render_pdf_document_profiled(&document, &options)
            .unwrap()
            .bytes
    );
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&document, &options, PdfEmitOptions::monolithic()).unwrap()
    );
    let browser = franken_markdown::wasm::render_pdf(
        source,
        &franken_markdown::wasm::WasmRenderOptions::default(),
    )
    .unwrap();
    assert_eq!(browser.bytes, pdf);
    assert!(browser.diagnostics.is_empty());
    assert_eq!(document, original);
    let archived = render_pdf_document_pdfa(&document, &options, PdfASettings::a2b()).unwrap();
    assert!(String::from_utf8_lossy(&archived).contains("/S /Formula "));
    let invalid = franken_markdown::wasm::render_pdf(
        "$$\\notacommand{x}$$",
        &franken_markdown::wasm::WasmRenderOptions::default(),
    )
    .unwrap();
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|warning| warning.message.contains("rendered as TeX source"))
    );
}

#[test]
fn display_equations_in_notes_survive_document_preparation() {
    let document = Document {
        blocks: vec![
            Block::Paragraph(vec![Inline::FootnoteRef { id: "proof".into() }]),
            Block::FootnoteDefinition {
                id: "proof".into(),
                blocks: vec![Block::MathBlock(r"\frac{1}{2}".into())],
            },
        ],
    };
    let options = PdfOptions::default();
    let pdf = render_pdf_document(&document, &options).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/S /Formula "));
    let verified = franken_markdown::verify::verify_pdf(&document, &options).unwrap();
    assert!(
        verified
            .pages
            .iter()
            .flat_map(|page| &page.runs)
            .any(|run| run.text == r"\frac{1}{2}")
    );
}
