//! Width-aware display layout over owned, host-shaped text runs.
//!
//! Hosts supply the existing fmd-font/native-route OwnedTextRun contract. This
//! module does not select system fonts, perform I/O, or substitute an estimated
//! width when shaping fails. Wrapping is greedy at spaces, with cluster-boundary
//! emergency breaks; it is not the PDF Knuth-Plass paragraph engine.

use super::{BlockMeta, DisplayBlock, FlowDisplayError, FlowInlineStyle, ResumableFlowDisplay};

mod styled;
use crate::ast::Align;
use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayImage, DisplayItem, DisplayList,
    DisplayRect, DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
use crate::flow_display::FlowInlineRun;
use crate::span::SourceSpan;
use crate::text::{Direction, OwnedTextRun};
use std::collections::HashMap;
use std::fmt;

/// Font/style role to resolve through a host's immutable font registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowTextRole {
    Body,
    Heading(u8),
    Code,
    Marker,
    TableHeader,
    TableCell,
}

/// Explicit viewport and work limits for shaped reflow. `max_shape_bytes` bounds
/// one call to the shaper; `max_total_shape_bytes` charges all calls, including
/// table measurement and reshaping at line boundaries. The same ceiling
/// separately bounds retained link-target bytes in styled output. Hosts must
/// bound their shaper's execution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowLayoutOptions {
    pub viewport_width: f32,
    pub body_size: f32,
    pub code_size: f32,
    pub line_height: f32,
    pub max_lines: usize,
    pub max_items: usize,
    pub max_shape_bytes: usize,
    pub max_total_shape_bytes: usize,
}

impl Default for FlowLayoutOptions {
    fn default() -> Self {
        Self {
            viewport_width: 800.0,
            body_size: 14.0,
            code_size: 13.0,
            line_height: 20.0,
            max_lines: 200_000,
            max_items: 500_000,
            max_shape_bytes: 16 * 1024,
            max_total_shape_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FlowLayoutError {
    Input(FlowDisplayError),
    InvalidOptions,
    Shaping(String),
    InvalidShapedRun,
    BudgetExceeded(&'static str),
    ClusterTooWide { advance: f32, available: f32 },
    /// Even emergency cluster-boundary wrapping cannot fit all columns.
    TableTooNarrow { minimum: f32, available: f32 },
}

impl fmt::Display for FlowLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => write!(f, "flow input error: {error}"),
            Self::InvalidOptions => write!(f, "flow layout needs finite positive width, sizes, and leading"),
            Self::Shaping(message) => write!(f, "flow text shaping failed: {message}"),
            Self::InvalidShapedRun => write!(f, "shaper returned inconsistent text, clusters, or metrics"),
            Self::BudgetExceeded(name) => write!(f, "flow layout budget exceeded: {name}"),
            Self::ClusterTooWide { advance, available } => {
                write!(f, "indivisible text cluster is {advance} wide; available width is {available}")
            }
            Self::TableTooNarrow { minimum, available } => {
                write!(f, "table columns require at least {minimum} points including padding; available width is {available}")
            }
        }
    }
}

impl std::error::Error for FlowLayoutError {}

impl ResumableFlowDisplay {
    /// Reflow currently emitted blocks using actual shaped glyph advances.
    ///
    /// `shape(text, size, role)` must return a complete, internally consistent
    /// `OwnedTextRun` for exactly that logical text. It can call `Font::shape`
    /// plus `OwnedTextRun::from_shaped_run`, or the native fallback route. Font
    /// identity, language, direction, and fallback capabilities remain the
    /// host's responsibility. No font or network discovery happens here.
    ///
    /// All text items contain owned glyph/cluster data for CPU hit-testing and
    /// UTF-8/UTF-16 selection. Those offsets are LOCAL TO THE LINE'S READING TEXT,
    /// not original Markdown offsets. `source_span` remains an enclosing AST
    /// range. The legacy `to_display_list` remains the estimate-only API.
    ///
    /// Code whitespace and text bytes are preserved; lines break only between
    /// shaped clusters. Each line is reshaped and measured again at its final
    /// boundary. An over-wide indivisible cluster returns an error rather than
    /// clipping text or silently splitting a ligature/combining sequence.
    /// Output is returned atomically; a failure publishes no partial display list.
    ///
    /// Tables share one measured column grid across headers and body rows.
    /// Compact columns reach their natural width before long narrative columns
    /// consume the remaining space. Already-prepared continuation rows also
    /// participate, so source/output batch boundaries cannot change that grid.
    /// Measurement uses the same shaper and cumulative budget as final layout.
    /// Explicit GFM left/center/right alignment applies to each final shaped
    /// line within its padded cell; unspecified alignment follows run direction.
    pub fn to_shaped_display_list<F>(
        &self,
        options: FlowLayoutOptions,
        mut shape: F,
    ) -> Result<DisplayList, FlowLayoutError>
    where
        F: FnMut(&str, f32, FlowTextRole) -> Result<OwnedTextRun, String>,
    {
        let mut adapter = |text: &str, size: f32, role: FlowTextRole, _style: FlowInlineStyle| {
            shape(text, size, role)
        };
        self.layout_with_styles(options, &mut adapter, false)
    }

    /// Render inline formatting and active link geometry using actual font faces.
    ///
    /// The callback receives the block role AND composable inline style. Select
    /// bold/italic/monospace faces before shaping; decoration flags do not imply
    /// a different font. Returned text items own fragment-local glyph/UTF-16
    /// offsets, not Markdown offsets. Reading order retains the unsplit text.
    ///
    /// Style transitions do not introduce word breaks. Every final fragment is
    /// reshaped and the combined line width is rechecked. Same-direction RTL
    /// fragments are placed right-to-left; mixed-direction styled lines require
    /// paragraph-level bidi resolution and return a shaping error explicitly.
    /// Links use the conservative active_link_target policy; navigation remains
    /// host-authorized. The estimate-only and three-argument APIs stay unchanged.
    pub fn to_styled_display_list<F>(
        &self,
        options: FlowLayoutOptions,
        mut shape: F,
    ) -> Result<DisplayList, FlowLayoutError>
    where
        F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        self.layout_with_styles(options, &mut shape, true)
    }

    fn layout_with_styles<F>(
        &self,
        options: FlowLayoutOptions,
        shape: &mut F,
        styled: bool,
    ) -> Result<DisplayList, FlowLayoutError>
    where
        F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        if let Some(error) = &self.initial_error {
            return Err(FlowLayoutError::Input(error.clone()));
        }
        let mut layout = Reflow::new(options, shape)?;
        let resolved: HashMap<_, _> = self.resolved_assets.iter()
            .filter(|result| result.generation == self.generation)
            .map(|result| (result.request_id, result)).collect();
        let mut y = 0.0;
        let mut table_edges = Vec::new();
        for (block_index, (block, meta)) in self.blocks.iter().zip(&self.metadata).enumerate() {
            let x = f32::from(meta.list_depth) * 20.0 + f32::from(meta.quote_depth) * 16.0;
            let width = options.viewport_width - x;
            if width <= 0.0 { return Err(FlowLayoutError::InvalidOptions); }
            let span = meta.span;
            if !matches!(block, DisplayBlock::TableRow { .. }) {
                table_edges.clear();
            }
            match block {
                DisplayBlock::Heading { level, text } => {
                    let factor = match level { 1 => 2.0, 2 => 22.0 / 14.0, 3 => 18.0 / 14.0, _ => 16.0 / 14.0 };
                    let size = options.body_size * factor;
                    let height = layout.inline_text(text, if styled { &meta.inline_runs } else { &[] },
                        x, y, width, size, size * 1.5,
                        FlowTextRole::Heading(*level), "heading", span)?;
                    let bounds = DisplayRect::new(x, y, width, height);
                    if let Some(id) = &meta.heading_id {
                        layout.item(DisplayItem::Anchor(DisplaySemanticAnchor {
                            bounds, anchor_id: id.clone(), is_heading: true, level: *level, source_span: span,
                        }))?;
                    }
                    layout.reading(AccessibleReadingRole::Heading { level: *level }, text, bounds, span);
                    y += height + 10.0;
                }
                DisplayBlock::Paragraph { text } | DisplayBlock::ListItem { text, .. }
                | DisplayBlock::Quote { text } => {
                    layout.marker(meta, x, y)?;
                    let height = layout.inline_text(text, if styled { &meta.inline_runs } else { &[] },
                        x, y, width, options.body_size, options.line_height,
                        FlowTextRole::Body, "text", span)?;
                    let bounds = DisplayRect::new(x, y, width, height);
                    if meta.quote_depth > 0 {
                        layout.vector(DisplayRect::new(x - 12.0, y, 4.0, height),
                            VectorShapeType::CalloutAccentBar, "accent", span)?;
                    }
                    let role = match block {
                        DisplayBlock::ListItem { .. } => AccessibleReadingRole::ListItem,
                        DisplayBlock::Quote { .. } => AccessibleReadingRole::BlockQuote,
                        _ => AccessibleReadingRole::Paragraph,
                    };
                    let reading = match meta.task {
                        Some(true) => format!("[x] {text}"),
                        Some(false) => format!("[ ] {text}"),
                        None => text.clone(),
                    };
                    layout.reading(role, &reading, bounds, span);
                    y += height + if meta.list_depth > 0 { 4.0 } else { 8.0 };
                }
                DisplayBlock::CodeBlock { source, .. } => {
                    let height = layout.text(source, x, y, width, options.code_size, options.line_height,
                        FlowTextRole::Code, "code", span)?;
                    layout.reading(AccessibleReadingRole::CodeBlock, source, DisplayRect::new(x, y, width, height), span);
                    y += height + 12.0;
                }
                DisplayBlock::Rule => {
                    let bounds = DisplayRect::new(x, y, width, 2.0);
                    layout.vector(bounds, VectorShapeType::HorizontalRule, "border", span)?;
                    layout.reading(AccessibleReadingRole::ThematicBreak, "", bounds, span);
                    y += 16.0;
                }
                DisplayBlock::TableHeader { cells } | DisplayBlock::TableRow { cells } => {
                    let header = matches!(block, DisplayBlock::TableHeader { .. });
                    if table_edges.is_empty() {
                        // Preparation owns the complete parsed table even when
                        // step() has only published its header. Inspect references,
                        // not cloned cell text, and stop at the next table/header.
                        let emitted = self.blocks[block_index..].iter()
                            .zip(&self.metadata[block_index..]);
                        let waiting = self.prepared.iter().flat_map(|prepared| {
                            prepared.iter().map(|item| (&item.block, &item.meta))
                        });
                        let rows = emitted.chain(waiting).enumerate()
                            .take_while(|(offset, (row, row_meta))| {
                                *offset == 0 || (matches!(row, DisplayBlock::TableRow { .. })
                                    && row_meta.span == span
                                    && row_meta.list_depth == meta.list_depth
                                    && row_meta.quote_depth == meta.quote_depth)
                            })
                            .map(|(_, row)| row);
                        table_edges = layout.table_columns(rows, width, styled)?;
                    }
                    let mut height = options.line_height;
                    let mut children = Vec::new();
                    for (column, edges) in table_edges.windows(2).enumerate() {
                        let cx = x + edges[0];
                        let cell_width = (x + edges[1]) - cx;
                        let text = cells.get(column).map_or("", String::as_str);
                        let runs = if styled { meta.cell_runs.get(column).map(Vec::as_slice).unwrap_or(&[]) } else { &[] };
                        layout.alignment = meta.table_alignments.get(column).copied().unwrap_or(Align::None);
                        let used = layout.inline_text(text, runs, cx + 4.0, y + 4.0, cell_width - 8.0,
                            options.body_size, options.line_height,
                            if header { FlowTextRole::TableHeader } else { FlowTextRole::TableCell },
                            if header { "heading" } else { "text" }, span)?;
                        layout.alignment = Align::None;
                        height = height.max(used);
                        children.push(AccessibleReadingNode {
                            role: if header { AccessibleReadingRole::TableHeaderCell } else { AccessibleReadingRole::TableCell },
                            text: text.to_owned(), source_span: span,
                            bounds: DisplayRect::new(cx, y, cell_width, 0.0), children: Vec::new(),
                        });
                    }
                    height += 8.0;
                    for child in &mut children {
                        child.bounds.height = height;
                        layout.vector(child.bounds, VectorShapeType::TableBorder, "table-border", span)?;
                    }
                    layout.list.push_reading_node(AccessibleReadingNode {
                        role: if header { AccessibleReadingRole::TableHeaderRow } else { AccessibleReadingRole::TableRow },
                        text: cells.join(" | "), source_span: span,
                        bounds: DisplayRect::new(x, y, width, height), children,
                    });
                    y += height;
                }
                DisplayBlock::UnresolvedAsset(asset) => {
                    let result = resolved.get(&asset.id);
                    let natural_width = result.map_or(asset.estimated_width, |r| r.width) as f32;
                    let natural_height = result.map_or(asset.estimated_height, |r| r.height) as f32;
                    let scale = (width / natural_width).min(1.0);
                    let bounds = DisplayRect::new(x, y, natural_width * scale, natural_height * scale);
                    layout.item(DisplayItem::Image(DisplayImage {
                        bounds, request_id: asset.id.0, destination: asset.reference.clone(),
                        alt_text: asset.alt_text.clone(), is_resolved: result.is_some(), source_span: span,
                    }))?;
                    if styled {
                        if let Some(target) = meta.image_link.as_deref().and_then(super::active_link_target) {
                            layout.link_anchor(target, bounds, span)?;
                        }
                    }
                    layout.reading(AccessibleReadingRole::Image, &asset.alt_text, bounds, span);
                    y += bounds.height + 10.0;
                }
            }
            if !y.is_finite() { return Err(FlowLayoutError::BudgetExceeded("layout height")); }
        }
        Ok(layout.list)
    }
}

struct Reflow<'a, F> {
    list: DisplayList,
    options: FlowLayoutOptions,
    shape: &'a mut F,
    lines: usize,
    shape_bytes: usize,
    link_bytes: usize,
    alignment: Align,
}

impl<'a, F> Reflow<'a, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    fn new(options: FlowLayoutOptions, shape: &'a mut F) -> Result<Self, FlowLayoutError> {
        if [options.viewport_width, options.body_size, options.code_size, options.line_height]
            .iter().any(|v| !v.is_finite() || *v <= 0.0 || *v > 1_000_000.0)
            || options.line_height < options.body_size.max(options.code_size)
        {
            return Err(FlowLayoutError::InvalidOptions);
        }
        Ok(Self { list: DisplayList::new(), options, shape, lines: 0, shape_bytes: 0, link_bytes: 0,
            alignment: Align::None })
    }

    /// Position the final reshaped line as a unit. Explicit alignment is physical
    /// left/center/right, independent of glyph direction. Unspecified alignment
    /// preserves the existing logical-start policy for LTR and RTL runs.
    fn line_start(&self, x: f32, width: f32, advance: f32, direction: Direction) -> f32 {
        match self.alignment {
            Align::Left => x,
            Align::Center => x + (width - advance) * 0.5,
            Align::Right => x + width - advance,
            Align::None => if direction == Direction::RightToLeft { x + width - advance } else { x },
        }
    }

    fn shaped(&mut self, text: &str, size: f32, role: FlowTextRole) -> Result<OwnedTextRun, FlowLayoutError> {
        self.shaped_with_style(text, size, role, FlowInlineStyle::default())
    }

    fn shaped_with_style(&mut self, text: &str, size: f32, role: FlowTextRole, style: FlowInlineStyle)
        -> Result<OwnedTextRun, FlowLayoutError>
    {
        if !size.is_finite() || size <= 0.0 { return Err(FlowLayoutError::InvalidOptions); }
        if text.len() > self.options.max_shape_bytes {
            return Err(FlowLayoutError::BudgetExceeded("bytes per shaping call"));
        }
        self.shape_bytes = self.shape_bytes.checked_add(text.len())
            .filter(|bytes| *bytes <= self.options.max_total_shape_bytes)
            .ok_or(FlowLayoutError::BudgetExceeded("total shaping bytes"))?;
        let run = (self.shape)(text, size, role, style).map_err(FlowLayoutError::Shaping)?;
        validate_run(&run, text, size)?;
        Ok(run)
    }

    /// Measure the entire admitted table once. Only column metrics survive;
    /// glyph runs are dropped after each cell fragment. A late wide cell cannot
    /// be missed by a sampling heuristic, and all shaping is budget-charged.
    fn table_columns<'b, I>(&mut self, rows: I, width: f32, styled: bool)
        -> Result<Vec<f32>, FlowLayoutError>
    where
        I: Iterator<Item = (&'b DisplayBlock, &'b BlockMeta)>,
    {
        let mut minimum = Vec::<f64>::new();
        let mut preferred = Vec::<f64>::new();
        for (block, meta) in rows {
            let (cells, role) = match block {
                DisplayBlock::TableHeader { cells } => (cells, FlowTextRole::TableHeader),
                DisplayBlock::TableRow { cells } => (cells, FlowTextRole::TableCell),
                _ => break,
            };
            // Empty text must not bypass metadata admission. Each output cell
            // requires a border item even when there are no glyphs to emit.
            if cells.len() > self.options.max_items {
                return Err(FlowLayoutError::BudgetExceeded("table columns"));
            }
            if cells.len() > minimum.len() {
                minimum.resize(cells.len(), 1.0);
                preferred.resize(cells.len(), 1.0);
            }
            for (column, text) in cells.iter().enumerate() {
                let runs = if styled { meta.cell_runs.get(column).map(Vec::as_slice).unwrap_or(&[]) } else { &[] };
                let (min, ideal) = self.cell_measures(text, runs, role)?;
                minimum[column] = minimum[column].max(min);
                preferred[column] = preferred[column].max(ideal);
            }
        }
        table_column_edges(&minimum, &preferred, width)
    }

    fn cell_measures(&mut self, text: &str, runs: &[FlowInlineRun], role: FlowTextRole)
        -> Result<(f64, f64), FlowLayoutError>
    {
        // Match inline_text's unstyled fast path, including shaping across
        // adjacent default spans (otherwise ligature metrics could disagree).
        let rich = !runs.is_empty()
            && !runs.iter().all(|run| run.style == FlowInlineStyle::default() && run.link.is_none());
        if rich {
            let mut end = 0;
            for run in runs {
                if run.range.start != end || run.range.start >= run.range.end
                    || text.get(run.range.clone()).is_none()
                {
                    return Err(FlowLayoutError::InvalidShapedRun);
                }
                end = run.range.end;
            }
            if end != text.len() { return Err(FlowLayoutError::InvalidShapedRun); }
        }
        let mut minimum = 1.0_f64;
        let mut preferred = 1.0_f64;
        let mut offset = 0;
        for physical in text.split_terminator('\n') {
            if physical.len() > self.options.max_shape_bytes {
                return Err(FlowLayoutError::BudgetExceeded("bytes per table physical line"));
            }
            let mut advance = 0.0_f64;
            if rich {
                let physical_end = offset + physical.len();
                let first = runs.partition_point(|run| run.range.end <= offset);
                for spec in runs.iter().skip(first) {
                    if spec.range.start >= physical_end { break; }
                    let start = spec.range.start.max(offset);
                    let end = spec.range.end.min(physical_end);
                    if start == end { continue; }
                    let size = if spec.style.code { self.options.code_size } else { self.options.body_size };
                    let run = self.shaped_with_style(&text[start..end], size, role, spec.style)?;
                    advance += f64::from(run.total_advance);
                    for cluster in &run.clusters {
                        minimum = minimum.max(f64::from(cluster.advance()));
                    }
                }
            } else if !physical.is_empty() {
                let run = self.shaped(physical, self.options.body_size, role)?;
                advance = f64::from(run.total_advance);
                for cluster in &run.clusters {
                    minimum = minimum.max(f64::from(cluster.advance()));
                }
            }
            if !advance.is_finite() { return Err(FlowLayoutError::InvalidShapedRun); }
            preferred = preferred.max(advance);
            offset += physical.len() + 1;
        }
        Ok((minimum, preferred.max(minimum)))
    }

    fn line(&mut self) -> Result<(), FlowLayoutError> {
        if self.lines >= self.options.max_lines {
            return Err(FlowLayoutError::BudgetExceeded("lines"));
        }
        self.lines += 1;
        Ok(())
    }

    fn item(&mut self, item: DisplayItem) -> Result<(), FlowLayoutError> {
        if self.list.items().len() >= self.options.max_items {
            return Err(FlowLayoutError::BudgetExceeded("display items"));
        }
        let bounds = item.bounds();
        if [bounds.x, bounds.y, bounds.width, bounds.height, bounds.right(), bounds.bottom()]
            .iter().any(|value| !value.is_finite()) || bounds.width < 0.0 || bounds.height < 0.0
        {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        self.list.push_item(item);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, text: &str, x: f32, y: f32, width: f32, size: f32, leading: f32,
        role: FlowTextRole, color: &str, span: SourceSpan) -> Result<f32, FlowLayoutError>
    {
        let mut height = 0.0;
        // split_terminator keeps interior blank code lines but does not create
        // an extra display line for the parser's terminal code newline.
        for physical in text.split_terminator('\n') {
            if physical.is_empty() {
                self.line()?;
                height += leading;
                continue;
            }
            let measured = self.shaped(physical, size, role)?;
            let mut clusters: Vec<_> = measured.clusters.iter().collect();
            clusters.sort_by_key(|cluster| cluster.byte_range.start);
            let mut first = 0;
            while first < clusters.len() {
                // The whole-line measurement chooses a candidate, then final
                // line shaping can change boundary kerning/joining. Recheck it.
                let mut end = first;
                let mut advance = 0.0;
                let mut space_break = None;
                while end < clusters.len() {
                    let next = advance + clusters[end].advance();
                    if next > width && end > first { break; }
                    advance = next;
                    let range = clusters[end].byte_range.clone();
                    if physical[range].chars().all(|c| c == ' ' || c == '\t') {
                        space_break = Some(end + 1);
                    }
                    end += 1;
                    if next > width { break; }
                }
                if end < clusters.len() && role != FlowTextRole::Code {
                    end = space_break.unwrap_or(end);
                }
                let start_byte = clusters[first].byte_range.start;
                let run = loop {
                    let end_byte = clusters[end - 1].byte_range.end;
                    let run = self.shaped(&physical[start_byte..end_byte], size, role)?;
                    if run.total_advance <= width { break run; }
                    if end == first + 1 {
                        return Err(FlowLayoutError::ClusterTooWide { advance: run.total_advance, available: width });
                    }
                    end -= 1;
                };
                self.line()?;
                let line_x = self.line_start(x, width, run.total_advance, run.context.direction);
                let bounds = DisplayRect::new(line_x, y + height, run.total_advance, leading);
                self.item(DisplayItem::Text(DisplayTextRun {
                    bounds, text: run.logical_text.clone(), font_run: Some(run),
                    color_role: color.to_owned(), source_span: span, font_size: size,
                }))?;
                height += leading;
                first = end;
            }
        }
        if height == 0.0 { self.line()?; height = leading; }
        Ok(height)
    }

    fn marker(&mut self, meta: &BlockMeta, x: f32, y: f32) -> Result<(), FlowLayoutError> {
        let Some(marker) = &meta.marker else { return Ok(()); };
        let bounds = DisplayRect::new((x - 20.0).max(0.0), y, 16.0, self.options.line_height);
        if let Some(checked) = meta.task {
            self.vector(bounds, VectorShapeType::CheckboxOutline, "border", meta.span)?;
            if checked { self.vector(bounds, VectorShapeType::CheckboxCheck, "accent", meta.span)?; }
        } else {
            let run = self.shaped(marker, self.options.body_size, FlowTextRole::Marker)?;
            if run.total_advance > x - 4.0 {
                return Err(FlowLayoutError::ClusterTooWide { advance: run.total_advance, available: (x - 4.0).max(0.0) });
            }
            self.item(DisplayItem::Text(DisplayTextRun {
                bounds: DisplayRect::new(x - 4.0 - run.total_advance, y, run.total_advance, self.options.line_height),
                text: marker.clone(), font_run: Some(run), color_role: "text".to_owned(),
                source_span: meta.span, font_size: self.options.body_size,
            }))?;
        }
        Ok(())
    }

    fn vector(&mut self, bounds: DisplayRect, shape: VectorShapeType, color: &str, span: SourceSpan) -> Result<(), FlowLayoutError> {
        self.item(DisplayItem::Vector(DisplayVectorPath {
            bounds, shape, stroke_width: 1.0, color_role: color.to_owned(), source_span: span,
        }))
    }

    fn reading(&mut self, role: AccessibleReadingRole, text: &str, bounds: DisplayRect, span: SourceSpan) {
        self.list.push_reading_node(AccessibleReadingNode {
            role, text: text.to_owned(), source_span: span, bounds, children: Vec::new(),
        });
    }
}

/// Deterministic capped water filling. Compact columns are capped at their
/// preferred width before narrative columns share the remaining width. Lower
/// bounds preserve whole shaped clusters plus padding. Absolute shared edges
/// avoid accumulating independently rounded widths differently in each row.
fn table_column_edges(minimum: &[f64], preferred: &[f64], width: f32)
    -> Result<Vec<f32>, FlowLayoutError>
{
    if minimum.is_empty() { return Ok(Vec::new()); }
    let padding = 8.0_f64;
    let lower: Vec<_> = minimum.iter().map(|value| value + padding).collect();
    let upper: Vec<_> = preferred.iter().zip(&lower)
        .map(|(value, min)| (value + padding).max(*min)).collect();
    let min_sum: f64 = lower.iter().sum();
    let preferred_sum: f64 = upper.iter().sum();
    let available = f64::from(width);
    if !min_sum.is_finite() || !preferred_sum.is_finite() {
        return Err(FlowLayoutError::InvalidShapedRun);
    }
    if min_sum > available {
        return Err(FlowLayoutError::TableTooNarrow { minimum: min_sum as f32, available: width });
    }
    let widths = if preferred_sum <= available {
        let extra = (available - preferred_sum) / lower.len() as f64;
        upper.iter().map(|value| value + extra).collect::<Vec<_>>()
    } else {
        let mut lo = 0.0;
        let mut hi = available;
        for _ in 0..64 {
            let level = (lo + hi) * 0.5;
            let used: f64 = lower.iter().zip(&upper).map(|(min, max)| level.clamp(*min, *max)).sum();
            if used <= available { lo = level; } else { hi = level; }
        }
        lower.iter().zip(&upper).map(|(min, max)| lo.clamp(*min, *max)).collect::<Vec<_>>()
    };
    let mut edges = Vec::with_capacity(widths.len() + 1);
    edges.push(0.0);
    let mut edge = 0.0;
    for (index, allocated) in widths.iter().enumerate() {
        edge += allocated;
        let right = if index + 1 == widths.len() { width } else { edge as f32 };
        if right - edges[index] <= padding as f32 {
            return Err(FlowLayoutError::TableTooNarrow { minimum: min_sum as f32, available: width });
        }
        edges.push(right);
    }
    Ok(edges)
}

fn validate_run(run: &OwnedTextRun, text: &str, size: f32) -> Result<(), FlowLayoutError> {
    if run.logical_text != text || run.context.font_size != size
        || run.clusters.len() > text.len()
        || !run.total_advance.is_finite() || run.total_advance < 0.0
    {
        return Err(FlowLayoutError::InvalidShapedRun);
    }
    let mut clusters: Vec<_> = run.clusters.iter().collect();
    clusters.sort_by_key(|cluster| cluster.byte_range.start);
    let mut end = 0;
    let mut utf16_end = 0;
    let mut glyph_count = 0;
    let mut advance = 0.0;
    for cluster in clusters {
        let range = &cluster.byte_range;
        if range.start != end || range.start >= range.end || text.get(range.clone()).is_none()
            || !cluster.x_start.is_finite() || !cluster.x_end.is_finite()
            || cluster.glyph_range.start >= cluster.glyph_range.end
            || run.glyphs.get(cluster.glyph_range.clone()).is_none()
            || run.clusters.get(cluster.cluster_index) != Some(cluster)
        {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        let utf16_start = utf16_end;
        utf16_end += text[range.clone()].encode_utf16().count();
        if cluster.utf16_range != (utf16_start..utf16_end) {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        let mut glyph_advance = 0.0;
        for glyph in &run.glyphs[cluster.glyph_range.clone()] {
            if glyph.cluster_index != cluster.cluster_index || glyph.font_id != cluster.font_id
                || [glyph.x_advance, glyph.y_advance, glyph.x_offset, glyph.y_offset]
                    .iter().any(|value| !value.is_finite())
            {
                return Err(FlowLayoutError::InvalidShapedRun);
            }
            glyph_advance += glyph.x_advance;
        }
        if (glyph_advance - cluster.advance()).abs() > 0.01 + cluster.advance() * 0.00001 {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        glyph_count += cluster.glyph_range.len();
        end = range.end;
        advance += cluster.advance();
    }
    if end != text.len() || glyph_count != run.glyphs.len() || !advance.is_finite()
        || (advance - run.total_advance).abs() > 0.01 + run.total_advance * 0.00001
    {
        return Err(FlowLayoutError::InvalidShapedRun);
    }
    // Logical ranges can be RTL while visual ranges run in the opposite order.
    // Check visual coverage independently without changing glyph presentation.
    let mut visual: Vec<_> = run.clusters.iter().collect();
    visual.sort_by(|left, right| left.x_start.total_cmp(&right.x_start)
        .then_with(|| left.x_end.total_cmp(&right.x_end)));
    let mut x = 0.0;
    for cluster in visual {
        if cluster.x_end < cluster.x_start
            || (cluster.x_start - x).abs() > 0.01 + run.total_advance * 0.00001
        {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        x = cluster.x_end;
    }
    if (x - run.total_advance).abs() > 0.01 + run.total_advance * 0.00001 {
        return Err(FlowLayoutError::InvalidShapedRun);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::text::{FontId, FontOrigin, RunGlyph, TextCluster, TextRunContext};

    // Deterministic measurement oracle, not a substitute for a real font. `fi`
    // and combining marks share a cluster so forbidden splits are observable.
    pub(super) fn measured(text: &str, size: f32, _role: FlowTextRole) -> Result<OwnedTextRun, String> {
        let context = TextRunContext {
            font_id: FontId::new(1), font_size: size, script: *b"latn", language: *b"dflt",
            direction: Direction::LeftToRight, font_origin: FontOrigin::BundledFace,
        };
        let mut run = OwnedTextRun {
            context, logical_text: text.to_owned(), clusters: Vec::new(),
            glyphs: Vec::new(), total_advance: 0.0,
        };
        let mut chars = text.char_indices().peekable();
        let mut utf16 = 0;
        while let Some((start, ch)) = chars.next() {
            let mut end = start + ch.len_utf8();
            if ch == 'f' && chars.peek().is_some_and(|(_, c)| *c == 'i') {
                let (index, c) = chars.next().unwrap();
                end = index + c.len_utf8();
            }
            while chars.peek().is_some_and(|(_, c)| ('\u{0300}'..='\u{036f}').contains(c)) {
                let (index, c) = chars.next().unwrap();
                end = index + c.len_utf8();
            }
            let index = run.clusters.len();
            let units = text[start..end].encode_utf16().count();
            run.clusters.push(TextCluster {
                cluster_index: index, byte_range: start..end, utf16_range: utf16..utf16 + units,
                glyph_range: index..index + 1, x_start: run.total_advance,
                x_end: run.total_advance + 10.0, font_id: FontId::new(1),
            });
            run.glyphs.push(RunGlyph {
                glyph_id: 1, font_id: FontId::new(1), cluster_index: index,
                x_advance: 10.0, y_advance: 0.0, x_offset: 0.0, y_offset: 0.0,
            });
            utf16 += units;
            run.total_advance += 10.0;
        }
        Ok(run)
    }

    fn engine(source: &str) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, 8);
        engine.process_all().unwrap();
        engine
    }

    fn text_items(list: &DisplayList) -> Vec<&DisplayTextRun> {
        list.items().iter().filter_map(|item| match item { DisplayItem::Text(run) => Some(run), _ => None }).collect()
    }

    fn narrow(width: f32) -> FlowLayoutOptions {
        FlowLayoutOptions { viewport_width: width, ..FlowLayoutOptions::default() }
    }

    #[test]
    fn shaped_wrapping_uses_measured_widths_and_retains_hit_testable_runs() {
        let engine = engine("alpha beta gamma");
        let list = engine.to_shaped_display_list(narrow(60.0), measured).unwrap();
        let runs = text_items(&list);
        let text: Vec<_> = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(text, ["alpha ", "beta ", "gamma"]);
        for (index, run) in runs.iter().enumerate() {
            assert_eq!(run.bounds.y, index as f32 * 20.0);
            assert!(run.bounds.right() <= 60.0);
            let shaped = run.font_run.as_ref().unwrap();
            assert_eq!(shaped.logical_text, run.text);
            assert_eq!(shaped.total_advance, run.bounds.width);
            assert_eq!(shaped.hit_test(0.0).caret.byte_offset, 0);
            assert!(!run.source_span.is_empty());
        }
        assert_eq!(list.reading_order()[0].text, "alpha beta gamma");
        assert_eq!(list.reading_order()[0].bounds.height, 60.0);
    }

    #[test]
    fn wrapping_never_splits_ligatures_combining_marks_or_multibyte_scalars() {
        for source in ["fi fi", "a\u{0301}b", "東京", "😀é"] {
            let engine = engine(source);
            let list = engine.to_shaped_display_list(narrow(10.0), measured).unwrap();
            let joined = text_items(&list).iter().map(|run| run.text.as_str()).collect::<String>();
            assert_eq!(joined, source);
            for run in text_items(&list) {
                assert_eq!(run.font_run.as_ref().unwrap().clusters.len(), 1);
                assert!(!run.text.starts_with('\u{0301}'));
                assert_ne!(run.text, "f");
                assert_ne!(run.text, "i");
            }
        }
    }

    #[test]
    fn final_lines_are_reshaped_and_rechecked_for_boundary_metric_changes() {
        let engine = engine("abcdef");
        let list = engine.to_shaped_display_list(narrow(10.0), |text, size, role| {
            let mut run = measured(text, size, role)?;
            // Boundary shaping differs from the initial paragraph measurement.
            if text.len() == 6 {
                for cluster in &mut run.clusters { cluster.x_start *= 0.3; cluster.x_end *= 0.3; }
                for glyph in &mut run.glyphs { glyph.x_advance *= 0.3; }
                run.total_advance *= 0.3;
            }
            Ok(run)
        }).unwrap();
        let runs = text_items(&list);
        assert_eq!(runs.len(), 6);
        assert!(runs.iter().all(|run| run.bounds.width <= 10.0));
        assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "abcdef");
        assert!(matches!(engine.to_shaped_display_list(narrow(9.0), measured),
            Err(FlowLayoutError::ClusterTooWide { .. })));
    }

    #[test]
    fn bad_options_shaper_failures_and_invalid_clusters_are_not_silent_fallbacks() {
        let engine = engine("abba");
        for width in [0.0, -1.0, f32::INFINITY, f32::NAN] {
            assert!(matches!(engine.to_shaped_display_list(narrow(width), measured), Err(FlowLayoutError::InvalidOptions)));
        }
        assert!(matches!(engine.to_shaped_display_list(narrow(80.0), |_, _, _| Err("missing font".to_owned())),
            Err(FlowLayoutError::Shaping(_))));
        for corruption in 0..4 {
            let result = engine.to_shaped_display_list(narrow(80.0), |text, size, role| {
                let mut run = measured(text, size, role)?;
                match corruption {
                    0 => run.logical_text.push('!'),
                    1 => run.clusters[0].utf16_range = 2..3,
                    2 => run.clusters[0].byte_range = 1..2,
                    _ => run.glyphs[0].x_offset = f32::NAN,
                }
                Ok(run)
            });
            assert!(matches!(result, Err(FlowLayoutError::InvalidShapedRun)));
        }
    }

    #[test]
    fn shape_work_and_output_budgets_are_enforced_without_mutating_engine() {
        let engine = engine("alpha beta");
        let before = engine.to_display_list();
        for options in [
            FlowLayoutOptions { max_shape_bytes: 1, ..narrow(50.0) },
            FlowLayoutOptions { max_total_shape_bytes: 10, ..narrow(50.0) },
            FlowLayoutOptions { max_lines: 1, ..narrow(50.0) },
            FlowLayoutOptions { max_items: 1, ..narrow(50.0) },
        ] {
            assert!(matches!(engine.to_shaped_display_list(options, measured), Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
    }

    #[test]
    fn table_row_heights_follow_wrapped_cells_and_keep_accessible_headers() {
        let engine = engine("| A | B |\n| --- | --- |\n| long cell text | tiny |\n");
        let list = engine.to_shaped_display_list(narrow(120.0), measured).unwrap();
        let header = &list.reading_order()[0];
        let row = &list.reading_order()[1];
        assert_eq!(header.role, AccessibleReadingRole::TableHeaderRow);
        assert_eq!(header.children[0].role, AccessibleReadingRole::TableHeaderCell);
        assert!(row.bounds.height > header.bounds.height);
        assert!(row.children.iter().all(|cell| cell.bounds.height == row.bounds.height));
        assert!(text_items(&list).iter().all(|run| run.bounds.right() <= 120.0));
        assert_eq!(row.bounds.y, header.bounds.bottom());
    }

    #[test]
    fn resolved_images_fit_viewport_preserve_aspect_ratio_and_reflow_followers() {
        let mut engine = engine("![picture](image.png)\n\n# After");
        let request = engine.unresolved_assets()[0].clone();
        engine.provide_asset(super::super::AssetResult {
            request_id: request.id, generation: request.generation,
            width: 640, height: 480, bytes: None,
        }).unwrap();
        let list = engine.to_shaped_display_list(narrow(100.0), measured).unwrap();
        let DisplayItem::Image(image) = &list.items()[0] else { panic!("expected image"); };
        assert_eq!(image.bounds.width, 100.0);
        assert_eq!(image.bounds.height, 75.0);
        assert!(image.is_resolved);
        assert_eq!(list.reading_order()[1].bounds.y, 85.0);
        assert_eq!(list.anchors().next().unwrap().anchor_id, "after");
    }

    #[test]
    fn shaped_code_preserves_whitespace_and_blank_line_height() {
        let engine = engine("```text\n  a\tb\n\nc\n```");
        let list = engine.to_shaped_display_list(narrow(40.0), measured).unwrap();
        assert_eq!(text_items(&list).iter().map(|run| run.text.as_str()).collect::<String>(), "  a\tbc");
        assert_eq!(list.reading_order()[0].text, "  a\tb\n\nc\n");
        // Two wrapped first-line segments, one blank line, one last line.
        assert_eq!(list.reading_order()[0].bounds.height, 80.0);
        assert!(text_items(&list).iter().all(|run| run.color_role == "code"));
    }

    #[test]
    fn native_font_contract_shapes_real_glyphs_into_display_output() {
        use crate::text::{Font, shaping::ShapeOptions};
        const BYTES: &[u8] = include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf");
        let font = Font::parse(BYTES.to_vec()).unwrap();
        let font_id = FontId::from_font_data(BYTES);
        let shape = |text: &str, size: f32, _role: FlowTextRole| {
            let shaped = font.shape(text, &ShapeOptions::default()).map_err(|err| err.to_string())?;
            OwnedTextRun::from_shaped_run(TextRunContext {
                font_id, font_size: size, script: *b"latn", language: *b"dflt",
                direction: Direction::LeftToRight, font_origin: FontOrigin::BundledFace,
            }, &shaped, size / f32::from(font.units_per_em))
        };
        let width = shape("abba", 14.0, FlowTextRole::Body).unwrap().total_advance;
        assert!(width > 0.0);
        let engine = engine("abba abba abba");
        let list = engine.to_shaped_display_list(narrow(width), shape).unwrap();
        let runs = text_items(&list);
        assert!(runs.len() > 1);
        assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "abba abba abba");
        for item in runs {
            let run = item.font_run.as_ref().unwrap();
            assert_eq!(run.context.font_id, font_id);
            assert!(run.glyphs.iter().all(|glyph| glyph.font_id == font_id));
            assert!(run.total_advance <= width);
        }
    }

    #[test]
    fn rtl_host_runs_keep_logical_text_and_right_align_each_wrapped_line() {
        let engine = engine("אבג דהו");
        let list = engine.to_shaped_display_list(narrow(40.0), |text, size, role| {
            let mut run = measured(text, size, role)?;
            run.context.direction = Direction::RightToLeft;
            for cluster in &mut run.clusters {
                let start = cluster.x_start;
                cluster.x_start = run.total_advance - cluster.x_end;
                cluster.x_end = run.total_advance - start;
            }
            Ok(run)
        }).unwrap();
        let runs = text_items(&list);
        assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "אבג דהו");
        assert!(runs.len() > 1);
        for run in runs {
            assert_eq!(run.bounds.right(), 40.0);
            assert_eq!(run.font_run.as_ref().unwrap().context.direction, Direction::RightToLeft);
        }
    }

    #[test]
    fn shaped_reflow_is_independent_of_source_and_output_batch_boundaries() {
        let source = "## Title\n\n- first item\n- second item\n\n![x](image.png)\n\nA long paragraph.\n";
        let whole = engine(source);
        let expected = whole.to_shaped_display_list(narrow(100.0), measured).unwrap();
        for batch in [1, 2, 3, 8, usize::MAX] {
            let mut stepped = ResumableFlowDisplay::new(source, batch);
            stepped.process_all().unwrap();
            assert_eq!(stepped.to_shaped_display_list(narrow(100.0), measured).unwrap(), expected);
        }
    }

    #[test]
    fn table_columns_give_narrative_space_without_wasting_it_on_ids() {
        let source = "| ID | Description |\n| --- | --- |\n| 7 | a much longer narrative description |\n| 42 | short |\n";
        let list = engine(source).to_shaped_display_list(narrow(200.0), measured).unwrap();
        let header = &list.reading_order()[0];
        assert_eq!(header.children[0].bounds.width, 28.0);
        assert_eq!(header.children[1].bounds.width, 172.0);
        for row in list.reading_order() {
            assert_eq!(row.children[0].bounds.x, 0.0);
            assert_eq!(row.children[0].bounds.width, 28.0);
            assert_eq!(row.children[1].bounds.x, 28.0);
            assert_eq!(row.children[1].bounds.right(), 200.0);
            assert!(row.children.iter().all(|cell| cell.bounds.height == row.bounds.height));
        }
        assert!(text_items(&list).iter().any(|run| run.text == "42"));
    }

    #[test]
    fn table_columns_include_unpublished_continuations_and_are_batch_stable() {
        let source = "| A | B |\n| --- | --- |\n| x | a long narrative |\n| wide identifier | x |\n";
        let complete = engine(source).to_shaped_display_list(narrow(200.0), measured).unwrap();
        let expected = &complete.reading_order()[0].children;
        let mut stepped = ResumableFlowDisplay::new(source, 1);
        while stepped.blocks().is_empty() { stepped.step().unwrap(); }
        assert_eq!(stepped.blocks().len(), 1);
        let partial = stepped.to_shaped_display_list(narrow(200.0), measured).unwrap();
        assert_eq!(&partial.reading_order()[0].children, expected);
        stepped.process_all().unwrap();
        assert_eq!(stepped.to_shaped_display_list(narrow(200.0), measured).unwrap(), complete);
    }

    #[test]
    fn table_columns_do_not_leak_between_adjacent_tables_or_nested_blocks() {
        let source = "| ID | Long description |\n| --- | --- |\n| 1 | narrative text |\n\n> | Long description | ID |\n> | --- | --- |\n> | narrative text | 1 |\n";
        let list = engine(source).to_shaped_display_list(narrow(200.0), measured).unwrap();
        let headers: Vec<_> = list.reading_order().iter()
            .filter(|node| node.role == AccessibleReadingRole::TableHeaderRow).collect();
        assert_eq!(headers.len(), 2);
        assert!(headers[0].children[0].bounds.width < headers[0].children[1].bounds.width);
        assert!(headers[1].children[0].bounds.width > headers[1].children[1].bounds.width);
        assert!(headers[1].bounds.x > headers[0].bounds.x);
        assert_eq!(headers[1].children[1].bounds.right(), 200.0);
    }

    #[test]
    fn table_measurement_uses_inline_faces_and_preserves_link_geometry() {
        let source = "| ID | Description |\n| --- | --- |\n| **WW** | [a long description](https://example.com) |\n";
        let engine = engine(source);
        let plain = engine.to_shaped_display_list(narrow(200.0), measured).unwrap();
        let rich = engine.to_styled_display_list(narrow(200.0), |text, size, role, style| {
            let mut run = measured(text, size, role)?;
            if style.bold {
                for cluster in &mut run.clusters { cluster.x_start *= 2.0; cluster.x_end *= 2.0; }
                for glyph in &mut run.glyphs { glyph.x_advance *= 2.0; }
                run.total_advance *= 2.0;
            }
            Ok(run)
        }).unwrap();
        assert_eq!(plain.reading_order()[0].children[0].bounds.width, 28.0);
        assert_eq!(rich.reading_order()[0].children[0].bounds.width, 48.0);
        let cell = &rich.reading_order()[1].children[1];
        for anchor in rich.anchors().filter(|anchor| !anchor.is_heading) {
            assert!(anchor.bounds.x >= cell.bounds.x + 4.0);
            assert!(anchor.bounds.right() <= cell.bounds.right() - 4.0);
            assert!(anchor.bounds.bottom() <= cell.bounds.bottom());
        }
        assert!(rich.anchors().any(|anchor| anchor.anchor_id == "https://example.com"));
    }

    #[test]
    fn table_measurement_failure_is_atomic_and_charges_the_shared_budget() {
        let engine = engine("| A | B |\n| --- | --- |\n| x | y |\n");
        let before = engine.to_display_list();
        let options = FlowLayoutOptions { max_total_shape_bytes: 3, ..narrow(200.0) };
        assert!(matches!(engine.to_shaped_display_list(options, measured),
            Err(FlowLayoutError::BudgetExceeded("total shaping bytes"))));
        assert!(matches!(engine.to_shaped_display_list(narrow(35.0), measured),
            Err(FlowLayoutError::TableTooNarrow { .. })));
        assert_eq!(engine.to_display_list(), before);
        assert!(engine.to_shaped_display_list(narrow(200.0), measured).is_ok());
    }

    #[test]
    fn column_water_filling_preserves_minima_and_exact_outer_edges() {
        for width in [36.0, 40.0, 60.0, 120.0, 200.0, 1000.0] {
            let edges = table_column_edges(&[10.0, 10.0], &[20.0, 150.0], width).unwrap();
            assert_eq!(edges[0], 0.0);
            assert_eq!(*edges.last().unwrap(), width);
            assert!(edges.windows(2).all(|pair| pair[1] - pair[0] >= 18.0));
            assert_eq!(edges, table_column_edges(&[10.0, 10.0], &[20.0, 150.0], width).unwrap());
        }
        assert_eq!(table_column_edges(&[10.0, 10.0], &[20.0, 150.0], 120.0).unwrap(),
            [0.0, 28.0, 120.0]);
    }

    #[test]
    fn gfm_alignment_positions_headers_and_body_within_the_shared_grid() {
        let source = "| L | C | R |\n| :--- | :---: | ---: |\n| a | b | c |\n\nafter";
        let list = engine(source).to_shaped_display_list(narrow(300.0), measured).unwrap();
        for (text, x) in [("L", 4.0), ("C", 145.0), ("R", 286.0),
            ("a", 4.0), ("b", 145.0), ("c", 286.0), ("after", 0.0)] {
            let run = text_items(&list).into_iter().find(|run| run.text == text).unwrap();
            assert_eq!(run.bounds.x, x, "{text}");
        }
        for row in &list.reading_order()[..2] {
            assert_eq!(row.children.iter().map(|cell| cell.bounds.x).collect::<Vec<_>>(), [0.0, 100.0, 200.0]);
            assert!(row.children.iter().all(|cell| cell.bounds.width == 100.0));
        }
    }

    #[test]
    fn alignment_uses_each_final_wrapped_line_without_losing_clusters() {
        for (delimiter, align) in [(":---", Align::Left), (":---:", Align::Center), ("---:", Align::Right)] {
            let source = format!("| H |\n| {delimiter} |\n| fi a\u{0301} 東京 😀 |\n");
            let engine = engine(&source);
            let list = engine.to_shaped_display_list(narrow(53.0), measured).unwrap();
            let body = &list.reading_order()[1];
            let runs: Vec<_> = text_items(&list).into_iter()
                .filter(|run| run.bounds.y >= body.bounds.y).collect();
            assert!(runs.len() > 1);
            assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "fi a\u{0301} 東京 😀");
            for run in runs {
                let expected = match align {
                    Align::Left => 4.0,
                    Align::Center => 4.0 + (45.0 - run.bounds.width) * 0.5,
                    Align::Right => 49.0 - run.bounds.width,
                    Align::None => unreachable!(),
                };
                assert_eq!(run.bounds.x, expected);
                assert!(run.bounds.right() <= 49.0);
                assert!(!run.text.starts_with('\u{0301}'));
                assert_ne!(run.text, "f");
                assert_ne!(run.text, "i");
            }
        }
    }

    #[test]
    fn explicit_table_alignment_overrides_direction_but_unset_keeps_rtl_start() {
        for (delimiter, expected) in [(":---", 4.0), (":---:", 40.0), ("---:", 76.0), ("---", 76.0)] {
            let source = format!("| H |\n| {delimiter} |\n| אב |\n");
            let list = engine(&source).to_shaped_display_list(narrow(100.0), |text, size, role| {
                let mut run = measured(text, size, role)?;
                run.context.direction = Direction::RightToLeft;
                for cluster in &mut run.clusters {
                    let start = cluster.x_start;
                    cluster.x_start = run.total_advance - cluster.x_end;
                    cluster.x_end = run.total_advance - start;
                }
                Ok(run)
            }).unwrap();
            let run = text_items(&list).into_iter().find(|run| run.text == "אב").unwrap();
            assert_eq!(run.bounds.x, expected);
            assert_eq!(run.font_run.as_ref().unwrap().context.direction, Direction::RightToLeft);
        }
    }
}
