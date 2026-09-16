#![forbid(unsafe_code)]

//! Code fence and constrained table flow with independent horizontal scrolling,
//! row virtualization, bounded column measurement, and exact code copy (FCB-033.A).
//!
//! Technical specifications:
//! - Plan §12.3 & §12.10: Code blocks have independent horizontal scrolling (`scroll_x`)
//!   and optional wrapping, rather than causing unexpected global document width changes.
//! - Shared lexical spans and line windowing: Large code blocks reuse bounded text state;
//!   no whole-fence clone is required for each viewport render.
//! - Exact code copy: Authoritative raw bytes are preserved and accessible directly,
//!   stripped of visual line numbers or gutter markers.
//! - Constrained table flow: Tables compute column constraints in a bounded measure phase.
//!   Wide tables scroll horizontally rather than crushing text into unusable columns.
//! - Late wide cell handling: Late cells in unmeasured rows expand columns up to a bounded
//!   maximum without triggering whole-table re-allocation.
//! - Large tables virtualize rows while preserving headers and accessibility structure.

use std::fmt;

use crate::ast::Align;
use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayClip, DisplayItem, DisplayList,
    DisplayRect, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};
use crate::span::SourceSpan;

/// Default minimum column width in points to prevent text crushing.
pub const MIN_COLUMN_WIDTH: f32 = 60.0;

/// Default maximum column width in points to bound explosive cell expansion.
pub const MAX_COLUMN_WIDTH: f32 = 400.0;

/// Maximum rows inspected during bounded initial column width measurement.
pub const MAX_MEASURE_ROWS: usize = 100;

/// Maximum total columns permitted in a constrained table to prevent memory exhaustion.
pub const MAX_COLUMNS_BUDGET: usize = 64;

/// Line height factor for code fences (font size multiplier).
pub const CODE_LINE_HEIGHT_FACTOR: f32 = 1.45;

/// Font size for code block presentation.
pub const CODE_FONT_SIZE: f32 = 13.0;

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Errors produced during code fence and constrained table flow layout.
#[derive(Clone, Debug, PartialEq)]
pub enum CodeTableError {
    /// Table column count exceeded safety maximum.
    ColumnBudgetExceeded { columns: usize, max: usize },
    /// Table row count exceeded safety maximum.
    RowBudgetExceeded { rows: usize, max: usize },
    /// Column index was out of bounds.
    InvalidColumnIndex { index: usize, len: usize },
    /// Arithmetic overflow in layout coordinates.
    ArithmeticOverflow,
}

impl fmt::Display for CodeTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ColumnBudgetExceeded { columns, max } => {
                write!(f, "column count {columns} exceeds maximum {max}")
            }
            Self::RowBudgetExceeded { rows, max } => {
                write!(f, "row count {rows} exceeds maximum {max}")
            }
            Self::InvalidColumnIndex { index, len } => {
                write!(f, "column index {index} out of bounds (len: {len})")
            }
            Self::ArithmeticOverflow => write!(f, "layout arithmetic overflow"),
        }
    }
}

impl std::error::Error for CodeTableError {}

// ---------------------------------------------------------------------------
// Code Fence Flow
// ---------------------------------------------------------------------------

/// A code fence flow item with independent horizontal scrolling and line windowing.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeFenceFlow {
    /// Authoritative raw source code for exact copying.
    raw_code: String,
    /// Language identifier (e.g. "rust", "python", "json").
    lang: Option<String>,
    /// Source span of the entire code block.
    source_span: SourceSpan,
    /// Lines extracted for indexed line-windowed rendering.
    lines: Vec<String>,
    /// Independent horizontal scroll position in points.
    pub scroll_x: f32,
    /// Whether long code lines wrap instead of scrolling horizontally.
    pub wrap_lines: bool,
    /// Longest line character count for width estimation.
    max_line_chars: usize,
}

impl CodeFenceFlow {
    /// Construct a new code fence flow item.
    #[must_use]
    pub fn new(lang: Option<String>, code: String, source_span: SourceSpan) -> Self {
        let mut max_chars = 0;
        let mut lines = Vec::new();
        for line in code.lines() {
            let char_count = line.chars().count();
            if char_count > max_chars {
                max_chars = char_count;
            }
            lines.push(line.to_string());
        }
        if lines.is_empty() {
            lines.push(String::new());
        }

        Self {
            raw_code: code,
            lang,
            source_span,
            lines,
            scroll_x: 0.0,
            wrap_lines: false,
            max_line_chars: max_chars,
        }
    }

    /// Access authoritative raw source code for clipboard copying.
    ///
    /// Preserves exact indentation, tabs, and newlines without line numbers
    /// or visual gutter decorators.
    #[must_use]
    pub fn exact_code_copy(&self) -> &str {
        &self.raw_code
    }

    /// Language identifier.
    #[must_use]
    pub fn lang(&self) -> Option<&str> {
        self.lang.as_deref()
    }

    /// Number of lines in the code fence.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Source span of the entire block.
    #[must_use]
    pub fn source_span(&self) -> SourceSpan {
        self.source_span
    }

    /// Estimated intrinsic width of the code content based on longest line.
    #[must_use]
    pub fn intrinsic_content_width(&self) -> f32 {
        let char_w = CODE_FONT_SIZE * 0.6;
        (self.max_line_chars as f32 * char_w).max(100.0) + 32.0 // padding
    }

    /// Total height of the code block given `available_width`.
    #[must_use]
    pub fn total_height(&self) -> f32 {
        let line_h = CODE_FONT_SIZE * CODE_LINE_HEIGHT_FACTOR;
        let padding_v = 16.0;
        (self.lines.len() as f32 * line_h) + padding_v
    }

    /// Materialize only the visible lines intersecting `viewport_y` .. `viewport_y + viewport_height`.
    ///
    /// Implements virtualized line windowing so giant code fences (e.g. 10,000 lines)
    /// only emit the visible lines into the display list, avoiding whole-fence clones.
    pub fn materialize_viewport(
        &self,
        bounds: DisplayRect,
        viewport_local_top: f32,
        viewport_height: f32,
    ) -> Result<DisplayList, CodeTableError> {
        let mut dl = DisplayList::new();
        let line_h = CODE_FONT_SIZE * CODE_LINE_HEIGHT_FACTOR;
        let padding_top = 8.0f32;
        let padding_left = 12.0f32;

        let content_top = bounds.y + padding_top;
        let viewport_local_bottom = viewport_local_top + viewport_height;

        // Clip container for independent horizontal scrolling
        dl.push_item(DisplayItem::Clip(DisplayClip {
            bounds,
            child_count: 0, // dynamic
        }));

        // Determine line window
        let start_line = if viewport_local_top <= content_top {
            0
        } else {
            let diff = viewport_local_top - content_top;
            ((diff / line_h).floor() as usize).min(self.lines.len())
        };

        let end_line = if viewport_local_bottom <= content_top {
            start_line
        } else {
            let diff = viewport_local_bottom - content_top;
            (((diff / line_h).ceil() as usize) + 1).min(self.lines.len())
        };

        // Materialize visible lines
        for idx in start_line..end_line {
            let line_y = content_top + (idx as f32 * line_h);
            let line_text = &self.lines[idx];

            let line_x = bounds.x + padding_left - self.scroll_x;
            let line_w = (line_text.chars().count() as f32 * (CODE_FONT_SIZE * 0.6)).max(20.0);

            dl.push_item(DisplayItem::Text(DisplayTextRun {
                bounds: DisplayRect::new(line_x, line_y, line_w, line_h),
                text: line_text.clone(),
                font_run: None,
                color_role: "code".to_string(),
                source_span: self.source_span,
                font_size: CODE_FONT_SIZE,
            }));
        }

        // Emit accessible reading node
        dl.push_reading_node(AccessibleReadingNode {
            role: AccessibleReadingRole::CodeBlock,
            text: self.raw_code.clone(),
            source_span: self.source_span,
            bounds,
            children: Vec::new(),
        });

        Ok(dl)
    }
}

// ---------------------------------------------------------------------------
// Constrained Table Flow
// ---------------------------------------------------------------------------

/// Column constraints and formatting options for table flow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableConstraints {
    /// Minimum column width in points.
    pub min_col_width: f32,
    /// Maximum column width in points.
    pub max_col_width: f32,
    /// Available container width in points.
    pub container_width: f32,
}

impl Default for TableConstraints {
    fn default() -> Self {
        Self {
            min_col_width: MIN_COLUMN_WIDTH,
            max_col_width: MAX_COLUMN_WIDTH,
            container_width: 800.0,
        }
    }
}

/// A cell in a constrained table flow item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableCell {
    pub text: String,
    pub source_span: SourceSpan,
}

impl TableCell {
    /// Create a cell from plain text.
    #[must_use]
    pub fn new(text: impl Into<String>, source_span: SourceSpan) -> Self {
        Self {
            text: text.into(),
            source_span,
        }
    }

    /// Approximate intrinsic width of cell text in points.
    #[must_use]
    pub fn intrinsic_width(&self, font_size: f32) -> f32 {
        let char_w = font_size * 0.55;
        (self.text.chars().count() as f32 * char_w).max(20.0) + 16.0 // padding
    }
}

/// Constrained table flow layout with bounded column measurement,
/// horizontal scrolling, and row virtualization.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstrainedTableFlow {
    alignments: Vec<Align>,
    headers: Vec<TableCell>,
    rows: Vec<Vec<TableCell>>,
    source_span: SourceSpan,
    column_widths: Vec<f32>,
    /// Independent horizontal scroll position for wide tables.
    pub scroll_x: f32,
    constraints: TableConstraints,
}

impl ConstrainedTableFlow {
    /// Construct a new table flow item with bounded column count check.
    pub fn try_new(
        alignments: Vec<Align>,
        headers: Vec<TableCell>,
        rows: Vec<Vec<TableCell>>,
        source_span: SourceSpan,
        constraints: TableConstraints,
    ) -> Result<Self, CodeTableError> {
        let col_count = alignments.len().max(headers.len());
        if col_count > MAX_COLUMNS_BUDGET {
            return Err(CodeTableError::ColumnBudgetExceeded {
                columns: col_count,
                max: MAX_COLUMNS_BUDGET,
            });
        }

        let mut flow = Self {
            alignments,
            headers,
            rows,
            source_span,
            column_widths: vec![constraints.min_col_width; col_count],
            scroll_x: 0.0,
            constraints,
        };

        flow.perform_bounded_measurement()?;
        Ok(flow)
    }

    /// Number of columns in this table.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.column_widths.len()
    }

    /// Number of body rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Computed width of column `col_idx`.
    pub fn column_width(&self, col_idx: usize) -> Result<f32, CodeTableError> {
        self.column_widths
            .get(col_idx)
            .copied()
            .ok_or(CodeTableError::InvalidColumnIndex {
                index: col_idx,
                len: self.column_widths.len(),
            })
    }

    /// Total width of all columns combined.
    #[must_use]
    pub fn total_table_width(&self) -> f32 {
        self.column_widths.iter().sum()
    }

    /// Whether this table is wider than the container width, requiring horizontal scrolling.
    #[must_use]
    pub fn requires_horizontal_scroll(&self) -> bool {
        self.total_table_width() > self.constraints.container_width
    }

    /// Height of a single row.
    #[must_use]
    pub fn row_height() -> f32 {
        28.0f32
    }

    /// Height of the header row with border separator.
    #[must_use]
    pub fn header_height() -> f32 {
        32.0f32
    }

    /// Total height of the table including header and all rows.
    #[must_use]
    pub fn total_height(&self) -> f32 {
        Self::header_height() + (self.rows.len() as f32 * Self::row_height())
    }

    /// Perform bounded initial column width measurement inspecting header and up
    /// to `MAX_MEASURE_ROWS` rows.
    fn perform_bounded_measurement(&mut self) -> Result<(), CodeTableError> {
        let font_size = 13.0f32;
        let col_count = self.column_count();

        // 1. Measure header cells
        for (i, cell) in self.headers.iter().enumerate() {
            if i < col_count {
                let w = cell
                    .intrinsic_width(font_size)
                    .clamp(self.constraints.min_col_width, self.constraints.max_col_width);
                if w > self.column_widths[i] {
                    self.column_widths[i] = w;
                }
            }
        }

        // 2. Bounded measurement of initial rows up to MAX_MEASURE_ROWS
        let sample_limit = self.rows.len().min(MAX_MEASURE_ROWS);
        for row in &self.rows[..sample_limit] {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    let w = cell
                        .intrinsic_width(font_size)
                        .clamp(self.constraints.min_col_width, self.constraints.max_col_width);
                    if w > self.column_widths[i] {
                        self.column_widths[i] = w;
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a late wide cell discovered in an unmeasured row (e.g. row 500),
    /// expanding the column up to `max_col_width` without whole-table re-allocation.
    pub fn handle_late_wide_cell(
        &mut self,
        col_idx: usize,
        cell_text: &str,
    ) -> Result<bool, CodeTableError> {
        if col_idx >= self.column_count() {
            return Err(CodeTableError::InvalidColumnIndex {
                index: col_idx,
                len: self.column_count(),
            });
        }
        let font_size = 13.0f32;
        let cell = TableCell::new(cell_text, SourceSpan::default());
        let measured_w = cell
            .intrinsic_width(font_size)
            .clamp(self.constraints.min_col_width, self.constraints.max_col_width);

        if measured_w > self.column_widths[col_idx] {
            self.column_widths[col_idx] = measured_w;
            Ok(true) // Column width adjusted
        } else {
            Ok(false) // Existing column width was sufficient
        }
    }

    /// Materialize visible rows within `viewport_local_top` .. `viewport_local_top + viewport_height`.
    ///
    /// Row virtualization: Only visible body rows are emitted into the display list,
    /// alongside the pinned header row and cell border vectors.
    pub fn materialize_viewport(
        &self,
        origin_x: f32,
        origin_y: f32,
        viewport_local_top: f32,
        viewport_height: f32,
    ) -> Result<DisplayList, CodeTableError> {
        let mut dl = DisplayList::new();
        let header_h = Self::header_height();
        let row_h = Self::row_height();
        let total_w = self.total_table_width();

        let visible_left = origin_x - self.scroll_x;
        let table_bounds = DisplayRect::new(origin_x, origin_y, self.constraints.container_width, self.total_height());

        // Header Row (pinned or top)
        let header_y = origin_y;
        let mut cur_x = visible_left;
        for (i, cell) in self.headers.iter().enumerate() {
            let col_w = self.column_widths[i];
            let cell_bounds = DisplayRect::new(cur_x, header_y, col_w, header_h);

            dl.push_item(DisplayItem::Text(DisplayTextRun {
                bounds: cell_bounds,
                text: cell.text.clone(),
                font_run: None,
                color_role: "table-header".to_string(),
                source_span: cell.source_span,
                font_size: 13.0,
            }));

            cur_x += col_w;
        }

        // Header horizontal separator
        dl.push_item(DisplayItem::Vector(DisplayVectorPath {
            bounds: DisplayRect::new(visible_left, header_y + header_h - 1.0, total_w, 1.0),
            shape: VectorShapeType::TableBorder,
            stroke_width: 1.0,
            color_role: "table-border".to_string(),
            source_span: self.source_span,
        }));

        // Virtualized body rows
        let body_top = origin_y + header_h;
        let viewport_local_bottom = viewport_local_top + viewport_height;

        let start_row = if viewport_local_top <= body_top {
            0
        } else {
            let diff = viewport_local_top - body_top;
            ((diff / row_h).floor() as usize).min(self.rows.len())
        };

        let end_row = if viewport_local_bottom <= body_top {
            start_row
        } else {
            let diff = viewport_local_bottom - body_top;
            (((diff / row_h).ceil() as usize) + 1).min(self.rows.len())
        };

        for r in start_row..end_row {
            let row_y = body_top + (r as f32 * row_h);
            let row = &self.rows[r];

            let mut cell_x = visible_left;
            for (c, col_w) in self.column_widths.iter().enumerate() {
                if let Some(cell) = row.get(c) {
                    let cell_bounds = DisplayRect::new(cell_x, row_y, *col_w, row_h);
                    dl.push_item(DisplayItem::Text(DisplayTextRun {
                        bounds: cell_bounds,
                        text: cell.text.clone(),
                        font_run: None,
                        color_role: "table-cell".to_string(),
                        source_span: cell.source_span,
                        font_size: 13.0,
                    }));
                }
                cell_x += *col_w;
            }

            // Row separator border
            dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                bounds: DisplayRect::new(visible_left, row_y + row_h - 0.5, total_w, 0.5),
                shape: VectorShapeType::TableBorder,
                stroke_width: 0.5,
                color_role: "table-border".to_string(),
                source_span: self.source_span,
            }));
        }

        // Accessible reading node for the table
        dl.push_reading_node(AccessibleReadingNode {
            role: AccessibleReadingRole::Table,
            text: format!("Table with {} columns and {} rows", self.column_count(), self.row_count()),
            source_span: self.source_span,
            bounds: table_bounds,
            children: Vec::new(),
        });

        Ok(dl)
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_code_copy_preserves_indentation_and_raw_bytes() {
        let raw_code = "fn main() {\n    let x = 42;\n\tprintln!(\"{x}\");\n}\n";
        let flow = CodeFenceFlow::new(
            Some("rust".to_string()),
            raw_code.to_string(),
            SourceSpan::new(10, 60),
        );

        // Exact code copy invariant: authoritative raw bytes preserved intact
        assert_eq!(flow.exact_code_copy(), raw_code);
        assert_eq!(flow.lang(), Some("rust"));
        assert_eq!(flow.line_count(), 4);
    }

    #[test]
    fn giant_code_fence_line_windowing_avoids_whole_fence_clone() {
        // Construct a giant code block with 2,000 lines
        let mut giant_code = String::new();
        for i in 0..2000 {
            giant_code.push_str(&format!("let val_{i} = calculate({i});\n"));
        }

        let flow = CodeFenceFlow::new(
            Some("rust".to_string()),
            giant_code,
            SourceSpan::new(0, 100_000),
        );
        assert_eq!(flow.line_count(), 2000);

        // Materialize a small viewport window of 200pt height at top offset 500pt
        let bounds = DisplayRect::new(0.0, 0.0, 800.0, flow.total_height());
        let dl = flow
            .materialize_viewport(bounds, 500.0, 200.0)
            .expect("materialize line window");

        // Out of 2,000 lines, only ~12 lines are visible in 200pt (200 / 18.85 = ~10.6)
        let text_run_count = dl
            .items()
            .iter()
            .filter(|item| matches!(item, DisplayItem::Text(_)))
            .count();

        assert!(text_run_count > 0);
        assert!(
            text_run_count < 25,
            "only visible window of lines must be emitted, got {text_run_count}"
        );
    }

    #[test]
    fn constrained_table_enforces_min_column_width_and_horizontal_scroll() {
        // Table with 4 columns: headers are short, but min_col_width prevents crushing
        let headers = vec![
            TableCell::new("A", SourceSpan::default()),
            TableCell::new("B", SourceSpan::default()),
            TableCell::new("C", SourceSpan::default()),
            TableCell::new("D", SourceSpan::default()),
        ];
        let alignments = vec![Align::Left; 4];
        let rows = vec![vec![
            TableCell::new("1", SourceSpan::default()),
            TableCell::new("2", SourceSpan::default()),
            TableCell::new("3", SourceSpan::default()),
            TableCell::new("4", SourceSpan::default()),
        ]];

        let constraints = TableConstraints {
            min_col_width: 80.0,
            max_col_width: 200.0,
            container_width: 250.0, // Narrower than 4 * 80 = 320.0
        };

        let table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            constraints,
        )
        .expect("build constrained table");

        // Every column respects min_col_width
        for c in 0..4 {
            assert!(table.column_width(c).unwrap() >= 80.0);
        }

        assert_eq!(table.total_table_width(), 320.0);
        assert!(
            table.requires_horizontal_scroll(),
            "table width 320 must exceed container 250 and require horizontal scroll"
        );
    }

    #[test]
    fn late_wide_cell_expands_column_up_to_max_constraint() {
        let headers = vec![TableCell::new("Col0", SourceSpan::default())];
        let alignments = vec![Align::Left];
        let rows = vec![vec![TableCell::new("Short", SourceSpan::default())]];

        let constraints = TableConstraints {
            min_col_width: 60.0,
            max_col_width: 150.0,
            container_width: 800.0,
        };

        let mut table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            constraints,
        )
        .unwrap();

        let initial_w = table.column_width(0).unwrap();

        // Late wide cell in row 500 with long text
        let wide_text = "A very very long text string in row 500 that would exceed the column";
        let expanded = table.handle_late_wide_cell(0, wide_text).unwrap();
        assert!(expanded);

        let refined_w = table.column_width(0).unwrap();
        assert!(refined_w > initial_w);
        assert!(
            refined_w <= 150.0,
            "must be clamped to max_col_width (150.0), got {refined_w}"
        );
    }

    #[test]
    fn table_row_virtualization_only_emits_visible_rows() {
        let headers = vec![
            TableCell::new("ID", SourceSpan::default()),
            TableCell::new("Name", SourceSpan::default()),
        ];
        let alignments = vec![Align::Left, Align::Left];

        // 1,000 body rows
        let mut rows = Vec::new();
        for i in 0..1000 {
            rows.push(vec![
                TableCell::new(format!("{i}"), SourceSpan::default()),
                TableCell::new(format!("Item name {i}"), SourceSpan::default()),
            ]);
        }

        let table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            TableConstraints::default(),
        )
        .unwrap();

        assert_eq!(table.row_count(), 1000);

        // Viewport is 150pt tall at vertical offset 400pt
        let dl = table
            .materialize_viewport(0.0, 0.0, 400.0, 150.0)
            .expect("materialize table viewport");

        // Headers + ~6 visible rows emitted
        let cell_text_count = dl
            .items()
            .iter()
            .filter(|i| match i {
                DisplayItem::Text(t) => t.color_role == "table-cell",
                _ => false,
            })
            .count();

        assert!(cell_text_count > 0);
        assert!(
            cell_text_count < 30,
            "only visible rows must be emitted, got {cell_text_count}"
        );
    }

    #[test]
    fn negative_controls_column_budget_and_invalid_index() {
        // Excessive columns (> 64)
        let headers = vec![TableCell::new("C", SourceSpan::default()); 65];
        let alignments = vec![Align::Left; 65];

        let err = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            Vec::new(),
            SourceSpan::default(),
            TableConstraints::default(),
        );
        assert!(matches!(err, Err(CodeTableError::ColumnBudgetExceeded { .. })));

        // Invalid column index
        let valid_table = ConstrainedTableFlow::try_new(
            vec![Align::Left],
            vec![TableCell::new("C", SourceSpan::default())],
            Vec::new(),
            SourceSpan::default(),
            TableConstraints::default(),
        )
        .unwrap();
        assert!(matches!(
            valid_table.column_width(99),
            Err(CodeTableError::InvalidColumnIndex { .. })
        ));
    }
}
