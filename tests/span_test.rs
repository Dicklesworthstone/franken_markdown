//! Unit coverage for `SourceSpan` arithmetic (`src/span.rs`), bead grn.5.4.
//!
//! These assertions exist to be mutation-resistant: every method is pinned to its
//! exact behavior on BOTH sides of each boundary (empty vs non-empty, in vs out of
//! range, ordered vs reversed), so a mutant that flips a comparison or returns a
//! constant is caught. Added after `scripts/mutation.sh` reported survivors here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::SourceSpan;

#[test]
fn new_and_len_measure_the_byte_range() {
    let s = SourceSpan::new(3, 10);
    assert_eq!(s.start, 3);
    assert_eq!(s.end, 10);
    assert_eq!(s.len(), 7);
    // A zero-width span has length 0; a reversed range saturates to 0 (not wraps).
    assert_eq!(SourceSpan::new(5, 5).len(), 0);
    assert_eq!(SourceSpan::new(8, 3).len(), 0);
}

#[test]
fn is_empty_is_true_only_for_zero_or_reversed_width() {
    // Kills the `is_empty -> true` mutant: a real non-empty span must be NON-empty.
    assert!(!SourceSpan::new(0, 1).is_empty());
    assert!(!SourceSpan::new(3, 10).is_empty());
    // ...and an empty or reversed span must be empty (kills `is_empty -> false`).
    assert!(SourceSpan::new(4, 4).is_empty());
    assert!(SourceSpan::new(9, 2).is_empty());
}

#[test]
fn contains_is_half_open_start_inclusive_end_exclusive() {
    let s = SourceSpan::new(2, 5);
    assert!(s.contains(2)); // start is inclusive
    assert!(s.contains(4));
    assert!(!s.contains(5)); // end is exclusive
    assert!(!s.contains(1)); // before start
    assert!(!s.contains(6)); // after end
    // An empty span contains nothing.
    assert!(!SourceSpan::new(3, 3).contains(3));
}

#[test]
fn merge_spans_the_outermost_bounds() {
    let a = SourceSpan::new(2, 5);
    let b = SourceSpan::new(8, 12);
    let m = a.merge(b);
    assert_eq!(m.start, 2); // min of starts
    assert_eq!(m.end, 12); // max of ends
    // Merge is symmetric and absorbs a contained span.
    assert_eq!(b.merge(a), m);
    assert_eq!(a.merge(SourceSpan::new(3, 4)), a);
}

#[test]
fn slice_returns_the_covered_text_or_none_on_bad_bounds() {
    let src = "hello world";
    assert_eq!(SourceSpan::new(0, 5).slice(src), Some("hello"));
    assert_eq!(SourceSpan::new(6, 11).slice(src), Some("world"));
    assert_eq!(SourceSpan::new(5, 5).slice(src), Some("")); // empty but valid
    // Reversed bounds yield None (the `start <= end` guard), not a panic.
    assert_eq!(SourceSpan::new(7, 3).slice(src), None);
    // Out-of-range / non-char-boundary yields None via str::get.
    assert_eq!(SourceSpan::new(0, 99).slice(src), None);
    assert_eq!(SourceSpan::new(0, 1).slice("é"), None); // mid-codepoint
}

#[test]
fn default_span_is_empty_at_origin() {
    let d = SourceSpan::default();
    assert_eq!(d.start, 0);
    assert_eq!(d.end, 0);
    assert!(d.is_empty());
}
