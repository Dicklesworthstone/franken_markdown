//! Physical table bounds and complete source cells checked against painted ink.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{Block, Document, Font, SLOT_COUNT, SvgOptions, render_svg_with_resources};
use super::*;
use franken_markdown::{FontAssets, PdfImageAsset};

fn text(value: impl Into<String>) -> Vec<Inline> {
    vec![Inline::Text(value.into())]
}

fn table(columns: usize) -> Table {
    Table {
        align: vec![Align::Left; columns],
        head: (0..columns).map(|c| text(format!("H{c:02}"))).collect(),
        rows: vec![(0..columns).map(|c| text(format!("W{c:02}"))).collect()],
    }
}

fn panels(p: &Poster) -> Vec<(f64, f64, f64, f64)> {
    p.ops
        .iter()
        .filter_map(|op| match op {
            Op::Rect {
                x,
                y,
                w,
                fill: Ink::BgSubtle,
                ..
            } => {
                let bottom = p
                    .ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::Rule {
                            x1,
                            y1,
                            x2,
                            y2,
                            w: 0.75,
                            ..
                        } if *x1 == *x && *x2 == *x + *w && y1 == y2 && *y1 > *y => Some(*y1),
                        _ => None,
                    })
                    .min_by(f64::total_cmp)
                    .unwrap();
                Some((*x, *y, *x + *w, bottom))
            }
            _ => None,
        })
        .collect()
}

fn source_text(p: &Poster) -> String {
    p.ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph { slot, gid, .. } if *slot < SLOT_COUNT => {
                let font: &Font = p.font_for_slot(*slot).unwrap();
                Some(
                    ('!'..='~')
                        .find(|ch| font.glyph_index(*ch) == *gid)
                        .unwrap(),
                )
            }
            _ => None,
        })
        .collect()
}

/// Read grid edges from the painted rules, then measure independent font ink.
/// A glyph must fit one whole cell, not merely fit the poster's outer viewBox.
fn assert_cells_contain_ink(p: &Poster, left: f64, right: f64) {
    let panels = panels(p);
    assert!(!panels.is_empty());
    for &(l, top, r, bottom) in &panels {
        assert!(l >= left - 0.001 && r <= right + 0.001);
        let mut edges = vec![l, r];
        edges.extend(p.ops.iter().filter_map(|op| match op {
            Op::Rule { x1, y1, x2, y2, .. } if x1 == x2 && *y1 == top && *y2 == bottom => Some(*x1),
            _ => None,
        }));
        edges.sort_by(f64::total_cmp);
        for op in &p.ops {
            let Op::Glyph {
                slot,
                gid,
                x,
                y,
                size,
                ..
            } = op
            else {
                continue;
            };
            if *y < top || *y > bottom {
                continue;
            }
            let font = p.font_for_slot(*slot).unwrap();
            let bbox = font.glyph_bbox(*gid).unwrap();
            let scale = *size / f64::from(font.units_per_em);
            let ink_left = *x + f64::from(bbox[0]) * scale;
            let ink_right = *x + f64::from(bbox[2]) * scale;
            assert!(
                edges
                    .windows(2)
                    .any(|edge| { ink_left >= edge[0] - 0.001 && ink_right <= edge[1] + 0.001 }),
                "glyph {gid} ink {ink_left}..{ink_right} crosses cell edges {edges:?}"
            );
        }
    }
}

#[test]
fn wide_tables_keep_every_cell_readable_inside_its_own_column() {
    let mut p = Poster::new(&SvgOptions::default());
    p.table(&table(60), 72.0, 540.0);
    assert_cells_contain_ink(&p, 72.0, 540.0);
    let panel_count = panels(&p).len();
    assert!(panel_count > 1);
    let painted = source_text(&p);
    for column in 0..60 {
        let repetitions = if column == 0 { panel_count } else { 1 };
        for prefix in ['H', 'W'] {
            assert_eq!(
                painted.matches(&format!("{prefix}{column:02}")).count(),
                repetitions,
                "source column {column} must survive in order with only context repeated"
            );
        }
    }
    assert!(
        p.ops
            .iter()
            .filter_map(|op| match op {
                Op::Glyph { size, .. } => Some(*size),
                _ => None,
            })
            .all(|size| size == 10.0),
        "table type must remain readable"
    );
}

#[test]
fn deeply_nested_narrow_tables_use_complete_single_column_panels() {
    let options = SvgOptions {
        max_width_pt: 144.0,
        ..SvgOptions::default()
    };
    let mut block = Block::Table(table(6));
    for _ in 0..8 {
        block = Block::BlockQuote(vec![block]);
    }
    let mut p = Poster::new(&options);
    p.layout(&Document {
        blocks: vec![block],
    });
    // 144pt poster retains 72pt of body after reducing its equal margins.
    // Nested quotes stop indenting once 11pt remains, so the table starts at97.
    assert_cells_contain_ink(&p, 97.0, 108.0);
    assert_eq!(panels(&p).len(), 6);
    let painted = source_text(&p);
    for column in 0..6 {
        for prefix in ['H', 'W'] {
            assert_eq!(painted.matches(&format!("{prefix}{column:02}")).count(), 1);
        }
    }
}

#[test]
fn uneven_columns_keep_a_readable_floor_without_unnecessary_panels() {
    let mut table = table(8);
    for cell in &mut table.rows[0][..6] {
        *cell = text("W".repeat(60));
    }
    table.rows[0][6] = text("W");
    table.rows[0][7] = text("W");
    let mut p = Poster::new(&SvgOptions::default());
    p.table(&table, 72.0, 372.0);
    assert_eq!(panels(&p).len(), 1);
    assert_cells_contain_ink(&p, 72.0, 372.0);
    assert_eq!(source_text(&p).matches('W').count(), 362);
}

#[test]
fn panelled_rich_cells_keep_links_math_images_alignment_and_financial_rules() {
    let link = |label: &str| {
        vec![Inline::Link {
            dest: "https://example.com/row".into(),
            title: Some("Read the source".into()),
            content: vec![Inline::Strong(text(label))],
        }]
    };
    let table = Table {
        align: vec![
            Align::Left,
            Align::Decimal,
            Align::Center,
            Align::Left,
            Align::Right,
            Align::Right,
        ],
        head: ["ID", "Amount", "Link", "Math", "Image", "Note"]
            .into_iter()
            .map(text)
            .collect(),
        rows: vec![
            vec![
                text("RowA"),
                text("1.2"),
                link("Open"),
                vec![Inline::Math(r"\frac{1}{2}".into())],
                vec![Inline::Image {
                    dest: "badge.svg".into(),
                    title: None,
                    alt: "Complete badge".into(),
                }],
                text("RIGHT"),
            ],
            vec![
                text("RowB"),
                text("12.5"),
                text("Closed"),
                vec![Inline::Math("x".into())],
                text("Absent"),
                text("END"),
            ],
            // Footings apply to the preceding source row. The columns which
            // receive these three rules are spread over two separate panels.
            vec![
                text(""),
                text("==="),
                text("---"),
                text(""),
                text("==="),
                text(""),
            ],
        ],
    };
    let mut options = SvgOptions::default();
    options.theme.spacing.table_cell_padding_x_em = 0.5;
    let images = vec![PdfImageAsset::new(
        "badge.svg",
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#.to_vec(),
    )];
    let fonts = FontAssets::default();
    let mut p = Poster::new(&options)
        .with_resources(&fonts, &images)
        .unwrap();
    p.table(&table, 72.0, 172.0);
    assert_eq!(panels(&p).len(), 3);
    assert_cells_contain_ink(&p, 72.0, 172.0);
    let painted = source_text(&p);
    for label in ["ID", "RowA", "RowB"] {
        assert_eq!(painted.matches(label).count(), 3, "context column {label}");
    }
    for label in [
        "Amount", "Link", "Math", "Image", "Note", "Open", "Closed", "RIGHT", "END",
    ] {
        assert_eq!(painted.matches(label).count(), 1, "source cell {label}");
    }
    assert!(!painted.contains("===") && !painted.contains("---"));
    let rules: Vec<_> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Rule {
                x1,
                y1,
                x2,
                y2,
                w: 0.7,
                ..
            } if y1 == y2 => Some((*x1, *x2, *y1)),
            _ => None,
        })
        .collect();
    assert_eq!(rules.len(), 5, "two double and one single footing");
    // No financial rule belongs to the repeated identifier column.
    for &(left, right, y) in &rules {
        let &(_, top, _, bottom) = panels(&p)
            .iter()
            .find(|&&(_, top, _, bottom)| y >= top && y <= bottom)
            .unwrap();
        let boundary = p
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Rule { x1, y1, x2, y2, .. } if x1 == x2 && *y1 == top && *y2 == bottom => {
                    Some(*x1)
                }
                _ => None,
            })
            .min_by(f64::total_cmp)
            .unwrap();
        assert!(left >= boundary && right <= 172.0);
    }
    let pictures: Vec<_> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Image { run, x, y } => Some((run, *x, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].0.alt, "Complete badge");
    assert!(pictures[0].1 >= 72.0 && pictures[0].1 + pictures[0].0.width <= 172.0);
    assert_eq!(pictures[0].0.width / pictures[0].0.height, 2.0);
    assert!(
        p.ops
            .iter()
            .any(|op| matches!(op, Op::Glyph { slot, .. } if *slot >= SLOT_COUNT))
    );
    let dots: Vec<_> = p
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph { slot, gid, x, .. }
                if *slot < SLOT_COUNT && *gid == p.font_for_slot(*slot)?.glyph_index('.') =>
            {
                Some(*x)
            }
            _ => None,
        })
        .collect();
    assert_eq!(dots.len(), 2);
    assert!(
        (dots[0] - dots[1]).abs() < 0.00001,
        "decimal anchors must remain aligned"
    );
    let height = p.y + 72.0;
    let (bytes, _, warnings) = p.emit(height);
    let svg = String::from_utf8(bytes).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    // The one source link may have a hit region on each wrapped physical line.
    let regions = svg.matches("href=\"https://example.com/row\"").count();
    assert!(regions > 0);
    assert_eq!(regions, svg.matches("aria-label=\"Open\"").count());

    let document = Document {
        blocks: vec![Block::Table(table)],
    };
    let original = document.clone();
    let narrow = SvgOptions {
        max_width_pt: 144.0,
        ..options
    };
    let first = render_svg_with_resources(&document, &narrow, &fonts, &images).unwrap();
    let second = render_svg_with_resources(&document, &narrow, &fonts, &images).unwrap();
    assert_eq!(first, second);
    assert_eq!(document, original);
}
