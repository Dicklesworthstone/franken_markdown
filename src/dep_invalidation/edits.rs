//! Atomic multi-range editor transactions in pre-edit UTF-8 coordinates.

use super::{FlowAssetReuse, FlowSession, FlowSessionError, FlowUpdate};
use crate::flow_display::{FlowInlineStyle, FlowTextRole};
use crate::span::SourceSpan;
use crate::text::OwnedTextRun;
use std::fmt;

/// Maximum operations in one transaction; checked before allocating a sort index.
pub const MAX_FLOW_EDITS: usize = 4096;

/// One replacement against the same captured source revision as every other
/// edit in its batch. An empty span inserts; an empty replacement deletes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowEdit<'a> {
    pub span: SourceSpan,
    pub replacement: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FlowEditBatchError {
    Session(FlowSessionError),
    /// Indices in the caller's original batch, not the sorted working index.
    OverlappingEdits { first: usize, second: usize },
    TooManyEdits { count: usize, maximum: usize },
}

impl fmt::Display for FlowEditBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(f, "{error}"),
            Self::OverlappingEdits { first, second } => write!(f,
                "edits {first} and {second} overlap in the pre-edit source; submit non-overlapping ranges"),
            Self::TooManyEdits { count, maximum } => write!(f,
                "edit batch has {count} operations; maximum is {maximum}"),
        }
    }
}

impl std::error::Error for FlowEditBatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Session(error) => Some(error),
            _ => None,
        }
    }
}

impl From<FlowSessionError> for FlowEditBatchError {
    fn from(error: FlowSessionError) -> Self { Self::Session(error) }
}

impl FlowSession {
    /// Apply a multi-cursor edit, replace-all, or editor transaction atomically.
    /// All ranges refer to `expected_revision`, never intermediate strings.
    /// Input order need not be sorted. Disjoint/adjacent replacements are valid;
    /// overlapping replacements and insertions inside replaced text are refused.
    /// Insertions at a replacement's start precede it; at its end they follow it.
    /// Several insertions at the same position retain their caller order.
    ///
    /// Staleness, count, UTF-8 boundaries, overlap, arithmetic and FINAL source
    /// size are checked before copying source or invoking the shaper. The final
    /// candidate goes through the existing transactional render path exactly
    /// once (that path still reparses whole documents). A failed stage publishes
    /// nothing; success advances each revision once. Empty/identity batches do
    /// not shape, invalidate assets, or advance either revision.
    pub fn edit_many<F>(
        &mut self,
        expected_revision: u64,
        edits: &[FlowEdit<'_>],
        reuse: FlowAssetReuse,
        shape: F,
    ) -> Result<FlowUpdate, FlowEditBatchError>
    where F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
    {
        if expected_revision != self.revision() {
            return Err(FlowSessionError::StaleRevision {
                expected: self.revision(), actual: expected_revision,
            }.into());
        }
        let source = prepare_edits(self.source(), edits, self.engine().limits().max_source_bytes)?;
        match source {
            Some(source) => self.replace_source(expected_revision, &source, reuse, shape).map_err(Into::into),
            None => self.edit(expected_revision, SourceSpan::new(0, 0), "", reuse, shape).map_err(Into::into),
        }
    }
}

fn prepare_edits(
    source: &str,
    edits: &[FlowEdit<'_>],
    maximum: usize,
) -> Result<Option<String>, FlowEditBatchError> {
    if edits.len() > MAX_FLOW_EDITS {
        return Err(FlowEditBatchError::TooManyEdits { count: edits.len(), maximum: MAX_FLOW_EDITS });
    }
    for edit in edits {
        if edit.span.slice(source).is_none() {
            return Err(FlowSessionError::InvalidEdit { span: edit.span, source_len: source.len() }.into());
        }
    }
    let mut order: Vec<_> = (0..edits.len()).collect();
    // Include the caller index so even an unstable sort gives deterministic
    // ordering to simultaneous insertions, without retaining replacement copies.
    order.sort_unstable_by_key(|&i| (edits[i].span.start, edits[i].span.end, i));
    let budget = || FlowEditBatchError::Session(FlowSessionError::SourceBudgetExceeded { maximum });
    let mut removed = 0usize;
    let mut added = 0usize;
    let mut previous: Option<usize> = None;
    let mut changed = false;
    for &index in &order {
        let edit = edits[index];
        if let Some(before) = previous {
            if edit.span.start < edits[before].span.end {
                return Err(FlowEditBatchError::OverlappingEdits { first: before, second: index });
            }
        }
        removed = removed.checked_add(edit.span.len()).ok_or_else(budget)?;
        added = added.checked_add(edit.replacement.len()).ok_or_else(budget)?;
        changed |= edit.span.slice(source) != Some(edit.replacement);
        previous = Some(index);
    }
    let required = source.len().checked_sub(removed)
        .and_then(|len| len.checked_add(added))
        .filter(|&len| len <= maximum).ok_or_else(budget)?;
    if !changed { return Ok(None); }
    let mut result = String::with_capacity(required);
    let mut cursor = 0;
    for index in order {
        let edit = edits[index];
        result.push_str(&source[cursor..edit.span.start]);
        result.push_str(edit.replacement);
        cursor = edit.span.end;
    }
    result.push_str(&source[cursor..]);
    // Distinct adjacent replacements can collectively be an identity edit.
    Ok((result != source).then_some(result))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::flow_display::{FlowDisplayLimits, FlowLayoutOptions};
    use crate::fonts::BundledFlowFonts;
    use crate::FontFamily;

    fn edit(start: usize, end: usize, replacement: &str) -> FlowEdit<'_> {
        FlowEdit { span: SourceSpan::new(start, end), replacement }
    }

    #[test]
    fn unordered_unicode_edits_use_original_byte_coordinates() {
        let source = "aé😀z\r\nend";
        let edits = [edit(10, 13, "fin"), edit(3, 7, "X"), edit(1, 3, "β")];
        assert_eq!(prepare_edits(source, &edits, 100).unwrap().as_deref(), Some("aβXz\r\nfin"));
    }

    #[test]
    fn adjacent_ranges_and_simultaneous_insertions_have_explicit_order() {
        let edits = [edit(3, 4, "D"), edit(1, 1, "X"), edit(1, 3, "BC"), edit(3, 3, "Y")];
        assert_eq!(prepare_edits("abcd", &edits, 100).unwrap().as_deref(), Some("aXBCYD"));
        let edits = [edit(1, 1, "second"), edit(1, 1, "first")];
        assert_eq!(prepare_edits("ab", &edits, 100).unwrap().as_deref(), Some("asecondfirstb"));
    }

    #[test]
    fn conflicting_ranges_and_invalid_utf8_fail_before_building_a_candidate() {
        for edits in [
            vec![edit(0, 3, "x"), edit(2, 4, "y")],
            vec![edit(0, 4, "x"), edit(1, 2, "y")],
            vec![edit(1, 3, "x"), edit(2, 2, "y")],
            vec![edit(1, 3, "x"), edit(1, 3, "x")],
        ] {
            assert!(matches!(prepare_edits("abcd", &edits, 100), Err(FlowEditBatchError::OverlappingEdits { .. })));
        }
        for span in [SourceSpan::new(2, 3), SourceSpan::new(4, 1), SourceSpan::new(0, usize::MAX)] {
            assert!(matches!(prepare_edits("aéz", &[FlowEdit { span, replacement: "" }], 100),
                Err(FlowEditBatchError::Session(FlowSessionError::InvalidEdit { .. }))));
        }
        let error = prepare_edits("abcd", &[edit(2, 4, ""), edit(1, 3, "")], 100).unwrap_err();
        assert_eq!(error, FlowEditBatchError::OverlappingEdits { first: 1, second: 0 });
    }

    #[test]
    fn admission_uses_final_size_not_an_intermediate_sequential_edit() {
        let edits = [edit(0, 0, "ABCD"), edit(0, 4, "")];
        assert_eq!(prepare_edits("abcd", &edits, 4).unwrap().as_deref(), Some("ABCD"));
        assert!(matches!(prepare_edits("abcd", &[edit(0, 0, "x")], 4),
            Err(FlowEditBatchError::Session(FlowSessionError::SourceBudgetExceeded { maximum: 4 }))));
        let edits = vec![edit(0, 0, ""); MAX_FLOW_EDITS + 1];
        assert!(matches!(prepare_edits("", &edits, 0), Err(FlowEditBatchError::TooManyEdits { .. })));
    }

    #[test]
    fn empty_and_collective_identity_edits_do_not_publish_source_changes() {
        assert_eq!(prepare_edits("ab", &[], 2).unwrap(), None);
        assert_eq!(prepare_edits("ab", &[edit(0, 1, "a"), edit(1, 2, "b")], 2).unwrap(), None);
        assert_eq!(prepare_edits("ab", &[edit(0, 1, "ab"), edit(1, 2, "")], 2).unwrap(), None);
    }

    #[test]
    fn successful_batch_renders_one_final_snapshot_and_preserves_middle_reuse() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let source = "before\n\nstable\n\nafter\n";
        let mut doc = FlowSession::new(source, 2, FlowDisplayLimits::default(), FlowLayoutOptions::default(),
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        let update = doc.edit_many(1, &[edit(0, 6, "first"), edit(16, 21, "last")], FlowAssetReuse::Invalidate,
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.source(), "first\n\nstable\n\nlast\n");
        assert_eq!(doc.revision(), 2);
        assert_eq!(doc.layout_revision(), 2);
        assert_eq!(update.changes.dirty_blocks, vec![0, 2]);
        assert_eq!(update.changes.reusable.len(), 1);
        let fresh = FlowSession::new(doc.source(), 2, FlowDisplayLimits::default(), doc.layout_options(),
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        assert_eq!(doc.display(), fresh.display());
    }

    #[test]
    fn rejected_batches_leave_source_display_assets_and_revisions_unchanged() {
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        let mut doc = FlowSession::new("before\n\n![image](image.png)\n\nafter\n", 2,
            FlowDisplayLimits::default(), FlowLayoutOptions::default(),
            |t, s, r, i| fonts.shape(t, s, r, i)).unwrap();
        let source = doc.source().to_owned();
        let display = doc.display().clone();
        let pending = doc.pending_assets().to_vec();
        assert!(matches!(doc.edit_many(0, &[], FlowAssetReuse::Invalidate, |_, _, _, _| panic!("stale")),
            Err(FlowEditBatchError::Session(FlowSessionError::StaleRevision { .. }))));
        assert!(doc.edit_many(1, &[edit(0, 4, "x"), edit(2, 5, "y")], FlowAssetReuse::Invalidate,
            |_, _, _, _| panic!("invalid ranges must not shape")).is_err());
        assert!(doc.edit_many(1, &[edit(0, 6, "new")], FlowAssetReuse::Invalidate,
            |_, _, _, _| Err("deliberate layout failure".to_owned())).is_err());
        assert_eq!(doc.source(), source);
        assert_eq!(doc.display(), &display);
        assert_eq!(doc.pending_assets(), pending);
        assert_eq!(doc.revision(), 1);
        assert_eq!(doc.layout_revision(), 1);
        let no_change = doc.edit_many(1, &[], FlowAssetReuse::Invalidate,
            |_, _, _, _| panic!("identity batch must not shape")).unwrap();
        assert!(!no_change.source_changed);
        assert_eq!(doc.pending_assets(), pending);
        assert_eq!(doc.revision(), 1);
        assert_eq!(doc.layout_revision(), 1);
    }
}
