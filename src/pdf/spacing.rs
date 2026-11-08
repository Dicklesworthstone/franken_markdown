//! Explicit shared-theme spacing, carried through measurement and placement.
//!
//! The historical print layout remains the default. Opting in gives the body
//! leading and per-side table padding their literal theme values; it never
//! rescales those tokens around the historical PDF constants.

use super::{
    Block, FlowKind, Line, PageGeom, PdfOptions, RenderError, Result, TABLE_COL_GUTTER,
    TABLE_MIN_COL_WIDTH,
};

#[derive(Clone, Copy, Default)]
pub(super) struct LineSpacing {
    pub leading: Option<f32>,
    pub padding_top: f32,
    pub padding_bottom: f32,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Policy {
    leading: Option<f32>,
    table_x: f32,
    table_y: f32,
}

impl Policy {
    pub fn new(options: &PdfOptions) -> Self {
        if options.use_theme_spacing {
            Self {
                leading: Some(options.theme.spacing.line_height),
                table_x: options.theme.spacing.table_cell_padding_x_em,
                table_y: options.theme.spacing.table_cell_padding_y_em,
            }
        } else {
            Self::default()
        }
    }

    pub fn enabled(self) -> bool {
        self.leading.is_some()
    }

    pub fn leading(self) -> Option<f32> {
        self.leading
    }

    pub fn flow_leading(self, kind: FlowKind) -> Option<f32> {
        if matches!(
            kind,
            FlowKind::Paragraph | FlowKind::TableHeader | FlowKind::TableRow
        ) {
            self.leading
        } else {
            None
        }
    }

    pub fn table_gutter(self, size: f32, nominal: f32) -> f32 {
        if self.enabled() {
            2.0 * self.table_x * size
        } else if size == nominal {
            TABLE_COL_GUTTER
        } else {
            (TABLE_COL_GUTTER * (size / nominal)).max(8.0)
        }
    }

    pub fn table_padding(self, size: f32) -> f32 {
        self.table_y * size
    }

    pub fn table_line(self, size: f32, index: usize, count: usize) -> LineSpacing {
        LineSpacing {
            leading: self.leading,
            padding_top: if index == 0 {
                self.table_padding(size)
            } else {
                0.0
            },
            padding_bottom: if index + 1 == count {
                self.table_padding(size)
            } else {
                0.0
            },
        }
    }

    pub fn table_gap(self, legacy: f32) -> f32 {
        if self.enabled() { 0.0 } else { legacy }
    }

    pub fn minimum_row_height(self, size: f32) -> f32 {
        size * self.leading.unwrap_or(1.32) + 2.0 * self.table_padding(size)
    }
}

fn invalid(message: impl std::fmt::Display) -> RenderError {
    RenderError::InvalidInput(format!("pdf_theme_spacing: {message}"))
}

pub(super) fn validate_options(options: &PdfOptions) -> Result<()> {
    if !options.use_theme_spacing {
        return Ok(());
    }
    let spacing = options.theme.spacing;
    for (name, value, minimum, maximum) in [
        ("line_height", spacing.line_height, 1.0, 4.0),
        (
            "table_cell_padding_x_em",
            spacing.table_cell_padding_x_em,
            0.0,
            8.0,
        ),
        (
            "table_cell_padding_y_em",
            spacing.table_cell_padding_y_em,
            0.0,
            8.0,
        ),
    ] {
        if !value.is_finite() || !(minimum..=maximum).contains(&value) {
            return Err(invalid(format_args!(
                "{name} must be finite and between {minimum} and {maximum}"
            )));
        }
    }
    Ok(())
}

/// A panel must retain one readable text column without discarding its padding.
/// Container gutters can yield to this measure; the page itself cannot grow.
pub(super) fn validate_tables(
    blocks: &[Block],
    options: &PdfOptions,
    page: PageGeom,
) -> Result<()> {
    let policy = Policy::new(options);
    let size = options.type_scale().table;
    let required = TABLE_MIN_COL_WIDTH + policy.table_gutter(size, size);
    if !policy.enabled() || required <= page.content_w {
        return Ok(());
    }
    fn has_table(blocks: &[Block]) -> bool {
        blocks.iter().any(|block| match block {
            Block::Table(table) => {
                !table.head.is_empty()
                    || !table.align.is_empty()
                    || table.rows.iter().any(|row| !row.is_empty())
            }
            Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                has_table(inner)
            }
            Block::List(list) => list.items.iter().any(|item| has_table(&item.blocks)),
            _ => false,
        })
    }
    if has_table(blocks) {
        Err(invalid(format_args!(
            "horizontal table padding requires at least {required:.3}pt of body width at the nominal table size; increase the page width, reduce its margins or reduce table padding"
        )))
    } else {
        Ok(())
    }
}

/// The legacy pager guarantees progress by placing an oversized first row.
/// Explicit spacing must not create such a row: after note sizing, table
/// wrapping, inline-object fitting and grid quantization, reject its complete
/// non-discardable box before either the strict or fallback planner runs.
/// Continuation headers keep their shared admission rule, which omits a
/// repeated header when that header plus its first body row cannot fit.
pub(super) fn validate_layout(lines: &[Line], options: &PdfOptions, page: PageGeom) -> Result<()> {
    if !options.use_theme_spacing {
        return Ok(());
    }
    let page = super::baseline_grid::page_for_lines(page, lines);
    let capacity = page.top_y() - page.bottom;
    for line in lines.iter().filter(|line| line.spacing.leading.is_some()) {
        let height = super::line_leading(line);
        if !height.is_finite() || height > capacity + 0.001 {
            return Err(invalid(format_args!(
                "vertical spacing requires a {height:.3}pt row but the usable page body is {capacity:.3}pt high; reduce leading or table padding, reduce the font size, or increase the usable page height"
            )));
        }
    }
    Ok(())
}

/// Idempotent and safe for enclosing lists/quotes to apply after their children.
/// Heading/code typography and indivisible figure/rule geometry stay independent.
pub(super) fn apply(lines: &mut [Line], options: &PdfOptions) {
    if !options.use_theme_spacing {
        return;
    }
    let policy = Policy::new(options);
    for line in lines {
        line.spacing.leading = if !line.rule && line.image.is_none() {
            policy.flow_leading(line.flow.kind)
        } else {
            None
        };
    }
}

pub(super) fn nominal_leading(line: &Line) -> f32 {
    line.size * line.spacing.leading.unwrap_or(1.32)
}

/// The opt-in line box includes its ordinary descent, rather than placing it
/// below the measured cursor. Tall glyphs and inline objects expand these rooms.
pub(super) fn rooms(size: f32, leading: Option<f32>) -> (f32, f32) {
    match leading {
        Some(leading) => (size * leading * 0.8, size * leading * 0.2),
        None => (size, size * 0.3),
    }
}

pub(super) fn baseline_lift(line: &Line) -> f32 {
    line.spacing
        .leading
        .map_or(0.0, |leading| line.size * leading * 0.2)
        + line.spacing.padding_bottom
}

/// Same box for zebra painting and the baseline grid's ink admission. Padding
/// belongs only to the first/last physical fragments of a logical table row.
pub(super) fn table_bounds(line: &Line) -> (f32, f32) {
    if line.spacing.leading.is_none() {
        return (
            (line.size * 0.92).max(line.text_ink.0),
            (line.size * 0.40).max(line.text_ink.1),
        );
    }
    let (above, below) = rooms(line.size, line.spacing.leading);
    let (extra_above, extra_below) = super::inline_math_extents(line);
    (
        above + extra_above + line.spacing.padding_top,
        below + extra_below + line.spacing.padding_bottom,
    )
}
