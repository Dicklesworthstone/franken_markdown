//! Bounded, renderer-neutral filled paths. No SVG parser, font identity or
//! executable drawing callback crosses the display boundary.

use super::DisplayRect;
use crate::span::SourceSpan;

pub const MAX_DISPLAY_PATH_COMMANDS: usize = 65_536;
pub const MAX_DISPLAY_TOTAL_PATH_COMMANDS: usize = 262_144;

/// Absolute logical coordinates, with a downward-positive y axis. Every
/// contour begins with MoveTo and ends with Close; fills use nonzero winding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayPathCommand {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    QuadTo {
        cx: f32,
        cy: f32,
        x: f32,
        y: f32,
    },
    CubicTo {
        cx1: f32,
        cy1: f32,
        cx2: f32,
        cy2: f32,
        x: f32,
        y: f32,
    },
    Close,
}

/// One paint operation. Multiple contours preserve holes within a glyph;
/// separate items preserve ordering and overlap between differently colored
/// primitives. Bounds include every endpoint and Bezier control point.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayPath {
    pub bounds: DisplayRect,
    pub commands: Vec<DisplayPathCommand>,
    pub color_role: String,
    /// Explicit RGBA channels in 0..=1, or the host's semantic palette role.
    pub fill: Option<[f32; 4]>,
    /// An enclosing source construct, never an invented glyph caret range.
    pub source_span: SourceSpan,
}

impl DisplayPath {
    /// Validate the complete control hull and bounded contour grammar before
    /// exposing geometry to indexing, serialization or a drawing backend.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let b = self.bounds;
        if self.commands.is_empty()
            || self.commands.len() > MAX_DISPLAY_PATH_COMMANDS
            || ![b.x, b.y, b.width, b.height, b.right(), b.bottom()]
                .into_iter()
                .all(f32::is_finite)
            || b.width < 0.0
            || b.height < 0.0
            || self.source_span.start > self.source_span.end
            || self.fill.is_some_and(|rgba| {
                rgba.into_iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            })
        {
            return false;
        }
        // Bounds and commands originate in f32 layout. Allow only the rounding
        // needed to reconstruct a right/bottom edge from origin plus extent.
        let epsilon = 0.00001
            + 4.0
                * f32::EPSILON
                * b.x
                    .abs()
                    .max(b.y.abs())
                    .max(b.right().abs())
                    .max(b.bottom().abs())
                    .max(1.0);
        let point = |x: f32, y: f32| {
            x.is_finite()
                && y.is_finite()
                && x >= b.x - epsilon
                && x <= b.right() + epsilon
                && y >= b.y - epsilon
                && y <= b.bottom() + epsilon
        };
        let mut open = false;
        let mut drawn = false;
        for command in &self.commands {
            match *command {
                DisplayPathCommand::MoveTo { x, y } => {
                    if open || !point(x, y) {
                        return false;
                    }
                    open = true;
                    drawn = false;
                }
                DisplayPathCommand::LineTo { x, y } => {
                    if !open || !point(x, y) {
                        return false;
                    }
                    drawn = true;
                }
                DisplayPathCommand::QuadTo { cx, cy, x, y } => {
                    if !open || !point(cx, cy) || !point(x, y) {
                        return false;
                    }
                    drawn = true;
                }
                DisplayPathCommand::CubicTo {
                    cx1,
                    cy1,
                    cx2,
                    cy2,
                    x,
                    y,
                } => {
                    if !open || !point(cx1, cy1) || !point(cx2, cy2) || !point(x, y) {
                        return false;
                    }
                    drawn = true;
                }
                DisplayPathCommand::Close => {
                    if !open || !drawn {
                        return false;
                    }
                    open = false;
                }
            }
        }
        !open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{DisplayItem, DisplayList, DisplayQueryError, DisplayQueryMode};

    fn square() -> DisplayPath {
        DisplayPath {
            bounds: DisplayRect::new(10.0, 20.0, 20.0, 20.0),
            commands: vec![
                DisplayPathCommand::MoveTo { x: 10.0, y: 20.0 },
                DisplayPathCommand::LineTo { x: 30.0, y: 20.0 },
                DisplayPathCommand::LineTo { x: 30.0, y: 40.0 },
                DisplayPathCommand::LineTo { x: 10.0, y: 40.0 },
                DisplayPathCommand::Close,
            ],
            color_role: "math".into(),
            fill: None,
            source_span: SourceSpan::new(0, 10),
        }
    }

    #[test]
    fn invalid_paths_cannot_enter_a_viewport_or_hit_result() {
        let mut controls_escape = square();
        controls_escape.commands[1] = DisplayPathCommand::QuadTo {
            cx: 1000.0,
            cy: 30.0,
            x: 30.0,
            y: 20.0,
        };
        let mut nonfinite = square();
        nonfinite.commands[1] = DisplayPathCommand::LineTo {
            x: f32::NAN,
            y: 20.0,
        };
        let mut open = square();
        open.commands.pop();
        let mut color = square();
        color.fill = Some([1.0, 0.0, 0.0, 2.0]);
        for path in [controls_escape, nonfinite, open, color] {
            let mut list = DisplayList::new();
            list.push_item(DisplayItem::Path(path));
            assert!(matches!(
                list.viewport_page(
                    DisplayRect::new(0.0, 0.0, 100.0, 100.0),
                    0,
                    10,
                    DisplayQueryMode::Visible
                ),
                Err(DisplayQueryError::InvalidPath { index: 0 })
            ));
            assert!(list.hit_test(15.0, 25.0).is_none());
        }
        let mut valid = DisplayList::new();
        valid.push_item(DisplayItem::Path(square()));
        assert!(matches!(
            valid.hit_test(15.0, 25.0),
            Some(DisplayItem::Path(_))
        ));
    }

    #[test]
    fn path_budget_counts_geometry_across_the_whole_display_list() {
        let mut list = DisplayList::new();
        let mut path = square();
        let contour = path.commands.clone();
        path.commands = contour.repeat(MAX_DISPLAY_PATH_COMMANDS / contour.len());
        assert!(path.is_valid());
        for _ in 0..5 {
            list.push_item(DisplayItem::Path(path.clone()));
        }
        assert!(matches!(
            list.viewport_page(
                DisplayRect::new(0.0, 0.0, 100.0, 100.0),
                0,
                1,
                DisplayQueryMode::Visible
            ),
            Err(DisplayQueryError::PathBudgetExceeded)
        ));
    }
}
