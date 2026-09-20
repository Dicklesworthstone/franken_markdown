//! Cached paint-order interval index. Only geometry and item indices are retained;
//! text, glyphs, assets and accessibility trees remain owned by the display list.

use super::{DisplayItem, DisplayList, DisplayRect};
use std::fmt;
use std::sync::OnceLock;

/// Hard admission ceilings for the optional display index.
pub const MAX_INDEXED_DISPLAY_ITEMS: usize = 1_000_000;
pub const MAX_DISPLAY_CLIP_DEPTH: usize = 64;
pub const MAX_DISPLAY_QUERY_ITEMS: usize = 2048;
const LEAF_ITEMS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayQueryError {
    InvalidViewport,
    InvalidCursor,
    InvalidLimit,
    ItemBudgetExceeded,
    InvalidItemBounds { index: usize },
    InvalidClipRange { index: usize },
    ClipDepthExceeded { index: usize },
}

impl fmt::Display for DisplayQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidViewport => f.write_str("display query needs finite, nonnegative extents"),
            Self::InvalidCursor => f.write_str("display cursor is outside the item inventory"),
            Self::InvalidLimit => f.write_str("display page limit must be in 1..=2048"),
            Self::ItemBudgetExceeded => f.write_str("display index exceeds 1000000 items"),
            Self::InvalidItemBounds { index } => write!(f, "invalid display bounds at item {index}"),
            Self::InvalidClipRange { index } => write!(f, "clip at item {index} extends beyond the inventory"),
            Self::ClipDepthExceeded { index } => write!(f, "more than 64 active clips at item {index}"),
        }
    }
}
impl std::error::Error for DisplayQueryError {}

/// Whether a query retains horizontally offscreen text for line-wide baselines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayQueryMode {
    /// Only primitives whose bounds intersect both viewport and active clips.
    Visible,
    /// Also retain every text fragment at visible Y, even when clipped away.
    /// Canvas uses this to keep shared line baselines independent of scrolling.
    /// Returned clips still constrain ink; this does not authorize hidden hits.
    TextLines,
}

/// One borrowed primitive in original drawing order. Clip commands themselves
/// are omitted; `clip` is the viewport intersected with EVERY active clip scope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayViewportItem<'a> {
    pub index: usize,
    pub item: &'a DisplayItem,
    pub clip: DisplayRect,
}

#[derive(Debug, PartialEq)]
pub struct DisplayViewportPage<'a> {
    pub items: Vec<DisplayViewportItem<'a>>,
    /// Cursor in the original item inventory, not a visible-item ordinal.
    /// Feed it back unchanged with the same immutable list, viewport and mode.
    pub next_index: Option<usize>,
    pub total_items: usize,
    /// Leaf entries inspected during this query (not initial index construction).
    pub visited_entries: usize,
}

// Cache state is not document identity. Clones start cold, equality ignores the
// cache, and append invalidates it. A cached build refusal is also immutable.
#[derive(Default)]
pub(super) struct IndexCache(OnceLock<Result<SpatialIndex, DisplayQueryError>>);
impl IndexCache {
    pub(super) const fn new() -> Self { Self(OnceLock::new()) }
    fn get(&self, items: &[DisplayItem]) -> Result<&SpatialIndex, DisplayQueryError> {
        self.0.get_or_init(|| SpatialIndex::build(items)).as_ref().map_err(Clone::clone)
    }
}
impl Clone for IndexCache { fn clone(&self) -> Self { Self::new() } }
impl PartialEq for IndexCache { fn eq(&self, _: &Self) -> bool { true } }
impl fmt::Debug for IndexCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("DisplayIndexCache") }
}

impl DisplayList {
    /// Query a bounded page without cloning text/glyph payloads or rescanning
    /// the entire inventory on each scroll. The first query builds an O(n)
    /// geometry index, with at most 64 active clip scopes per item. Later
    /// queries skip paint-order subtrees whose Y ranges cannot intersect.
    ///
    /// Typical vertically ordered documents cost O(log n + intersecting leaf
    /// entries); adversarial overlapping/out-of-order bounds can still cost
    /// O(n). `visited_entries` reports actual leaf work. This is not incremental
    /// layout: changing the list invalidates its index. No source span changes.
    ///
    /// All geometry/clip scopes are validated before the first result. Crossing
    /// clip intervals follow their literal child counts, not an assumed stack.
    /// Empty viewports return empty pages. Results preserve original item IDs
    /// and drawing order, including across pages beginning inside a clip scope.
    pub fn viewport_page(
        &self, viewport: DisplayRect, after: usize, limit: usize, mode: DisplayQueryMode,
    ) -> Result<DisplayViewportPage<'_>, DisplayQueryError> {
        if !valid_rect(viewport) { return Err(DisplayQueryError::InvalidViewport); }
        if after > self.items.len() { return Err(DisplayQueryError::InvalidCursor); }
        if limit == 0 || limit > MAX_DISPLAY_QUERY_ITEMS { return Err(DisplayQueryError::InvalidLimit); }
        let index = self.spatial.get(&self.items)?;
        let mut result = DisplayViewportPage {
            items: Vec::new(), next_index: None, total_items: self.items.len(), visited_entries: 0,
        };
        if viewport.width == 0.0 || viewport.height == 0.0 { return Ok(result); }
        let start = index.entries.partition_point(|entry| entry.index < after);
        let more = index.collect(1, 0, index.leaves * LEAF_ITEMS,
            start, viewport, mode, limit, &self.items, &mut result);
        if more { result.next_index = result.items.last().map(|item| item.index + 1); }
        Ok(result)
    }

    /// Reverse-paint-order, clip-aware hit testing with a kind filter. Returns
    /// the original item index so selections need not search by equal text or
    /// shared source spans. Clip commands never count as hits. Invalid display
    /// geometry is an explicit error rather than an invisible clickable region.
    pub fn hit_test_filtered(
        &self, x: f32, y: f32, accept: impl Fn(&DisplayItem) -> bool,
    ) -> Result<Option<(usize, &DisplayItem)>, DisplayQueryError> {
        if !x.is_finite() || !y.is_finite() { return Err(DisplayQueryError::InvalidViewport); }
        let index = self.spatial.get(&self.items)?;
        Ok(index.hit(1, 0, index.leaves * LEAF_ITEMS, x, y, &self.items, &accept)
            .map(|id| (id, &self.items[id])))
    }
}

#[derive(Clone, Copy)]
struct Entry {
    index: usize,
    bounds: DisplayRect,
    clip: Option<DisplayRect>,
}
#[derive(Clone, Copy)]
struct YRange { top: f32, bottom: f32 }
impl YRange {
    const EMPTY: Self = Self { top: f32::INFINITY, bottom: f32::NEG_INFINITY };
    fn union(self, other: Self) -> Self {
        Self { top: self.top.min(other.top), bottom: self.bottom.max(other.bottom) }
    }
    fn intersects(self, top: f32, bottom: f32) -> bool {
        self.top < bottom && self.bottom > top
    }
}
struct SpatialIndex {
    entries: Vec<Entry>,
    ranges: Vec<YRange>,
    leaves: usize,
}

impl SpatialIndex {
    fn build(items: &[DisplayItem]) -> Result<Self, DisplayQueryError> {
        if items.len() > MAX_INDEXED_DISPLAY_ITEMS { return Err(DisplayQueryError::ItemBudgetExceeded); }
        let mut entries = Vec::with_capacity(items.len());
        let mut clips: Vec<(usize, DisplayRect)> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let bounds = item.bounds();
            if !valid_rect(bounds) { return Err(DisplayQueryError::InvalidItemBounds { index }); }
            clips.retain(|(end, _)| *end > index);
            if let DisplayItem::Clip(clip) = item {
                if clip.child_count > items.len() - index - 1 {
                    return Err(DisplayQueryError::InvalidClipRange { index });
                }
                if clip.child_count != 0 {
                    if clips.len() == MAX_DISPLAY_CLIP_DEPTH {
                        return Err(DisplayQueryError::ClipDepthExceeded { index });
                    }
                    clips.push((index + 1 + clip.child_count, bounds));
                }
            } else {
                let clip = clips.iter().map(|(_, rect)| *rect).reduce(intersection);
                entries.push(Entry { index, bounds, clip });
            }
        }
        let leaves = entries.len().div_ceil(LEAF_ITEMS).max(1).next_power_of_two();
        let mut ranges = vec![YRange::EMPTY; leaves * 2];
        for (position, entry) in entries.iter().enumerate() {
            // TextLines must retain all fragments of a visible text line even
            // when a clip hides one face, to keep the common baseline stable.
            let bounds = if matches!(items[entry.index], DisplayItem::Text(_)) {
                entry.bounds
            } else { entry.clip.map_or(entry.bounds, |clip| intersection(entry.bounds, clip)) };
            if bounds.height > 0.0 {
                let leaf = leaves + position / LEAF_ITEMS;
                ranges[leaf] = ranges[leaf].union(YRange { top: bounds.y, bottom: bounds.bottom() });
            }
        }
        for node in (1..leaves).rev() { ranges[node] = ranges[node * 2].union(ranges[node * 2 + 1]); }
        Ok(Self { entries, ranges, leaves })
    }

    #[allow(clippy::too_many_arguments)]
    fn collect<'a>(
        &self, node: usize, first: usize, end: usize, start: usize,
        view: DisplayRect, mode: DisplayQueryMode, limit: usize,
        items: &'a [DisplayItem], out: &mut DisplayViewportPage<'a>,
    ) -> bool {
        if end <= start || first >= self.entries.len()
            || !self.ranges[node].intersects(view.y, view.bottom()) { return false; }
        if node < self.leaves {
            let mid = first + (end - first) / 2;
            return self.collect(node * 2, first, mid, start, view, mode, limit, items, out)
                || self.collect(node * 2 + 1, mid, end, start, view, mode, limit, items, out);
        }
        for entry in &self.entries[first.max(start)..end.min(self.entries.len())] {
            out.visited_entries += 1;
            let item = &items[entry.index];
            let clip = entry.clip.map_or(view, |clip| intersection(view, clip));
            let visible = if mode == DisplayQueryMode::TextLines && matches!(item, DisplayItem::Text(_)) {
                entry.bounds.y < view.bottom() && entry.bounds.bottom() > view.y
            } else { nonempty(intersection(entry.bounds, clip)) };
            if !visible { continue; }
            if out.items.len() == limit { return true; }
            out.items.push(DisplayViewportItem { index: entry.index, item, clip });
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn hit(
        &self, node: usize, first: usize, end: usize, x: f32, y: f32,
        items: &[DisplayItem], accept: &impl Fn(&DisplayItem) -> bool,
    ) -> Option<usize> {
        if first >= self.entries.len() || y < self.ranges[node].top || y >= self.ranges[node].bottom {
            return None;
        }
        if node < self.leaves {
            let mid = first + (end - first) / 2;
            return self.hit(node * 2 + 1, mid, end, x, y, items, accept)
                .or_else(|| self.hit(node * 2, first, mid, x, y, items, accept));
        }
        self.entries[first..end.min(self.entries.len())].iter().rev().find_map(|entry| {
            (entry.bounds.contains_point(x, y)
                && entry.clip.is_none_or(|clip| clip.contains_point(x, y))
                && accept(&items[entry.index])).then_some(entry.index)
        })
    }
}

fn valid_rect(rect: DisplayRect) -> bool {
    [rect.x, rect.y, rect.width, rect.height, rect.right(), rect.bottom()]
        .iter().all(|value| value.is_finite()) && rect.width >= 0.0 && rect.height >= 0.0
}
fn nonempty(rect: DisplayRect) -> bool { rect.width > 0.0 && rect.height > 0.0 }
fn intersection(a: DisplayRect, b: DisplayRect) -> DisplayRect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    DisplayRect::new(x, y, (a.right().min(b.right()) - x).max(0.0),
        (a.bottom().min(b.bottom()) - y).max(0.0))
}

#[cfg(test)]
#[path = "spatial_tests.rs"]
mod tests;
