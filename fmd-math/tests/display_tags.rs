#![cfg(feature = "bundled-faces")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{Budgets, Engine, Layout, MathError, PathSeg, Style, parse, parse_display};

fn ink(engine: &Engine, layout: &Layout) -> (f64, f64) {
    let contours = fmd_math::paths::resolve_paths(engine, layout).unwrap();
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut point = |x: f64| {
        left = left.min(x);
        right = right.max(x);
    };
    for contour in contours {
        point(contour.start.0);
        for segment in contour.segments {
            match segment {
                PathSeg::Line { to } => point(to.0),
                PathSeg::Quad { ctrl, to } => {
                    point(ctrl.0);
                    point(to.0);
                }
            }
        }
    }
    (left, right)
}

fn partition(layout: &Layout, tag_start: usize, label: bool) -> Layout {
    Layout {
        glyphs: layout
            .glyphs
            .iter()
            .filter(|g| (g.span.start >= tag_start) == label)
            .cloned()
            .collect(),
        rules: layout
            .rules
            .iter()
            .filter(|r| (r.span.start >= tag_start) == label)
            .cloned()
            .collect(),
        paths: layout
            .paths
            .iter()
            .filter(|p| (p.span.start >= tag_start) == label)
            .cloned()
            .collect(),
        ..Layout::default()
    }
}

#[test]
fn explicit_labels_are_text_with_exact_source_spans_and_optional_parentheses() {
    let engine = Engine::bundled().unwrap();
    for (command, expected) in [(r"\tag{Energy}", "(Energy)"), (r"\tag*{Energy}", "Energy")] {
        let source = format!("x^2 {command}");
        let parsed = parse_display(&source).unwrap();
        let mathml = parsed.tag_mathml().unwrap();
        assert!(
            mathml.contains("<mtext>"),
            "label must be upright text: {mathml}"
        );
        let layout = engine.typeset_display(&source, 40.0).unwrap();
        let tag_start = source.find("\\tag").unwrap();
        let label = partition(&layout, tag_start, true);
        assert_eq!(
            label.glyphs.iter().map(|g| g.ch).collect::<String>(),
            expected
        );
        let upright = engine.typeset_text(expected).unwrap();
        assert_eq!(label.glyphs.len(), upright.glyphs.len());
        for (actual, expected) in label.glyphs.iter().zip(upright.glyphs) {
            assert_eq!(
                (actual.face, actual.gid, actual.size),
                (expected.face, expected.gid, expected.size)
            );
            let slice = &source[actual.span.start..actual.span.end];
            if actual.ch == '(' || actual.ch == ')' {
                assert_eq!(slice, "\\tag");
            } else {
                assert_eq!(slice, actual.ch.to_string());
            }
        }
        assert!(
            parse(&source).is_err(),
            "intrinsic/inline parse cannot discard a label"
        );
    }
}

#[test]
fn label_placement_uses_the_column_and_reserves_actual_outline_clearance() {
    let engine = Engine::bundled().unwrap();
    for source in [
        r"x^2\tag{1}",
        r"\hspace{-3em}\frac{a+b}{\sqrt{c}}\tag*{Long label}",
        r"\overrightarrow{AB}\tag*{\hspace{-2em}Energy}",
        r"\begin{aligned}a&=b\\c&=d\end{aligned}\tag{System}",
        r"a&=b\\c&=d\tag{System}",
    ] {
        let tag_start = source.rfind("\\tag").unwrap();
        for width in [0.5, 20.0, 80.0] {
            let layout = engine.typeset_display(source, width).unwrap();
            let body = ink(&engine, &partition(&layout, tag_start, false));
            let tag = ink(&engine, &partition(&layout, tag_start, true));
            assert!(
                body.1 + 0.99 <= tag.0,
                "body/tag ink collided: {source} {body:?} {tag:?}"
            );
            assert!(body.0 >= -1e-9 && tag.1 <= layout.width + 1e-9);
            assert!(
                layout.width >= width,
                "a narrow column must not clip the pair"
            );
            assert_eq!(layout, engine.typeset_display(source, width).unwrap());
            assert_eq!(
                layout.order.len(),
                layout.glyphs.len() + layout.rules.len() + layout.paths.len()
            );
        }
        let narrow = engine.typeset_display(source, 40.0).unwrap();
        let wide = engine.typeset_display(source, 80.0).unwrap();
        let narrow_tag = ink(&engine, &partition(&narrow, tag_start, true));
        let wide_tag = ink(&engine, &partition(&wide, tag_start, true));
        assert!((wide_tag.1 - narrow_tag.1 - (wide.width - narrow.width)).abs() < 1e-8);
        assert_eq!(
            narrow.glyphs.iter().map(|g| g.size).collect::<Vec<_>>(),
            wide.glyphs.iter().map(|g| g.size).collect::<Vec<_>>()
        );
    }
}

#[test]
fn untagged_display_layouts_are_unchanged_at_every_containing_width() {
    let engine = Engine::bundled().unwrap();
    for source in [
        "x+y",
        r"\frac{1}{2}",
        r"a&=b\\c&=d",
        r"\begin{matrix}a&b\\c&d\end{matrix}",
    ] {
        let expected = engine.typeset(source, Style::Display).unwrap();
        for width in [1.0, 40.0, 100.0] {
            assert_eq!(engine.typeset_display(source, width).unwrap(), expected);
        }
        assert_eq!(
            parse_display(source).unwrap().body(),
            &parse(source).unwrap()
        );
    }
}

#[test]
fn tag_syntax_is_strict_and_nested_or_numbered_constructs_are_not_reinterpreted() {
    let engine = Engine::bundled().unwrap();
    for source in [
        r"x\tag",
        r"x\tag a",
        r"x\tag{unclosed",
        r"x\tag{a}\tag{b}",
        r"{x\tag{a}}",
        r"x^{y\tag{a}}",
        r"\text{x\tag{a}}",
        r"x\tag{a\tag{b}}",
        r"\begin{aligned}x&=y\tag{a}\end{aligned}",
        r"\begin{equation}x=y\end{equation}",
        r"\begin{align}x&=y\end{align}",
    ] {
        assert!(parse_display(source).is_err(), "must refuse {source}");
        assert!(
            engine.typeset_display(source, 40.0).is_err(),
            "must not paint a partial {source}"
        );
    }
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY, 1_000_001.0] {
        assert!(engine.typeset_display(r"x\tag{1}", width).is_err());
    }
}

#[test]
fn tags_use_the_existing_macro_and_text_grammars_without_slicing_rewritten_source() {
    let engine = Engine::bundled().unwrap();
    let source =
        "\\newcommand{\\label}[1]{\\tag*{#1}} x \\label{Café \\& $n_1$} % \\tag{ignored}\n";
    let parsed = parse_display(source).unwrap();
    let tag = parsed.tag_mathml().unwrap();
    assert!(tag.contains("Café "));
    assert!(tag.contains("&amp;"));
    assert!(tag.contains("<msub>"));
    fmd_math::mathml_well_formed(&tag).unwrap();
    let layout = engine.typeset_display(source, 40.0).unwrap();
    for glyph in &layout.glyphs {
        assert!(source.get(glyph.span.start..glyph.span.end).is_some());
    }
    assert!(
        parse_display(r"x % \tag{only a comment}")
            .unwrap()
            .tag()
            .is_none()
    );
    let nested = parse_display(r"x\tag*{\textbf{Case $n_1$: \text{if $x>0$}}}").unwrap();
    let tag = nested.tag_mathml().unwrap();
    assert!(tag.contains("mathvariant=\"bold\">Case "));
    assert!(tag.contains("<msub>") && tag.contains("<mi>x</mi>"));
    assert!(tag.contains("&gt;") && tag.contains("<mn>0</mn>"));
    fmd_math::mathml_well_formed(&tag).unwrap();
}

#[test]
fn body_and_label_share_the_same_output_budget() {
    let engine = Engine::bundled().unwrap().with_budgets(Budgets {
        max_glyphs: 5,
        ..Budgets::default()
    });
    assert!(engine.typeset("abc", Style::Display).is_ok());
    assert!(engine.typeset_text("def").is_ok());
    assert!(matches!(
        engine.typeset_display(r"abc\tag*{def}", 40.0),
        Err(MathError::Budget {
            kind: fmd_math::BudgetKind::Glyphs,
            limit: 5,
            ..
        })
    ));
}
