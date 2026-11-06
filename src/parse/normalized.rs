//! CommonMark's NUL replacement at source admission, with original coordinates.
//!
//! Recognition must see U+FFFD even in labels, destinations, code and raw HTML.
//! The ordinary parser and its copied-text trackers therefore share one normalized
//! buffer. Only published ranges are translated back to the caller's bytes.

use std::borrow::Cow;
use std::cell::OnceCell;
use std::ops::Range;

use crate::span::{SourceSpan, SpannedDocument};

pub(super) struct NormalizedSource<'a> {
    original: &'a str,
    text: Cow<'a, str>,
    /// End offsets of replacements in the normalized buffer. Ordinary parsing
    /// needs no map; build it only when an editor/source query publishes ranges.
    replacement_ends: OnceCell<Vec<usize>>,
}

impl<'a> NormalizedSource<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            original: source,
            text: if source.contains('\0') {
                Cow::Owned(source.replace('\0', "\u{fffd}"))
            } else {
                Cow::Borrowed(source)
            },
            replacement_ends: OnceCell::new(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    fn original_offset(&self, offset: usize) -> usize {
        if matches!(self.text, Cow::Borrowed(_)) {
            return offset;
        }
        let ends = self.replacement_ends.get_or_init(|| {
            self.original
                .match_indices('\0')
                .enumerate()
                .map(|(index, (start, _))| start + index * 2 + 3)
                .collect()
        });
        // Every parser endpoint is a UTF-8 character boundary. Replacing one
        // byte with three adds exactly two bytes before each completed U+FFFD.
        offset - ends.partition_point(|&end| end <= offset) * 2
    }

    pub fn original_range(&self, range: Range<usize>) -> Range<usize> {
        self.original_offset(range.start)..self.original_offset(range.end)
    }

    fn original_span(&self, span: SourceSpan) -> SourceSpan {
        SourceSpan::new(
            self.original_offset(span.start),
            self.original_offset(span.end),
        )
    }

    pub fn restore_spanned(&self, mut document: SpannedDocument) -> SpannedDocument {
        if matches!(self.text, Cow::Owned(_)) {
            for block in &mut document.blocks {
                block.span = self.original_span(block.span);
            }
            for diagnostic in &mut document.diagnostics {
                diagnostic.span = self.original_span(diagnostic.span);
            }
            document.source_len = self.original.len();
        }
        document
    }
}
