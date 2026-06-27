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
    #[must_use]
    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// True when the span has no byte width.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }

    /// True when `offset` is inside `[start, end)`.
    #[must_use]
    pub fn contains(self, offset: usize) -> bool {
        self.start <= offset && offset < self.end
    }

    /// Return a span covering both inputs.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Borrow the original source slice covered by this span.
    #[must_use]
    pub fn slice(self, source: &str) -> Option<&str> {
        if self.start <= self.end {
            source.get(self.start..self.end)
        } else {
            None
        }
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
