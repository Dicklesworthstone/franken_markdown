//! Actual first-party diagram geometry with font-shaped labels. The complete
//! figure is prepared before publication, then uniformly fitted to its lane.
//! Unsupported compiler output remains readable, exact source code.

use std::mem::size_of;

use super::{FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow, validate_run};
use crate::display::{
    AccessibleReadingRole, DisplayItem, DisplayPath, DisplayPathCommand, DisplayPathStroke,
    DisplayRect, DisplayTextRun, MAX_DISPLAY_TOTAL_PATH_COMMANDS,
};
use crate::pdf::flow_diagram::{self as scene, Item};
use crate::span::SourceSpan;
use crate::text::OwnedTextRun;

/// ASCII diagrams deliberately keep their fixed-width source panel, matching
/// the paged outputs. Unsupported Mermaid types still get an explicit fallback.
pub(super) fn is_figure(source: &str, language: &str) -> bool {
    let first = source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    !language.trim().eq_ignore_ascii_case("ditaa")
        && !(first.starts_with("+---") && source.contains('|'))
        && crate::diagrams::is_diagram_code(source, language)
}

enum Failure {
    Source(&'static str),
    Layout(FlowLayoutError),
}

impl From<FlowLayoutError> for Failure {
    fn from(error: FlowLayoutError) -> Self {
        Self::Layout(error)
    }
}

enum PreparedItem {
    Path(scene::Path),
    Text {
        run: OwnedTextRun,
        bounds: DisplayRect,
        baseline: f32,
        fill: [f32; 4],
    },
}

impl<F: ?Sized> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn diagram_block(
        &mut self,
        source: &str,
        language: &str,
        x: f32,
        y: f32,
        width: f32,
        span: SourceSpan,
    ) -> Result<f32, FlowLayoutError> {
        self.diagram_bytes(source.len())?;
        let bounds = match self.prepare_diagram(source, language, x, y, width, span) {
            Ok((items, bounds, commands, lines)) => {
                // The whole figure, including every shaped label and stroke,
                // has been validated. No recoverable failure can publish half.
                self.path_commands += commands;
                self.lines += lines;
                self.list.include_layout_bounds(bounds);
                for item in items {
                    self.item(item)?;
                }
                bounds
            }
            Err(Failure::Layout(error)) => return Err(error),
            Err(Failure::Source(reason)) => {
                let explanation = format!("Diagram source fallback: {reason}");
                let notice_height = self.inline_text(
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
                    y + notice_height,
                    width,
                    self.options.code_size,
                    self.options.line_height,
                    FlowTextRole::Code,
                    "code",
                    span,
                )?;
                DisplayRect::new(x, y, width, notice_height + source_height)
            }
        };
        // One exact source node preserves block/reading parity. Label clusters
        // are local to their own text, never invented Markdown caret offsets.
        self.reading(AccessibleReadingRole::Diagram, source, bounds, span);
        Ok(bounds.height)
    }

    fn diagram_bytes(&mut self, bytes: usize) -> Result<(), FlowLayoutError> {
        self.shape_bytes = self
            .shape_bytes
            .checked_add(bytes)
            .filter(|value| *value <= self.options.max_total_shape_bytes)
            .ok_or(FlowLayoutError::BudgetExceeded("total shaping bytes"))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_diagram(
        &mut self,
        source: &str,
        language: &str,
        x: f32,
        y: f32,
        width: f32,
        span: SourceSpan,
    ) -> Result<(Vec<DisplayItem>, DisplayRect, usize, usize), Failure> {
        let scene = scene::compile(source, language).map_err(Failure::Source)?;
        let item_count = scene.items.len() + 1; // The compiler's light canvas.
        if item_count
            > self
                .options
                .max_items
                .saturating_sub(self.list.items().len())
        {
            return Err(FlowLayoutError::BudgetExceeded("display items").into());
        }
        let lines = 1 + scene
            .items
            .iter()
            .filter(|item| matches!(item, Item::Text(_)))
            .count();
        if lines > self.options.max_lines.saturating_sub(self.lines) {
            return Err(FlowLayoutError::BudgetExceeded("lines").into());
        }
        let mut commands = 5usize; // Backdrop rectangle.
        let mut dash_bytes = 0usize;
        for item in &scene.items {
            if let Item::Path(path) = item {
                commands = commands
                    .checked_add(path.commands.len())
                    .ok_or(FlowLayoutError::BudgetExceeded("diagram path commands"))?;
                if let Some(stroke) = &path.stroke {
                    dash_bytes += stroke.dash.len() * size_of::<f32>();
                }
            }
        }
        if commands > MAX_DISPLAY_TOTAL_PATH_COMMANDS.saturating_sub(self.path_commands) {
            return Err(FlowLayoutError::BudgetExceeded("diagram path commands").into());
        }
        let retained_bytes = commands
            .checked_mul(size_of::<DisplayPathCommand>())
            .and_then(|bytes| bytes.checked_add(dash_bytes))
            .ok_or(FlowLayoutError::BudgetExceeded("total shaping bytes"))?;
        self.diagram_bytes(retained_bytes)?;

        let mut envelope = DisplayRect::new(0.0, 0.0, scene.width, scene.height);
        let mut prepared = Vec::with_capacity(scene.items.len());
        for item in scene.items {
            match item {
                Item::Path(path) => {
                    envelope = envelope.union(path.bounds);
                    prepared.push(PreparedItem::Path(path));
                }
                Item::Text(label) => {
                    let style = FlowInlineStyle {
                        bold: label.bold,
                        ..FlowInlineStyle::default()
                    };
                    let run = self
                        .shaped_with_style(&label.text, label.font_size, FlowTextRole::Body, style)
                        .map_err(|error| match error {
                            FlowLayoutError::Shaping(_) => Failure::Source(
                                "a diagram label is outside the active text shaping profile",
                            ),
                            other => Failure::Layout(other),
                        })?;
                    let metrics = run.vertical_metrics.ok_or(Failure::Source(
                        "diagram labels require measured text ascent and descent",
                    ))?;
                    let baseline = match label.baseline {
                        scene::Baseline::Alphabetic => label.y,
                        scene::Baseline::Middle => {
                            label.y + (metrics.ascent - metrics.descent) * 0.5
                        }
                    };
                    let left = match label.anchor {
                        scene::Anchor::Start => label.x,
                        scene::Anchor::Middle => label.x - run.total_advance * 0.5,
                        scene::Anchor::End => label.x - run.total_advance,
                    };
                    let bounds = DisplayRect::new(
                        left,
                        baseline - metrics.ascent,
                        run.total_advance,
                        metrics.ascent + metrics.descent,
                    );
                    if !valid_rect(bounds) {
                        return Err(FlowLayoutError::InvalidShapedRun.into());
                    }
                    envelope = envelope.union(bounds);
                    prepared.push(PreparedItem::Text {
                        run,
                        bounds,
                        baseline,
                        fill: label.fill,
                    });
                }
            }
        }
        if !valid_rect(envelope) || envelope.width <= 0.0 || envelope.height <= 0.0 {
            return Err(Failure::Source("diagram has no finite display envelope"));
        }
        // Fitting the complete measured scene also contains return routes,
        // arrowheads, caps, miter joins and labels outside the nominal SVG box.
        let scale = (f64::from(self.options.body_size) / 14.0)
            .min(f64::from(width) / f64::from(envelope.width));
        let figure_width = (f64::from(envelope.width) * scale) as f32;
        let figure_height = (f64::from(envelope.height) * scale) as f32;
        let bounds = DisplayRect::new(
            x + (width - figure_width) * 0.5,
            y,
            figure_width,
            figure_height,
        );
        if !scale.is_finite()
            || scale <= 0.0
            || !valid_rect(bounds)
            || bounds.width <= 0.0
            || bounds.height <= 0.0
        {
            return Err(Failure::Source(
                "diagram cannot fit the available display geometry",
            ));
        }
        let point = |px: f32, py: f32| {
            (
                (f64::from(bounds.x) + (f64::from(px) - f64::from(envelope.x)) * scale) as f32,
                (f64::from(bounds.y) + (f64::from(py) - f64::from(envelope.y)) * scale) as f32,
            )
        };
        let rect = |rect: DisplayRect| {
            let (left, top) = point(rect.x, rect.y);
            let (right, bottom) = point(rect.right(), rect.bottom());
            DisplayRect::new(left, top, right - left, bottom - top)
        };
        let mut output = Vec::with_capacity(item_count);
        output.push(DisplayItem::Path(DisplayPath {
            bounds,
            commands: vec![
                DisplayPathCommand::MoveTo {
                    x: bounds.x,
                    y: bounds.y,
                },
                DisplayPathCommand::LineTo {
                    x: bounds.right(),
                    y: bounds.y,
                },
                DisplayPathCommand::LineTo {
                    x: bounds.right(),
                    y: bounds.bottom(),
                },
                DisplayPathCommand::LineTo {
                    x: bounds.x,
                    y: bounds.bottom(),
                },
                DisplayPathCommand::Close,
            ],
            color_role: "diagram".to_owned(),
            fill: Some([1.0; 4]),
            stroke: None,
            source_span: span,
        }));
        for item in prepared {
            match item {
                PreparedItem::Path(path) => {
                    let commands = path
                        .commands
                        .into_iter()
                        .map(|command| map_command(command, &point))
                        .collect();
                    let stroke = path.stroke.map(|stroke| DisplayPathStroke {
                        color: stroke.color,
                        width: (f64::from(stroke.width) * scale) as f32,
                        dash: stroke
                            .dash
                            .into_iter()
                            .map(|value| (f64::from(value) * scale) as f32)
                            .collect(),
                        dash_offset: (f64::from(stroke.dash_offset) * scale) as f32,
                        cap: stroke.cap.into(),
                        join: stroke.join.into(),
                        miter_limit: stroke.miter_limit,
                    });
                    let path = DisplayPath {
                        bounds: rect(path.bounds),
                        commands,
                        color_role: "diagram".to_owned(),
                        fill: path.fill,
                        stroke,
                        source_span: span,
                    };
                    if !path.is_valid() {
                        return Err(Failure::Source("diagram has invalid fitted path geometry"));
                    }
                    output.push(DisplayItem::Path(path));
                }
                PreparedItem::Text {
                    mut run,
                    bounds: text_bounds,
                    baseline,
                    fill,
                } => {
                    scale_run(&mut run, scale as f32);
                    validate_run(&run, &run.logical_text, run.context.font_size)?;
                    let bounds = rect(text_bounds);
                    let text = DisplayTextRun {
                        bounds,
                        text: run.logical_text.clone(),
                        font_size: run.context.font_size,
                        font_run: Some(run),
                        color_role: "diagram".to_owned(),
                        source_span: span,
                        baseline_y: Some(point(0.0, baseline).1),
                        math_fallback: None,
                        fill: Some(fill),
                    };
                    if !text.has_valid_placement() || !valid_rect(bounds) {
                        return Err(FlowLayoutError::InvalidShapedRun.into());
                    }
                    output.push(DisplayItem::Text(text));
                }
            }
        }
        Ok((output, bounds, commands, lines))
    }
}

fn valid_rect(rect: DisplayRect) -> bool {
    [
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        rect.right(),
        rect.bottom(),
    ]
    .into_iter()
    .all(f32::is_finite)
        && rect.width >= 0.0
        && rect.height >= 0.0
}

fn scale_run(run: &mut OwnedTextRun, scale: f32) {
    run.context.font_size *= scale;
    run.total_advance *= scale;
    for glyph in &mut run.glyphs {
        glyph.x_advance *= scale;
        glyph.y_advance *= scale;
        glyph.x_offset *= scale;
        glyph.y_offset *= scale;
    }
    for cluster in &mut run.clusters {
        cluster.x_start *= scale;
        cluster.x_end *= scale;
    }
    if let Some(metrics) = &mut run.vertical_metrics {
        metrics.ascent *= scale;
        metrics.descent *= scale;
    }
}

fn map_command(
    command: DisplayPathCommand,
    point: &impl Fn(f32, f32) -> (f32, f32),
) -> DisplayPathCommand {
    match command {
        DisplayPathCommand::MoveTo { x, y } => {
            let (x, y) = point(x, y);
            DisplayPathCommand::MoveTo { x, y }
        }
        DisplayPathCommand::LineTo { x, y } => {
            let (x, y) = point(x, y);
            DisplayPathCommand::LineTo { x, y }
        }
        DisplayPathCommand::QuadTo { cx, cy, x, y } => {
            let (cx, cy) = point(cx, cy);
            let (x, y) = point(x, y);
            DisplayPathCommand::QuadTo { cx, cy, x, y }
        }
        DisplayPathCommand::CubicTo {
            cx1,
            cy1,
            cx2,
            cy2,
            x,
            y,
        } => {
            let (cx1, cy1) = point(cx1, cy1);
            let (cx2, cy2) = point(cx2, cy2);
            let (x, y) = point(x, y);
            DisplayPathCommand::CubicTo {
                cx1,
                cy1,
                cx2,
                cy2,
                x,
                y,
            }
        }
        DisplayPathCommand::Close => DisplayPathCommand::Close,
    }
}
