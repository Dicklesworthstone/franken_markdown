//! Containing-width and page-height caps shared by measurement and paint.
//!
//! Cached formulas stay immutable and use their original TeX source as the
//! cache key. Only a token that needs fitting carries private geometry caps;
//! ordinary formulas retain their historical advances and output bytes.

use std::sync::Arc;

use super::{
    F_DISPLAY_MATH, F_IMAGE, Faces, Line, RenderWarning, Tok, font_size_of, inline_image,
    is_inline_math, math, text_positioning,
};

const MARKER: char = '\u{1e}';
const ESCAPED: &str = "\u{1e}S";
const FITTED: &str = "\u{1e}F";
const VERTICAL: &str = "\u{1e}V";

#[derive(Clone, Copy, Debug, PartialEq)]
struct VerticalFit {
    size: f32,
    height: f32,
}

/// Preserve arbitrary host-created math source, including our private marker.
pub(super) fn token_text(source: &str) -> String {
    if source.starts_with(MARKER) {
        format!("{ESCAPED}{source}")
    } else {
        source.to_owned()
    }
}

fn parts(text: &str) -> (&str, Option<f32>, Option<VerticalFit>) {
    if let Some(source) = text.strip_prefix(ESCAPED) {
        return (source, None, None);
    }
    if let Some(rest) = text.strip_prefix(VERTICAL)
        && let Some((caps, source)) = rest.split_once(':')
        && caps.len() == 24
        && caps.is_ascii()
        && let (Ok(width), Ok(size), Ok(height)) = (
            u32::from_str_radix(&caps[..8], 16),
            u32::from_str_radix(&caps[8..16], 16),
            u32::from_str_radix(&caps[16..], 16),
        )
    {
        let (width, size, height) = (
            f32::from_bits(width),
            f32::from_bits(size),
            f32::from_bits(height),
        );
        if width.is_finite()
            && width >= 0.0
            && size.is_finite()
            && size > 0.0
            && height.is_finite()
            && height > 0.0
        {
            return (
                source,
                (width > 0.0).then_some(width),
                Some(VerticalFit { size, height }),
            );
        }
    }
    if let Some(rest) = text.strip_prefix(FITTED)
        && let Some((cap, source)) = rest.split_once(':')
        && cap.len() == 8
        && let Ok(bits) = u32::from_str_radix(cap, 16)
    {
        let width = f32::from_bits(bits);
        if width.is_finite() && width > 0.0 {
            return (source, Some(width), None);
        }
    }
    (text, None, None)
}

fn with_caps(source: &str, width: Option<f32>, vertical: Option<VerticalFit>) -> String {
    match vertical {
        Some(vertical) => format!(
            "{VERTICAL}{:08x}{:08x}{:08x}:{source}",
            width.unwrap_or(0.0).to_bits(),
            vertical.size.to_bits(),
            vertical.height.to_bits(),
        ),
        None => format!(
            "{FITTED}{:08x}:{source}",
            width.unwrap_or(f32::MAX).to_bits()
        ),
    }
}

pub(super) fn source(text: &str) -> &str {
    parts(text).0
}

/// Resolved geometry in points. Every consumer uses this same placement;
/// reducing only the advance would leave the actual paths outside the box.
pub(super) struct Placement<'a> {
    pub source: &'a str,
    pub formula: Arc<math::InlineFormula>,
    pub fitted: bool,
    pub size: f32,
    pub x_offset: f32,
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
    pub left: f32,
    pub right: f32,
}

impl Placement<'_> {
    pub fn bbox(&self, x: f32, baseline: f32, segment_width: f32) -> [f32; 4] {
        [
            x + self.left,
            baseline - self.descent,
            x + if self.fitted {
                self.right
            } else {
                segment_width
            },
            baseline + self.ascent,
        ]
    }
}

pub(super) fn placement(text: &str, nominal_size: f32, display: bool) -> Option<Placement<'_>> {
    let (source, cap, vertical) = parts(text);
    let formula = math::inline_formula(source, display).ok()?;
    let (size, x_offset, advance, left, right) = if cap.is_some() || vertical.is_some() {
        let cap = cap.unwrap_or(f32::MAX);
        let nominal_size = vertical.map_or(nominal_size, |fit| nominal_size.min(fit.size));
        let width = formula.right - formula.left;
        let mut size = if width > 0.0 {
            nominal_size.min(cap / width)
        } else {
            nominal_size
        };
        // Division may round upward by an ulp. Keep the actual painted width
        // inside the cap before the final two-decimal PDF serialization.
        if width * size > cap && size > 0.0 {
            // Positive finite f32 values follow their bit ordering (MSRV 1.85).
            size = f32::from_bits(size.to_bits() - 1);
        }
        let advance = width * size;
        (size, -formula.left * size, advance, 0.0, advance)
    } else {
        // Preserve ordinary formula placement and advances byte for byte.
        (
            nominal_size,
            0.0,
            formula.advance * nominal_size,
            0.0,
            formula.advance * nominal_size,
        )
    };
    Some(Placement {
        source,
        fitted: cap.is_some() || vertical.is_some(),
        ascent: formula.ascent * size,
        descent: formula.descent * size,
        formula,
        size,
        x_offset,
        advance,
        left,
        right,
    })
}

/// Fit only formulas whose advance or actual ink exceeds this local measure.
/// A previous cap can only tighten; ordinary cached source is never mutated.
pub(super) fn fitted_token(tok: &Tok, size: f32, max_width: f32) -> Option<Tok> {
    if !is_inline_math(tok.slot) || !max_width.is_finite() || max_width <= 0.0 {
        return None;
    }
    let (source, cap, vertical) = parts(&tok.text);
    let formula = math::inline_formula(source, tok.slot == F_DISPLAY_MATH).ok()?;
    let size = vertical.map_or(size, |fit| size.min(fit.size));
    let width = (formula.right - formula.left) * size;
    if width <= max_width || cap.is_some_and(|cap| cap <= max_width) {
        return None;
    }
    let mut fitted = tok.clone();
    fitted.text = with_caps(source, Some(max_width), vertical);
    Some(fitted)
}

/// Limit the combined above/below-baseline envelope of a paragraph or table
/// row, including positioned text ink and bounded inline images.
/// Independent per-formula caps are insufficient: a tall superscript and a
/// deep subscript can share one row. Smaller formulas keep their original size
/// when they already fit the shared envelope.
pub(super) fn fit_heights<'a>(
    tokens: impl IntoIterator<Item = &'a mut Tok>,
    size: f32,
    max_height: f32,
    faces: &Faces,
) {
    if !max_height.is_finite() || max_height <= 0.0 {
        return;
    }
    let mut formulas = Vec::new();
    let (mut above, mut below) = (0.0_f32, 0.0_f32);
    let (mut fixed_above, mut fixed_below) = (size, size * 0.3);
    for tok in tokens {
        if tok.slot == F_IMAGE {
            if let Some(image) = inline_image::image_box(&tok.text, size) {
                fixed_above = fixed_above.max(image.height - image.descent);
                fixed_below = fixed_below.max(image.descent);
            }
            continue;
        }
        if !is_inline_math(tok.slot) {
            // Pagination reserves actual positioned mark ink, which can rise
            // far above one em. Keep that same fixed envelope when a deep
            // formula shares its baseline; nominal text metrics alone would
            // let the later line-height adjustment push formula ink off-page.
            if let Some(run) = text_positioning::prepare(faces.face(tok.slot), &tok.text).run {
                let (above, below) = run.ink(font_size_of(size));
                fixed_above = fixed_above.max(above);
                fixed_below = fixed_below.max(below);
            }
            continue;
        }
        let Some(formula) = placement(&tok.text, size, tok.slot == F_DISPLAY_MATH) else {
            continue;
        };
        above = above.max(formula.ascent);
        below = below.max(formula.descent);
        let dimensions = (
            formula.formula.ascent,
            formula.formula.descent,
            formula.size,
        );
        formulas.push((tok, dimensions));
    }
    // For a row at the top of a page, the baseline is 1.32em below
    // the cursor, plus ascent beyond 1em. Its lowest ink therefore occupies
    // 0.32em + max(above, fixed_above) + max(below, fixed_below).
    let available_ink = (max_height - size * 0.32).max(0.0);
    if formulas.is_empty() || above.max(fixed_above) + below.max(fixed_below) <= available_ink {
        return;
    }
    let mut scale = available_ink / (above + below);
    if above > 0.0 {
        scale = scale.min((available_ink - fixed_below).max(0.0) / above);
    }
    if below > 0.0 {
        scale = scale.min((available_ink - fixed_above).max(0.0) / below);
    }
    let (above_cap, below_cap) = (above * scale, below * scale);
    for (tok, (ascent, descent, previous_size)) in formulas {
        let mut fitted_size = previous_size;
        if ascent > 0.0 {
            fitted_size = fitted_size.min(above_cap / ascent);
        }
        if descent > 0.0 {
            fitted_size = fitted_size.min(below_cap / descent);
        }
        if fitted_size >= previous_size {
            continue;
        }
        // Keep rounded multiplication inside both caps; all values are
        // positive and finite after bounded formula admission.
        if (ascent * fitted_size > above_cap || descent * fitted_size > below_cap)
            && fitted_size > 0.0
        {
            fitted_size = f32::from_bits(fitted_size.to_bits() - 1);
        }
        let (source, width, vertical) = parts(&tok.text);
        let fitted_size = vertical.map_or(fitted_size, |fit| fitted_size.min(fit.size));
        let height = (ascent + descent) * fitted_size;
        if fitted_size > 0.0 && height > 0.0 {
            tok.text = with_caps(
                source,
                width,
                Some(VerticalFit {
                    size: fitted_size,
                    height,
                }),
            );
        }
    }
}

pub(super) fn fit_tokens(tokens: &mut [Tok], size: f32, max_width: f32) {
    for token in tokens {
        if let Some(fitted) = fitted_token(token, size, max_width) {
            *token = fitted;
        }
    }
}

/// Report actual final local fits, including table font scaling and columns,
/// heading sizes, nested indentation, notes and page-budget fitting.
pub(super) fn append_warnings(lines: &[Line], warnings: &mut Vec<RenderWarning>) {
    for line in lines {
        for seg in line.segs.iter().filter(|seg| is_inline_math(seg.slot)) {
            let (source, cap, vertical) = parts(&seg.text);
            if cap.is_none() && vertical.is_none() {
                continue;
            }
            let Ok(formula) = math::inline_formula(source, seg.slot == F_DISPLAY_MATH) else {
                continue;
            };
            let width = (formula.right - formula.left) * line.size;
            if let Some(cap) = cap.filter(|&cap| width > cap) {
                let warning = RenderWarning::MathOverflow {
                    source: source.chars().take(120).collect(),
                    width_pt: width.ceil() as u32,
                    measure_pt: cap.floor() as u32,
                };
                if !warnings.contains(&warning) {
                    warnings.push(warning);
                }
            }
            if let Some(fit) = vertical.filter(|fit| line.size > fit.size) {
                let height = (formula.ascent + formula.descent) * line.size;
                let warning = RenderWarning::MathHeightOverflow {
                    source: source.chars().take(120).collect(),
                    height_pt: height.ceil() as u32,
                    measure_pt: fit.height.floor() as u32,
                };
                if !warnings.contains(&warning) {
                    warnings.push(warning);
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn private_marker_in_source_cannot_inject_a_fitted_token() {
        for source in [
            "x^2",
            "\u{1e}Sx",
            "\u{1e}F3f800000:x",
            "\u{1e}Fnot-hex:x",
            "\u{1e}V000000003f8000003f800000:x",
        ] {
            let text = token_text(source);
            assert_eq!(parts(&text), (source, None, None));
        }
    }

    #[test]
    fn ordinarily_sized_math_keeps_its_original_token_and_geometry() {
        let mut tokens = vec![Tok {
            text: token_text("x^2"),
            slot: super::super::F_MATH,
            space: false,
            hard_break: false,
            literal: false,
            link: None,
            strike: false,
        }];
        let before = placement(&tokens[0].text, 11.0, false).unwrap();
        let dimensions = (before.size, before.advance, before.ascent, before.descent);
        let faces = Faces::load(&crate::PdfOptions::default()).unwrap();
        fit_heights(&mut tokens, 11.0, 140.0, &faces);
        assert_eq!(tokens[0].text, "x^2");
        let after = placement(&tokens[0].text, 11.0, false).unwrap();
        assert_eq!(
            dimensions,
            (after.size, after.advance, after.ascent, after.descent)
        );
        assert!(!after.fitted);
    }
}
