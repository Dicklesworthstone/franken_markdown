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
    /// Materialized viewport text exceeded its byte budget.
    OutputBudgetExceeded { bytes: usize, max: usize },
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
            Self::OutputBudgetExceeded { bytes, max } => {
                write!(f, "viewport text size {bytes} exceeds maximum {max}")
            }
            Self::InvalidColumnIndex { index, len } => {
                write!(f, "column index {index} out of bounds (len: {len})")
            }
            Self::ArithmeticOverflow => write!(f, "layout arithmetic overflow"),
        }
    }
}

impl std::error::Error for CodeTableError {}

#[path = "code_table_flow/code.rs"]
mod code;
pub use code::CodeFenceFlow;

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
    /// Whether semantic headers remain pinned to the visible viewport top during scrolling.
    pub pinned_headers: bool,
}

impl Default for TableConstraints {
    fn default() -> Self {
        Self {
            min_col_width: MIN_COLUMN_WIDTH,
            max_col_width: MAX_COLUMN_WIDTH,
            container_width: 800.0,
            pinned_headers: true,
        }
    }
}

/// Measurement confidence and progress for a constrained table flow (FCB-033.B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableMeasurementState {
    /// Column widths are provisional estimates based on initial sample rows.
    Provisional { measured_rows: usize, total_rows: usize },
    /// All rows have been measured; column widths are final and optimal.
    Complete { total_rows: usize },
}

/// A record of column width expansion during measurement refinement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnExpansion {
    /// Index of the column that expanded.
    pub col_idx: usize,
    /// Previous column width in points.
    pub old_width: f32,
    /// New expanded column width in points.
    pub new_width: f32,
}

/// Result of an incremental or complete table width measurement step.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementRefinement {
    /// Number of rows measured so far.
    pub measured_rows: usize,
    /// Total rows in the table.
    pub total_rows: usize,
    /// True when all rows have been measured.
    pub is_complete: bool,
    /// List of column expansions that occurred during this step.
    pub column_expansions: Vec<ColumnExpansion>,
    /// Whether an explicit anchored reflow is required due to column width changes.
    pub reflow_required: bool,
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
/// horizontal scrolling, row virtualization, pinned semantic headers,
/// and accessible row/column hierarchy.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstrainedTableFlow {
    alignments: Vec<Align>,
    headers: Vec<TableCell>,
    rows: Vec<Vec<TableCell>>,
    source_span: SourceSpan,
    column_widths: Vec<f32>,
    /// Number of body rows measured so far.
    measured_rows: usize,
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
            measured_rows: 0,
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

    /// Access header cells.
    #[must_use]
    pub fn headers(&self) -> &[TableCell] {
        &self.headers
    }

    /// Access column alignments.
    #[must_use]
    pub fn alignments(&self) -> &[Align] {
        &self.alignments
    }

    /// Access table constraints.
    #[must_use]
    pub fn constraints(&self) -> &TableConstraints {
        &self.constraints
    }

    /// Mutably access table constraints.
    pub fn constraints_mut(&mut self) -> &mut TableConstraints {
        &mut self.constraints
    }

    /// Measurement state indicating whether column widths are provisional estimates
    /// or fully measured.
    #[must_use]
    pub fn measurement_state(&self) -> TableMeasurementState {
        if self.measured_rows >= self.rows.len() {
            TableMeasurementState::Complete {
                total_rows: self.rows.len(),
            }
        } else {
            TableMeasurementState::Provisional {
                measured_rows: self.measured_rows,
                total_rows: self.rows.len(),
            }
        }
    }

    /// Whether all table rows have been measured.
    #[must_use]
    pub fn is_fully_measured(&self) -> bool {
        self.measured_rows >= self.rows.len()
    }

    /// Number of body rows measured so far.
    #[must_use]
    pub fn measured_rows_count(&self) -> usize {
        self.measured_rows
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
        self.measured_rows = sample_limit;

        Ok(())
    }

    /// Measure a subsequent batch of unmeasured rows up to `batch_size`.
    ///
    /// Preserves stable estimates during rendering; changes are reported in
    /// `MeasurementRefinement` with `reflow_required = true` so the host can
    /// trigger an explicit anchored reflow without surprise layout thrashing.
    pub fn measure_batch(&mut self, batch_size: usize) -> Result<MeasurementRefinement, CodeTableError> {
        let font_size = 13.0f32;
        let col_count = self.column_count();
        let total_rows = self.rows.len();
        let start = self.measured_rows;
        let end = (start + batch_size).min(total_rows);

        let mut expansions = Vec::new();

        for row in &self.rows[start..end] {
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx < col_count {
                    let w = cell
                        .intrinsic_width(font_size)
                        .clamp(self.constraints.min_col_width, self.constraints.max_col_width);
                    let old_w = self.column_widths[col_idx];
                    if w > old_w {
                        self.column_widths[col_idx] = w;
                        expansions.push(ColumnExpansion {
                            col_idx,
                            old_width: old_w,
                            new_width: w,
                        });
                    }
                }
            }
        }

        self.measured_rows = end;
        let is_complete = self.measured_rows >= total_rows;
        let reflow_required = !expansions.is_empty();

        Ok(MeasurementRefinement {
            measured_rows: self.measured_rows,
            total_rows,
            is_complete,
            column_expansions: expansions,
            reflow_required,
        })
    }

    /// Complete measurement of all remaining rows in the table.
    pub fn complete_measurement(&mut self) -> Result<MeasurementRefinement, CodeTableError> {
        let remaining = self.rows.len().saturating_sub(self.measured_rows);
        self.measure_batch(remaining)
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
    /// alongside the pinned semantic header row and cell border vectors.
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
        let total_h = self.total_height();

        let visible_left = origin_x - self.scroll_x;
        let table_bounds = DisplayRect::new(origin_x, origin_y, self.constraints.container_width, total_h);

        // Header Row: pinned to top of visible table area or static at origin_y
        let header_y = if self.constraints.pinned_headers {
            let max_pin_y = (origin_y + total_h - header_h).max(origin_y);
            viewport_local_top.clamp(origin_y, max_pin_y)
        } else {
            origin_y
        };

        // Clip container for independent horizontal scrolling
        dl.push_item(DisplayItem::Clip(DisplayClip {
            bounds: table_bounds,
            child_count: 0,
        }));

        // Pinned header background/separator bar when scrolled past origin_y
        let is_pinned = header_y > origin_y;
        if is_pinned {
            dl.push_item(DisplayItem::Vector(DisplayVectorPath {
                bounds: DisplayRect::new(visible_left, header_y, total_w, header_h),
                shape: VectorShapeType::TableBorder,
                stroke_width: 1.0,
                color_role: "table-header-bg".to_string(),
                source_span: self.source_span,
            }));
        }

        let mut cur_x = visible_left;
        let mut header_cell_nodes = Vec::with_capacity(self.headers.len());
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

            header_cell_nodes.push(AccessibleReadingNode {
                role: AccessibleReadingRole::TableHeaderCell,
                text: format!("Header Col {i}: {}", cell.text),
                source_span: cell.source_span,
                bounds: cell_bounds,
                children: Vec::new(),
            });

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

        let header_row_node = AccessibleReadingNode {
            role: AccessibleReadingRole::TableHeaderRow,
            text: format!("Header row with {} columns", self.headers.len()),
            source_span: self.source_span,
            bounds: DisplayRect::new(visible_left, header_y, total_w, header_h),
            children: header_cell_nodes,
        };

        // Virtualized body rows
        let body_top = origin_y + header_h;
        let viewport_local_bottom = viewport_local_top + viewport_height;

        let visible_body_top = if self.constraints.pinned_headers && is_pinned {
            header_y + header_h
        } else {
            viewport_local_top.max(body_top)
        };

        let start_row = if visible_body_top <= body_top {
            0
        } else {
            let diff = visible_body_top - body_top;
            ((diff / row_h).floor() as usize).min(self.rows.len())
        };

        let end_row = if viewport_local_bottom <= body_top {
            start_row
        } else {
            let diff = viewport_local_bottom - body_top;
            (((diff / row_h).ceil() as usize) + 1).min(self.rows.len())
        };

        let mut visible_row_nodes = Vec::with_capacity(end_row.saturating_sub(start_row));

        for r in start_row..end_row {
            let row_y = body_top + (r as f32 * row_h);
            let row = &self.rows[r];

            let mut cell_x = visible_left;
            let mut row_cell_nodes = Vec::with_capacity(self.column_widths.len());

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

                    row_cell_nodes.push(AccessibleReadingNode {
                        role: AccessibleReadingRole::TableCell,
                        text: format!("Row {r}, Col {c}: {}", cell.text),
                        source_span: cell.source_span,
                        bounds: cell_bounds,
                        children: Vec::new(),
                    });
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

            visible_row_nodes.push(AccessibleReadingNode {
                role: AccessibleReadingRole::TableRow,
                text: format!("Row {r}"),
                source_span: self.source_span,
                bounds: DisplayRect::new(visible_left, row_y, total_w, row_h),
                children: row_cell_nodes,
            });
        }

        // Accessible reading node for the table hierarchy
        let mut table_children = Vec::with_capacity(1 + visible_row_nodes.len());
        table_children.push(header_row_node);
        table_children.extend(visible_row_nodes);

        dl.push_reading_node(AccessibleReadingNode {
            role: AccessibleReadingRole::Table,
            text: format!(
                "Table with {} columns and {} rows ({})",
                self.column_count(),
                self.row_count(),
                if self.is_fully_measured() { "fully measured" } else { "provisional estimates" }
            ),
            source_span: self.source_span,
            bounds: table_bounds,
            children: table_children,
        });

        Ok(dl)
    }

    /// Build the full accessible reading tree for the entire table.
    ///
    /// Unlike `materialize_viewport` which only attaches visible rows to the
    /// display list, this method produces the complete structural hierarchy
    /// for assistive technology and screen readers.
    #[must_use]
    pub fn full_accessible_tree(&self, origin: DisplayRect) -> AccessibleReadingNode {
        let header_h = Self::header_height();
        let row_h = Self::row_height();
        let total_w = self.total_table_width();

        let mut header_cells = Vec::with_capacity(self.headers.len());
        let mut cur_x = origin.x;
        for (i, cell) in self.headers.iter().enumerate() {
            let col_w = self.column_widths[i];
            header_cells.push(AccessibleReadingNode {
                role: AccessibleReadingRole::TableHeaderCell,
                text: format!("Header Col {i}: {}", cell.text),
                source_span: cell.source_span,
                bounds: DisplayRect::new(cur_x, origin.y, col_w, header_h),
                children: Vec::new(),
            });
            cur_x += col_w;
        }

        let header_row_node = AccessibleReadingNode {
            role: AccessibleReadingRole::TableHeaderRow,
            text: format!("Header row with {} columns", self.headers.len()),
            source_span: self.source_span,
            bounds: DisplayRect::new(origin.x, origin.y, total_w, header_h),
            children: header_cells,
        };

        let mut row_nodes = Vec::with_capacity(self.rows.len());
        let body_top = origin.y + header_h;
        for (r, row) in self.rows.iter().enumerate() {
            let row_y = body_top + (r as f32 * row_h);
            let mut cell_nodes = Vec::with_capacity(row.len());
            let mut cell_x = origin.x;
            for (c, col_w) in self.column_widths.iter().enumerate() {
                if let Some(cell) = row.get(c) {
                    cell_nodes.push(AccessibleReadingNode {
                        role: AccessibleReadingRole::TableCell,
                        text: format!("Row {r}, Col {c}: {}", cell.text),
                        source_span: cell.source_span,
                        bounds: DisplayRect::new(cell_x, row_y, *col_w, row_h),
                        children: Vec::new(),
                    });
                }
                cell_x += *col_w;
            }
            row_nodes.push(AccessibleReadingNode {
                role: AccessibleReadingRole::TableRow,
                text: format!("Row {r}"),
                source_span: self.source_span,
                bounds: DisplayRect::new(origin.x, row_y, total_w, row_h),
                children: cell_nodes,
            });
        }

        let mut children = Vec::with_capacity(1 + row_nodes.len());
        children.push(header_row_node);
        children.extend(row_nodes);

        AccessibleReadingNode {
            role: AccessibleReadingRole::Table,
            text: format!(
                "Table with {} columns and {} rows ({})",
                self.column_count(),
                self.row_count(),
                if self.is_fully_measured() { "fully measured" } else { "provisional estimates" }
            ),
            source_span: self.source_span,
            bounds: origin,
            children,
        }
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
            pinned_headers: true,
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
            pinned_headers: true,
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
    fn table_measurement_state_and_batch_refinement_stable_estimates() {
        let headers = vec![
            TableCell::new("ID", SourceSpan::default()),
            TableCell::new("Data", SourceSpan::default()),
        ];
        let alignments = vec![Align::Left, Align::Left];

        // 300 rows: rows 0..100 have short content, row 150 has a very wide cell
        let mut rows = Vec::new();
        for i in 0..300 {
            let data = if i == 150 {
                "Extraordinarily long data value occurring in row 150".to_string()
            } else {
                format!("val_{i:04}")
            };
            rows.push(vec![
                TableCell::new(format!("{i:04}"), SourceSpan::default()),
                TableCell::new(data, SourceSpan::default()),
            ]);
        }

        let mut table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            TableConstraints::default(),
        )
        .unwrap();

        // Initially: only first 100 rows measured (MAX_MEASURE_ROWS)
        assert_eq!(table.measured_rows_count(), 100);
        assert!(!table.is_fully_measured());
        assert_eq!(
            table.measurement_state(),
            TableMeasurementState::Provisional {
                measured_rows: 100,
                total_rows: 300,
            }
        );

        let initial_data_col_w = table.column_width(1).unwrap();

        // Stable estimates invariant: scrolling / materializing does NOT alter widths
        let _ = table.materialize_viewport(0.0, 0.0, 500.0, 200.0).unwrap();
        assert_eq!(table.column_width(1).unwrap(), initial_data_col_w);

        // Incremental batch measurement of next 100 rows (rows 100..200)
        let refinement = table.measure_batch(100).expect("batch measure");
        assert_eq!(refinement.measured_rows, 200);
        assert!(!refinement.is_complete);
        assert!(refinement.reflow_required);
        assert_eq!(refinement.column_expansions.len(), 1);
        assert_eq!(refinement.column_expansions[0].col_idx, 1);
        assert!(refinement.column_expansions[0].new_width > initial_data_col_w);

        let expanded_w = table.column_width(1).unwrap();
        assert_eq!(expanded_w, refinement.column_expansions[0].new_width);

        // Complete measurement of remaining rows
        let complete_refinement = table.complete_measurement().expect("complete measurement");
        assert!(complete_refinement.is_complete);
        assert_eq!(table.measured_rows_count(), 300);
        assert!(table.is_fully_measured());
        assert_eq!(
            table.measurement_state(),
            TableMeasurementState::Complete { total_rows: 300 }
        );
        // No further expansion happened in rows 200..300
        assert!(!complete_refinement.reflow_required);
        assert_eq!(table.column_width(1).unwrap(), expanded_w);
    }

    #[test]
    fn pinned_semantic_headers_remain_visible_when_scrolled() {
        let headers = vec![
            TableCell::new("HeaderA", SourceSpan::default()),
            TableCell::new("HeaderB", SourceSpan::default()),
        ];
        let alignments = vec![Align::Left, Align::Left];

        let mut rows = Vec::new();
        for i in 0..500 {
            rows.push(vec![
                TableCell::new(format!("A_{i}"), SourceSpan::default()),
                TableCell::new(format!("B_{i}"), SourceSpan::default()),
            ]);
        }

        let mut table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            TableConstraints::default(),
        )
        .unwrap();

        // 1. At scroll offset 0: header is at origin_y = 0.0
        let dl_top = table.materialize_viewport(0.0, 0.0, 0.0, 300.0).unwrap();
        let header_top = dl_top
            .items()
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text(t) if t.color_role == "table-header" => Some(t),
                _ => None,
            })
            .expect("header text run at top");
        assert_eq!(header_top.bounds.y, 0.0);

        // 2. Scrolled deep to 1,500pt: header is pinned to viewport top (1,500pt)
        let dl_scrolled = table.materialize_viewport(0.0, 0.0, 1500.0, 300.0).unwrap();
        let header_pinned = dl_scrolled
            .items()
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text(t) if t.color_role == "table-header" => Some(t),
                _ => None,
            })
            .expect("pinned header text run");
        assert_eq!(header_pinned.bounds.y, 1500.0);

        // Header separator vector also pinned at 1500 + 32 - 1 = 1531.0
        let header_border = dl_scrolled
            .items()
            .iter()
            .find_map(|item| match item {
                DisplayItem::Vector(v) if v.color_role == "table-border" => Some(v),
                _ => None,
            })
            .expect("header border");
        assert_eq!(header_border.bounds.y, 1531.0);

        // 3. When pinned_headers = false: header remains at origin_y
        table.constraints_mut().pinned_headers = false;
        let dl_unpinned = table.materialize_viewport(0.0, 0.0, 1500.0, 300.0).unwrap();
        let header_unpinned = dl_unpinned
            .items()
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text(t) if t.color_role == "table-header" => Some(t),
                _ => None,
            })
            .expect("unpinned header");
        assert_eq!(header_unpinned.bounds.y, 0.0);
    }

    #[test]
    fn accessible_table_hierarchy_and_semantic_roles() {
        let headers = vec![
            TableCell::new("Col1", SourceSpan::default()),
            TableCell::new("Col2", SourceSpan::default()),
        ];
        let alignments = vec![Align::Left, Align::Left];
        let rows = vec![
            vec![
                TableCell::new("Row0Col0", SourceSpan::default()),
                TableCell::new("Row0Col1", SourceSpan::default()),
            ],
            vec![
                TableCell::new("Row1Col0", SourceSpan::default()),
                TableCell::new("Row1Col1", SourceSpan::default()),
            ],
        ];

        let table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            TableConstraints::default(),
        )
        .unwrap();

        // Test full accessible tree
        let full_tree = table.full_accessible_tree(DisplayRect::new(0.0, 0.0, 400.0, 100.0));
        assert_eq!(full_tree.role, AccessibleReadingRole::Table);
        // Children: 1 header row + 2 body rows = 3
        assert_eq!(full_tree.children.len(), 3);

        // Header row
        let header_row = &full_tree.children[0];
        assert_eq!(header_row.role, AccessibleReadingRole::TableHeaderRow);
        assert_eq!(header_row.children.len(), 2);
        assert_eq!(header_row.children[0].role, AccessibleReadingRole::TableHeaderCell);
        assert!(header_row.children[0].text.contains("Col1"));

        // Body rows
        let body_row_0 = &full_tree.children[1];
        assert_eq!(body_row_0.role, AccessibleReadingRole::TableRow);
        assert_eq!(body_row_0.children.len(), 2);
        assert_eq!(body_row_0.children[0].role, AccessibleReadingRole::TableCell);
        assert!(body_row_0.children[0].text.contains("Row0Col0"));

        // Viewport accessible tree
        let dl = table.materialize_viewport(0.0, 0.0, 0.0, 200.0).unwrap();
        let reading_node = &dl.reading_order()[0];
        assert_eq!(reading_node.role, AccessibleReadingRole::Table);
        assert_eq!(reading_node.children[0].role, AccessibleReadingRole::TableHeaderRow);
    }

    #[test]
    fn task_list_rendering_read_only_contract_and_hit_testing() {
        use crate::block_flow::{BlockFlowEngine, FlowBlockItem, ListMarker, LogicalHeight};

        let items = vec![
            FlowBlockItem::ListItem {
                depth: 0,
                marker: ListMarker::Task { checked: true },
                text: "Completed verification task".to_string(),
                source_span: SourceSpan::new(0, 35),
            },
            FlowBlockItem::ListItem {
                depth: 0,
                marker: ListMarker::Task { checked: false },
                text: "Pending task".to_string(),
                source_span: SourceSpan::new(36, 60),
            },
        ];

        let mut engine = BlockFlowEngine::new();
        for item in items {
            engine.push_block(item, 800.0).expect("push block");
        }
        let dl = engine
            .materialize_viewport(0.0, LogicalHeight::from_points(0.0), 800.0, 400.0)
            .expect("materialize task list");

        // Read-only invariant 1: Absolutely zero interactive Anchor primitives emitted for checkboxes
        assert_eq!(
            dl.anchors().count(),
            0,
            "task lists must never produce interactive mutation anchors"
        );

        // Read-only invariant 2: Checkboxes are emitted strictly as Vector shapes
        let outline_count = dl
            .items()
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::CheckboxOutline,
                _ => false,
            })
            .count();
        let check_count = dl
            .items()
            .iter()
            .filter(|i| match i {
                DisplayItem::Vector(v) => v.shape == VectorShapeType::CheckboxCheck,
                _ => false,
            })
            .count();
        assert_eq!(outline_count, 2, "2 checkbox outlines");
        assert_eq!(check_count, 1, "1 checkmark for checked task");

        // Read-only invariant 3: Hit-testing at checkbox location returns Vector, NOT Anchor
        let hit = dl.hit_test(0.0, 6.0); // Inside first checkbox bounds (indent = 0)
        assert!(hit.is_some());
        assert!(!matches!(hit.unwrap(), DisplayItem::Anchor(_)));

        // Read-only invariant 4: Source span is preserved without mutations
        assert_eq!(dl.items()[0].source_span(), SourceSpan::new(0, 35));
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

    #[test]
    fn negative_control_measure_batch_when_already_complete() {
        let headers = vec![TableCell::new("C", SourceSpan::default())];
        let alignments = vec![Align::Left];
        let rows = vec![vec![TableCell::new("Val", SourceSpan::default())]];

        let mut table = ConstrainedTableFlow::try_new(
            alignments,
            headers,
            rows,
            SourceSpan::default(),
            TableConstraints::default(),
        )
        .unwrap();

        assert!(table.is_fully_measured());

        // Measuring a batch when already complete returns no expansions and reflow_required = false
        let refinement = table.measure_batch(50).unwrap();
        assert!(refinement.is_complete);
        assert!(!refinement.reflow_required);
        assert!(refinement.column_expansions.is_empty());
    }
}
