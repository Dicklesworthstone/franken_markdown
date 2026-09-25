//! Real font shaping, retained-fragment painting, and enclosing line geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use super::super::{Ink, SvgOptions, render_svg_with_resources};
use franken_markdown::{FontAssets, ast::{Align, Block, Document, Inline, Table}};

const FIXTURE: &[u8] = include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf");

fn resources() -> FontAssets {
    FontAssets {
        body_regular: Some(FIXTURE.to_vec()), body_bold: Some(FIXTURE.to_vec()),
        body_italic: Some(FIXTURE.to_vec()), body_bold_italic: Some(FIXTURE.to_vec()),
        mono_regular: Some(FIXTURE.to_vec()), ..FontAssets::default()
    }
}

fn poster() -> Poster {
    Poster::new(&SvgOptions::default()).with_resources(&resources(), &[]).unwrap()
}

fn lines(poster: &Poster, source: &str, width: f64) -> Vec<Vec<Word>> {
    poster.wrap(&[Piece::Text(source.to_owned(), RStyle::BODY)], 10.0, width)
}

fn glyphs(poster: &Poster) -> Vec<(usize, u16, f64, f64)> {
    poster.ops.iter().filter_map(|op| match op {
        Op::Glyph { slot, gid, x, y, .. } => Some((*slot, *gid, *x, *y)), _ => None,
    }).collect()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-7, "{a} != {b}");
}

/// The fixture's actual font header bounds, at the emitted glyph positions.
fn ink_bounds(poster: &Poster) -> Vec<(f64, f64)> {
    poster.ops.iter().filter_map(|op| {
        if let Op::Glyph { slot, gid, y, size, .. } = op {
            let font = poster.font_for_slot(*slot).unwrap();
            let bbox = font.glyph_bbox(*gid).unwrap();
            let k = size / f64::from(font.units_per_em);
            Some((y - f64::from(bbox[3]) * k, y - f64::from(bbox[1]) * k))
        } else { None }
    }).collect()
}

#[test]
fn saved_harfbuzz_mark_offsets_survive_into_svg_glyph_positions() {
    let mut p = poster();
    let rows = lines(&p, "a\u{0301}b", 100.0);
    let word = &rows[0][0];
    assert_eq!(word.text, "a\u{0301}b");
    close(word.w, 10.0);
    let prepared = word.shaped.as_ref().unwrap();
    assert!(prepared.contextual);
    assert_eq!(prepared.clusters.len(), 2);
    assert_eq!(prepared.clusters[0].bytes, 0..3);
    // Saved HarfBuzz reference: a=500, mark=0 at (-350,+500), b=500.
    p.draw_words(&rows[0], 17.0, 30.0, 10.0);
    let actual = glyphs(&p);
    assert_eq!(actual.len(), 3);
    assert_eq!(actual.iter().map(|g| (g.0, g.1)).collect::<Vec<_>>(), [(0, 2), (0, 7), (0, 3)]);
    close(actual[0].2, 17.0); close(actual[0].3, 30.0);
    close(actual[1].2, 18.5); close(actual[1].3, 25.0);
    close(actual[2].2, 22.0); close(actual[2].3, 30.0);
    assert!(p.warnings.is_empty());
    assert_eq!(p.missing, 0);
}

#[test]
fn stacked_marks_increase_line_ascent_without_inventing_advance() {
    let mut p = poster();
    let rows = lines(&p, "a\u{0301}\u{0307}", 5.0);
    assert_eq!(rows.len(), 1);
    let prepared = rows[0][0].shaped.as_ref().unwrap();
    assert_eq!(prepared.clusters.len(), 1);
    assert_eq!(prepared.clusters[0].bytes, 0..5);
    close(prepared.width(), 5.0);
    // Original fixture bbox top=400; second mark offset=650: 10.5pt ink.
    close(prepared.ink_ascent(), 10.5);
    close(prepared.ink_descent(), 0.0);
    let (ascent, height) = p.line_metrics(&rows[0], 8.5, 14.0);
    close(ascent, 10.5); close(height, 16.0);
    p.draw_words(&rows[0], 0.0, ascent, 10.0);
    let ink = glyphs(&p);
    close(ink[1].2, 1.5); close(ink[1].3, ascent - 5.0);
    close(ink[2].2, 1.5); close(ink[2].3, ascent - 6.5);
    assert!(ink_bounds(&p).iter().all(|&(top, bottom)| top >= -1e-7 && bottom <= height));
}

#[test]
fn following_lines_begin_after_all_stacked_mark_ink() {
    let mut p = poster();
    let top = p.y;
    let pieces = [Piece::Text("a\u{0301}\u{0307}".into(), RStyle::BODY), Piece::Break,
        Piece::Text("a\u{0301}\u{0307}".into(), RStyle::BODY)];
    p.text_lines(&pieces, 10.0, 0.0, 100.0, false, 0.0);
    let bounds = ink_bounds(&p);
    assert_eq!(bounds.len(), 6);
    let first_bottom = bounds[..3].iter().map(|v| v.1).fold(top, f64::max);
    let second_top = bounds[3..].iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
    assert!(first_bottom <= second_top);
    assert!(bounds.iter().all(|&(a, b)| a >= top - 1e-7 && b <= p.y + 1e-7));
}

#[test]
fn contextual_line_cuts_keep_original_clusters_and_actual_fragment_measure() {
    let mut p = poster();
    let source = "aba\u{0301}\u{0307}fiba\u{0301}".repeat(12);
    for width in [4.0, 5.0, 9.8, 10.0, 14.7, 23.0, 49.0] {
        let rows = lines(&p, &source, width);
        assert_eq!(rows.iter().flatten().map(|w| w.text.as_str()).collect::<String>(), source);
        for row in rows {
            assert_eq!(row.len(), 1);
            let word = &row[0];
            assert!(!word.text.chars().next().is_some_and(|c| matches!(c as u32, 0x0300..=0x036f)));
            let prepared = word.shaped.as_ref().unwrap();
            close(word.w, prepared.width());
            let isolated = Shaper::new(&p).shape(&word.text, word.style, 10.0);
            close(word.w, isolated.width());
            assert!(word.w <= width + 1e-7 || prepared.clusters.len() == 1);
            let left = 13.0;
            p.ops.clear();
            let end = prepared.paint(&mut p, left, 30.0, word.style, 10.0);
            close(end - left, word.w);
            let expected = glyphs(&p);
            p.ops.clear();
            p.draw_words(&row, left, 30.0, 10.0);
            assert_eq!(glyphs(&p), expected);
        }
    }
    assert!(p.warnings.is_empty());
}

#[test]
fn unmarked_word_slices_still_remove_the_outgoing_kerning_adjustment() {
    let mut p = poster();
    for width in [5.0, 9.7, 10.0, 17.0] {
        let rows = lines(&p, "abfiabbafi", width);
        for row in &rows {
            let word = &row[0];
            let prepared = word.shaped.as_ref().unwrap();
            assert!(!prepared.contextual);
            close(word.w, Shaper::new(&p).shape(&word.text, word.style, 10.0).width());
            close(prepared.paint(&mut p, 0.0, 20.0, word.style, 10.0), word.w);
        }
    }
}

#[test]
fn code_preserves_spacing_and_tabs_but_positions_marks_without_ligatures() {
    let mut p = poster();
    let rows = p.code_words("fi a\u{0301}\u{0307}\tfi\n\n  a\u{0301}", 10.0, 200.0);
    assert_eq!(rows.len(), 3);
    // Source scalar columns: fi(2), space(1), base+two marks(3), tab adds 2.
    assert_eq!(rows[0].text, "fi a\u{0301}\u{0307}  fi");
    assert!(rows[1].text.is_empty());
    assert_eq!(rows[2].text, "  a\u{0301}");
    p.draw_words(std::slice::from_ref(&rows[0]), 0.0, 30.0, 10.0);
    let ink = glyphs(&p);
    assert!(!ink.iter().any(|g| g.1 == 6)); // no fi ligature in code
    assert!(ink.iter().all(|g| g.0 == 4));
    assert!(ink.iter().any(|g| g.1 == 8 && (g.3 - 23.5).abs() < 1e-7));
    assert!(p.warnings.is_empty());
}

#[test]
fn code_panel_background_contains_positioned_ink_on_every_wrapped_row() {
    let mut p = poster();
    p.code_panel(&"aba\u{0301}\u{0307}fi".repeat(20), 0.0, 60.0);
    let (top, bottom) = p.ops.iter().find_map(|op| match op {
        Op::Rect { y, h, fill: Ink::CodeBg, .. } => Some((*y, y + h)), _ => None,
    }).unwrap();
    let ink = ink_bounds(&p);
    assert!(ink.len() > 50);
    assert!(ink.iter().all(|&(a, b)| a >= top && b <= bottom));
    assert!(p.y > bottom);
    assert!(p.warnings.is_empty());
}

#[test]
fn table_row_sizing_contains_marks_in_headers_and_body_cells() {
    let mut p = poster();
    let top = p.y;
    let cell = || vec![Inline::Text("a\u{0301}\u{0307}".into())];
    p.table(&Table { align: vec![Align::Left], head: vec![cell()], rows: vec![vec![cell()]] }, 0.0, 200.0);
    let header_bottom = p.ops.iter().find_map(|op| match op {
        Op::Rect { y, h, fill: Ink::BgSubtle, .. } => Some(y + h), _ => None,
    }).unwrap();
    let table_bottom = p.ops.iter().filter_map(|op| match op {
        Op::Rule { y1, y2, .. } if y1 == y2 => Some(*y1), _ => None,
    }).fold(top, f64::max);
    let ink = ink_bounds(&p);
    assert_eq!(ink.len(), 6);
    assert!(ink[..3].iter().all(|&(a, b)| a >= top && b <= header_bottom));
    assert!(ink[3..].iter().all(|&(a, b)| a >= header_bottom && b <= table_bottom));
    assert!(p.warnings.is_empty());
}

#[test]
fn heading_rule_stays_below_descenders_and_the_following_paragraph() {
    // Use real bundled descenders, whose bottom extends below the baseline.
    let mut p = Poster::new(&SvgOptions::default());
    p.heading(1, &[Inline::Text("gy".into())], 0.0, 300.0, false);
    let deepest = ink_bounds(&p).iter().map(|v| v.1).fold(0.0, f64::max);
    let rule = p.ops.iter().find_map(|op| match op {
        Op::Rule { y1, w, ink: Ink::BorderMuted, .. } => Some((*y1, *w)), _ => None,
    }).unwrap();
    assert!(rule.0 - rule.1 / 2.0 > deepest);
    let next_top = p.y;
    p.ops.clear();
    p.paragraph(&[Inline::Text("next".into())], 0.0, 300.0, false);
    assert!(ink_bounds(&p).iter().all(|&(top, _)| top >= next_top && top > rule.0));
}

#[test]
fn unsupported_and_budget_limited_marks_keep_source_and_report_fallback() {
    for source in ["\u{0301}a".to_owned(), "a\u{036f}".to_owned(),
        format!("a{}", "\u{0301}".repeat(65)), format!("{}\u{0301}", "a".repeat(4096))]
    {
        let mut p = poster();
        let rows = lines(&p, &source, 80.0);
        assert_eq!(rows.iter().flatten().map(|w| w.text.as_str()).collect::<String>(), source);
        for row in &rows { p.draw_words(row, 0.0, 30.0, 10.0); }
        assert!(p.warnings.iter().any(|w| w.code == "svg_text_shaping_unsupported" || w.code == "svg_text_shaping_limit"));
    }
}

#[test]
fn repeated_positioned_glyphs_deduplicate_outlines_in_public_resource_export() {
    let doc = Document { blocks: vec![Block::Paragraph(vec![
        Inline::Text("a\u{0301}\u{0307} a\u{0301}\u{0307}".into())]) ] };
    let fonts = resources();
    let first = render_svg_with_resources(&doc, &SvgOptions::default(), &fonts, &[]).unwrap();
    let second = render_svg_with_resources(&doc, &SvgOptions::default(), &fonts, &[]).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.1.glyphs_drawn, 6);
    assert_eq!(first.1.paths_emitted, 3);
    assert_eq!(first.1.glyphs_missing, 0);
    assert!(first.2.is_empty());
    let xml = String::from_utf8(first.0).unwrap();
    assert!(!xml.contains("<text"));
    assert_eq!(xml.matches("<use href=\"#g").count(), 6);
}

#[test]
fn mixed_fallback_symbols_keep_their_face_while_latin_marks_stay_with_the_base() {
    let mut p = poster();
    let rows = lines(&p, "∑a\u{0301}\u{0307}∑", 100.0);
    p.draw_words(&rows[0], 0.0, 30.0, 10.0);
    let ink = glyphs(&p);
    assert_eq!(ink.len(), 5);
    assert_eq!(ink[0].0, 5);
    assert_eq!(ink[4].0, 5);
    assert!(ink[1..4].iter().all(|g| g.0 == 0));
    close(ink[2].3, 25.0);
    close(ink[3].3, 23.5);
    assert!(p.warnings.is_empty());
}
