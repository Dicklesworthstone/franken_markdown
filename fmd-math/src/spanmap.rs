//! The span map (§11.3): querying a [`Layout`] by source provenance.
//!
//! The Reference obtains substring→glyph maps by rendering every string
//! *twice* through its black-box typesetters with injected color labels
//! and aligning the two renders. This engine is not a black box: every
//! output primitive already names the byte range it came from, so the
//! span map is a **query**, not a reconstruction.
//!
//! Semantics: [`Layout::select`] returns the primitives whose spans are
//! **contained** in the query range. Containment (not overlap) is the
//! sound choice for substring maps: the `π` glyph of `\pi` carries the
//! whole command's span, so a query for the source letter `i` inside
//! `\pi` selects nothing — no false positives on command-name substrings.
//! [`find_occurrences`] locates a needle's byte occurrences in the
//! source, which composes with `select` into exactly the
//! `tex_to_color_map` / `isolate` / `TransformMatchingTex` consumption
//! pattern: match by *source identity*, never by shape correlation.
//!
//! ## The synthetic-span policy (documented for the inspector)
//!
//! Every primitive span points into the source string and is non-empty:
//!
//! - character glyphs carry their exact token span; text-run characters
//!   carry per-character spans (escapes cover their two source bytes,
//!   collapsed whitespace its run);
//! - primes carry their own `'` token spans;
//! - glyphs produced by a command (`\pi`, operator-name letters, accent
//!   marks, delimiters after `\left`) carry the producing command's span —
//!   the expansion site, exactly as macro expansion should;
//! - rules carry their construct's span (a fraction bar belongs to the
//!   whole fraction);
//! - built-in default-pack expansions (`\minus`, `\mathds`) map to the
//!   command occurrence in the source.

use crate::mbox::Layout;
use crate::node::Span;

/// Indices of the primitives a span query selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// Indices into [`Layout::glyphs`].
    pub glyphs: Vec<usize>,
    /// Indices into [`Layout::rules`].
    pub rules: Vec<usize>,
    /// Indices into [`Layout::paths`].
    pub paths: Vec<usize>,
}

impl Selection {
    /// True when nothing was selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty() && self.rules.is_empty() && self.paths.is_empty()
    }

    /// Total number of selected primitives.
    #[must_use]
    pub fn len(&self) -> usize {
        self.glyphs.len() + self.rules.len() + self.paths.len()
    }
}

const fn contained(inner: &Span, outer: Span) -> bool {
    inner.start >= outer.start && inner.end <= outer.end
}

const fn overlaps(a: &Span, b: Span) -> bool {
    a.start < b.end && b.start < a.end
}

impl Layout {
    /// The primitives whose spans are contained in `range` — the substring
    /// map (`isolate` / `tex_to_color_map` semantics).
    #[must_use]
    pub fn select(&self, range: Span) -> Selection {
        Selection {
            glyphs: indices(self.glyphs.iter().map(|g| &g.span), |s| contained(s, range)),
            rules: indices(self.rules.iter().map(|r| &r.span), |s| contained(s, range)),
            paths: indices(self.paths.iter().map(|p| &p.span), |s| contained(s, range)),
        }
    }

    /// The primitives whose spans overlap `range` — the inspector's
    /// hit-test semantics (what did this byte produce, in any part).
    #[must_use]
    pub fn select_touching(&self, range: Span) -> Selection {
        Selection {
            glyphs: indices(self.glyphs.iter().map(|g| &g.span), |s| overlaps(s, range)),
            rules: indices(self.rules.iter().map(|r| &r.span), |s| overlaps(s, range)),
            paths: indices(self.paths.iter().map(|p| &p.span), |s| overlaps(s, range)),
        }
    }
}

fn indices<'a>(
    spans: impl Iterator<Item = &'a Span>,
    mut keep: impl FnMut(&Span) -> bool,
) -> Vec<usize> {
    spans
        .enumerate()
        .filter_map(|(i, s)| keep(s).then_some(i))
        .collect()
}

/// Byte-level occurrences of `needle` in `source` (non-overlapping, left
/// to right) — the string side of the `t2c`/`isolate` consumption pattern.
#[must_use]
pub fn find_occurrences(source: &str, needle: &str) -> Vec<Span> {
    if needle.is_empty() {
        return Vec::new();
    }
    source
        .match_indices(needle)
        .map(|(i, m)| Span::new(i, i + m.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occurrences_are_byte_spans() {
        assert_eq!(
            find_occurrences("a+a", "a"),
            vec![Span::new(0, 1), Span::new(2, 3)]
        );
        assert!(find_occurrences("abc", "").is_empty());
        assert_eq!(find_occurrences(r"\pi\pi", r"\pi").len(), 2);
    }
}
