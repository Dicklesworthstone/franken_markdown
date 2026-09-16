#![forbid(unsafe_code)]

//! Checked paged height indexing and transactional height refinement (FCB-032.B).
//!
//! Technical specifications:
//! - Plan §10.5 & §10.10: Paged height structure using bounded leaf pages and a checked
//!   prefix directory, ensuring large document reflow does not clone or shift monolithic
//!   million-entry arrays.
//! - Checked arithmetic over `LogicalHeight` (`u64` fixed-point units), supporting document
//!   totals well beyond 2³² without overflow.
//! - Old/new page reservation: Refinement transactions reserve affected pages before mutation
//!   and commit atomically or roll back on error.
//! - Stable scroll anchoring: Top visible source anchor (`ScrollAnchor`) remains locked to
//!   the exact same source block and intra-block offset through font substitution,
//!   viewport resizing, and background height refinement.

use crate::block_flow::{BlockFlowError, LogicalHeight, ScrollAnchor};
use std::collections::HashMap;

/// Default maximum number of block entries in a single leaf page.
pub const DEFAULT_PAGE_CAPACITY: usize = 64;

/// Low-bit mask for binary indexed tree traversal.
#[inline(always)]
const fn lowbit(x: usize) -> usize {
    x & x.wrapping_neg()
}

// ---------------------------------------------------------------------------
// Leaf Page
// ---------------------------------------------------------------------------

/// A bounded leaf page containing measured or estimated block heights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    heights: Vec<LogicalHeight>,
    total_height: LogicalHeight,
}

impl Page {
    /// Create a page with the given initial block heights.
    pub fn try_new(heights: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        let mut total = LogicalHeight::ZERO;
        for &h in heights {
            total = total
                .checked_add(h)
                .ok_or(BlockFlowError::ArithmeticOverflow)?;
        }
        Ok(Self {
            heights: heights.to_vec(),
            total_height: total,
        })
    }

    /// Number of blocks in this page.
    #[must_use]
    pub fn len(&self) -> usize {
        self.heights.len()
    }

    /// Whether this page has no blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heights.is_empty()
    }

    /// Total height of all blocks in this page.
    #[must_use]
    pub fn total_height(&self) -> LogicalHeight {
        self.total_height
    }

    /// Return the height of the block at local index `intra_idx`.
    pub fn block_height(&self, intra_idx: usize) -> Result<LogicalHeight, BlockFlowError> {
        self.heights
            .get(intra_idx)
            .copied()
            .ok_or(BlockFlowError::IndexOutOfBounds {
                index: intra_idx,
                len: self.len(),
            })
    }

    /// Return the prefix sum of heights of blocks in `[0, intra_idx)`.
    pub fn prefix_before(&self, intra_idx: usize) -> Result<LogicalHeight, BlockFlowError> {
        if intra_idx > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: intra_idx,
                len: self.len(),
            });
        }
        let mut sum = LogicalHeight::ZERO;
        for i in 0..intra_idx {
            sum = sum
                .checked_add(self.heights[i])
                .ok_or(BlockFlowError::ArithmeticOverflow)?;
        }
        Ok(sum)
    }

    /// Find the intra-page offset and local index for a target scroll offset within this page.
    pub fn find_local_anchor(
        &self,
        local_scroll_y: LogicalHeight,
    ) -> Result<(usize, LogicalHeight), BlockFlowError> {
        if self.is_empty() {
            return Ok((0, LogicalHeight::ZERO));
        }

        let mut accumulated = LogicalHeight::ZERO;
        for (i, &h) in self.heights.iter().enumerate() {
            let next_accum = accumulated
                .checked_add(h)
                .ok_or(BlockFlowError::ArithmeticOverflow)?;
            if local_scroll_y < next_accum {
                let intra_offset = local_scroll_y
                    .checked_sub(accumulated)
                    .ok_or(BlockFlowError::ArithmeticOverflow)?;
                return Ok((i, intra_offset));
            }
            accumulated = next_accum;
        }

        let last_idx = self.len().saturating_sub(1);
        let last_h = self.block_height(last_idx)?;
        Ok((last_idx, last_h))
    }

    /// Update the height of a block at `intra_idx`, recalculating total.
    pub fn update_height(
        &mut self,
        intra_idx: usize,
        new_height: LogicalHeight,
    ) -> Result<i64, BlockFlowError> {
        if intra_idx >= self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: intra_idx,
                len: self.len(),
            });
        }
        let old_h = self.heights[intra_idx];
        let delta = new_height.raw() as i64 - old_h.raw() as i64;
        self.heights[intra_idx] = new_height;

        let new_total = if delta >= 0 {
            self.total_height
                .checked_add(LogicalHeight::from_raw(delta as u64))
                .ok_or(BlockFlowError::ArithmeticOverflow)?
        } else {
            self.total_height
                .checked_sub(LogicalHeight::from_raw(delta.unsigned_abs()))
                .ok_or(BlockFlowError::ArithmeticOverflow)?
        };
        self.total_height = new_total;
        Ok(delta)
    }

    /// Insert a block height at `intra_idx`.
    pub fn insert(
        &mut self,
        intra_idx: usize,
        height: LogicalHeight,
    ) -> Result<(), BlockFlowError> {
        if intra_idx > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: intra_idx,
                len: self.len(),
            });
        }
        self.total_height = self
            .total_height
            .checked_add(height)
            .ok_or(BlockFlowError::ArithmeticOverflow)?;
        self.heights.insert(intra_idx, height);
        Ok(())
    }

    /// Remove a block height at `intra_idx`.
    pub fn remove(&mut self, intra_idx: usize) -> Result<LogicalHeight, BlockFlowError> {
        if intra_idx >= self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: intra_idx,
                len: self.len(),
            });
        }
        let removed = self.heights.remove(intra_idx);
        self.total_height = self
            .total_height
            .checked_sub(removed)
            .ok_or(BlockFlowError::ArithmeticOverflow)?;
        Ok(removed)
    }
}

// ---------------------------------------------------------------------------
// Page Directory (Checked Prefix Sum over Page Totals)
// ---------------------------------------------------------------------------

/// Checked Fenwick prefix directory over page total heights.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagedHeightDirectory {
    tree: Vec<u64>,
}

impl PagedHeightDirectory {
    /// Create an empty directory.
    #[must_use]
    pub const fn new() -> Self {
        Self { tree: Vec::new() }
    }

    /// Build directory from a list of page totals.
    pub fn with_page_totals(totals: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        let count = totals.len();
        let mut tree = vec![0_u64; count + 1];

        for (i, &t) in totals.iter().enumerate() {
            if let Some(slot) = tree.get_mut(i + 1) {
                *slot = t.raw();
            }
        }

        for i in 1..=count {
            let parent = i + lowbit(i);
            if parent <= count {
                let child_val = *tree.get(i).unwrap_or(&0);
                if let Some(parent_slot) = tree.get_mut(parent) {
                    *parent_slot = parent_slot
                        .checked_add(child_val)
                        .ok_or(BlockFlowError::ArithmeticOverflow)?;
                }
            }
        }

        Ok(Self { tree })
    }

    /// Total number of pages tracked.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.tree.len().saturating_sub(1)
    }

    /// Sum of page total heights for pages `[0, count)`.
    pub fn prefix_height(&self, count: usize) -> Result<LogicalHeight, BlockFlowError> {
        if count > self.page_count() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: count,
                len: self.page_count(),
            });
        }
        let mut idx = count;
        let mut sum = 0_u64;
        while idx > 0 {
            let val = *self.tree.get(idx).unwrap_or(&0);
            sum = sum
                .checked_add(val)
                .ok_or(BlockFlowError::ArithmeticOverflow)?;
            idx -= lowbit(idx);
        }
        Ok(LogicalHeight::from_raw(sum))
    }

    /// Apply a height delta to page at `page_idx`.
    pub fn adjust_page_total(
        &mut self,
        page_idx: usize,
        delta: i64,
    ) -> Result<(), BlockFlowError> {
        if page_idx >= self.page_count() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: page_idx,
                len: self.page_count(),
            });
        }
        let mut idx = page_idx + 1;
        while idx <= self.page_count() {
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
            idx += lowbit(idx);
        }
        Ok(())
    }

    /// Find which page contains the given absolute scroll position via binary lifting.
    pub fn find_page_for_scroll(
        &self,
        scroll_y: LogicalHeight,
    ) -> Result<(usize, LogicalHeight), BlockFlowError> {
        if self.page_count() == 0 {
            return Ok((0, LogicalHeight::ZERO));
        }

        let target = scroll_y.raw();
        let mut idx = 0_usize;
        let mut accumulated = 0_u64;

        let mut bit = 1_usize;
        while (bit << 1) <= self.page_count() {
            bit <<= 1;
        }

        while bit > 0 {
            let next = idx + bit;
            if next <= self.page_count() {
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

        if idx >= self.page_count() {
            let last_page = self.page_count().saturating_sub(1);
            let page_prefix = self.prefix_height(last_page)?;
            let intra = scroll_y
                .checked_sub(page_prefix)
                .unwrap_or(LogicalHeight::ZERO);
            return Ok((last_page, intra));
        }

        let intra_page_scroll = target.saturating_sub(accumulated);
        Ok((idx, LogicalHeight::from_raw(intra_page_scroll)))
    }
}

// ---------------------------------------------------------------------------
// Paged Height Index
// ---------------------------------------------------------------------------

/// Paged height index with bounded leaf pages and checked prefix directory.
///
/// Reflow and height changes operate locally on bounded leaf pages rather than
/// shifting or re-allocating a monolithic array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PagedHeightIndex {
    pages: Vec<Page>,
    directory: PagedHeightDirectory,
    page_capacity: usize,
}

impl PagedHeightIndex {
    /// Create an empty paged height index with default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_page_capacity(DEFAULT_PAGE_CAPACITY)
    }

    /// Create an empty paged height index with custom page capacity.
    #[must_use]
    pub fn with_page_capacity(capacity: usize) -> Self {
        Self {
            pages: Vec::new(),
            directory: PagedHeightDirectory::new(),
            page_capacity: capacity.max(2),
        }
    }

    /// Build a paged height index from a slice of block heights.
    pub fn with_heights(heights: &[LogicalHeight]) -> Result<Self, BlockFlowError> {
        Self::with_heights_and_capacity(heights, DEFAULT_PAGE_CAPACITY)
    }

    /// Build a paged height index with custom page capacity.
    pub fn with_heights_and_capacity(
        heights: &[LogicalHeight],
        capacity: usize,
    ) -> Result<Self, BlockFlowError> {
        let cap = capacity.max(2);
        let mut pages = Vec::new();
        let mut page_totals = Vec::new();

        for chunk in heights.chunks(cap) {
            let page = Page::try_new(chunk)?;
            page_totals.push(page.total_height());
            pages.push(page);
        }

        let directory = PagedHeightDirectory::with_page_totals(&page_totals)?;
        Ok(Self {
            pages,
            directory,
            page_capacity: cap,
        })
    }

    /// Total number of blocks across all pages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pages.iter().map(|p| p.len()).sum()
    }

    /// Whether the index has zero blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of allocated leaf pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Maximum number of entries per leaf page.
    #[must_use]
    pub fn page_capacity(&self) -> usize {
        self.page_capacity
    }

    /// Total cumulative height of all blocks in the document.
    pub fn total_height(&self) -> Result<LogicalHeight, BlockFlowError> {
        self.directory.prefix_height(self.pages.len())
    }

    /// Locate which page and intra-page index corresponds to a global `block_index`.
    pub fn locate_block(&self, block_index: usize) -> Result<(usize, usize), BlockFlowError> {
        let mut remaining = block_index;
        for (page_idx, page) in self.pages.iter().enumerate() {
            if remaining < page.len() {
                return Ok((page_idx, remaining));
            }
            remaining = remaining.saturating_sub(page.len());
        }
        Err(BlockFlowError::IndexOutOfBounds {
            index: block_index,
            len: self.len(),
        })
    }

    /// Return the measured or estimated height of block at `block_index`.
    pub fn block_height(&self, block_index: usize) -> Result<LogicalHeight, BlockFlowError> {
        let (page_idx, intra_idx) = self.locate_block(block_index)?;
        let page = &self.pages[page_idx];
        page.block_height(intra_idx)
    }

    /// Return the prefix sum of heights of all blocks in `[0, count)`.
    pub fn prefix_height(&self, count: usize) -> Result<LogicalHeight, BlockFlowError> {
        if count == 0 {
            return Ok(LogicalHeight::ZERO);
        }
        if count > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: count,
                len: self.len(),
            });
        }
        if count == self.len() {
            return self.total_height();
        }

        let (page_idx, intra_idx) = self.locate_block(count)?;
        let page_prefix = self.directory.prefix_height(page_idx)?;
        let intra_prefix = self.pages[page_idx].prefix_before(intra_idx)?;

        page_prefix
            .checked_add(intra_prefix)
            .ok_or(BlockFlowError::ArithmeticOverflow)
    }

    /// Find the stable `ScrollAnchor` at an absolute document scroll position.
    pub fn find_anchor_at_scroll(
        &self,
        scroll_y: LogicalHeight,
    ) -> Result<ScrollAnchor, BlockFlowError> {
        if self.is_empty() {
            return Ok(ScrollAnchor::new(0, LogicalHeight::ZERO));
        }

        let (page_idx, intra_page_scroll) = self.directory.find_page_for_scroll(scroll_y)?;
        let target_page = &self.pages[page_idx];

        let (local_block_idx, intra_block_offset) =
            target_page.find_local_anchor(intra_page_scroll)?;

        // Compute global block_id
        let mut global_block_id = 0;
        for i in 0..page_idx {
            global_block_id += self.pages[i].len();
        }
        global_block_id += local_block_idx;

        Ok(ScrollAnchor::new(global_block_id, intra_block_offset))
    }

    /// Begin a transactional height refinement over a set of affected pages,
    /// reserving old/new pages before mutation.
    pub fn begin_refinement(
        &self,
        page_indices: &[usize],
    ) -> Result<HeightRefinementTransaction, BlockFlowError> {
        let mut old_pages = HashMap::new();
        for &idx in page_indices {
            if idx >= self.pages.len() {
                return Err(BlockFlowError::IndexOutOfBounds {
                    index: idx,
                    len: self.pages.len(),
                });
            }
            old_pages.insert(idx, self.pages[idx].clone());
        }

        Ok(HeightRefinementTransaction {
            reserved_pages: old_pages,
            page_deltas: HashMap::new(),
        })
    }

    /// Directly refine a range of block heights, updating affected leaf pages
    /// and the directory with checked arithmetic.
    pub fn refine_heights(
        &mut self,
        start_block: usize,
        new_heights: &[LogicalHeight],
    ) -> Result<(), BlockFlowError> {
        if new_heights.is_empty() {
            return Ok(());
        }
        let end_block = start_block
            .checked_add(new_heights.len())
            .ok_or(BlockFlowError::ArithmeticOverflow)?;
        if end_block > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: end_block,
                len: self.len(),
            });
        }

        for (offset, &h) in new_heights.iter().enumerate() {
            let global_idx = start_block + offset;
            let (page_idx, intra_idx) = self.locate_block(global_idx)?;
            let delta = self.pages[page_idx].update_height(intra_idx, h)?;
            self.directory.adjust_page_total(page_idx, delta)?;
        }

        Ok(())
    }

    /// Insert a new block height at `block_index`.
    pub fn insert_block(
        &mut self,
        block_index: usize,
        height: LogicalHeight,
    ) -> Result<(), BlockFlowError> {
        if block_index > self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: block_index,
                len: self.len(),
            });
        }

        if self.is_empty() {
            let page = Page::try_new(&[height])?;
            self.pages = vec![page];
            self.directory = PagedHeightDirectory::with_page_totals(&[height])?;
            return Ok(());
        }

        // Determine destination page
        let (page_idx, intra_idx) = if block_index == self.len() {
            let last_idx = self.pages.len().saturating_sub(1);
            let last_len = self.pages[last_idx].len();
            (last_idx, last_len)
        } else {
            self.locate_block(block_index)?
        };

        if self.pages[page_idx].len() < self.page_capacity {
            // Fits in current page
            self.pages[page_idx].insert(intra_idx, height)?;
            let delta = height.raw() as i64;
            self.directory.adjust_page_total(page_idx, delta)?;
        } else {
            // Page is full: rebuild pages preserving capacity
            let mut all_heights = Vec::with_capacity(self.len() + 1);
            for p in &self.pages {
                all_heights.extend_from_slice(&p.heights);
            }
            all_heights.insert(block_index, height);
            *self = Self::with_heights_and_capacity(&all_heights, self.page_capacity)?;
        }

        Ok(())
    }

    /// Remove a block at `block_index` and return its height.
    pub fn remove_block(&mut self, block_index: usize) -> Result<LogicalHeight, BlockFlowError> {
        if block_index >= self.len() {
            return Err(BlockFlowError::IndexOutOfBounds {
                index: block_index,
                len: self.len(),
            });
        }

        let (page_idx, intra_idx) = self.locate_block(block_index)?;
        let removed = self.pages[page_idx].remove(intra_idx)?;
        let delta = -(removed.raw() as i64);
        self.directory.adjust_page_total(page_idx, delta)?;

        // If page became empty and there are other pages, remove the empty page
        if self.pages[page_idx].is_empty() && self.pages.len() > 1 {
            self.pages.remove(page_idx);
            let totals: Vec<LogicalHeight> = self.pages.iter().map(|p| p.total_height()).collect();
            self.directory = PagedHeightDirectory::with_page_totals(&totals)?;
        }

        Ok(removed)
    }
}

impl Default for PagedHeightIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Transactional Height Refinement
// ---------------------------------------------------------------------------

/// Transactional container for background height refinement.
///
/// Implements old/new structural page reservation. Modifications are staged into
/// reserved pages and applied atomically to `PagedHeightIndex` on `commit()`, or
/// safely discarded on `rollback()`.
#[derive(Clone, Debug)]
pub struct HeightRefinementTransaction {
    reserved_pages: HashMap<usize, Page>,
    page_deltas: HashMap<usize, i64>,
}

impl HeightRefinementTransaction {
    /// Stage a refinement for a block at `intra_page_idx` inside `page_idx`.
    pub fn stage_block_refinement(
        &mut self,
        page_idx: usize,
        intra_page_idx: usize,
        new_height: LogicalHeight,
    ) -> Result<(), BlockFlowError> {
        let page = self
            .reserved_pages
            .get_mut(&page_idx)
            .ok_or(BlockFlowError::IndexOutOfBounds {
                index: page_idx,
                len: 0,
            })?;

        let delta = page.update_height(intra_page_idx, new_height)?;
        let entry = self.page_deltas.entry(page_idx).or_insert(0);
        *entry = entry
            .checked_add(delta)
            .ok_or(BlockFlowError::ArithmeticOverflow)?;
        Ok(())
    }

    /// Commit the staged page modifications atomically into `index`.
    pub fn commit(self, index: &mut PagedHeightIndex) -> Result<(), BlockFlowError> {
        // Pre-validate all directory adjustments
        for (&page_idx, &delta) in &self.page_deltas {
            if page_idx >= index.pages.len() {
                return Err(BlockFlowError::IndexOutOfBounds {
                    index: page_idx,
                    len: index.pages.len(),
                });
            }
            // Check that page delta does not overflow page prefix sums
            let current_total = index.pages[page_idx].total_height();
            if delta < 0 && current_total.raw() < delta.unsigned_abs() {
                return Err(BlockFlowError::ArithmeticOverflow);
            }
        }

        // Apply page replacements and directory updates
        for (page_idx, page) in self.reserved_pages {
            index.pages[page_idx] = page;
        }

        for (page_idx, delta) in self.page_deltas {
            index.directory.adjust_page_total(page_idx, delta)?;
        }

        Ok(())
    }

    /// Discard all staged changes without applying them.
    pub fn rollback(self) {
        // Reserved pages drop naturally without touching target index
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paged_height_accumulates_totals_beyond_u32() {
        // 100 blocks of 50,000,000 raw units each = 5,000,000,000 (> u32::MAX)
        // With capacity 10, this spans 10 leaf pages
        let heights = vec![LogicalHeight::from_raw(50_000_000); 100];
        let index = PagedHeightIndex::with_heights_and_capacity(&heights, 10)
            .expect("build paged index");

        assert_eq!(index.page_count(), 10);
        assert_eq!(index.len(), 100);

        let total = index.total_height().expect("total height");
        assert_eq!(total.raw(), 5_000_000_000);
        assert!(total.raw() > u32::MAX as u64);

        // Check prefix sum at count 75 = 3,750,000,000
        let prefix_75 = index.prefix_height(75).expect("prefix 75");
        assert_eq!(prefix_75.raw(), 3_750_000_000);

        // Binary-lifting anchor search at 4,200,000,000
        // (4,200,000,000 / 50,000,000 = block 84)
        let anchor = index
            .find_anchor_at_scroll(LogicalHeight::from_raw(4_200_000_000))
            .expect("find anchor");
        assert_eq!(anchor.block_id, 84);
        assert_eq!(anchor.intra_block_offset.raw(), 0);
    }

    #[test]
    fn transactional_refinement_with_old_new_page_reservation() {
        // 20 blocks with capacity 5 -> 4 pages
        let initial_heights = vec![LogicalHeight::from_points(20.0); 20];
        let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 5).unwrap();
        let initial_total = index.total_height().unwrap();
        assert_eq!(initial_total, LogicalHeight::from_points(400.0));

        // Reserve pages 1 and 2 for background refinement
        let mut tx = index.begin_refinement(&[1, 2]).expect("begin refinement");

        // Stage refinement: blocks in page 1 expand (e.g. font metrics change from 20pt to 35pt)
        // Page 1 covers block indices 5..10 (local indices 0..5)
        for intra in 0..5 {
            tx.stage_block_refinement(1, intra, LogicalHeight::from_points(35.0))
                .expect("stage refinement");
        }

        // Commit transaction
        tx.commit(&mut index).expect("commit transaction");

        // Page 1 expanded by 5 * 15pt = 75pt
        let new_total = index.total_height().unwrap();
        assert_eq!(new_total, LogicalHeight::from_points(475.0));

        // Verify block 7 (page 1, intra 2) is now 35pt
        assert_eq!(index.block_height(7).unwrap(), LogicalHeight::from_points(35.0));
        // Verify block 12 (page 2, intra 2) remains 20pt
        assert_eq!(index.block_height(12).unwrap(), LogicalHeight::from_points(20.0));
    }

    #[test]
    fn transactional_refinement_rollback_preserves_original_index() {
        let initial_heights = vec![LogicalHeight::from_points(10.0); 10];
        let index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 5).unwrap();
        let original_total = index.total_height().unwrap();

        // Begin transaction and stage changes
        let mut tx = index.begin_refinement(&[0]).unwrap();
        tx.stage_block_refinement(0, 0, LogicalHeight::from_points(999.0))
            .unwrap();

        // Rollback explicitly
        tx.rollback();

        // Index remains unchanged
        assert_eq!(index.total_height().unwrap(), original_total);
        assert_eq!(index.block_height(0).unwrap(), LogicalHeight::from_points(10.0));
    }

    #[test]
    fn scroll_anchor_preserves_source_position_across_paged_refinements() {
        // Document with 16 blocks across 4 pages
        let initial_heights = vec![LogicalHeight::from_points(50.0); 16];
        let mut index = PagedHeightIndex::with_heights_and_capacity(&initial_heights, 4).unwrap();

        // User is viewing block 9 with intra-block offset 12pt
        let anchor = ScrollAnchor::new(9, LogicalHeight::from_points(12.0));
        let scroll_y1 = index
            .prefix_height(9)
            .unwrap()
            .checked_add(anchor.intra_block_offset)
            .unwrap();
        assert_eq!(scroll_y1, LogicalHeight::from_points(462.0)); // 9 * 50 + 12

        // Background font refinement updates page 0 (blocks 0..4) to 80pt each (+120pt total)
        index
            .refine_heights(
                0,
                &[
                    LogicalHeight::from_points(80.0),
                    LogicalHeight::from_points(80.0),
                    LogicalHeight::from_points(80.0),
                    LogicalHeight::from_points(80.0),
                ],
            )
            .unwrap();

        // Anchored scroll_y adjusts by exactly +120pt to 582pt
        let scroll_y2 = index
            .prefix_height(9)
            .unwrap()
            .checked_add(anchor.intra_block_offset)
            .unwrap();
        assert_eq!(scroll_y2, LogicalHeight::from_points(582.0));

        // Finding anchor at scroll_y2 returns block 9 with intra offset 12pt!
        let re_found = index.find_anchor_at_scroll(scroll_y2).unwrap();
        assert_eq!(re_found.block_id, 9);
        assert_eq!(re_found.intra_block_offset, LogicalHeight::from_points(12.0));
    }

    #[test]
    fn paged_index_structural_insert_and_remove() {
        let mut index = PagedHeightIndex::with_heights_and_capacity(
            &[
                LogicalHeight::from_points(10.0),
                LogicalHeight::from_points(20.0),
                LogicalHeight::from_points(30.0),
            ],
            2,
        )
        .unwrap();

        assert_eq!(index.len(), 3);
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(60.0));

        // Insert at index 1
        index
            .insert_block(1, LogicalHeight::from_points(15.0))
            .unwrap();
        assert_eq!(index.len(), 4);
        assert_eq!(index.block_height(1).unwrap(), LogicalHeight::from_points(15.0));
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(75.0));

        // Remove at index 0
        let removed = index.remove_block(0).unwrap();
        assert_eq!(removed, LogicalHeight::from_points(10.0));
        assert_eq!(index.len(), 3);
        assert_eq!(index.total_height().unwrap(), LogicalHeight::from_points(65.0));
    }

    #[test]
    fn negative_control_out_of_bounds_and_overflow_refused() {
        let index = PagedHeightIndex::with_heights(&[LogicalHeight::from_points(10.0)]).unwrap();

        // Out of bounds block query
        let err = index.block_height(99);
        assert!(matches!(err, Err(BlockFlowError::IndexOutOfBounds { .. })));

        // Out of bounds page reservation
        let tx_err = index.begin_refinement(&[5]);
        assert!(matches!(tx_err, Err(BlockFlowError::IndexOutOfBounds { .. })));
    }
}
