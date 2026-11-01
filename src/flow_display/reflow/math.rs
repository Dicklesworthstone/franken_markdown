//! Display math uses the shared engine's actual glyph outlines and constructed
//! geometry. Each primitive remains one filled path in the engine's paint order;
//! a glyph's contours stay together so nonzero filling preserves its holes.

use std::cell::OnceCell;
use std::mem::size_of;

use fmd_math::{Color, Engine, Layout, PathContour, PathSeg, Placed, Style};

use super::{FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow};
use crate::display::{
    AccessibleReadingRole, DisplayItem, DisplayPath, DisplayPathCommand, DisplayRect,
    MAX_DISPLAY_PATH_COMMANDS, MAX_DISPLAY_TOTAL_PATH_COMMANDS,
};
use crate::span::SourceSpan;
use crate::text::OwnedTextRun;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_PRIMITIVES: usize = 4096;
const MAX_COORDINATE: f64 = 1_000_000.0;
const INK_PADDING_EM: f64 = 0.10;
pub(in crate::flow_display) const COMMAND_BYTES: usize = size_of::<DisplayPathCommand>();

#[derive(Default)]
pub(in crate::flow_display) struct MathRenderer {
    // Per reflow pass and lazy: ordinary prose never parses the math fonts.
    engine: OnceCell<Result<Engine, String>>,
}

pub(in crate::flow_display) enum MathFailure {
    /// A bounded formula failure preserves the entire source as visible code.
    Fallback(String),
    /// The enclosing layout's remaining budget cannot retain this formula.
    Budget,
}

impl From<String> for MathFailure {
    fn from(reason: String) -> Self {
        Self::Fallback(reason)
    }
}

struct FormulaPath {
    contours: Vec<PathContour>,
    color: Option<[f32; 4]>,
    command_count: usize,
}

pub(in crate::flow_display) struct Formula {
    paths: Vec<FormulaPath>,
    bounds: Bounds,
    commands: usize,
}

pub(in crate::flow_display) struct PlacedFormula {
    pub(in crate::flow_display) paths: Vec<DisplayPath>,
    pub(in crate::flow_display) bounds: DisplayRect,
    pub(in crate::flow_display) commands: usize,
}

impl MathRenderer {
    pub(in crate::flow_display) fn typeset(
        &self,
        source: &str,
        remaining_commands: usize,
    ) -> Result<Formula, MathFailure> {
        self.typeset_style(source, Style::Display, remaining_commands)
    }

    pub(super) fn typeset_style(
        &self,
        source: &str,
        style: Style,
        remaining_commands: usize,
    ) -> Result<Formula, MathFailure> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(MathFailure::Fallback(
                "formula exceeds the 64 KiB source limit".to_owned(),
            ));
        }
        let engine = self
            .engine
            .get_or_init(|| Engine::bundled().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(Clone::clone)?;
        let mut layout = engine
            .typeset(source, style)
            .map_err(|error| error.to_string())?;
        let primitives = layout
            .glyphs
            .len()
            .saturating_add(layout.rules.len())
            .saturating_add(layout.paths.len());
        if primitives > MAX_PRIMITIVES {
            return Err(MathFailure::Fallback(
                "formula exceeds the 4096 primitive limit".to_owned(),
            ));
        }
        if layout.order.len() != primitives {
            return Err(MathFailure::Fallback(
                "math engine returned an incomplete paint order".to_owned(),
            ));
        }
        let mut seen = vec![false; primitives];
        let mut bounds = Bounds::default();
        // Layout advance/phantom space and the full outline control hull are
        // both significant. Italic overhangs and radical corners can lie beyond
        // the nominal advance box.
        bounds.add(0.0, -layout.depth)?;
        bounds.add(layout.width, layout.height)?;
        let mut paths = Vec::with_capacity(primitives);
        let mut commands = 0usize;
        let mut add_path = |contours: Vec<PathContour>, color: Option<Color>| {
            let color = color.map(valid_color).transpose()?;
            let mut command_count = 0usize;
            for contour in &contours {
                if contour.segments.is_empty() {
                    return Err(MathFailure::Fallback(
                        "math engine returned an empty contour".to_owned(),
                    ));
                }
                command_count = command_count
                    .checked_add(contour.segments.len())
                    .and_then(|count| count.checked_add(2))
                    .ok_or_else(|| {
                        MathFailure::Fallback("formula path is too complex".to_owned())
                    })?;
                if command_count > MAX_DISPLAY_PATH_COMMANDS {
                    return Err(MathFailure::Fallback(
                        "formula exceeds the 65536 commands per primitive limit".to_owned(),
                    ));
                }
                bounds.add(contour.start.0, contour.start.1)?;
                for segment in &contour.segments {
                    match *segment {
                        PathSeg::Line { to } => bounds.add(to.0, to.1)?,
                        PathSeg::Quad { ctrl, to } => {
                            bounds.add(ctrl.0, ctrl.1)?;
                            bounds.add(to.0, to.1)?;
                        }
                    }
                }
            }
            commands = commands
                .checked_add(command_count)
                .ok_or_else(|| MathFailure::Fallback("formula path is too complex".to_owned()))?;
            if commands > MAX_DISPLAY_TOTAL_PATH_COMMANDS {
                return Err(MathFailure::Fallback(
                    "formula exceeds the 262144 path command limit".to_owned(),
                ));
            }
            if commands > remaining_commands {
                return Err(MathFailure::Budget);
            }
            if !contours.is_empty() {
                paths.push(FormulaPath {
                    contours,
                    color,
                    command_count,
                });
            }
            Ok(())
        };
        let mut primitive = Layout::default();
        for placed in std::mem::take(&mut layout.order) {
            // Verify each primitive exactly once before indexing. Treat an
            // invalid engine inventory as one source-preserving fallback.
            let ordinal = match placed {
                Placed::Glyph(index) if index < layout.glyphs.len() => index,
                Placed::Rule(index) if index < layout.rules.len() => layout.glyphs.len() + index,
                Placed::Path(index) if index < layout.paths.len() => {
                    layout.glyphs.len() + layout.rules.len() + index
                }
                _ => {
                    return Err(MathFailure::Fallback(
                        "math engine returned an invalid paint order".to_owned(),
                    ));
                }
            };
            if std::mem::replace(&mut seen[ordinal], true) {
                return Err(MathFailure::Fallback(
                    "math engine repeated a paint primitive".to_owned(),
                ));
            }
            match placed {
                Placed::Glyph(index) => {
                    let glyph = &layout.glyphs[index];
                    primitive.glyphs.push(glyph.clone());
                    add_path(
                        fmd_math::paths::resolve_paths(engine, &primitive)
                            .map_err(|error| error.to_string())?,
                        glyph.color,
                    )?;
                    primitive.glyphs.clear();
                }
                Placed::Rule(index) => {
                    let rule = &layout.rules[index];
                    primitive.rules.push(rule.clone());
                    add_path(
                        fmd_math::paths::resolve_paths(engine, &primitive)
                            .map_err(|error| error.to_string())?,
                        rule.color,
                    )?;
                    primitive.rules.clear();
                }
                Placed::Path(index) => {
                    let path = &mut layout.paths[index];
                    add_path(std::mem::take(&mut path.contours), path.color)?;
                }
            }
        }
        bounds.left -= INK_PADDING_EM;
        bounds.right += INK_PADDING_EM;
        bounds.bottom -= INK_PADDING_EM;
        bounds.top += INK_PADDING_EM;
        Ok(Formula {
            paths,
            bounds,
            commands,
        })
    }
}

fn valid_color(color: Color) -> Result<[f32; 4], String> {
    if [color.r, color.g, color.b]
        .into_iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err("math engine returned an invalid color".to_owned());
    }
    Ok([color.r as f32, color.g as f32, color.b as f32, 1.0])
}

#[derive(Clone, Copy)]
pub(super) struct InlineMetrics {
    pub(super) advance: f32,
    pub(super) ascent: f32,
    pub(super) descent: f32,
    scale: f64,
}

impl Formula {
    pub(super) fn command_count(&self) -> usize {
        self.commands
    }

    pub(super) fn natural_advance(&self, font_size: f32) -> f64 {
        (self.bounds.right - self.bounds.left) * f64::from(font_size)
    }

    pub(super) fn inline_metrics(
        &self,
        font_size: f32,
        available_width: f32,
    ) -> Result<InlineMetrics, String> {
        let width = self.bounds.right - self.bounds.left;
        let scale = f64::from(font_size).min(f64::from(available_width) / width);
        let metrics = InlineMetrics {
            advance: (width * scale) as f32,
            ascent: (self.bounds.top * scale) as f32,
            descent: (-self.bounds.bottom * scale) as f32,
            scale,
        };
        if !scale.is_finite()
            || scale <= 0.0
            || ![metrics.advance, metrics.ascent, metrics.descent]
                .into_iter()
                .all(|value| value.is_finite() && value >= 0.0)
            || metrics.advance == 0.0
        {
            return Err("formula cannot fit the available inline geometry".to_owned());
        }
        Ok(metrics)
    }

    pub(super) fn place_inline(
        &self,
        x: f32,
        baseline_y: f32,
        metrics: InlineMetrics,
        span: SourceSpan,
    ) -> Result<PlacedFormula, String> {
        self.place_at(
            f64::from(x),
            f64::from(baseline_y) - self.bounds.top * metrics.scale,
            metrics.scale,
            span,
        )
    }

    pub(in crate::flow_display) fn place(
        &self,
        x: f32,
        y: f32,
        available_width: f32,
        font_size: f32,
        span: SourceSpan,
    ) -> Result<PlacedFormula, String> {
        let width = self.bounds.right - self.bounds.left;
        let scale = f64::from(font_size).min(f64::from(available_width) / width);
        let placed_width = width * scale;
        let origin_x = f64::from(x) + (f64::from(available_width) - placed_width) * 0.5;
        self.place_at(origin_x, f64::from(y), scale, span)
    }

    fn place_at(
        &self,
        origin_x: f64,
        origin_y: f64,
        scale: f64,
        span: SourceSpan,
    ) -> Result<PlacedFormula, String> {
        let bounds = DisplayRect::new(
            origin_x as f32,
            origin_y as f32,
            ((self.bounds.right - self.bounds.left) * scale) as f32,
            ((self.bounds.top - self.bounds.bottom) * scale) as f32,
        );
        if !scale.is_finite()
            || scale <= 0.0
            || [
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                bounds.right(),
                bounds.bottom(),
            ]
            .into_iter()
            .any(|value| !value.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return Err("formula cannot fit the available display geometry".to_owned());
        }
        let mut paths = Vec::with_capacity(self.paths.len());
        for path in &self.paths {
            let mut left = f32::INFINITY;
            let mut right = f32::NEG_INFINITY;
            let mut top = f32::INFINITY;
            let mut bottom = f32::NEG_INFINITY;
            let mut point = |x: f64, y: f64| {
                let x = (origin_x + (x - self.bounds.left) * scale) as f32;
                let y = (origin_y + (self.bounds.top - y) * scale) as f32;
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
                (x, y)
            };
            let mut commands = Vec::with_capacity(path.command_count);
            for contour in &path.contours {
                let (x, y) = point(contour.start.0, contour.start.1);
                commands.push(DisplayPathCommand::MoveTo { x, y });
                for segment in &contour.segments {
                    commands.push(match *segment {
                        PathSeg::Line { to } => {
                            let (x, y) = point(to.0, to.1);
                            DisplayPathCommand::LineTo { x, y }
                        }
                        PathSeg::Quad { ctrl, to } => {
                            let (cx, cy) = point(ctrl.0, ctrl.1);
                            let (x, y) = point(to.0, to.1);
                            DisplayPathCommand::QuadTo { cx, cy, x, y }
                        }
                    });
                }
                commands.push(DisplayPathCommand::Close);
            }
            let path = DisplayPath {
                bounds: DisplayRect::new(left, top, right - left, bottom - top),
                commands,
                color_role: "math".to_owned(),
                fill: path.color,
                source_span: span,
            };
            if !path.is_valid() {
                return Err("math engine returned invalid outline geometry".to_owned());
            }
            paths.push(path);
        }
        Ok(PlacedFormula {
            paths,
            bounds,
            commands: self.commands,
        })
    }
}

#[derive(Default)]
struct Bounds {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
}

impl Bounds {
    fn add(&mut self, x: f64, y: f64) -> Result<(), String> {
        if !x.is_finite() || !y.is_finite() || x.abs() > MAX_COORDINATE || y.abs() > MAX_COORDINATE
        {
            return Err("formula exceeds the finite coordinate limit".to_owned());
        }
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.bottom = self.bottom.min(y);
        self.top = self.top.max(y);
        Ok(())
    }
}

pub(in crate::flow_display) fn fallback_explanation(reason: &str) -> String {
    let reason: String = reason.chars().take(160).collect();
    format!("Math source fallback: {reason}")
}

impl<F: ?Sized> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    pub(super) fn math_block(
        &mut self,
        source: &str,
        x: f32,
        y: f32,
        width: f32,
        span: SourceSpan,
    ) -> Result<f32, FlowLayoutError> {
        // Formula input and retained command payload share the existing global
        // work budget with host shaping. Charge input before font initialization.
        self.shape_bytes = self
            .shape_bytes
            .checked_add(source.len())
            .filter(|bytes| *bytes <= self.options.max_total_shape_bytes)
            .ok_or(FlowLayoutError::BudgetExceeded("total shaping bytes"))?;
        let remaining_bytes = self.options.max_total_shape_bytes - self.shape_bytes;
        let remaining_commands = MAX_DISPLAY_TOTAL_PATH_COMMANDS - self.math_commands;
        let command_limit = remaining_commands.min(remaining_bytes / COMMAND_BYTES);
        let result = self
            .math
            .typeset(source, command_limit)
            .and_then(|formula| {
                formula
                    .place(x, y, width, self.options.body_size, span)
                    .map_err(MathFailure::Fallback)
            });
        let (bounds, height) = match result {
            Ok(formula) => {
                self.line()?;
                self.math_commands += formula.commands;
                self.shape_bytes += formula.commands * COMMAND_BYTES;
                self.list.include_layout_bounds(formula.bounds);
                for path in formula.paths {
                    self.item(DisplayItem::Path(path))?;
                }
                (formula.bounds, formula.bounds.height)
            }
            Err(MathFailure::Budget) => {
                return Err(FlowLayoutError::BudgetExceeded(
                    if remaining_bytes / COMMAND_BYTES < remaining_commands {
                        "total shaping bytes"
                    } else {
                        "math path commands"
                    },
                ));
            }
            Err(MathFailure::Fallback(reason)) => {
                let explanation = fallback_explanation(&reason);
                let explanation_height = self.inline_text(
                    &explanation,
                    &[],
                    x,
                    y,
                    width,
                    self.options.body_size,
                    self.options.line_height,
                    FlowTextRole::Body,
                    "muted",
                    span,
                )?;
                let source_height = self.text(
                    source,
                    x,
                    y + explanation_height,
                    width,
                    self.options.code_size,
                    self.options.line_height,
                    FlowTextRole::Code,
                    "code",
                    span,
                )?;
                let height = explanation_height + source_height;
                (DisplayRect::new(x, y, width, height), height)
            }
        };
        // Exactly one source-only node keeps block/reading index parity and
        // avoids inventing per-glyph offsets or accessibility text from paths.
        self.reading(AccessibleReadingRole::Math, source, bounds, span);
        Ok(height)
    }
}
