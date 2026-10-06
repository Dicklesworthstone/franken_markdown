//! Transactional admission for the reusable book's PDF publishing options.
//! Validation builds a small candidate, never clones retained fonts or images.

use crate::wasm::WasmRenderOptions;
use crate::{PdfRunningBand, PdfRunningContent, RenderError, Result};

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidInput(message.into())
}

fn size(value: Option<f64>, name: &str, min: f64, max: f64) -> Result<Option<f32>> {
    value
        .map(|value| {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(invalid(format!("{name} must be finite and in {min}..={max}")));
            }
            Ok(value as f32)
        })
        .transpose()
}

fn integer(value: Option<f64>, name: &str, min: u64, max: u64) -> Result<Option<u64>> {
    value
        .map(|value| {
            if !value.is_finite()
                || value.fract() != 0.0
                || value < min as f64
                || value > max as f64
            {
                return Err(invalid(format!("{name} must be an integer in {min}..={max}")));
            }
            Ok(value as u64)
        })
        .transpose()
}

/// Validated, replace-all publishing settings. Presentation/asset/source state
/// outside this profile is not reset when it is committed.
pub(super) struct PdfSettings(WasmRenderOptions);

impl PdfSettings {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        typography: Option<&str>,
        base_font_size: Option<f64>,
        heading_scale: Option<f64>,
        table_font_size: Option<f64>,
        toc_depth: Option<f64>,
        fit_to_pages: Option<f64>,
        code_line_numbers: bool,
        metadata_epoch_seconds: Option<f64>,
        running_slots: Vec<String>,
        header_rule: bool,
        footer_rule: bool,
        skip_first_page: bool,
    ) -> Result<Self> {
        if typography.is_some_and(|text| text.len() > 256) {
            return Err(invalid("typography is limited to 256 UTF-8 bytes"));
        }
        let mut candidate = WasmRenderOptions::default();
        // The shared token parser can mutate before encountering an unknown
        // token. Run it on this candidate, NEVER the retained book options.
        candidate.apply_typography_tokens(typography).map_err(invalid)?;
        candidate.base_font_size = size(base_font_size, "baseFontSize", 6.0, 24.0)?;
        candidate.heading_scale = size(heading_scale, "headingScale", 1.05, 2.0)?;
        candidate.table_font_size = size(table_font_size, "tableFontSize", 5.0, 24.0)?;
        candidate.toc_depth = integer(toc_depth, "tocDepth", 1, 6)?.map(|n| n as u8);
        candidate.fit_to_pages = integer(fit_to_pages, "fitToPages", 1, u64::from(u32::MAX))?
            .map(|n| n as usize);
        candidate.code_line_numbers = code_line_numbers;
        candidate.metadata_epoch_seconds = integer(
            metadata_epoch_seconds,
            "metadataEpochSeconds",
            0,
            9_007_199_254_740_991,
        )?;
        if !running_slots.is_empty() && running_slots.len() != 6 {
            return Err(invalid("running slots must be empty or exactly six templates"));
        }
        if running_slots.iter().any(|slot| slot.len() > 4096) {
            return Err(invalid("each running slot is limited to 4096 UTF-8 bytes"));
        }
        let mut slots = running_slots.into_iter().map(|slot| {
            (!slot.is_empty()).then_some(slot)
        });
        let mut band = |rule| PdfRunningBand {
            left: slots.next().flatten(),
            center: slots.next().flatten(),
            right: slots.next().flatten(),
            rule,
        };
        candidate.running = PdfRunningContent {
            header: band(header_rule),
            footer: band(footer_rule),
            skip_first_page,
        };
        Ok(Self(candidate))
    }

    /// Infallible commit after all fields have been admitted. No asset copies,
    /// source changes, theme resets, or increment to the source revision.
    pub(super) fn apply(self, options: &mut WasmRenderOptions) {
        let candidate = self.0;
        options.gradual_demerits = candidate.gradual_demerits;
        options.river_penalty = candidate.river_penalty;
        options.pareto_line_breaking = candidate.pareto_line_breaking;
        options.optimal_pagination = candidate.optimal_pagination;
        options.microtype = candidate.microtype;
        options.base_font_size = candidate.base_font_size;
        options.heading_scale = candidate.heading_scale;
        options.table_font_size = candidate.table_font_size;
        options.toc_depth = candidate.toc_depth;
        options.fit_to_pages = candidate.fit_to_pages;
        options.code_line_numbers = candidate.code_line_numbers;
        options.metadata_epoch_seconds = candidate.metadata_epoch_seconds;
        options.running = candidate.running;
    }
}

#[cfg(test)]
#[path = "browser_render_options_tests.rs"]
mod tests;
