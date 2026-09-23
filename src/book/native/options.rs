//! Native book typography admission and bounded host-font loading.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;

use super::{BookArgs, BookTarget, Failure, error};
use crate::{FontAssetSlot, FontAssets, FontScale, PageMargins, PageSize, RenderWarning, Theme};

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
    let (name, width, height) = match selected {
        Some(value) => page_size(value)?,
        None => (
            theme.page.size.name,
            f64::from(theme.page.size.width_pt),
            f64::from(theme.page.size.height_pt),
        ),
    };
    let defaults = theme.page.margins;
    let [top, right, bottom, left] = std::array::from_fn(|i| {
        margins[i].unwrap_or(f64::from(
            [
                defaults.top_pt,
                defaults.right_pt,
                defaults.bottom_pt,
                defaults.left_pt,
            ][i],
        ))
    });
    if [top, right, bottom, left]
        .iter()
        .any(|point| !point.is_finite() || !(0.0..=14_400.0).contains(point))
    {
        return Err(error(
            64,
            "invalid_page",
            "PDF margins must be finite values from 0 through 14400 points",
        ));
    }
    let [width32, height32, top32, right32, bottom32, left32] =
        [width, height, top, right, bottom, left].map(|point| point as f32);
    // Admit at host precision AND using the exact arithmetic used by the PDF
    // layout engine. Rounding must not admit an impossible content rectangle.
    if width - left - right < 72.0
        || height - top - bottom < 72.0
        || width32 - left32 - right32 < 72.0
        || height32 - top32 - bottom32 < 72.0
    {
        return Err(error(
            64,
            "invalid_page",
            "PDF margins must leave at least 72 points of content width and height",
        ));
    }
    theme.page.size = PageSize {
        name,
        width_pt: width32,
        height_pt: height32,
    };
    theme.page.margins = PageMargins {
        top_pt: top32,
        right_pt: right32,
        bottom_pt: bottom32,
        left_pt: left32,
    };
    Ok(())
}

fn page_size(value: &str) -> Result<(&'static str, f64, f64), Failure> {
    let value = value.trim().to_ascii_lowercase();
    let (name, width, height) =
        match value.as_str() {
            "letter" => ("letter", 612.0, 792.0),
            "a4" => ("a4", 210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
            "a5" => ("a5", 148.0 * 72.0 / 25.4, 210.0 * 72.0 / 25.4),
            "legal" => ("legal", 612.0, 1008.0),
            "tabloid" => ("tabloid", 792.0, 1224.0),
            _ => {
                let parsed = value.split_once('x').and_then(|(width, height)| {
                    Some((
                        width.trim().parse::<f64>().ok()?,
                        height.trim().parse::<f64>().ok()?,
                    ))
                });
                let (width, height) = parsed.ok_or_else(|| error(64, "invalid_page",
                "--page-size must be letter, a4, a5, legal, tabloid, or WIDTHxHEIGHT in points"))?;
                ("custom", width, height)
            }
        };
    if [width, height]
        .iter()
        .any(|point| !point.is_finite() || !(144.0..=14_400.0).contains(point))
    {
        return Err(error(
            64,
            "invalid_page",
            "PDF paper dimensions must be finite values from 144 through 14400 points",
        ));
    }
    Ok((name, width, height))
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
