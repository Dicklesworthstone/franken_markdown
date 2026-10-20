//! Decimal-aligned HTML for explicitly marked financial tables.
//!
//! Each numeric cell ends with a shared, per-column suffix track. Its prefix
//! keeps its intrinsic width, so right alignment places every decimal origin
//! at the same coordinate, without inserting padding into the cell's text.
//! Footing markers decorate existing cells; they never become accessible rows.

use std::borrow::Cow;

use super::{
    Font, FontStyle, HtmlOptions, Inline, RenderState, body_font_face, render_inlines,
    render_link_open,
};
use crate::ast::{Align, Table};
use crate::table_format::{
    FinancialRow, FootingRule, decimal_anchor, decimal_cell_text, split_decimal_cell,
};
use crate::text::Kerning;

pub(super) fn render(
    table: &Table,
    rows: &[FinancialRow<'_>],
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) {
    let widths = suffix_widths(table, rows, opts);
    out.push_str(
        "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n<table class=\"fmd-financial-table\">\n<thead>\n<tr>",
    );
    render_cells(
        &table.head,
        &table.align,
        &[],
        &widths,
        true,
        out,
        opts,
        state,
    );
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    for row in rows {
        out.push_str("<tr>");
        render_cells(
            row.cells,
            &table.align,
            &row.rules,
            &widths,
            false,
            out,
            opts,
            state,
        );
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n</table>\n</div>\n");
}

#[allow(clippy::too_many_arguments)]
fn render_cells(
    cells: &[Vec<Inline>],
    align: &[Align],
    rules: &[FootingRule],
    widths: &[Option<f32>],
    header: bool,
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) {
    for (column, cell) in cells.iter().enumerate() {
        out.push_str(if header { "<th scope=\"col\"" } else { "<td" });
        let alignment = align.get(column).copied().unwrap_or(Align::None);
        let rule = rules.get(column).copied().unwrap_or_default();
        if alignment != Align::None || rule != FootingRule::None {
            out.push_str(" style=\"");
            out.push_str(match alignment {
                Align::None => "",
                Align::Left => "text-align:left;",
                Align::Center => "text-align:center;",
                Align::Right | Align::Decimal => "text-align:right;",
            });
            out.push_str(match rule {
                FootingRule::None => "",
                FootingRule::Single => "border-bottom:1px solid currentColor;",
                FootingRule::Double => "border-bottom:3px double currentColor;",
            });
            out.push('"');
        }
        out.push('>');
        if !header
            && alignment == Align::Decimal
            && let Some(width) = widths.get(column).copied().flatten()
            && let Some(text) = decimal_cell_text(cell)
            && let Some(anchor) = decimal_anchor(&text)
        {
            render_decimal(cell, anchor, width, out, opts, state);
        } else {
            render_inlines(cell, out, opts, state);
        }
        out.push_str(if header { "</th>" } else { "</td>" });
    }
}

fn render_decimal(
    cell: &[Inline],
    anchor: usize,
    width: f32,
    out: &mut String,
    opts: &HtmlOptions,
    state: &mut RenderState<'_>,
) {
    // A whole-cell link remains one accessible link and one keyboard stop.
    // Keep surrounding styles outside the grid as well; only mixed inline
    // runs need to split their wrappers at the decimal boundary.
    if let [wrapper] = cell {
        let wrapped = match wrapper {
            Inline::Strong(content) => Some(("<strong>", "</strong>", content)),
            Inline::Emphasis(content) => Some(("<em>", "</em>", content)),
            Inline::Strikethrough(content) => Some(("<del>", "</del>", content)),
            _ => None,
        };
        if let Some((open, close, content)) = wrapped {
            out.push_str(open);
            render_decimal(content, anchor, width, out, opts, state);
            out.push_str(close);
            return;
        }
        if let Inline::Link {
            dest,
            title,
            content,
        } = wrapper
        {
            let linked = render_link_open(dest, title.as_deref(), out, state);
            render_decimal(content, anchor, width, out, opts, state);
            if linked {
                out.push_str("</a>");
            }
            return;
        }
    }
    let (before, after) = split_decimal_cell(cell, anchor);
    // Round upward so decimal serialization never narrows a measured track.
    // The width is in the cell's own em, honoring uniform font scaling.
    let width = (width * 1000.0).ceil() / 1000.0;
    out.push_str(&format!(
        "<span class=\"fmd-decimal\" style=\"display:inline-grid;grid-template-columns:max-content {width:.3}em;column-gap:0;text-align:left;white-space:pre;direction:ltr;unicode-bidi:isolate\"><span class=\"fmd-decimal-before\">"
    ));
    render_inlines(&before, out, opts, state);
    out.push_str("</span><span class=\"fmd-decimal-after\">");
    render_inlines(&after, out, opts, state);
    out.push_str("</span></span>");
}

fn suffix_widths(table: &Table, rows: &[FinancialRow<'_>], opts: &HtmlOptions) -> Vec<Option<f32>> {
    let mut widths: Vec<_> = table
        .align
        .iter()
        .map(|align| (*align == Align::Decimal).then_some(0.0f32))
        .collect();
    let mut metrics = Metrics::new(opts);
    // Years and numeric headings are labels, matching PDF and SVG headers.
    for row in rows {
        for (cell, width) in row.cells.iter().zip(&mut widths) {
            if let Some(current) = *width
                && let Some(text) = decimal_cell_text(cell)
                && let Some(anchor) = decimal_anchor(&text)
            {
                let (_, after) = split_decimal_cell(cell, anchor);
                *width = metrics.width(&after, false).map(|value| current.max(value));
            }
        }
    }
    widths
}

struct Face {
    font: Cow<'static, Font>,
    kern: Kerning,
}

struct Metrics<'a> {
    opts: &'a HtmlOptions,
    faces: [Option<Face>; 4],
}

impl<'a> Metrics<'a> {
    fn new(opts: &'a HtmlOptions) -> Self {
        Self {
            opts,
            faces: std::array::from_fn(|_| None),
        }
    }

    fn face(&mut self, style: FontStyle) -> Option<&Face> {
        let index = style_index(style);
        if self.faces[index].is_none() {
            let source = body_font_face(&self.opts.font_assets, self.opts.theme.font, style);
            let font = if let Some(font) = source.parsed {
                Cow::Borrowed(font)
            } else if let Some(weight) = source.instance_weight {
                Cow::Owned(crate::instance_host_font(source.slot, source.bytes, weight).ok()?)
            } else {
                Cow::Owned(Font::parse(source.bytes.to_vec()).ok()?)
            };
            let kern = font.gpos_kerning();
            self.faces[index] = Some(Face { font, kern });
        }
        self.faces[index].as_ref()
    }

    fn width(&mut self, cell: &[Inline], header: bool) -> Option<f32> {
        let mut runs = Vec::new();
        styled_runs(cell, header, false, &mut runs);
        let mut width = 0.0f32;
        for (style, text) in runs {
            let face = self.face(style)?;
            let mut units = 0.0;
            let mut previous = None;
            for character in text.chars() {
                let glyph = face.font.glyph_index(character);
                if let Some(left) = previous {
                    units += f32::from(face.kern.pair(left, glyph));
                }
                units += f32::from(face.font.advance_width(glyph));
                previous = Some(glyph);
            }
            width += units / f32::from(face.font.units_per_em);
        }
        width.is_finite().then_some(width.max(0.0))
    }
}

fn style_index(style: FontStyle) -> usize {
    match style {
        FontStyle::Regular => 0,
        FontStyle::Bold => 1,
        FontStyle::Italic => 2,
        FontStyle::BoldItalic => 3,
    }
}

fn styled_runs(cell: &[Inline], bold: bool, italic: bool, runs: &mut Vec<(FontStyle, String)>) {
    for inline in cell {
        match inline {
            Inline::Text(text) => {
                let style = FontStyle::new(bold, italic);
                if let Some((last_style, last_text)) = runs.last_mut()
                    && *last_style == style
                {
                    last_text.push_str(text);
                } else {
                    runs.push((style, text.clone()));
                }
            }
            Inline::Strong(content) => styled_runs(content, true, italic, runs),
            Inline::Emphasis(content) => styled_runs(content, bold, true, runs),
            Inline::Strikethrough(content) | Inline::Link { content, .. } => {
                styled_runs(content, bold, italic, runs);
            }
            // Admission already excluded boxes, raw markup, and note marks.
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{Block, FontAssetSlot, FontAssets, FontFamily, parse_markdown};

    fn widths(source: &str, opts: &HtmlOptions) -> Vec<Option<f32>> {
        let doc = parse_markdown(source);
        let Block::Table(table) = &doc.blocks[0] else {
            panic!("table");
        };
        suffix_widths(
            table,
            &crate::table_format::financial_rows(table).unwrap(),
            opts,
        )
    }

    #[test]
    fn suffix_tracks_use_styled_host_faces_and_independent_column_maxima() {
        let source = "| Item | Amount | Rate |\n| --- | ---. | ---. |\n| A | 1.2 | 7.12345% |\n| B | **2.3456** | 2.5% |\n";
        let opts = HtmlOptions {
            font_assets: FontAssets::default()
                .with_slot(
                    FontAssetSlot::BodyRegular,
                    crate::fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec(),
                )
                .unwrap()
                .with_slot(
                    FontAssetSlot::BodyBold,
                    crate::fonts::body_bytes(FontFamily::Sans, FontStyle::Bold).to_vec(),
                )
                .unwrap(),
            ..HtmlOptions::default()
        };
        let measured = widths(source, &opts);
        let mut metrics = Metrics::new(&opts);
        let regular = metrics.width(&[Inline::Text(".2".into())], false).unwrap();
        let bold = metrics
            .width(&[Inline::Strong(vec![Inline::Text(".3456".into())])], false)
            .unwrap();
        let rate = metrics
            .width(&[Inline::Text(".12345%".into())], false)
            .unwrap();
        assert!(bold > regular);
        assert_eq!(measured, [None, Some(bold), Some(rate)]);
        let default = widths(source, &HtmlOptions::default());
        assert_ne!(
            measured[2], default[2],
            "host body metrics must influence the suffix track"
        );
    }

    #[test]
    fn metric_faces_use_the_same_variable_weight_instances_as_embedded_html() {
        let bytes = crate::text::variable_triangle_fixture();
        let opts = HtmlOptions {
            font_assets: FontAssets::default()
                .with_slot(FontAssetSlot::BodyRegular, bytes.clone())
                .unwrap()
                .with_slot_weight(FontAssetSlot::BodyRegular, 400)
                .unwrap()
                .with_slot_weight(FontAssetSlot::BodyBold, 900)
                .unwrap(),
            ..HtmlOptions::default()
        };
        let mut metrics = Metrics::new(&opts);
        for (style, slot, weight) in [
            (FontStyle::Regular, FontAssetSlot::BodyRegular, 400),
            (FontStyle::Bold, FontAssetSlot::BodyBold, 900),
        ] {
            let expected = crate::instance_host_font(slot, &bytes, weight).unwrap();
            assert_eq!(
                metrics.face(style).unwrap().font.as_sfnt(),
                expected.as_sfnt()
            );
        }
        assert_ne!(
            metrics
                .face(FontStyle::Regular)
                .unwrap()
                .font
                .as_sfnt()
                .to_vec(),
            metrics
                .face(FontStyle::Bold)
                .unwrap()
                .font
                .as_sfnt()
                .to_vec(),
            "fixture must exercise distinct wght instances"
        );
    }

    #[test]
    fn measured_runs_keep_gpos_pairs_across_style_neutral_link_boundaries() {
        let opts = HtmlOptions::default();
        let mut metrics = Metrics::new(&opts);
        let text = "AVTo";
        let font = crate::fonts::body_font(FontFamily::Sans, FontStyle::Regular).unwrap();
        let advances: f32 = text
            .chars()
            .map(|c| f32::from(font.advance_width(font.glyph_index(c))))
            .sum();
        let glyphs: Vec<_> = text.chars().map(|c| font.glyph_index(c)).collect();
        let kern = font.gpos_kerning();
        let adjustments: i32 = glyphs
            .windows(2)
            .map(|pair| i32::from(kern.pair(pair[0], pair[1])))
            .sum();
        assert_ne!(adjustments, 0, "witness must contain actual GPOS pairs");
        let actual = metrics
            .width(
                &[
                    Inline::Text("A".into()),
                    Inline::Link {
                        dest: "#value".into(),
                        title: None,
                        content: vec![Inline::Text("VT".into())],
                    },
                    Inline::Text("o".into()),
                ],
                false,
            )
            .unwrap();
        let expected = (advances + adjustments as f32) / f32::from(font.units_per_em);
        assert!((actual - expected).abs() < 0.00001);
    }
}
