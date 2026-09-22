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
    let reference = Engine::bundled().unwrap().typeset(source, Style::Text).unwrap();
    let word = p.math_word(source, false, RStyle::BODY, 11.0, 400.0);
    assert!(word.warning.is_none());
    let run = word.formula.unwrap();
    p.draw_math(&run, 20.0, 80.0, Ink::Fg);
    let glyphs: Vec<_> = p.ops.iter().filter_map(|op| match op {
        Op::Glyph { slot, gid, y, size, .. } => Some((*slot, *gid, *y, *size)),
        _ => None,
    }).collect();
    assert_eq!(glyphs.len(), reference.glyphs.len());
    for (actual, expected) in glyphs.iter().zip(&reference.glyphs) {
        assert_eq!(actual.0, SLOT_COUNT + expected.face.0);
        assert_eq!(actual.1, expected.gid);
        assert!((actual.2 - (80.0 - expected.y * 11.0)).abs() < 0.00001);
        assert!((actual.3 - expected.size * 11.0).abs() < 0.00001);
    }
    assert!(glyphs[0].2 < glyphs[1].2, "numerator must be above denominator");
    let rule = &reference.rules[0];
    assert!(p.ops.iter().any(|op| match op {
        Op::Rect { y, w, h, stroke: None, .. } => {
            (*y - (80.0 - (rule.y + rule.height) * 11.0)).abs() < 0.00001
                && (*w - rule.width * 11.0).abs() < 0.00001
                && (*h - rule.height * 11.0).abs() < 0.00001
        }
        _ => false,
    }));
}

#[test]
fn display_fraction_is_not_a_literal_code_panel() {
    let doc = Document { blocks: vec![Block::MathBlock(r"\frac{1}{2}".into())] };
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
    let mut p = poster();
    p.math_block(r"\left(\sqrt{\frac{1}{x^2}}\right)", 72.0, 540.0, false);
    assert!(p.warnings.is_empty());
    let paths: Vec<_> = p.ops.iter().filter_map(|op| match op {
        Op::Path { data, .. } => Some(data), _ => None,
    }).collect();
    assert!(!paths.is_empty(), "drawn radicals must not be lost");
    assert!(paths.iter().all(|path| path.starts_with('M') && path.ends_with('Z')));
    let (bytes, _, _) = p.emit(400.0);
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("<path d=\"M"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn inline_formula_keeps_adjacent_punctuation_and_grows_line_height() {
    let p = poster();
    let pieces = vec![Piece::Text("value=".into(), RStyle::BODY),
        Piece::Math(r"\dfrac{1}{\frac{2}{3}}".into(), false, RStyle::BODY),
        Piece::Text(",done".into(), RStyle::BODY)];
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
        if let Op::Glyph { slot, gid, x: gx, y: gy, size, .. } = op {
            let font = p.font_for_slot(*slot).unwrap();
            let bbox = font.glyph_bbox(*gid).unwrap();
            let scale = *size / f64::from(font.units_per_em);
            assert!(*gx + f64::from(bbox[0]) * scale >= x - 0.00001);
            assert!(*gx + f64::from(bbox[2]) * scale <= x + word.w + 0.00001);
            assert!(*gy - f64::from(bbox[3]) * scale >=
                baseline - run.formula.ascent * run.size - 0.00001);
            assert!(*gy - f64::from(bbox[1]) * scale <=
                baseline + run.formula.descent * run.size + 0.00001);
        }
    }
}

#[test]
fn unsupported_inline_math_warns_once_even_when_wrapped() {
    let mut p = poster();
    let source = r"\fmdUnknownCommand{unsupported}";
    let pieces = vec![Piece::Math(source.into(), false, RStyle::BODY)];
    let lines = p.wrap(&pieces, 11.0, 36.0);
    assert!(p.warnings.is_empty(), "measurement must not emit diagnostics");
    assert!(lines.len() > 1);
    for (index, line) in lines.iter().enumerate() {
        p.draw_words(line, 0.0, 20.0 + index as f64 * 20.0, 11.0);
    }
    assert_eq!(p.warnings.len(), 1);
    assert_eq!(p.warnings[0].code, "svg_math_unsupported");
    let fallback: String = lines.iter().flatten().map(|word| word.text.as_str()).collect();
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
    let doc = Document { blocks: vec![
        Block::Paragraph(vec![Inline::Math(r"x^2+\frac{1}{y}".into())]),
        Block::MathBlock(r"\sqrt{x}".into()),
        Block::MathBlock(r"\fmdUnknownCommand{x}".into()),
    ] };
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
    assert!(p.ops.iter().any(|op| matches!(op, Op::Glyph { ink: Ink::QuoteFg, .. })));
    assert!(!p.ops.iter().any(|op| matches!(op, Op::Glyph { ink: Ink::Fg, .. })));
}
