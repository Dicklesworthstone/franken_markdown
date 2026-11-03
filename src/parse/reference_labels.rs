//! Parser-owned reference-label locations for source editing.

use std::ops::Range;

/// The spelling of an accepted reference definition or resolved reference use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReferenceLabelForm {
    /// `[label]: destination` (including unused or shadowed definitions).
    Definition,
    /// `[text][label]` or `![alt][label]`.
    Full,
    /// `[label][]` or `![label][]`.
    Collapsed,
    /// `[label]` or `![label]`.
    Shortcut,
}

/// One label recognized by the ordinary Markdown parser.
///
/// The lookup key follows the parser's pinned Unicode fold and whitespace
/// rules. `label` selects the source label without its brackets. `replacement`
/// is the range to edit when changing that key while preserving visible text:
/// replace it with the new label for definitions/full uses, or `[new label]`
/// for collapsed/shortcut uses. Shortcut replacement ranges are insertions.
///
/// A range is `None` when transformed container text or multiline labels have
/// no single exact source range. Consumers must refuse an incomplete rename
/// instead of silently omitting those occurrences, and reparse proposed edits
/// to prove that destinations, titles, images and surrounding content survive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceLabelSpan {
    pub key: String,
    pub form: ReferenceLabelForm,
    pub label: Option<Range<usize>>,
    pub replacement: Option<Range<usize>>,
}
