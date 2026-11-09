//! Original-source coordinates composed through includes and URL rewrites.
//! Maps own bounded metadata, never source bodies or filesystem authority.

use std::ops::Range;

use crate::transclude::ExpandedDocument;

const MAX_SEGMENTS: usize = 65_536;
const MAX_PATH_BYTES: usize = 256 * 1024;
const MAX_PATH: usize = 4096;
const MAX_RANGES: usize = 64;

/// How a published range relates to the original source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookSourceRangeKind {
    /// The selected bytes were copied unchanged from this source range.
    Exact,
    /// URL rebasing replaced this complete original token. Its interior does
    /// not have a one-to-one byte correspondence with the published token.
    Rewritten,
    /// Expansion inserted a separator at this original source boundary.
    Generated,
}

impl BookSourceRangeKind {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Rewritten => "rewritten",
            Self::Generated => "generated",
        }
    }
}

/// A verified half-open UTF-8 range in a captured original chapter or include.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSourceRange {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub kind: BookSourceRangeKind,
}

/// Complete attribution for one parser finding. Multiple ranges remain
/// separate: an expanded warning can cross files or repeated include instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookSourceLocation {
    Mapped(Vec<BookSourceRange>),
    /// Stable reason; an unavailable location never carries a partial range set.
    Unavailable(&'static str),
}

#[derive(Debug, Clone)]
struct Fragment {
    source: usize,
    original: Range<usize>,
    kind: BookSourceRangeKind,
}

#[derive(Debug, Clone)]
struct Segment {
    expanded: Range<usize>,
    fragments: Vec<Fragment>,
}

/// A map has nonoverlapping, ordered output segments. Rewritten segments may
/// name several original fragments, but never interpolate between their bytes.
#[derive(Debug, Clone)]
pub(crate) struct SourceMap {
    length: usize,
    paths: Vec<String>,
    segments: Vec<Segment>,
    reason: Option<&'static str>,
}

/// Shared across a complete book; many small chapters cannot each claim the
/// full metadata budget. Exhaustion disables attribution, not publication.
#[derive(Default)]
pub(crate) struct SourceMapBudget {
    segments: usize,
    paths: usize,
}

impl SourceMapBudget {
    pub(crate) fn admit(&mut self, map: SourceMap) -> SourceMap {
        let units = map
            .segments
            .iter()
            .map(|s| s.fragments.len())
            .sum::<usize>();
        let paths = map.paths.iter().map(String::len).sum::<usize>();
        if units > MAX_SEGMENTS.saturating_sub(self.segments)
            || paths > MAX_PATH_BYTES.saturating_sub(self.paths)
        {
            return SourceMap::unavailable(map.length, "provenance_limit");
        }
        self.segments += units;
        self.paths += paths;
        map
    }
}

impl SourceMap {
    pub(crate) const fn len(&self) -> usize {
        self.length
    }

    pub(crate) fn identity(path: &str, length: usize) -> Self {
        if path.len() > MAX_PATH {
            return Self::unavailable(length, "provenance_limit");
        }
        Self {
            length,
            paths: vec![path.to_owned()],
            segments: vec![Segment {
                expanded: 0..length,
                fragments: vec![Fragment {
                    source: 0,
                    original: 0..length,
                    kind: BookSourceRangeKind::Exact,
                }],
            }],
            reason: None,
        }
    }

    pub(crate) fn unavailable(length: usize, reason: &'static str) -> Self {
        Self {
            length,
            paths: Vec::new(),
            segments: Vec::new(),
            reason: Some(reason),
        }
    }

    pub(crate) fn from_expanded(
        document: &ExpandedDocument,
        logical_origin: &impl Fn(&str) -> Option<String>,
    ) -> Self {
        let length = document.text.len();
        if document.spans.len() > MAX_SEGMENTS || document.sources.len() > 4096 {
            return Self::unavailable(length, "provenance_limit");
        }
        let mut paths = Vec::with_capacity(document.sources.len());
        let mut bytes = 0;
        for source in &document.sources {
            let Some(path) = logical_origin(source) else {
                return Self::unavailable(length, "source_unavailable");
            };
            if path.len() > MAX_PATH || path.len() > MAX_PATH_BYTES - bytes {
                return Self::unavailable(length, "provenance_limit");
            }
            bytes += path.len();
            paths.push(path);
        }
        let mut segments = Vec::with_capacity(document.spans.len());
        let mut frontier = 0;
        for span in &document.spans {
            if span.expanded.start != frontier
                || span.expanded.end > length
                || span.expanded.start > span.expanded.end
                || span.source_id >= paths.len()
                || span.original.start > span.original.end
                || (!span.generated && span.expanded.len() != span.original.len())
                || (span.generated && !span.original.is_empty())
            {
                return Self::unavailable(length, "invalid_provenance");
            }
            frontier = span.expanded.end;
            segments.push(Segment {
                expanded: span.expanded.clone(),
                fragments: vec![Fragment {
                    source: span.source_id,
                    original: span.original.clone(),
                    kind: if span.generated {
                        BookSourceRangeKind::Generated
                    } else {
                        BookSourceRangeKind::Exact
                    },
                }],
            });
        }
        if frontier != length {
            return Self::unavailable(length, "invalid_provenance");
        }
        Self {
            length,
            paths,
            segments,
            reason: None,
        }
    }

    /// Compose edits once. `edits` are the actual sorted parser-owned URL
    /// replacements already admitted by the rebasing operation.
    pub(crate) fn rebase(mut self, edits: &[(Range<usize>, String)], length: usize) -> Self {
        if let Some(reason) = self.reason {
            return Self::unavailable(length, reason);
        }
        if edits.is_empty() {
            return self;
        }
        let mut segments = Vec::new();
        let mut before = 0;
        let mut after = 0;
        let mut units = 0;
        for (range, replacement) in edits {
            if before > range.start || range.start > range.end || range.end > self.length {
                return Self::unavailable(length, "invalid_provenance");
            }
            if !self.copy_segments(before..range.start, &mut after, &mut segments, &mut units) {
                return Self::unavailable(length, "provenance_limit");
            }
            let Some(mut fragments) = self.fragments(range.clone(), MAX_RANGES) else {
                return Self::unavailable(length, "provenance_limit");
            };
            for fragment in &mut fragments {
                if fragment.kind != BookSourceRangeKind::Generated {
                    fragment.kind = BookSourceRangeKind::Rewritten;
                }
            }
            units += fragments.len();
            if units > MAX_SEGMENTS {
                return Self::unavailable(length, "provenance_limit");
            }
            segments.push(Segment {
                expanded: after..after + replacement.len(),
                fragments,
            });
            after += replacement.len();
            before = range.end;
        }
        if !self.copy_segments(before..self.length, &mut after, &mut segments, &mut units)
            || after != length
        {
            return Self::unavailable(length, "provenance_limit");
        }
        self.length = length;
        self.segments = segments;
        self
    }

    fn copy_segments(
        &self,
        range: Range<usize>,
        after: &mut usize,
        output: &mut Vec<Segment>,
        units: &mut usize,
    ) -> bool {
        if range.is_empty() {
            return true;
        }
        let index = self
            .segments
            .partition_point(|segment| segment.expanded.end <= range.start);
        for segment in &self.segments[index..] {
            if segment.expanded.start >= range.end {
                break;
            }
            let start = segment.expanded.start.max(range.start);
            let end = segment.expanded.end.min(range.end);
            let Some(fragments) = Self::clip(segment, start..end) else {
                return false;
            };
            *units += fragments.len();
            if *units > MAX_SEGMENTS {
                return false;
            }
            output.push(Segment {
                expanded: *after..*after + end - start,
                fragments,
            });
            *after += end - start;
        }
        true
    }

    fn clip(segment: &Segment, range: Range<usize>) -> Option<Vec<Fragment>> {
        let mut fragments = segment.fragments.clone();
        for fragment in &mut fragments {
            if fragment.kind == BookSourceRangeKind::Exact {
                let start = fragment
                    .original
                    .start
                    .checked_add(range.start - segment.expanded.start)?;
                let end = start.checked_add(range.len())?;
                if end > fragment.original.end {
                    return None;
                }
                fragment.original = start..end;
            }
        }
        Some(fragments)
    }

    fn fragments(&self, range: Range<usize>, maximum: usize) -> Option<Vec<Fragment>> {
        if range.start > range.end || range.end > self.length {
            return None;
        }
        let mut index = self
            .segments
            .partition_point(|segment| segment.expanded.end <= range.start);
        if range.is_empty() && index == self.segments.len() {
            index = index.checked_sub(1)?;
        }
        let mut output: Vec<Fragment> = Vec::new();
        for segment in &self.segments[index..] {
            if segment.expanded.start >= range.end && !range.is_empty() {
                break;
            }
            let start = segment.expanded.start.max(range.start);
            let end = segment.expanded.end.min(range.end);
            if start > end {
                return None;
            }
            for fragment in Self::clip(segment, start..end)? {
                if let Some(previous) = output.last_mut()
                    && previous.source == fragment.source
                    && previous.kind == fragment.kind
                    && previous.original.end == fragment.original.start
                {
                    previous.original.end = fragment.original.end;
                } else {
                    if output.len() == maximum {
                        return None;
                    }
                    output.push(fragment);
                }
            }
            if range.is_empty() {
                break;
            }
        }
        (!output.is_empty()).then_some(output)
    }

    pub(crate) fn locate(&self, range: Range<usize>) -> BookSourceLocation {
        if let Some(reason) = self.reason {
            return BookSourceLocation::Unavailable(reason);
        }
        let Some(fragments) = self.fragments(range, MAX_RANGES) else {
            return BookSourceLocation::Unavailable("range_limit");
        };
        BookSourceLocation::Mapped(
            fragments
                .into_iter()
                .map(|fragment| BookSourceRange {
                    path: self.paths[fragment.source].clone(),
                    start: fragment.original.start,
                    end: fragment.original.end,
                    kind: fragment.kind,
                })
                .collect(),
        )
    }
}

#[cfg(test)]
#[path = "source_locations_tests.rs"]
mod tests;
