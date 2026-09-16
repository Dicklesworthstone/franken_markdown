//! Headless continuous-flow layout engine and semantic fixture generator (FCB-073.B).
//!
//! Plan §12.9 & §27.4:
//! - "Add a tiny FrankenMarkdown-owned headless flow consumer before the FCB
//!   integration passes. It measures/layouts the same document, validates nested
//!   provenance and budgets, and serializes a deterministic semantic layout fixture
//!   with no FCB dependency."
//! - "The entire reusable Markdown pipeline belongs upstream: parsing/dialect
//!   fixes, nested inline/source provenance, references/heading IDs, block
//!   semantics, safe asset requests, reusable style rules, continuous-flow
//!   layout, text/math/diagram integration, tables/lists/code, renderer-neutral
//!   display output, reading order/accessibility semantics, incremental
//!   dependencies, and selection/copy maps."
//! - "Giant/hostile document budgets: protect against runaway layout and memory."

#![forbid(unsafe_code)]

use std::fmt;

use crate::parse_markdown_spanned;
use crate::source_map::{
    DocumentSourceMap, HeadingSourceAnchor, RenderedElement, SourceMapError, TextSelectionRange,
};
use crate::span::{ProvenanceError, SourceSpan, SpannedDocument};

/// Viewport and typography constraints for continuous-flow layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowConstraints {
    /// Target viewport width in columns or layout units (e.g. 80 columns).
    pub viewport_width: u32,
    /// Height of one text line in vertical layout units (e.g. 16).
    pub line_height: u32,
    /// Width of one character in horizontal layout units (e.g. 1 or 8).
    pub char_width: u32,
    /// Optional limit on the number of rendered lines.
    pub max_viewport_lines: Option<usize>,
}

impl Default for FlowConstraints {
    fn default() -> Self {
        Self {
            viewport_width: 80,
            line_height: 16,
            char_width: 1,
            max_viewport_lines: None,
        }
    }
}

/// Resource budgets defending against giant or hostile documents.
///
/// Plan §27.4: "giant/hostile document budgets"
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowBudgets {
    /// Maximum allowed top-level AST blocks.
    pub max_blocks: usize,
    /// Maximum allowed flow lines.
    pub max_lines: usize,
    /// Maximum allowed document source bytes.
    pub max_bytes: usize,
    /// Maximum allowed interactive rendered elements.
    pub max_items: usize,
}

impl Default for FlowBudgets {
    fn default() -> Self {
        Self {
            max_blocks: 50_000,
            max_lines: 200_000,
            max_bytes: 32 * 1024 * 1024, // 32 MiB
            max_items: 500_000,
        }
    }
}

impl FlowBudgets {
    /// Strict budgets for hostile-document tests and untrusted inputs.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            max_blocks: 500,
            max_lines: 2_000,
            max_bytes: 128 * 1024, // 128 KiB
            max_items: 5_000,
        }
    }
}

/// Errors during headless continuous flow layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlowError {
    /// A configured resource budget was exceeded.
    BudgetExceeded { reason: String },
    /// Invalid constraints (e.g. zero viewport width or line height).
    InvalidConstraint { reason: String },
    /// Upstream source map error.
    SourceMap(SourceMapError),
    /// Upstream provenance error.
    Provenance(ProvenanceError),
}

impl fmt::Display for FlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BudgetExceeded { reason } => write!(f, "flow budget exceeded: {}", reason),
            Self::InvalidConstraint { reason } => write!(f, "invalid flow constraint: {}", reason),
            Self::SourceMap(err) => write!(f, "source map error: {}", err),
            Self::Provenance(err) => write!(f, "provenance error: {}", err),
        }
    }
}

impl std::error::Error for FlowError {}

impl From<SourceMapError> for FlowError {
    fn from(err: SourceMapError) -> Self {
        Self::SourceMap(err)
    }
}

impl From<ProvenanceError> for FlowError {
    fn from(err: ProvenanceError) -> Self {
        Self::Provenance(err)
    }
}

/// A laid-out text line with rendered content, bounding geometry, and source spans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowLine {
    /// Zero-based line index in reading order.
    pub line_index: usize,
    /// Vertical baseline coordinate in layout units.
    pub baseline_y: u32,
    /// Rendered text content of this line (without trailing newline).
    pub rendered_text: String,
    /// Byte range of this line in the full rendered reading text.
    pub rendered_range: TextSelectionRange,
    /// Enclosing source span covering all elements present on this line.
    pub source_span: SourceSpan,
    /// Indices of rendered elements that contributed to this line.
    pub element_indices: Vec<usize>,
}

/// The complete output of continuous-flow layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowOutput {
    /// All laid-out lines in reading order.
    pub lines: Vec<FlowLine>,
    /// Total layout height in layout units.
    pub total_height: u32,
    /// Maximum layout width in layout units.
    pub total_width: u32,
    /// The authoritative document source map.
    pub source_map: DocumentSourceMap,
    /// Total top-level blocks consumed from the AST.
    pub consumed_blocks: usize,
    /// Total primary source bytes consumed.
    pub consumed_bytes: usize,
    /// Total lines produced.
    pub consumed_lines: usize,
}

impl FlowOutput {
    /// Borrow all authoritative heading anchors.
    #[inline]
    #[must_use]
    pub fn headings(&self) -> &[HeadingSourceAnchor] {
        self.source_map.headings()
    }

    /// Borrow all interactive rendered elements.
    #[inline]
    #[must_use]
    pub fn elements(&self) -> &[RenderedElement] {
        self.source_map.elements()
    }

    /// Serialize a deterministic, canonical semantic layout fixture.
    ///
    /// Plan §12.9: "measures/layouts the same document, validates nested provenance
    /// and budgets, and serializes a deterministic semantic layout fixture with
    /// no FCB dependency."
    #[must_use]
    pub fn to_semantic_fixture(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        out.push_str("=== FRANKEN_MARKDOWN SEMANTIC LAYOUT FIXTURE ===\n");
        let _ = writeln!(out, "total_lines: {}", self.lines.len());
        let _ = writeln!(out, "total_height: {}", self.total_height);
        let _ = writeln!(out, "total_width: {}", self.total_width);
        let _ = writeln!(out, "consumed_blocks: {}", self.consumed_blocks);
        let _ = writeln!(out, "consumed_bytes: {}", self.consumed_bytes);

        out.push_str("\n--- HEADINGS ---\n");
        for h in self.source_map.headings() {
            let _ = writeln!(
                out,
                "#{slug} [level={level}] span=[{start}, {end}) title=\"{title}\" rendered_offset={offset}",
                slug = h.slug,
                level = h.level,
                start = h.source_span.start,
                end = h.source_span.end,
                title = h.title,
                offset = h.rendered_offset,
            );
        }

        out.push_str("\n--- LINES ---\n");
        for line in &self.lines {
            let _ = writeln!(
                out,
                "[{:04}] y={:04} rendered=[{}, {}) source=[{}, {}) \"{}\"",
                line.line_index,
                line.baseline_y,
                line.rendered_range.start,
                line.rendered_range.end,
                line.source_span.start,
                line.source_span.end,
                line.rendered_text,
            );
        }

        out.push_str("\n--- PROVENANCE AUDIT ---\n");
        let _ = writeln!(
            out,
            "total_elements: {}",
            self.source_map.elements().len()
        );
        let _ = writeln!(
            out,
            "provenance_nodes: {}",
            self.source_map.provenance_graph().total_nodes()
        );
        out.push_str("=== END FIXTURE ===\n");
        out
    }
}

/// Headless continuous-flow layout consumer with budget enforcement and provenance tracking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadlessFlowConsumer {
    constraints: FlowConstraints,
    budgets: FlowBudgets,
}

impl Default for HeadlessFlowConsumer {
    fn default() -> Self {
        Self {
            constraints: FlowConstraints::default(),
            budgets: FlowBudgets::default(),
        }
    }
}

impl HeadlessFlowConsumer {
    /// Create a headless flow consumer with explicit constraints and budgets.
    #[must_use]
    pub const fn new(constraints: FlowConstraints, budgets: FlowBudgets) -> Self {
        Self {
            constraints,
            budgets,
        }
    }

    /// Create a headless flow consumer with custom constraints and default budgets.
    #[must_use]
    pub fn with_constraints(constraints: FlowConstraints) -> Self {
        Self {
            constraints,
            budgets: FlowBudgets::default(),
        }
    }

    /// Flow a pre-parsed spanned document and source text into lines and source mapping.
    pub fn consume_document(
        &self,
        doc: &SpannedDocument,
        source: &str,
    ) -> Result<FlowOutput, FlowError> {
        self.validate_constraints()?;
        self.validate_budgets(doc, source)?;

        let source_map = DocumentSourceMap::from_spanned_document(doc, source)?;
        if source_map.elements().len() > self.budgets.max_items {
            return Err(FlowError::BudgetExceeded {
                reason: format!(
                    "rendered elements {} exceeds budget {}",
                    source_map.elements().len(),
                    self.budgets.max_items
                ),
            });
        }

        let max_cols = if self.constraints.char_width > 0 {
            (self.constraints.viewport_width / self.constraints.char_width).max(1) as usize
        } else {
            80
        };

        let rendered = source_map.rendered_text();
        let mut lines = Vec::new();
        let mut max_line_width_chars = 0usize;

        let paragraphs = rendered.split("\n\n");
        let mut current_offset = 0usize;

        for para in paragraphs {
            if para.is_empty() {
                current_offset = current_offset.saturating_add(2).min(rendered.len());
                continue;
            }

            let para_start = current_offset;
            let para_lines = para.split('\n');

            for raw_line in para_lines {
                let mut line_cursor = 0usize;
                let raw_bytes = raw_line.as_bytes();

                while line_cursor < raw_bytes.len() {
                    let Some(remaining) = raw_line.get(line_cursor..) else {
                        break;
                    };
                    let chunk_len = if remaining.chars().count() <= max_cols {
                        remaining.len()
                    } else {
                        // Find wrap point near max_cols characters
                        let mut byte_limit = 0usize;
                        let mut char_count = 0usize;
                        for (idx, ch) in remaining.char_indices() {
                            if char_count >= max_cols {
                                break;
                            }
                            byte_limit = idx.saturating_add(ch.len_utf8());
                            char_count += 1;
                        }

                        // Try to break on whitespace
                        let search_slice = remaining.get(..byte_limit).unwrap_or(remaining);
                        if let Some(last_space) = search_slice.rfind(' ') {
                            if last_space > 0 {
                                last_space.saturating_add(1)
                            } else {
                                byte_limit
                            }
                        } else {
                            byte_limit
                        }
                    };

                    let chunk = remaining.get(..chunk_len).unwrap_or(remaining);
                    let line_rendered_start = para_start.saturating_add(line_cursor);
                    let line_rendered_end = line_rendered_start.saturating_add(chunk_len);
                    let line_range = TextSelectionRange::new(line_rendered_start, line_rendered_end);

                    let (source_span, element_indices) =
                        Self::compute_line_source_span(&source_map, line_range);

                    let line_idx = lines.len();
                    if line_idx >= self.budgets.max_lines {
                        return Err(FlowError::BudgetExceeded {
                            reason: format!(
                                "line count reached budget limit of {}",
                                self.budgets.max_lines
                            ),
                        });
                    }

                    if let Some(max_view) = self.constraints.max_viewport_lines {
                        if line_idx >= max_view {
                            break;
                        }
                    }

                    let baseline_y = (line_idx as u32).saturating_mul(self.constraints.line_height);
                    let line_chars = chunk.trim_end().chars().count();
                    max_line_width_chars = max_line_width_chars.max(line_chars);

                    lines.push(FlowLine {
                        line_index: line_idx,
                        baseline_y,
                        rendered_text: chunk.trim_end().to_string(),
                        rendered_range: line_range,
                        source_span,
                        element_indices,
                    });

                    line_cursor += chunk_len;
                }
            }

            current_offset = para_start + para.len() + 2;
        }

        let total_lines = lines.len();
        let total_height = (total_lines as u32).saturating_mul(self.constraints.line_height);
        let total_width = (max_line_width_chars as u32).saturating_mul(self.constraints.char_width);

        Ok(FlowOutput {
            lines,
            total_height,
            total_width,
            source_map,
            consumed_blocks: doc.block_count(),
            consumed_bytes: source.len(),
            consumed_lines: total_lines,
        })
    }

    /// Parse Markdown source and flow into lines and source mapping.
    pub fn consume_source(&self, source: &str) -> Result<FlowOutput, FlowError> {
        if source.len() > self.budgets.max_bytes {
            return Err(FlowError::BudgetExceeded {
                reason: format!(
                    "source length {} bytes exceeds budget of {}",
                    source.len(),
                    self.budgets.max_bytes
                ),
            });
        }
        let doc = parse_markdown_spanned(source);
        self.consume_document(&doc, source)
    }

    fn validate_constraints(&self) -> Result<(), FlowError> {
        if self.constraints.viewport_width == 0 {
            return Err(FlowError::InvalidConstraint {
                reason: "viewport_width must be greater than zero".to_string(),
            });
        }
        if self.constraints.line_height == 0 {
            return Err(FlowError::InvalidConstraint {
                reason: "line_height must be greater than zero".to_string(),
            });
        }
        if self.constraints.char_width == 0 {
            return Err(FlowError::InvalidConstraint {
                reason: "char_width must be greater than zero".to_string(),
            });
        }
        Ok(())
    }

    fn validate_budgets(&self, doc: &SpannedDocument, source: &str) -> Result<(), FlowError> {
        if source.len() > self.budgets.max_bytes {
            return Err(FlowError::BudgetExceeded {
                reason: format!(
                    "source length {} exceeds budget {}",
                    source.len(),
                    self.budgets.max_bytes
                ),
            });
        }
        if doc.block_count() > self.budgets.max_blocks {
            return Err(FlowError::BudgetExceeded {
                reason: format!(
                    "block count {} exceeds budget {}",
                    doc.block_count(),
                    self.budgets.max_blocks
                ),
            });
        }
        Ok(())
    }

    fn compute_line_source_span(
        source_map: &DocumentSourceMap,
        line_range: TextSelectionRange,
    ) -> (SourceSpan, Vec<usize>) {
        let mut min_start = usize::MAX;
        let mut max_end = 0;
        let mut element_indices = Vec::new();

        for (idx, elem) in source_map.elements().iter().enumerate() {
            if !elem.rendered_range.is_empty() && elem.rendered_range.overlaps(line_range) {
                element_indices.push(idx);
                for span in elem.source_ranges.spans() {
                    min_start = min_start.min(span.start);
                    max_end = max_end.max(span.end);
                }
            }
        }

        if min_start <= max_end && max_end > 0 {
            (SourceSpan::new(min_start, max_end), element_indices)
        } else {
            (SourceSpan::default(), element_indices)
        }
    }
}
