//! Native book typography admission and bounded host-font loading.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{BookArgs, BookTarget, Failure, error};
use crate::cli::host_fonts::{HostFontError, HostFonts};
use crate::{FontAssetSlot, FontAssets, FontScale, RenderWarning, Theme};

/// Parse during argument admission, before configuration or source/asset I/O.
/// Keep aliases and normalization identical to the single-document renderer.
pub(super) fn parse_authoring_profile(value: &str) -> Result<crate::Profile, String> {
    crate::Profile::parse(value).ok_or_else(|| {
        format!(
            "unknown markdown authoring profile: {value:?}. Valid choices: 'commonmark-gfm', 'gfm-plus'"
        )
    })
}

pub(super) struct Typography {
    pub theme: Theme,
    pub font_scale: Option<FontScale>,
    fonts: HostFonts,
}

/// Validate all explicit option values before reading chapter or font files.
pub(super) fn prepare(
    args: &BookArgs,
    mut theme: Theme,
    defaults: &crate::config::RenderDefaults,
) -> Result<Typography, Failure> {
    let margins = [
        args.margin_top_pt,
        args.margin_right_pt,
        args.margin_bottom_pt,
        args.margin_left_pt,
    ];
    if !matches!(args.to, BookTarget::Pdf | BookTarget::Both)
        && (args.page_size.is_some()
            || margins.iter().any(Option::is_some)
            || args.pdf_line_numbers
            || args.pdf_baseline_grid_pt.is_some())
    {
        return Err(error(
            64,
            "unsupported_target_option",
            "page size, margins, --pdf-line-numbers and --pdf-baseline-grid-pt require --to pdf or --to both",
        ));
    }
    let font_scale = args.font_scale.as_deref().or(defaults.font_scale.as_deref()).map(|value| {
        FontScale::parse(value).ok_or_else(|| error(64, "usage_error",
            format!("invalid --font-scale {value:?}; use a preset such as lg, a percentage such as 125%, or a positive multiplier")))
    }).transpose()?;
    if let Some(scale) = font_scale {
        theme = theme.with_font_scale(scale);
    }
    if matches!(args.to, BookTarget::Pdf | BookTarget::Both) {
        configure_page(&mut theme, args.page_size.as_deref(), margins)?;
        crate::config::page::configure_baseline_grid(
            &mut theme,
            args.pdf_baseline_grid_pt,
            defaults.pdf_baseline_grid_pt.as_deref(),
            args.publishing.fit_to_pages,
        )
        .map_err(|err| error(64, err.code(), err.to_string()))?;
    }
    let fonts = HostFonts::parse(
        &args.pdf_fonts,
        &args.pdf_font_weights,
        &args.pdf_font_faces,
    )
    .map_err(|message| error(64, "usage_error", message))?;
    Ok(Typography {
        theme,
        font_scale,
        fonts,
    })
}

fn configure_page(
    theme: &mut Theme,
    selected: Option<&str>,
    margins: [Option<f64>; 4],
) -> Result<(), Failure> {
    crate::config::page::configure(theme, selected, margins)
        .map_err(|message| error(64, "invalid_page", message))
}

impl Typography {
    pub(super) fn load_fonts(
        &self,
        protected: &mut BTreeSet<PathBuf>,
    ) -> Result<FontAssets, Failure> {
        let mut paths = Vec::new();
        let assets = self
            .fonts
            .load(&mut paths)
            .map_err(|failure| match failure {
                HostFontError::Usage(message) => error(64, "usage_error", message),
                HostFontError::Input(message) => error(66, "font_error", message),
            })?;
        protected.extend(paths);
        Ok(assets)
    }
}

pub(super) fn warning_text(warning: &RenderWarning) -> String {
    format!("{}: {}", warning.code(), warning.message())
}

/// HTML and EPUB do not have PDF's glyph/image fallbacks, but they still need
/// the shared host-font warning when a static face cannot honor a weight pin.
pub(super) fn font_warnings(assets: &FontAssets) -> Vec<String> {
    FontAssetSlot::ALL
        .into_iter()
        .filter_map(|slot| {
            let weight = assets.slot_weight(slot)?;
            let bytes = assets.resolved_bytes(slot)?;
            let instanced = crate::text::Font::parse(bytes.to_vec())
                .ok()
                .and_then(|font| font.instance(f32::from(weight)));
            instanced.is_none().then(|| {
                warning_text(&RenderWarning::FontWeightIgnoredStatic {
                    slot: slot.as_str().to_owned(),
                    weight,
                })
            })
        })
        .collect()
}
