#![forbid(unsafe_code)]

//! Checked paged height indexing and transactional height refinement (FCB-032.B).
//!
//! Heights and document totals use the entire nonnegative `u64` domain. Mutation
//! validates every affected leaf, directory node, and the document total before
//! publishing anything. Refinements reserve only affected leaf pages; directory
//! updates stage only the logarithmic paths that actually change. Parallel
//! height/count directories give logarithmic scroll and ordinal lookup even
//! with zero-height blocks. Structural edits copy only the affected leaf; a
//! split rebuilds page metadata rather than flattening the document.

use crate::block_flow::{BlockFlowError, LogicalHeight, ScrollAnchor};
use std::collections::BTreeMap;

/// Default maximum number of block entries in a single leaf page.
pub const DEFAULT_PAGE_CAPACITY: usize = 64;

#[inline(always)]
const fn lowbit(x: usize) -> usize {
    x & x.wrapping_neg()
}

fn adjusted(value: u64, delta: i128) -> Result<u64, BlockFlowError> {
    let next = i128::from(value)
        .checked_add(delta)
        .ok_or(BlockFlowError::ArithmeticOverflow)?;
    u64::try_from(next).map_err(|_| BlockFlowError::ArithmeticOverflow)
}

/// A bounded leaf page containing measured or estimated block heights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    heights: Vec<LogicalHeight>,
    total_height: LogicalHeight,
}

impl Page {
    /// Create a page, rejecting an unrepresentable total before copying heights.
    pub fn try_new(heights: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        let mut total = LogicalHeight::ZERO;
        for &height in heights {
            total = total
                .checked_add(height)
                .ok_or(BlockFlowError::ArithmeticOverflow)?;
        }
        Ok(Self { heights: heights.to_vec(), total_height: total })
    }

    /// Number of blocks in this page.
    #[must_use]
    pub fn len(&self) -> usize { self.heights.len() }

    /// Whether this page has no blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.heights.is_empty() }

    /// Total height of all blocks in this page.
    #[must_use]
    pub fn total_height(&self) -> LogicalHeight { self.total_height }

    /// Return a block's height by its local index.
    pub fn block_height(&self, intra_idx: usize) -> Result<LogicalHeight, BlockFlowError> {
        self.heights.get(intra_idx).copied().ok_or(BlockFlowError::IndexOutOfBounds {
            index: intra_idx, len: self.len(),
        })
    }

    /// Sum heights in `[0, intra_idx)`.
    pub fn prefix_before(&self, intra_idx: usize) -> Result<LogicalHeight, BlockFlowError> {
        if intra_idx > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds { index: intra_idx, len: self.len() });
        }
        let mut sum = LogicalHeight::ZERO;
        for &height in &self.heights[..intra_idx] {
            sum = sum.checked_add(height).ok_or(BlockFlowError::ArithmeticOverflow)?;
        }
        Ok(sum)
    }

    /// Find the local block and offset containing a scroll position.
    /// Positions at or past the end clamp to the end of the last block.
    pub fn find_local_anchor(
        &self, local_scroll_y: LogicalHeight,
    ) -> Result<(usize, LogicalHeight), BlockFlowError> {
        if self.is_empty() { return Ok((0, LogicalHeight::ZERO)); }
        let mut accumulated = LogicalHeight::ZERO;
        for (index, &height) in self.heights.iter().enumerate() {
            let next = accumulated.checked_add(height).ok_or(BlockFlowError::ArithmeticOverflow)?;
            if local_scroll_y < next {
                let offset = local_scroll_y.checked_sub(accumulated)
                    .ok_or(BlockFlowError::ArithmeticOverflow)?;
                return Ok((index, offset));
            }
            accumulated = next;
        }
        let last = self.len() - 1;
        Ok((last, self.heights[last]))
    }

    /// Replace one height and return the signed delta.
    ///
    /// This legacy delta-returning API rejects differences outside `i64` without
    /// modifying the page. Index and transaction APIs support full-width `u64`
    /// replacements without routing their arithmetic through this narrow delta.
    pub fn update_height(
        &mut self, intra_idx: usize, new_height: LogicalHeight,
    ) -> Result<i64, BlockFlowError> {
        let old = self.block_height(intra_idx)?;
        let delta = i64::try_from(i128::from(new_height.raw()) - i128::from(old.raw()))
            .map_err(|_| BlockFlowError::ArithmeticOverflow)?;
        self.replace_height(intra_idx, new_height)?;
        Ok(delta)
    }

    fn replace_height(
        &mut self, intra_idx: usize, new_height: LogicalHeight,
    ) -> Result<(), BlockFlowError> {
        let old = self.block_height(intra_idx)?;
        let total = self.total_height.checked_sub(old)
            .and_then(|value| value.checked_add(new_height))
            .ok_or(BlockFlowError::ArithmeticOverflow)?;
        self.heights[intra_idx] = new_height;
        self.total_height = total;
        Ok(())
    }

    /// Insert a height. Errors leave the page unchanged.
    pub fn insert(&mut self, intra_idx: usize, height: LogicalHeight) -> Result<(), BlockFlowError> {
        if intra_idx > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds { index: intra_idx, len: self.len() });
        }
        let total = self.total_height.checked_add(height).ok_or(BlockFlowError::ArithmeticOverflow)?;
        self.heights.insert(intra_idx, height);
        self.total_height = total;
        Ok(())
    }

    /// Remove a height. Errors leave the page unchanged.
    pub fn remove(&mut self, intra_idx: usize) -> Result<LogicalHeight, BlockFlowError> {
        let removed = self.block_height(intra_idx)?;
        let total = self.total_height.checked_sub(removed).ok_or(BlockFlowError::ArithmeticOverflow)?;
        self.heights.remove(intra_idx);
        self.total_height = total;
        Ok(removed)
    }
}

/// Checked Fenwick prefix directory over page total heights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagedHeightDirectory {
    tree: Vec<u64>,
    total: u64,
}

struct DirectoryUpdate {
    nodes: Vec<(usize, u64)>,
    total: u64,
}

impl PagedHeightDirectory {
    /// Create an empty directory.
    #[must_use]
    pub const fn new() -> Self { Self { tree: Vec::new(), total: 0 } }

    /// Build a directory, checking the complete total even when the number of
    /// pages is not a power of two and no single Fenwick node covers them all.
    pub fn with_page_totals(totals: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        let mut total = 0_u64;
        for height in totals {
            total = total.checked_add(height.raw()).ok_or(BlockFlowError::ArithmeticOverflow)?;
        }
        let count = totals.len();
        let len = count.checked_add(1).ok_or(BlockFlowError::ArithmeticOverflow)?;
        let mut tree = vec![0_u64; len];
        for (index, height) in totals.iter().enumerate() { tree[index + 1] = height.raw(); }
        for index in 1..=count {
            if let Some(parent) = index.checked_add(lowbit(index)).filter(|&parent| parent <= count) {
                tree[parent] = tree[parent].checked_add(tree[index])
                    .ok_or(BlockFlowError::ArithmeticOverflow)?;
            }
        }
        Ok(Self { tree, total })
    }

    /// Number of pages tracked.
    #[must_use]
    pub fn page_count(&self) -> usize { self.tree.len().saturating_sub(1) }

    /// Sum page totals in `[0, count)`.
    pub fn prefix_height(&self, count: usize) -> Result<LogicalHeight, BlockFlowError> {
        if count > self.page_count() {
            return Err(BlockFlowError::IndexOutOfBounds { index: count, len: self.page_count() });
        }
        if count == self.page_count() { return Ok(LogicalHeight::from_raw(self.total)); }
        let mut index = count;
        let mut sum = 0_u64;
        while index > 0 {
            sum = sum.checked_add(self.tree[index]).ok_or(BlockFlowError::ArithmeticOverflow)?;
            index -= lowbit(index);
        }
        Ok(LogicalHeight::from_raw(sum))
    }

    /// Apply a signed page delta atomically. A negative leaf total is rejected
    /// even when the Fenwick node also contains other, taller pages.
    pub fn adjust_page_total(&mut self, page_idx: usize, delta: i64) -> Result<(), BlockFlowError> {
        let changes = BTreeMap::from([(page_idx, i128::from(delta))]);
        let update = self.prepare_adjustments(&changes)?;
        self.apply(update);
        Ok(())
    }

    // All final values, not transient application order, determine admission.
    // In particular, moving height from one page to another near u64::MAX is
    // legal when the final leaves, nodes, and document total are representable.
    fn prepare_adjustments(
        &self, changes: &BTreeMap<usize, i128>,
    ) -> Result<DirectoryUpdate, BlockFlowError> {
        let mut node_deltas = BTreeMap::<usize, i128>::new();
        let mut total_delta = 0_i128;
        for (&page, &delta) in changes {
            if page >= self.page_count() {
                return Err(BlockFlowError::IndexOutOfBounds { index: page, len: self.page_count() });
            }
            let before = self.prefix_height(page)?.raw();
            let after = self.prefix_height(page + 1)?.raw();
            let leaf = after.checked_sub(before).ok_or(BlockFlowError::ArithmeticOverflow)?;
            adjusted(leaf, delta)?;
            total_delta = total_delta.checked_add(delta).ok_or(BlockFlowError::ArithmeticOverflow)?;
            let mut index = page + 1;
            while index <= self.page_count() {
                let entry = node_deltas.entry(index).or_default();
                *entry = entry.checked_add(delta).ok_or(BlockFlowError::ArithmeticOverflow)?;
                let Some(next) = index.checked_add(lowbit(index)) else { break; };
                index = next;
            }
        }
        let total = adjusted(self.total, total_delta)?;
        let mut nodes = Vec::with_capacity(node_deltas.len());
        for (index, delta) in node_deltas { nodes.push((index, adjusted(self.tree[index], delta)?)); }
        Ok(DirectoryUpdate { nodes, total })
    }

    fn apply(&mut self, update: DirectoryUpdate) {
        for (index, value) in update.nodes { self.tree[index] = value; }
        self.total = update.total;
    }

    /// Find the page containing an absolute scroll position via binary lifting.
    pub fn find_page_for_scroll(
        &self, scroll_y: LogicalHeight,
    ) -> Result<(usize, LogicalHeight), BlockFlowError> {
        let count = self.page_count();
        if count == 0 { return Ok((0, LogicalHeight::ZERO)); }
        let mut index = 0_usize;
        let mut accumulated = 0_u64;
        let mut bit = 1_usize;
        while bit <= count / 2 { bit <<= 1; }
        while bit > 0 {
            if let Some(next) = index.checked_add(bit).filter(|&next| next <= count) {
                let sum = accumulated.checked_add(self.tree[next])
                    .ok_or(BlockFlowError::ArithmeticOverflow)?;
                if sum <= scroll_y.raw() { index = next; accumulated = sum; }
            }
            bit >>= 1;
        }
        if index == count {
            let last = count - 1;
            let prefix = self.prefix_height(last)?.raw();
            return Ok((last, LogicalHeight::from_raw(scroll_y.raw().saturating_sub(prefix))));
        }
        Ok((index, LogicalHeight::from_raw(scroll_y.raw() - accumulated)))
    }
}

impl Default for PagedHeightDirectory {
    fn default() -> Self { Self::new() }
}

/// Paged height index with bounded leaf pages and a checked prefix directory.
#[derive(Clone, Debug)]
pub struct PagedHeightIndex {
    pages: Vec<Page>,
    directory: PagedHeightDirectory,
    // A second directory indexes block counts, not heights. Zero-height blocks
    // still occupy positions and must never disappear from ordinal lookup.
    counts: PagedHeightDirectory,
    block_count: usize,
    page_capacity: usize,
    structural_revision: u64,
}

// Equality describes index contents, not its private transaction history.
impl PartialEq for PagedHeightIndex {
    fn eq(&self, other: &Self) -> bool {
        self.pages == other.pages && self.directory == other.directory
            && self.counts == other.counts && self.block_count == other.block_count
            && self.page_capacity == other.page_capacity
    }
}
impl Eq for PagedHeightIndex {}

impl PagedHeightIndex {
    /// Create an empty index with default capacity.
    #[must_use]
    pub fn new() -> Self { Self::with_page_capacity(DEFAULT_PAGE_CAPACITY) }

    /// Create an empty index with custom capacity (at least two).
    #[must_use]
    pub fn with_page_capacity(capacity: usize) -> Self {
        Self { pages: Vec::new(), directory: PagedHeightDirectory::new(),
            counts: PagedHeightDirectory::new(), block_count: 0,
            page_capacity: capacity.max(2), structural_revision: 0 }
    }

    /// Build an index from block heights.
    pub fn with_heights(heights: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        Self::with_heights_and_capacity(heights, DEFAULT_PAGE_CAPACITY)
    }

    /// Build an index with custom capacity.
    pub fn with_heights_and_capacity(
        heights: &[LogicalHeight], capacity: usize,
    ) -> Result<Self, BlockFlowError> {
        let capacity = capacity.max(2);
        let mut pages = Vec::new();
        let mut totals = Vec::new();
        let mut lengths = Vec::new();
        for chunk in heights.chunks(capacity) {
            let page = Page::try_new(chunk)?;
            totals.push(page.total_height());
            lengths.push(LogicalHeight::from_raw(chunk.len() as u64));
            pages.push(page);
        }
        let directory = PagedHeightDirectory::with_page_totals(&totals)?;
        let counts = PagedHeightDirectory::with_page_totals(&lengths)?;
        Ok(Self { pages, directory, counts, block_count: heights.len(),
            page_capacity: capacity, structural_revision: 0 })
    }

    /// Total number of blocks, available in constant time.
    #[must_use]
    pub fn len(&self) -> usize { self.block_count }

    /// Whether the index has zero blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Number of allocated leaf pages.
    #[must_use]
    pub fn page_count(&self) -> usize { self.pages.len() }

    /// Maximum entries per leaf page.
    #[must_use]
    pub fn page_capacity(&self) -> usize { self.page_capacity }

    /// Total cumulative document height.
    pub fn total_height(&self) -> Result<LogicalHeight, BlockFlowError> {
        self.directory.prefix_height(self.pages.len())
    }

    /// Locate a global block in logarithmic page-count time, including blocks
    /// whose measured height is zero. No preceding leaves are scanned.
    pub fn locate_block(&self, block_index: usize) -> Result<(usize, usize), BlockFlowError> {
        if block_index >= self.block_count {
            return Err(BlockFlowError::IndexOutOfBounds { index: block_index, len: self.block_count });
        }
        let (page, local) = self.counts.find_page_for_scroll(LogicalHeight::from_raw(block_index as u64))?;
        let local = usize::try_from(local.raw()).map_err(|_| BlockFlowError::ArithmeticOverflow)?;
        Ok((page, local))
    }

    /// Return a block height.
    pub fn block_height(&self, block_index: usize) -> Result<LogicalHeight, BlockFlowError> {
        let (page, local) = self.locate_block(block_index)?;
        self.pages[page].block_height(local)
    }

    /// Sum block heights in `[0, count)`.
    pub fn prefix_height(&self, count: usize) -> Result<LogicalHeight, BlockFlowError> {
        if count == 0 { return Ok(LogicalHeight::ZERO); }
        let len = self.len();
        if count > len { return Err(BlockFlowError::IndexOutOfBounds { index: count, len }); }
        if count == len { return self.total_height(); }
        let (page, local) = self.locate_block(count)?;
        self.directory.prefix_height(page)?.checked_add(self.pages[page].prefix_before(local)?)
            .ok_or(BlockFlowError::ArithmeticOverflow)
    }

    /// Find a stable source-block anchor at an absolute document position.
    pub fn find_anchor_at_scroll(&self, scroll_y: LogicalHeight) -> Result<ScrollAnchor, BlockFlowError> {
        if self.is_empty() { return Ok(ScrollAnchor::new(0, LogicalHeight::ZERO)); }
        let (page, local_y) = self.directory.find_page_for_scroll(scroll_y)?;
        let (local, offset) = self.pages[page].find_local_anchor(local_y)?;
        let start = usize::try_from(self.counts.prefix_height(page)?.raw())
            .map_err(|_| BlockFlowError::ArithmeticOverflow)?;
        let block = start.checked_add(local).ok_or(BlockFlowError::ArithmeticOverflow)?;
        Ok(ScrollAnchor::new(block, offset))
    }

    /// Reserve old/new versions of the affected pages for background refinement.
    /// Unrelated height refinements may commit meanwhile. Structural edits or
    /// changes to a reserved page invalidate the reservation.
    pub fn begin_refinement(&self, page_indices: &[usize]) -> Result<HeightRefinementTransaction, BlockFlowError> {
        let mut reserved_pages = BTreeMap::new();
        for &page in page_indices {
            let leaf = self.pages.get(page).ok_or(BlockFlowError::IndexOutOfBounds {
                index: page, len: self.pages.len(),
            })?;
            reserved_pages.entry(page).or_insert_with(|| (leaf.clone(), leaf.clone()));
        }
        Ok(HeightRefinementTransaction { reserved_pages, structural_revision: self.structural_revision })
    }

    // Every operation that can fail is completed before either directory or
    // pages are published. Only touched Fenwick nodes and leaf pages are staged.
    fn replace_pages(&mut self, replacements: BTreeMap<usize, Page>) -> Result<(), BlockFlowError> {
        let mut changes = BTreeMap::new();
        let mut count_changes = BTreeMap::new();
        for (&index, page) in &replacements {
            let old = self.pages.get(index).ok_or(BlockFlowError::IndexOutOfBounds {
                index, len: self.pages.len(),
            })?;
            if old.total_height != page.total_height {
                changes.insert(index, i128::from(page.total_height.raw()) - i128::from(old.total_height.raw()));
            }
            if old.len() != page.len() {
                count_changes.insert(index, page.len() as i128 - old.len() as i128);
            }
        }
        let update = self.directory.prepare_adjustments(&changes)?;
        let counts = self.counts.prepare_adjustments(&count_changes)?;
        let block_count = usize::try_from(counts.total).map_err(|_| BlockFlowError::ArithmeticOverflow)?;
        for (index, page) in replacements { self.pages[index] = page; }
        self.directory.apply(update);
        self.counts.apply(counts);
        self.block_count = block_count;
        Ok(())
    }

    /// Refine a contiguous range atomically, accepting full-width heights and
    /// simultaneous transfers that would overflow if applied one block at a time.
    pub fn refine_heights(&mut self, start_block: usize, new_heights: &[LogicalHeight]) -> Result<(), BlockFlowError> {
        let end = start_block.checked_add(new_heights.len()).ok_or(BlockFlowError::ArithmeticOverflow)?;
        let len = self.len();
        if end > len { return Err(BlockFlowError::IndexOutOfBounds { index: end, len }); }
        if new_heights.is_empty() { return Ok(()); }
        let (mut page, mut local) = self.locate_block(start_block)?;
        let mut remaining = new_heights;
        let mut replacements = BTreeMap::new();
        while !remaining.is_empty() {
            let mut heights = self.pages[page].heights.clone();
            let count = remaining.len().min(heights.len() - local);
            heights[local..local + count].copy_from_slice(&remaining[..count]);
            replacements.insert(page, Page::try_new(&heights)?);
            remaining = &remaining[count..];
            page += 1;
            local = 0;
        }
        self.replace_pages(replacements)
    }

    /// Insert a block, splitting only the destination leaf when it is full.
    /// A split rebuilds page-level metadata, never a flat copy of all blocks.
    /// Errors leave both directories and every leaf unchanged.
    pub fn insert_block(&mut self, block_index: usize, height: LogicalHeight) -> Result<(), BlockFlowError> {
        let len = self.len();
        if block_index > len { return Err(BlockFlowError::IndexOutOfBounds { index: block_index, len }); }
        let revision = self.structural_revision.checked_add(1).ok_or(BlockFlowError::ArithmeticOverflow)?;
        let block_count = len.checked_add(1).ok_or(BlockFlowError::ArithmeticOverflow)?;
        self.total_height()?.checked_add(height).ok_or(BlockFlowError::ArithmeticOverflow)?;
        if self.is_empty() {
            let mut next = Self::with_heights_and_capacity(&[height], self.page_capacity)?;
            next.structural_revision = revision;
            *self = next;
            return Ok(());
        }
        let (page, local) = if block_index == len {
            let last = self.pages.len() - 1;
            (last, self.pages[last].len())
        } else { self.locate_block(block_index)? };
        if self.pages[page].len() < self.page_capacity {
            let mut replacement = self.pages[page].clone();
            replacement.insert(local, height)?;
            self.replace_pages(BTreeMap::from([(page, replacement)]))?;
            self.structural_revision = revision;
        } else {
            // Only this leaf's height buffer is copied. Other leaf buffers
            // retain their allocations even if the outer page vector moves.
            let mut heights = self.pages[page].heights.clone();
            heights.insert(local, height);
            let middle = heights.len() / 2;
            let left = Page::try_new(&heights[..middle])?;
            let right = Page::try_new(&heights[middle..])?;
            let page_count = self.pages.len().checked_add(1).ok_or(BlockFlowError::ArithmeticOverflow)?;
            let mut totals = Vec::with_capacity(page_count);
            let mut lengths = Vec::with_capacity(page_count);
            for (index, leaf) in self.pages.iter().enumerate() {
                if index == page {
                    totals.extend([left.total_height(), right.total_height()]);
                    lengths.extend([LogicalHeight::from_raw(left.len() as u64),
                        LogicalHeight::from_raw(right.len() as u64)]);
                } else {
                    totals.push(leaf.total_height());
                    lengths.push(LogicalHeight::from_raw(leaf.len() as u64));
                }
            }
            let directory = PagedHeightDirectory::with_page_totals(&totals)?;
            let counts = PagedHeightDirectory::with_page_totals(&lengths)?;
            self.pages.reserve(1);
            self.pages[page] = left;
            self.pages.insert(page + 1, right);
            self.directory = directory;
            self.counts = counts;
            self.block_count = block_count;
            self.structural_revision = revision;
        }
        Ok(())
    }

    /// Remove a block, returning its height. Errors leave the index unchanged.
    pub fn remove_block(&mut self, block_index: usize) -> Result<LogicalHeight, BlockFlowError> {
        let (page, local) = self.locate_block(block_index)?;
        let revision = self.structural_revision.checked_add(1).ok_or(BlockFlowError::ArithmeticOverflow)?;
        let mut replacement = self.pages[page].clone();
        let removed = replacement.remove(local)?;
        if replacement.is_empty() {
            let totals: Vec<_> = self.pages.iter().enumerate()
                .filter(|(index, _)| *index != page).map(|(_, leaf)| leaf.total_height()).collect();
            let lengths: Vec<_> = self.pages.iter().enumerate()
                .filter(|(index, _)| *index != page)
                .map(|(_, leaf)| LogicalHeight::from_raw(leaf.len() as u64)).collect();
            let directory = PagedHeightDirectory::with_page_totals(&totals)?;
            let counts = PagedHeightDirectory::with_page_totals(&lengths)?;
            self.pages.remove(page);
            self.directory = directory;
            self.counts = counts;
            self.block_count -= 1;
        } else {
            self.replace_pages(BTreeMap::from([(page, replacement)]))?;
        }
        self.structural_revision = revision;
        Ok(removed)
    }
}

impl Default for PagedHeightIndex {
    fn default() -> Self { Self::new() }
}

/// Optimistic transaction holding old and staged versions of reserved pages.
/// Successful commits are atomic. Dropping or rolling back never changes the
/// index. An invalidated reservation is reported as `InvalidRange` by the
/// existing `BlockFlowError` API; it must be re-reserved before retrying.
#[derive(Clone, Debug)]
pub struct HeightRefinementTransaction {
    reserved_pages: BTreeMap<usize, (Page, Page)>,
    structural_revision: u64,
}

impl HeightRefinementTransaction {
    /// Stage a single full-width block height. Failed staging leaves all
    /// previous staged values intact, so the transaction remains usable.
    pub fn stage_block_refinement(
        &mut self, page_idx: usize, intra_page_idx: usize, new_height: LogicalHeight,
    ) -> Result<(), BlockFlowError> {
        let (_, staged) = self.reserved_pages.get_mut(&page_idx)
            .ok_or(BlockFlowError::IndexOutOfBounds { index: page_idx, len: 0 })?;
        staged.replace_height(intra_page_idx, new_height)
    }

    /// Commit only if reserved pages and their structural coordinates still
    /// match. All arithmetic is validated against the current directory before
    /// publication; unrelated page refinements are preserved.
    pub fn commit(self, index: &mut PagedHeightIndex) -> Result<(), BlockFlowError> {
        if self.reserved_pages.is_empty() { return Ok(()); }
        for (&page, (original, _)) in &self.reserved_pages {
            let current = index.pages.get(page).ok_or(BlockFlowError::IndexOutOfBounds {
                index: page, len: index.pages.len(),
            })?;
            if self.structural_revision != index.structural_revision || current != original {
                return Err(BlockFlowError::InvalidRange { start: page, end: page + 1, len: index.pages.len() });
            }
        }
        let replacements = self.reserved_pages.into_iter()
            .filter(|(_, (original, staged))| original != staged)
            .map(|(page, (_, staged))| (page, staged))
            .collect();
        index.replace_pages(replacements)
    }

    /// Discard staged values without applying them.
    pub fn rollback(self) {}
}

// Original regression cases remain alongside the new integration regressions.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn paged_height_accumulates_totals_beyond_u32() {
        let heights = vec![LogicalHeight::from_raw(50_000_000); 100];
        let index = PagedHeightIndex::with_heights_and_capacity(&heights, 10).expect("build paged index");
        assert_eq!(index.page_count(), 10);
        assert_eq!(index.len(), 100);
        let total = index.total_height().expect("total height");
        assert_eq!(total.raw(), 5_000_000_000);
        assert!(total.raw() > u32::MAX as u64);
        assert_eq!(index.prefix_height(75).expect("prefix 75").raw(), 3_750_000_000);
        let anchor = index.find_anchor_at_scroll(LogicalHeight::from_raw(4_200_000_000))
            .expect("find anchor");
        assert_eq!(anchor.block_id, 84);
        assert_eq!(anchor.intra_block_offset.raw(), 0);
    }

    #[test]
    fn transactional_refinement_with_old_new_page_reservation() {
        let initial_heights = vec![LogicalHeight::from_points(20.0); 20];
        let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 5).unwrap();
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(400.0));
        let mut tx = index.begin_refinement(&[1, 2]).expect("begin refinement");
        for intra in 0..5 {
            tx.stage_block_refinement(1, intra, LogicalHeight::from_points(35.0))
                .expect("stage refinement");
        }
        tx.commit(&mut index).expect("commit transaction");
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(475.0));
        assert_eq!(index.block_height(7).unwrap(), LogicalHeight::from_points(35.0));
        assert_eq!(index.block_height(12).unwrap(), LogicalHeight::from_points(20.0));
    }

    #[test]
    fn transactional_refinement_rollback_preserves_original_index() {
        let initial_heights = vec![LogicalHeight::from_points(10.0); 10];
        let index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 5).unwrap();
        let original_total = index.total_height().unwrap();
        let mut tx = index.begin_refinement(&[0]).unwrap();
        tx.stage_block_refinement(0, 0, LogicalHeight::from_points(999.0)).unwrap();
        tx.rollback();
        assert_eq!(index.total_height().unwrap(), original_total);
        assert_eq!(index.block_height(0).unwrap(), LogicalHeight::from_points(10.0));
    }

    #[test]
    fn scroll_anchor_preserves_source_position_across_paged_refinements() {
        let initial_heights = vec![LogicalHeight::from_points(50.0); 16];
        let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 4).unwrap();
        let anchor = ScrollAnchor::new(9, LogicalHeight::from_points(12.0));
        let scroll_y1 = index.prefix_height(9).unwrap().checked_add(anchor.intra_block_offset).unwrap();
        assert_eq!(scroll_y1, LogicalHeight::from_points(462.0));
        index.refine_heights(0, &[LogicalHeight::from_points(80.0); 4]).unwrap();
        let scroll_y2 = index.prefix_height(9).unwrap().checked_add(anchor.intra_block_offset).unwrap();
        assert_eq!(scroll_y2, LogicalHeight::from_points(582.0));
        let re_found = index.find_anchor_at_scroll(scroll_y2).unwrap();
        assert_eq!(re_found.block_id, 9);
        assert_eq!(re_found.intra_block_offset, LogicalHeight::from_points(12.0));
    }

    #[test]
    fn paged_index_structural_insert_and_remove() {
        let mut index = PagedHeightIndex::with_heights_and_capacity(&[
            LogicalHeight::from_points(10.0), LogicalHeight::from_points(20.0),
            LogicalHeight::from_points(30.0),
        ], 2).unwrap();
        assert_eq!(index.len(), 3);
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(60.0));
        index.insert_block(1, LogicalHeight::from_points(15.0)).unwrap();
        assert_eq!(index.len(), 4);
        assert_eq!(index.block_height(1).unwrap(), LogicalHeight::from_points(15.0));
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(75.0));
        assert_eq!(index.remove_block(0).unwrap(), LogicalHeight::from_points(10.0));
        assert_eq!(index.len(), 3);
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(65.0));
    }

    #[test]
    fn negative_control_out_of_bounds_and_overflow_refused() {
        let index = PagedHeightIndex::with_heights(&[LogicalHeight::from_points(10.0)]).unwrap();
        assert!(matches!(index.block_height(99), Err(BlockFlowError::IndexOutOfBounds { .. })));
        assert!(matches!(index.begin_refinement(&[5]), Err(BlockFlowError::IndexOutOfBounds { .. })));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod atomicity_tests {
    use franken_markdown::block_flow::{BlockFlowError, LogicalHeight};
    use franken_markdown::paged_height::{Page, PagedHeightDirectory, PagedHeightIndex};

    fn heights(values: &[u64]) -> Vec<LogicalHeight> {
        values.iter().copied().map(LogicalHeight::from_raw).collect()
    }

    fn index(values: &[u64], capacity: usize) -> PagedHeightIndex {
        PagedHeightIndex::with_heights_and_capacity(&heights(values), capacity).unwrap()
    }

    fn assert_contents(index: &PagedHeightIndex, values: &[u64]) {
        assert_eq!(index.len(), values.len());
        let mut prefix = 0;
        for (position, &value) in values.iter().enumerate() {
            assert_eq!(index.prefix_height(position).unwrap().raw(), prefix);
            assert_eq!(index.block_height(position).unwrap().raw(), value);
            prefix += value;
        }
        assert_eq!(index.prefix_height(values.len()).unwrap().raw(), prefix);
        assert_eq!(index.total_height().unwrap().raw(), prefix);
    }

    #[test]
    fn leaf_overflow_and_unrepresentable_legacy_delta_leave_page_unchanged() {
        let mut page = Page::try_new(&heights(&[u64::MAX - 1, 1])).unwrap();
        let before = page.clone();
        assert_eq!(page.update_height(1, LogicalHeight::from_raw(2)), Err(BlockFlowError::ArithmeticOverflow));
        assert_eq!(page, before);
        assert!(page.insert(0, LogicalHeight::from_raw(1)).is_err());
        assert_eq!(page, before);
        assert!(page.update_height(0, LogicalHeight::ZERO).is_err());
        assert_eq!(page, before);

        // Signed-boundary crossings with representable deltas are valid.
        let mut page = Page::try_new(&heights(&[i64::MAX as u64])).unwrap();
        assert_eq!(page.update_height(0, LogicalHeight::from_raw(1_u64 << 63)).unwrap(), 1);
        assert_eq!(page.update_height(0, LogicalHeight::ZERO).unwrap(), i64::MIN);
    }

    #[test]
    fn directory_checks_entire_non_power_of_two_total() {
        assert!(matches!(PagedHeightDirectory::with_page_totals(&heights(&[u64::MAX, 0, 1])),
            Err(BlockFlowError::ArithmeticOverflow)));
        assert!(PagedHeightIndex::with_heights_and_capacity(&heights(&[u64::MAX, 0, 0, 0, 1]), 2).is_err());
    }

    #[test]
    fn directory_errors_never_publish_a_partial_fenwick_path() {
        let mut directory = PagedHeightDirectory::with_page_totals(&heights(&[1, u64::MAX - 1])).unwrap();
        let before = directory.clone();
        assert!(directory.adjust_page_total(0, 1).is_err());
        assert_eq!(directory, before);
        assert_eq!(directory.prefix_height(1).unwrap().raw(), 1);
        assert_eq!(directory.prefix_height(2).unwrap().raw(), u64::MAX);

        let mut directory = PagedHeightDirectory::with_page_totals(&heights(&[10, 0])).unwrap();
        let before = directory.clone();
        assert!(directory.adjust_page_total(1, -1).is_err());
        assert_eq!(directory, before, "a combined node cannot conceal a negative leaf");
        assert!(directory.adjust_page_total(2, 0).is_err());
        assert_eq!(directory, before);
    }

    #[test]
    fn full_width_index_refinements_insertions_and_removals_round_trip() {
        let mut index = index(&[0, 0], 3);
        index.refine_heights(0, &heights(&[u64::MAX, 0])).unwrap();
        assert_contents(&index, &[u64::MAX, 0]);
        index.refine_heights(0, &heights(&[0, u64::MAX])).unwrap();
        assert_contents(&index, &[0, u64::MAX]);
        assert_eq!(index.remove_block(1).unwrap().raw(), u64::MAX);
        index.insert_block(0, LogicalHeight::from_raw(u64::MAX)).unwrap();
        assert_contents(&index, &[u64::MAX, 0]);
        assert_eq!(index.remove_block(0).unwrap().raw(), u64::MAX);
        assert_contents(&index, &[0]);
    }

    #[test]
    fn refinement_validates_final_pages_not_temporary_application_order() {
        let mut index = index(&[0, 0, u64::MAX, 0], 2);
        index.refine_heights(0, &heights(&[u64::MAX, 0, 0, 0])).unwrap();
        assert_contents(&index, &[u64::MAX, 0, 0, 0]);
    }

    #[test]
    fn failed_late_refinement_and_insertion_leave_every_leaf_and_prefix_unchanged() {
        let mut index = index(&[1, 0, u64::MAX - 1, 0], 2);
        let before = index.clone();
        assert!(index.refine_heights(0, &heights(&[2, 0])).is_err());
        assert_eq!(index, before);
        assert!(index.refine_heights(0, &heights(&[0, 0, u64::MAX, 1])).is_err());
        assert_eq!(index, before);
        assert!(index.insert_block(1, LogicalHeight::from_raw(1)).is_err());
        assert_eq!(index, before);
        assert_contents(&index, &[1, 0, u64::MAX - 1, 0]);

        let mut spare = self::index(&[u64::MAX], 3);
        let before = spare.clone();
        assert!(spare.insert_block(1, LogicalHeight::from_raw(1)).is_err());
        assert_eq!(spare, before);
    }

    #[test]
    fn transaction_overflow_is_atomic_and_failed_stage_is_recoverable() {
        let mut index = index(&[1, 0, u64::MAX - 1, 0], 2);
        let before = index.clone();
        let mut tx = index.begin_refinement(&[0, 1]).unwrap();
        tx.stage_block_refinement(0, 0, LogicalHeight::from_raw(2)).unwrap();
        assert!(tx.commit(&mut index).is_err());
        assert_eq!(index, before);

        let mut tx = index.begin_refinement(&[1]).unwrap();
        assert!(tx.stage_block_refinement(1, 1, LogicalHeight::from_raw(2)).is_err());
        tx.stage_block_refinement(1, 0, LogicalHeight::ZERO).unwrap();
        tx.commit(&mut index).unwrap();
        assert_contents(&index, &[1, 0, 0, 0]);
    }

    #[test]
    fn transaction_transfers_full_width_height_across_pages_in_any_reservation_order() {
        for order in [[0, 1], [1, 0]] {
            let mut index = index(&[0, 0, u64::MAX, 0], 2);
            let mut tx = index.begin_refinement(&order).unwrap();
            tx.stage_block_refinement(0, 0, LogicalHeight::from_raw(u64::MAX)).unwrap();
            tx.stage_block_refinement(1, 0, LogicalHeight::ZERO).unwrap();
            tx.commit(&mut index).unwrap();
            assert_contents(&index, &[u64::MAX, 0, 0, 0]);
        }
    }

    #[test]
    fn stale_transactions_never_overwrite_reserved_or_unmodified_pages() {
        let mut index = index(&[10, 20, 30, 40], 2);
        let mut tx = index.begin_refinement(&[0, 1]).unwrap();
        tx.stage_block_refinement(0, 0, LogicalHeight::from_raw(11)).unwrap();
        index.refine_heights(2, &heights(&[31])).unwrap();
        let before = index.clone();
        assert!(matches!(tx.commit(&mut index), Err(BlockFlowError::InvalidRange { .. })));
        assert_eq!(index, before);
        assert_contents(&index, &[10, 20, 31, 40]);
    }

    #[test]
    fn unrelated_height_refinements_do_not_invalidate_a_transaction() {
        let mut index = index(&[10, 20, 30, 40], 2);
        let mut tx = index.begin_refinement(&[0, 0]).unwrap();
        tx.stage_block_refinement(0, 0, LogicalHeight::from_raw(11)).unwrap();
        index.refine_heights(2, &heights(&[31])).unwrap();
        tx.commit(&mut index).unwrap();
        assert_contents(&index, &[11, 20, 31, 40]);
    }

    #[test]
    fn structural_edits_invalidate_even_equal_looking_reserved_pages() {
        let mut index = index(&[10, 10, 10, 10], 2);
        let mut tx = index.begin_refinement(&[1]).unwrap();
        tx.stage_block_refinement(1, 0, LogicalHeight::from_raw(99)).unwrap();
        index.remove_block(0).unwrap();
        index.insert_block(0, LogicalHeight::from_raw(10)).unwrap();
        let before = index.clone();
        assert!(tx.commit(&mut index).is_err());
        assert_eq!(index, before);
        assert_contents(&index, &[10, 10, 10, 10]);
    }

    #[test]
    fn removed_reserved_pages_return_errors_instead_of_panicking() {
        let mut index = index(&[10, 10, 10, 10], 2);
        let tx = index.begin_refinement(&[1]).unwrap();
        index.remove_block(3).unwrap();
        index.remove_block(2).unwrap();
        let before = index.clone();
        assert!(matches!(tx.commit(&mut index), Err(BlockFlowError::IndexOutOfBounds { .. })));
        assert_eq!(index, before);
    }

    #[test]
    fn empty_and_zero_height_boundaries_remain_queryable() {
        let mut index = index(&[0, 0, 4, 0, 3], 2);
        let anchor = index.find_anchor_at_scroll(LogicalHeight::ZERO).unwrap();
        assert_eq!(anchor.block_id, 2);
        let anchor = index.find_anchor_at_scroll(LogicalHeight::from_raw(4)).unwrap();
        assert_eq!(anchor.block_id, 4);
        let anchor = index.find_anchor_at_scroll(LogicalHeight::from_raw(u64::MAX)).unwrap();
        assert_eq!(anchor.block_id, 4);
        assert_eq!(anchor.intra_block_offset.raw(), 3);
        assert!(index.refine_heights(index.len() + 1, &[]).is_err());
        index.refine_heights(index.len(), &[]).unwrap();
        while !index.is_empty() { index.remove_block(0).unwrap(); }
        assert_eq!(index.page_count(), 0);
        assert_contents(&index, &[]);
        assert_eq!(index.find_anchor_at_scroll(LogicalHeight::from_raw(99)).unwrap().block_id, 0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod structural_tests {
    use super::*;

    fn h(value: u64) -> LogicalHeight { LogicalHeight::from_raw(value) }

    fn check(index: &PagedHeightIndex, reference: &[LogicalHeight]) {
        assert_eq!(index.len(), reference.len());
        assert_eq!(index.directory.page_count(), index.pages.len());
        assert_eq!(index.counts.page_count(), index.pages.len());
        assert_eq!(index.counts.total, reference.len() as u64);
        assert!(index.pages.iter().all(|page| !page.is_empty() && page.len() <= index.page_capacity()));
        let mut prefix = 0_u64;
        let mut page = 0;
        let mut local = 0;
        for (position, &height) in reference.iter().enumerate() {
            if local == index.pages[page].len() { page += 1; local = 0; }
            assert_eq!(index.locate_block(position).unwrap(), (page, local));
            assert_eq!(index.block_height(position).unwrap(), height);
            assert_eq!(index.prefix_height(position).unwrap().raw(), prefix);
            prefix += height.raw();
            local += 1;
        }
        assert_eq!(index.total_height().unwrap().raw(), prefix);
        assert_eq!(index.prefix_height(reference.len()).unwrap().raw(), prefix);
        assert!(index.locate_block(reference.len()).is_err());
    }

    #[test]
    fn splitting_a_leaf_preserves_every_other_height_buffer() {
        let capacity = 64;
        let heights: Vec<_> = (0..2048).map(|_| h(1)).collect();
        let mut index = PagedHeightIndex::with_heights_and_capacity(&heights, capacity).unwrap();
        let addresses: Vec<_> = index.pages.iter().map(|page| page.heights.as_ptr()).collect();
        let split = 9;
        index.insert_block(split * capacity + 7, h(9)).unwrap();
        assert_eq!(index.page_count(), addresses.len() + 1);
        assert_eq!(index.len(), 2049);
        assert_eq!(index.total_height().unwrap().raw(), 2057);
        for (old, address) in addresses.into_iter().enumerate() {
            if old != split {
                let new = if old < split { old } else { old + 1 };
                assert_eq!(index.pages[new].heights.as_ptr(), address,
                    "a split must not clone an unrelated leaf buffer");
            }
        }
        assert!(index.pages.iter().all(|page| page.len() <= capacity));
        assert_eq!(index.block_height(split * capacity + 7).unwrap(), h(9));
    }

    #[test]
    fn ordinal_directory_handles_zero_heights_and_uneven_pages() {
        let mut reference = vec![h(0); 10];
        let mut index = PagedHeightIndex::with_heights_and_capacity(&reference, 3).unwrap();
        index.remove_block(0).unwrap();
        reference.remove(0);
        index.insert_block(8, h(0)).unwrap();
        reference.insert(8, h(0));
        check(&index, &reference);
        assert_eq!(index.total_height().unwrap(), LogicalHeight::ZERO);
        assert_eq!(index.counts.total, 10);
    }

    #[test]
    fn mixed_structural_edits_and_refinements_match_a_flat_oracle() {
        fn random(seed: &mut u64) -> u64 {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *seed >> 32
        }
        let mut seed = 32_032;
        let mut reference = Vec::new();
        let mut index = PagedHeightIndex::with_page_capacity(7);
        for _ in 0..2000 {
            match random(&mut seed) % 4 {
                0 | 1 => {
                    let position = random(&mut seed) as usize % (reference.len() + 1);
                    let height = h(random(&mut seed) % 32);
                    index.insert_block(position, height).unwrap();
                    reference.insert(position, height);
                }
                2 if !reference.is_empty() => {
                    let position = random(&mut seed) as usize % reference.len();
                    assert_eq!(index.remove_block(position).unwrap(), reference.remove(position));
                }
                _ if !reference.is_empty() => {
                    let position = random(&mut seed) as usize % reference.len();
                    let count = (reference.len() - position).min(3);
                    let values: Vec<_> = (0..count).map(|_| h(random(&mut seed) % 32)).collect();
                    index.refine_heights(position, &values).unwrap();
                    reference[position..position + count].copy_from_slice(&values);
                }
                _ => {}
            }
            check(&index, &reference);
            if !reference.is_empty() {
                let total: u64 = reference.iter().map(|value| value.raw()).sum();
                let target = random(&mut seed) % (total + 1);
                let mut offset = target;
                let mut position = 0;
                while position < reference.len() && offset >= reference[position].raw() {
                    offset -= reference[position].raw();
                    position += 1;
                }
                if position == reference.len() {
                    position -= 1;
                    offset = reference[position].raw();
                }
                let anchor = index.find_anchor_at_scroll(h(target)).unwrap();
                assert_eq!((anchor.block_id, anchor.intra_block_offset.raw()), (position, offset));
            }
        }
    }

    #[test]
    fn large_document_queries_use_consistent_ordinal_and_height_directories() {
        let heights: Vec<_> = (0..100_000).map(|_| h(3)).collect();
        let index = PagedHeightIndex::with_heights(&heights).unwrap();
        for position in (0..100_000).step_by(7919).chain([99_999]) {
            assert_eq!(index.locate_block(position).unwrap(), (position / 64, position % 64));
            assert_eq!(index.prefix_height(position).unwrap().raw(), position as u64 * 3);
            let anchor = index.find_anchor_at_scroll(h(position as u64 * 3 + 2)).unwrap();
            assert_eq!(anchor.block_id, position);
            assert_eq!(anchor.intra_block_offset.raw(), 2);
        }
        assert_eq!(index.total_height().unwrap().raw(), 300_000);
    }

    #[test]
    fn failed_structural_edits_preserve_both_directories_and_reservations() {
        let mut index = PagedHeightIndex::with_heights_and_capacity(&[h(1), h(0), h(u64::MAX - 1), h(0)], 2).unwrap();
        let before = index.clone();
        let mut transaction = index.begin_refinement(&[0]).unwrap();
        transaction.stage_block_refinement(0, 0, h(0)).unwrap();
        assert!(index.insert_block(0, h(1)).is_err());
        assert!(index.remove_block(index.len()).is_err());
        assert_eq!(index, before);
        transaction.commit(&mut index).unwrap();
        check(&index, &[h(0), h(0), h(u64::MAX - 1), h(0)]);
    }
}
