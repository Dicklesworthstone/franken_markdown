//! Split-point plans for the three-way resumable-lexer split corpora.
//!
//! A three-way split test runs one full resumable lex for every
//! `(first, second)` pair, which is about `n^2 / 2` runs of `O(n)` work each.
//! That is fine for the short adversarial fixtures but not for the 1-2 KB
//! consumer documents: a 2017-byte C++ document needs ~2.03M runs, which
//! takes tens of minutes in a debug build and looks like a hang under
//! `cargo test --workspace`.
//!
//! Short fixtures keep every split pair. Longer ones use a bounded set of
//! split points with varied residues, so the default suite stays fast. Each
//! corpus keeps an `#[ignore]`d exhaustive variant for deliberate deep runs.

#![allow(dead_code)]

/// Fixtures up to this many bytes get every split point.
pub const EXHAUSTIVE_MAX_LEN: usize = 256;

/// Approximate number of split points sampled from a longer fixture.
pub const SAMPLED_POINTS: usize = 64;

/// Sorted, deduplicated split points in `0..=len`. Every point is included
/// when `exhaustive` is set or `len <= EXHAUSTIVE_MAX_LEN`. Otherwise about
/// `SAMPLED_POINTS` points are returned, always including `0`, `1`, `len - 1`,
/// and `len`. Offsets inside each stride vary, so samples are not aligned to
/// one residue class.
pub fn split_points(len: usize, exhaustive: bool) -> Vec<usize> {
    if exhaustive || len <= EXHAUSTIVE_MAX_LEN {
        return (0..=len).collect();
    }
    let stride = len.div_ceil(SAMPLED_POINTS);
    let mut points: Vec<usize> = (0..SAMPLED_POINTS)
        .map(|k| k * stride + (k * 7) % stride)
        .filter(|&point| point <= len)
        .collect();
    points.extend([0, 1, len - 1, len]);
    points.sort_unstable();
    points.dedup();
    points
}

/// `(first, second)` pairs with `first < len` and `first <= second <= len`,
/// matching the exhaustive three-way loop's domain.
pub fn three_way_pairs(len: usize, exhaustive: bool) -> Vec<(usize, usize)> {
    let points = split_points(len, exhaustive);
    let mut pairs = Vec::new();
    for &first in points.iter().filter(|&&point| point < len) {
        for &second in points.iter().filter(|&&point| point >= first) {
            pairs.push((first, second));
        }
    }
    pairs
}
