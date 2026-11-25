//! Authoritative source mapping and preview-to-source synchronization (FCB-073.B).
//!
//! Plan §12.4 & §27.4:
//! - "Every interactive rendered element identifies the source spans from which it
//!   was derived, using an upstream nested provenance graph rather than a top-level
//!   block envelope."
//! - "Source mapping can return a disjoint ordered set of ranges; the caller must
//!   not invent contiguous source by concatenating unrelated spans and presenting
//!   it as the original literal slice. 'Copy enclosing Markdown block' is a
//!   separate useful command."
//! - "Preview selection offers two explicit actions: copy rendered reading text
//!   and copy corresponding Markdown source."
//! - "Following a heading link resolves the heading identity, not a cached pixel
//!   position."

#![forbid(unsafe_code)]

use std::fmt;

mod inline;

use crate::ast::{Block, Inline, List, Table};
use crate::html::inlines_to_plain;
use crate::span::{
    CaptureId, DisjointSourceRanges, NestedProvenanceGraph, NestedProvenanceNode, ProvenanceError,
    ProvenanceKind, ProvenanceRelation, QualifiedSpan, SourceOrigin, SourceSpan, SpannedBlock,
    SpannedDocument,
};

/// A selection range in rendered reading text coordinates: `[start, end)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TextSelectionRange {
    /// Inclusive start byte offset in rendered text.
    pub start: usize,
    /// Exclusive end byte offset in rendered text.
    pub end: usize,
}

impl TextSelectionRange {
    /// Create a new selection range.
    #[inline]
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Length in bytes, saturating to zero if reversed.
    #[inline]
    #[must_use]
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// True when the selection range has no byte width.
    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// True when `offset` is inside `[start, end)`.
    #[inline]
    #[must_use]
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// True when this range overlaps with another selection range.
    #[inline]
    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        !self.is_empty() && !other.is_empty() && self.start < other.end && other.start < self.end
    }
}

/// Errors arising during source mapping or copy operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceMapError {
    /// The supplied AST or source bounds do not belong to this source capture.
    SourceMismatch,
    /// The parser could not retain complete inline provenance within its bounds.
    InlineSourceUnavailable,
    /// Provided selection range is invalid (reversed, out of bounds, or splits UTF-8).
    InvalidSelectionRange {
        start: usize,
        end: usize,
        max: usize,
    },
    /// Requested byte offset is outside the source text bounds.
    SourceOutOfBounds { offset: usize, source_len: usize },
    /// Target heading slug could not be resolved.
    HeadingNotFound { slug: String },
    /// Attempted to present disjoint source ranges as an invented contiguous slice.
    ///
    /// Plan §12.4: "the caller must not invent contiguous source by concatenating
    /// unrelated spans and presenting it as the original literal slice."
    InventedContiguous {
        enclosing: SourceSpan,
        disjoint_count: usize,
    },
    /// Upstream provenance validation error.
    Provenance(ProvenanceError),
}

impl fmt::Display for SourceMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceMismatch => write!(
                f,
                "document and source do not match; parse this exact source before building its source map"
            ),
            Self::InlineSourceUnavailable => write!(
                f,
                "complete inline source provenance is unavailable within the parser's tracking limits"
            ),
            Self::InvalidSelectionRange { start, end, max } => {
                write!(
                    f,
                    "invalid selection range [{}, {}), rendered length is {}",
                    start, end, max
                )
            }
            Self::SourceOutOfBounds { offset, source_len } => {
                write!(
                    f,
                    "source byte offset {} is out of bounds for source length {}",
                    offset, source_len
                )
            }
            Self::HeadingNotFound { slug } => {
                write!(f, "heading with slug '{}' not found in source map", slug)
            }
            Self::InventedContiguous {
                enclosing,
                disjoint_count,
            } => {
                write!(
                    f,
                    "rejected invented contiguous source [{}, {}) across {} disjoint ranges",
                    enclosing.start, enclosing.end, disjoint_count
                )
            }
            Self::Provenance(err) => write!(f, "provenance error: {}", err),
        }
    }
}

impl std::error::Error for SourceMapError {}

impl From<ProvenanceError> for SourceMapError {
    fn from(err: ProvenanceError) -> Self {
        Self::Provenance(err)
    }
}

/// An authoritative heading anchor linking heading identity to its source location.
///
/// Plan §12.4: "Following a heading link resolves the heading identity, not a cached pixel position."
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingSourceAnchor {
    /// Canonical heading slug (e.g. `introduction`, `getting-started-2`).
    pub slug: String,
    /// Plain-text heading title.
    pub title: String,
    /// Heading level (1 to 6).
    pub level: u8,
    /// Exact byte span in the original source capture.
    pub source_span: SourceSpan,
    /// Source capture origin (primary or transcluded).
    pub origin: SourceOrigin,
    /// Index of the enclosing block in the spanned document.
    pub block_index: usize,
    /// Byte offset in the rendered reading text where this heading starts.
    pub rendered_offset: usize,
}

/// An interactive rendered element or text run mapped to original source bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedElement {
    /// Unique element index in layout order.
    pub id: u64,
    /// Byte range in rendered text coordinates: `[start, end)`.
    pub rendered_range: TextSelectionRange,
    /// Disjoint source ranges from which this element was derived.
    pub source_ranges: DisjointSourceRanges,
    /// Exact provenance relation (Literal, Escape, Entity, StrippedDelimiter, GeneratedMarker, etc.).
    pub relation: ProvenanceRelation,
    /// Rendered text slice for this element.
    pub rendered_text: String,
    /// True if synthetic / generated without literal counterpart.
    pub is_generated: bool,
    /// Enclosing block index in the spanned document.
    pub block_index: usize,
}

/// Document source map providing bidirectional source-to-layout synchronization,
/// heading anchor resolution, and truthful copy semantics without per-frame cloning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSourceMap {
    /// Total rendered reading text produced for the document.
    rendered_text: String,
    /// Elements mapped between rendered text and source spans.
    elements: Vec<RenderedElement>,
    /// Heading anchors indexed for constant-time or linear identity resolution.
    headings: Vec<HeadingSourceAnchor>,
    /// Nested provenance graph for the entire document hierarchy.
    provenance_graph: NestedProvenanceGraph,
    /// Total primary source length in bytes.
    source_len: usize,
    /// Enclosing source spans for all top-level blocks.
    block_spans: Vec<SourceSpan>,
}

impl DocumentSourceMap {
    /// Build a document source map from matching parsed nodes and source text.
    ///
    /// Inline ranges come from the normal parser's accepted tokens, including
    /// complete escapes/entities and disjoint container source slices. Generated
    /// list markers have no exact editable ranges; enclosing-block copy remains
    /// available. An AST or block span that disagrees with this source returns
    /// [`SourceMapError::SourceMismatch`] instead of assigning bytes by searching
    /// display text. This proves structural agreement, not capture identity:
    /// differently spelled sources may legitimately parse to the same nodes.
    pub fn from_spanned_document(
        doc: &SpannedDocument,
        source: &str,
    ) -> Result<Self, SourceMapError> {
        let inline_sources = inline::InlineSources::capture(doc, source)?;
        let mut builder = SourceMapBuilder::new(doc.source_len, inline_sources);
        builder.build(doc, source)?;
        builder.finish()
    }

    /// Borrow the complete rendered reading text.
    #[inline]
    #[must_use]
    pub fn rendered_text(&self) -> &str {
        &self.rendered_text
    }

    /// Length in bytes of the complete rendered reading text.
    #[inline]
    #[must_use]
    pub fn rendered_len(&self) -> usize {
        self.rendered_text.len()
    }

    /// Length in bytes of the original primary source.
    #[inline]
    #[must_use]
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// Borrow all interactive rendered elements.
    #[inline]
    #[must_use]
    pub fn elements(&self) -> &[RenderedElement] {
        &self.elements
    }

    /// Borrow authoritative anchors for primary, top-level headings.
    ///
    /// IDs follow actual HTML publication allocation, including collisions
    /// with headings inside lists and quotes. Nested headings and note bodies
    /// do not acquire invented exact source ranges here.
    #[inline]
    #[must_use]
    pub fn headings(&self) -> &[HeadingSourceAnchor] {
        &self.headings
    }

    /// Borrow the nested provenance graph.
    #[inline]
    #[must_use]
    pub fn provenance_graph(&self) -> &NestedProvenanceGraph {
        &self.provenance_graph
    }

    // -----------------------------------------------------------------------
    // Truthful Copy APIs (Plan §12.4)
    // -----------------------------------------------------------------------

    /// Copy rendered reading text corresponding to `range`.
    ///
    /// Plan §12.4: "Preview selection offers two explicit actions: copy rendered reading
    /// text and copy corresponding Markdown source."
    pub fn copy_rendered_text(&self, range: TextSelectionRange) -> Result<String, SourceMapError> {
        self.validate_selection_range(range)?;
        Ok(self.rendered_text[range.start..range.end].to_string())
    }

    /// Copy the enclosing Markdown source slice covering all elements in `range`.
    ///
    /// Plan §12.4: "'Copy enclosing Markdown block' is a separate useful command."
    pub fn copy_enclosing_source<'s>(
        &self,
        range: TextSelectionRange,
        source: &'s str,
    ) -> Result<&'s str, SourceMapError> {
        self.validate_selection_range(range)?;

        let overlapping = self.elements_overlapping(range);
        if overlapping.is_empty() {
            // Check if selection is zero-width at an element boundary
            if let Some(e) = self.element_at_rendered_offset(range.start)
                && let Some(enclosing) = self
                    .block_spans
                    .get(e.block_index)
                    .copied()
                    .or_else(|| e.source_ranges.enclosing_span())
            {
                return enclosing
                    .slice(source)
                    .ok_or(SourceMapError::SourceOutOfBounds {
                        offset: enclosing.end,
                        source_len: source.len(),
                    });
            }
            return Err(SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: self.rendered_text.len(),
            });
        }

        let mut min_start = usize::MAX;
        let mut max_end = 0;

        for e in overlapping {
            if let Some(bspan) = self.block_spans.get(e.block_index) {
                min_start = min_start.min(bspan.start);
                max_end = max_end.max(bspan.end);
            } else {
                for span in e.source_ranges.spans() {
                    min_start = min_start.min(span.start);
                    max_end = max_end.max(span.end);
                }
            }
        }

        if min_start > max_end || max_end > source.len() {
            return Err(SourceMapError::SourceOutOfBounds {
                offset: max_end,
                source_len: source.len(),
            });
        }

        source
            .get(min_start..max_end)
            .ok_or(SourceMapError::SourceOutOfBounds {
                offset: max_end,
                source_len: source.len(),
            })
    }

    /// Copy the enclosing Markdown block covering all elements in `range`.
    ///
    /// Plan §12.4: "'Copy enclosing Markdown block' is a separate useful command."
    pub fn copy_enclosing_block<'s>(
        &self,
        range: TextSelectionRange,
        source: &'s str,
    ) -> Result<&'s str, SourceMapError> {
        self.copy_enclosing_source(range, source)
    }

    /// Extract the exact disjoint source ranges for all elements in `range`.
    pub fn copy_exact_source_ranges(
        &self,
        range: TextSelectionRange,
    ) -> Result<Vec<QualifiedSpan>, SourceMapError> {
        self.validate_selection_range(range)?;

        let mut qualified = Vec::new();
        for e in self.elements_overlapping(range) {
            let origin = e.source_ranges.origin();
            for span in e.source_ranges.spans() {
                qualified.push(QualifiedSpan {
                    origin,
                    span: *span,
                });
            }
        }
        Ok(qualified)
    }

    /// Extract separate string slices for each disjoint source range in `range`.
    ///
    /// Never concatenates disjoint ranges into an invented contiguous slice!
    pub fn copy_exact_source_slices<'s>(
        &self,
        range: TextSelectionRange,
        source: &'s str,
    ) -> Result<Vec<&'s str>, SourceMapError> {
        let ranges = self.copy_exact_source_ranges(range)?;
        let mut slices = Vec::with_capacity(ranges.len());
        for q in ranges {
            if q.origin != SourceOrigin::Primary {
                // Transclusion slices belong to another capture
                continue;
            }
            let slice = q
                .span
                .slice(source)
                .ok_or(SourceMapError::SourceOutOfBounds {
                    offset: q.span.end,
                    source_len: source.len(),
                })?;
            slices.push(slice);
        }
        Ok(slices)
    }

    /// Reject an attempt to present disjoint source ranges as an invented contiguous slice.
    ///
    /// Plan §12.4: "the caller must not invent contiguous source by concatenating unrelated spans
    /// and presenting it as the original literal slice."
    pub fn reject_invented_contiguous(
        &self,
        range: TextSelectionRange,
        purported_span: SourceSpan,
    ) -> Result<(), SourceMapError> {
        let ranges = self.copy_exact_source_ranges(range)?;
        if ranges.len() > 1 {
            let min_start = ranges.iter().map(|q| q.span.start).min().unwrap_or(0);
            let max_end = ranges.iter().map(|q| q.span.end).max().unwrap_or(0);
            let enclosing = SourceSpan::new(min_start, max_end);
            if purported_span == enclosing {
                return Err(SourceMapError::InventedContiguous {
                    enclosing,
                    disjoint_count: ranges.len(),
                });
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Anchor & Synchronization APIs (Plan §12.4)
    // -----------------------------------------------------------------------

    /// Resolve a heading identity (slug) to its authoritative source anchor.
    ///
    /// Plan §12.4: "Following a heading link resolves the heading identity, not a cached pixel position."
    #[must_use]
    pub fn resolve_source_anchor(&self, heading_slug: &str) -> Option<&HeadingSourceAnchor> {
        self.headings.iter().find(|h| h.slug == heading_slug)
    }

    /// Map a primary source byte offset to the corresponding rendered text byte offset.
    #[must_use]
    pub fn sync_source_to_rendered(&self, source_offset: usize) -> Option<usize> {
        for e in &self.elements {
            if e.source_ranges.origin() == SourceOrigin::Primary {
                for span in e.source_ranges.spans() {
                    if span.contains(source_offset) {
                        let local = source_offset.saturating_sub(span.start);
                        let mapped = e
                            .rendered_range
                            .start
                            .saturating_add(local.min(e.rendered_range.len()));
                        return Some(mapped);
                    }
                }
            }
        }
        None
    }

    /// Map a rendered text byte offset to the corresponding primary source byte offset.
    #[must_use]
    pub fn sync_rendered_to_source(&self, rendered_offset: usize) -> Option<usize> {
        let elem = self.element_at_rendered_offset(rendered_offset)?;
        if elem.source_ranges.origin() == SourceOrigin::Primary
            && let Some(span) = elem.source_ranges.spans().first()
        {
            let local = rendered_offset.saturating_sub(elem.rendered_range.start);
            let mapped = span.start.saturating_add(local.min(span.len()));
            return Some(mapped);
        }
        None
    }

    /// Return the interactive rendered element containing the given rendered byte offset.
    #[must_use]
    pub fn element_at_rendered_offset(&self, offset: usize) -> Option<&RenderedElement> {
        self.elements
            .iter()
            .find(|e| e.rendered_range.contains(offset))
    }

    /// Return the interactive rendered element containing the given source byte offset.
    #[must_use]
    pub fn element_at_source_offset(&self, offset: usize) -> Option<&RenderedElement> {
        self.elements.iter().find(|e| {
            e.source_ranges.origin() == SourceOrigin::Primary && e.source_ranges.contains(offset)
        })
    }

    fn validate_selection_range(&self, range: TextSelectionRange) -> Result<(), SourceMapError> {
        if range.start > range.end
            || !self.rendered_text.is_char_boundary(range.start)
            || !self.rendered_text.is_char_boundary(range.end)
        {
            return Err(SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: self.rendered_text.len(),
            });
        }
        Ok(())
    }

    fn elements_overlapping(&self, range: TextSelectionRange) -> Vec<&RenderedElement> {
        self.elements
            .iter()
            .filter(|e| !e.rendered_range.is_empty() && e.rendered_range.overlaps(range))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Internal Builder for DocumentSourceMap
// ---------------------------------------------------------------------------

struct SourceMapBuilder {
    inline_sources: inline::InlineSources,
    rendered_text: String,
    elements: Vec<RenderedElement>,
    headings: Vec<HeadingSourceAnchor>,
    graph_children: Vec<NestedProvenanceNode>,
    heading_identifiers: std::vec::IntoIter<Option<String>>,
    next_element_id: u64,
    source_len: usize,
    block_spans: Vec<SourceSpan>,
}

impl SourceMapBuilder {
    fn new(source_len: usize, inline_sources: inline::InlineSources) -> Self {
        Self {
            inline_sources,
            rendered_text: String::new(),
            elements: Vec::new(),
            headings: Vec::new(),
            graph_children: Vec::new(),
            heading_identifiers: Vec::new().into_iter(),
            next_element_id: 1,
            source_len,
            block_spans: Vec::new(),
        }
    }

    fn build(&mut self, doc: &SpannedDocument, source: &str) -> Result<(), SourceMapError> {
        self.heading_identifiers = crate::html::spanned_heading_identifiers(doc)
            .headings
            .into_iter();
        self.block_spans = doc.iter_blocks().map(|b| b.span).collect();
        for (block_idx, block) in doc.iter_blocks().enumerate() {
            self.build_block(block_idx, block, source)?;
        }
        self.inline_sources.finish()?;
        Ok(())
    }

    fn build_block(
        &mut self,
        block_idx: usize,
        spanned_block: &SpannedBlock,
        source: &str,
    ) -> Result<(), SourceMapError> {
        let block_span = spanned_block.span;
        let mut block_children = Vec::new();

        match &spanned_block.node {
            Block::Heading { level, inlines } => {
                let title = inlines_to_plain(inlines);
                let rendered_offset = self.rendered_text.len();
                if let Some(slug) = self.heading_identifiers.next().flatten() {
                    self.headings.push(HeadingSourceAnchor {
                        slug,
                        title,
                        level: *level,
                        source_span: block_span,
                        origin: SourceOrigin::Primary,
                        block_index: block_idx,
                        rendered_offset,
                    });
                }

                self.append_inlines(block_idx, inlines, block_span, source, &mut block_children)?;
                self.append_rendered_str("\n\n");
            }
            Block::Paragraph(inlines) => {
                self.append_inlines(block_idx, inlines, block_span, source, &mut block_children)?;
                self.append_rendered_str("\n\n");
            }
            Block::CodeBlock { code, .. } => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str(code);
                let end_rendered = self.rendered_text.len();

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
                let elem = RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges.clone(),
                    relation: ProvenanceRelation::Literal,
                    rendered_text: code.clone(),
                    is_generated: false,
                    block_index: block_idx,
                };
                self.elements.push(elem);

                let node = NestedProvenanceNode::leaf(
                    elem_id,
                    ProvenanceKind::Inline,
                    ProvenanceRelation::Literal,
                    ranges,
                )?;
                block_children.push(node);
                self.append_rendered_str("\n\n");
            }
            Block::List(list) => {
                self.build_list(block_idx, list, block_span, source, &mut block_children)?;
            }
            Block::BlockQuote(blocks) => {
                let mut search_cursor = block_span.start;
                for inner in blocks {
                    self.append_inner_block(
                        block_idx,
                        inner,
                        block_span,
                        source,
                        &mut block_children,
                        &mut search_cursor,
                    )?;
                }
            }
            Block::ThematicBreak => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str("---\n\n");
                let end_rendered = self.rendered_text.len().saturating_sub(2);

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
                let elem = RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges.clone(),
                    relation: ProvenanceRelation::Literal,
                    rendered_text: "---".to_string(),
                    is_generated: false,
                    block_index: block_idx,
                };
                self.elements.push(elem);
            }
            Block::Table(table) => {
                self.build_table(block_idx, table, block_span, source, &mut block_children)?;
            }
            Block::HtmlBlock(html) if crate::parse::is_page_break_marker(html) => {
                self.append_rendered_str("\n--- PAGE BREAK ---\n\n");
            }
            Block::HtmlBlock(html) => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str(html);
                let end_rendered = self.rendered_text.len();

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
                self.elements.push(RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges,
                    relation: ProvenanceRelation::Literal,
                    rendered_text: html.clone(),
                    is_generated: false,
                    block_index: block_idx,
                });
                self.append_rendered_str("\n\n");
            }
            Block::FootnoteDefinition { id, blocks } => {
                let start_rendered = self.rendered_text.len();
                let header = format!("[^{}]: ", id);
                self.append_rendered_str(&header);
                let end_rendered = self.rendered_text.len();

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::empty(SourceOrigin::Primary);
                self.elements.push(RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges,
                    relation: ProvenanceRelation::ReferenceDefinition {
                        label: id.clone(),
                        definition_span: block_span,
                    },
                    rendered_text: header,
                    is_generated: true,
                    block_index: block_idx,
                });

                let mut search_cursor = block_span.start;
                for inner in blocks {
                    self.append_inner_block(
                        block_idx,
                        inner,
                        block_span,
                        source,
                        &mut block_children,
                        &mut search_cursor,
                    )?;
                }
            }
            Block::MathBlock(math) => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str(math);
                let end_rendered = self.rendered_text.len();

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
                self.elements.push(RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges,
                    relation: ProvenanceRelation::Literal,
                    rendered_text: math.clone(),
                    is_generated: false,
                    block_index: block_idx,
                });
                self.append_rendered_str("\n\n");
            }
            Block::DefinitionList(items) => {
                let mut search_cursor = block_span.start;
                for item in items {
                    for term in &item.terms {
                        self.append_inlines_at(
                            block_idx,
                            term,
                            block_span,
                            source,
                            &mut block_children,
                            &mut search_cursor,
                        )?;
                        self.append_rendered_str("\n");
                    }
                    for def in &item.definitions {
                        self.append_rendered_str("  : ");
                        self.append_inlines_at(
                            block_idx,
                            def,
                            block_span,
                            source,
                            &mut block_children,
                            &mut search_cursor,
                        )?;
                        self.append_rendered_str("\n");
                    }
                }
                self.append_rendered_str("\n");
            }
            Block::PageBreak => {
                self.append_rendered_str("\n--- PAGE BREAK ---\n\n");
            }
        }

        let block_ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
        let block_node = NestedProvenanceNode::try_new(
            self.next_id(),
            ProvenanceKind::Block,
            ProvenanceRelation::Literal,
            block_ranges,
            block_children,
        )?;
        self.graph_children.push(block_node);

        Ok(())
    }

    fn append_inner_block(
        &mut self,
        block_idx: usize,
        inner: &Block,
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
        search_cursor: &mut usize,
    ) -> Result<(), SourceMapError> {
        match inner {
            Block::Paragraph(inlines) => {
                self.append_inlines_at(
                    block_idx,
                    inlines,
                    block_span,
                    source,
                    block_children,
                    search_cursor,
                )?;
                self.append_rendered_str("\n\n");
            }
            Block::Heading { inlines, .. } => {
                // Nested headings participate in publication ID allocation,
                // including deferred or omitted notes. Retain their slot but
                // do not invent an exact nested heading source range.
                let _ = self.heading_identifiers.next();
                self.append_inlines_at(
                    block_idx,
                    inlines,
                    block_span,
                    source,
                    block_children,
                    search_cursor,
                )?;
                self.append_rendered_str("\n\n");
            }
            Block::CodeBlock { code, .. } => {
                let code_span = Self::find_in_source(source, *search_cursor, block_span.end, code)
                    .unwrap_or_else(|| {
                        let start = *search_cursor;
                        let end = start
                            .saturating_add(code.len())
                            .min(block_span.end)
                            .max(start);
                        SourceSpan::new(start, end)
                    });
                *search_cursor = code_span.end;

                let start_rendered = self.rendered_text.len();
                self.append_rendered_str(code);
                let end_rendered = self.rendered_text.len();

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, code_span)?;
                let elem = RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges.clone(),
                    relation: ProvenanceRelation::Literal,
                    rendered_text: code.clone(),
                    is_generated: false,
                    block_index: block_idx,
                };
                self.elements.push(elem);

                let node = NestedProvenanceNode::leaf(
                    elem_id,
                    ProvenanceKind::Inline,
                    ProvenanceRelation::Literal,
                    ranges,
                )?;
                block_children.push(node);
                self.append_rendered_str("\n\n");
            }
            Block::List(list) => {
                self.build_list_at(
                    block_idx,
                    list,
                    block_span,
                    source,
                    block_children,
                    search_cursor,
                )?;
            }
            Block::BlockQuote(blocks) => {
                for nested in blocks {
                    self.append_inner_block(
                        block_idx,
                        nested,
                        block_span,
                        source,
                        block_children,
                        search_cursor,
                    )?;
                }
            }
            Block::ThematicBreak => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str("---\n\n");
                let end_rendered = self.rendered_text.len().saturating_sub(2);
                let tb_span = SourceSpan::new(
                    *search_cursor,
                    (*search_cursor).saturating_add(3).min(block_span.end),
                );
                *search_cursor = tb_span.end;

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, tb_span)?;
                let elem = RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges.clone(),
                    relation: ProvenanceRelation::Literal,
                    rendered_text: "---".to_string(),
                    is_generated: false,
                    block_index: block_idx,
                };
                self.elements.push(elem);
                let node = NestedProvenanceNode::leaf(
                    elem_id,
                    ProvenanceKind::Inline,
                    ProvenanceRelation::Literal,
                    ranges,
                )?;
                block_children.push(node);
            }
            Block::Table(table) => {
                self.build_table_at(
                    block_idx,
                    table,
                    block_span,
                    source,
                    block_children,
                    search_cursor,
                )?;
            }
            Block::HtmlBlock(html) if crate::parse::is_page_break_marker(html) => {
                self.append_rendered_str("\n--- PAGE BREAK ---\n\n");
                // The separator is generated, not editable comment text. Still
                // consume this child's source before mapping following prose,
                // so a later literal "pagebreak" cannot match inside it. This
                // advances the private search cursor; it publishes no guessed
                // inline span for a nested comment's container indentation.
                *search_cursor =
                    Self::source_page_break_end(source, *search_cursor, block_span.end)
                        .unwrap_or(block_span.end);
            }
            Block::HtmlBlock(html) => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str(html);
                let end_rendered = self.rendered_text.len();
                let html_span = SourceSpan::new(
                    *search_cursor,
                    (*search_cursor)
                        .saturating_add(html.len())
                        .min(block_span.end),
                );
                *search_cursor = html_span.end;

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, html_span)?;
                self.elements.push(RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges.clone(),
                    relation: ProvenanceRelation::Literal,
                    rendered_text: html.clone(),
                    is_generated: false,
                    block_index: block_idx,
                });
                let node = NestedProvenanceNode::leaf(
                    elem_id,
                    ProvenanceKind::Inline,
                    ProvenanceRelation::Literal,
                    ranges,
                )?;
                block_children.push(node);
                self.append_rendered_str("\n\n");
            }
            Block::FootnoteDefinition { blocks, .. } => {
                for nested in blocks {
                    self.append_inner_block(
                        block_idx,
                        nested,
                        block_span,
                        source,
                        block_children,
                        search_cursor,
                    )?;
                }
            }
            Block::MathBlock(math) => {
                let start_rendered = self.rendered_text.len();
                self.append_rendered_str(math);
                let end_rendered = self.rendered_text.len();
                let math_span = SourceSpan::new(
                    *search_cursor,
                    (*search_cursor)
                        .saturating_add(math.len())
                        .min(block_span.end),
                );
                *search_cursor = math_span.end;

                let elem_id = self.next_id();
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, math_span)?;
                self.elements.push(RenderedElement {
                    id: elem_id,
                    rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                    source_ranges: ranges.clone(),
                    relation: ProvenanceRelation::Literal,
                    rendered_text: math.clone(),
                    is_generated: false,
                    block_index: block_idx,
                });
                let node = NestedProvenanceNode::leaf(
                    elem_id,
                    ProvenanceKind::Inline,
                    ProvenanceRelation::Literal,
                    ranges,
                )?;
                block_children.push(node);
                self.append_rendered_str("\n\n");
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        self.append_inlines_at(
                            block_idx,
                            term,
                            block_span,
                            source,
                            block_children,
                            search_cursor,
                        )?;
                        self.append_rendered_str("\n");
                    }
                    for def in &item.definitions {
                        self.append_rendered_str("  : ");
                        self.append_inlines_at(
                            block_idx,
                            def,
                            block_span,
                            source,
                            block_children,
                            search_cursor,
                        )?;
                        self.append_rendered_str("\n");
                    }
                }
                self.append_rendered_str("\n");
            }
            Block::PageBreak => {
                self.append_rendered_str("\n--- PAGE BREAK ---\n\n");
            }
        }
        Ok(())
    }

    fn build_list(
        &mut self,
        block_idx: usize,
        list: &List,
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
    ) -> Result<(), SourceMapError> {
        let mut search_cursor = block_span.start;
        self.build_list_at(
            block_idx,
            list,
            block_span,
            source,
            block_children,
            &mut search_cursor,
        )
    }

    fn build_list_at(
        &mut self,
        block_idx: usize,
        list: &List,
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
        search_cursor: &mut usize,
    ) -> Result<(), SourceMapError> {
        for (i, item) in list.items.iter().enumerate() {
            let marker = if list.ordered {
                format!("{}. ", list.start.saturating_add(i as u64))
            } else if let Some(task) = item.task {
                if task {
                    "- [x] ".to_string()
                } else {
                    "- [ ] ".to_string()
                }
            } else {
                "- ".to_string()
            };

            let start_rendered = self.rendered_text.len();
            self.append_rendered_str(&marker);
            let end_rendered = self.rendered_text.len();

            // Display numbering and task markers are generated. Their exact
            // spelling may differ from the authored marker, so searching for
            // that spelling can only invent source ownership (including inside
            // an earlier image title). Enclosing-block copy remains available.
            let marker_id = self.next_id();
            let marker_ranges = DisjointSourceRanges::empty(SourceOrigin::Primary);
            let marker_elem = RenderedElement {
                id: marker_id,
                rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                source_ranges: marker_ranges.clone(),
                relation: ProvenanceRelation::GeneratedMarker {
                    marker_text: marker.clone(),
                },
                rendered_text: marker.clone(),
                is_generated: true,
                block_index: block_idx,
            };
            self.elements.push(marker_elem);

            let marker_node = NestedProvenanceNode::leaf(
                marker_id,
                ProvenanceKind::Generated,
                ProvenanceRelation::GeneratedMarker {
                    marker_text: marker,
                },
                marker_ranges,
            )?;
            block_children.push(marker_node);

            for sub_block in &item.blocks {
                match sub_block {
                    Block::Paragraph(inlines) => {
                        self.append_inlines_at(
                            block_idx,
                            inlines,
                            block_span,
                            source,
                            block_children,
                            search_cursor,
                        )?;
                        self.append_rendered_str("\n");
                    }
                    _ => {
                        self.append_inner_block(
                            block_idx,
                            sub_block,
                            block_span,
                            source,
                            block_children,
                            search_cursor,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn build_table(
        &mut self,
        block_idx: usize,
        table: &Table,
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
    ) -> Result<(), SourceMapError> {
        let mut search_cursor = block_span.start;
        self.build_table_at(
            block_idx,
            table,
            block_span,
            source,
            block_children,
            &mut search_cursor,
        )
    }

    fn build_table_at(
        &mut self,
        block_idx: usize,
        table: &Table,
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
        search_cursor: &mut usize,
    ) -> Result<(), SourceMapError> {
        // Header
        for (col_idx, cell) in table.head.iter().enumerate() {
            if col_idx > 0 {
                self.append_rendered_str(" | ");
            }
            self.append_inlines_at(
                block_idx,
                cell,
                block_span,
                source,
                block_children,
                search_cursor,
            )?;
        }
        self.append_rendered_str("\n");

        // Separator
        for (col_idx, _align) in table.align.iter().enumerate() {
            if col_idx > 0 {
                self.append_rendered_str(" | ");
            }
            self.append_rendered_str("---");
        }
        self.append_rendered_str("\n");

        // Rows
        for row in &table.rows {
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx > 0 {
                    self.append_rendered_str(" | ");
                }
                self.append_inlines_at(
                    block_idx,
                    cell,
                    block_span,
                    source,
                    block_children,
                    search_cursor,
                )?;
            }
            self.append_rendered_str("\n");
        }
        self.append_rendered_str("\n");
        Ok(())
    }

    fn append_inlines(
        &mut self,
        block_idx: usize,
        inlines: &[Inline],
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
    ) -> Result<(), SourceMapError> {
        let mut search_cursor = block_span.start;
        self.append_inlines_at(
            block_idx,
            inlines,
            block_span,
            source,
            block_children,
            &mut search_cursor,
        )
    }

    fn append_inlines_at(
        &mut self,
        block_idx: usize,
        inlines: &[Inline],
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
        search_cursor: &mut usize,
    ) -> Result<(), SourceMapError> {
        use crate::parse::InlineSourceKind;
        use std::borrow::Cow;

        for inline in inlines {
            let (kind, text): (InlineSourceKind, Cow<'_, str>) = match inline {
                Inline::Emphasis(content)
                | Inline::Strong(content)
                | Inline::Strikethrough(content)
                | Inline::Link { content, .. } => {
                    self.append_inlines_at(
                        block_idx,
                        content,
                        block_span,
                        source,
                        block_children,
                        search_cursor,
                    )?;
                    continue;
                }
                Inline::Text(text) => (InlineSourceKind::Text, Cow::Borrowed(text)),
                Inline::Code(code) => (InlineSourceKind::Code, Cow::Borrowed(code)),
                Inline::Image { alt, .. } => (InlineSourceKind::Image, Cow::Borrowed(alt)),
                Inline::Math(math) => (InlineSourceKind::Math, Cow::Borrowed(math)),
                Inline::DisplayMath(math) => (InlineSourceKind::DisplayMath, Cow::Borrowed(math)),
                Inline::Html(html) => (InlineSourceKind::Html, Cow::Borrowed(html)),
                Inline::FootnoteRef { id } => {
                    (InlineSourceKind::Footnote, Cow::Owned(format!("[^{id}]")))
                }
                Inline::SoftBreak => (InlineSourceKind::SoftBreak, Cow::Borrowed(" ")),
                Inline::HardBreak => (InlineSourceKind::HardBreak, Cow::Borrowed("\n")),
            };
            for atom in self.inline_sources.take(kind, &text, block_span, source)? {
                *search_cursor = (*search_cursor).max(atom.end);
                let start = self.rendered_text.len();
                self.append_rendered_str(&atom.text);
                let end = self.rendered_text.len();
                let id = self.next_id();
                let ranges = DisjointSourceRanges::try_new(SourceOrigin::Primary, atom.ranges)?;
                let generated = ranges.is_empty();
                let node = NestedProvenanceNode::leaf(
                    id,
                    if generated {
                        ProvenanceKind::Generated
                    } else {
                        ProvenanceKind::Inline
                    },
                    atom.relation.clone(),
                    ranges.clone(),
                )?;
                self.elements.push(RenderedElement {
                    id,
                    rendered_range: TextSelectionRange::new(start, end),
                    source_ranges: ranges,
                    relation: atom.relation,
                    rendered_text: atom.text,
                    is_generated: generated,
                    block_index: block_idx,
                });
                block_children.push(node);
            }
        }
        Ok(())
    }

    fn find_in_source(
        source: &str,
        cursor: usize,
        max_end: usize,
        needle: &str,
    ) -> Option<SourceSpan> {
        if needle.is_empty() || cursor >= source.len() {
            return None;
        }
        let search_slice = source.get(cursor..max_end.min(source.len()))?;
        let rel_pos = search_slice.find(needle)?;
        let start = cursor + rel_pos;
        let end = start + needle.len();
        Some(SourceSpan::new(start, end))
    }

    /// Consume a complete source line, not a marker substring in an image title
    /// or other unread inline syntax. Container prefixes are absent from the
    /// nested AST, so admit their source spelling without publishing a span.
    fn source_page_break_end(source: &str, cursor: usize, max_end: usize) -> Option<usize> {
        fn container_prefix(mut prefix: &str) -> bool {
            loop {
                prefix = prefix.trim_start_matches([' ', '\t']);
                if prefix.is_empty() {
                    return true;
                }
                if let Some(rest) = prefix.strip_prefix('>') {
                    prefix = rest;
                    continue;
                }
                let marker_end = if prefix.starts_with(['-', '+', '*']) {
                    1
                } else if prefix.starts_with("[ ]")
                    || prefix.starts_with("[x]")
                    || prefix.starts_with("[X]")
                {
                    3
                } else if prefix.starts_with("[^") {
                    let Some(end) = prefix.find("]:") else {
                        return false;
                    };
                    end + 2
                } else {
                    let digits = prefix.bytes().take_while(u8::is_ascii_digit).count();
                    if !(1..=9).contains(&digits)
                        || !matches!(prefix.as_bytes().get(digits), Some(b'.' | b')'))
                    {
                        return false;
                    }
                    digits + 1
                };
                if !matches!(prefix.as_bytes().get(marker_end), Some(b' ' | b'\t')) {
                    return false;
                }
                prefix = &prefix[marker_end..];
            }
        }

        let line_start = source
            .get(..cursor)?
            .rfind(['\r', '\n'])
            .map_or(0, |offset| offset + 1);
        let mut offset = line_start;
        for line in crate::parse::logical_lines_inclusive(source.get(line_start..max_end)?) {
            if let Some(marker) = line.find(crate::parse::PAGE_BREAK_MARKER)
                && offset + marker >= cursor
                && crate::parse::is_page_break_marker(&line[marker..])
                && container_prefix(&line[..marker])
            {
                return Some(offset + line.len());
            }
            offset += line.len();
        }
        None
    }

    fn append_rendered_str(&mut self, s: &str) {
        self.rendered_text.push_str(s);
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_element_id;
        self.next_element_id = self.next_element_id.saturating_add(1);
        id
    }

    fn finish(self) -> Result<DocumentSourceMap, SourceMapError> {
        let doc_ranges = DisjointSourceRanges::single(
            SourceOrigin::Primary,
            SourceSpan::new(0, self.source_len),
        )?;
        let root = NestedProvenanceNode::try_new(
            0,
            ProvenanceKind::Document,
            ProvenanceRelation::Literal,
            doc_ranges,
            self.graph_children,
        )?;
        let provenance_graph = NestedProvenanceGraph::try_new(CaptureId::PRIMARY, root)?;

        Ok(DocumentSourceMap {
            rendered_text: self.rendered_text,
            elements: self.elements,
            headings: self.headings,
            provenance_graph,
            source_len: self.source_len,
            block_spans: self.block_spans,
        })
    }
}
