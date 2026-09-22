//! Resolve the shared theme once, before any measurement or painting.
//! SVG remains a single growing page; its physical width is not a CSS measure.

use franken_markdown::theme::{ThemeSpacing, TypeScale};
use super::{SvgOptions, SvgWarning};

pub(super) struct Geometry {
    pub(super) width: f64,
    pub(super) left: f64,
    pub(super) right: f64,
    pub(super) top: f64,
    pub(super) bottom: f64,
    pub(super) scale: TypeScale,
    pub(super) line_height: f64,
    pub(super) table_pad_x: f64,
    pub(super) table_pad_y: f64,
    pub(super) warnings: Vec<SvgWarning>,
}

pub(super) fn resolve(options: &SvgOptions) -> Geometry {
    let mut warnings = Vec::new();
    let defaults = ThemeSpacing::default();
    let spacing = &options.theme.spacing;
    let width = bounded(options.max_width_pt, 612.0, 144.0, 14_400.0,
        "poster width", &mut warnings);
    // The shared theme stores its root size in integral CSS pixels. Preserve
    // that contract's 16px -> 11pt baseline, then use the existing PDF ladder
    // and its 6..24pt body-size bounds rather than an independent size table.
    let base_px = if spacing.base_px == 0 {
        adjusted(&mut warnings, "zero base size replaced with the default");
        defaults.base_px
    } else {
        spacing.base_px
    };
    let requested = TypeScale::default().body * f32::from(base_px) / f32::from(defaults.base_px);
    let scale = TypeScale::resolve(Some(requested), None, None);
    if scale.body != requested {
        adjusted(&mut warnings, "body size clamped to the shared 6..24pt range");
    }
    let line_height = bounded(spacing.line_height, defaults.line_height, 1.0, 4.0,
        "body leading", &mut warnings);
    let table_pad_x = bounded(spacing.table_cell_padding_x_em,
        defaults.table_cell_padding_x_em, 0.0, 8.0, "horizontal table padding", &mut warnings)
        * f64::from(scale.table);
    let table_pad_y = bounded(spacing.table_cell_padding_y_em,
        defaults.table_cell_padding_y_em, 0.0, 8.0, "vertical table padding", &mut warnings)
        * f64::from(scale.table);
    let margins = &options.theme.page.margins;
    let top = bounded(margins.top_pt, 72.0, 0.0, 14_400.0, "top margin", &mut warnings);
    let bottom = bounded(margins.bottom_pt, 72.0, 0.0, 14_400.0, "bottom margin", &mut warnings);
    let mut left = bounded(margins.left_pt, 72.0, 0.0, 14_400.0, "left margin", &mut warnings);
    let mut right = bounded(margins.right_pt, 72.0, 0.0, 14_400.0, "right margin", &mut warnings);
    // Honour asymmetric margins whenever they fit. On a narrow poster reduce
    // them proportionally, preserving at least 72pt for actual document content.
    let available = width - 72.0;
    if left + right > available {
        left = available * (left / (left + right));
        right = available - left;
        adjusted(&mut warnings, "horizontal margins reduced to retain 72pt of content");
    }
    Geometry { width, left, right, top, bottom, scale, line_height,
        table_pad_x, table_pad_y, warnings }
}

fn adjusted(warnings: &mut Vec<SvgWarning>, message: &str) {
    warnings.push(SvgWarning { code: "svg_layout_adjusted", message: message.to_owned() });
}

fn bounded(value: f32, fallback: f32, min: f32, max: f32, label: &str,
    warnings: &mut Vec<SvgWarning>) -> f64
{
    if value.is_finite() && (min..=max).contains(&value) {
        f64::from(value)
    } else {
        adjusted(warnings, &format!("invalid {label}; using {fallback}"));
        f64::from(fallback)
    }
}
