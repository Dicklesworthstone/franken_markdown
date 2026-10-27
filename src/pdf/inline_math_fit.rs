//! A containing-width cap that follows inline math from measurement to paint.
//!
//! Cached formulas stay immutable and use their original TeX source as the
//! cache key. Only a token that needs fitting carries a private width cap;
//! ordinary formulas retain their historical advances and output bytes.

use std::sync::Arc;

use super::{F_DISPLAY_MATH, Line, RenderWarning, Tok, is_inline_math, math};

const MARKER: char = '\u{1e}';
const ESCAPED: &str = "\u{1e}S";
const FITTED: &str = "\u{1e}F";

/// Preserve arbitrary host-created math source, including our private marker.
pub(super) fn token_text(source: &str) -> String {
    if source.starts_with(MARKER) {
        format!("{ESCAPED}{source}")
    } else {
        source.to_owned()
    }
}

fn parts(text: &str) -> (&str, Option<f32>) {
    if let Some(source) = text.strip_prefix(ESCAPED) {
        return (source, None);
    }
    if let Some(rest) = text.strip_prefix(FITTED)
        && let Some((cap, source)) = rest.split_once(':')
        && cap.len() == 8
        && let Ok(bits) = u32::from_str_radix(cap, 16)
    {
        let width = f32::from_bits(bits);
        if width.is_finite() && width > 0.0 {
            return (source, Some(width));
        }
    }
    (text, None)
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
    let (source, cap) = parts(text);
    let formula = math::inline_formula(source, display).ok()?;
    let (size, x_offset, advance, left, right) = if let Some(cap) = cap {
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
        fitted: cap.is_some(),
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
    let (source, cap) = parts(&tok.text);
    let formula = math::inline_formula(source, tok.slot == F_DISPLAY_MATH).ok()?;
    let width = (formula.right - formula.left) * size;
    if width <= max_width || cap.is_some_and(|cap| cap <= max_width) {
        return None;
    }
    let mut fitted = tok.clone();
    fitted.text = format!("{FITTED}{:08x}:{source}", max_width.to_bits());
    Some(fitted)
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
            let (source, Some(cap)) = parts(&seg.text) else {
                continue;
            };
            let Ok(formula) = math::inline_formula(source, seg.slot == F_DISPLAY_MATH) else {
                continue;
            };
            let width = (formula.right - formula.left) * line.size;
            if width <= cap {
                continue;
            }
            let warning = RenderWarning::MathOverflow {
                source: source.chars().take(120).collect(),
                width_pt: width.ceil() as u32,
                measure_pt: cap.floor() as u32,
            };
            if !warnings.contains(&warning) {
                warnings.push(warning);
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
        for source in ["x^2", "\u{1e}Sx", "\u{1e}F3f800000:x", "\u{1e}Fnot-hex:x"] {
            let text = token_text(source);
            assert_eq!(parts(&text), (source, None));
        }
    }
}
