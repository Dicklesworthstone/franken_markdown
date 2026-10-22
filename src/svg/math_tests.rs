//! Real shared-engine geometry and poster integration regressions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use franken_markdown::math::{Engine, Style};

fn poster() -> Poster {
    Poster::new(&SvgOptions::default())
}

#[test]
fn fraction_uses_shared_engine_positions_and_rule_geometry() {
    let mut p = poster();
    let source = r"\frac{1}{2}";
    let reference = Engine::bundled()
        .unwrap()
        .typeset(source, Style::Text)
        .unwrap();
    let word = p.math_word(source, false, RStyle::BODY, 11.0, 400.0);
    assert!(word.warning.is_none());
    let run = word.formula.unwrap();
    p.draw_math(&run, 20.0, 80.0, Ink::Fg);
    let glyphs: Vec<_> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph {
                slot, gid, y, size, ..
            } => Some((*slot, *gid, *y, *size)),
            _ => None,
        })
        .collect();
    assert_eq!(glyphs.len(), reference.glyphs.len());
    for (actual, expected) in glyphs.iter().zip(&reference.glyphs) {
        assert_eq!(actual.0, SLOT_COUNT + expected.face.0);
        assert_eq!(actual.1, expected.gid);
        assert!((actual.2 - (80.0 - expected.y * 11.0)).abs() < 0.00001);
        assert!((actual.3 - expected.size * 11.0).abs() < 0.00001);
    }
    assert!(
        glyphs[0].2 < glyphs[1].2,
        "numerator must be above denominator"
    );
    let rule = &reference.rules[0];
    assert!(p.ops.iter().any(|op| match op {
        Op::Rect {
            y,
            w,
            h,
            stroke: None,
            ..
        } => {
            (*y - (80.0 - (rule.y + rule.height) * 11.0)).abs() < 0.00001
                && (*w - rule.width * 11.0).abs() < 0.00001
                && (*h - rule.height * 11.0).abs() < 0.00001
        }
        _ => false,
    }));
}

#[test]
fn display_fraction_is_not_a_literal_code_panel() {
    let doc = Document {
        blocks: vec![Block::MathBlock(r"\frac{1}{2}".into())],
    };
    let (bytes, report, warnings) = render_svg_with_diagnostics(&doc, &SvgOptions::default());
    let svg = String::from_utf8(bytes).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(report.glyphs_drawn, 2);
    assert_eq!(svg.matches("<use href=").count(), 2);
    assert!(!svg.contains("<text"));
    assert!(!svg.contains("foreignObject"));
}

#[test]
fn radicals_and_stretchy_delimiters_emit_closed_vector_paths() {
    // Delimiters and radicals up to cmex's largest size are cmex glyphs; a
    // radical past its 3.0 em variant is still a drawn construction (fmd-math
    // has no radical extensible), and drawn constructions must emit closed
    // vector paths.
    let mut p = poster();
    p.math_block(
        r"\left(\sqrt{\begin{matrix}a\\b\\c\\d\end{matrix}}\right)",
        72.0,
        540.0,
        false,
    );
    assert!(p.warnings.is_empty());
    assert!(
        p.ops.iter().any(|op| matches!(op, Op::Glyph { .. })),
        "cmex parens are glyphs"
    );
    let paths: Vec<_> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Path { data, .. } => Some(data),
            _ => None,
        })
        .collect();
    assert!(!paths.is_empty(), "drawn radicals must not be lost");
    assert!(
        paths
            .iter()
            .all(|path| path.starts_with('M') && path.ends_with('Z'))
    );
    let (bytes, _, _) = p.emit(400.0);
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<path d=\"M"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn inline_formula_keeps_adjacent_punctuation_and_grows_line_height() {
    let p = poster();
    let pieces = vec![
        Piece::Text("value=".into(), RStyle::BODY),
        Piece::Math(r"\dfrac{1}{\frac{2}{3}}".into(), false, RStyle::BODY),
        Piece::Text(",done".into(), RStyle::BODY),
    ];
    let lines = p.wrap(&pieces, 11.0, 400.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].len(), 3);
    assert!(lines[0].iter().all(|word| word.gap == 0.0));
    let (above, height) = p.line_metrics(&lines[0], 9.35, 16.0);
    assert!(above > 9.35);
    assert!(height > 16.0);
}

#[test]
fn table_rows_reserve_full_formula_height() {
    let mut p = poster();
    let source = r"\dfrac{1}{\dfrac{2}{\dfrac{3}{4}}}";
    let size = f64::from(p.scale.table);
    let word = p.math_word(source, false, RStyle::BODY, size, 400.0);
    let formula = word.formula.unwrap();
    let expected = (formula.formula.ascent + formula.formula.descent) * formula.size;
    let table = Table {
        head: vec![vec![Inline::Text("H".into())]],
        align: vec![Align::Left],
        rows: vec![vec![vec![Inline::Math(source.into())]]],
    };
    let top = p.y;
    p.table(&table, 72.0, 540.0);
    assert!(p.y - top >= expected + 16.0);
    assert!(p.warnings.is_empty());
}

#[test]
fn oversized_formula_is_scaled_as_one_indivisible_box() {
    let p = poster();
    let source = "x+".repeat(80) + "x";
    let word = p.math_word(&source, false, RStyle::BODY, 11.0, 36.0);
    assert!(word.w <= 36.00001);
    assert!(word.formula.as_ref().unwrap().size < 11.0);
    let lines = p.wrap(&[Piece::Math(source, false, RStyle::BODY)], 11.0, 36.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].len(), 1);
    assert!(lines[0][0].formula.is_some());
}

#[test]
fn formula_ink_stays_inside_measured_bounds() {
    let mut p = poster();
    let word = p.math_word(r"\int_0^1 x^2\,dx", true, RStyle::BODY, 11.0, 400.0);
    let run = word.formula.unwrap();
    let x = 30.0;
    let baseline = 90.0;
    p.draw_math(&run, x, baseline, Ink::Fg);
    for op in &p.ops {
        if let Op::Glyph {
            slot,
            gid,
            x: gx,
            y: gy,
            size,
            ..
        } = op
        {
            let font = p.font_for_slot(*slot).unwrap();
            let bbox = font.glyph_bbox(*gid).unwrap();
            let scale = *size / f64::from(font.units_per_em);
            assert!(*gx + f64::from(bbox[0]) * scale >= x - 0.00001);
            assert!(*gx + f64::from(bbox[2]) * scale <= x + word.w + 0.00001);
            assert!(
                *gy - f64::from(bbox[3]) * scale
                    >= baseline - run.formula.ascent * run.size - 0.00001
            );
            assert!(
                *gy - f64::from(bbox[1]) * scale
                    <= baseline + run.formula.descent * run.size + 0.00001
            );
        }
    }
}

#[test]
fn unsupported_inline_math_warns_once_even_when_wrapped() {
    let mut p = poster();
    let source = r"\fmdUnknownCommand{unsupported}";
    let pieces = vec![Piece::Math(source.into(), false, RStyle::BODY)];
    let lines = p.wrap(&pieces, 11.0, 36.0);
    assert!(
        p.warnings.is_empty(),
        "measurement must not emit diagnostics"
    );
    assert!(lines.len() > 1);
    for (index, line) in lines.iter().enumerate() {
        p.draw_words(line, 0.0, 20.0 + index as f64 * 20.0, 11.0);
    }
    assert_eq!(p.warnings.len(), 1);
    assert_eq!(p.warnings[0].code, "svg_math_unsupported");
    let fallback: String = lines
        .iter()
        .flatten()
        .map(|word| word.text.as_str())
        .collect();
    assert_eq!(fallback, source);
}

#[test]
fn source_budget_returns_visible_fallback_with_typed_diagnostic() {
    let p = poster();
    let source = "x".repeat(64 * 1024 + 1);
    let word = p.math_word(&source, false, RStyle::BODY, 11.0, 400.0);
    assert!(word.formula.is_none());
    assert_eq!(word.warning.unwrap().code, "svg_math_limit");
    assert_eq!(word.text, source);
}

#[test]
fn plain_prose_does_not_load_the_math_faces() {
    let mut p = poster();
    p.paragraph(&[Inline::Text("ordinary prose".into())], 72.0, 540.0, false);
    assert!(p.math_engine.get().is_none());
    assert!(p.warnings.is_empty());
}

#[test]
fn formula_output_and_diagnostics_are_deterministic() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph(vec![Inline::Math(r"x^2+\frac{1}{y}".into())]),
            Block::MathBlock(r"\sqrt{x}".into()),
            Block::MathBlock(r"\fmdUnknownCommand{x}".into()),
        ],
    };
    let options = SvgOptions::default();
    let first = render_svg_with_diagnostics(&doc, &options);
    let second = render_svg_with_diagnostics(&doc, &options);
    assert_eq!(first, second);
    assert_eq!(first.2.len(), 1);
    let (legacy_bytes, legacy_report) = render_svg_with_report(&doc, &options);
    assert_eq!(legacy_bytes, first.0);
    assert_eq!(legacy_report, first.1);
}

#[test]
fn quote_math_inherits_quote_ink() {
    let mut p = poster();
    p.math_block("x^2", 72.0, 540.0, true);
    assert!(p.ops.iter().any(|op| matches!(
        op,
        Op::Glyph {
            ink: Ink::QuoteFg,
            ..
        }
    )));
    assert!(
        !p.ops
            .iter()
            .any(|op| matches!(op, Op::Glyph { ink: Ink::Fg, .. }))
    );
}

#[test]
fn scoped_math_colors_reach_svg_glyphs_and_fraction_rules() {
    let source = r"a {\color{red}\frac{b}{\textcolor{blue}{c}}} d";
    let mut options = SvgOptions::default();
    options.theme.colors.fg = "#234567".into();
    for block in [
        Block::MathBlock(source.into()),
        Block::Paragraph(vec![Inline::Math(source.into())]),
    ] {
        let document = Document {
            blocks: vec![block],
        };
        let (bytes, _, warnings) = render_svg_with_diagnostics(&document, &options);
        assert!(warnings.is_empty());
        let svg = String::from_utf8(bytes).unwrap();
        let fills: Vec<_> = svg
            .lines()
            .filter(|line| line.starts_with("<use href=\"#g"))
            .map(|line| {
                line.split_once("fill=\"")
                    .unwrap()
                    .1
                    .split('"')
                    .next()
                    .unwrap()
            })
            .collect();
        assert_eq!(
            fills,
            ["#234567", "rgb(100%,0%,0%)", "rgb(0%,0%,100%)", "#234567"]
        );
        let rules: Vec<_> = svg
            .lines()
            .filter(|line| line.starts_with("<rect ") && line.contains("fill=\"rgb("))
            .collect();
        assert_eq!(rules.len(), 1);
        assert!(rules[0].contains("fill=\"rgb(100%,0%,0%)\""));
        assert_eq!(
            svg.as_bytes(),
            render_svg_with_diagnostics(&document, &options).0
        );
    }
}

#[test]
fn constructed_math_paths_and_color_mixes_reach_svg_without_palette_quantization() {
    let source = r"\color[rgb]{0.12345,0.5,0.75}\sqrt{\begin{matrix}a\\b\\c\\d\end{matrix}}";
    let document = Document {
        blocks: vec![Block::MathBlock(source.into())],
    };
    let (bytes, _, warnings) = render_svg_with_diagnostics(&document, &SvgOptions::default());
    assert!(warnings.is_empty());
    let svg = String::from_utf8(bytes).unwrap();
    let paths: Vec<_> = svg
        .lines()
        .filter(|line| line.starts_with("<path d="))
        .collect();
    assert!(!paths.is_empty(), "exercise a constructed radical");
    for line in svg.lines().filter(|line| {
        line.starts_with("<use href=\"#g")
            || line.starts_with("<path d=")
            || (line.starts_with("<rect ") && line.contains("fill=\"rgb("))
    }) {
        assert!(line.contains("fill=\"rgb(12.345%,50%,75%)\""), "{line}");
    }
    let mixed = Document {
        blocks: vec![Block::MathBlock(r"\color{red!30!blue} x".into())],
    };
    let svg = String::from_utf8(render_svg(&mixed, &SvgOptions::default())).unwrap();
    assert!(svg.contains("fill=\"rgb(30%,0%,70%)\""));
}

#[test]
fn colored_overlays_emit_svg_elements_in_tex_order() {
    use franken_markdown::math::{Color, Placed};

    let engine = Engine::bundled().unwrap();
    let red = Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
    };
    let blue = Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
    };
    for source in [
        r"{\color{red}\frac{a}{b}}\!\!\!{\color{blue}X}",
        r"{\color{red}\sqrt{\begin{matrix}a\\b\\c\\d\end{matrix}}}\!\!\!{\color{blue}X}",
    ] {
        for inline in [false, true] {
            let layout = engine
                .typeset(source, if inline { Style::Text } else { Style::Display })
                .unwrap();
            let expected: Vec<_> = layout
                .order
                .iter()
                .filter_map(|placed| {
                    let (kind, color) = match *placed {
                        Placed::Glyph(index) => {
                            let glyph = &layout.glyphs[index];
                            if glyph.ch.is_whitespace() {
                                return None;
                            }
                            ("glyph", glyph.color.unwrap())
                        }
                        Placed::Rule(index) => ("rule", layout.rules[index].color.unwrap()),
                        Placed::Path(index) => ("path", layout.paths[index].color.unwrap()),
                    };
                    let fill = if color == red {
                        "rgb(100%,0%,0%)"
                    } else {
                        assert_eq!(color, blue);
                        "rgb(0%,0%,100%)"
                    };
                    Some((kind, fill))
                })
                .collect();
            let document = Document {
                blocks: vec![if inline {
                    Block::Paragraph(vec![Inline::Math(source.into())])
                } else {
                    Block::MathBlock(source.into())
                }],
            };
            let (bytes, _, warnings) =
                render_svg_with_diagnostics(&document, &SvgOptions::default());
            assert!(warnings.is_empty());
            let svg = String::from_utf8(bytes).unwrap();
            let actual: Vec<_> = svg
                .lines()
                .filter_map(|line| {
                    let kind = if line.starts_with("<use href=\"#g") {
                        "glyph"
                    } else if line.starts_with("<rect ") {
                        "rule"
                    } else if line.starts_with("<path d=") {
                        "path"
                    } else {
                        return None;
                    };
                    let fill = line.split_once("fill=\"")?.1.split('"').next()?;
                    fill.starts_with("rgb(").then_some((kind, fill))
                })
                .collect();
            assert_eq!(actual, expected, "{source}, inline={inline}");
        }
    }
}

#[test]
fn color_does_not_change_math_geometry_or_escape_quote_ink() {
    let mut plain = poster();
    let mut colored = poster();
    plain.math_block(r"a {\frac{b}{c}} d", 72.0, 540.0, true);
    colored.math_block(r"a {\color{red}\frac{b}{c}} d", 72.0, 540.0, true);
    assert!(plain.warnings.is_empty());
    assert!(colored.warnings.is_empty());
    let glyph_inks: Vec<_> = colored
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph { ink, .. } => Some(*ink),
            _ => None,
        })
        .collect();
    assert_eq!(glyph_inks.first(), Some(&Ink::QuoteFg));
    assert_eq!(glyph_inks.last(), Some(&Ink::QuoteFg));
    let mut recolored = 0;
    for op in &mut colored.ops {
        let ink = match op {
            Op::Glyph { ink, .. } | Op::Path { ink, .. } => ink,
            Op::Rect { fill, .. } => fill,
            _ => continue,
        };
        if matches!(ink, Ink::Rgb(_)) {
            *ink = Ink::QuoteFg;
            recolored += 1;
        }
    }
    assert_eq!(recolored, 3, "two fraction glyphs and its rule");
    // Explicit colour enables TeX's paint order, but every primitive keeps
    // exactly the same geometry as the formula with inherited colour.
    assert_eq!(plain.ops.len(), colored.ops.len());
    for op in &plain.ops {
        let index = colored
            .ops
            .iter()
            .position(|candidate| candidate == op)
            .unwrap();
        colored.ops.remove(index);
    }
    assert!(colored.ops.is_empty());
    assert_eq!(plain.y, colored.y);
}
