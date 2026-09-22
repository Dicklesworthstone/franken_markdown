//! Browser-coordinate and packed-WASM adapters for atomic core edit batches.

use super::{BrowserFlowError, BrowserFlowSession, MAX_BROWSER_SOURCE_BYTES};
use crate::dep_invalidation::{FlowAssetReuse, FlowEdit, FlowEditBatchError, FlowSessionError, MAX_FLOW_EDITS};
use crate::SourceSpan;

/// A replacement using UTF-16 code-unit offsets in the pre-edit source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16Edit<'a> {
    pub start: usize,
    pub end: usize,
    pub replacement: &'a str,
}

fn check_count(count: usize) -> Result<(), BrowserFlowError> {
    if count > MAX_FLOW_EDITS {
        return Err(FlowEditBatchError::TooManyEdits { count, maximum: MAX_FLOW_EDITS }.into());
    }
    Ok(())
}

// Sort endpoints once and scan source only forward. Repeated positions do not
// rescan; a surrogate interior is rejected rather than rounded. The output stays
// in caller order, which is significant for simultaneous insertions.
fn to_byte_edits<'a>(source: &str, edits: &[Utf16Edit<'a>]) -> Result<Vec<FlowEdit<'a>>, BrowserFlowError> {
    check_count(edits.len())?;
    let mut points = Vec::with_capacity(edits.len() * 2);
    let mut converted = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        if edit.start > edit.end {
            return Err(BrowserFlowError::InvalidSelection);
        }
        points.push((edit.start, index, false));
        points.push((edit.end, index, true));
        converted.push(FlowEdit { span: SourceSpan::new(0, 0), replacement: edit.replacement });
    }
    points.sort_unstable();
    let mut chars = source.char_indices();
    let mut units = 0;
    let mut byte = 0;
    for (offset, index, is_end) in points {
        while units < offset {
            let (start, ch) = chars.next().ok_or(BrowserFlowError::InvalidSelection)?;
            units += ch.len_utf16();
            byte = start + ch.len_utf8();
        }
        if units != offset {
            return Err(BrowserFlowError::InvalidSelection);
        }
        if is_end {
            converted[index].span.end = byte;
        } else {
            converted[index].span.start = byte;
        }
    }
    Ok(converted)
}

impl BrowserFlowSession {
    /// Commit a byte-coordinate batch with one revision/layout publication.
    pub fn edit_many_bytes(&mut self, revision: u64, edits: &[FlowEdit<'_>], reuse: FlowAssetReuse)
        -> Result<(), BrowserFlowError>
    {
        let fonts = &mut self.fonts;
        self.session.edit_many(revision, edits, reuse,
            |text, size, role, style| fonts.shape(text, size, role, style))?;
        self.source_utf16_len = self.source().encode_utf16().count();
        Ok(())
    }

    /// Apply a multi-cursor/editor transaction against one UTF-16 source snapshot.
    /// Any bad range, overlapping replacement or failed layout rejects the WHOLE
    /// batch. Source and layout revisions advance once, not once per cursor.
    pub fn edit_many_utf16(&mut self, revision: u64, edits: &[Utf16Edit<'_>], reuse: FlowAssetReuse)
        -> Result<(), BrowserFlowError>
    {
        self.check_revision(revision)?;
        let converted = to_byte_edits(self.source(), edits)?;
        self.edit_many_bytes(revision, &converted, reuse)
    }

    /// Data-only WASM seam: pairs of pre-edit UTF-16 endpoints, one UTF-8 byte
    /// length per replacement, and their concatenated text. All metadata must
    /// account for the payload exactly; no JSON parser or ambient I/O is needed.
    /// JS checks count/bytes before conversion; direct binding callers are also
    /// checked here before constructing the native transaction.
    pub fn edit_many_utf16_packed(
        &mut self, revision: u64, ranges: &[u32], lengths: &[u32], replacements: &str,
        reuse: FlowAssetReuse,
    ) -> Result<(), BrowserFlowError> {
        self.check_revision(revision)?;
        check_count(lengths.len())?;
        if ranges.len() != lengths.len() * 2 {
            return Err(BrowserFlowError::InvalidEditBatch);
        }
        if replacements.len() > MAX_BROWSER_SOURCE_BYTES {
            return Err(FlowSessionError::SourceBudgetExceeded { maximum: MAX_BROWSER_SOURCE_BYTES }.into());
        }
        let mut edits = Vec::with_capacity(lengths.len());
        let mut cursor = 0usize;
        for (range, &length) in ranges.chunks_exact(2).zip(lengths) {
            let end = cursor.checked_add(length as usize).ok_or(BrowserFlowError::InvalidEditBatch)?;
            let replacement = replacements.get(cursor..end).ok_or(BrowserFlowError::InvalidEditBatch)?;
            edits.push(Utf16Edit { start: range[0] as usize, end: range[1] as usize, replacement });
            cursor = end;
        }
        if cursor != replacements.len() {
            return Err(BrowserFlowError::InvalidEditBatch);
        }
        self.edit_many_utf16(revision, &edits, reuse)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::flow_display::FlowLayoutOptions;

    fn edit(start: usize, end: usize, replacement: &str) -> Utf16Edit<'_> {
        Utf16Edit { start, end, replacement }
    }

    fn session(source: &str) -> BrowserFlowSession {
        BrowserFlowSession::new(source, "sans", FlowLayoutOptions::default()).unwrap()
    }

    #[test]
    fn endpoint_scan_matches_independent_conversion_in_every_unicode_range() {
        for source in ["", "ascii", "aé😀z\r\n", "𐐀😀é"] {
            let len = source.encode_utf16().count();
            for start in 0..=len + 1 {
                for end in 0..=len + 1 {
                    let expected = super::super::utf16_span(source, start, end);
                    let actual = to_byte_edits(source, &[edit(start, end, "")]);
                    assert_eq!(actual.as_ref().map(|edits| edits[0].span), expected.as_ref().copied());
                }
            }
        }
        let edits = [edit(4, 5, "last"), edit(1, 1, "a"), edit(1, 1, "b"), edit(2, 4, "emoji")];
        let converted = to_byte_edits("aé😀z", &edits).unwrap();
        for (before, after) in edits.iter().zip(converted) {
            assert_eq!(after.span, super::super::utf16_span("aé😀z", before.start, before.end).unwrap());
            assert_eq!(after.replacement, before.replacement);
        }
    }

    #[test]
    fn browser_batch_commits_once_and_updates_utf16_metadata() {
        let mut state = session("café\n\nlast");
        state.edit_many_utf16(1, &[edit(6, 10, "fin"), edit(3, 4, "e")], FlowAssetReuse::Invalidate).unwrap();
        assert_eq!(state.source(), "cafe\n\nfin");
        assert_eq!(state.revision(), 2);
        assert_eq!(state.layout_revision(), 2);
        assert_eq!(state.source_utf16_len, 9);
        state.edit_many_utf16(2, &[], FlowAssetReuse::Invalidate).unwrap();
        assert_eq!((state.revision(), state.layout_revision()), (2, 2));
    }

    #[test]
    fn packed_utf8_lengths_are_not_utf16_lengths() {
        let mut state = session("abc");
        state.edit_many_utf16_packed(1, &[0, 1, 2, 3], &[2, 1], "éz", FlowAssetReuse::Invalidate).unwrap();
        assert_eq!(state.source(), "ébz");
        assert_eq!(state.source_utf16_len, 3);
        assert_eq!(state.revision(), 2);
    }

    #[test]
    fn malformed_packed_batches_preserve_the_entire_snapshot() {
        let mut state = session("abc");
        let before = state.snapshot_json(1, 1, 0, 100, true).unwrap();
        for (ranges, lengths, text) in [
            (vec![0], vec![1], "x"),
            (vec![0, 1], vec![1], "é"),
            (vec![0, 1], vec![2], "x"),
            (vec![0, 1], vec![0], "x"),
            (vec![], vec![], "x"),
        ] {
            let error = state.edit_many_utf16_packed(1, &ranges, &lengths, text, FlowAssetReuse::Invalidate).unwrap_err();
            assert_eq!(error.code(), "INVALID_EDIT_BATCH");
            assert_eq!(state.source(), "abc");
            assert_eq!(state.snapshot_json(1, 1, 0, 100, true).unwrap(), before);
        }
        let error = state.edit_many_utf16_packed(0, &[0], &[], "x", FlowAssetReuse::Invalidate).unwrap_err();
        assert_eq!(error.code(), "STALE_REVISION");
    }

    #[test]
    fn overlap_and_layout_failure_leave_source_assets_and_revisions_intact() {
        let mut state = session("before\n\n![plot](plot.png)\n\nafter");
        let source = state.source().to_owned();
        let display = state.session.display().clone();
        let requests = state.pending_assets_json(1, 0, 10).unwrap();
        let error = state.edit_many_utf16(1, &[edit(0, 4, "x"), edit(2, 5, "y")], FlowAssetReuse::Invalidate).unwrap_err();
        assert_eq!(error.code(), "OVERLAPPING_EDITS");
        assert!(state.edit_many_utf16(1, &[edit(0, 6, "😀"), edit(source.len() - 5, source.len(), "last")],
            FlowAssetReuse::Invalidate).is_err());
        assert_eq!(state.source(), source);
        assert_eq!(state.session.display(), &display);
        assert_eq!(state.pending_assets_json(1, 0, 10).unwrap(), requests);
        assert_eq!(state.source_utf16_len, source.encode_utf16().count());
        assert_eq!((state.revision(), state.layout_revision()), (1, 1));
    }

    #[test]
    fn packed_count_and_aggregate_bytes_are_bounded() {
        let mut state = session("a");
        let error = state.edit_many_utf16_packed(1, &[], &vec![0; MAX_FLOW_EDITS + 1], "",
            FlowAssetReuse::Invalidate).unwrap_err();
        assert_eq!(error.code(), "BUDGET_EXCEEDED");
        let huge = "x".repeat(MAX_BROWSER_SOURCE_BYTES + 1);
        let error = state.edit_many_utf16_packed(1, &[0, 1], &[huge.len() as u32], &huge,
            FlowAssetReuse::Invalidate).unwrap_err();
        assert_eq!(error.code(), "BUDGET_EXCEEDED");
        assert_eq!(state.source(), "a");
        assert_eq!((state.revision(), state.layout_revision()), (1, 1));
    }
}
