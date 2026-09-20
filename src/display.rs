//! Renderer-neutral display output and accessible reading structure (FCB-074.A).
//!
//! Plan §12.8 & §27.4:
//! - "Display output expresses text runs, vector paths, image references, clipping,
//!   logical geometry, semantic reading order, and selection provenance. It contains
//!   no Metal texture pointers, AppKit types, or FCB window IDs, arbitrary script, or
//!   live closures that can perform ambient I/O. A host maps upstream asset/source
//!   IDs to its own resource and authorization domains."
//! - "The core is synchronous-resumable and host-neutral."

#![forbid(unsafe_code)]

use crate::span::SourceSpan;
use crate::text::OwnedTextRun;

#[path = "display/spatial.rs"]
mod spatial;
pub use spatial::{
    DisplayQueryError, DisplayQueryMode, DisplayViewportItem, DisplayViewportPage,
    MAX_DISPLAY_CLIP_DEPTH, MAX_DISPLAY_QUERY_ITEMS, MAX_INDEXED_DISPLAY_ITEMS,
};

/// A 2D bounding rectangle in logical layout coordinates: `(x, y, width, height)`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DisplayRect {
    /// Left horizontal coordinate.
    pub x: f32,
    /// Top vertical coordinate.
    pub y: f32,
    /// Width extent.
    pub width: f32,
    /// Height extent.
    pub height: f32,
}

impl DisplayRect {
    /// Create a new rectangle.
    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Right edge coordinate (`x + width`).
    #[inline]
    #[must_use]
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// Bottom edge coordinate (`y + height`).
    #[inline]
    #[must_use]
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// True when `(px, py)` lies within this rectangle.
    #[inline]
    #[must_use]
    pub fn contains_point(self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// Return the minimal bounding box containing both rectangles.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if self.width <= 0.0 && self.height <= 0.0 {
            return other;
        }
        if other.width <= 0.0 && other.height <= 0.0 {
            return self;
        }
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = self.right().max(other.right());
        let max_y = self.bottom().max(other.bottom());
        Self {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        }
    }
}

/// A text run element in the renderer-neutral display list.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayTextRun {
    /// Bounding rectangle in layout coordinates.
    pub bounds: DisplayRect,
    /// Plain-text content of the run.
    pub text: String,
    /// Optional owned text run with cluster advances and glyph indices from `fmd-font`.
    pub font_run: Option<OwnedTextRun>,
    /// Semantic color/palette role (e.g. "text", "heading", "code", "link", "strong").
    pub color_role: String,
    /// Source byte span that produced this run.
    pub source_span: SourceSpan,
    /// Font size in layout units/points.
    pub font_size: f32,
}

/// Semantic vector shape types for rules, borders, and markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorShapeType {
    /// Horizontal rule (`---`).
    HorizontalRule,
    /// Table grid border or separator.
    TableBorder,
    /// Blockquote left accent bar.
    CalloutAccentBar,
    /// Task list checkbox box outline.
    CheckboxOutline,
    /// Task list checkmark inside checkbox.
    CheckboxCheck,
    /// Diagram node shape or box boundary.
    DiagramBox,
    /// Diagram directed arrow.
    DiagramArrow,
    /// Diagram connecting line.
    DiagramConnector,
}

/// A 2D vector path primitive in the renderer-neutral display list.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayVectorPath {
    /// Bounding rectangle of the vector path.
    pub bounds: DisplayRect,
    /// Shape classification.
    pub shape: VectorShapeType,
    /// Stroke line width.
    pub stroke_width: f32,
    /// Semantic color/palette role (e.g. "border", "accent", "muted").
    pub color_role: String,
    /// Source byte span that produced this vector path.
    pub source_span: SourceSpan,
}

/// An image reference in the display list, either loaded or pending resolution.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayImage {
    /// Bounding rectangle for the image placeholder or bitmap.
    pub bounds: DisplayRect,
    /// Unique asset request identifier.
    pub request_id: u64,
    /// Original URL or asset path specified in Markdown.
    pub destination: String,
    /// Alt-text description for accessibility and fallback presentation.
    pub alt_text: String,
    /// True when host has supplied resolved image bytes or dimensions.
    pub is_resolved: bool,
    /// Source byte span of the image syntax.
    pub source_span: SourceSpan,
}

/// An axis-aligned rectangular clipping primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayClip {
    /// Clip bounds.
    pub bounds: DisplayRect,
    /// Number of subsequent display items subject to this clipping region.
    pub child_count: usize,
}

/// A semantic anchor for headings and interactive links.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplaySemanticAnchor {
    /// Interactive hit rectangle.
    pub bounds: DisplayRect,
    /// Slug or anchor identifier (e.g. `overview`, `installation-1`).
    pub anchor_id: String,
    /// True if this represents a document heading target.
    pub is_heading: bool,
    /// Heading level (1 to 6) if a heading, otherwise 0.
    pub level: u8,
    /// Source byte span of the anchor.
    pub source_span: SourceSpan,
}

/// A discrete drawing and interaction primitive in the display list.
///
/// Contains NO Metal texture pointers, AppKit types, or FCB window IDs.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayItem {
    /// Formatted text run.
    Text(DisplayTextRun),
    /// Vector rule, border, or marker.
    Vector(DisplayVectorPath),
    /// Image reference or placeholder.
    Image(DisplayImage),
    /// Clipping rectangle.
    Clip(DisplayClip),
    /// Heading or link anchor.
    Anchor(DisplaySemanticAnchor),
}

impl DisplayItem {
    /// Bounding rectangle of this display item.
    #[must_use]
    pub fn bounds(&self) -> DisplayRect {
        match self {
            Self::Text(t) => t.bounds,
            Self::Vector(v) => v.bounds,
            Self::Image(img) => img.bounds,
            Self::Clip(c) => c.bounds,
            Self::Anchor(a) => a.bounds,
        }
    }

    /// Source byte span associated with this display item.
    #[must_use]
    pub fn source_span(&self) -> SourceSpan {
        match self {
            Self::Text(t) => t.source_span,
            Self::Vector(v) => v.source_span,
            Self::Image(img) => img.source_span,
            Self::Clip(_) => SourceSpan::default(),
            Self::Anchor(a) => a.source_span,
        }
    }
}

/// Semantic role of an accessible reading structure node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibleReadingRole {
    /// Entire document root.
    Document,
    /// Heading at given level (1 to 6).
    Heading { level: u8 },
    /// Paragraph of prose.
    Paragraph,
    /// Preformatted code fence.
    CodeBlock,
    /// Ordered or unordered list.
    List,
    /// List item.
    ListItem,
    /// Table grid.
    Table,
    /// Table header row.
    TableHeaderRow,
    /// Table body row.
    TableRow,
    /// Table header cell.
    TableHeaderCell,
    /// Table body cell.
    TableCell,
    /// Blockquote or callout.
    BlockQuote,
    /// Thematic horizontal rule.
    ThematicBreak,
    /// Embedded image.
    Image,
}

/// Hierarchical accessible reading node representing logical document structure.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibleReadingNode {
    /// Accessible structural role.
    pub role: AccessibleReadingRole,
    /// Accessible plain text / transcript.
    pub text: String,
    /// Source byte span for this structure.
    pub source_span: SourceSpan,
    /// Layout bounds.
    pub bounds: DisplayRect,
    /// Nested child reading nodes.
    pub children: Vec<AccessibleReadingNode>,
}

/// Complete renderer-neutral display list containing items and accessible reading order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DisplayList {
    items: Vec<DisplayItem>,
    reading_order: Vec<AccessibleReadingNode>,
    total_bounds: DisplayRect,
    spatial: spatial::IndexCache,
}

impl DisplayList {
    /// Construct a new empty display list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: Vec::new(),
            reading_order: Vec::new(),
            total_bounds: DisplayRect::new(0.0, 0.0, 0.0, 0.0),
            spatial: spatial::IndexCache::new(),
        }
    }

    /// Borrow all display items in drawing order.
    #[inline]
    #[must_use]
    pub fn items(&self) -> &[DisplayItem] {
        &self.items
    }

    /// Borrow all accessible reading tree nodes.
    #[inline]
    #[must_use]
    pub fn reading_order(&self) -> &[AccessibleReadingNode] {
        &self.reading_order
    }

    /// Total bounding box enclosing all display items.
    #[inline]
    #[must_use]
    pub const fn total_bounds(&self) -> DisplayRect {
        self.total_bounds
    }

    /// Append a display item to the list.
    pub fn push_item(&mut self, item: DisplayItem) {
        self.total_bounds = self.total_bounds.union(item.bounds());
        self.items.push(item);
        self.spatial = spatial::IndexCache::new();
    }

    /// Append an accessible reading structure node.
    pub fn push_reading_node(&mut self, node: AccessibleReadingNode) {
        self.reading_order.push(node);
    }

    /// Hit-test visible primitives in reverse drawing order, respecting every
    /// active clip. Clip commands never count as hits. Invalid geometry returns
    /// no hit; use `hit_test_filtered` to receive the validation error.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&DisplayItem> {
        self.hit_test_filtered(x, y, |_| true).ok().flatten().map(|(_, item)| item)
    }

    /// Iterator over all semantic heading and link anchors.
    pub fn anchors(&self) -> impl Iterator<Item = &DisplaySemanticAnchor> {
        self.items.iter().filter_map(|item| match item {
            DisplayItem::Anchor(a) => Some(a),
            _ => None,
        })
    }
}
