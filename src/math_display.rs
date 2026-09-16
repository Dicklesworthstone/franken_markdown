#![forbid(unsafe_code)]

//! Bridge from `fmd-math` layout output and first-party diagram syntax to the
//! renderer-neutral display list (FCB-034.A).
//!
//! Converts positioned math glyphs, rules, and drawn paths as well as diagram
//! boxes, connectors, arrows, and labels into [`DisplayItem`]s that any renderer
//! can consume without knowing about TeX, Mermaid, Metal, or AppKit.
//!
//! Source spans survive: every display item carries the byte span of the source
//! construct that produced it.
//!
//! Non-negotiable safety contracts:
//! - No external TeX engine, no JavaScript, no ambient scripting.
//! - Diagram output flows through qualified first-party vector parsing/geometry,
//!   not executing embedded script or arbitrary markup.
//! - Unsupported elements display a source-preserving fallback and a concise
//!   capability explanation.

use crate::display::{
    DisplayItem, DisplayRect, DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath,
    VectorShapeType,
};
use crate::span::SourceSpan;
use fmd_math::Engine;
use fmd_math::Layout;

// ---------------------------------------------------------------------------
// Math Display Types & Bridge
// ---------------------------------------------------------------------------

/// Errors occurring during math display validation or parsing.
#[derive(Clone, Debug, PartialEq)]
pub enum MathDisplayError {
    /// The math source failed to parse.
    ParseError(String),
    /// A required source span was missing or reversed.
    SourceAnchor(String),
    /// Layout or path resolution failed.
    LayoutError(String),
}

impl std::fmt::Display for MathDisplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(s) => write!(f, "math parse error: {s}"),
            Self::SourceAnchor(s) => write!(f, "source anchor error: {s}"),
            Self::LayoutError(s) => write!(f, "layout error: {s}"),
        }
    }
}

impl std::error::Error for MathDisplayError {}

/// A resolved math display descriptor: the source text and its document offset.
#[derive(Clone, Debug, PartialEq)]
pub struct MathDisplay {
    /// The raw math source text (e.g. the contents of `$...$`).
    pub source: String,
    /// The byte offset of `source` within the parent document.
    pub source_offset: usize,
}

impl MathDisplay {
    /// Create a math display from source text and its document offset.
    ///
    /// The math is parsed eagerly to validate; parse errors are surfaced
    /// immediately rather than during rendering.
    pub fn new(source: &str, offset: usize) -> Result<Self, MathDisplayError> {
        let _node = fmd_math::parse(source)
            .map_err(|e| MathDisplayError::ParseError(e.to_string()))?;
        Ok(Self {
            source: source.to_owned(),
            source_offset: offset,
        })
    }

    /// The byte length of the math source text.
    pub fn source_len(&self) -> usize {
        self.source.len()
    }
}

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
    _source: &str,
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
        // from top-left.
        let dy = (origin_y + font_size - (glyph.y as f32) * font_size).round();
        let size = ((glyph.size as f32) * font_size).round().max(1.0);

        let span = source_offset_span(source_offset, glyph.span.start, glyph.span.end);
        let text_run = DisplayTextRun {
            bounds: glyph_bounds(dx, dy, glyph.ch, size),
            text: glyph.ch.to_string(),
            font_run: None, // host supplies resolved font runs
            color_role: "math".to_string(),
            source_span: span,
            font_size: size,
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

/// Extract `$...$` (inline) and `$$...$$` (display) math regions from source Markdown.
///
/// Returns tuples of `(math_source, byte_offset)`.
pub fn extract_math_spans(source: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;

    while pos < len {
        if let Some(&b'$') = bytes.get(pos) {
            // Display math: $$...$$
            if pos + 1 < len && bytes.get(pos + 1) == Some(&b'$') {
                if let Some(rest) = source.get(pos + 2..) {
                    if let Some(end) = rest.find("$$") {
                        if let Some(inner) = rest.get(..end) {
                            if !inner.is_empty() {
                                out.push((inner.to_owned(), pos + 2));
                            }
                        }
                        pos = pos + 2 + end + 2;
                        continue;
                    }
                }
            }
            // Inline math: $...$ (not $$)
            if let Some(rest) = source.get(pos + 1..) {
                if let Some(end) = rest.find('$') {
                    if let Some(inner) = rest.get(..end) {
                        if !inner.is_empty() && !inner.contains('$') {
                            out.push((inner.to_owned(), pos + 1));
                        }
                    }
                    pos = pos + 1 + end + 1;
                    continue;
                }
            }
        }
        pos += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// Diagram Display Types & First-Party Vector Bridge
// ---------------------------------------------------------------------------

/// Errors produced during diagram display parsing or validation.
#[derive(Clone, Debug, PartialEq)]
pub enum DiagramError {
    /// Forbidden executable script or active markup was detected.
    HostileContent(String),
    /// The diagram dialect or language is not supported for native vector layout.
    UnsupportedLanguage(String),
    /// The diagram source could not be parsed into valid vector geometry.
    ParseError(String),
    /// Bounded safety limit exceeded (e.g. node count or nesting depth).
    LimitExceeded(String),
}

impl std::fmt::Display for DiagramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostileContent(s) => write!(f, "hostile content rejected: {s}"),
            Self::UnsupportedLanguage(s) => write!(f, "unsupported diagram language: {s}"),
            Self::ParseError(s) => write!(f, "diagram parse error: {s}"),
            Self::LimitExceeded(s) => write!(f, "diagram limit exceeded: {s}"),
        }
    }
}

impl std::error::Error for DiagramError {}

/// A diagram block reference with its display metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramDisplay {
    /// The diagram language tag (e.g. "mermaid", "flowchart").
    pub language: String,
    /// The diagram source text.
    pub source: String,
    /// Byte offset in the parent document.
    pub source_offset: usize,
}

impl DiagramDisplay {
    /// Create a diagram display from a fenced block's language, source, and offset.
    pub fn new(language: &str, source: &str, offset: usize) -> Self {
        Self {
            language: language.to_owned(),
            source: source.to_owned(),
            source_offset: offset,
        }
    }
}

/// Check for forbidden active scripts, events, and unsafe markup.
pub fn contains_hostile_markup(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.contains("<script")
        || lower.contains("</script")
        || lower.contains("javascript:")
        || lower.contains("vbscript:")
        || lower.contains("data:text/html")
        || lower.contains("onload=")
        || lower.contains("onerror=")
        || lower.contains("onclick=")
        || lower.contains("<iframe")
        || lower.contains("<object")
        || lower.contains("<embed")
        || lower.contains("<foreignobject")
        || lower.contains("eval(")
        || lower.contains("document.cookie")
}

/// Check whether a code fence language tag represents a diagram dialect.
pub fn is_diagram_language(lang: &str) -> bool {
    let lower = lang.to_ascii_lowercase();
    let name = lower.split(',').next().unwrap_or("").trim();
    matches!(
        name,
        "mermaid"
            | "flowchart"
            | "graph"
            | "diagram"
            | "dot"
            | "plantuml"
            | "d2"
            | "ascii-diagram"
            | "ascii"
    )
}

/// Extract fenced diagram blocks (```lang ... ```) from Markdown source.
pub fn extract_diagram_blocks(source: &str) -> Vec<DiagramDisplay> {
    let mut out = Vec::new();
    let mut lines = source.lines().peekable();
    let mut offset = 0usize;

    while let Some(line) = lines.next() {
        let line_len = line.len() + 1; // +1 for \n
        let trimmed = line.trim();
        if trimmed.starts_with("```") && trimmed.len() > 3 {
            let tag = trimmed.trim_start_matches('`').trim();
            if is_diagram_language(tag) {
                let content_start = offset + line_len;
                let mut content = Vec::new();
                let mut closed = false;
                for inner in lines.by_ref() {
                    offset += inner.len() + 1;
                    if inner.trim() == "```" {
                        closed = true;
                        break;
                    }
                    content.push(inner);
                }
                if closed {
                    let block_source = content.join("\n");
                    out.push(DiagramDisplay::new(tag, &block_source, content_start));
                }
            }
        }
        offset += line_len;
    }
    out
}

/// Convert a diagram source block into renderer-neutral display items using first-party vector forms.
///
/// Rejects hostile scripts and returns a typed `DiagramError`.
pub fn diagram_to_display(
    language: &str,
    source: &str,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Result<Vec<DisplayItem>, DiagramError> {
    if contains_hostile_markup(source) {
        return Err(DiagramError::HostileContent(
            "active scripts or executable markup forbidden in diagram source".to_string(),
        ));
    }

    let clean_lang = language.split(',').next().unwrap_or("").trim().to_ascii_lowercase();
    match clean_lang.as_str() {
        "mermaid" | "flowchart" | "graph" | "diagram" | "" => {
            parse_flowchart_vector(source, origin_x, origin_y, font_size, source_offset)
        }
        "ascii-diagram" | "ascii" => {
            parse_ascii_diagram_vector(source, origin_x, origin_y, font_size, source_offset)
        }
        other => Err(DiagramError::UnsupportedLanguage(format!(
            "diagram dialect '{other}' is not supported for native vector rendering"
        ))),
    }
}

/// Convert a diagram source block into display items, falling back to a source-preserving
/// display with capability explanation when vector parsing cannot be performed.
///
/// Preserves source anchors and authoritative source bytes. Never panics or executes code.
pub fn diagram_to_display_with_fallback(
    language: &str,
    source: &str,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Vec<DisplayItem> {
    match diagram_to_display(language, source, origin_x, origin_y, font_size, source_offset) {
        Ok(items) => items,
        Err(err) => {
            let explanation = match err {
                DiagramError::HostileContent(_) => {
                    "forbidden executable script or markup detected - rendered as source fallback"
                }
                DiagramError::UnsupportedLanguage(ref dialect) => dialect.as_str(),
                DiagramError::ParseError(ref msg) => msg.as_str(),
                DiagramError::LimitExceeded(ref msg) => msg.as_str(),
            };
            diagram_fallback(source, explanation, origin_x, origin_y, font_size, source_offset)
        }
    }
}

/// Generate a source-preserving fallback display with a concise capability explanation.
///
/// Every line of the source text is preserved with exact character offsets into
/// the original document. No script is executed, no ambient I/O occurs.
pub fn diagram_fallback(
    source: &str,
    explanation: &str,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Vec<DisplayItem> {
    let mut items = Vec::new();
    let line_height = (font_size * 1.4).round().max(16.0);
    let mut current_y = origin_y + 8.0;

    // Header explanation run: concise capability explanation
    let header_text = format!("[Diagram: {explanation}]");
    let header_len = header_text.len();
    let header_w = (header_len as f32 * font_size * 0.55).round().max(120.0);
    items.push(DisplayItem::Text(DisplayTextRun {
        bounds: DisplayRect {
            x: origin_x + 10.0,
            y: current_y,
            width: header_w,
            height: line_height,
        },
        text: header_text,
        font_run: None,
        color_role: "diagram-fallback-note".to_string(),
        source_span: SourceSpan {
            start: source_offset,
            end: source_offset + source.len().min(header_len),
        },
        font_size: (font_size * 0.85).round().max(10.0),
    }));
    current_y += line_height + 4.0;

    // Source lines rendered with exact character-preserving source spans
    let mut line_offset = 0usize;
    let mut max_line_w = header_w;
    for line in source.lines() {
        let line_len = line.len();
        let span = SourceSpan {
            start: source_offset + line_offset,
            end: source_offset + line_offset + line_len,
        };
        let line_w = (line_len as f32 * font_size * 0.6).round().max(1.0);
        if line_w > max_line_w {
            max_line_w = line_w;
        }
        items.push(DisplayItem::Text(DisplayTextRun {
            bounds: DisplayRect {
                x: origin_x + 10.0,
                y: current_y,
                width: line_w,
                height: line_height,
            },
            text: line.to_string(),
            font_run: None,
            color_role: "diagram-fallback-text".to_string(),
            source_span: span,
            font_size,
        }));
        current_y += line_height;
        line_offset += line_len + 1; // newline
    }

    // Border / bounding outline around fallback box
    let total_w = (max_line_w + 20.0).round().max(120.0);
    let total_h = (current_y - origin_y + 8.0).round().max(32.0);
    let container_span = SourceSpan {
        start: source_offset,
        end: source_offset + source.len(),
    };
    items.insert(
        0,
        DisplayItem::Vector(DisplayVectorPath {
            bounds: DisplayRect {
                x: origin_x,
                y: origin_y,
                width: total_w,
                height: total_h,
            },
            shape: VectorShapeType::DiagramBox,
            stroke_width: 1.0,
            color_role: "diagram-fallback-border".to_string(),
            source_span: container_span,
        }),
    );

    items
}

/// Compute a semantic anchor enclosing all diagram display items.
pub fn diagram_anchor(
    source: &str,
    items: &[DisplayItem],
    origin_x: f32,
    origin_y: f32,
    source_offset: usize,
) -> DisplaySemanticAnchor {
    let mut min_x = origin_x;
    let mut min_y = origin_y;
    let mut max_x = origin_x + 1.0;
    let mut max_y = origin_y + 1.0;

    for item in items {
        let b = item.bounds();
        if b.x < min_x {
            min_x = b.x;
        }
        if b.y < min_y {
            min_y = b.y;
        }
        if b.right() > max_x {
            max_x = b.right();
        }
        if b.bottom() > max_y {
            max_y = b.bottom();
        }
    }

    let span = SourceSpan {
        start: source_offset,
        end: source_offset + source.len(),
    };
    DisplaySemanticAnchor {
        bounds: DisplayRect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).round().max(1.0),
            height: (max_y - min_y).round().max(1.0),
        },
        anchor_id: format!("diagram-{source_offset}"),
        is_heading: false,
        level: 0,
        source_span: span,
    }
}

// ---------------------------------------------------------------------------
// Internal First-Party Vector Layout Parser
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagramDirection {
    TopDown,
    LeftToRight,
}

#[derive(Clone, Debug)]
struct ParsedNode {
    id: String,
    label: String,
    span: SourceSpan,
    rank: usize,
}

#[derive(Clone, Debug)]
struct ParsedEdge {
    from_id: String,
    to_id: String,
    is_directed: bool,
    span: SourceSpan,
}

const MAX_DIAGRAM_NODES: usize = 128;
const MAX_DIAGRAM_EDGES: usize = 256;

fn parse_flowchart_vector(
    source: &str,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Result<Vec<DisplayItem>, DiagramError> {
    let mut direction = DiagramDirection::TopDown;
    let mut nodes: Vec<ParsedNode> = Vec::new();
    let mut edges: Vec<ParsedEdge> = Vec::new();

    let mut line_byte_offset = 0usize;
    for raw_line in source.lines() {
        let line_len = raw_line.len();
        let trimmed = raw_line.trim();

        if trimmed.starts_with("%%") || trimmed.is_empty() {
            line_byte_offset += line_len + 1;
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("graph") || lower.starts_with("flowchart") {
            if lower.contains("lr") {
                direction = DiagramDirection::LeftToRight;
            } else {
                direction = DiagramDirection::TopDown;
            }
            line_byte_offset += line_len + 1;
            continue;
        }

        // Look for edges: -->, ->, ---, -.->
        let edge_pattern = if let Some(idx) = trimmed.find("-->") {
            Some((idx, 3, true))
        } else if let Some(idx) = trimmed.find("---") {
            Some((idx, 3, false))
        } else if let Some(idx) = trimmed.find("->") {
            Some((idx, 2, true))
        } else {
            None
        };

        if let Some((arrow_idx, arrow_len, is_directed)) = edge_pattern {
            if edges.len() >= MAX_DIAGRAM_EDGES {
                return Err(DiagramError::LimitExceeded(
                    "diagram edge count exceeds maximum limit".to_string(),
                ));
            }

            let left_str = trimmed.get(..arrow_idx).unwrap_or("").trim();
            let right_str = trimmed.get(arrow_idx + arrow_len..).unwrap_or("").trim();

            let edge_span_start = source_offset + line_byte_offset + arrow_idx;
            let edge_span = SourceSpan {
                start: edge_span_start,
                end: edge_span_start + arrow_len,
            };

            let left_node = parse_node_token(left_str, source_offset + line_byte_offset);
            let right_node = parse_node_token(
                right_str,
                source_offset + line_byte_offset + arrow_idx + arrow_len,
            );

            if let (Some(from), Some(to)) = (left_node, right_node) {
                ensure_node_registered(&mut nodes, from.clone())?;
                ensure_node_registered(&mut nodes, to.clone())?;
                edges.push(ParsedEdge {
                    from_id: from.id,
                    to_id: to.id,
                    is_directed,
                    span: edge_span,
                });
            }
        } else if let Some(node) = parse_node_token(trimmed, source_offset + line_byte_offset) {
            ensure_node_registered(&mut nodes, node)?;
        }

        line_byte_offset += line_len + 1;
    }

    if nodes.is_empty() {
        return Err(DiagramError::ParseError(
            "no recognizable nodes or flowchart elements found in diagram source".to_string(),
        ));
    }

    // Assign ranks to nodes for hierarchical layout
    assign_node_ranks(&mut nodes, &edges);

    // Layout coordinates computation
    let mut items = Vec::new();
    let node_h = (font_size * 2.2).round().max(32.0);
    let v_spacing = (font_size * 2.5).round().max(36.0);
    let h_spacing = (font_size * 3.0).round().max(40.0);

    // Group nodes by rank
    let max_rank = nodes.iter().map(|n| n.rank).max().unwrap_or(0);
    let mut rank_nodes: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, node) in nodes.iter().enumerate() {
        if let Some(bucket) = rank_nodes.get_mut(node.rank) {
            bucket.push(i);
        }
    }

    // Compute bounding rectangles for nodes
    let mut node_rects = vec![DisplayRect::default(); nodes.len()];
    for (rank, node_indices) in rank_nodes.iter().enumerate() {
        for (col, &node_idx) in node_indices.iter().enumerate() {
            let node = match nodes.get(node_idx) {
                Some(n) => n,
                None => continue,
            };
            let label_len = node.label.chars().count();
            let node_w = ((label_len as f32 * font_size * 0.65) + 24.0)
                .round()
                .max(64.0);

            let (nx, ny) = match direction {
                DiagramDirection::TopDown => (
                    origin_x + (col as f32) * (node_w + h_spacing),
                    origin_y + (rank as f32) * (node_h + v_spacing),
                ),
                DiagramDirection::LeftToRight => (
                    origin_x + (rank as f32) * (node_w + h_spacing),
                    origin_y + (col as f32) * (node_h + v_spacing),
                ),
            };

            let rect = DisplayRect {
                x: nx,
                y: ny,
                width: node_w,
                height: node_h,
            };
            if let Some(r) = node_rects.get_mut(node_idx) {
                *r = rect;
            }

            // Node vector box outline
            items.push(DisplayItem::Vector(DisplayVectorPath {
                bounds: rect,
                shape: VectorShapeType::DiagramBox,
                stroke_width: 1.5,
                color_role: "diagram-node".to_string(),
                source_span: node.span,
            }));

            // Node centered text label
            let text_w = (label_len as f32 * font_size * 0.6).round().max(1.0);
            let text_h = font_size;
            let tx = nx + ((node_w - text_w) * 0.5).round().max(4.0);
            let ty = ny + ((node_h - text_h) * 0.5).round().max(4.0);
            items.push(DisplayItem::Text(DisplayTextRun {
                bounds: DisplayRect {
                    x: tx,
                    y: ty,
                    width: text_w,
                    height: text_h,
                },
                text: node.label.clone(),
                font_run: None,
                color_role: "diagram-text".to_string(),
                source_span: node.span,
                font_size,
            }));
        }
    }

    // Layout edges between nodes
    for edge in &edges {
        let from_idx = nodes.iter().position(|n| n.id == edge.from_id);
        let to_idx = nodes.iter().position(|n| n.id == edge.to_id);
        if let (Some(fi), Some(ti)) = (from_idx, to_idx) {
            let r1 = match node_rects.get(fi) {
                Some(r) => *r,
                None => continue,
            };
            let r2 = match node_rects.get(ti) {
                Some(r) => *r,
                None => continue,
            };

            let (start_x, start_y, end_x, end_y) = match direction {
                DiagramDirection::TopDown => (
                    r1.x + r1.width * 0.5,
                    r1.bottom(),
                    r2.x + r2.width * 0.5,
                    r2.y,
                ),
                DiagramDirection::LeftToRight => (
                    r1.right(),
                    r1.y + r1.height * 0.5,
                    r2.x,
                    r2.y + r2.height * 0.5,
                ),
            };

            let min_x = start_x.min(end_x);
            let min_y = start_y.min(end_y);
            let w = (start_x - end_x).abs().max(2.0);
            let h = (start_y - end_y).abs().max(2.0);

            // Connector line
            items.push(DisplayItem::Vector(DisplayVectorPath {
                bounds: DisplayRect {
                    x: min_x,
                    y: min_y,
                    width: w,
                    height: h,
                },
                shape: VectorShapeType::DiagramConnector,
                stroke_width: 1.5,
                color_role: "diagram-edge".to_string(),
                source_span: edge.span,
            }));

            // Arrowhead at destination
            if edge.is_directed {
                let arrow_size = (font_size * 0.5).round().max(6.0);
                let (ax, ay) = match direction {
                    DiagramDirection::TopDown => (end_x - arrow_size * 0.5, end_y - arrow_size),
                    DiagramDirection::LeftToRight => (end_x - arrow_size, end_y - arrow_size * 0.5),
                };
                items.push(DisplayItem::Vector(DisplayVectorPath {
                    bounds: DisplayRect {
                        x: ax,
                        y: ay,
                        width: arrow_size,
                        height: arrow_size,
                    },
                    shape: VectorShapeType::DiagramArrow,
                    stroke_width: 1.5,
                    color_role: "diagram-edge".to_string(),
                    source_span: edge.span,
                }));
            }
        }
    }

    Ok(items)
}

fn parse_ascii_diagram_vector(
    source: &str,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    source_offset: usize,
) -> Result<Vec<DisplayItem>, DiagramError> {
    let mut items = Vec::new();
    let char_w = (font_size * 0.6).round().max(7.0);
    let line_h = (font_size * 1.3).round().max(14.0);

    let mut line_offset = 0usize;
    for (row, line) in source.lines().enumerate() {
        let line_len = line.len();
        let y = origin_y + (row as f32) * line_h;

        // Parse consecutive runs of characters
        let mut col = 0usize;
        let mut seg_start = 0usize;
        let mut in_box_char = false;

        for ch in line.chars() {
            let is_box = matches!(ch, '+' | '-' | '|' | '=' | '#');
            if is_box != in_box_char && col > seg_start {
                let text_seg = line.get(seg_start..col).unwrap_or("");
                let span = SourceSpan {
                    start: source_offset + line_offset + seg_start,
                    end: source_offset + line_offset + col,
                };
                let x = origin_x + (seg_start as f32) * char_w;
                let w = ((col - seg_start) as f32 * char_w).max(1.0);

                if in_box_char {
                    items.push(DisplayItem::Vector(DisplayVectorPath {
                        bounds: DisplayRect {
                            x,
                            y,
                            width: w,
                            height: line_h,
                        },
                        shape: VectorShapeType::DiagramConnector,
                        stroke_width: 1.0,
                        color_role: "diagram-ascii-box".to_string(),
                        source_span: span,
                    }));
                } else if !text_seg.trim().is_empty() {
                    items.push(DisplayItem::Text(DisplayTextRun {
                        bounds: DisplayRect {
                            x,
                            y,
                            width: w,
                            height: line_h,
                        },
                        text: text_seg.to_string(),
                        font_run: None,
                        color_role: "diagram-text".to_string(),
                        source_span: span,
                        font_size,
                    }));
                }
                seg_start = col;
            }
            in_box_char = is_box;
            col += 1;
        }

        // Emit final segment on this line
        if col > seg_start {
            let text_seg = line.get(seg_start..col).unwrap_or("");
            let span = SourceSpan {
                start: source_offset + line_offset + seg_start,
                end: source_offset + line_offset + col,
            };
            let x = origin_x + (seg_start as f32) * char_w;
            let w = ((col - seg_start) as f32 * char_w).max(1.0);

            if in_box_char {
                items.push(DisplayItem::Vector(DisplayVectorPath {
                    bounds: DisplayRect {
                        x,
                        y,
                        width: w,
                        height: line_h,
                    },
                    shape: VectorShapeType::DiagramConnector,
                    stroke_width: 1.0,
                    color_role: "diagram-ascii-box".to_string(),
                    source_span: span,
                }));
            } else if !text_seg.trim().is_empty() {
                items.push(DisplayItem::Text(DisplayTextRun {
                    bounds: DisplayRect {
                        x,
                        y,
                        width: w,
                        height: line_h,
                    },
                    text: text_seg.to_string(),
                    font_run: None,
                    color_role: "diagram-text".to_string(),
                    source_span: span,
                    font_size,
                }));
            }
        }

        line_offset += line_len + 1;
    }

    if items.is_empty() {
        return Err(DiagramError::ParseError(
            "empty ascii diagram source".to_string(),
        ));
    }

    Ok(items)
}

fn parse_node_token(token: &str, line_offset: usize) -> Option<ParsedNode> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let span = SourceSpan {
        start: line_offset,
        end: line_offset + trimmed.len(),
    };

    if let Some(open) = trimmed.find('[') {
        if let Some(close) = trimmed.find(']') {
            if close > open {
                let id = trimmed.get(..open)?.trim().to_string();
                let label = trimmed.get(open + 1..close)?.trim().to_string();
                if !id.is_empty() {
                    return Some(ParsedNode {
                        id,
                        label,
                        span,
                        rank: 0,
                    });
                }
            }
        }
    }

    // Default: token itself is ID and label
    let id = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();
    if !id.is_empty() {
        Some(ParsedNode {
            id: id.clone(),
            label: id,
            span,
            rank: 0,
        })
    } else {
        None
    }
}

fn ensure_node_registered(nodes: &mut Vec<ParsedNode>, node: ParsedNode) -> Result<(), DiagramError> {
    if let Some(existing) = nodes.iter_mut().find(|n| n.id == node.id) {
        if existing.label == existing.id && node.label != node.id {
            existing.label = node.label;
        }
    } else {
        if nodes.len() >= MAX_DIAGRAM_NODES {
            return Err(DiagramError::LimitExceeded(
                "diagram node count exceeds maximum safety limit".to_string(),
            ));
        }
        nodes.push(node);
    }
    Ok(())
}

fn assign_node_ranks(nodes: &mut [ParsedNode], edges: &[ParsedEdge]) {
    // Top-down ranking with bounded relaxation to handle arbitrary graphs/cycles safely
    let max_iter = nodes.len().min(16);
    for _ in 0..max_iter {
        let mut changed = false;
        for edge in edges {
            let from_rank = nodes
                .iter()
                .find(|n| n.id == edge.from_id)
                .map(|n| n.rank)
                .unwrap_or(0);
            if let Some(to_node) = nodes.iter_mut().find(|n| n.id == edge.to_id) {
                if to_node.rank <= from_rank {
                    to_node.rank = from_rank + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        let has_vector = items
            .iter()
            .any(|item| matches!(item, DisplayItem::Vector(_)));
        assert!(has_vector, "fraction bar is a vector path");
    }

    #[test]
    fn hostile_expansion_does_not_panic() {
        let engine = engine();
        for hostile in [
            "",
            "\\\\{}",
            "$#$",
            "\\frac{\\frac{\\frac{a}{b}}{\\frac{c}{d}}}{\\frac{e}{f}}",
            "\\left(\\right)",
            "\\sqrt{}",
            "\\text{}",
            "𝔘𝔫𝔦𝔠𝔬𝔡𝔢",
        ] {
            let result = math_to_display(hostile, &engine, 0.0, 0.0, 16.0, 0);
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

    #[test]
    fn extract_math_spans_finds_inline_and_display() {
        let source = "Text $x + 1$ and $$\\frac{a}{b}$$ more";
        let spans = extract_math_spans(source);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, "x + 1");
        assert_eq!(spans[1].0, "\\frac{a}{b}");
    }

    #[test]
    fn diagram_flowchart_vector_layout_produces_nodes_and_edges() {
        let source = "graph TD\n  A[Start] --> B[Process]\n  B --> C[End]";
        let items = diagram_to_display("mermaid", source, 10.0, 10.0, 14.0, 100)
            .expect("flowchart parses");

        // Must produce vector boxes, connector lines, arrowheads, and text labels
        let boxes = items
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::DiagramBox,
                _ => false,
            })
            .count();
        let connectors = items
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::DiagramConnector,
                _ => false,
            })
            .count();
        let arrows = items
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::DiagramArrow,
                _ => false,
            })
            .count();
        let texts = items
            .iter()
            .filter(|i| matches!(i, DisplayItem::Text(_)))
            .count();

        assert_eq!(boxes, 3, "3 node boxes");
        assert_eq!(connectors, 2, "2 connector lines");
        assert_eq!(arrows, 2, "2 arrowheads");
        assert_eq!(texts, 3, "3 text labels");

        // Every item has valid bounds and non-negative dimensions
        for item in &items {
            let b = item.bounds();
            assert!(b.width > 0.0 && b.height > 0.0);
            assert!(b.x.is_finite() && b.y.is_finite());
        }
    }

    #[test]
    fn diagram_source_anchors_survive_vector_layout() {
        let source = "flowchart LR\n  Alpha --> Beta";
        let offset = 250;
        let items = diagram_to_display("flowchart", source, 0.0, 0.0, 16.0, offset)
            .expect("flowchart parses");

        for item in &items {
            let span = item.source_span();
            assert!(
                span.start >= offset,
                "span start {} >= offset {}",
                span.start,
                offset
            );
            assert!(
                span.end <= offset + source.len(),
                "span end {} <= max offset {}",
                span.end,
                offset + source.len()
            );
        }

        let anchor = diagram_anchor(source, &items, 0.0, 0.0, offset);
        assert_eq!(anchor.anchor_id, format!("diagram-{offset}"));
        assert!(anchor.bounds.width > 0.0);
        assert!(anchor.bounds.height > 0.0);
        assert_eq!(anchor.source_span.start, offset);
        assert_eq!(anchor.source_span.end, offset + source.len());
    }

    #[test]
    fn diagram_ascii_parsing_emits_vectors_and_texts() {
        let source = "+---+    +---+\n| A | -> | B |\n+---+    +---+";
        let items = diagram_to_display("ascii", source, 0.0, 0.0, 12.0, 0)
            .expect("ascii diagram parses");
        assert!(!items.is_empty());
        let has_vector = items
            .iter()
            .any(|i| matches!(i, DisplayItem::Vector(_)));
        assert!(has_vector, "ascii diagram emits vector shapes");
    }

    #[test]
    fn hostile_script_in_diagram_is_strictly_rejected() {
        // Negative control: Active script or markup must NEVER be executed
        let hostile_cases = [
            "<script>alert('pwned')</script>",
            "graph TD\n  A[<iframe src='x'>] --> B",
            "flowchart LR\n  A --> B[onload=alert(1)]",
            "javascript:void(0)",
            "<foreignObject>evil</foreignObject>",
        ];

        for hostile in hostile_cases {
            assert!(
                contains_hostile_markup(hostile),
                "must detect hostile markup in {hostile}"
            );
            let res = diagram_to_display("mermaid", hostile, 0.0, 0.0, 14.0, 0);
            assert!(
                matches!(res, Err(DiagramError::HostileContent(_))),
                "hostile markup must return HostileContent error: {hostile}"
            );

            // With fallback: safely renders source without execution
            let fallback =
                diagram_to_display_with_fallback("mermaid", hostile, 0.0, 0.0, 14.0, 0);
            assert!(!fallback.is_empty());
            let note = fallback
                .iter()
                .find(|i| match i {
                    DisplayItem::Text(t) => t.color_role == "diagram-fallback-note",
                    _ => false,
                });
            assert!(
                note.is_some(),
                "fallback must include concise capability explanation"
            );
        }
    }

    #[test]
    fn unsupported_diagram_dialect_displays_source_fallback() {
        let source = "entity User {\n  id: int\n}";
        let res = diagram_to_display("nomnoml", source, 0.0, 0.0, 14.0, 50);
        assert!(
            matches!(res, Err(DiagramError::UnsupportedLanguage(_))),
            "unsupported dialect returns typed error"
        );

        let fallback = diagram_to_display_with_fallback("nomnoml", source, 0.0, 0.0, 14.0, 50);
        assert!(!fallback.is_empty());
        // Verify source-preserving fallback contains the source text
        let text_runs: Vec<&DisplayTextRun> = fallback
            .iter()
            .filter_map(|i| match i {
                DisplayItem::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert!(
            text_runs.iter().any(|t| t.text.contains("entity User")),
            "fallback preserves original source text"
        );
    }

    #[test]
    fn extract_diagram_blocks_finds_fenced_blocks() {
        let markdown = "\
# Documentation

```mermaid
graph TD
  A --> B
```

Some text.

```plantuml
class Foo
```
";
        let blocks = extract_diagram_blocks(markdown);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, "mermaid");
        assert!(blocks[0].source.contains("graph TD"));
        assert_eq!(blocks[1].language, "plantuml");
        assert!(blocks[1].source.contains("class Foo"));
    }

    #[test]
    fn diagram_fallback_preserves_exact_source_bytes_and_spans() {
        let source = "line 1\nline 2\nline 3";
        let offset = 400;
        let items = diagram_fallback(source, "test explanation", 0.0, 0.0, 14.0, offset);

        let text_lines: Vec<&DisplayTextRun> = items
            .iter()
            .filter_map(|i| match i {
                DisplayItem::Text(t) if t.color_role == "diagram-fallback-text" => Some(t),
                _ => None,
            })
            .collect();

        assert_eq!(text_lines.len(), 3);
        assert_eq!(text_lines[0].text, "line 1");
        assert_eq!(text_lines[1].text, "line 2");
        assert_eq!(text_lines[2].text, "line 3");

        // Verify exact spans
        assert_eq!(text_lines[0].source_span.start, offset);
        assert_eq!(text_lines[0].source_span.end, offset + 6);
        assert_eq!(text_lines[1].source_span.start, offset + 7);
        assert_eq!(text_lines[1].source_span.end, offset + 13);
        assert_eq!(text_lines[2].source_span.start, offset + 14);
        assert_eq!(text_lines[2].source_span.end, offset + 20);
    }
}
