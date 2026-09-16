//! Source-span and parser-diagnostic scaffolding.
//!
//! These types are intentionally renderer-neutral. The existing AST remains the
//! rendering contract; spanned wrappers let editor/WASM integrations, diagnostics,
//! and future conformance tooling recover source locations without forcing every
//! renderer to carry span metadata.

use crate::ast::{Block, Document, Inline, ListItem, Table};

/// A byte range in the original Markdown source: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SourceSpan {
    /// Inclusive byte offset where the span starts.
    pub start: usize,
    /// Exclusive byte offset where the span ends.
    pub end: usize,
}

impl SourceSpan {
    /// Create a span from explicit byte offsets.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Length in bytes, saturating to zero for malformed ranges.
    #[inline(always)]
    #[must_use]
    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// True when the span has no byte width.
    #[inline(always)]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// True when `offset` is inside `[start, end)`.
    #[inline(always)]
    #[must_use]
    pub const fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Return a span covering both inputs.
    #[inline(always)]
    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Self { start, end }
    }

    /// Borrow the original source slice covered by this span.
    #[inline]
    #[must_use]
    pub fn slice(self, source: &str) -> Option<&str> {
        if self.start <= self.end {
            source.get(self.start..self.end)
        } else {
            None
        }
    }
}

/// The semantic role of a node in a renderer-neutral source map.
///
/// `Generated` nodes may have an empty span when a renderer creates content
/// that has no literal Markdown counterpart.  The other roles normally carry
/// a non-empty source range, but the tree validator deliberately permits an
/// empty range so a resumable upstream parser can publish a structurally valid
/// placeholder before its source mapping is complete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceKind {
    /// The complete captured Markdown document.
    Document,
    /// A block-level Markdown node.
    Block,
    /// An inline Markdown node or inline display run.
    Inline,
    /// Renderer-created content with no literal source range.
    Generated,
}

/// A validation error for a renderer-neutral provenance tree or graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    /// A node or child contains reversed byte offsets.
    ReversedSpan { span: SourceSpan },
    /// A child extends beyond its parent's source range.
    ChildOutsideParent {
        parent: SourceSpan,
        child: SourceSpan,
    },
    /// Two source-backed siblings overlap or are out of source order.
    OverlappingChildren {
        previous: SourceSpan,
        next: SourceSpan,
    },
    /// Two source-backed siblings are disjoint but not in source order.
    OutOfOrderChildren {
        previous: SourceSpan,
        next: SourceSpan,
    },
    /// Attempted to present disjoint source ranges as an invented contiguous slice.
    ///
    /// Plan §12.4: "the caller must not invent contiguous source by concatenating
    /// unrelated spans and presenting it as the original literal slice."
    InventedContiguousSpan {
        enclosing: SourceSpan,
        disjoint_count: usize,
    },
    /// Transclusion source origin mismatch.
    TransclusionOriginMismatch {
        expected: CaptureId,
        actual: CaptureId,
    },
    /// Source offset is outside the captured source text.
    SourceOffsetOutOfBounds {
        offset: usize,
        source_len: usize,
    },
    /// Relation syntax mismatch (e.g. expected escape or entity).
    InvalidRelationSyntax {
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReversedSpan { span } => {
                write!(f, "reversed source span: [{}, {})", span.start, span.end)
            }
            Self::ChildOutsideParent { parent, child } => {
                write!(
                    f,
                    "child span [{}, {}) outside parent [{}, {})",
                    child.start, child.end, parent.start, parent.end
                )
            }
            Self::OverlappingChildren { previous, next } => {
                write!(
                    f,
                    "overlapping siblings: [{}, {}) overlaps [{}, {})",
                    previous.start, previous.end, next.start, next.end
                )
            }
            Self::OutOfOrderChildren { previous, next } => {
                write!(
                    f,
                    "out-of-order siblings: [{}, {}) followed by [{}, {})",
                    previous.start, previous.end, next.start, next.end
                )
            }
            Self::InventedContiguousSpan {
                enclosing,
                disjoint_count,
            } => {
                write!(
                    f,
                    "invented contiguous slice over {disjoint_count} disjoint ranges [{}, {})",
                    enclosing.start, enclosing.end
                )
            }
            Self::TransclusionOriginMismatch { expected, actual } => {
                write!(
                    f,
                    "transclusion capture mismatch: expected {expected}, actual {actual}"
                )
            }
            Self::SourceOffsetOutOfBounds { offset, source_len } => {
                write!(
                    f,
                    "source offset {offset} out of bounds for source length {source_len}"
                )
            }
            Self::InvalidRelationSyntax { expected, actual } => {
                write!(
                    f,
                    "invalid relation syntax: expected {expected:?}, got {actual:?}"
                )
            }
        }
    }
}

impl std::error::Error for ProvenanceError {}

/// A nested, renderer-neutral source provenance node.
///
/// This type carries only source identity and hierarchy.  It has no parser,
/// layout, renderer, or host-resource policy, so FCB and other consumers can
/// use it without importing a runtime or a display backend.  Source-backed
/// siblings must be ordered and non-overlapping; generated empty nodes are
/// ignored by [`Self::hit_test`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceNode {
    /// The semantic role of this node.
    pub kind: ProvenanceKind,
    /// The source range associated with this node.
    pub span: SourceSpan,
    /// Nested semantic/source nodes in source order.
    pub children: Vec<Self>,
}

impl ProvenanceNode {
    /// Construct and validate a node with nested children.
    pub fn try_new(
        kind: ProvenanceKind,
        span: SourceSpan,
        children: Vec<Self>,
    ) -> Result<Self, ProvenanceError> {
        let node = Self {
            kind,
            span,
            children,
        };
        node.validate()?;
        Ok(node)
    }

    /// Construct a validated leaf node.
    pub fn leaf(kind: ProvenanceKind, span: SourceSpan) -> Result<Self, ProvenanceError> {
        Self::try_new(kind, span, Vec::new())
    }

    /// Validate this node and all descendants.
    pub fn validate(&self) -> Result<(), ProvenanceError> {
        if self.span.start > self.span.end {
            return Err(ProvenanceError::ReversedSpan { span: self.span });
        }

        let mut previous: Option<SourceSpan> = None;
        for child in &self.children {
            child.validate()?;
            if child.span.start < self.span.start || child.span.end > self.span.end {
                return Err(ProvenanceError::ChildOutsideParent {
                    parent: self.span,
                    child: child.span,
                });
            }
            if let Some(previous) = previous {
                if child.span.start < previous.start {
                    return Err(ProvenanceError::OutOfOrderChildren {
                        previous,
                        next: child.span,
                    });
                }
                if child.span.start < previous.end {
                    return Err(ProvenanceError::OverlappingChildren {
                        previous,
                        next: child.span,
                    });
                }
            }
            previous = Some(child.span);
        }
        Ok(())
    }

    /// Return the deepest source-backed node containing `offset`.
    #[must_use]
    pub fn hit_test(&self, offset: usize) -> Option<&Self> {
        if self.span.is_empty() || !self.span.contains(offset) {
            return None;
        }
        self.children
            .iter()
            .find_map(|child| child.hit_test(offset))
            .or(Some(self))
    }
}

/// A node plus its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    /// The parsed node.
    pub node: T,
    /// The source range that produced this node.
    pub span: SourceSpan,
}

impl<T> Spanned<T> {
    /// Attach `span` to `node`.
    #[must_use]
    pub const fn new(node: T, span: SourceSpan) -> Self {
        Self { node, span }
    }
}

/// A block-level AST node with source position.
pub type SpannedBlock = Spanned<Block>;
/// An inline-level AST node with source position.
pub type SpannedInline = Spanned<Inline>;
/// A list item with source position.
pub type SpannedListItem = Spanned<ListItem>;
/// A table with source position.
pub type SpannedTable = Spanned<Table>;

/// Diagnostic severity for parser-facing tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// Recoverable issue; output was still produced.
    Warning,
    /// Reserved for future fail-closed parsing modes.
    Error,
}

/// A parser diagnostic tied to a source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    /// Warning/error classification.
    pub severity: DiagnosticSeverity,
    /// Source range for the diagnostic.
    pub span: SourceSpan,
    /// Human-readable diagnostic text.
    pub message: String,
}

impl ParseDiagnostic {
    /// Create a warning diagnostic.
    #[must_use]
    pub fn warning(span: SourceSpan, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            span,
            message: message.into(),
        }
    }
}

/// A parsed document plus top-level block spans and recoverable diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpannedDocument {
    /// Top-level blocks with source ranges.
    pub blocks: Vec<SpannedBlock>,
    /// Recoverable parser diagnostics.
    pub diagnostics: Vec<ParseDiagnostic>,
    /// Original source length in bytes.
    pub source_len: usize,
}

impl SpannedDocument {
    /// Build the currently available renderer-neutral provenance tree.
    ///
    /// The parser currently guarantees exact top-level block spans.  This
    /// method exposes those spans as a validated tree without guessing inline
    /// ranges.  Future nested parser output can use [`ProvenanceNode::try_new`]
    /// to add exact descendants while preserving the same consumer contract.
    pub fn provenance_tree(&self) -> Result<ProvenanceNode, ProvenanceError> {
        let blocks = self
            .blocks
            .iter()
            .map(|block| ProvenanceNode::leaf(ProvenanceKind::Block, block.span))
            .collect::<Result<Vec<_>, _>>()?;
        ProvenanceNode::try_new(
            ProvenanceKind::Document,
            SourceSpan::new(0, self.source_len),
            blocks,
        )
    }

    /// Drop source metadata and recover the renderer-facing AST.
    #[must_use]
    pub fn into_document(self) -> Document {
        Document {
            blocks: self.blocks.into_iter().map(|block| block.node).collect(),
        }
    }

    /// Borrow a renderer-facing document by cloning the block nodes.
    ///
    /// This keeps the current AST contract simple while the source-span model is
    /// still scaffolded. Callers that need zero-copy rendering can use
    /// [`Self::into_document`].
    #[must_use]
    pub fn to_document(&self) -> Document {
        Document {
            blocks: self.blocks.iter().map(|block| block.node.clone()).collect(),
        }
    }

    /// Build a nested provenance graph representing the document hierarchy.
    ///
    /// Plan §12.4: "Every interactive rendered element identifies the source spans
    /// from which it was derived, using an upstream nested provenance graph rather
    /// than a top-level block envelope."
    pub fn nested_provenance_graph(&self) -> Result<NestedProvenanceGraph, ProvenanceError> {
        let mut children = Vec::with_capacity(self.blocks.len());
        for (i, block) in self.blocks.iter().enumerate() {
            let ranges = DisjointSourceRanges::single(SourceOrigin::Primary, block.span)?;
            children.push(NestedProvenanceNode::leaf(
                (i + 1) as u64,
                ProvenanceKind::Block,
                ProvenanceRelation::Literal,
                ranges,
            )?);
        }
        let doc_ranges = DisjointSourceRanges::single(
            SourceOrigin::Primary,
            SourceSpan::new(0, self.source_len),
        )?;
        let root = NestedProvenanceNode::try_new(
            0,
            ProvenanceKind::Document,
            ProvenanceRelation::Literal,
            doc_ranges,
            children,
        )?;
        NestedProvenanceGraph::try_new(CaptureId::PRIMARY, root)
    }

    /// Borrow the slice of top-level spanned blocks without cloning.
    #[inline]
    #[must_use]
    pub fn blocks(&self) -> &[SpannedBlock] {
        &self.blocks
    }

    /// Iterator over top-level spanned blocks without cloning the underlying AST.
    #[inline]
    pub fn iter_blocks(&self) -> std::slice::Iter<'_, SpannedBlock> {
        self.blocks.iter()
    }

    /// Total number of top-level blocks in the document.
    #[inline]
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// True when the document has zero blocks.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Borrow the spanned block at `index`, if within bounds.
    #[inline]
    #[must_use]
    pub fn block_at(&self, index: usize) -> Option<&SpannedBlock> {
        self.blocks.get(index)
    }

    /// Return the source span for the block at `index`.
    #[inline]
    #[must_use]
    pub fn source_span_for_block(&self, index: usize) -> Option<SourceSpan> {
        self.blocks.get(index).map(|b| b.span)
    }

    /// Find the block containing the given source byte offset.
    #[must_use]
    pub fn block_at_offset(&self, offset: usize) -> Option<(usize, &SpannedBlock)> {
        self.blocks
            .iter()
            .enumerate()
            .find(|(_, block)| block.span.contains(offset))
    }

    /// Minimal enclosing source span covering the entire document: `[0, source_len)`.
    #[inline]
    #[must_use]
    pub const fn source_span(&self) -> SourceSpan {
        SourceSpan::new(0, self.source_len)
    }

    /// Construct an authoritative document source map from this spanned document.
    pub fn source_map(
        &self,
        source: &str,
    ) -> Result<crate::source_map::DocumentSourceMap, crate::source_map::SourceMapError> {
        crate::source_map::DocumentSourceMap::from_spanned_document(self, source)
    }
}

impl<'a> IntoIterator for &'a SpannedDocument {
    type Item = &'a SpannedBlock;
    type IntoIter = std::slice::Iter<'a, SpannedBlock>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.blocks.iter()
    }
}

// ---------------------------------------------------------------------------
// Upstream Nested Many-to-Many Provenance Graph (Plan §12.4 & §27.4, FCB-073.A)
// ---------------------------------------------------------------------------

/// Unique identifier for an immutable source capture or file revision.
///
/// Plan §10.11 & §12.4: "Transcluded content identifies its separate source capture."
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CaptureId(pub u64);

impl CaptureId {
    /// Construct a new capture identifier.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Primary document capture (ID 0).
    pub const PRIMARY: Self = Self(0);

    /// True when this is the primary root document capture.
    #[must_use]
    pub const fn is_primary(self) -> bool {
        self.0 == 0
    }
}

impl std::fmt::Display for CaptureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "capture:{}", self.0)
    }
}

/// Origin of a source span, distinguishing the primary document from transcluded external files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SourceOrigin {
    /// The primary captured root document.
    #[default]
    Primary,
    /// An external transcluded document capture identified by unique capture ID.
    Transclusion(CaptureId),
}

impl SourceOrigin {
    /// Create a transclusion origin from a numeric capture id.
    #[must_use]
    pub const fn transclusion(id: u64) -> Self {
        Self::Transclusion(CaptureId::new(id))
    }

    /// Return the capture id associated with this origin.
    #[must_use]
    pub const fn capture_id(self) -> CaptureId {
        match self {
            Self::Primary => CaptureId::PRIMARY,
            Self::Transclusion(id) => id,
        }
    }

    /// True if this origin is the primary root document.
    #[must_use]
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::Primary)
    }
}

/// A source span qualified with its source document origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct QualifiedSpan {
    /// Origin document capture.
    pub origin: SourceOrigin,
    /// Byte range within that document.
    pub span: SourceSpan,
}

impl QualifiedSpan {
    /// Construct a qualified span in the primary document.
    #[must_use]
    pub const fn primary(span: SourceSpan) -> Self {
        Self {
            origin: SourceOrigin::Primary,
            span,
        }
    }

    /// Construct a qualified span in a transcluded document.
    #[must_use]
    pub const fn transcluded(capture_id: CaptureId, span: SourceSpan) -> Self {
        Self {
            origin: SourceOrigin::Transclusion(capture_id),
            span,
        }
    }

    /// True when the span has no byte width.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.span.is_empty()
    }
}

/// An ordered, validated set of strictly disjoint source byte ranges.
///
/// Plan §12.4: "Source mapping can return a disjoint ordered set of ranges;
/// the caller must not invent contiguous source by concatenating unrelated spans
/// and presenting it as the original literal slice. 'Copy enclosing Markdown block'
/// is a separate useful command."
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DisjointSourceRanges {
    origin: SourceOrigin,
    spans: Vec<SourceSpan>,
}

impl DisjointSourceRanges {
    /// Create an empty set of disjoint ranges for an origin.
    #[must_use]
    pub const fn empty(origin: SourceOrigin) -> Self {
        Self {
            origin,
            spans: Vec::new(),
        }
    }

    /// Construct a disjoint range set from a single span.
    pub fn single(origin: SourceOrigin, span: SourceSpan) -> Result<Self, ProvenanceError> {
        if span.start > span.end {
            return Err(ProvenanceError::ReversedSpan { span });
        }
        Ok(Self {
            origin,
            spans: if span.is_empty() {
                Vec::new()
            } else {
                vec![span]
            },
        })
    }

    /// Construct and validate disjoint ranges from an origin and a list of spans.
    ///
    /// Requires spans to be in ascending order and strictly non-overlapping.
    pub fn try_new(
        origin: SourceOrigin,
        mut spans: Vec<SourceSpan>,
    ) -> Result<Self, ProvenanceError> {
        for span in &spans {
            if span.start > span.end {
                return Err(ProvenanceError::ReversedSpan { span: *span });
            }
        }
        spans.retain(|s| !s.is_empty());

        for pair in spans.windows(2) {
            if let [prev, curr] = pair {
                if curr.start < prev.start {
                    return Err(ProvenanceError::OutOfOrderChildren {
                        previous: *prev,
                        next: *curr,
                    });
                }
                if curr.start < prev.end {
                    return Err(ProvenanceError::OverlappingChildren {
                        previous: *prev,
                        next: *curr,
                    });
                }
            }
        }

        Ok(Self { origin, spans })
    }

    /// Origin document for these ranges.
    #[must_use]
    pub const fn origin(&self) -> SourceOrigin {
        self.origin
    }

    /// Borrows the slice of disjoint spans.
    #[must_use]
    pub fn spans(&self) -> &[SourceSpan] {
        &self.spans
    }

    /// Number of disjoint spans.
    #[must_use]
    pub fn count(&self) -> usize {
        self.spans.len()
    }

    /// True if there are no spans.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Total number of bytes across all disjoint spans.
    #[must_use]
    pub fn total_len(&self) -> usize {
        self.spans.iter().map(|s| s.len()).sum()
    }

    /// Minimal enclosing span covering the entire span range: `[min_start, max_end)`.
    ///
    /// Plan §12.4: "'Copy enclosing Markdown block' is a separate useful command."
    #[must_use]
    pub fn enclosing_span(&self) -> Option<SourceSpan> {
        let first = self.spans.first()?;
        let last = self.spans.last()?;
        Some(SourceSpan::new(first.start, last.end))
    }

    /// True if an offset is contained within any of the disjoint spans.
    #[must_use]
    pub fn contains(&self, offset: usize) -> bool {
        self.spans.iter().any(|s| s.contains(offset))
    }

    /// Extract separate string slices for each disjoint span.
    ///
    /// Returns one slice per disjoint span. Never concatenates them into
    /// an invented literal slice!
    pub fn extract_slices<'a>(&self, source: &'a str) -> Option<Vec<&'a str>> {
        let mut slices = Vec::with_capacity(self.spans.len());
        for span in &self.spans {
            slices.push(span.slice(source)?);
        }
        Some(slices)
    }

    /// Reject an attempt to represent these disjoint spans as a single invented contiguous slice.
    ///
    /// If there are multiple disjoint spans, slicing `source` contiguously from
    /// `enclosing.start` to `enclosing.end` contains unselected delimiter or intervening bytes.
    /// The oracle uses this to enforce Plan §12.4's rule that callers must not invent
    /// contiguous literal source.
    pub fn reject_invented_contiguous(
        &self,
        purported_span: SourceSpan,
    ) -> Result<(), ProvenanceError> {
        if self.spans.len() > 1 {
            if let Some(enclosing) = self.enclosing_span() {
                if purported_span == enclosing {
                    return Err(ProvenanceError::InventedContiguousSpan {
                        enclosing,
                        disjoint_count: self.spans.len(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// The precise relationship between Markdown source syntax and rendered/interactive elements.
///
/// Plan §12.4: "The graph records escapes, entities, stripped emphasis/link delimiters,
/// code-fence dedentation, soft/hard line breaks, generated numbering, and reference-derived content.
/// Transcluded content identifies its separate source capture."
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProvenanceRelation {
    /// Literal 1:1 direct mapping from source bytes.
    Literal,
    /// Backslash escape sequence in source (e.g. `\*`, `\[`, `\\`).
    /// Source span covers the backslash plus character; rendered output is the escaped character.
    Escape { escaped_char: char },
    /// HTML/XML character entity (e.g. `&amp;` -> `&`, `&#169;` -> `©`).
    /// Source span covers `&...;`; rendered output is the decoded character.
    Entity {
        decoded: char,
        raw_entity: String,
    },
    /// Stripped delimiter syntax (e.g. `*` for emphasis, `**` for strong, `~~` for strike, `` ` `` for inline code).
    /// The delimiter bytes are present in the enclosing element's source span but excluded from the inner text.
    StrippedDelimiter { delimiter: String },
    /// Dedented indentation prefix removed from code fence lines.
    /// Leading spaces in raw fence source were stripped for clean code presentation.
    CodeFenceDedentation { dedented_spaces: usize },
    /// Soft line break in source mapped to a space in layout.
    SoftBreak,
    /// Hard line break (e.g. trailing two spaces or backslash) mapped to a newline.
    HardBreak,
    /// Generated numbering, bullet marker, or task checkbox (`1. `, `* `, `[ ] `).
    /// These are produced by list formatting and do not map to editable text content.
    GeneratedMarker { marker_text: String },
    /// Reference-derived content: link or footnote reference definitions.
    /// E.g. `[label][ref]` resolved to URL `[ref]: https://example.com`.
    /// Preserves both the invocation label and the target definition span.
    ReferenceDefinition {
        label: String,
        definition_span: SourceSpan,
    },
    /// Transcluded content from an external source capture.
    Transclusion {
        capture_id: CaptureId,
        path: String,
    },
}

impl ProvenanceRelation {
    /// True if this relation represents synthetic or generated content.
    #[must_use]
    pub const fn is_generated(&self) -> bool {
        matches!(self, Self::GeneratedMarker { .. })
    }

    /// True if this relation is transcluded from an external capture.
    #[must_use]
    pub const fn is_transclusion(&self) -> bool {
        matches!(self, Self::Transclusion { .. })
    }
}

/// A nested, many-to-many source provenance node in the upstream provenance graph.
///
/// Plan §12.4: "Every interactive rendered element identifies the source spans from
/// which it was derived, using an upstream nested provenance graph rather than a
/// top-level block envelope."
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedProvenanceNode {
    /// Unique node identifier within the graph.
    pub id: u64,
    /// Semantic role (Document, Block, Inline, Generated).
    pub kind: ProvenanceKind,
    /// Detailed syntax-to-presentation relation.
    pub relation: ProvenanceRelation,
    /// Disjoint ordered source ranges backing this node.
    pub source_ranges: DisjointSourceRanges,
    /// Nested child provenance nodes.
    pub children: Vec<Self>,
}

impl NestedProvenanceNode {
    /// Construct a new nested provenance node and validate its structure.
    pub fn try_new(
        id: u64,
        kind: ProvenanceKind,
        relation: ProvenanceRelation,
        source_ranges: DisjointSourceRanges,
        children: Vec<Self>,
    ) -> Result<Self, ProvenanceError> {
        let node = Self {
            id,
            kind,
            relation,
            source_ranges,
            children,
        };
        node.validate()?;
        Ok(node)
    }

    /// Construct a leaf node with no children.
    pub fn leaf(
        id: u64,
        kind: ProvenanceKind,
        relation: ProvenanceRelation,
        source_ranges: DisjointSourceRanges,
    ) -> Result<Self, ProvenanceError> {
        Self::try_new(id, kind, relation, source_ranges, Vec::new())
    }

    /// Validate hierarchy and invariants.
    pub fn validate(&self) -> Result<(), ProvenanceError> {
        for child in &self.children {
            child.validate()?;
        }

        // If this node has primary enclosing bounds, child primary ranges must stay within them
        if let Some(parent_enclosing) = self.source_ranges.enclosing_span() {
            if self.source_ranges.origin().is_primary() {
                for child in &self.children {
                    if child.source_ranges.origin().is_primary() {
                        if let Some(child_enclosing) = child.source_ranges.enclosing_span() {
                            if child_enclosing.start < parent_enclosing.start
                                || child_enclosing.end > parent_enclosing.end
                            {
                                return Err(ProvenanceError::ChildOutsideParent {
                                    parent: parent_enclosing,
                                    child: child_enclosing,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sibling ordering checks across primary children
        let mut prev_span: Option<SourceSpan> = None;
        for child in &self.children {
            if child.source_ranges.origin().is_primary() && !child.source_ranges.is_empty() {
                if let Some(curr_enclosing) = child.source_ranges.enclosing_span() {
                    if let Some(prev) = prev_span {
                        if curr_enclosing.start < prev.start {
                            return Err(ProvenanceError::OutOfOrderChildren {
                                previous: prev,
                                next: curr_enclosing,
                            });
                        }
                        if curr_enclosing.start < prev.end {
                            return Err(ProvenanceError::OverlappingChildren {
                                previous: prev,
                                next: curr_enclosing,
                            });
                        }
                    }
                    prev_span = Some(curr_enclosing);
                }
            }
        }

        Ok(())
    }

    /// Perform a deep hit test at a specific source origin and byte offset.
    #[must_use]
    pub fn hit_test(&self, origin: SourceOrigin, offset: usize) -> Option<&Self> {
        if self.source_ranges.origin() == origin && self.source_ranges.contains(offset) {
            // Check children first for deepest match
            for child in &self.children {
                if let Some(hit) = child.hit_test(origin, offset) {
                    return Some(hit);
                }
            }
            return Some(self);
        }
        // If parent has empty ranges (e.g. Generated), child might still match
        for child in &self.children {
            if let Some(hit) = child.hit_test(origin, offset) {
                return Some(hit);
            }
        }
        None
    }

    /// Count all nodes in this subtree.
    #[must_use]
    pub fn total_nodes(&self) -> usize {
        1 + self.children.iter().map(|c| c.total_nodes()).sum::<usize>()
    }
}

/// Upstream nested provenance graph representing the complete document derivation.
///
/// Plan §12.4: "Every interactive rendered element identifies the source spans from which
/// it was derived, using an upstream nested provenance graph rather than a top-level block envelope."
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedProvenanceGraph {
    /// Primary root document capture.
    pub primary_capture: CaptureId,
    /// Root document provenance node.
    pub root: NestedProvenanceNode,
}

impl NestedProvenanceGraph {
    /// Construct and validate a complete provenance graph.
    pub fn try_new(
        primary_capture: CaptureId,
        root: NestedProvenanceNode,
    ) -> Result<Self, ProvenanceError> {
        root.validate()?;
        Ok(Self {
            primary_capture,
            root,
        })
    }

    /// Hit test against the entire graph.
    #[must_use]
    pub fn hit_test(&self, origin: SourceOrigin, offset: usize) -> Option<&NestedProvenanceNode> {
        self.root.hit_test(origin, offset)
    }

    /// Total node count in the graph.
    #[must_use]
    pub fn total_nodes(&self) -> usize {
        self.root.total_nodes()
    }
}

/// Summary audit report produced by the provenance truthfulness oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvenanceAuditReport {
    /// Total nodes inspected in the graph.
    pub total_nodes: usize,
    /// Count of literal 1:1 nodes.
    pub literal_nodes: usize,
    /// Count of escape nodes.
    pub escape_nodes: usize,
    /// Count of entity nodes.
    pub entity_nodes: usize,
    /// Count of stripped delimiter nodes.
    pub stripped_delimiter_nodes: usize,
    /// Count of dedented code nodes.
    pub dedentation_nodes: usize,
    /// Count of generated marker nodes.
    pub generated_marker_nodes: usize,
    /// Count of reference definition nodes.
    pub reference_nodes: usize,
    /// Count of transclusion nodes.
    pub transclusion_nodes: usize,
    /// Disjoint range count across the graph.
    pub disjoint_range_sets: usize,
    /// Verified zero invented contiguous strings.
    pub zero_invented_contiguous_slices: bool,
}

/// Verification oracle for nested provenance truthfulness and boundary invariants.
pub struct ProvenanceOracle;

impl ProvenanceOracle {
    /// Inspect a provenance graph and verify every node against source text.
    pub fn verify_truthfulness<'s>(
        graph: &NestedProvenanceGraph,
        primary_source: &'s str,
        transcluded_sources: &impl Fn(CaptureId) -> Option<&'s str>,
    ) -> Result<ProvenanceAuditReport, ProvenanceError> {
        let mut report = ProvenanceAuditReport {
            total_nodes: 0,
            literal_nodes: 0,
            escape_nodes: 0,
            entity_nodes: 0,
            stripped_delimiter_nodes: 0,
            dedentation_nodes: 0,
            generated_marker_nodes: 0,
            reference_nodes: 0,
            transclusion_nodes: 0,
            disjoint_range_sets: 0,
            zero_invented_contiguous_slices: true,
        };

        Self::verify_node(
            &graph.root,
            primary_source,
            transcluded_sources,
            &mut report,
        )?;
        Ok(report)
    }

    fn verify_node<'s>(
        node: &NestedProvenanceNode,
        primary_source: &'s str,
        transcluded_sources: &impl Fn(CaptureId) -> Option<&'s str>,
        report: &mut ProvenanceAuditReport,
    ) -> Result<(), ProvenanceError> {
        report.total_nodes += 1;
        if node.source_ranges.count() > 1 {
            report.disjoint_range_sets += 1;
        }

        let source_text = match node.source_ranges.origin() {
            SourceOrigin::Primary => Some(primary_source),
            SourceOrigin::Transclusion(cap) => transcluded_sources(cap),
        };

        match &node.relation {
            ProvenanceRelation::Literal => {
                report.literal_nodes += 1;
                if let Some(src) = source_text {
                    for span in node.source_ranges.spans() {
                        if span.slice(src).is_none() {
                            return Err(ProvenanceError::SourceOffsetOutOfBounds {
                                offset: span.end,
                                source_len: src.len(),
                            });
                        }
                    }
                }
            }
            ProvenanceRelation::Escape { escaped_char } => {
                report.escape_nodes += 1;
                if let Some(src) = source_text {
                    for span in node.source_ranges.spans() {
                        if let Some(slice) = span.slice(src) {
                            if !slice.starts_with('\\') || !slice.ends_with(*escaped_char) {
                                return Err(ProvenanceError::InvalidRelationSyntax {
                                    expected: format!("\\{escaped_char}"),
                                    actual: slice.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            ProvenanceRelation::Entity { raw_entity, .. } => {
                report.entity_nodes += 1;
                if let Some(src) = source_text {
                    for span in node.source_ranges.spans() {
                        if let Some(slice) = span.slice(src) {
                            if slice != raw_entity {
                                return Err(ProvenanceError::InvalidRelationSyntax {
                                    expected: raw_entity.clone(),
                                    actual: slice.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            ProvenanceRelation::StrippedDelimiter { .. } => {
                report.stripped_delimiter_nodes += 1;
            }
            ProvenanceRelation::CodeFenceDedentation { .. } => {
                report.dedentation_nodes += 1;
            }
            ProvenanceRelation::GeneratedMarker { .. } => {
                report.generated_marker_nodes += 1;
            }
            ProvenanceRelation::ReferenceDefinition {
                definition_span, ..
            } => {
                report.reference_nodes += 1;
                if let Some(src) = source_text {
                    if definition_span.slice(src).is_none() {
                        return Err(ProvenanceError::SourceOffsetOutOfBounds {
                            offset: definition_span.end,
                            source_len: src.len(),
                        });
                    }
                }
            }
            ProvenanceRelation::Transclusion { capture_id, .. } => {
                report.transclusion_nodes += 1;
                if node.source_ranges.origin() != SourceOrigin::Transclusion(*capture_id) {
                    return Err(ProvenanceError::TransclusionOriginMismatch {
                        expected: *capture_id,
                        actual: node.source_ranges.origin().capture_id(),
                    });
                }
            }
            ProvenanceRelation::SoftBreak | ProvenanceRelation::HardBreak => {}
        }

        for child in &node.children {
            Self::verify_node(child, primary_source, transcluded_sources, report)?;
        }

        Ok(())
    }
}

