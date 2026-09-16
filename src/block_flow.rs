#![forbid(unsafe_code)]

//! Continuous block-level flow layout with checked height indexing, stable scroll
//! anchoring, and nesting limits (FCB-032.A).
//!
//! Technical specifications:
//! - Plan §10.5 & §10.10: Reader height indexing using checked `u64` fixed-point units,
//!   supporting cumulative document heights beyond 2³² without overflow.
//! - Plan §12.3 & §12.10: Bounded intrinsic measurement, nesting depth defense,
//!   linebreak caches, stable semantic anchors, and read-only task checkbox markers.
//! - Scroll anchoring: Reflow and structural edits preserve the top visible source
//!   anchor and intra-block offset rather than jumping by scrollbar percentage.
//! - GPU conversion: Local visible origin mapping ensures coordinates sent to the
//!   presentation layer are finite, well-conditioned `f32` values.

use std::collections::HashMap;
use std::fmt;

use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayItem, DisplayList, DisplayRect,
    DisplaySemanticAnchor, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
use crate::span::SourceSpan;

/// Scale factor: 256 sub-units per logical layout point (8 fractional bits).
pub const FIXED_POINT_SCALE: u64 = 256;

/// Maximum allowed nesting depth for lists and blockquotes to defend against stack exhaustion.
pub const MAX_NESTING_DEPTH: usize = 16;

/// Maximum lines allowed per individual paragraph to defend against hostile giant blocks.
pub const MAX_PARAGRAPH_LINES: usize = 1000;

// ---------------------------------------------------------------------------
// Fixed-Point Non-Negative Logical Height
// ---------------------------------------------------------------------------

/// Non-negative fixed-point logical height representation with checked `u64` accumulation.
///
/// Uses 1/256th of a logical point precision, supporting cumulative document heights
/// well beyond 2³² (up to ~7.2 × 10¹⁶ points).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LogicalHeight(pub u64);

impl LogicalHeight {
    /// Zero height.
    pub const ZERO: Self = Self(0);

    /// Create a logical height from fractional points.
    #[must_use]
    pub fn from_points(points: f32) -> Self {
        if points <= 0.0 || !points.is_finite() {
            return Self::ZERO;
        }
        let units = (points as f64 * FIXED_POINT_SCALE as f64).round();
        if units >= u64::MAX as f64 {
            Self(u64::MAX)
        } else {
            Self(units as u64)
        }
    }

    /// Convert logical height to floating-point points.
    #[must_use]
    pub fn to_points(self) -> f32 {
        (self.0 as f64 / FIXED_POINT_SCALE as f64) as f32
    }

    /// Construct directly from raw fixed-point units.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw fixed-point unit value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Checked addition.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    /// Checked subtraction.
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }

    /// Convert an absolute document height into a local visible `f32` coordinate
    /// relative to a visible origin.
    #[must_use]
    pub fn to_local_f32(self, visible_origin: Self) -> f32 {
        if self >= visible_origin {
            let diff = self.0 - visible_origin.0;
            (diff as f64 / FIXED_POINT_SCALE as f64) as f32
        } else {
            let diff = visible_origin.0 - self.0;
            -((diff as f64 / FIXED_POINT_SCALE as f64) as f32)
        }
    }
}

impl fmt::Display for LogicalHeight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}pt", self.to_points())
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced during block flow layout, height calculation, and indexing.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockFlowError {
    /// Document or block height arithmetic overflowed `u64`.
    ArithmeticOverflow,
    /// Block or child index was outside valid sequence boundaries.
    IndexOutOfBounds { index: usize, len: usize },
    /// List or blockquote nesting exceeded safety limits.
    NestingDepthExceeded { depth: usize, max: usize },
    /// Paragraph exceeded maximum permitted line count.
    ParagraphBudgetExceeded { lines: usize, max: usize },
    /// Range endpoints were reversed or invalid.
    InvalidRange { start: usize, end: usize, len: usize },
}

impl fmt::Display for BlockFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArithmeticOverflow => write!(f, "logical height arithmetic overflow"),
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "block index {index} out of bounds (len: {len})")
            }
            Self::NestingDepthExceeded { depth, max } => {
                write!(f, "nesting depth {depth} exceeds safety maximum {max}")
            }
            Self::ParagraphBudgetExceeded { lines, max } => {
                write!(f, "paragraph lines {lines} exceeds safety maximum {max}")
            }
            Self::InvalidRange { start, end, len } => {
                write!(f, "invalid range [{start}, {end}) for len {len}")
            }
        }
    }
}

impl std::error::Error for BlockFlowError {}

// ---------------------------------------------------------------------------
// Stable Scroll Anchoring
// ---------------------------------------------------------------------------

/// Stable anchor representing a viewport's scroll position anchored to a block.
///
/// Unlike scrollbar percentages (which cause visible text to jump whenever document
/// height changes due to reflow or distant edits), a `ScrollAnchor` keeps the same
/// source block and intra-block offset steady in the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollAnchor {
    /// Stable index of the block at the top of the viewport.
    pub block_id: usize,
    /// Fixed-point vertical offset into the block.
    pub intra_block_offset: LogicalHeight,
}

impl ScrollAnchor {
    /// Create a new scroll anchor.
    #[must_use]
    pub const fn new(block_id: usize, intra_block_offset: LogicalHeight) -> Self {
        Self {
            block_id,
            intra_block_offset,
        }
    }

    /// Resolve this anchor to an absolute scroll offset in the document.
    pub fn resolve_scroll_y(&self, index: &BlockHeightIndex) -> Result<LogicalHeight, BlockFlowError> {
        let block_top = index.prefix_height(self.block_id)?;
        block_top
            .checked_add(self.intra_block_offset)
            .ok_or(BlockFlowError::ArithmeticOverflow)
    }

    /// Adjust this anchor following an insertion of blocks before it.
    #[must_use]
    pub fn shift_after_insert(self, insert_at: usize, count: usize) -> Self {
        if self.block_id >= insert_at {
            Self {
                block_id: self.block_id.saturating_add(count),
                intra_block_offset: self.intra_block_offset,
            }
        } else {
            self
        }
    }

    /// Adjust this anchor following a removal of blocks before it.
    #[must_use]
    pub fn shift_after_remove(self, remove_at: usize, count: usize) -> Self {
        if self.block_id >= remove_at.saturating_add(count) {
            Self {
                block_id: self.block_id.saturating_sub(count),
                intra_block_offset: self.intra_block_offset,
            }
        } else if self.block_id >= remove_at {
            Self {
                block_id: remove_at,
                intra_block_offset: LogicalHeight::ZERO,
            }
        } else {
            self
        }
    }
}

// ---------------------------------------------------------------------------
// Block Height Index
// ---------------------------------------------------------------------------

/// Checked prefix-sum index over block heights.
///
/// Supports query and structural edits over heights beyond 2³² with checked arithmetic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockHeightIndex {
    /// Measured or estimated height of each block.
    heights: Vec<LogicalHeight>,
    /// One-indexed Fenwick prefix sum storage (slot 0 unused).
    tree: Vec<u64>,
}

impl BlockHeightIndex {
    /// Create an empty height index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            heights: Vec::new(),
            tree: vec![0],
        }
    }

    /// Build a height index from a slice of block heights.
    pub fn with_heights(heights: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        let len = heights.len();
        let mut tree = vec![0_u64; len + 1];

        for (i, &h) in heights.iter().enumerate() {
            if let Some(slot) = tree.get_mut(i + 1) {
                *slot = h.0;
            }
        }

        for i in 1..=len {
            let parent = i + (i & i.wrapping_neg());
            if parent <= len {
                let child_val = *tree.get(i).unwrap_or(&0);
                if let Some(parent_slot) = tree.get_mut(parent) {
                    *parent_slot = parent_slot
                        .checked_add(child_val)
                        .ok_or(BlockFlowError::ArithmeticOverflow)?;
                }
            }
        }

        Ok(Self {
            heights: heights.to_vec(),
            tree,
        })
    }

    /// Number of blocks in the index.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heights.len()
    }

    /// Whether the index contains no blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// Return the measured height of block at `index`.
    pub fn block_height(&self, index: usize) -> Result<LogicalHeight, BlockFlowError> {
        self.heights
            .get(index)
            .copied()
            .ok_or(BlockFlowError::IndexOutOfBounds {
                index,
                len: self.len(),
            })
    }

    /// Return the accumulated sum of heights of blocks in `[0, count)`.
    pub fn prefix_height(&self, count: usize) -> Result<LogicalHeight, BlockFlowError> {
        if count > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: count,
                len: self.len(),
            });
        }
        let mut idx = count;
        let mut sum = 0_u64;
        while idx > 0 {
            let val = *self.tree.get(idx).unwrap_or(&0);
            sum = sum
                .checked_add(val)
                .ok_or(BlockFlowError::ArithmeticOverflow)?;
            idx -= idx & idx.wrapping_neg();
        }
        Ok(LogicalHeight(sum))
    }

    /// Total cumulative height of all blocks in the document.
    pub fn total_height(&self) -> Result<LogicalHeight, BlockFlowError> {
        self.prefix_height(self.len())
    }

    /// Find the stable `ScrollAnchor` at a given absolute scroll position.
    ///
    /// Returns the block index containing `scroll_y` and the intra-block offset.
    pub fn find_anchor_at_scroll(&self, scroll_y: LogicalHeight) -> Result<ScrollAnchor, BlockFlowError> {
        if self.is_empty() {
            return Ok(ScrollAnchor::new(0, LogicalHeight::ZERO));
        }

        let target = scroll_y.0;
        let mut idx = 0_usize;
        let mut accumulated = 0_u64;

        // Binary lifting over Fenwick tree
        let mut bit = 1_usize;
        while (bit << 1) <= self.len() {
            bit <<= 1;
        }

        while bit > 0 {
            let next = idx + bit;
            if next <= self.len() {
                let candidate = *self.tree.get(next).unwrap_or(&0);
                if let Some(new_sum) = accumulated.checked_add(candidate) {
                    if new_sum <= target {
                        idx = next;
                        accumulated = new_sum;
                    }
                }
            }
            bit >>= 1;
        }

        if idx >= self.len() {
            let last_idx = self.len().saturating_sub(1);
            let last_h = self.block_height(last_idx)?;
            return Ok(ScrollAnchor::new(last_idx, last_h));
        }

        let intra_offset = target.saturating_sub(accumulated);
        Ok(ScrollAnchor::new(idx, LogicalHeight(intra_offset)))
    }

    /// Insert a block with `height` at `index`.
    pub fn insert_block(&mut self, index: usize, height: LogicalHeight) -> Result<(), BlockFlowError> {
        if index > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index,
                len: self.len(),
            });
        }
        let mut new_heights = self.heights.clone();
        new_heights.insert(index, height);
        let replacement = Self::with_heights(&new_heights)?;
        *self = replacement;
        Ok(())
    }

    /// Remove a block at `index` and return its height.
    pub fn remove_block(&mut self, index: usize) -> Result<LogicalHeight, BlockFlowError> {
        if index >= self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index,
                len: self.len(),
            });
        }
        let removed = self.heights.remove(index);
        let replacement = Self::with_heights(&self.heights)?;
        *self = replacement;
        Ok(removed)
    }

    /// Update the measured height of an existing block.
    pub fn update_block_height(
        &mut self,
        index: usize,
        new_height: LogicalHeight,
    ) -> Result<(), BlockFlowError> {
        if index >= self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index,
                len: self.len(),
            });
        }
        let current = match self.heights.get_mut(index) {
            Some(h) => h,
            None => {
                return Err(BlockFlowError::IndexOutOfBounds {
                    index,
                    len: self.len(),
                })
            }
        };
        let delta = new_height.0 as i64 - current.0 as i64;
        *current = new_height;

        let mut idx = index + 1;
        while idx <= self.len() {
            if let Some(slot) = self.tree.get_mut(idx) {
                if delta >= 0 {
                    *slot = slot
                        .checked_add(delta as u64)
                        .ok_or(BlockFlowError::ArithmeticOverflow)?;
                } else {
                    *slot = slot
                        .checked_sub(delta.unsigned_abs())
                        .ok_or(BlockFlowError::ArithmeticOverflow)?;
                }
            }
            idx += idx & idx.wrapping_neg();
        }
        Ok(())
    }
}

impl Default for BlockHeightIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Block Flow Models & Linebreak Caching
// ---------------------------------------------------------------------------

/// Visual list marker classification.
#[derive(Clone, Debug, PartialEq)]
pub enum ListMarker {
    /// Bullet marker with character (e.g. `•`, `◦`, `▪`).
    Bullet(char),
    /// Numbered marker with ordinal integer (e.g. `1.`, `2.`).
    Ordered(u32),
    /// Read-only task list checkbox marker.
    Task { checked: bool },
}

/// A block-level flow element with typography and nesting metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum FlowBlockItem {
    /// Heading with level 1-6 and stable text.
    Heading {
        level: u8,
        text: String,
        source_span: SourceSpan,
    },
    /// Paragraph of continuous prose.
    Paragraph {
        text: String,
        source_span: SourceSpan,
    },
    /// List item with marker, nesting depth (0-15), and text.
    ListItem {
        marker: ListMarker,
        depth: usize,
        text: String,
        source_span: SourceSpan,
    },
    /// Blockquote with nesting depth (0-15) and text.
    Blockquote {
        depth: usize,
        text: String,
        source_span: SourceSpan,
    },
}

impl FlowBlockItem {
    /// Generates a heading slug for stable anchors.
    #[must_use]
    pub fn slug(&self) -> Option<String> {
        match self {
            Self::Heading { text, .. } => {
                let slug: String = text
                    .to_lowercase()
                    .replace(' ', "-")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-')
                    .collect();
                Some(slug)
            }
            _ => None,
        }
    }
}

/// Cache of computed line breaks keyed by text hash and available layout width.
#[derive(Clone, Debug, Default)]
pub struct LineBreakCache {
    cache: HashMap<(u64, u32), Vec<String>>,
}

impl LineBreakCache {
    /// Create a new linebreak cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Obtain or compute wrapped lines for a text block within `available_width` points.
    pub fn get_or_compute(
        &mut self,
        text: &str,
        available_width: f32,
        font_size: f32,
    ) -> Result<&[String], BlockFlowError> {
        let width_key = (available_width.round().max(1.0)) as u32;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(text, &mut hasher);
        use std::hash::Hasher;
        let text_hash = hasher.finish();

        let key = (text_hash, width_key);
        if !self.cache.contains_key(&key) {
            let lines = wrap_prose(text, available_width, font_size)?;
            self.cache.insert(key, lines);
        }

        Ok(self.cache.get(&key).map(|v| v.as_slice()).unwrap_or(&[]))
    }

    /// Number of cached entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

/// Simple word-wrapping helper for continuous paragraph prose.
fn wrap_prose(
    text: &str,
    available_width: f32,
    font_size: f32,
) -> Result<Vec<String>, BlockFlowError> {
    let char_width = (font_size * 0.55).max(4.0);
    let max_chars_per_line = ((available_width / char_width).floor() as usize).max(10);

    let mut lines = Vec::new();
    for raw_line in text.lines() {
        if raw_line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let words: Vec<&str> = raw_line.split_whitespace().collect();
        let mut current_line = String::new();

        for word in words {
            if current_line.is_empty() {
                current_line.push_str(word);
            } else if current_line.chars().count() + 1 + word.chars().count() <= max_chars_per_line {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                if lines.len() > MAX_PARAGRAPH_LINES {
                    return Err(BlockFlowError::ParagraphBudgetExceeded {
                        lines: lines.len(),
                        max: MAX_PARAGRAPH_LINES,
                    });
                }
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
            if lines.len() > MAX_PARAGRAPH_LINES {
                return Err(BlockFlowError::ParagraphBudgetExceeded {
                    lines: lines.len(),
                    max: MAX_PARAGRAPH_LINES,
                });
            }
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    Ok(lines)
}

// ---------------------------------------------------------------------------
// Block Flow Engine & Materialization
// ---------------------------------------------------------------------------

/// Continuous flow layout engine for paragraphs, headings, lists, and quotes.
#[derive(Clone, Debug, Default)]
pub struct BlockFlowEngine {
    items: Vec<FlowBlockItem>,
    height_index: BlockHeightIndex,
    linebreak_cache: LineBreakCache,
}

impl BlockFlowEngine {
    /// Create an empty flow engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            height_index: BlockHeightIndex::new(),
            linebreak_cache: LineBreakCache::new(),
        }
    }

    /// Add a block item to the flow engine, enforcing nesting depth limits.
    pub fn push_block(
        &mut self,
        block: FlowBlockItem,
        viewport_width: f32,
    ) -> Result<usize, BlockFlowError> {
        // Enforce nesting limits
        match &block {
            FlowBlockItem::ListItem { depth, .. } if *depth >= MAX_NESTING_DEPTH => {
                return Err(BlockFlowError::NestingDepthExceeded {
                    depth: *depth,
                    max: MAX_NESTING_DEPTH,
                });
            }
            FlowBlockItem::Blockquote { depth, .. } if *depth >= MAX_NESTING_DEPTH => {
                return Err(BlockFlowError::NestingDepthExceeded {
                    depth: *depth,
                    max: MAX_NESTING_DEPTH,
                });
            }
            _ => {}
        }

        // Measure block height
        let measured_height = self.measure_block(&block, viewport_width)?;
        let index = self.items.len();
        self.items.push(block);
        self.height_index.insert_block(index, measured_height)?;

        Ok(index)
    }

    /// Number of blocks in this engine.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.items.len()
    }

    /// Reference to the underlying checked height index.
    #[must_use]
    pub fn height_index(&self) -> &BlockHeightIndex {
        &self.height_index
    }

    /// Total height of all blocks in logical units.
    pub fn total_height(&self) -> Result<LogicalHeight, BlockFlowError> {
        self.height_index.total_height()
    }

    /// Measure the intrinsic height of a single block item given `viewport_width`.
    pub fn measure_block(
        &mut self,
        block: &FlowBlockItem,
        viewport_width: f32,
    ) -> Result<LogicalHeight, BlockFlowError> {
        match block {
            FlowBlockItem::Heading { level, .. } => {
                let font_size = heading_font_size(*level);
                let line_height = font_size * 1.4;
                let margin_top = font_size * 0.6;
                let margin_bottom = font_size * 0.3;
                let total = line_height + margin_top + margin_bottom;
                Ok(LogicalHeight::from_points(total))
            }
            FlowBlockItem::Paragraph { text, .. } => {
                let font_size = 14.0f32;
                let line_height = font_size * 1.4;
                let margin_bottom = 8.0f32;
                let lines = self
                    .linebreak_cache
                    .get_or_compute(text, viewport_width, font_size)?;
                let total = (lines.len() as f32) * line_height + margin_bottom;
                Ok(LogicalHeight::from_points(total))
            }
            FlowBlockItem::ListItem { depth, text, .. } => {
                let font_size = 14.0f32;
                let line_height = font_size * 1.35;
                let indent = (*depth as f32) * 20.0;
                let available = (viewport_width - indent - 30.0).max(50.0);
                let lines = self
                    .linebreak_cache
                    .get_or_compute(text, available, font_size)?;
                let total = (lines.len() as f32) * line_height + 4.0;
                Ok(LogicalHeight::from_points(total))
            }
            FlowBlockItem::Blockquote { depth, text, .. } => {
                let font_size = 14.0f32;
                let line_height = font_size * 1.4;
                let indent = (*depth as f32) * 16.0;
                let available = (viewport_width - indent - 24.0).max(50.0);
                let lines = self
                    .linebreak_cache
                    .get_or_compute(text, available, font_size)?;
                let total = (lines.len() as f32) * line_height + 8.0;
                Ok(LogicalHeight::from_points(total))
            }
        }
    }

    /// Materialize display list items within a visible viewport window.
    ///
    /// Uses local visible origin conversion so all generated coordinates are well-conditioned
    /// finite `f32` numbers regardless of total document height.
    pub fn materialize_viewport(
        &mut self,
        origin_x: f32,
        visible_origin_y: LogicalHeight,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<DisplayList, BlockFlowError> {
        let mut dl = DisplayList::new();
        let viewport_end_y = visible_origin_y
            .checked_add(LogicalHeight::from_points(viewport_height))
            .ok_or(BlockFlowError::ArithmeticOverflow)?;

        let mut current_block_top = LogicalHeight::ZERO;

        let item_count = self.items.len();
        for idx in 0..item_count {
            let block_h = self.height_index.block_height(idx)?;
            let block_bottom = current_block_top
                .checked_add(block_h)
                .ok_or(BlockFlowError::ArithmeticOverflow)?;

            // Viewport intersection test in logical height units
            if block_bottom >= visible_origin_y && current_block_top <= viewport_end_y {
                let local_y = current_block_top.to_local_f32(visible_origin_y);
                let item = &self.items[idx];
                Self::materialize_block_into(
                    &mut self.linebreak_cache,
                    &mut dl,
                    item,
                    origin_x,
                    local_y,
                    viewport_width,
                )?;
            }

            current_block_top = block_bottom;
        }

        Ok(dl)
    }

    fn materialize_block_into(
        linebreak_cache: &mut LineBreakCache,
        dl: &mut DisplayList,
        item: &FlowBlockItem,
        origin_x: f32,
        y: f32,
        viewport_width: f32,
    ) -> Result<(), BlockFlowError> {
        match item {
            FlowBlockItem::Heading {
                level,
                text,
                source_span,
            } => {
                let font_size = heading_font_size(*level);
                let line_h = font_size * 1.4;
                let bounds = DisplayRect::new(origin_x, y, viewport_width, line_h);

                dl.push_item(DisplayItem::Text(DisplayTextRun {
                    bounds,
                    text: text.clone(),
                    font_run: None,
                    color_role: "heading".to_string(),
                    source_span: *source_span,
                    font_size,
                }));

                if let Some(slug) = item.slug() {
                    dl.push_item(DisplayItem::Anchor(DisplaySemanticAnchor {
                        bounds,
                        anchor_id: slug,
                        is_heading: true,
                        level: *level,
                        source_span: *source_span,
                    }));
                }

                dl.push_reading_node(AccessibleReadingNode {
                    role: AccessibleReadingRole::Heading { level: *level },
                    text: text.clone(),
                    source_span: *source_span,
                    bounds,
                    children: Vec::new(),
                });
            }
            FlowBlockItem::Paragraph { text, source_span } => {
                let font_size = 14.0f32;
                let line_h = font_size * 1.4;
                let lines = linebreak_cache
                    .get_or_compute(text, viewport_width, font_size)?
                    .to_vec();

                let mut cur_y = y;
                for line in lines {
                    let bounds = DisplayRect::new(origin_x, cur_y, viewport_width, line_h);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: line,
                        font_run: None,
                        color_role: "text".to_string(),
                        source_span: *source_span,
                        font_size,
                    }));
                    cur_y += line_h;
                }

                let total_h = (cur_y - y).max(line_h);
                dl.push_reading_node(AccessibleReadingNode {
                    role: AccessibleReadingRole::Paragraph,
                    text: text.clone(),
                    source_span: *source_span,
                    bounds: DisplayRect::new(origin_x, y, viewport_width, total_h),
                    children: Vec::new(),
                });
            }
            FlowBlockItem::ListItem {
                marker,
                depth,
                text,
                source_span,
            } => {
                let font_size = 14.0f32;
                let line_h = font_size * 1.35;
                let indent = origin_x + (*depth as f32) * 20.0;
                let marker_w = 20.0f32;
                let text_x = indent + marker_w;
                let available = (viewport_width - (*depth as f32) * 20.0 - marker_w).max(50.0);

                // Render marker
                match marker {
                    ListMarker::Bullet(ch) => {
                        dl.push_item(DisplayItem::Text(DisplayTextRun {
                            bounds: DisplayRect::new(indent, y, marker_w, line_h),
                            text: ch.to_string(),
                            font_run: None,
                            color_role: "list-marker".to_string(),
                            source_span: *source_span,
                            font_size,
                        }));
                    }
                    ListMarker::Ordered(num) => {
                        let marker_text = format!("{num}.");
                        dl.push_item(DisplayItem::Text(DisplayTextRun {
                            bounds: DisplayRect::new(indent, y, marker_w, line_h),
                            text: marker_text,
                            font_run: None,
                            color_role: "list-marker".to_string(),
                            source_span: *source_span,
                            font_size,
                        }));
                    }
                    ListMarker::Task { checked } => {
                        // Read-only task checkbox vector shapes
                        let box_size = 12.0f32;
                        let box_y = y + (line_h - box_size) * 0.5;
                        dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                            bounds: DisplayRect::new(indent, box_y, box_size, box_size),
                            shape: VectorShapeType::CheckboxOutline,
                            stroke_width: 1.2,
                            color_role: "checkbox-border".to_string(),
                            source_span: *source_span,
                        }));
                        if *checked {
                            dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                                bounds: DisplayRect::new(
                                    indent + 2.0,
                                    box_y + 2.0,
                                    box_size - 4.0,
                                    box_size - 4.0,
                                ),
                                shape: VectorShapeType::CheckboxCheck,
                                stroke_width: 1.5,
                                color_role: "checkbox-check".to_string(),
                                source_span: *source_span,
                            }));
                        }
                    }
                }

                // Render text lines with continuation indent
                let lines = linebreak_cache
                    .get_or_compute(text, available, font_size)?
                    .to_vec();

                let mut cur_y = y;
                for line in lines {
                    let bounds = DisplayRect::new(text_x, cur_y, available, line_h);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: line,
                        font_run: None,
                        color_role: "text".to_string(),
                        source_span: *source_span,
                        font_size,
                    }));
                    cur_y += line_h;
                }

                let total_h = (cur_y - y).max(line_h);
                dl.push_reading_node(AccessibleReadingNode {
                    role: AccessibleReadingRole::ListItem,
                    text: text.clone(),
                    source_span: *source_span,
                    bounds: DisplayRect::new(indent, y, viewport_width - indent, total_h),
                    children: Vec::new(),
                });
            }
            FlowBlockItem::Blockquote {
                depth,
                text,
                source_span,
            } => {
                let font_size = 14.0f32;
                let line_h = font_size * 1.4;
                let indent = origin_x + (*depth as f32) * 16.0;
                let bar_w = 3.0f32;
                let text_x = indent + 12.0;
                let available = (viewport_width - (*depth as f32) * 16.0 - 16.0).max(50.0);

                let lines = linebreak_cache
                    .get_or_compute(text, available, font_size)?
                    .to_vec();

                let total_h = (lines.len() as f32) * line_h;

                // Left accent bar
                dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                    bounds: DisplayRect::new(indent, y, bar_w, total_h),
                    shape: VectorShapeType::CalloutAccentBar,
                    stroke_width: bar_w,
                    color_role: "blockquote-accent".to_string(),
                    source_span: *source_span,
                }));

                let mut cur_y = y;
                for line in lines {
                    let bounds = DisplayRect::new(text_x, cur_y, available, line_h);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds,
                        text: line,
                        font_run: None,
                        color_role: "quote".to_string(),
                        source_span: *source_span,
                        font_size,
                    }));
                    cur_y += line_h;
                }

                dl.push_reading_node(AccessibleReadingNode {
                    role: AccessibleReadingRole::BlockQuote,
                    text: text.clone(),
                    source_span: *source_span,
                    bounds: DisplayRect::new(indent, y, viewport_width - indent, total_h),
                    children: Vec::new(),
                });
            }
        }
        Ok(())
    }
}

fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => 28.0,
        2 => 22.0,
        3 => 18.0,
        4 => 16.0,
        5 => 14.0,
        _ => 12.0,
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_height_supports_totals_beyond_u32() {
        // Construct a total larger than u32::MAX (4,294,967,295 units)
        let large_units = (u32::MAX as u64) + 1_000_000;
        let h1 = LogicalHeight::from_raw(large_units);
        let h2 = LogicalHeight::from_raw(500);

        let total = h1.checked_add(h2).expect("addition beyond u32");
        assert_eq!(total.raw(), large_units + 500);
        assert!(total.raw() > u32::MAX as u64);

        let sub = total.checked_sub(h2).expect("subtraction");
        assert_eq!(sub, h1);
    }

    #[test]
    fn height_index_prefix_and_queries_beyond_u32() {
        let count = 10;
        // Each block has raw height > 500,000,000, so 10 blocks exceed u32::MAX
        let block_raw = 500_000_000_u64;
        let heights = vec![LogicalHeight::from_raw(block_raw); count];
        let index = BlockHeightIndex::with_heights(&heights).expect("build index");

        let total = index.total_height().expect("total height");
        assert_eq!(total.raw(), 5_000_000_000_u64);
        assert!(total.raw() > u32::MAX as u64);

        // Find anchor at offset 2,200,000,000 (lies inside block 4: 4 * 500M = 2B, + 200M)
        let anchor = index
            .find_anchor_at_scroll(LogicalHeight::from_raw(2_200_000_000))
            .expect("find anchor");
        assert_eq!(anchor.block_id, 4);
        assert_eq!(anchor.intra_block_offset.raw(), 200_000_000);
    }

    #[test]
    fn scroll_anchor_preserves_position_during_resize_and_reflow() {
        // Oracle: Window resize changes total height, but ScrollAnchor stays steady
        let initial_heights = vec![
            LogicalHeight::from_points(100.0),
            LogicalHeight::from_points(200.0),
            LogicalHeight::from_points(150.0),
        ];
        let mut index = BlockHeightIndex::with_heights(&initial_heights).unwrap();

        // Viewport is scrolled to block 1 with intra-block offset 20pt
        let anchor = ScrollAnchor::new(1, LogicalHeight::from_points(20.0));
        let scroll_y1 = anchor.resolve_scroll_y(&index).unwrap();
        assert_eq!(scroll_y1, LogicalHeight::from_points(120.0));

        // Reflow occurs on window resize: block 0 wraps and expands from 100pt to 180pt
        index
            .update_block_height(0, LogicalHeight::from_points(180.0))
            .unwrap();

        // With scroll anchoring: top visible position adjusts naturally to follow block 1
        let scroll_y2 = anchor.resolve_scroll_y(&index).unwrap();
        assert_eq!(scroll_y2, LogicalHeight::from_points(200.0));
        // Block 1 remains at the top with intra-block offset 20pt!
    }

    #[test]
    fn scroll_anchor_shift_after_block_insertion_and_removal() {
        let anchor = ScrollAnchor::new(5, LogicalHeight::from_points(10.0));

        // Insert 2 blocks at index 1: anchor block_id shifts to 7
        let shifted = anchor.shift_after_insert(1, 2);
        assert_eq!(shifted.block_id, 7);
        assert_eq!(shifted.intra_block_offset, anchor.intra_block_offset);

        // Remove 1 block at index 0: anchor block_id shifts to 6
        let unshifted = shifted.shift_after_remove(0, 1);
        assert_eq!(unshifted.block_id, 6);
    }

    #[test]
    fn local_visible_origin_gpu_conversion_is_finite_and_accurate() {
        let visible_origin = LogicalHeight::from_raw(4_000_000_000);
        let block_top = LogicalHeight::from_raw(4_000_002_560); // 10 points below origin

        let local_y = block_top.to_local_f32(visible_origin);
        assert!((local_y - 10.0).abs() < 1e-4);
        assert!(local_y.is_finite());
    }

    #[test]
    fn list_nesting_limit_enforces_safety_budget() {
        let mut engine = BlockFlowEngine::new();

        // Allowed nesting depths 0..=15
        for depth in 0..MAX_NESTING_DEPTH {
            let res = engine.push_block(
                FlowBlockItem::ListItem {
                    marker: ListMarker::Bullet('•'),
                    depth,
                    text: format!("Level {depth}"),
                    source_span: SourceSpan::default(),
                },
                800.0,
            );
            assert!(res.is_ok(), "depth {depth} must be permitted");
        }

        // Negative control: depth >= MAX_NESTING_DEPTH must be rejected
        let rejected = engine.push_block(
            FlowBlockItem::ListItem {
                marker: ListMarker::Bullet('•'),
                depth: MAX_NESTING_DEPTH,
                text: "Hostile deep list item".to_string(),
                source_span: SourceSpan::default(),
            },
            800.0,
        );
        assert!(
            matches!(rejected, Err(BlockFlowError::NestingDepthExceeded { .. })),
            "must reject excessive nesting depth"
        );
    }

    #[test]
    fn read_only_task_checkboxes_emit_vector_shapes() {
        let mut engine = BlockFlowEngine::new();
        engine
            .push_block(
                FlowBlockItem::ListItem {
                    marker: ListMarker::Task { checked: false },
                    depth: 0,
                    text: "Unchecked task".to_string(),
                    source_span: SourceSpan::default(),
                },
                800.0,
            )
            .unwrap();
        engine
            .push_block(
                FlowBlockItem::ListItem {
                    marker: ListMarker::Task { checked: true },
                    depth: 0,
                    text: "Completed task".to_string(),
                    source_span: SourceSpan::default(),
                },
                800.0,
            )
            .unwrap();

        let dl = engine
            .materialize_viewport(0.0, LogicalHeight::ZERO, 800.0, 600.0)
            .unwrap();

        let outline_count = dl
            .items()
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::CheckboxOutline,
                _ => false,
            })
            .count();
        let check_count = dl
            .items()
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::CheckboxCheck,
                _ => false,
            })
            .count();

        assert_eq!(outline_count, 2, "both tasks have checkbox outline");
        assert_eq!(check_count, 1, "only completed task has checkmark");
    }

    #[test]
    fn blockquote_emits_callout_accent_bar() {
        let mut engine = BlockFlowEngine::new();
        engine
            .push_block(
                FlowBlockItem::Blockquote {
                    depth: 0,
                    text: "Quoted passage".to_string(),
                    source_span: SourceSpan::default(),
                },
                800.0,
            )
            .unwrap();

        let dl = engine
            .materialize_viewport(0.0, LogicalHeight::ZERO, 800.0, 600.0)
            .unwrap();

        let has_bar = dl.items().iter().any(|i| match i {
            DisplayItem::Vector(v) => v.shape == VectorShapeType::CalloutAccentBar,
            _ => false,
        });
        assert!(has_bar, "blockquote must emit CalloutAccentBar");
    }
}
