//! Shared CJK pair boundaries for bounded Flow shaping and final wrapping.
//!
//! These are opportunities between already shaped clusters, not permission to
//! divide a cluster. Latin words keep their existing separator policy, and a
//! measurement boundary never becomes a visible line break by itself.

use crate::layout::{cjk_break_allowed, cjk_break_prohibited};

/// Whether an interior UTF-8 boundary admits a break because one side is CJK.
pub(super) fn cjk_boundary(text: &str, at: usize) -> bool {
    pair(text, at).is_some_and(|(left, right)| {
        // A joiner belongs to the surrounding sequence even though the shared
        // CJK pair classifier groups it with marks that otherwise bind left.
        left != '\u{200d}' && cjk_break_allowed(left, right)
    })
}

/// Whether a cluster boundary would strand CJK punctuation or a bound mark.
pub(super) fn prohibited_boundary(text: &str, at: usize) -> bool {
    pair(text, at).is_some_and(|(left, right)| {
        cjk_break_prohibited(left, right)
            || (left == '\u{200d}' && crate::layout::is_cjk_char(right))
    })
}

fn pair(text: &str, at: usize) -> Option<(char, char)> {
    let left = text.get(..at)?.chars().next_back()?;
    let right = text.get(at..)?.chars().next()?;
    Some((left, right))
}

/// Latest safe work boundary in `(start, start + limit]`, before `end`.
///
/// Preserve the established cut BEFORE an ASCII space/tab. CJK adds cuts at
/// allowed scalar pairs, including CJK/Latin boundaries; a base and its mark
/// are never divided. Callers must still retain their last contextual shaped
/// cluster/group and publish only final, reshaped lines. No larger shaping
/// callback or unbounded scan is needed to locate the boundary.
pub(super) fn measurement_boundary(
    text: &str,
    start: usize,
    end: usize,
    limit: usize,
) -> Option<usize> {
    let source = text.get(start..end)?;
    let cap = start.saturating_add(limit).min(end);
    let mut latest = None;
    let mut previous = None;
    for (offset, ch) in source.char_indices() {
        let at = start + offset;
        if at > cap {
            break;
        }
        if at > start
            && ((at < cap && matches!(ch, ' ' | '\t'))
                || previous.is_some_and(|left| left != '\u{200d}' && cjk_break_allowed(left, ch)))
        {
            latest = Some(at);
        }
        previous = Some(ch);
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_cuts_keep_latin_words_marks_and_cjk_nonstarters_together() {
        assert_eq!(
            measurement_boundary("alpha beta gamma", 0, 16, 11),
            Some(10)
        );
        assert_eq!(measurement_boundary("abcdefghijk", 0, 11, 8), None);
        assert_eq!(measurement_boundary("中文漢字", 0, 12, 8), Some(6));
        assert_eq!(measurement_boundary("中文。漢字", 0, 15, 8), Some(3));
        assert_eq!(measurement_boundary("中a\u{0301}漢字", 0, 12, 5), Some(3));
        assert_eq!(measurement_boundary("中a\u{0301}漢字", 0, 12, 6), Some(6));
        assert_eq!(measurement_boundary("「中文」漢", 0, 15, 4), None);
    }

    #[test]
    fn boundaries_only_apply_at_utf8_pairs_and_preserve_cjk_attachments() {
        assert!(cjk_boundary("漢a\u{0301}", 3));
        assert!(!cjk_boundary("漢a\u{0301}", 4));
        assert!(!cjk_boundary("漢字", 1));
        assert!(!cjk_boundary("漢字", 0));
        assert!(!cjk_boundary("漢字", 6));
        assert!(prohibited_boundary("中文。", 6));
        assert!(prohibited_boundary("「中文", 3));
        assert!(!prohibited_boundary("alpha", 2));
        assert!(!cjk_boundary("漢\u{200d}字", 6));
        assert!(prohibited_boundary("漢\u{200d}字", 6));
    }
}
