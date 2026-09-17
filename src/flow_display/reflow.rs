//! Width-aware display layout over owned, host-shaped text runs.
//!
//! Hosts supply the existing fmd-font/native-route OwnedTextRun contract. This
//! module does not select system fonts, perform I/O, or substitute an estimated
//! width when shaping fails. Wrapping is greedy at spaces, with cluster-boundary
//! emergency breaks; it is not the PDF Knuth-Plass paragraph engine.

use super::{BlockMeta, DisplayBlock, FlowDisplayError, FlowInlineStyle, ResumableFlowDisplay};

mod styled;
use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayImage, DisplayItem, DisplayList,
    DisplayRect, DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
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
/// reshaping at line boundaries. The same ceiling separately bounds retained
/// link-target bytes in styled output. Hosts must bound their shaper's execution.
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
        for (block, meta) in self.blocks.iter().zip(&self.metadata) {
            let x = f32::from(meta.list_depth) * 20.0 + f32::from(meta.quote_depth) * 16.0;
            let width = options.viewport_width - x;
            if width <= 0.0 { return Err(FlowLayoutError::InvalidOptions); }
            let span = meta.span;
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
                    let cell_width = width / cells.len().max(1) as f32;
                    if cell_width <= 8.0 { return Err(FlowLayoutError::InvalidOptions); }
                    let mut height = options.line_height;
                    let mut children = Vec::new();
                    for (column, text) in cells.iter().enumerate() {
                        let cx = x + column as f32 * cell_width;
                        let runs = if styled { meta.cell_runs.get(column).map(Vec::as_slice).unwrap_or(&[]) } else { &[] };
                        let used = layout.inline_text(text, runs, cx + 4.0, y + 4.0, cell_width - 8.0,
                            options.body_size, options.line_height,
                            if header { FlowTextRole::TableHeader } else { FlowTextRole::TableCell },
                            if header { "heading" } else { "text" }, span)?;
                        height = height.max(used);
                        children.push(AccessibleReadingNode {
                            role: if header { AccessibleReadingRole::TableHeaderCell } else { AccessibleReadingRole::TableCell },
                            text: text.clone(), source_span: span,
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
        Ok(Self { list: DisplayList::new(), options, shape, lines: 0, shape_bytes: 0, link_bytes: 0 })
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
                let line_x = if run.context.direction == Direction::RightToLeft {
                    x + width - run.total_advance
                } else { x };
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
}
