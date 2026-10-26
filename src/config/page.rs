//! Shared native PDF paper selection and geometry admission.
//!
//! Retain host precision until the complete page has been validated, then
//! recheck the exact arithmetic used by the renderer before converting it.

use crate::theme::page_geometry::{MAX_POINT, MIN_PAPER, named_paper};
use crate::{PageSize, PageStyle, Theme};

pub(crate) struct Paper {
    name: &'static str,
    width: f64,
    height: f64,
}

impl Paper {
    pub(crate) fn size(&self) -> PageSize {
        PageSize {
            name: self.name,
            width_pt: self.width as f32,
            height_pt: self.height as f32,
        }
    }

    /// Custom dimensions must remain usable as a config value, rather than
    /// being serialized as the renderer's internal `custom` name.
    pub(crate) fn config_value(&self) -> String {
        if self.name == "custom" {
            format!("{}x{}", self.width, self.height)
        } else {
            self.name.to_string()
        }
    }
}

pub(crate) fn parse_size(value: &str) -> Result<Paper, String> {
    let value = value.trim().to_ascii_lowercase();
    let (name, width, height) = match named_paper(&value) {
        Some((name, [width, height])) => (name, width, height),
        None => {
            let parsed = value.split_once('x').and_then(|(width, height)| {
                Some((
                    width.trim().parse::<f64>().ok()?,
                    height.trim().parse::<f64>().ok()?,
                ))
            });
            let (width, height) = parsed.ok_or_else(|| {
                "page size must be letter, a4, a5, legal, tabloid, or WIDTHxHEIGHT in points"
                    .to_string()
            })?;
            ("custom", width, height)
        }
    };
    if [width, height]
        .iter()
        .any(|point| !point.is_finite() || !(MIN_PAPER..=MAX_POINT).contains(point))
    {
        return Err(
            "PDF paper dimensions must be finite values from 144 through 14400 points".to_string(),
        );
    }
    Ok(Paper {
        name,
        width,
        height,
    })
}

/// Apply explicit paper and margin overrides to an already configured theme.
/// Omitted sides retain their configured values. Validation is transactional:
/// a rejected page leaves the theme unchanged.
pub(crate) fn configure(
    theme: &mut Theme,
    selected: Option<&str>,
    margins: [Option<f64>; 4],
) -> Result<(), String> {
    let paper = match selected {
        Some(value) => parse_size(value)?,
        None => Paper {
            name: theme.page.size.name,
            width: f64::from(theme.page.size.width_pt),
            height: f64::from(theme.page.size.height_pt),
        },
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
    let mut page = PageStyle::from_points([paper.width, paper.height], [top, right, bottom, left])
        .map_err(str::to_string)?;
    // Native config preserves the selected spelling for round trips. Browser
    // and MCP requests use the same geometry without persisting a paper name.
    page.size.name = paper.name;
    page.baseline_grid = theme.page.baseline_grid;
    theme.page = page;
    Ok(())
}

/// Admit a persisted grid without losing host precision. Zero is the explicit
/// host spelling for disabling a configured grid; positive values use the core
/// constructor and its stable typed error.
pub(crate) fn parse_baseline_grid(value: &str) -> crate::Result<Option<crate::BaselineGrid>> {
    let points = value.trim().parse::<f64>().map_err(|_| {
        crate::RenderError::InvalidInput(
            "pdf_baseline_grid: spacing must be 0 (disabled) or a number from 1 through 144 points"
                .to_string(),
        )
    })?;
    baseline_grid(points)
}

fn baseline_grid(points: f64) -> crate::Result<Option<crate::BaselineGrid>> {
    if points == 0.0 {
        Ok(None)
    } else {
        crate::BaselineGrid::from_points(points).map(Some)
    }
}

/// Apply an explicit native flag before its persistent default. Call after
/// page admission so paper/margin reconstruction cannot discard the grid.
pub(crate) fn configure_baseline_grid(
    theme: &mut Theme,
    explicit: Option<f64>,
    configured: Option<&str>,
    fit_to_pages: Option<usize>,
) -> crate::Result<()> {
    let grid = match (explicit, configured) {
        (Some(points), _) => baseline_grid(points)?,
        (None, Some(value)) => parse_baseline_grid(value)?,
        (None, None) => theme.page.baseline_grid,
    };
    if grid.is_some() && fit_to_pages.is_some_and(|pages| pages > 0) {
        return Err(crate::RenderError::InvalidInput(
            "pdf_baseline_grid_incompatible: baseline grids cannot be combined with fit_to_pages; remove the page-fit target or disable the grid".into(),
        ));
    }
    theme.page.baseline_grid = grid;
    Ok(())
}
