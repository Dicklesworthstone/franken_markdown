//! Geometry and typography assertions against the actual SVG painter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use super::*;
use franken_markdown::ast::{List, ListItem};
use franken_markdown::{FontAssetSlot, FontAssets, FontScale, PdfImageAsset, SystemAppearance, TypeScalePreset};

fn options(factor: f32) -> SvgOptions {
    SvgOptions { theme: Theme::default().with_font_scale(FontScale::from_factor(factor)),
        ..SvgOptions::default() }
}

fn paragraph(text: &str) -> Block { Block::Paragraph(vec![Inline::Text(text.to_owned())]) }
fn list(start: u64, blocks: Vec<Vec<Block>>) -> List {
    List { ordered: true, start, tight: true,
        items: blocks.into_iter().map(|blocks| ListItem { task: None, blocks }).collect() }
}
fn glyphs(p: &Poster) -> Vec<(u16, f64, f64, f64)> {
    p.ops.iter().filter_map(|op| match op {
        Op::Glyph { gid, x, y, size, .. } => Some((*gid, *x, *y, *size)),
        _ => None,
    }).collect()
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.00001, "{actual} != {expected}");
}

#[test]
fn default_prose_retains_the_legacy_ladder_baselines_and_page() {
    let p = Poster::new(&SvgOptions::default());
    assert_eq!(p.scale, TypeScale::default());
    assert_eq!((p.width, p.content_left(), p.content_right(), p.y, p.margin_bottom),
        (612.0, 72.0, 540.0, 72.0, 72.0));
    assert!(p.warnings.is_empty());
    let mut p = p;
    p.paragraph(&[Inline::Text("AA".into())], 72.0, 540.0, false);
    let actual = glyphs(&p);
    assert_eq!(actual.len(), 2);
    close(actual[0].1, 72.0);
    close(actual[0].2, 72.0 + 11.0 * 0.85);
    close(actual[0].3, 11.0);
    close(actual[1].1 - actual[0].1, p.measure("A", RStyle::BODY, 11.0));
}

#[test]
fn every_named_scale_uses_the_shared_point_ladder() {
    for preset in TypeScalePreset::ALL {
        let scale = FontScale::Preset(preset);
        let p = Poster::new(&SvgOptions {
            theme: Theme::default().with_font_scale(scale), ..Default::default()
        });
        assert_eq!(p.scale, TypeScale::resolve(Some(scale.pdf_base_pt()), None, None));
        assert!(p.warnings.is_empty(), "{preset:?}");
    }
    for (base_px, body) in [(0, 11.0), (1, 6.0), (8, 6.0), (48, 24.0), (u16::MAX, 24.0)] {
        let mut opts = SvgOptions::default();
        opts.theme.spacing.base_px = base_px;
        let p = Poster::new(&opts);
        assert_eq!(p.scale.body, body);
        assert!(p.warnings.iter().all(|warning| warning.code == "svg_layout_adjusted"));
        assert!(!p.warnings.is_empty());
    }
}

#[test]
fn body_headings_code_and_table_paint_the_resolved_sizes() {
    let opts = options(1.5);
    let mut p = Poster::new(&opts);
    let sizes = p.scale;
    p.paragraph(&[Inline::Text("A".into())], 0.0, 500.0, false);
    close(glyphs(&p)[0].3, f64::from(sizes.body));
    for level in 1..=6 {
        p.ops.clear();
        p.heading(level, &[Inline::Text("A".into())], 0.0, 500.0, false);
        close(glyphs(&p)[0].3, f64::from(sizes.h[usize::from(level - 1)]));
    }
    p.ops.clear();
    p.code_panel("A", 0.0, 500.0);
    close(glyphs(&p)[0].3, f64::from(sizes.code));
    p.ops.clear();
    p.table(&Table { align: vec![Align::Left], head: vec![vec![Inline::Text("A".into())]],
        rows: vec![vec![vec![Inline::Text("B".into())]]] }, 0.0, 500.0);
    assert!(glyphs(&p).iter().all(|glyph| (glyph.3 - f64::from(sizes.table)).abs() < 0.00001));
}

#[test]
fn larger_text_rewraps_at_the_same_physical_width_without_losing_glyphs() {
    let source = "readable prose ".repeat(30);
    let mut normal = Poster::new(&options(1.0));
    let mut large = Poster::new(&options(2.0));
    for p in [&mut normal, &mut large] {
        p.paragraph(&[Inline::Text(source.clone())], 0.0, 180.0, false);
    }
    let a = glyphs(&normal);
    let b = glyphs(&large);
    let lines = |glyphs: &[(u16, f64, f64, f64)]| glyphs.iter()
        .map(|glyph| glyph.2.to_bits()).collect::<BTreeSet<_>>().len();
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), source.chars().filter(|c| !c.is_whitespace()).count());
    assert!(lines(&b) > lines(&a));
    assert!(large.y > normal.y);
    assert_eq!(normal.width, large.width);
}

#[test]
fn all_four_asymmetric_margins_affect_actual_placement_and_height() {
    let mut opts = SvgOptions::default();
    opts.theme.page.margins = franken_markdown::PageMargins {
        top_pt: 20.0, right_pt: 90.0, bottom_pt: 30.0, left_pt: 40.0,
    };
    let mut p = Poster::new(&opts);
    let l = p.content_left();
    let r = p.content_right();
    assert_eq!((l, r), (40.0, 522.0));
    p.paragraph(&[Inline::Text("A".into())], l, r, false);
    close(glyphs(&p)[0].1, 40.0);
    close(glyphs(&p)[0].2, 20.0 + 11.0 * 0.85);
    let height = p.y + 30.0;
    let (bytes, _, warnings) = p.emit(height);
    assert!(warnings.is_empty());
    assert!(String::from_utf8(bytes).unwrap().contains(&format!("height=\"{}pt\"", q2(height))));
}

#[test]
fn narrow_posters_reduce_margins_proportionally_not_to_a_fixed_gutter() {
    let mut opts = SvgOptions { max_width_pt: 144.0, ..Default::default() };
    opts.theme.page.margins.left_pt = 100.0;
    opts.theme.page.margins.right_pt = 200.0;
    let p = Poster::new(&opts);
    close(p.margin_left, 24.0);
    close(p.margin_right, 48.0);
    close(p.content_right() - p.content_left(), 72.0);
    assert_eq!(p.warnings.len(), 1);
    assert_eq!(p.warnings[0].code, "svg_layout_adjusted");
}

#[test]
fn invalid_theme_numbers_have_finite_deterministic_fallbacks() {
    let mut opts = options(1.0);
    opts.max_width_pt = f32::INFINITY;
    opts.theme.spacing.line_height = f32::NAN;
    opts.theme.spacing.table_cell_padding_x_em = -1.0;
    opts.theme.spacing.table_cell_padding_y_em = f32::INFINITY;
    opts.theme.page.margins.top_pt = -1.0;
    opts.theme.page.margins.bottom_pt = f32::NAN;
    opts.theme.page.margins.left_pt = f32::NEG_INFINITY;
    opts.theme.page.margins.right_pt = f32::INFINITY;
    let p = Poster::new(&opts);
    for value in [p.width, p.y, p.margin_bottom, p.margin_left, p.margin_right,
        p.line_height, p.table_pad_x, p.table_pad_y] { assert!(value.is_finite() && value > 0.0); }
    assert_eq!(p.warnings.len(), 8);
    let doc = Document { blocks: vec![paragraph("A")] };
    let first = render_svg_with_diagnostics(&doc, &opts);
    assert_eq!(first, render_svg_with_diagnostics(&doc, &opts));
    assert!(first.2.iter().all(|warning| warning.code == "svg_layout_adjusted"));
    let xml = String::from_utf8(first.0).unwrap();
    assert!(!xml.contains("NaN") && !xml.contains("inf"));
}

#[test]
fn table_padding_is_measured_in_actual_table_ems() {
    let mut opts = options(1.5);
    opts.theme.spacing.table_cell_padding_x_em = 0.5;
    opts.theme.spacing.table_cell_padding_y_em = 1.25;
    let mut p = Poster::new(&opts);
    let size = f64::from(p.scale.table);
    close(p.table_pad_x, size * 0.5);
    close(p.table_pad_y, size * 1.25);
    p.table(&Table { align: vec![Align::Left], head: vec![vec![Inline::Text("Header".into())]],
        rows: vec![vec![vec![Inline::Text("Considerably wider body".into())]]] }, 40.0, 540.0);
    let stripe = p.ops.iter().find_map(|op| match op {
        Op::Rect { h, fill: Ink::BgSubtle, .. } => Some(*h), _ => None,
    }).unwrap();
    close(stripe, size * 1.35 + 2.0 * size * 1.25);
    close(glyphs(&p)[0].1, 40.0 + size * 0.5);
}

#[test]
fn numbered_lists_measure_a_shared_gutter_using_the_actual_font() {
    let custom = FontAssets::default().with_slot(FontAssetSlot::BodyRegular,
        franken_markdown::fonts::body_bytes(franken_markdown::FontFamily::Serif, FontStyle::Regular).to_vec()).unwrap();
    for fonts in [FontAssets::default(), custom] {
        let mut p = Poster::new(&options(1.5)).with_resources(&fonts, &[]).unwrap();
        let size = f64::from(p.scale.body);
        let gap = p.space_width(RStyle::BODY, size).max(size * 0.2);
        let gutter = (p.measure("99999999.", RStyle::BODY, size) + gap)
            .max(p.measure("100000000.", RStyle::BODY, size) + gap)
            .max(18.0 * p.unit_scale());
        let items = list(99_999_999, vec![vec![paragraph("Z")], vec![paragraph("Z")]]);
        p.list(&items, 30.0, 540.0, false);
        let z = p.resolve('Z', RStyle::BODY).1;
        let body: Vec<_> = glyphs(&p).into_iter().filter(|glyph| glyph.0 == z).collect();
        assert_eq!(body.len(), 2);
        for glyph in body { close(glyph.1, 30.0 + gutter); }
    }
}

#[test]
fn narrow_list_moves_the_whole_marker_above_content_instead_of_overlapping() {
    let mut p = Poster::new(&options(1.5));
    let z = p.resolve('Z', RStyle::BODY).1;
    p.list(&list(u64::MAX, vec![vec![paragraph("Z")]]), 0.0, 40.0, false);
    let actual = glyphs(&p);
    let body = actual.iter().find(|glyph| glyph.0 == z).unwrap();
    close(body.1, 0.0);
    assert!(actual.iter().filter(|glyph| glyph.0 != z).all(|glyph| glyph.2 < body.2));
    assert_eq!(actual.len(), u64::MAX.to_string().len() + 2);
    assert!(actual.iter().all(|glyph| glyph.1 >= 0.0 && glyph.1 < 40.0));
}

#[test]
fn ordinals_after_u64_max_are_preserved_and_empty_items_advance() {
    let mut p = Poster::new(&SvgOptions::default());
    p.list(&list(u64::MAX, vec![vec![], vec![]]), 0.0, 500.0, false);
    let expected = format!("{}.{}.", u64::MAX, u128::from(u64::MAX) + 1);
    let ids: Vec<_> = expected.chars().map(|ch| p.resolve(ch, RStyle::BODY).1).collect();
    let actual = glyphs(&p);
    assert_eq!(actual.iter().map(|glyph| glyph.0).collect::<Vec<_>>(), ids);
    assert_eq!(actual.iter().map(|glyph| glyph.2.to_bits()).collect::<BTreeSet<_>>().len(), 2);
    assert!(p.y > p.top + 2.0 * 11.0 * p.line_height);
}

#[test]
fn deeply_nested_quotes_stop_consuming_the_remaining_body_measure() {
    let mut block = paragraph("A");
    for _ in 0..80 { block = Block::BlockQuote(vec![block]); }
    let mut p = Poster::new(&options(1.5));
    p.block(&block, 0.0, 72.0, false);
    let actual = glyphs(&p);
    assert_eq!(actual.len(), 1);
    assert!(actual[0].1 <= 72.0 - f64::from(p.scale.body));
    assert!(p.ops.iter().all(|op| match op {
        Op::Rect { x, w, .. } => *x >= 0.0 && *w >= 0.0 && *x + *w <= 72.0,
        _ => true,
    }));
}

#[test]
fn mathematics_scales_but_intrinsic_image_dimensions_do_not() {
    let asset = PdfImageAsset::new("plot", br#"<svg width="20pt" height="10pt"><rect width="20" height="10"/></svg>"#.to_vec());
    let mut outputs = Vec::new();
    for factor in [1.0, 1.5] {
        let mut p = Poster::new(&options(factor)).with_resources(&FontAssets::default(), std::slice::from_ref(&asset)).unwrap();
        p.paragraph(&[Inline::Math("x^2".into()), Inline::SoftBreak,
            Inline::Image { dest: "plot".into(), alt: "plot".into(), title: None }], 0.0, 500.0, false);
        let dimensions = p.ops.iter().find_map(|op| match op {
            Op::Image { run, .. } => Some((run.width, run.height)), _ => None,
        }).unwrap();
        assert_eq!(dimensions, (20.0, 10.0));
        assert!(p.warnings.is_empty());
        outputs.push(glyphs(&p));
    }
    assert_eq!(outputs[0].len(), outputs[1].len());
    assert!(!outputs[0].is_empty());
    for (small, large) in outputs[0].iter().zip(&outputs[1]) {
        assert_eq!(small.0, large.0);
        close(large.3, small.3 * 1.5);
    }
}

#[test]
fn explicit_appearance_selects_the_theme_palette_without_host_state() {
    for appearance in [SystemAppearance::Auto, SystemAppearance::Light, SystemAppearance::Dark,
        SystemAppearance::Charcoal, SystemAppearance::HighContrastLight, SystemAppearance::HighContrastDark] {
        let opts = SvgOptions { theme: Theme::default().with_appearance(appearance), ..Default::default() };
        let p = Poster::new(&opts);
        assert_eq!(&p.colors, opts.theme.effective_colors(false, false));
        if matches!(appearance, SystemAppearance::Dark | SystemAppearance::HighContrastDark) {
            assert_eq!(p.colors, opts.theme.dark_colors);
        }
    }
}

#[test]
fn zero_margin_empty_document_still_has_a_positive_viewbox() {
    let mut opts = SvgOptions::default();
    opts.theme.page.margins = franken_markdown::PageMargins { top_pt: 0.0, right_pt: 0.0,
        bottom_pt: 0.0, left_pt: 0.0 };
    let (bytes, _, warnings) = render_svg_with_diagnostics(&Document::default(), &opts);
    assert!(warnings.is_empty());
    assert!(String::from_utf8(bytes).unwrap().contains("viewBox=\"0 0 612.00 1.00\""));
}
