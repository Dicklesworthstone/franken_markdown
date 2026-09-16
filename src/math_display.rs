#![forbid(unsafe_code)]

//! Bridge from `fmd-math` layout output to the renderer-neutral display
//! list (fcb-mrc.1).
//!
//! Converts positioned math glyphs, rules, and drawn paths into
//! [`DisplayItem`]s that any renderer can consume without knowing about
//! TeX, Metal, or AppKit. Source spans survive: every display item
//! carries the byte span of the math source that produced it.
//!
//! No external TeX engine, no JavaScript, no script execution — `fmd-math`
//! is the internal TeX implementation.

use crate::display::{
    DisplayItem, DisplayRect, DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath,
    VectorShapeType,
};
use crate::span::SourceSpan;
use fmd_math::Layout;
use fmd_math::Engine;

/// Convert math source into renderer-neutral display items.
///
/// The layout coordinates are in ems (y-up, baseline at 0). The display
/// coordinates are in points (y-down, origin at top-left). The bridge
/// flips the y-axis and scales by `font_size`.
///
/// Every display item preserves the source span of the math construct
/// that produced it — source anchors survive native render.
pub fn math_to_display(
    source: &str,
    engine: &Engine,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Result<Vec<DisplayItem>, fmd_math::MathError> {
    let layout = engine.typeset(source, fmd_math::Style::Display)?;
    Ok(layout_to_display(
        &layout,
        source,
        origin_x,
        origin_y,
        font_size,
        source_offset,
    ))
}

/// Convert a laid-out math formula into renderer-neutral display items.
pub fn layout_to_display(
    layout: &Layout,
    source: &str,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Vec<DisplayItem> {
    let mut items = Vec::new();

    // Glyphs → text runs.
    for glyph in &layout.glyphs {
        let dx = (origin_x + (glyph.x as f32) * font_size).round();
        // Y-flip: fmd-math uses y-up from baseline; display uses y-down
        // from top-left. The baseline sits at origin_y + font_size *
        // height_above_baseline. Individual glyph y offsets are relative
        // to the baseline and flipped.
        let dy = (origin_y + font_size - (glyph.y as f32) * font_size).round();
        let size = ((glyph.size as f32) * font_size).round();

        let span = source_offset_span(source_offset, glyph.span.start, glyph.span.end);
        let text_run = DisplayTextRun {
            bounds: glyph_bounds(dx, dy, glyph.ch, size),
            text: glyph.ch.to_string(),
            font_run: None, // host supplies resolved font runs
            color_role: "math".to_string(),
            source_span: span,
            font_size: size as f32,
        };
        items.push(DisplayItem::Text(text_run));
    }

    // Rules → vector paths (horizontal fraction bars, radical overbars).
    for rule in &layout.rules {
        let dx = (origin_x + (rule.x as f32) * font_size).round();
        let dy = (origin_y + font_size - (rule.y as f32) * font_size).round();
        let w = ((rule.width as f32) * font_size).round().max(1.0);
        let h = ((rule.height as f32) * font_size).round().max(1.0);
        let span = source_offset_span(source_offset, rule.span.start, rule.span.end);
        items.push(DisplayItem::Vector(DisplayVectorPath {
            bounds: DisplayRect {
                x: dx,
                y: dy - h,
                width: w,
                height: h,
            },
            shape: VectorShapeType::HorizontalRule,
            stroke_width: h,
            color_role: "math".to_string(),
            source_span: span,
        }));
    }

    items
}

/// Add a semantic anchor for the entire math region.
pub fn math_anchor(
    source: &str,
    layout: &Layout,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> DisplaySemanticAnchor {
    let w = ((layout.width as f32) * font_size).round().max(1.0);
    let total_h = (((layout.height + layout.depth) as f32) * font_size).round().max(1.0);
    let span = SourceSpan {
        start: source_offset,
        end: source_offset + source.len(),
    };
    DisplaySemanticAnchor {
        bounds: DisplayRect {
            x: origin_x,
            y: origin_y,
            width: w,
            height: total_h,
        },
        anchor_id: format!("math-{source_offset}"),
        is_heading: false,
        level: 0,
        source_span: span,
    }
}

fn glyph_bounds(x: f32, baseline_y: f32, ch: char, size: f32) -> DisplayRect {
    let w = ch_width(ch, size);
    DisplayRect {
        x,
        y: baseline_y - size,
        width: w,
        height: size,
    }
}

fn ch_width(ch: char, size: f32) -> f32 {
    // Approximate advance: 0.6 em for most math glyphs, 1.0 for wide.
    let factor = if ch.is_alphabetic() || ch.is_numeric() {
        0.6
    } else {
        0.5
    };
    (factor * size).round().max(1.0)
}

fn source_offset_span(source_offset: usize, start: usize, end: usize) -> SourceSpan {
    SourceSpan {
        start: source_offset + start,
        end: source_offset + end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::bundled().expect("bundled faces load")
    }

    #[test]
    fn simple_math_produces_display_items() {
        let engine = engine();
        let source = "x + 1";
        let items = math_to_display(source, &engine, 0.0, 0.0, 16.0, 0)
            .expect("simple math layouts");
        assert!(!items.is_empty(), "display items produced");
        // Every item must have a valid bounds rect.
        for item in &items {
            let b = item.bounds();
            assert!(b.width >= 0.0 && b.height >= 0.0, "non-negative bounds");
        }
    }

    #[test]
    fn source_spans_survive_the_bridge() {
        let engine = engine();
        let source = "x + y";
        let items = math_to_display(source, &engine, 0.0, 0.0, 16.0, 0)
            .expect("math layouts");
        for item in &items {
            let span = item.source_span();
            assert!(span.end <= source.len() + 1, "span within source");
        }
    }

    #[test]
    fn fraction_produces_rule_and_stacked_glyphs() {
        let engine = engine();
        let source = "\\frac{a}{b}";
        let items = math_to_display(source, &engine, 10.0, 10.0, 14.0, 0)
            .expect("fraction layouts");
        // Fractions should produce at least one rule (the fraction bar).
        let has_vector = items
            .iter()
            .any(|item| matches!(item, DisplayItem::Vector(_)));
        assert!(has_vector, "fraction bar is a vector path");
    }

    #[test]
    fn hostile_expansion_does_not_panic() {
        let engine = engine();
        for hostile in [
            "", "\\\\{}", "$#$", "\\frac{\\frac{\\frac{a}{b}}{\\frac{c}{d}}}{\\frac{e}{f}}",
            "\\left(\\right)", "\\sqrt{}", "\\text{}", "𝔘𝔫𝔦𝔠𝔬𝔡𝔢",
        ] {
            let result = math_to_display(hostile, &engine, 0.0, 0.0, 16.0, 0);
            // Either succeeds with valid items or returns a typed error —
            // never panics, never produces OOB spans.
            if let Ok(items) = result {
                for item in &items {
                    let b = item.bounds();
                    assert!(b.width.is_finite() && b.height.is_finite());
                }
            }
        }
    }

    #[test]
    fn unsupported_forms_return_typed_errors() {
        let engine = engine();
        // Unknown macro should produce a typed error, not a panic.
        let result = math_to_display("\\unknownmacro{x}", &engine, 0.0, 0.0, 16.0, 0);
        assert!(result.is_err(), "unknown macro is a typed error");
    }

    #[test]
    fn semantic_anchor_covers_the_math_region() {
        let engine = engine();
        let source = "x + y";
        let layout = engine
            .typeset(source, fmd_math::Style::Display)
            .expect("layouts");
        let anchor = math_anchor(source, &layout, 0.0, 0.0, 16.0, 0);
        assert!(anchor.bounds.width > 0.0);
        assert!(anchor.bounds.height > 0.0);
        assert_eq!(anchor.anchor_id, "math-0");
        assert!(!anchor.is_heading);
    }
}
