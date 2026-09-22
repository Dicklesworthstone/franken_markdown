//! Behavior checks against the actual painter, not a second layout algorithm.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn poster() -> Poster {
    Poster::new(&SvgOptions::default())
}

fn pieces(poster: &Poster, nodes: &[Inline]) -> Vec<Piece> {
    let mut result = Vec::new();
    poster.flatten(nodes, RStyle::BODY, &mut result);
    result
}

fn text(lines: &[Vec<Word>]) -> String {
    lines.iter().flatten().map(|run| run.text.as_str()).collect()
}

#[test]
fn styled_word_has_no_invented_space_or_break() {
    let p = poster();
    let nodes = vec![Inline::Text("pre".into()), Inline::Strong(vec![
        Inline::Text("fix".into())]), Inline::Text(",suffix".into())];
    let input = pieces(&p, &nodes);
    let lines = p.wrap(&input, 11.0, 400.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(text(&lines), "prefix,suffix");
    assert!(lines[0].iter().all(|word| word.gap == 0.0));
    let measure = p.words_width(&lines[0], 11.0);
    let mut with_prefix = vec![Piece::Text("abc ".into(), RStyle::BODY)];
    with_prefix.extend(input);
    let wrapped = p.wrap(&with_prefix, 11.0, measure + 0.1);
    assert_eq!(wrapped.len(), 2);
    assert_eq!(text(&wrapped[1..]), "prefix,suffix");
}

#[test]
fn whitespace_only_runs_keep_one_real_gap() {
    let p = poster();
    let nodes = vec![Inline::Strong(vec![Inline::Text("one".into())]),
        Inline::Text("  \t".into()), Inline::Emphasis(vec![Inline::Text("two".into())])];
    let lines = p.wrap(&pieces(&p, &nodes), 11.0, 400.0);
    assert_eq!(lines[0].len(), 2);
    assert_eq!(lines[0][1].gap, p.space_width(RStyle::BODY, 11.0));
}

#[test]
fn consecutive_and_trailing_hard_breaks_keep_empty_lines() {
    let p = poster();
    let input = vec![Piece::Text("a".into(), RStyle::BODY), Piece::Break,
        Piece::Break, Piece::Text("b".into(), RStyle::BODY), Piece::Break];
    let lines = p.wrap(&input, 11.0, 400.0);
    assert_eq!(lines.len(), 4);
    assert!(lines[1].is_empty());
    assert!(lines[3].is_empty());
}

#[test]
fn code_span_spaces_are_preserved() {
    let p = poster();
    let nodes = vec![Inline::Text("(".into()), Inline::Code("a  b".into()),
        Inline::Text(")".into())];
    let lines = p.wrap(&pieces(&p, &nodes), 11.0, 400.0);
    assert_eq!(text(&lines), "(a  b)");
    assert!(lines[0].iter().all(|word| word.gap == 0.0));
}

#[test]
fn nbsp_remains_inside_the_word() {
    let p = poster();
    let source = "a\u{00a0}b\u{202f}c";
    let lines = p.wrap(&[Piece::Text(source.into(), RStyle::BODY)], 11.0, 400.0);
    assert_eq!(text(&lines), source);
    assert_eq!(lines[0].len(), 1);
}

#[test]
fn long_mixed_style_token_wraps_without_losing_utf8() {
    let p = poster();
    let source = "éΩabc".repeat(100);
    let width = 36.0;
    let input = vec![Piece::Text(source.clone(), RStyle::BODY)];
    let lines = p.wrap(&input, 11.0, width);
    assert_eq!(text(&lines), source);
    assert!(lines.len() > 10);
    for line in &lines {
        assert!(p.words_width(line, 11.0) <= width + 0.00001);
    }
}

#[test]
fn painted_width_matches_layout_with_mixed_styles_and_spaces() {
    let mut p = poster();
    let input = pieces(&p, &[Inline::Text("a".into()),
        Inline::Strong(vec![Inline::Text("b".into())]),
        Inline::Text(" c".into()), Inline::Code("d  e".into())]);
    let lines = p.wrap(&input, 11.0, 400.0);
    let expected = p.words_width(&lines[0], 11.0);
    p.draw_words(&lines[0], 0.0, 20.0, 11.0);
    let final_run = lines[0].last().unwrap();
    let last_ch = final_run.text.chars().last().unwrap();
    let last_advance = p.advance(p.resolve(last_ch, final_run.style).0, last_ch, 11.0);
    let last_x = p.ops.iter().filter_map(|op| match op {
        Op::Glyph { x, .. } => Some(*x), _ => None,
    }).last().unwrap();
    assert!((last_x + last_advance - expected).abs() < 0.00001);
}

#[test]
fn code_wrap_keeps_indentation_spaces_and_tab_stops() {
    let p = poster();
    let lines = p.code_lines("a\tb\n\n    hello  world", 10.0, 36.0);
    assert_eq!(lines.concat(), "a   b    hello  world");
    assert!(lines.iter().any(String::is_empty));
    let style = RStyle { mono: true, ..RStyle::BODY };
    assert!(lines.iter().all(|line| p.measure(line, style, 10.0) <= 36.00001));
}

#[test]
fn long_code_panel_reserves_height_for_every_wrapped_line() {
    let mut p = poster();
    let source = "abcdefghij".repeat(80);
    let top = p.y;
    let size = f64::from(p.scale.code);
    let rows = p.code_lines(&source, size, 60.0 - 24.0).len();
    p.code_panel(&source, 0.0, 60.0);
    assert!(rows > 20);
    assert!(p.y - top >= rows as f64 * size * 1.45);
    let drawn = p.ops.iter().filter(|op| matches!(op, Op::Glyph { .. })).count();
    assert_eq!(drawn, source.len());
}

#[test]
fn raw_html_is_inert_visible_text_not_silently_dropped() {
    let p = poster();
    let source = "<b>visible</b>";
    let lines = p.wrap(&pieces(&p, &[Inline::Html(source.into())]), 11.0, 400.0);
    assert_eq!(text(&lines), source);
    let doc = Document { blocks: vec![Block::HtmlBlock(source.into())] };
    let (bytes, report) = render_svg_with_report(&doc, &SvgOptions::default());
    assert_eq!(report.glyphs_drawn, source.len());
    assert!(!String::from_utf8(bytes).unwrap().contains("<b>"));
}
