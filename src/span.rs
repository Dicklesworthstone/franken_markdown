//! Source-span and parser-diagnostic scaffolding.
//!
//! These types are intentionally renderer-neutral. The existing AST remains the
//! rendering contract; spanned wrappers let editor/WASM integrations, diagnostics,
//! and future conformance tooling recover source locations without forcing every
//! renderer to carry span metadata.

use crate::ast::{Block, Document, Inline, ListItem, Table};

/// A byte range in the original Markdown source: `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// A validation error for a renderer-neutral provenance tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

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

        let mut previous = None;
        for child in &self.children {
            child.validate()?;
            if child.span.start < self.span.start || child.span.end > self.span.end {
                return Err(ProvenanceError::ChildOutsideParent {
                    parent: self.span,
                    child: child.span,
                });
            }
            if let Some(previous) = previous {
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
}
