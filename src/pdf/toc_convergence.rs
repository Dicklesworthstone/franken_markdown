//! Conservative page-label columns for a TOC that did not reach a fixed point.
//!
//! A proportional numeral can make an entry alternate between one and two
//! physical lines as its destination changes. Keep the original measured
//! columns for the ordinary passes, then reserve the widest observed label for
//! each heading and font size. A reserved column can grow, but never shrink.

use crate::layout::FontSize;
use std::collections::BTreeMap;

/// Preserve the existing layout path and bytes for ordinary convergent TOCs.
pub(super) const ORDINARY_PASSES: usize = 5;
/// Bound pathological re-layout even when fonts or layout policies keep
/// changing geometry for reasons beyond the page-label advance width.
pub(super) const MAX_PASSES: usize = 16;

#[derive(Debug, Clone, Default)]
pub(super) struct PageLabelWidths {
    conservative: bool,
    by_size: BTreeMap<FontSize, f32>,
}

impl PageLabelWidths {
    pub(super) fn reserve(&mut self) {
        self.conservative = true;
    }

    pub(super) fn column_width(&mut self, size: FontSize, measured: f32) -> f32 {
        let widest = self.by_size.entry(size).or_insert(measured);
        *widest = widest.max(measured);
        if self.conservative { *widest } else { measured }
    }
}
