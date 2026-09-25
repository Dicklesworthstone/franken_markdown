//! End-to-end wrapping/painting checks with real, independently specified fonts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::super::{Ink, SvgOptions, render_svg_with_resources};
use franken_markdown::{FontAssets, ast::{Block, Document, Inline}};

const FIXTURE: &[u8] = include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf");

fn poster() -> Poster {
    Poster::new(&SvgOptions::default()).with_resources(&FontAssets {
        body_regular: Some(FIXTURE.to_vec()),
        body_bold: Some(FIXTURE.to_vec()),
        mono_regular: Some(FIXTURE.to_vec()),
        ..FontAssets::default()
    }, &[]).unwrap()
}

fn words(poster: &Poster, source: &str, width: f64) -> Vec<Vec<Word>> {
    poster.wrap(&[Piece::Text(source.to_owned(), RStyle::BODY)], 10.0, width)
}

fn glyphs(poster: &Poster) -> Vec<(usize, u16, f64)> {
    poster.ops.iter().filter_map(|op| match op {
        Op::Glyph { slot, gid, x, .. } => Some((*slot, *gid, *x)),
        _ => None,
    }).collect()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn actual_custom_font_pair_positions_match_saved_harfbuzz_reference() {
    // HarfBuzz fixture: ab -> gids 2,3; advances 460,500 design units.
    let mut poster = poster();
    let lines = words(&poster, "ab", 10.0);
    assert_eq!(lines.len(), 1);
    close(lines[0][0].w, 9.6);
    close(poster.measure("ab", RStyle::BODY, 10.0), 10.0); // unshaped negative control
    poster.draw_words(&lines[0], 17.0, 30.0, 10.0);
    let ink = glyphs(&poster);
    assert_eq!(ink.len(), 2);
    assert_eq!((ink[0].0, ink[0].1, ink[1].1), (0, 2, 3));
    close(ink[0].2, 17.0);
    close(ink[1].2, 21.6);
}

#[test]
fn ligature_outlines_and_break_measure_use_the_substituted_glyph() {
    // HarfBuzz fixture: fi -> gid 6, 500 units; neither input glyph is painted.
    let mut poster = poster();
    let lines = words(&poster, "fi", 5.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0][0].text, "fi");
    close(lines[0][0].w, 5.0);
    poster.draw_words(&lines[0], 0.0, 20.0, 10.0);
    assert_eq!(glyphs(&poster), [(0, 6, 0.0)]);
}

#[test]
fn equal_style_ast_fragments_shape_together_but_real_style_changes_do_not() {
    let mut poster = poster();
    let joined = poster.wrap(&[
        Piece::Text("f".to_owned(), RStyle::BODY),
        Piece::Text("i".to_owned(), RStyle::BODY),
    ], 10.0, 5.0);
    assert_eq!(joined.len(), 1);
    close(joined[0][0].w, 5.0);
    poster.draw_words(&joined[0], 0.0, 20.0, 10.0);
    assert_eq!(glyphs(&poster), [(0, 6, 0.0)]);
    poster.ops.clear();
    let separated = poster.wrap(&[
        Piece::Text("f".to_owned(), RStyle::BODY),
        Piece::Text("i".to_owned(), RStyle { bold: true, ..RStyle::BODY }),
    ], 10.0, 10.0);
    close(poster.words_width(&separated[0], 10.0), 10.0);
    assert!(separated[0].iter().all(|word| word.gap == 0.0));
    poster.draw_words(&separated[0], 0.0, 20.0, 10.0);
    assert_eq!(glyphs(&poster), [(0, 4, 0.0), (1, 5, 5.0)]);
}

#[test]
fn pair_compression_changes_real_line_break_choices() {
    let poster = poster();
    let lines = words(&poster, "ab ab", 22.3);
    // Each ab is 9.6pt and a space is 3pt. Raw hmtx would require 23pt.
    assert_eq!(lines.len(), 1);
    close(poster.words_width(&lines[0], 10.0), 22.2);
    assert_eq!(words(&poster, "ab ab", 22.1).len(), 2);
}

#[test]
fn emergency_wrap_discards_cross_line_kerning_without_splitting_ligatures() {
    let mut poster = poster();
    let lines = words(&poster, "abfiabfi", 5.0);
    assert_eq!(lines.iter().map(|line| line.iter().map(|word| word.text.as_str()).collect::<String>()).collect::<Vec<_>>(),
        ["a", "b", "fi", "a", "b", "fi"]);
    for line in &lines {
        close(poster.words_width(line, 10.0), 5.0);
        let shape = Shaper::new(&poster).shape(&line[0].text, RStyle::BODY, 10.0);
        close(shape.width(), line[0].w);
        poster.draw_words(line, 0.0, 20.0, 10.0);
    }
    assert_eq!(glyphs(&poster).len(), 6);
}

#[test]
fn many_widths_keep_complete_source_and_the_same_emitted_measure() {
    let mut poster = poster();
    let source = "abfibbabfi".repeat(30);
    for width in [5.0, 5.1, 9.5, 9.6, 10.0, 14.9, 15.0, 24.1, 57.3] {
        let lines = words(&poster, &source, width);
        assert_eq!(lines.iter().flatten().map(|word| word.text.as_str()).collect::<String>(), source);
        for line in &lines {
            assert!(poster.words_width(line, 10.0) <= width + 1e-8);
            for word in line {
                let shaped = Shaper::new(&poster).shape(&word.text, word.style, 10.0);
                close(shaped.width(), word.w);
                let end = shaped.paint(&mut poster, 7.0, 20.0, word.style, 10.0);
                close(end - 7.0, word.w);
            }
            poster.ops.clear();
        }
    }
}

#[test]
fn mono_inline_text_and_fenced_code_retain_literal_glyphs_and_spaces() {
    let mut poster = poster();
    let style = RStyle { mono: true, ..RStyle::BODY };
    let source = "fi  ab";
    let shaped = Shaper::new(&poster).shape(source, style, 10.0);
    close(shaped.width(), poster.measure(source, style, 10.0));
    close(shaped.width(), 26.0);
    shaped.paint(&mut poster, 0.0, 20.0, style, 10.0);
    assert_eq!(glyphs(&poster).iter().map(|glyph| glyph.1).collect::<Vec<_>>(), [4, 5, 2, 3]);
    assert_eq!(poster.code_lines("f\ti", 10.0, 100.0), ["f   i"]);
}

#[test]
fn shaped_strikethrough_ends_at_the_actual_glyph_advance() {
    let mut poster = poster();
    let style = RStyle { strike: true, ink: Ink::Accent, ..RStyle::BODY };
    let lines = poster.wrap(&[Piece::Text("abfi".to_owned(), style)], 10.0, 100.0);
    poster.draw_words(&lines[0], 13.0, 20.0, 10.0);
    let (left, right, ink) = poster.ops.iter().find_map(|op| match op {
        Op::Rule { x1, x2, ink, .. } => Some((*x1, *x2, *ink)), _ => None,
    }).unwrap();
    close(left, 13.0);
    close(right, 27.6);
    assert_eq!(ink, Ink::Accent);
}

#[test]
fn actual_fallback_faces_and_missing_character_reporting_are_preserved() {
    let mut poster = poster();
    let lines = words(&poster, "fi∑fi\u{10ffff}", 100.0);
    poster.draw_words(&lines[0], 0.0, 20.0, 10.0);
    let ink = glyphs(&poster);
    assert_eq!(ink.len(), 3);
    assert_eq!((ink[0].0, ink[0].1, ink[2].0, ink[2].1), (0, 6, 0, 6));
    assert_eq!(ink[1].0, 5);
    assert_eq!(ink[1].1, poster.faces[5].as_ref().unwrap().glyph_index('∑'));
    assert_eq!(poster.missing, 1);
}

#[test]
fn zero_advance_marks_stay_with_their_base_during_emergency_wrapping() {
    let poster = poster();
    let source = "a\u{0301}b";
    let lines = words(&poster, source, 5.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0][0].text, "a\u{0301}");
    assert_eq!(lines[1][0].text, "b");
    assert_eq!(lines.iter().flatten().map(|word| word.text.as_str()).collect::<String>(), source);
}

#[test]
fn supplied_font_shaping_reaches_the_public_svg_exporter_and_deduplicated_defs() {
    let doc = Document { blocks: vec![Block::Paragraph(vec![Inline::Text("fi fi".to_owned())])] };
    let assets = FontAssets { body_regular: Some(FIXTURE.to_vec()), ..FontAssets::default() };
    let first = render_svg_with_resources(&doc, &SvgOptions::default(), &assets, &[]).unwrap();
    let second = render_svg_with_resources(&doc, &SvgOptions::default(), &assets, &[]).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.1.glyphs_drawn, 2);
    assert_eq!(first.1.paths_emitted, 1);
    assert_eq!(first.1.glyphs_missing, 0);
    assert!(first.2.is_empty());
    let xml = String::from_utf8(first.0).unwrap();
    assert_eq!(xml.matches("<use href=\"#g").count(), 2);
    assert!(!xml.contains("<text"));
}
