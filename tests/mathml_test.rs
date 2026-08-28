use franken_markdown::{
    HtmlOptions, parse_markdown, render_html, render_html_document,
};

#[test]
fn test_inline_math_renders_mathml() {
    let md = "Here is the Pythagorean theorem: $a^2 + b^2 = c^2$ in text.";
    let doc = parse_markdown(md);
    let opts = HtmlOptions::default();
    let html = render_html_document(&doc, &opts).expect("render html document");

    assert!(
        html.contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"inline\">"),
        "should contain inline <math>: {html}"
    );
    assert!(html.contains("<msup>"), "should contain power superscript <msup>");
    assert!(html.contains("<mi>a</mi>"), "should contain variable a");
    assert!(html.contains("<mi>b</mi>"), "should contain variable b");
    assert!(html.contains("<mi>c</mi>"), "should contain variable c");
}

#[test]
fn test_display_math_renders_block_mathml() {
    let md = "$$\\int_0^\\infty e^{-x} dx = 1$$";
    let doc = parse_markdown(md);
    let opts = HtmlOptions::default();
    let html = render_html_document(&doc, &opts).expect("render html document");

    assert!(
        html.contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">"),
        "should contain display block <math>: {html}"
    );
    assert!(html.contains("<msubsup>"), "should contain integral sub/sup");
    assert!(html.contains("<mi>∞</mi>"), "should contain infinity");
}

#[test]
fn test_fenced_math_code_block() {
    let md = "```math\nE = mc^2\n```";
    let doc = parse_markdown(md);
    let opts = HtmlOptions::default();
    let html = render_html_document(&doc, &opts).expect("render html document");

    assert!(
        html.contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">"),
        "fenced math block should render display mathml: {html}"
    );
    assert!(html.contains("<mi>E</mi>"));
    assert!(html.contains("<mi>m</mi>"));
    assert!(html.contains("<msup>"));
}

#[test]
fn test_escaped_dollar_does_not_trigger_math() {
    let md = "The price is \\$100 and the tax is \\$10.";
    let doc = parse_markdown(md);
    let opts = HtmlOptions::default();
    let html = render_html_document(&doc, &opts).expect("render html document");

    assert!(!html.contains("<math"), "escaped dollar must not produce mathml");
    assert!(html.contains("$100"));
    assert!(html.contains("$10"));
}

#[test]
fn test_html_custom_lang_attribute() {
    let md = "# Hallo Welt\n\nEin mathematischer Ausdruck: $e^{i\\pi} + 1 = 0$.";
    let doc = parse_markdown(md);
    let mut opts = HtmlOptions::default();
    opts.lang = Some("de".to_string());
    let html = render_html_document(&doc, &opts).expect("render html document");

    assert!(html.contains("<html lang=\"de\">"), "html lang attribute must match: {html}");
    assert!(html.contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"inline\">"));
}

#[test]
fn test_mathml_well_formedness() {
    let formulas = [
        "x + y = z",
        "\\frac{a + b}{c + d}",
        "\\sqrt{x^2 + 1}",
        "\\sum_{k=1}^n k = \\frac{n(n+1)}{2}",
        "\\mathbf{v} = (v_1, v_2, \\dots, v_n)",
    ];

    for formula in formulas {
        let md = format!("${formula}$");
        let html = render_html(&md, &HtmlOptions::default()).expect("render html");
        assert!(
            html.contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"inline\">"),
            "formula `{formula}` should produce mathml, got: {html}"
        );
    }
}
