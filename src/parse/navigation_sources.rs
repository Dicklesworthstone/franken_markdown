//! Optional source ownership for the links created by one parser invocation.
//!
//! An owned, nonempty String keeps its allocation when its Inline node moves
//! through delimiter resolution and block assembly. These addresses are only
//! equality keys while that very AST is alive; nothing is dereferenced through
//! an address, and no address escapes into retained book metadata. Every link
//! constructor records its allocation, including unsupported/unknown spans, so
//! a reused allocation cannot inherit a discarded speculative link's location.

use std::collections::BTreeMap;
use std::ops::Range;

const MAX_CAPTURED_TOKENS: usize = 65_536;

#[derive(Default)]
pub(crate) struct NavigationCapture {
    tokens: BTreeMap<(u8, usize, usize, usize), Option<Range<usize>>>,
    exhausted: bool,
}

impl NavigationCapture {
    fn key(kind: u8, text: &str, index: usize) -> (u8, usize, usize, usize) {
        (kind, text.as_ptr() as usize, text.len(), index)
    }

    fn insert(&mut self, kind: u8, text: &str, index: usize, range: Option<Range<usize>>) {
        if self.exhausted || text.is_empty() {
            return;
        }
        let key = Self::key(kind, text, index);
        if self.tokens.len() == MAX_CAPTURED_TOKENS && !self.tokens.contains_key(&key) {
            self.tokens.clear();
            self.exhausted = true;
            return;
        }
        self.tokens.insert(key, range);
    }

    pub(super) fn link(&mut self, destination: &str, range: Option<Range<usize>>) {
        self.insert(0, destination, 0, range);
    }

    pub(super) fn html(&mut self, html: &str, index: usize, range: Option<Range<usize>>) {
        self.insert(1, html, index, range);
    }

    pub(crate) fn link_range(&self, destination: &str) -> Option<Range<usize>> {
        self.tokens.get(&Self::key(0, destination, 0))?.clone()
    }

    pub(crate) fn html_range(&self, html: &str, index: usize) -> Option<Range<usize>> {
        self.tokens.get(&Self::key(1, html, index))?.clone()
    }

    pub(super) fn restore(&mut self, original: impl Fn(Range<usize>) -> Range<usize>) {
        for range in self.tokens.values_mut().flatten() {
            *range = original(range.clone());
        }
    }
}
