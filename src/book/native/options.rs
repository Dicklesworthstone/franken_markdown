//! Native book typography admission and bounded host-font loading.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

use super::{BookArgs, BookTarget, Failure, error};
use crate::{FontAssetSlot, FontAssets, FontScale, RenderWarning, Theme};

pub(super) struct Typography {
    pub theme: Theme,
    pub font_scale: Option<FontScale>,
    fonts: Vec<(FontAssetSlot, PathBuf)>,
    weights: Vec<(FontAssetSlot, u16)>,
}

/// Validate all explicit option values before reading chapter or font files.
pub(super) fn prepare(args: &BookArgs, mut theme: Theme) -> Result<Typography, Failure> {
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
            || args.toc_depth.is_some())
    {
        return Err(error(
            64,
            "unsupported_target_option",
            "page size, margins, --pdf-line-numbers and --toc-depth require --to pdf or --to both",
        ));
    }
    let font_scale = args.font_scale.as_deref().map(|value| {
        FontScale::parse(value).ok_or_else(|| error(64, "usage_error",
            format!("invalid --font-scale {value:?}; use a preset such as lg, a percentage such as 125%, or a positive multiplier")))
    }).transpose()?;
    if let Some(scale) = font_scale {
        theme = theme.with_font_scale(scale);
    }
    if matches!(args.to, BookTarget::Pdf | BookTarget::Both) {
        configure_page(&mut theme, args.page_size.as_deref(), margins)?;
    }
    let fonts = args.pdf_fonts.iter().map(|spec| {
        let (slot, path) = spec.split_once('=').ok_or_else(|| error(64, "usage_error",
            format!("invalid --pdf-font {spec:?}; use SLOT=PATH, for example body-regular=./Body.ttf")))?;
        let slot = parse_slot(slot, "--pdf-font")?;
        let path = path.trim();
        if path.is_empty() { return Err(error(64, "usage_error", "--pdf-font PATH must not be blank")); }
        Ok((slot, PathBuf::from(path)))
    }).collect::<Result<Vec<_>, Failure>>()?;
    let weights = args
        .pdf_font_weights
        .iter()
        .map(|spec| {
            let (slot, value) = match spec.split_once('=') {
                Some((slot, value)) => (parse_slot(slot, "--pdf-font-weight")?, value),
                None => (FontAssetSlot::BodyRegular, spec.as_str()),
            };
            let weight = value
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|weight| (1..=1000).contains(weight))
                .ok_or_else(|| {
                    error(
                        64,
                        "usage_error",
                        "--pdf-font-weight must be an integer from 1 through 1000",
                    )
                })?;
            Ok((slot, weight))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(Typography {
        theme,
        font_scale,
        fonts,
        weights,
    })
}

fn parse_slot(value: &str, flag: &str) -> Result<FontAssetSlot, Failure> {
    FontAssetSlot::parse(value).ok_or_else(|| error(64, "usage_error", format!(
        "unknown {flag} slot {value:?}; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular")))
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
        let mut assets = FontAssets::default();
        let limit = crate::MAX_FONT_ASSET_BYTES as u64;
        for (slot, path) in &self.fonts {
            let label = format!("{} font from {}", slot.as_str(), path.display());
            let metadata = std::fs::metadata(path)
                .map_err(|e| error(66, "font_error", format!("reading {label}: {e}")))?;
            if !metadata.is_file() || metadata.len() > limit {
                return Err(error(
                    66,
                    "font_error",
                    format!("{label} must be a regular file of at most {limit} bytes"),
                ));
            }
            let canonical = path
                .canonicalize()
                .map_err(|e| error(66, "font_error", format!("resolving {label}: {e}")))?;
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
                .map_err(|e| error(66, "font_error", format!("reading {label}: {e}")))?;
            if bytes.len() as u64 > limit {
                return Err(error(
                    66,
                    "font_error",
                    format!("{label} exceeds the {limit}-byte font limit"),
                ));
            }
            assets
                .set_slot(*slot, bytes)
                .map_err(|e| error(66, "font_error", format!("{label}: {e}")))?;
            protected.insert(canonical);
        }
        for (slot, weight) in &self.weights {
            assets
                .set_slot_weight(*slot, *weight)
                .map_err(|e| error(64, "usage_error", e))?;
        }
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
