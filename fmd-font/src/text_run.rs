//! Shared owned text/font/run contract (FCB-016.A).
//!
//! Implements Plan §13.1, §13.2, §13.6, and §27.5:
//! - Immutable font backing and unique font identity.
//! - Bidirectional mapping between Byte (UTF-8), Native (UTF-16), and Visual domains.
//! - Cluster and source associations preserving exact logical source ranges.
//! - Caret affinity (`Leading` vs `Trailing`) and CPU line-level hit testing.
//! - Discontiguous visual selection rectangles for logical source ranges.
//! - Capability distinction between bundled deterministic faces and system fallback faces.

use crate::shaping::{Direction, ShapedRun};
use std::ops::Range;

mod interaction;
mod shaped;

/// Unique identifier for an immutable font face.
///
/// Plan §13.1: "Cache font data once per font identity, not per file or pane."
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId(pub u64);

impl FontId {
    /// Construct a font identifier from a raw numeric id.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Compute a stable FNV-1a content hash from immutable font binary bytes.
    #[must_use]
    pub fn from_font_data(data: &[u8]) -> Self {
        let mut hash = 0xcbf29ce484222325u64;
        for &b in data {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        Self(hash)
    }
}

/// Capability origin of the font face used for shaping.
///
/// Plan §13.2: "The capability record distinguishes deterministic bundled-face
/// runs from system-shaped fallback runs."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontOrigin {
    /// Deterministic bundled font face (e.g. OFL IBM Plex Sans, Computer Modern).
    BundledFace,
    /// Platform/system fallback font face (e.g. CoreText on macOS).
    SystemFallbackFace,
}

impl FontOrigin {
    /// Whether this font origin provides cross-machine pixel determinism.
    #[must_use]
    pub const fn is_deterministic(self) -> bool {
        matches!(self, Self::BundledFace)
    }
}

/// Explicit caret affinity for cursor positioning at cluster and direction boundaries.
///
/// Plan §13.6: "Retain cluster advance arrays and a line-level hit-test index."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaretAffinity {
    /// Associated with the leading edge / preceding character.
    Leading,
    /// Associated with the trailing edge / subsequent character.
    Trailing,
}

/// Caret position resolved across all three coordinate domains simultaneously.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretPosition {
    /// Offset in the original UTF-8 source bytes.
    pub byte_offset: usize,
    /// Offset in native UTF-16 code units (for Apple AppKit/CoreText interop).
    pub utf16_offset: usize,
    /// Visual horizontal pixel position from the start of the text run.
    pub visual_x: f32,
    /// Caret affinity at this boundary.
    pub affinity: CaretAffinity,
}

/// Result of hit-testing a visual horizontal coordinate against a text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitTestResult {
    /// Index of the hit text cluster.
    pub cluster_index: usize,
    /// Exact caret location and affinity.
    pub caret: CaretPosition,
    /// Whether the visual coordinate fell strictly inside the run's bounds.
    pub is_exact: bool,
}

/// A visual selection rectangle covering a contiguous visual portion of a selection.
///
/// Plan §13.6: "Selection rectangles may cover discontiguous visual runs for one logical range."
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionRect {
    /// Visual X coordinate of the left edge.
    pub x: f32,
    /// Visual Y coordinate of the top edge.
    pub y: f32,
    /// Width of this selection slice.
    pub width: f32,
    /// Height of this selection slice.
    pub height: f32,
}

/// Context identity for an owned text run.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRunContext {
    /// Font identity.
    pub font_id: FontId,
    /// Font point size in user/layout space.
    pub font_size: f32,
    /// OpenType script tag (e.g. `*b"latn"`).
    pub script: [u8; 4],
    /// OpenType language tag (e.g. `*b"dflt"`).
    pub language: [u8; 4],
    /// Text direction (LTR or RTL).
    pub direction: Direction,
    /// Font origin and capability tier.
    pub font_origin: FontOrigin,
}

/// A positioned glyph within an owned text run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunGlyph {
    /// OpenType glyph identifier.
    pub glyph_id: u16,
    /// Producing font identity (Plan §13.7: glyph ID has meaning only with actual font).
    pub font_id: FontId,
    /// Index of the parent cluster in the run's cluster array.
    pub cluster_index: usize,
    /// Horizontal advance in user units.
    pub x_advance: f32,
    /// Vertical advance in user units.
    pub y_advance: f32,
    /// Horizontal placement offset.
    pub x_offset: f32,
    /// Vertical placement offset.
    pub y_offset: f32,
}

/// An atomic text cluster associating source code points with shaped glyphs.
#[derive(Clone, Debug, PartialEq)]
pub struct TextCluster {
    /// 0-based index of this cluster within the run.
    pub cluster_index: usize,
    /// Byte range in the original UTF-8 logical source text.
    pub byte_range: Range<usize>,
    /// UTF-16 code unit range in native representation.
    pub utf16_range: Range<usize>,
    /// Range of glyph indices in the run's `glyphs` array belonging to this cluster.
    pub glyph_range: Range<usize>,
    /// Visual horizontal start coordinate (inclusive).
    pub x_start: f32,
    /// Visual horizontal end coordinate (inclusive).
    pub x_end: f32,
    /// Producing font identity for this cluster.
    pub font_id: FontId,
}

impl TextCluster {
    /// Visual advance width of this cluster.
    #[must_use]
    pub fn advance(&self) -> f32 {
        (self.x_end - self.x_start).abs()
    }
}

/// A fully measured, shaped, and hit-testable text run owning all metrics.
///
/// Provides the shared text-run representation across source code views,
/// Markdown documents, labels, and search excerpts (Plan §13.1).
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedTextRun {
    /// Run styling, font, and direction context.
    pub context: TextRunContext,
    /// Preserved exact original logical text.
    pub logical_text: String,
    /// Clustered source-to-glyph associations in logical order.
    pub clusters: Vec<TextCluster>,
    /// Positioned glyphs in visual presentation order.
    pub glyphs: Vec<RunGlyph>,
    /// Total visual horizontal advance width of the run.
    pub total_advance: f32,
}

impl OwnedTextRun {
    /// Construct an `OwnedTextRun` from a shaped run and layout scale.
    ///
    /// The `scale` converts font design units into user coordinates
    /// (`font_size / units_per_em`). Glyphs retain the shaper's visual order;
    /// clusters are stored in logical source order in both directions. In RTL
    /// runs their visual positions therefore descend through the cluster array.
    ///
    /// # Errors
    /// Rejects a direction mismatch, non-positive/non-finite size or scale,
    /// incomplete or non-monotone source coverage, invalid scalar boundaries,
    /// missing glyphs, negative horizontal advances, or non-finite positioning.
    /// Repeated cluster ranges must be consecutive, as for a base and its marks.
    pub fn from_shaped_run(
        context: TextRunContext,
        shaped: &ShapedRun,
        scale: f32,
    ) -> Result<Self, String> {
        shaped::build(context, shaped, scale)
    }

    /// Perform CPU line-level hit testing against visual X position.
    ///
    /// Resolves to an actual atomic cluster edge. Exterior coordinates and
    /// gaps produce inexact nearest-edge hits. NaN produces an inexact logical
    /// start; infinities clamp to the corresponding visual extreme. Empty runs
    /// have an inexact zero caret. Glyph positions and cluster geometry must
    /// come from the same layout snapshot.
    #[must_use]
    pub fn hit_test(&self, visual_x: f32) -> HitTestResult {
        interaction::hit_test(self, visual_x)
    }

    /// Resolve a logical UTF-8 position to an atomic source-cluster caret.
    ///
    /// At an interior scalar boundary of a ligature or combining cluster,
    /// `Leading` snaps to the cluster's logical start and `Trailing` to its
    /// logical end. The returned byte and UTF-16 offsets identify that real
    /// edge, not the requested interior position. No glyph-internal caret is
    /// invented. Mid-scalar and out-of-range offsets return `None`.
    ///
    /// At a shared boundary, `Leading` chooses the following cluster's leading
    /// edge; `Trailing` chooses the preceding cluster's trailing edge. At the
    /// document endpoints only the existing inward cluster edge is available.
    #[must_use]
    pub fn caret_at_byte(&self, byte_offset: usize, affinity: CaretAffinity) -> Option<CaretPosition> {
        interaction::caret_at_byte(self, byte_offset, affinity)
    }

    /// Resolve a native UTF-16 position using the same cluster-snapping policy.
    /// Surrogate-pair interiors and out-of-range offsets return `None`.
    #[must_use]
    pub fn caret_at_utf16(
        &self,
        utf16_offset: usize,
        affinity: CaretAffinity,
    ) -> Option<CaretPosition> {
        let byte_offset = utf16_to_byte(&self.logical_text, utf16_offset)?;
        self.caret_at_byte(byte_offset, affinity)
    }

    /// Compute visual-order selection rectangles for a logical source range.
    ///
    /// Partial cluster selections cover the complete cluster. Adjacent and
    /// overlapping visual intervals are unioned in either direction; separated
    /// intervals remain separate. Ranges must be in bounds at UTF-8 scalar
    /// boundaries. Invalid ranges, non-finite coordinates, or non-positive heights return
    /// no rectangles, rather than publishing a partial selection.
    #[must_use]
    pub fn selection_rects(
        &self,
        range: Range<usize>,
        y: f32,
        height: f32,
    ) -> Vec<SelectionRect> {
        interaction::selection_rects(self, range, y, height)
    }

}

/// Convert a UTF-8 byte offset to a native UTF-16 code unit offset.
#[must_use]
pub fn byte_to_utf16(text: &str, byte_offset: usize) -> Option<usize> {
    if byte_offset > text.len() || !text.is_char_boundary(byte_offset) {
        return None;
    }
    let mut u16_count = 0;
    for ch in text[..byte_offset].chars() {
        u16_count += ch.len_utf16();
    }
    Some(u16_count)
}

/// Convert a native UTF-16 code unit offset to a UTF-8 byte offset.
#[must_use]
pub fn utf16_to_byte(text: &str, utf16_offset: usize) -> Option<usize> {
    let mut cur_u16 = 0;
    let mut cur_byte = 0;

    for ch in text.chars() {
        if cur_u16 == utf16_offset {
            return Some(cur_byte);
        }
        if cur_u16 > utf16_offset {
            // Offset landed inside a surrogate pair interior
            return None;
        }
        cur_u16 += ch.len_utf16();
        cur_byte += ch.len_utf8();
    }

    if cur_u16 == utf16_offset {
        Some(cur_byte)
    } else {
        None
    }
}
