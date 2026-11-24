//! Native PDF spacing options shared by publication and verification.

use clap::Args;

use crate::ThemeSpacing;

#[derive(Args, Clone, Copy)]
#[group(id = "pdf_spacing", multiple = true)]
pub(crate) struct PdfSpacingArgs {
    /// Apply theme body leading and table padding to PDF. =false overrides config.
    #[arg(long, num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pdf_theme_spacing: Option<bool>,
    /// PDF body and table leading as a font-size multiplier (1..=4). Enables theme spacing.
    #[arg(long, value_name = "MULTIPLIER", value_parser = line_height)]
    pdf_line_height: Option<f32>,
    /// PDF padding on each horizontal side of a table cell (0..=8 em). Enables theme spacing.
    #[arg(long, value_name = "EM", value_parser = table_padding)]
    pdf_table_cell_padding_x_em: Option<f32>,
    /// PDF padding above and below each table row (0..=8 em). Enables theme spacing.
    #[arg(long, value_name = "EM", value_parser = table_padding)]
    pdf_table_cell_padding_y_em: Option<f32>,
}

fn bounded_number(text: &str, min: f64, max: f64) -> Result<f32, String> {
    // Compare at input precision before converting to renderer coordinates.
    text.parse::<f64>()
        .ok()
        .filter(|value| (min..=max).contains(value))
        .map(|value| value as f32)
        .ok_or_else(|| format!("expected a finite number from {min} through {max}"))
}

fn line_height(text: &str) -> Result<f32, String> {
    bounded_number(text, 1.0, 4.0)
}

fn table_padding(text: &str) -> Result<f32, String> {
    bounded_number(text, 0.0, 8.0)
}

impl PdfSpacingArgs {
    fn has_values(&self) -> bool {
        self.pdf_line_height.is_some()
            || self.pdf_table_cell_padding_x_em.is_some()
            || self.pdf_table_cell_padding_y_em.is_some()
    }

    pub(crate) fn is_explicit(&self) -> bool {
        self.pdf_theme_spacing.is_some() || self.has_values()
    }

    pub(crate) fn enabled(&self, configured: Option<bool>) -> Result<bool, &'static str> {
        if self.has_values() {
            if self.pdf_theme_spacing == Some(false) {
                return Err(
                    "explicit PDF leading or padding conflicts with --pdf-theme-spacing=false",
                );
            }
            Ok(true)
        } else {
            Ok(self.pdf_theme_spacing.or(configured).unwrap_or(false))
        }
    }

    pub(crate) fn apply(&self, spacing: &mut ThemeSpacing) {
        if let Some(value) = self.pdf_line_height {
            spacing.line_height = value;
        }
        if let Some(value) = self.pdf_table_cell_padding_x_em {
            spacing.table_cell_padding_x_em = value;
        }
        if let Some(value) = self.pdf_table_cell_padding_y_em {
            spacing.table_cell_padding_y_em = value;
        }
    }
}
