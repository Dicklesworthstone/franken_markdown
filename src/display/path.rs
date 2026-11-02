//! Bounded, renderer-neutral filled and stroked paths. No SVG parser, font identity or
//! executable drawing callback crosses the display boundary.

use super::DisplayRect;
use crate::span::SourceSpan;

pub const MAX_DISPLAY_PATH_COMMANDS: usize = 65_536;
pub const MAX_DISPLAY_TOTAL_PATH_COMMANDS: usize = 262_144;
pub const MAX_DISPLAY_STROKE_DASHES: usize = 16;

/// Absolute logical coordinates, with a downward-positive y axis. Every
/// contour begins with MoveTo. Filled contours end with Close and use nonzero
/// winding; stroke-only contours may be open.
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

/// A bounded explicit stroke in the path's logical coordinate units.
/// Odd dash lists repeat as in SVG/Canvas; an empty list is solid.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayPathStroke {
    pub color: [f32; 4],
    pub width: f32,
    pub dash: Vec<f32>,
    pub dash_offset: f32,
    pub cap: DisplayLineCap,
    pub join: DisplayLineJoin,
    pub miter_limit: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayLineCap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayLineJoin {
    Miter,
    Round,
    Bevel,
}
impl DisplayPathStroke {
    // Inflate every control point for miters, square caps and dashed caps.
    fn hull_padding(&self) -> Option<f32> {
        if !self.width.is_finite()
            || self.width <= 0.0
            || self.width > 1_000_000.0
            || !self.dash_offset.is_finite()
            || self.dash_offset.abs() > 1_000_000_000_000.0
            || !self.miter_limit.is_finite()
            || !(1.0..=1_000_000.0).contains(&self.miter_limit)
            || self
                .color
                .into_iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            || self.dash.len() > MAX_DISPLAY_STROKE_DASHES
            || self
                .dash
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1_000_000.0).contains(v))
            || (!self.dash.is_empty() && !self.dash.iter().any(|v| *v > 0.0))
        {
            return None;
        }
        let join = if self.join == DisplayLineJoin::Miter {
            self.miter_limit
        } else {
            1.0
        };
        let cap = if self.cap == DisplayLineCap::Square {
            std::f32::consts::SQRT_2
        } else {
            1.0
        };
        let padding = self.width * 0.5 * join.max(cap);
        padding.is_finite().then_some(padding)
    }
}

/// One paint operation. Multiple contours preserve holes within a glyph;
/// separate items preserve ordering and overlap between differently colored
/// primitives. Bounds include every endpoint and Bezier control point plus
/// the conservative stroke cap/join inflation.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayPath {
    pub bounds: DisplayRect,
    pub commands: Vec<DisplayPathCommand>,
    pub color_role: String,
    /// Explicit RGBA channels. Without a stroke, None uses the semantic
    /// palette fill; with a stroke, None means stroke-only paint.
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<DisplayPathStroke>,
    /// An enclosing source construct, never an invented glyph caret range.
    pub source_span: SourceSpan,
}

impl DisplayPath {
    /// Validate the complete control hull and bounded contour grammar before
    /// exposing geometry to indexing, serialization or a drawing backend.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let b = self.bounds;
        let stroke_padding = match &self.stroke {
            Some(stroke) => match stroke.hull_padding() {
                Some(padding) => padding,
                None => return false,
            },
            None => 0.0,
        };
        let filled = self.stroke.is_none() || self.fill.is_some();
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
                && x >= b.x + stroke_padding - epsilon
                && x <= b.right() - stroke_padding + epsilon
                && y >= b.y + stroke_padding - epsilon
                && y <= b.bottom() - stroke_padding + epsilon
        };
        let mut open = false;
        let mut drawn = false;
        for command in &self.commands {
            match *command {
                DisplayPathCommand::MoveTo { x, y } => {
                    if (open && (filled || !drawn)) || !point(x, y) {
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
        !open || (!filled && drawn)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
            stroke: None,
            source_span: SourceSpan::new(0, 10),
        }
    }

    fn stroked_line() -> DisplayPath {
        DisplayPath {
            bounds: DisplayRect::new(2.0, 2.0, 96.0, 16.0),
            commands: vec![
                DisplayPathCommand::MoveTo { x: 10.0, y: 10.0 },
                DisplayPathCommand::LineTo { x: 90.0, y: 10.0 },
            ],
            color_role: "diagram-edge".into(),
            fill: None,
            stroke: Some(DisplayPathStroke {
                color: [0.2, 0.3, 0.4, 1.0],
                width: 4.0,
                dash: vec![4.0, 3.0],
                dash_offset: 1.0,
                cap: DisplayLineCap::Butt,
                join: DisplayLineJoin::Miter,
                miter_limit: 4.0,
            }),
            source_span: SourceSpan::new(10, 30),
        }
    }

    #[test]
    fn open_strokes_keep_ink_in_viewport_and_source_hits() {
        let path = stroked_line();
        assert!(path.is_valid());
        let mut list = DisplayList::new();
        list.push_item(DisplayItem::Path(path));
        assert!(
            list.viewport_page(
                DisplayRect::new(9.0, 8.0, 2.0, 4.0),
                0,
                10,
                DisplayQueryMode::Visible,
            )
            .is_ok()
        );
        assert!(matches!(
            list.hit_test(10.0, 9.0),
            Some(DisplayItem::Path(_))
        ));
        let mut too_small = stroked_line();
        too_small.bounds = DisplayRect::new(10.0, 10.0, 80.0, 0.0);
        assert!(
            !too_small.is_valid(),
            "centerline-only bounds lose stroke ink"
        );
        let mut escaped_control = stroked_line();
        escaped_control.commands[1] = DisplayPathCommand::CubicTo {
            cx1: 30.0,
            cy1: -2.0,
            cx2: 70.0,
            cy2: 10.0,
            x: 90.0,
            y: 10.0,
        };
        assert!(!escaped_control.is_valid());
    }

    #[test]
    fn closed_fills_and_separate_open_contours_keep_distinct_grammar() {
        let mut open = stroked_line();
        open.commands.extend([
            DisplayPathCommand::MoveTo { x: 20.0, y: 10.0 },
            DisplayPathCommand::LineTo { x: 80.0, y: 10.0 },
        ]);
        assert!(
            open.is_valid(),
            "crossed arrows and actors retain separate strokes"
        );
        open.commands
            .push(DisplayPathCommand::MoveTo { x: 40.0, y: 10.0 });
        assert!(!open.is_valid(), "empty terminal subpath");
        let mut filled = stroked_line();
        filled.fill = Some([1.0; 4]);
        assert!(
            !filled.is_valid(),
            "fills still need explicit closed contours"
        );
        filled.commands.push(DisplayPathCommand::Close);
        assert!(filled.is_valid());
        filled.stroke = None;
        assert!(filled.is_valid(), "legacy closed fill remains admitted");
        filled.commands.pop();
        assert!(
            !filled.is_valid(),
            "legacy fill cannot become an implicit open path"
        );
    }

    #[test]
    fn invalid_strokes_cannot_reach_drawing_or_indexing() {
        let template = stroked_line().stroke.unwrap();
        let mut bad = Vec::new();
        for width in [0.0, -1.0, f32::INFINITY, 1_000_001.0] {
            bad.push(DisplayPathStroke {
                width,
                ..template.clone()
            });
        }
        for dash in [
            vec![0.0, 0.0],
            vec![1.0, -1.0],
            vec![f32::NAN],
            vec![1.0; MAX_DISPLAY_STROKE_DASHES + 1],
            vec![1_000_001.0],
        ] {
            bad.push(DisplayPathStroke {
                dash,
                ..template.clone()
            });
        }
        for dash_offset in [
            f32::NAN,
            f32::INFINITY,
            2_000_000_000_000.0,
            -2_000_000_000_000.0,
        ] {
            bad.push(DisplayPathStroke {
                dash_offset,
                ..template.clone()
            });
        }
        for miter_limit in [0.5, 1_000_001.0] {
            bad.push(DisplayPathStroke {
                miter_limit,
                ..template.clone()
            });
        }
        bad.push(DisplayPathStroke {
            color: [0.0, 0.0, 0.0, 1.1],
            ..template
        });
        for stroke in bad {
            let mut path = stroked_line();
            path.stroke = Some(stroke);
            assert!(!path.is_valid());
            let mut list = DisplayList::new();
            list.push_item(DisplayItem::Path(path));
            assert!(matches!(
                list.viewport_page(
                    DisplayRect::new(0.0, 0.0, 100.0, 100.0),
                    0,
                    10,
                    DisplayQueryMode::Visible,
                ),
                Err(DisplayQueryError::InvalidPath { index: 0 })
            ));
            assert!(list.hit_test(10.0, 10.0).is_none());
        }
    }

    #[test]
    fn caps_and_miters_must_fit_the_declared_control_hull() {
        let mut path = stroked_line();
        path.bounds = DisplayRect::new(8.0, 8.0, 84.0, 4.0);
        let stroke = path.stroke.as_mut().unwrap();
        stroke.join = DisplayLineJoin::Round;
        stroke.cap = DisplayLineCap::Round;
        assert!(path.is_valid());
        path.stroke.as_mut().unwrap().cap = DisplayLineCap::Square;
        assert!(
            !path.is_valid(),
            "square corners exceed a round half-width halo"
        );
        path.bounds = DisplayRect::new(7.0, 7.0, 86.0, 6.0);
        assert!(path.is_valid());
        path.stroke.as_mut().unwrap().join = DisplayLineJoin::Miter;
        assert!(!path.is_valid(), "a miter needs its configured outer bound");
        path.bounds = stroked_line().bounds;
        assert!(path.is_valid());
        path.stroke.as_mut().unwrap().dash = vec![2.0, 0.0, 3.0];
        assert!(
            path.is_valid(),
            "bounded odd patterns retain SVG/Canvas semantics"
        );
        for offset in [-1_000_000_000_000.0, 1_000_000_000_000.0] {
            path.stroke.as_mut().unwrap().dash_offset = offset;
            assert!(path.is_valid());
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
