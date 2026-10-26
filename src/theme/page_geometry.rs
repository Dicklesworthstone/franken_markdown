//! Paper definitions and precision-safe geometry shared by every PDF adapter.

use super::{PageMargins, PageSize, PageStyle};

pub(crate) const MIN_PAPER: f64 = 144.0;
pub(crate) const MAX_POINT: f64 = 14_400.0;
const MIN_CONTENT: f64 = 72.0;

/// Keep standard-paper dimensions at host precision until all margins have
/// been admitted. Rounding A-series dimensions first can admit an undersized
/// content rectangle at a boundary.
pub(crate) const NAMED_PAPER_SIZES: [(&str, [f64; 2]); 5] = [
    ("letter", [612.0, 792.0]),
    ("a4", [210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4]),
    ("a5", [148.0 * 72.0 / 25.4, 210.0 * 72.0 / 25.4]),
    ("legal", [612.0, 1008.0]),
    ("tabloid", [792.0, 1224.0]),
];

pub(crate) fn named_paper(name: &str) -> Option<(&'static str, [f64; 2])> {
    NAMED_PAPER_SIZES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .copied()
}

impl PageSize {
    /// Resolve a standard paper name: `letter`, `a4`, `a5`, `legal`, or `tabloid`.
    /// Dimensions are in PDF points; all named sizes are portrait.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        let (name, [width, height]) = named_paper(name)?;
        Some(Self {
            name,
            width_pt: width as f32,
            height_pt: height as f32,
        })
    }
}

impl PageStyle {
    /// Construct PDF paper and margins from point values at host precision.
    ///
    /// `size` is `[width, height]`; `margins` is `[top, right, bottom, left]`.
    /// The supplied order determines orientation. This function does not rotate
    /// paper or margins, scale output, or consult native configuration.
    ///
    /// Native, MCP and browser adapters share this admission policy. Library
    /// callers can also use it before constructing their render options.
    ///
    /// # Errors
    /// Paper dimensions must be finite and in `144..=14400` points. Margins must
    /// be finite and in `0..=14400` points, and leave at least 72 points of content
    /// width and height both before and after conversion to renderer precision.
    pub fn from_points(size: [f64; 2], margins: [f64; 4]) -> Result<Self, &'static str> {
        if size
            .iter()
            .any(|value| !value.is_finite() || !(MIN_PAPER..=MAX_POINT).contains(value))
        {
            return Err("PDF paper dimensions must be finite values from 144 through 14400 points");
        }
        if margins
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=MAX_POINT).contains(value))
        {
            return Err("PDF margins must be finite values from 0 through 14400 points");
        }
        let [width, height] = size;
        let [top, right, bottom, left] = margins;
        let [width32, height32] = size.map(|value| value as f32);
        let [top32, right32, bottom32, left32] = margins.map(|value| value as f32);
        // Check host values AND the renderer's sequential f32 subtractions.
        // Neither a rounded input nor a final-only cast is sufficient.
        if width - left - right < MIN_CONTENT
            || height - top - bottom < MIN_CONTENT
            || width32 - left32 - right32 < MIN_CONTENT as f32
            || height32 - top32 - bottom32 < MIN_CONTENT as f32
        {
            return Err("PDF margins must leave at least 72 points of content width and height");
        }
        let size = if width32 == PageSize::LETTER.width_pt && height32 == PageSize::LETTER.height_pt
        {
            PageSize::LETTER
        } else {
            PageSize {
                name: "custom",
                width_pt: width32,
                height_pt: height32,
            }
        };
        Ok(Self {
            size,
            margins: PageMargins {
                top_pt: top32,
                right_pt: right32,
                bottom_pt: bottom32,
                left_pt: left32,
            },
            baseline_grid: None,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn named_paper_definitions_preserve_physical_dimensions() {
        for (name, points) in [
            ("letter", [612.0, 792.0]),
            ("a4", [210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4]),
            ("a5", [148.0 * 72.0 / 25.4, 210.0 * 72.0 / 25.4]),
            ("legal", [612.0, 1008.0]),
            ("tabloid", [792.0, 1224.0]),
        ] {
            let size = PageSize::named(name).unwrap();
            assert_eq!(size.name, name);
            assert_eq!([size.width_pt, size.height_pt], points.map(|p| p as f32));
        }
        assert!(PageSize::named("unknown").is_none());
    }

    #[test]
    fn default_and_landscape_preserve_geometry_and_margin_order() {
        assert_eq!(
            PageStyle::from_points([612.0, 792.0], [72.0; 4]).unwrap(),
            PageStyle::default(),
        );
        let page = PageStyle::from_points([792.0, 612.0], [18.0, 24.0, 30.0, 36.0]).unwrap();
        assert_eq!([page.size.width_pt, page.size.height_pt], [792.0, 612.0]);
        assert_eq!(
            page.margins,
            PageMargins {
                top_pt: 18.0,
                right_pt: 24.0,
                bottom_pt: 30.0,
                left_pt: 36.0,
            },
        );
    }

    #[test]
    fn admission_checks_both_host_and_renderer_precision() {
        assert!(PageStyle::from_points([144.0, 144.0], [36.0; 4]).is_ok());
        assert!(PageStyle::from_points([14400.0, 14400.0], [0.0; 4]).is_ok());
        // An impossible host rectangle must not become valid by rounding.
        assert!(PageStyle::from_points([144.0, 300.0], [0.0, 36.0000001, 0.0, 36.0]).is_err());
        // A valid f64 rectangle can still fail the actual f32 subtraction.
        assert!(
            PageStyle::from_points([144.00731624, 300.0], [0.0, 36.00155196, 0.0, 36.00576428],)
                .is_err()
        );
        for index in 0..6 {
            for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 14400.01] {
                let mut values = [612.0, 792.0, 72.0, 72.0, 72.0, 72.0];
                values[index] = bad;
                assert!(
                    PageStyle::from_points(
                        [values[0], values[1]],
                        [values[2], values[3], values[4], values[5]],
                    )
                    .is_err()
                );
            }
        }
        assert!(PageStyle::from_points([143.999999, 300.0], [0.0; 4]).is_err());
        assert!(PageStyle::from_points([300.0, 143.999999], [0.0; 4]).is_err());
    }
}
