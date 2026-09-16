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

use std::collections::HashMap;
use std::fmt;

use crate::ast::{Block, Inline, List, Table};
use crate::html::{inlines_to_plain, slug_inlines};
use crate::span::{
    CaptureId, DisjointSourceRanges, NestedProvenanceGraph, NestedProvenanceNode,
    ProvenanceError, ProvenanceKind, ProvenanceRelation, QualifiedSpan, SourceOrigin,
    SourceSpan, SpannedBlock, SpannedDocument,
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
        self.start < other.end && other.start < self.end
    }
}

/// Errors arising during source mapping or copy operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceMapError {
    /// Provided selection range is invalid (reversed or out of bounds).
    InvalidSelectionRange {
        start: usize,
        end: usize,
        max: usize,
    },
    /// Requested byte offset is outside the source text bounds.
    SourceOutOfBounds {
        offset: usize,
        source_len: usize,
    },
    /// Target heading slug could not be resolved.
    HeadingNotFound {
        slug: String,
    },
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
    /// Build an authoritative document source map from a spanned document and source text.
    pub fn from_spanned_document(
        doc: &SpannedDocument,
        source: &str,
    ) -> Result<Self, SourceMapError> {
        let mut builder = SourceMapBuilder::new(doc.source_len);
        builder.build(doc, source)?;
        Ok(builder.finish()?)
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

    /// Borrow all authoritative heading anchors.
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
        if range.start > range.end || range.end > self.rendered_text.len() {
            return Err(SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: self.rendered_text.len(),
            });
        }
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
        if range.start > range.end || range.end > self.rendered_text.len() {
            return Err(SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: self.rendered_text.len(),
            });
        }

        let overlapping = self.elements_overlapping(range);
        if overlapping.is_empty() {
            // Check if selection is zero-width at an element boundary
            if let Some(e) = self.element_at_rendered_offset(range.start) {
                if let Some(enclosing) = e.source_ranges.enclosing_span() {
                    return enclosing
                        .slice(source)
                        .ok_or(SourceMapError::SourceOutOfBounds {
                            offset: enclosing.end,
                            source_len: source.len(),
                        });
                }
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
        if range.start > range.end || range.end > self.rendered_text.len() {
            return Err(SourceMapError::InvalidSelectionRange {
                start: range.start,
                end: range.end,
                max: self.rendered_text.len(),
            });
        }

        let mut qualified = Vec::new();
        for e in self.elements_overlapping(range) {
            let origin = e.source_ranges.origin();
            for span in e.source_ranges.spans() {
                qualified.push(QualifiedSpan { origin, span: *span });
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
            let slice = q.span.slice(source).ok_or(SourceMapError::SourceOutOfBounds {
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
        if elem.source_ranges.origin() == SourceOrigin::Primary {
            if let Some(span) = elem.source_ranges.spans().first() {
                let local = rendered_offset.saturating_sub(elem.rendered_range.start);
                let mapped = span.start.saturating_add(local.min(span.len()));
                return Some(mapped);
            }
        }
        None
    }

    /// Return the interactive rendered element containing the given rendered byte offset.
    #[must_use]
    pub fn element_at_rendered_offset(&self, offset: usize) -> Option<&RenderedElement> {
        self.elements.iter().find(|e| e.rendered_range.contains(offset))
    }

    /// Return the interactive rendered element containing the given source byte offset.
    #[must_use]
    pub fn element_at_source_offset(&self, offset: usize) -> Option<&RenderedElement> {
        self.elements.iter().find(|e| {
            e.source_ranges.origin() == SourceOrigin::Primary && e.source_ranges.contains(offset)
        })
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
    rendered_text: String,
    elements: Vec<RenderedElement>,
    headings: Vec<HeadingSourceAnchor>,
    graph_children: Vec<NestedProvenanceNode>,
    heading_slug_counts: HashMap<String, usize>,
    next_element_id: u64,
    source_len: usize,
    block_spans: Vec<SourceSpan>,
}

impl SourceMapBuilder {
    fn new(source_len: usize) -> Self {
        Self {
            rendered_text: String::new(),
            elements: Vec::new(),
            headings: Vec::new(),
            graph_children: Vec::new(),
            heading_slug_counts: HashMap::new(),
            next_element_id: 1,
            source_len,
            block_spans: Vec::new(),
        }
    }

    fn build(&mut self, doc: &SpannedDocument, source: &str) -> Result<(), SourceMapError> {
        self.block_spans = doc.iter_blocks().map(|b| b.span).collect();
        for (block_idx, block) in doc.iter_blocks().enumerate() {
            self.build_block(block_idx, block, source)?;
        }
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
                let base_slug = slug_inlines(inlines);
                let base = if base_slug.is_empty() {
                    "section".to_string()
                } else {
                    base_slug
                };

                let count = self.heading_slug_counts.entry(base.clone()).or_insert(0);
                *count += 1;
                let slug = if *count == 1 {
                    base
                } else {
                    format!("{}-{}", base, count)
                };

                let rendered_offset = self.rendered_text.len();
                self.headings.push(HeadingSourceAnchor {
                    slug,
                    title: title.clone(),
                    level: *level,
                    source_span: block_span,
                    origin: SourceOrigin::Primary,
                    block_index: block_idx,
                    rendered_offset,
                });

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
                for (sub_idx, inner) in blocks.iter().enumerate() {
                    let sub_spanned = SpannedBlock::new(inner.clone(), block_span);
                    self.build_block(block_idx + sub_idx, &sub_spanned, source)?;
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
                let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
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

                for (sub_idx, inner) in blocks.iter().enumerate() {
                    let sub_spanned = SpannedBlock::new(inner.clone(), block_span);
                    self.build_block(block_idx + sub_idx, &sub_spanned, source)?;
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
                for item in items {
                    for term in &item.terms {
                        self.append_inlines(block_idx, term, block_span, source, &mut block_children)?;
                        self.append_rendered_str("\n");
                    }
                    for def in &item.definitions {
                        self.append_rendered_str("  : ");
                        self.append_inlines(block_idx, def, block_span, source, &mut block_children)?;
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

    fn build_list(
        &mut self,
        block_idx: usize,
        list: &List,
        block_span: SourceSpan,
        source: &str,
        block_children: &mut Vec<NestedProvenanceNode>,
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

            let marker_id = self.next_id();
            let marker_ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
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

            for (sub_idx, sub_block) in item.blocks.iter().enumerate() {
                let sub_spanned = SpannedBlock::new(sub_block.clone(), block_span);
                self.build_block(block_idx + sub_idx, &sub_spanned, source)?;
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
        // Header
        for (col_idx, cell) in table.head.iter().enumerate() {
            if col_idx > 0 {
                self.append_rendered_str(" | ");
            }
            self.append_inlines(block_idx, cell, block_span, source, block_children)?;
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
                self.append_inlines(block_idx, cell, block_span, source, block_children)?;
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

        for inline in inlines {
            match inline {
                Inline::Text(text) => {
                    let text_span = Self::find_in_source(source, search_cursor, block_span.end, text)
                        .unwrap_or(block_span);
                    search_cursor = text_span.end;

                    let start_rendered = self.rendered_text.len();
                    self.append_rendered_str(text);
                    let end_rendered = self.rendered_text.len();

                    let elem_id = self.next_id();
                    let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, text_span)?;
                    self.elements.push(RenderedElement {
                        id: elem_id,
                        rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                        source_ranges: ranges.clone(),
                        relation: ProvenanceRelation::Literal,
                        rendered_text: text.clone(),
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
                }
                Inline::Emphasis(inner) => {
                    self.append_inlines(block_idx, inner, block_span, source, block_children)?;
                }
                Inline::Strong(inner) => {
                    self.append_inlines(block_idx, inner, block_span, source, block_children)?;
                }
                Inline::Strikethrough(inner) => {
                    self.append_inlines(block_idx, inner, block_span, source, block_children)?;
                }
                Inline::Code(code) => {
                    let code_span = Self::find_in_source(source, search_cursor, block_span.end, code)
                        .unwrap_or(block_span);
                    search_cursor = code_span.end;

                    let start_rendered = self.rendered_text.len();
                    self.append_rendered_str(code);
                    let end_rendered = self.rendered_text.len();

                    let elem_id = self.next_id();
                    let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, code_span)?;
                    self.elements.push(RenderedElement {
                        id: elem_id,
                        rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                        source_ranges: ranges.clone(),
                        relation: ProvenanceRelation::StrippedDelimiter {
                            delimiter: "`".to_string(),
                        },
                        rendered_text: code.clone(),
                        is_generated: false,
                        block_index: block_idx,
                    });

                    let node = NestedProvenanceNode::leaf(
                        elem_id,
                        ProvenanceKind::Inline,
                        ProvenanceRelation::StrippedDelimiter {
                            delimiter: "`".to_string(),
                        },
                        ranges,
                    )?;
                    block_children.push(node);
                }
                Inline::Link { content, dest, title } => {
                    self.append_inlines(block_idx, content, block_span, source, block_children)?;
                    let _ = (dest, title);
                }
                Inline::Image { alt, .. } => {
                    let start_rendered = self.rendered_text.len();
                    self.append_rendered_str(alt);
                    let end_rendered = self.rendered_text.len();

                    let elem_id = self.next_id();
                    let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
                    self.elements.push(RenderedElement {
                        id: elem_id,
                        rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                        source_ranges: ranges.clone(),
                        relation: ProvenanceRelation::Literal,
                        rendered_text: alt.clone(),
                        is_generated: false,
                        block_index: block_idx,
                    });
                }
                Inline::SoftBreak => {
                    let start_rendered = self.rendered_text.len();
                    self.append_rendered_str(" ");
                    let end_rendered = self.rendered_text.len();

                    let elem_id = self.next_id();
                    let ranges = DisjointSourceRanges::single(
                        SourceOrigin::Primary,
                        SourceSpan::new(search_cursor, search_cursor.saturating_add(1).min(block_span.end)),
                    )?;
                    self.elements.push(RenderedElement {
                        id: elem_id,
                        rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                        source_ranges: ranges,
                        relation: ProvenanceRelation::SoftBreak,
                        rendered_text: " ".to_string(),
                        is_generated: false,
                        block_index: block_idx,
                    });
                }
                Inline::HardBreak => {
                    let start_rendered = self.rendered_text.len();
                    self.append_rendered_str("\n");
                    let end_rendered = self.rendered_text.len();

                    let elem_id = self.next_id();
                    let ranges = DisjointSourceRanges::single(
                        SourceOrigin::Primary,
                        SourceSpan::new(search_cursor, search_cursor.saturating_add(1).min(block_span.end)),
                    )?;
                    self.elements.push(RenderedElement {
                        id: elem_id,
                        rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                        source_ranges: ranges,
                        relation: ProvenanceRelation::HardBreak,
                        rendered_text: "\n".to_string(),
                        is_generated: false,
                        block_index: block_idx,
                    });
                }
                Inline::FootnoteRef { id } => {
                    let ref_text = format!("[^{}]", id);
                    let start_rendered = self.rendered_text.len();
                    self.append_rendered_str(&ref_text);
                    let end_rendered = self.rendered_text.len();

                    let elem_id = self.next_id();
                    let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block_span)?;
                    self.elements.push(RenderedElement {
                        id: elem_id,
                        rendered_range: TextSelectionRange::new(start_rendered, end_rendered),
                        source_ranges: ranges,
                        relation: ProvenanceRelation::ReferenceDefinition {
                            label: id.clone(),
                            definition_span: block_span,
                        },
                        rendered_text: ref_text,
                        is_generated: false,
                        block_index: block_idx,
                    });
                }
                Inline::Math(math) | Inline::DisplayMath(math) => {
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
                }
                Inline::Html(html) => {
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
                }
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
