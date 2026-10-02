//! Exercise the production tokenizer, shared planner and real SVG painter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{
    Align, Block, Document, Inline, SvgOptions, Table, render_svg_with_diagnostics,
};
use super::*;

fn poster() -> Poster {
    Poster::new(&SvgOptions::default())
}

fn source(lines: &[Vec<Word>]) -> String {
    lines
        .iter()
        .flatten()
        .map(|word| word.text.as_str())
        .collect()
}

fn line_text(line: &[Word]) -> String {
    let mut text = String::new();
    for word in line {
        if word.gap > 0.0 {
            text.push(' ');
        }
        text.push_str(&word.text);
    }
    text
}

#[test]
fn actual_shaped_prose_rebalances_a_greedy_short_middle_line() {
    let p = poster();
    let pieces = [Piece::Text("xxx xx xx xxxxx".into(), RStyle::BODY)];
    let wide = p.wrap(&pieces, 11.0, 1000.0);
    let words = &wide[0];
    assert_eq!(words.len(), 4);
    let measure = words[0].w + words[1].gap + words[1].w + 0.01;
    assert!(words[2].w + words[3].gap + words[3].w > measure);
    let lines = p.wrap(&pieces, 11.0, measure);
    assert_eq!(
        lines.iter().map(|line| line_text(line)).collect::<Vec<_>>(),
        ["xxx", "xx xx", "xxxxx"]
    );
    assert!(
        lines
            .iter()
            .all(|line| p.words_width(line, 11.0) <= measure)
    );
    assert_eq!(source(&lines), "xxxxxxxxxxxx");
}

#[test]
fn retained_prepared_runs_have_the_exact_advances_the_painter_spends() {
    let mut p = poster();
    let pieces = [
        Piece::Text("AV ffi e\u{301} ".into(), RStyle::BODY),
        Piece::Text(
            "bold ".into(),
            RStyle {
                bold: true,
                ..RStyle::BODY
            },
        ),
        Piece::Text(
            "italic AV ffi tail".into(),
            RStyle {
                italic: true,
                ..RStyle::BODY
            },
        ),
    ];
    let lines = p.wrap(&pieces, 11.0, 60.0);
    assert!(lines.len() > 1);
    for line in lines {
        let expected = p.words_width(&line, 11.0);
        let mut pen = 0.0;
        for word in &line {
            pen += word.gap;
            let prepared = word
                .shaped
                .as_ref()
                .expect("ordinary runs retain their shape");
            pen = prepared.paint(&mut p, pen, 20.0, word.style, 11.0);
        }
        assert!((pen - expected).abs() < 0.000_001);
        assert!(pen <= 60.000_001);
    }
}

#[test]
fn contiguous_styles_and_nonbreaking_spaces_remain_unbreakable() {
    let p = poster();
    let pieces = [
        Piece::Text("before pre".into(), RStyle::BODY),
        Piece::Text(
            "fix".into(),
            RStyle {
                bold: true,
                ..RStyle::BODY
            },
        ),
        Piece::Text(",end after a\u{00a0}b\u{202f}c".into(), RStyle::BODY),
    ];
    let wide = p.wrap(&pieces, 11.0, 1000.0);
    let contiguous: f64 = wide[0][1..4].iter().map(|run| run.w).sum();
    let lines = p.wrap(&pieces, 11.0, contiguous + 0.01);
    assert_eq!(source(&lines), "beforeprefix,endaftera\u{00a0}b\u{202f}c");
    let styled = lines
        .iter()
        .find(|line| line.iter().any(|run| run.text == "fix"))
        .unwrap();
    assert_eq!(line_text(styled), "prefix,end");
    assert!(styled.iter().all(|run| run.gap == 0.0));
    assert!(
        lines
            .iter()
            .flatten()
            .any(|run| run.text == "a\u{00a0}b\u{202f}c")
    );
}

#[test]
fn forced_breaks_isolate_optimization_and_keep_blank_and_final_lines() {
    let p = poster();
    let source = "xxx xx xx xxxxx";
    let input = [
        Piece::Break,
        Piece::Text(source.into(), RStyle::BODY),
        Piece::Break,
        Piece::Break,
        Piece::Text(source.into(), RStyle::BODY),
        Piece::Break,
    ];
    let expected = p.wrap(&[Piece::Text(source.into(), RStyle::BODY)], 11.0, 32.0);
    let lines = p.wrap(&input, 11.0, 32.0);
    let n = expected.len();
    assert_eq!(lines.len(), n * 2 + 3);
    assert!(lines[0].is_empty() && lines[n + 1].is_empty() && lines.last().unwrap().is_empty());
    for (left, right) in lines[1..=n].iter().zip(expected.iter()) {
        assert_eq!(line_text(left), line_text(right));
    }
    for (left, right) in lines[n + 2..n * 2 + 2].iter().zip(expected.iter()) {
        assert_eq!(line_text(left), line_text(right));
    }
}

#[test]
fn code_whitespace_and_overwide_combining_words_keep_the_existing_emergency_path() {
    let p = poster();
    let token = "e\u{301}".repeat(80);
    let input = [
        Piece::Text("before ".into(), RStyle::BODY),
        Piece::Text(token.clone(), RStyle::BODY),
        Piece::Text(" after ".into(), RStyle::BODY),
        Piece::Text(
            "a  b\tc".into(),
            RStyle {
                mono: true,
                ..RStyle::BODY
            },
        ),
    ];
    let lines = p.wrap(&input, 11.0, 36.0);
    assert_eq!(source(&lines), format!("before{token}aftera  b\tc"));
    for line in &lines {
        assert!(p.words_width(line, 11.0) <= 36.000_001);
        assert!(
            !line
                .first()
                .is_some_and(|run| run.text.starts_with('\u{301}'))
        );
    }
    let code = p.code_lines("a\tb\n\n  c  d", 10.0, 36.0);
    assert_eq!(code.concat(), "a   b  c  d");
    assert!(code.iter().any(String::is_empty));
}

#[test]
fn formulas_images_and_their_diagnostics_survive_word_planning() {
    let mut p = poster();
    let input = [
        Piece::Text("prefix ".into(), RStyle::BODY),
        Piece::Math("x^2 + y^2".into(), false, RStyle::BODY),
        Piece::Text(" suffix ".into(), RStyle::BODY),
        Piece::Image("missing.png".into(), "diagram".into(), RStyle::BODY),
    ];
    let lines = p.wrap(&input, 11.0, 100.0);
    assert_eq!(
        lines
            .iter()
            .flatten()
            .filter(|run| run.formula.is_some())
            .count(),
        1
    );
    assert!(lines.iter().flatten().any(|run| run.warning.is_some()));
    let warnings: Vec<_> = lines
        .iter()
        .flatten()
        .filter_map(|run| run.warning.clone())
        .collect();
    for (index, line) in lines.iter().enumerate() {
        p.draw_words(line, 0.0, 30.0 * (index + 1) as f64, 11.0);
    }
    for warning in warnings {
        assert_eq!(
            p.warnings.iter().filter(|found| **found == warning).count(),
            1
        );
    }
    assert!(p.ops.iter().any(|op| matches!(op, Op::Glyph { .. })));
}

#[test]
fn word_budget_falls_back_once_without_losing_source_and_resets_at_hard_break() {
    let mut p = poster();
    let first = "x ".repeat(2050);
    let input = [
        Piece::Text(first, RStyle::BODY),
        Piece::Break,
        Piece::Text("tail".into(), RStyle::BODY),
    ];
    let lines = p.wrap(&input, 11.0, 100.0);
    assert_eq!(source(&lines), format!("{}tail", "x".repeat(2050)));
    assert_eq!(lines.last().unwrap()[0].text, "tail");
    assert!(
        lines
            .iter()
            .all(|line| p.words_width(line, 11.0) <= 100.000_001)
    );
    for (index, line) in lines.iter().enumerate() {
        p.draw_words(line, 0.0, 20.0 * index as f64, 11.0);
    }
    assert_eq!(
        p.warnings
            .iter()
            .filter(|warning| warning.code == "svg_paragraph_limit")
            .count(),
        1
    );
}

#[test]
fn full_renderer_uses_planned_lines_inside_nested_blocks_and_tables_deterministically() {
    let document = Document {
        blocks: vec![
            Block::Heading {
                level: 2,
                inlines: vec![Inline::Text("A planned heading".into())],
            },
            Block::BlockQuote(vec![Block::Paragraph(vec![Inline::Text(
                "xxx xx xx xxxxx ".repeat(8),
            )])]),
            Block::Paragraph(vec![Inline::Text("AV ffi e\u{301} ".repeat(12))]),
            Block::Table(Table {
                align: vec![Align::Left, Align::Right],
                head: vec![
                    vec![Inline::Text("Column one".into())],
                    vec![Inline::Text("Column two".into())],
                ],
                rows: vec![vec![
                    vec![Inline::Text("xxx xx xx xxxxx ".repeat(3))],
                    vec![Inline::Text("AV ffi ".repeat(4))],
                ]],
            }),
        ],
    };
    let options = SvgOptions {
        max_width_pt: 220.0,
        ..SvgOptions::default()
    };
    let first = render_svg_with_diagnostics(&document, &options);
    let second = render_svg_with_diagnostics(&document, &options);
    assert_eq!(first, second);
    let svg = String::from_utf8(first.0).unwrap();
    assert!(svg.contains("<use ") && !svg.contains("<text") && !svg.contains("foreignObject"));
    assert!(first.1.glyphs_drawn > 100);
    assert!(
        !first
            .2
            .iter()
            .any(|warning| warning.code == "svg_paragraph_limit")
    );
}
