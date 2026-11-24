//! Assertions against retained glyph origins and the real vector painter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{Align, Block, Ink, Op, Poster, SvgOptions, Table, render_svg_with_diagnostics};
use super::*;
use franken_markdown::{parse_markdown, theme::FontFamily};

fn table(source: &str) -> Table {
    let doc = parse_markdown(source);
    let Block::Table(table) = &doc.blocks[0] else {
        panic!("expected table");
    };
    table.clone()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
}

fn dots(p: &Poster) -> Vec<f64> {
    p.ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph { slot, gid, x, .. } if *gid == p.font_for_slot(*slot)?.glyph_index('.') => {
                Some(*x)
            }
            _ => None,
        })
        .collect()
}

fn footings(p: &Poster) -> Vec<(f64, f64, f64)> {
    p.ops
        .iter()
        .filter_map(|op| match op {
            Op::Rule {
                x1,
                y1,
                x2,
                y2,
                ink: Ink::Border,
                w,
            } if *w == 0.7 && y1 == y2 => Some((*x1, *x2, *y1)),
            _ => None,
        })
        .collect()
}

#[test]
fn financial_svg_aligns_painted_decimal_glyphs_across_styles_and_precision() {
    let source = "| Item | Amount |\n| --- | ---. |\n\
                  | Revenue | 1,234.5 |\n\
                  | Cost | (234.50) |\n\
                  | Total | **9.0000** |\n\
                  | Currency | $12.25 |\n\
                  | Rate | *2.5%* |\n\
                  | Linked | [7.00](https://example.com/value) |\n\
                  | Mixed | **20**.125 |\n\
                  | Integer | 17 |\n";
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let opts = SvgOptions {
            theme: franken_markdown::theme::Theme::default().with_font(family),
            ..Default::default()
        };
        let mut p = Poster::new(&opts);
        p.table(&table(source), 72.0, 540.0);
        let anchors = dots(&p);
        assert_eq!(anchors.len(), 7);
        for anchor in &anchors {
            close(*anchor, anchors[0]);
        }
        // The final integer has no decimal glyph: its last digit's advance
        // ends exactly at the same virtual anchor.
        let (slot, gid, x, size) = p
            .ops
            .iter()
            .rev()
            .find_map(|op| match op {
                Op::Glyph {
                    slot, gid, x, size, ..
                } => Some((*slot, *gid, *x, *size)),
                _ => None,
            })
            .unwrap();
        let face = p.font_for_slot(slot).unwrap();
        assert_eq!(gid, face.glyph_index('7'));
        close(
            x + f64::from(face.advance_width(gid)) * size / f64::from(face.units_per_em),
            anchors[0],
        );
        assert!(
            p.ops
                .iter()
                .any(|op| matches!(op, Op::Glyph { slot: 1, .. }))
        );
        assert!(
            p.ops
                .iter()
                .any(|op| matches!(op, Op::Glyph { slot: 2, .. }))
        );
        let doc = parse_markdown(source);
        let first = render_svg_with_diagnostics(&doc, &opts);
        assert_eq!(first, render_svg_with_diagnostics(&doc, &opts));
        assert!(first.2.is_empty(), "{:?}", first.2);
        assert_eq!(first.1.glyphs_missing, 0);
        let svg = String::from_utf8(first.0).unwrap();
        assert!(svg.contains("href=\"https://example.com/value\""));
        assert!(svg.contains("pointer-events=\"all\""));
    }
}

#[test]
fn financial_svg_anchor_includes_actual_kerning_before_decimal() {
    // Bundled body fonts have no digit-dot pair adjustment. Use the existing
    // independently specified shaping fixture's a/b pair to exercise the
    // retained-cluster accessor used for a decimal glyph: 500 - 40 = 460 units.
    let fonts = franken_markdown::FontAssets {
        body_regular: Some(
            include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf").to_vec(),
        ),
        ..Default::default()
    };
    let mut p = Poster::new(&SvgOptions::default())
        .with_resources(&fonts, &[])
        .unwrap();
    let lines = p.wrap(
        &[super::super::Piece::Text("ab".into(), RStyle::BODY)],
        10.0,
        500.0,
    );
    let prepared = lines[0][0].shaped.as_ref().unwrap();
    let before = prepared.decimal_anchor_position(1, true).unwrap();
    close(before, 4.6);
    close(prepared.decimal_anchor_position(1, false).unwrap(), 5.0);
    p.draw_words(&lines[0], 30.0 - before, 50.0, 10.0);
    let painted = p
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Glyph {
                slot: 0, gid: 3, x, ..
            } => Some(*x),
            _ => None,
        })
        .unwrap();
    close(painted, 30.0);
}

#[test]
fn financial_svg_reserves_combined_integer_and_fraction_extents() {
    let t = table(
        "| Item | Amount |\n| --- | ---. |\n\
                   | A | 123456789.0 |\n| B | 1.123456789 |\n",
    );
    let mut p = Poster::new(&SvgOptions::default());
    p.table(&t, 72.0, 540.0);
    let anchors = dots(&p);
    assert_eq!(anchors.len(), 2);
    close(anchors[0], anchors[1]);
    let column_edge = p
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Rule {
                x1, x2, y1, y2, w, ..
            } if x1 == x2 && y1 != y2 && *w == 0.5 => Some(*x1),
            _ => None,
        })
        .unwrap();
    let body_top = p
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Rect {
                y,
                h,
                fill: Ink::BgSubtle,
                ..
            } => Some(y + h),
            _ => None,
        })
        .unwrap();
    for op in &p.ops {
        if let Op::Glyph {
            slot,
            gid,
            x,
            y,
            size,
            ..
        } = op
        {
            let face = p.font_for_slot(*slot).unwrap();
            if *y > body_top && ('0'..='9').any(|ch| face.glyph_index(ch) == *gid) {
                assert!(*x >= column_edge + p.table_pad_x - 1e-7);
                let end =
                    x + f64::from(face.advance_width(*gid)) * size / f64::from(face.units_per_em);
                assert!(end <= 540.0 - p.table_pad_x + 1e-7);
            }
        }
    }
}

#[test]
fn financial_svg_footings_fold_markers_and_clear_wrapped_rows_and_deep_math() {
    let source = "| Item | Amount |\n| --- | ---. |\n\
                  | A long description that occupies several lines | $\\frac{1}{\\frac{2}{3}}$ |\n\
                  | --- | === |\n";
    let mut p = Poster::new(&SvgOptions::default());
    let t = table(source);
    p.table(&t, 20.0, 170.0);
    let rules = footings(&p);
    assert_eq!(rules.len(), 3);
    close(rules[0].2, rules[1].2);
    close(rules[2].2 - rules[1].2, 2.0);
    assert!(rules[0].1 < rules[1].0);
    let deepest_ink = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph {
                slot, gid, y, size, ..
            } => {
                let face = p.font_for_slot(*slot)?;
                let bbox = face.glyph_bbox(*gid)?;
                Some(y - f64::from(bbox[1]) * size / f64::from(face.units_per_em))
            }
            _ => None,
        })
        .fold(0.0_f64, f64::max);
    assert!(rules[0].2 > deepest_ink, "footing crosses row ink");
    assert!(
        p.ops
            .iter()
            .any(|op| matches!(op, Op::Glyph { slot, .. } if *slot >= super::super::SLOT_COUNT))
    );
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    // Marker rows contribute no glyphs. The visible data is identical to the
    // same table with those rows removed.
    let mut unmarked = t;
    unmarked.rows.pop();
    let mut reference = Poster::new(&SvgOptions::default());
    reference.table(&unmarked, 20.0, 170.0);
    let glyphs = |poster: &Poster| {
        poster
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Glyph {
                    slot,
                    gid,
                    x,
                    y,
                    size,
                    ..
                } => Some((*slot, *gid, *x, *y, *size)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(glyphs(&p), glyphs(&reference));
    close(p.y - reference.y, 5.0);
}

#[test]
fn financial_svg_narrow_wrapped_numbers_fall_back_to_existing_right_alignment() {
    let t = table("| Amount |\n| ---. |\n| 123456789.0123456789 |\n");
    let mut financial = Poster::new(&SvgOptions::default());
    financial.table(&t, 20.0, 60.0);
    let body_top = financial
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Rect {
                y,
                h,
                fill: Ink::BgSubtle,
                ..
            } => Some(y + h),
            _ => None,
        })
        .unwrap();
    let mut lines: Vec<(f64, f64)> = Vec::new();
    for op in &financial.ops {
        if let Op::Glyph {
            slot,
            gid,
            x,
            y,
            size,
            ..
        } = op
            && *y > body_top
        {
            let face = financial.font_for_slot(*slot).unwrap();
            let end = x + f64::from(face.advance_width(*gid)) * size / f64::from(face.units_per_em);
            if let Some((_, right)) = lines.last_mut().filter(|(baseline, _)| baseline == y) {
                *right = end;
            } else {
                lines.push((*y, end));
            }
        }
    }
    assert!(lines.len() > 1);
    for (_, edge) in lines {
        close(edge, 60.0 - financial.table_pad_x.min(10.0));
    }
}

#[test]
fn ordinary_svg_tables_keep_marker_text_literal_and_have_no_footings() {
    let mut t = table("| Item | Amount |\n| --- | ---: |\n| A | 12.5 |\n| | === |\n");
    let mut ordinary = Poster::new(&SvgOptions::default());
    ordinary.table(&t, 72.0, 540.0);
    assert!(footings(&ordinary).is_empty());
    let equals = ordinary.faces[0].as_ref().unwrap().glyph_index('=');
    assert_eq!(
        ordinary
            .ops
            .iter()
            .filter(|op| matches!(op, Op::Glyph { slot: 0, gid, .. } if *gid == equals))
            .count(),
        3
    );
    t.align[1] = Align::Decimal;
    let mut financial = Poster::new(&SvgOptions::default());
    financial.table(&t, 72.0, 540.0);
    assert_eq!(footings(&financial).len(), 2);
    assert!(
        !financial
            .ops
            .iter()
            .any(|op| matches!(op, Op::Glyph { slot: 0, gid, .. } if *gid == equals))
    );
    assert!(financial.y < ordinary.y);
}

fn cited_statement() -> String {
    let mut source = String::from("Earlier citations");
    for number in 1..=9 {
        source.push_str(&format!("[^n{number}]"));
    }
    source.push_str(
        "\n\n| Item | Amount |\n| --- | ---. |\n\
         | Revenue | 1,234.5[^n10] |\n\
         | Cost | (234.50)[^n11][^n12] |\n\
         | Total | **9.0000[^n10]** |\n\
         | Currency | $12.25[^n11] |\n\
         | Rate | *2.5%[^n10]* |\n\
         | Linked | [7.00[^n10]](https://example.com/value) |\n\
         | Mixed | **20**.125[^n12] |\n\
         | Integer | 17[^n11] |\n\n",
    );
    for number in 1..=12 {
        source.push_str(&format!("[^n{number}]: Evidence\n"));
    }
    source
}

#[test]
fn cited_financial_svg_aligns_complete_prepared_runs_and_preserves_navigation() {
    let source = cited_statement();
    let doc = parse_markdown(&source);
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let opts = SvgOptions {
            theme: franken_markdown::theme::Theme::default().with_font(family),
            ..Default::default()
        };
        let mut poster = Poster::new(&opts);
        poster.layout(&doc);
        let anchors = dots(&poster);
        assert_eq!(anchors.len(), 7);
        for anchor in &anchors {
            close(*anchor, anchors[0]);
        }

        // The only ordinary "17" in the document ends at the shared virtual
        // decimal anchor; its source-owned superscript suffix is still painted.
        let integer_end = poster
            .ops
            .windows(2)
            .find_map(|pair| match pair {
                [
                    Op::Glyph { slot, gid, y, .. },
                    Op::Glyph {
                        slot: next_slot,
                        gid: next_gid,
                        x,
                        y: next_y,
                        size,
                        ..
                    },
                ] if slot == next_slot && y == next_y => {
                    let face = poster.font_for_slot(*slot)?;
                    (*gid == face.glyph_index('1') && *next_gid == face.glyph_index('7')).then_some(
                        x + f64::from(face.advance_width(*next_gid)) * size
                            / f64::from(face.units_per_em),
                    )
                }
                _ => None,
            })
            .unwrap();
        close(integer_end, anchors[0]);

        let output = render_svg_with_diagnostics(&doc, &opts);
        assert_eq!(output, render_svg_with_diagnostics(&doc, &opts));
        assert_eq!(output.1.glyphs_missing, 0);
        assert!(output.2.is_empty(), "{:?}", output.2);
        let svg = String::from_utf8(output.0).unwrap();
        assert!(svg.contains("href=\"https://example.com/value\""));
        for number in [10, 11, 12] {
            assert!(svg.contains(&format!("href=\"#fmd-note-{number}\"")));
            assert!(svg.contains(&format!("href=\"#fmd-note-ref-{number}\"")));
        }
    }
}

#[test]
fn cited_financial_svg_reserves_full_suffixes_from_other_rows() {
    let mut source = String::from(
        "| Item | Amount |\n| --- | ---. |\n\
         | A | 123456789.0[^n1] |\n\
         | B | **1.123456789**",
    );
    for number in 2..=12 {
        source.push_str(&format!("[^n{number}]"));
    }
    source.push_str(" |\n\n");
    for number in 1..=12 {
        source.push_str(&format!("[^n{number}]: Evidence\n"));
    }
    let mut poster = Poster::new(&SvgOptions::default());
    poster.layout(&parse_markdown(&source));
    let anchors = dots(&poster);
    assert_eq!(anchors.len(), 2);
    close(anchors[0], anchors[1]);
    let (column_edge, body_bottom) = poster
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Rule { x1, x2, y2, w, .. } if x1 == x2 && *w == 0.5 => Some((*x1, *y2)),
            _ => None,
        })
        .unwrap();
    let body_top = poster
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Rect {
                y,
                h,
                fill: Ink::BgSubtle,
                ..
            } => Some(y + h),
            _ => None,
        })
        .unwrap();
    let mut superscripts = 0;
    for op in &poster.ops {
        if let Op::Glyph {
            slot,
            gid,
            x,
            y,
            size,
            ..
        } = op
            && *x > column_edge
            && *y > body_top
            && *y < body_bottom
        {
            let face = poster.font_for_slot(*slot).unwrap();
            let bbox = face.glyph_bbox(*gid).unwrap();
            let scale = size / f64::from(face.units_per_em);
            assert!(x + f64::from(bbox[0]) * scale >= column_edge + poster.table_pad_x - 1e-7);
            assert!(
                x + f64::from(bbox[2]) * scale
                    <= poster.content_right() - poster.table_pad_x + 1e-7
            );
            superscripts +=
                usize::from("⁰¹²³⁴⁵⁶⁷⁸⁹".chars().any(|ch| face.glyph_index(ch) == *gid));
        }
    }
    // Nine one-digit labels followed by 10, 11 and 12, all still present.
    assert_eq!(superscripts, 15);
}

#[test]
fn citation_admission_does_not_claim_undefined_embedded_or_authored_marks() {
    for value in [
        "7.00[^missing]",
        "7[^a].00",
        "7.00¹",
        "[7.00¹](#fn-a)",
        "`7.00`[^a]",
        "$7.00$[^a]",
        "7.00[^a] units",
    ] {
        let source = format!("| Amount |\n| ---. |\n| {value} |\n\n[^a]: Evidence\n");
        let ordinary = source.replace("---.", "---:");
        let opts = SvgOptions::default();
        assert_eq!(
            render_svg_with_diagnostics(&parse_markdown(&source), &opts),
            render_svg_with_diagnostics(&parse_markdown(&ordinary), &opts),
            "ordinary right-aligned cell changed: {value}",
        );
    }
}

#[test]
fn superscript_fallback_retains_real_styled_glyphs_and_primary_font_precedence() {
    use super::super::{FontStyle, fonts};
    use franken_markdown::FontAssets;

    let digits = "⁰¹²³⁴⁵⁶⁷⁸⁹";
    let source = format!("{digits}\n\n**{digits}**\n\n*{digits}*\n\n***{digits}***");
    let doc = parse_markdown(&source);
    let styles = [
        FontStyle::Regular,
        FontStyle::Bold,
        FontStyle::Italic,
        FontStyle::BoldItalic,
    ];
    for family in [FontFamily::Sans, FontFamily::Serif] {
        let options = SvgOptions {
            theme: franken_markdown::theme::Theme::default().with_font(family),
            ..Default::default()
        };
        let mut poster = Poster::new(&options);
        poster.layout(&parse_markdown("Ordinary **bold** *italic* prose"));
        assert!(
            poster
                .superscript_faces
                .iter()
                .all(|face| face.get().is_none())
        );
        assert_eq!(poster.resolve('\u{10ffff}', RStyle::BODY).1, 0);
        assert!(
            poster
                .superscript_faces
                .iter()
                .all(|face| face.get().is_none())
        );
        poster.ops.clear();
        poster.layout(&doc);
        let painted: Vec<_> = poster
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Glyph { slot, gid, .. } => Some((*slot, *gid)),
                _ => None,
            })
            .collect();
        assert_eq!(painted.len(), 40, "all ten numerals in all four styles");
        for (run, style) in painted.as_chunks::<10>().0.iter().zip(styles) {
            let expected = fonts::load_body(FontFamily::Sans, style).unwrap();
            for ((slot, gid), digit) in run.iter().copied().zip(digits.chars()) {
                assert_ne!(gid, 0);
                assert_eq!(gid, expected.glyph_index(digit));
                let actual = poster.font_for_slot(slot).unwrap();
                let expected_path = Poster::glyph_path(&expected, expected.glyph_index(digit))
                    .expect("each superscript has a real outline");
                assert_eq!(Poster::glyph_path(actual, gid), Some(expected_path));
            }
        }
        assert_eq!(poster.missing, 0);
        assert!(poster.superscript_faces.iter().all(|face| {
            face.get().is_some_and(Option::is_some) == (family == FontFamily::Serif)
        }));
    }

    // A supplied primary face that already owns a superscript stays primary.
    let options = SvgOptions {
        theme: franken_markdown::theme::Theme::default().with_font(FontFamily::Serif),
        ..Default::default()
    };
    let poster = Poster::new(&options)
        .with_resources(
            &FontAssets {
                body_regular: Some(
                    fonts::body_bytes(FontFamily::Sans, FontStyle::Regular).to_vec(),
                ),
                ..FontAssets::default()
            },
            &[],
        )
        .unwrap();
    for digit in digits.chars() {
        let (slot, gid) = poster.resolve(digit, RStyle::BODY);
        assert_eq!(slot, 0);
        assert_ne!(gid, 0);
    }
    assert!(
        poster
            .superscript_faces
            .iter()
            .all(|face| face.get().is_none())
    );
}
