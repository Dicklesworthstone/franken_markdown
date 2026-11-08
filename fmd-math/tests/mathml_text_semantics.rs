//! Text boxes must preserve the mathematical conditions and formatting they
//! contain, including when a script or fraction selects a smaller style.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{
    mathml_well_formed, parse, parse_display, parse_text, to_mathml, to_mathml_element,
};

fn render(source: &str, display: bool) -> String {
    let xml = to_mathml(&parse(source).unwrap(), display);
    mathml_well_formed(&xml).unwrap();
    xml
}

#[test]
fn mathematical_conditions_inside_text_are_not_dropped() {
    let source = r"x=1\text{ if $n_1>0$ and $m^2=4$}";
    for display in [false, true] {
        let xml = render(source, display);
        for required in [
            "<mtext> if </mtext>",
            "<msub><mi>n</mi><mn>1</mn></msub>",
            "<mo>&gt;</mo><mn>0</mn>",
            "<mtext> and </mtext>",
            "<msup><mi>m</mi><mn>2</mn></msup>",
            "<mo>=</mo><mn>4</mn>",
        ] {
            assert!(xml.contains(required), "missing {required}: {xml}");
        }
        assert_eq!(xml.matches("<mi>n</mi>").count(), 1);
        assert_eq!(xml.matches("<mi>m</mi>").count(), 1);
    }
    // Text-mode mathematical commands also form islands without literal '$'.
    let xml = render(r"\text{angle \alpha and ratio \frac{p}{q}}", true);
    assert!(xml.contains("<mi>α</mi>") && xml.contains("<mfrac>"));
    assert!(xml.contains("<mi>p</mi>") && xml.contains("<mi>q</mi>"));
}

#[test]
fn nested_text_fonts_restore_and_do_not_leak_into_math_islands() {
    let xml = render(
        r"\textbf{before \textit{inside} after $x+\text{plain}$ end}\text{ outside}",
        true,
    );
    for required in [
        "<mtext mathvariant=\"bold\">before </mtext>",
        "<mtext mathvariant=\"italic\">inside</mtext>",
        "<mtext mathvariant=\"bold\"> after </mtext>",
        "<mi>x</mi>",
        "<mtext>plain</mtext>",
        "<mtext mathvariant=\"bold\"> end</mtext>",
        "<mtext> outside</mtext>",
    ] {
        assert!(xml.contains(required), "missing {required}: {xml}");
    }
    assert!(!xml.contains("mathvariant=\"bold\">plain"));
    let xml = render(r"\text{\underline{under $x_i$} beside}", true);
    assert!(xml.contains("<munder accentunder=\"true\">"));
    assert!(xml.contains("<msub><mi>x</mi><mi>i</mi></msub>"));
    assert!(xml.contains("<mo stretchy=\"true\">_</mo></munder>"));
    assert!(xml.ends_with("<mtext> beside</mtext></mrow></math>"));
}

#[test]
fn text_colors_sizes_and_explicit_spacing_keep_their_scopes() {
    let xml = render(
        r"\text{left\hspace{2em}\textcolor{red}{red $n_i$} middle {\small small $m$} right}",
        true,
    );
    assert!(xml.contains("<mspace width=\"2em\"></mspace>"));
    assert!(xml.contains("<mstyle mathcolor=\"red\">"), "{xml}");
    assert!(xml.contains("<mi>n</mi>") && xml.contains("<mi>i</mi>"));
    assert!(
        xml.contains("</mstyle></mrow><mtext> middle </mtext>"),
        "{xml}"
    );
    assert!(xml.contains("mathsize=\"90%\""), "{xml}");
    assert!(
        xml.ends_with("</mrow><mtext> right</mtext></mrow></math>"),
        "{xml}"
    );
    let xml = render(r"\text{a\,b\quad c\!d}", true);
    for width in ["0.167em", "1em", "-0.167em"] {
        assert!(
            xml.contains(&format!("<mspace width=\"{width}\">")),
            "{xml}"
        );
    }
}

#[test]
fn plain_text_and_ordinary_structures_keep_their_existing_bytes() {
    let prefix = "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">";
    for (source, body) in [
        (
            r"\text{hello {world}~\&}",
            "<mtext>hello world\u{a0}&amp;</mtext>",
        ),
        (r"\textbf{bold}", "<mtext mathvariant=\"bold\">bold</mtext>"),
        (
            r"\textit{italic}",
            "<mtext mathvariant=\"italic\">italic</mtext>",
        ),
        (r"\text{}", "<mtext></mtext>"),
        (
            r"x_{i}^2",
            "<msubsup><mi>x</mi><mrow><mi>i</mi></mrow><mn>2</mn></msubsup>",
        ),
        (
            r"\frac{a}{b}",
            "<mfrac><mrow><mi>a</mi></mrow><mrow><mi>b</mi></mrow></mfrac>",
        ),
    ] {
        assert_eq!(
            render(source, true),
            format!("{prefix}{body}</math>"),
            "{source}"
        );
    }
}

#[test]
fn math_islands_follow_script_fraction_and_display_context() {
    // Each branch is a distinct authored atom, so an incorrect style changes
    // visible scientific content rather than merely the serializer's shape.
    let choice = r"\mathchoice{D}{T}{S}{U}";
    #[cfg(feature = "bundled-faces")]
    let engine = fmd_math::Engine::bundled().unwrap();
    for (template, display, expected) in [
        (r"\text{$CHOICE$}", true, 'T'),
        (r"\text{$$CHOICE$$}", false, 'D'),
        (r"x_{\text{$CHOICE$}}", true, 'S'),
        (r"x^{y_{\text{$CHOICE$}}}", true, 'U'),
        (r"x_{\text{$$CHOICE$$}}", true, 'S'),
        (r"\frac{\text{$CHOICE$}}{z}", true, 'T'),
        (r"\frac{\text{$CHOICE$}}{z}", false, 'S'),
        (r"\frac{\frac{\text{$CHOICE$}}{y}}{z}", true, 'S'),
        (r"\tfrac{\text{$CHOICE$}}{z}", true, 'S'),
        (r"\dfrac{\text{$CHOICE$}}{z}", false, 'T'),
        (r"\sqrt[\text{$CHOICE$}]{x}", true, 'U'),
        (r"\overset{\text{$CHOICE$}}{x}", true, 'S'),
        (r"\xrightarrow{\text{$CHOICE$}}", true, 'S'),
    ] {
        let source = template.replace("CHOICE", choice);
        let xml = render(&source, display);
        for candidate in ['D', 'T', 'S', 'U'] {
            assert_eq!(
                xml.contains(&format!("<mi>{candidate}</mi>")),
                candidate == expected,
                "wrong mathematical branch for {source}: {xml}"
            );
        }
        #[cfg(feature = "bundled-faces")]
        {
            let style = if display {
                fmd_math::Style::Display
            } else {
                fmd_math::Style::Text
            };
            let layout = engine.typeset(&source, style).unwrap();
            let actual: Vec<_> = layout
                .glyphs
                .iter()
                .filter(|g| ['D', 'T', 'S', 'U'].contains(&g.ch))
                .map(|g| g.ch)
                .collect();
            assert_eq!(actual, vec![expected], "layout reference: {source}");
        }
    }
    let inline = render(r"\text{$\sum_{i=1}^n i$}", true);
    assert!(inline.contains("<mstyle displaystyle=\"false\">"));
    assert!(inline.contains("<msubsup>") && !inline.contains("<munderover>"));
    let display = render(r"\text{$$\sum_{i=1}^n i$$}", false);
    assert!(display.contains("<mstyle displaystyle=\"true\">"));
    assert!(display.contains("<munderover>"));
    let explicit = render(r"\text{$\displaystyle\sum_{i=1}^n i$}", true);
    assert!(explicit.contains("<mstyle displaystyle=\"true\" scriptlevel=\"0\">"));
    assert!(explicit.contains("<munderover>"));
    let explicit = render(r"\text{$$\textstyle\sum_{i=1}^n i$$}", false);
    assert!(explicit.contains("<mstyle displaystyle=\"false\" scriptlevel=\"0\">"));
    assert!(explicit.contains("<msubsup>") && !explicit.contains("<munderover>"));
}

#[test]
fn text_math_and_label_entry_points_share_escaping_and_semantics() {
    let text = r"\textbf{Café \& <proof> $n_1$}";
    let tree = parse_text(text).unwrap();
    let ordinary = to_mathml(&tree, false);
    let element = to_mathml_element(&tree);
    let label = parse_display(&format!(r"x\tag*{{{text}}}"))
        .unwrap()
        .tag_mathml()
        .unwrap();
    for xml in [&ordinary, &element, &label] {
        mathml_well_formed(xml).unwrap();
        assert!(xml.contains("Café &amp; &lt;proof&gt;"), "{xml}");
        assert!(!xml.contains("<proof>"));
        assert!(xml.contains("<msub><mi>n</mi><mn>1</mn></msub>"), "{xml}");
    }
}
