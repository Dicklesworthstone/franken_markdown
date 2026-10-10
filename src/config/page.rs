//! Shared native PDF paper selection and geometry admission.
//!
//! Retain host precision until the complete page has been validated, then
//! recheck the exact arithmetic used by the renderer before converting it.

use crate::{PageMargins, PageSize, Theme};

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
    let (name, width, height) = match value.as_str() {
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
            let (width, height) = parsed.ok_or_else(|| {
                "page size must be letter, a4, a5, legal, tabloid, or WIDTHxHEIGHT in points"
                    .to_string()
            })?;
            ("custom", width, height)
        }
    };
    if [width, height]
        .iter()
        .any(|point| !point.is_finite() || !(144.0..=14_400.0).contains(point))
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
    if [top, right, bottom, left]
        .iter()
        .any(|point| !point.is_finite() || !(0.0..=14_400.0).contains(point))
    {
        return Err("PDF margins must be finite values from 0 through 14400 points".to_string());
    }
    let size = paper.size();
    let [top32, right32, bottom32, left32] = [top, right, bottom, left].map(|point| point as f32);
    // Keep the book command's admission at both precisions. A decimal that
    // rounds onto a boundary must not admit an impossible content rectangle.
    if paper.width - left - right < 72.0
        || paper.height - top - bottom < 72.0
        || size.width_pt - left32 - right32 < 72.0
        || size.height_pt - top32 - bottom32 < 72.0
    {
        return Err(
            "PDF margins must leave at least 72 points of content width and height".to_string(),
        );
    }
    theme.page.size = size;
    theme.page.margins = PageMargins {
        top_pt: top32,
        right_pt: right32,
        bottom_pt: bottom32,
        left_pt: left32,
    };
    Ok(())
}
