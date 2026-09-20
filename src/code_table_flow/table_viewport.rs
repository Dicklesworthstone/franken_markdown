//! Admission and clipped, bounded table presentation. No measurement changes
//! occur while painting: callers explicitly request width refinement.

use super::{
    Align, CodeTableError, ConstrainedTableFlow, TableCell, TableConstraints,
    MAX_COLUMNS_BUDGET,
};
use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayClip, DisplayItem, DisplayList,
    DisplayRect, DisplayTextRun, DisplayVectorPath, VectorShapeType,
};

const MAX_TABLE_ROWS: usize = 1_000_000;
const MAX_VIEWPORT_CELLS: usize = 16_384;
const MAX_CELL_TEXT_BYTES: usize = 4 * 1024 * 1024;
const FONT_SIZE: f32 = 13.0;
const CELL_ADVANCE: f32 = FONT_SIZE * 0.55;

impl TableConstraints {
    /// Reject non-finite, non-positive or reversed column bounds before any
    /// clamp or allocation. A zero-width container is an empty viewport.
    pub fn validate(&self) -> Result<(), CodeTableError> {
        if !self.min_col_width.is_finite()
            || !self.max_col_width.is_finite()
            || !self.container_width.is_finite()
            || self.min_col_width <= 0.0
            || self.max_col_width < self.min_col_width
            || self.container_width < 0.0
            || !(self.max_col_width * MAX_COLUMNS_BUDGET as f32).is_finite()
        {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        Ok(())
    }
}

pub(super) fn admit(
    alignments: &[Align],
    headers: &[TableCell],
    rows: &[Vec<TableCell>],
    constraints: TableConstraints,
) -> Result<usize, CodeTableError> {
    constraints.validate()?;
    if rows.len() > MAX_TABLE_ROWS {
        return Err(CodeTableError::RowBudgetExceeded { rows: rows.len(), max: MAX_TABLE_ROWS });
    }
    // Inspect only row metadata, not text outside the initial measure sample.
    // Ragged input must neither silently lose body columns nor bypass the cap.
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0)
        .max(headers.len()).max(alignments.len());
    if columns > MAX_COLUMNS_BUDGET {
        return Err(CodeTableError::ColumnBudgetExceeded { columns, max: MAX_COLUMNS_BUDGET });
    }
    Ok(columns)
}

/// Stop decoding once the column's width cap has been reached. This preserves
/// the historical scalar-cell estimate without scanning an entire huge cell.
pub(super) fn bounded_width(text: &str, constraints: TableConstraints) -> f32 {
    let limit = (((constraints.max_col_width - 16.0).max(0.0) / CELL_ADVANCE).ceil()
        as usize).saturating_add(1);
    let count = text.chars().take(limit).count();
    ((count as f32 * CELL_ADVANCE).max(20.0) + 16.0)
        .clamp(constraints.min_col_width, constraints.max_col_width)
}

impl ConstrainedTableFlow {
    /// Apply new constraints atomically, remeasuring the already-admitted
    /// sample/prefix without advancing measurement progress. Returns whether
    /// column geometry changed and the host should perform anchored reflow.
    pub fn set_constraints(&mut self, constraints: TableConstraints) -> Result<bool, CodeTableError> {
        constraints.validate()?;
        let mut widths = vec![constraints.min_col_width; self.column_count()];
        for row in std::iter::once(self.headers.as_slice())
            .chain(self.rows[..self.measured_rows].iter().map(Vec::as_slice))
        {
            for (column, cell) in row.iter().enumerate() {
                widths[column] = widths[column].max(bounded_width(&cell.text, constraints));
            }
        }
        let changed = widths != self.column_widths;
        self.column_widths = widths;
        self.constraints = constraints;
        Ok(changed)
    }

    /// Materialize one page of whole body rows with a repeated semantic header.
    ///
    /// `first_body_row` is a zero-based continuation cursor, not a pixel offset.
    /// The returned cursor is the first row not emitted. Stop when it equals
    /// [`Self::row_count`]; do not derive the next cursor from page height.
    /// Unlike a scrolling viewport, a page never hides rows beneath a pinned
    /// header or cuts a body row at the bottom edge. Headers repeat regardless
    /// of `pinned_headers`, which is a scrolling-only policy.
    ///
    /// `Ok(None)` means there is no drawable content, the cursor is already at
    /// the end of a nonempty table, or the available height cannot hold a header
    /// and at least one remaining body row. In the latter case retry the SAME
    /// cursor on a taller/fresh page; no content has been consumed. A header-only
    /// table may emit one header page and returns cursor zero (already complete).
    ///
    /// Widths and measurement progress are never changed. Call
    /// [`Self::complete_measurement`] before starting when all columns must use
    /// final rather than provisional widths, and do not refine between pages.
    /// Horizontal scrolling, alignment, per-cell clips and source provenance use
    /// the same renderer as [`Self::materialize_viewport`]. This fixed-height,
    /// plain-text table flow does not shape or wrap rich cell contents.
    ///
    /// At most 16,384 body-cell slots plus headers are materialized per call,
    /// even with an enormous page height. Copied text retains the 4 MiB budget.
    /// Invalid geometry or an out-of-range cursor returns
    /// [`CodeTableError::ArithmeticOverflow`]; budget failures return no partial
    /// page and do not change the caller's cursor or the table.
    pub fn materialize_page(
        &self,
        origin_x: f32,
        origin_y: f32,
        page_height: f32,
        first_body_row: usize,
    ) -> Result<Option<(DisplayList, usize)>, CodeTableError> {
        self.constraints.validate()?;
        let width = self.constraints.container_width;
        let table_width = self.total_table_width();
        let left = origin_x - self.scroll_x;
        let right = origin_x + width;
        let page_bottom = origin_y + page_height;
        if first_body_row > self.rows.len()
            || [origin_x, origin_y, page_height, self.scroll_x, table_width,
                left, right, left + table_width, page_bottom]
                .iter().any(|value| !value.is_finite())
            || page_height < 0.0 || self.scroll_x < 0.0
            || self.column_widths.iter().any(|width| !width.is_finite() || *width <= 0.0)
        {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        if width == 0.0 || table_width == 0.0 || page_height == 0.0
            || left + table_width <= origin_x
            || (!self.rows.is_empty() && first_body_row == self.rows.len())
        {
            return Ok(None);
        }
        if right <= origin_x || page_bottom <= origin_y {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        let header_height = Self::header_height();
        let row_height = Self::row_height();
        if page_height < header_height {
            return Ok(None);
        }
        // Use f64 for capacity arithmetic so an almost-full final row cannot
        // round up to a whole row. Bound metadata before allocating any output.
        let capacity = ((f64::from(page_height) - f64::from(header_height))
            / f64::from(row_height)).floor() as usize;
        let count = capacity.min(self.rows.len() - first_body_row)
            .min(MAX_VIEWPORT_CELLS / self.column_count().max(1));
        if count == 0 && first_body_row < self.rows.len() {
            return Ok(None);
        }
        let next_row = first_body_row + count;
        let height = header_height + count as f32 * row_height;
        let bottom = origin_y + height;
        let body_top = origin_y + header_height;
        if !bottom.is_finite() || bottom > page_bottom || body_top <= origin_y {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        let clip = DisplayRect::new(origin_x, origin_y, width, height);
        let header_bounds = DisplayRect::new(left, origin_y, table_width, header_height);
        let mut items = vec![border(self, header_bounds, "table-header-bg", 1.0)];
        let mut copied_bytes = 0;
        let header_cells = self.emit_cells(&self.headers, None, header_bounds, clip,
            &mut items, &mut copied_bytes)?;
        items.push(border(self,
            DisplayRect::new(left, body_top - 1.0, table_width, 1.0),
            "table-border", 1.0));
        let mut children = vec![AccessibleReadingNode {
            role: AccessibleReadingRole::TableHeaderRow,
            text: format!("Header row with {} columns", self.headers.len()),
            source_span: self.source_span,
            bounds: header_bounds,
            children: header_cells,
        }];
        for (offset, index) in (first_body_row..next_row).enumerate() {
            let row_y = body_top + offset as f32 * row_height;
            let row_bottom = row_y + row_height;
            if !row_bottom.is_finite() || row_bottom <= row_y || row_bottom > bottom {
                return Err(CodeTableError::ArithmeticOverflow);
            }
            let bounds = DisplayRect::new(left, row_y, table_width, row_height);
            let cells = self.emit_cells(&self.rows[index], Some(index), bounds, clip,
                &mut items, &mut copied_bytes)?;
            items.push(border(self,
                DisplayRect::new(left, row_bottom - 0.5, table_width, 0.5),
                "table-border", 0.5));
            children.push(AccessibleReadingNode {
                role: AccessibleReadingRole::TableRow,
                text: format!("Row {index}"),
                source_span: self.source_span,
                bounds,
                children: cells,
            });
        }
        let mut output = DisplayList::new();
        output.push_item(DisplayItem::Clip(DisplayClip { bounds: clip, child_count: items.len() }));
        for item in items { output.push_item(item); }
        output.push_reading_node(AccessibleReadingNode {
            role: AccessibleReadingRole::Table,
            text: format!("Table with {} columns and {} rows; body rows {}..{} ({})",
                self.column_count(), self.row_count(), first_body_row, next_row,
                if self.is_fully_measured() { "fully measured" } else { "provisional estimates" }),
            source_span: self.source_span,
            bounds: clip,
            children,
        });
        Ok(Some((output, next_row)))
    }

    pub(super) fn materialize_table_viewport(
        &self,
        origin_x: f32,
        origin_y: f32,
        viewport_top: f32,
        viewport_height: f32,
    ) -> Result<DisplayList, CodeTableError> {
        self.constraints.validate()?;
        let width = self.constraints.container_width;
        let table_width = self.total_table_width();
        let table_bottom = origin_y + self.total_height();
        let viewport_bottom = viewport_top + viewport_height;
        let right = origin_x + width;
        let left = origin_x - self.scroll_x;
        if [origin_x, origin_y, viewport_top, viewport_height, self.scroll_x,
            table_width, table_bottom, viewport_bottom, right, left, left + table_width]
            .iter().any(|value| !value.is_finite())
            || viewport_height < 0.0 || self.scroll_x < 0.0
            || self.column_widths.iter().any(|width| !width.is_finite() || *width <= 0.0)
        {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        let top = origin_y.max(viewport_top);
        let bottom = table_bottom.min(viewport_bottom);
        let mut output = DisplayList::new();
        if width == 0.0 || top >= bottom || table_width == 0.0 || left + table_width <= origin_x {
            return Ok(output);
        }
        let clip = DisplayRect::new(origin_x, top, width, bottom - top);
        let header_height = Self::header_height();
        let header_y = if self.constraints.pinned_headers {
            viewport_top.clamp(origin_y, (table_bottom - header_height).max(origin_y))
        } else { origin_y };
        let pinned = header_y > origin_y;
        let body_origin = origin_y + header_height;
        let body_top = if self.constraints.pinned_headers {
            top.max(header_y + header_height).max(body_origin)
        } else { top.max(body_origin) };
        let start = (((body_top - body_origin).max(0.0) / Self::row_height()).floor() as usize)
            .min(self.rows.len());
        let end = if body_top >= bottom { start } else {
            (((bottom - body_origin).max(0.0) / Self::row_height()).ceil() as usize)
                .min(self.rows.len())
        };
        // Also bounds metadata for empty cells; byte limits alone do not do so.
        let max_rows = MAX_VIEWPORT_CELLS / self.column_count().max(1);
        if end.saturating_sub(start) > max_rows {
            return Err(CodeTableError::RowBudgetExceeded { rows: end - start, max: max_rows });
        }

        let mut header_items = Vec::new();
        let mut body_items = Vec::new();
        let mut children = Vec::new();
        let mut copied_bytes = 0usize;
        if pinned {
            header_items.push(border(self, DisplayRect::new(left, header_y, table_width, header_height),
                "table-header-bg", 1.0));
        }
        let header_bounds = DisplayRect::new(left, header_y, table_width, header_height);
        let header_cells = self.emit_cells(&self.headers, None, header_bounds, clip,
            &mut header_items, &mut copied_bytes)?;
        header_items.push(border(self,
            DisplayRect::new(left, header_y + header_height - 1.0, table_width, 1.0),
            "table-border", 1.0));
        // Static headers remain in the semantic structure and retain their
        // historical coordinates; the viewport clip prevents offscreen paint.
        children.push(AccessibleReadingNode {
            role: AccessibleReadingRole::TableHeaderRow,
            text: format!("Header row with {} columns", self.headers.len()),
            source_span: self.source_span,
            bounds: header_bounds,
            children: header_cells,
        });
        for index in start..end {
            let row_y = body_origin + index as f32 * Self::row_height();
            let row_bounds = DisplayRect::new(left, row_y, table_width, Self::row_height());
            let cells = self.emit_cells(&self.rows[index], Some(index), row_bounds, clip,
                &mut body_items, &mut copied_bytes)?;
            body_items.push(border(self,
                DisplayRect::new(left, row_y + Self::row_height() - 0.5, table_width, 0.5),
                "table-border", 0.5));
            children.push(AccessibleReadingNode {
                role: AccessibleReadingRole::TableRow,
                text: format!("Row {index}"),
                source_span: self.source_span,
                bounds: row_bounds,
                children: cells,
            });
        }
        let has_body = !body_items.is_empty();
        output.push_item(DisplayItem::Clip(DisplayClip {
            bounds: clip,
            child_count: header_items.len() + body_items.len() + usize::from(has_body),
        }));
        for item in header_items { output.push_item(item); }
        if has_body {
            output.push_item(DisplayItem::Clip(DisplayClip {
                bounds: DisplayRect::new(origin_x, body_top, width, bottom - body_top),
                child_count: body_items.len(),
            }));
            for item in body_items { output.push_item(item); }
        }
        output.push_reading_node(AccessibleReadingNode {
            role: AccessibleReadingRole::Table,
            text: format!("Table with {} columns and {} rows ({})", self.column_count(),
                self.row_count(), if self.is_fully_measured() { "fully measured" } else { "provisional estimates" }),
            source_span: self.source_span,
            bounds: clip,
            children,
        });
        Ok(output)
    }

    fn emit_cells(
        &self,
        cells: &[TableCell],
        row: Option<usize>,
        row_bounds: DisplayRect,
        viewport: DisplayRect,
        items: &mut Vec<DisplayItem>,
        copied_bytes: &mut usize,
    ) -> Result<Vec<AccessibleReadingNode>, CodeTableError> {
        let mut x = row_bounds.x;
        let mut nodes = Vec::new();
        for (column, width) in self.column_widths.iter().copied().enumerate() {
            let bounds = DisplayRect::new(x, row_bounds.y, width, row_bounds.height);
            x += width;
            if bounds.right() <= viewport.x || bounds.x >= viewport.right() { continue; }
            let Some(cell) = cells.get(column) else { continue; };
            let label = match row {
                Some(row) => format!("Row {row}, Col {column}: "),
                None => format!("Header Col {column}: "),
            };
            let next = copied_bytes.saturating_add(cell.text.len().saturating_mul(2))
                .saturating_add(label.len());
            if next > MAX_CELL_TEXT_BYTES {
                return Err(CodeTableError::OutputBudgetExceeded { bytes: next, max: MAX_CELL_TEXT_BYTES });
            }
            *copied_bytes = next;
            let offset = match self.alignments.get(column).copied().unwrap_or(Align::None) {
                Align::Center | Align::Right => {
                    let count = cell.text.chars().take((width / CELL_ADVANCE).ceil() as usize).count();
                    let free = (width - count as f32 * CELL_ADVANCE).max(0.0);
                    if self.alignments.get(column) == Some(&Align::Center) { free / 2.0 } else { free }
                }
                Align::None | Align::Left => 0.0,
            };
            // Per-cell clipping prevents long text painting into its neighbor.
            items.push(DisplayItem::Clip(DisplayClip { bounds, child_count: 1 }));
            items.push(DisplayItem::Text(DisplayTextRun {
                bounds: DisplayRect::new(bounds.x + offset, bounds.y, width - offset, bounds.height),
                text: cell.text.clone(),
                font_run: None,
                color_role: (if row.is_some() { "table-cell" } else { "table-header" }).to_string(),
                source_span: cell.source_span,
                font_size: FONT_SIZE,
            }));
            nodes.push(AccessibleReadingNode {
                role: if row.is_some() { AccessibleReadingRole::TableCell } else { AccessibleReadingRole::TableHeaderCell },
                text: label + &cell.text,
                source_span: cell.source_span,
                bounds,
                children: Vec::new(),
            });
        }
        Ok(nodes)
    }
}

fn border(table: &ConstrainedTableFlow, bounds: DisplayRect, role: &str, stroke_width: f32) -> DisplayItem {
    DisplayItem::Vector(DisplayVectorPath {
        bounds,
        shape: VectorShapeType::TableBorder,
        stroke_width,
        color_role: role.to_string(),
        source_span: table.source_span,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::span::SourceSpan;

    fn cell(text: &str) -> TableCell { TableCell::new(text, SourceSpan::new(4, 9)) }
    fn table(rows: usize) -> ConstrainedTableFlow {
        ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("Header")],
            vec![vec![cell("Body")]; rows], SourceSpan::default(), TableConstraints::default()).unwrap()
    }

    #[test]
    fn ragged_body_columns_are_never_lost_or_allowed_to_bypass_budget() {
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("Header")],
            vec![vec![cell("A"), cell("B"), cell("C")]], SourceSpan::default(), TableConstraints::default()).unwrap();
        assert_eq!(flow.column_count(), 3);
        let dl = flow.materialize_viewport(0.0, 0.0, 0.0, 100.0).unwrap();
        assert!(dl.items().iter().any(|item| matches!(item, DisplayItem::Text(t) if t.text == "C")));
        assert_eq!(flow.full_accessible_tree(DisplayRect::default()).children[1].children.len(), 3);
        let mut rows = vec![vec![cell("A")]; 150];
        rows.push(vec![cell("X"); MAX_COLUMNS_BUDGET + 1]);
        assert!(matches!(ConstrainedTableFlow::try_new(vec![], vec![], rows,
            SourceSpan::default(), TableConstraints::default()), Err(CodeTableError::ColumnBudgetExceeded { .. })));
    }

    #[test]
    fn invalid_constraints_are_errors_even_without_cells_and_after_mutation() {
        for (min, max, width) in [(f32::NAN, 400.0, 800.0), (1.0, f32::INFINITY, 800.0),
            (60.0, 20.0, 800.0), (0.0, 400.0, 800.0), (1.0, 400.0, -1.0)] {
            let constraints = TableConstraints { min_col_width: min, max_col_width: max,
                container_width: width, pinned_headers: true };
            assert!(ConstrainedTableFlow::try_new(vec![], vec![], vec![], SourceSpan::default(), constraints).is_err());
        }
        let mut flow = table(101);
        let width = flow.column_width(0).unwrap();
        flow.constraints_mut().max_col_width = f32::NAN;
        assert!(flow.measure_batch(usize::MAX).is_err());
        assert!(flow.handle_late_wide_cell(0, "wide").is_err());
        assert!(flow.materialize_viewport(0.0, 0.0, 0.0, 100.0).is_err());
        assert_eq!(flow.measured_rows_count(), 100);
        assert_eq!(flow.column_width(0).unwrap(), width);
    }

    #[test]
    fn maximum_batch_size_saturates_instead_of_overflowing() {
        let mut flow = table(150);
        let result = flow.measure_batch(usize::MAX).unwrap();
        assert_eq!(result.measured_rows, 150);
        assert!(result.is_complete);
        assert!(flow.measure_batch(usize::MAX).unwrap().is_complete);
    }

    #[test]
    fn empty_offscreen_and_invalid_viewports_are_explicit() {
        let mut flow = table(5);
        for (top, height) in [(0.0, 50.0), (1000.0, 100.0), (120.0, 0.0)] {
            let dl = flow.materialize_viewport(20.0, 100.0, top, height).unwrap();
            assert!(dl.items().is_empty());
            assert!(dl.reading_order().is_empty());
        }
        for top in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(flow.materialize_viewport(0.0, 0.0, top, 100.0).is_err());
        }
        assert!(flow.materialize_viewport(0.0, 0.0, 0.0, -1.0).is_err());
        flow.constraints_mut().container_width = 0.0;
        assert!(flow.materialize_viewport(0.0, 0.0, 0.0, 100.0).unwrap().items().is_empty());
    }

    #[test]
    fn clips_bound_every_item_and_keep_body_out_of_pinned_header() {
        let flow = table(100);
        let dl = flow.materialize_viewport(10.0, 0.0, 501.0, 100.0).unwrap();
        let DisplayItem::Clip(root) = &dl.items()[0] else { panic!("root clip") };
        assert_eq!(root.child_count, dl.items().len() - 1);
        assert_eq!(root.bounds, DisplayRect::new(10.0, 501.0, 800.0, 100.0));
        assert!(dl.items().iter().any(|item| matches!(item, DisplayItem::Clip(c)
            if c.bounds.y == 533.0 && c.bounds.height == 68.0 && c.child_count > 1)));
        for (index, item) in dl.items().iter().enumerate() {
            if let DisplayItem::Clip(clip) = item {
                assert!(clip.child_count <= dl.items().len() - index - 1);
            }
        }
        assert_eq!(flow.measured_rows_count(), 100);
    }

    #[test]
    fn horizontal_culling_and_alignment_preserve_logical_cell_bounds() {
        let constraints = TableConstraints { min_col_width: 100.0, max_col_width: 100.0,
            container_width: 200.0, pinned_headers: true };
        let mut flow = ConstrainedTableFlow::try_new(vec![Align::Left, Align::Center, Align::Right],
            vec![cell("L"), cell("C"), cell("R")], vec![], SourceSpan::default(), constraints).unwrap();
        flow.scroll_x = 100.0;
        let dl = flow.materialize_viewport(0.0, 0.0, 0.0, 100.0).unwrap();
        let runs: Vec<_> = dl.items().iter().filter_map(|item|
            if let DisplayItem::Text(run) = item { Some(run) } else { None }).collect();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "C");
        assert!((runs[0].bounds.x - (100.0 - CELL_ADVANCE) / 2.0).abs() < 0.001);
        assert!((runs[1].bounds.x - (200.0 - CELL_ADVANCE)).abs() < 0.001);
        let header = &dl.reading_order()[0].children[0];
        assert_eq!(header.children[0].bounds.x, 0.0);
        assert!(header.children[0].text.starts_with("Header Col 1:"));
    }

    #[test]
    fn text_and_metadata_budgets_fail_before_unbounded_output() {
        let huge = "x".repeat(MAX_CELL_TEXT_BYTES);
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell(&huge)],
            vec![], SourceSpan::default(), TableConstraints::default()).unwrap();
        assert!(matches!(flow.materialize_viewport(0.0, 0.0, 0.0, 100.0),
            Err(CodeTableError::OutputBudgetExceeded { .. })));
        let flow = table(MAX_VIEWPORT_CELLS + 1);
        assert!(matches!(flow.materialize_viewport(0.0, 0.0, 0.0, flow.total_height()),
            Err(CodeTableError::RowBudgetExceeded { .. })));
    }

    #[test]
    fn capped_measurement_matches_reference_width() {
        let constraints = TableConstraints::default();
        for text in ["", "é中🙂", "small cell", &"x".repeat(100_000)] {
            let expected = cell(text).intrinsic_width(FONT_SIZE)
                .clamp(constraints.min_col_width, constraints.max_col_width);
            assert_eq!(bounded_width(text, constraints), expected);
        }
    }

    #[test]
    fn constraint_updates_are_atomic_and_do_not_advance_measurement() {
        let mut flow = table(150);
        let original = *flow.constraints();
        let old_width = flow.column_width(0).unwrap();
        let invalid = TableConstraints { min_col_width: 500.0, ..original };
        assert!(flow.set_constraints(invalid).is_err());
        assert_eq!(*flow.constraints(), original);
        assert_eq!(flow.column_width(0).unwrap(), old_width);
        let narrow = TableConstraints { min_col_width: 20.0, max_col_width: 30.0, ..original };
        assert!(flow.set_constraints(narrow).unwrap());
        assert_eq!(flow.column_width(0).unwrap(), 30.0);
        assert_eq!(flow.measured_rows_count(), 100);
        assert!(!flow.set_constraints(narrow).unwrap());
        assert!(flow.set_constraints(original).unwrap());
        assert_eq!(flow.column_width(0).unwrap(), old_width);
    }

    #[test]
    fn pages_repeat_headers_without_losing_rows_or_changing_measurement() {
        let rows = (0..107).map(|index| vec![cell(&format!("Body {index}"))]).collect();
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("Header")],
            rows, SourceSpan::new(4, 9), TableConstraints::default()).unwrap();
        let width = flow.column_width(0).unwrap();
        let mut cursor = 0;
        let mut seen = Vec::new();
        // The spare 27 points are not enough for a third 28-point row.
        let page_height = 32.0 + 2.0 * 28.0 + 27.0;
        while cursor < flow.row_count() {
            let (page, next) = flow.materialize_page(10.0, 75.0, page_height, cursor)
                .unwrap().unwrap();
            assert_eq!(next, (cursor + 2).min(flow.row_count()));
            let root = &page.reading_order()[0];
            assert_eq!(root.children[0].role, AccessibleReadingRole::TableHeaderRow);
            assert_eq!(root.children[0].bounds.y, 75.0);
            assert_eq!(root.children[0].children[0].bounds.width, width);
            assert_eq!(root.children[0].children[0].source_span, SourceSpan::new(4, 9));
            for row in &root.children[1..] {
                assert!(row.bounds.y >= 107.0);
                assert!(row.bounds.y + row.bounds.height <= 75.0 + page_height);
                assert_eq!(row.children[0].bounds.width, width);
            }
            for item in page.items() {
                if let DisplayItem::Text(run) = item {
                    if run.color_role == "table-cell" { seen.push(run.text.clone()); }
                }
            }
            let DisplayItem::Clip(clip) = &page.items()[0] else { panic!("root clip") };
            assert_eq!(clip.child_count, page.items().len() - 1);
            cursor = next;
        }
        assert_eq!(seen, (0..107).map(|index| format!("Body {index}")).collect::<Vec<_>>());
        assert_eq!(flow.column_width(0).unwrap(), width);
        assert_eq!(flow.measured_rows_count(), 100);
        assert!(flow.materialize_page(10.0, 75.0, page_height, cursor).unwrap().is_none());
    }

    #[test]
    fn short_pages_do_not_orphan_headers_or_consume_the_next_row() {
        let flow = table(3);
        for height in [0.0, 31.0, 32.0, 59.0, f32::from_bits(60.0_f32.to_bits() - 1)] {
            assert!(flow.materialize_page(0.0, 0.0, height, 1).unwrap().is_none());
        }
        let (page, next) = flow.materialize_page(0.0, 0.0, 60.0, 1).unwrap().unwrap();
        assert_eq!(next, 2);
        assert_eq!(page.reading_order()[0].children[1].text, "Row 1");
        assert_eq!(page.reading_order()[0].bounds.height, 60.0);
        assert_eq!(flow.row_count(), 3);
    }

    #[test]
    fn header_only_tables_and_disabled_pinning_have_explicit_page_semantics() {
        let empty = table(0);
        assert!(empty.materialize_page(0.0, 0.0, 31.0, 0).unwrap().is_none());
        let (page, next) = empty.materialize_page(0.0, 0.0, 32.0, 0).unwrap().unwrap();
        assert_eq!(next, 0);
        assert_eq!(page.reading_order()[0].children.len(), 1);
        let mut flow = table(3);
        flow.constraints_mut().pinned_headers = false;
        let (page, next) = flow.materialize_page(0.0, 500.0, 60.0, 2).unwrap().unwrap();
        assert_eq!(next, 3);
        assert_eq!(page.reading_order()[0].children[0].bounds.y, 500.0);
        assert_eq!(page.reading_order()[0].children[1].text, "Row 2");
    }

    #[test]
    fn pagination_rejects_invalid_geometry_and_invalid_cursors() {
        let flow = table(2);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(flow.materialize_page(bad, 0.0, 60.0, 0).is_err());
            assert!(flow.materialize_page(0.0, bad, 60.0, 0).is_err());
            assert!(flow.materialize_page(0.0, 0.0, bad, 0).is_err());
        }
        assert!(flow.materialize_page(0.0, 0.0, -1.0, 0).is_err());
        assert!(flow.materialize_page(0.0, 0.0, 60.0, 3).is_err());
        assert!(flow.materialize_page(0.0, f32::MAX, f32::MAX, 0).is_err());
        // Finite coordinates can still be too coarse to represent a row.
        assert!(flow.materialize_page(0.0, 1.0e30, 60.0, 0).is_err());
    }

    #[test]
    fn enormous_pages_still_materialize_a_bounded_prefix() {
        let flow = table(MAX_VIEWPORT_CELLS + 1);
        let (page, next) = flow.materialize_page(0.0, 0.0, 1.0e9, 0).unwrap().unwrap();
        assert_eq!(next, MAX_VIEWPORT_CELLS);
        assert_eq!(page.reading_order()[0].children.len(), MAX_VIEWPORT_CELLS + 1);
        let (_, end) = flow.materialize_page(0.0, 0.0, 60.0, next).unwrap().unwrap();
        assert_eq!(end, flow.row_count());
    }

    #[test]
    fn page_text_budget_errors_leave_the_table_and_cursor_reusable() {
        let huge = "x".repeat(MAX_CELL_TEXT_BYTES);
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("Header")],
            vec![vec![cell("Small")], vec![cell(&huge)]], SourceSpan::default(),
            TableConstraints::default()).unwrap();
        assert!(matches!(flow.materialize_page(0.0, 0.0, 88.0, 0),
            Err(CodeTableError::OutputBudgetExceeded { .. })));
        let (page, next) = flow.materialize_page(0.0, 0.0, 60.0, 0).unwrap().unwrap();
        assert_eq!(next, 1);
        assert!(page.items().iter().any(|item| matches!(item, DisplayItem::Text(run) if run.text == "Small")));
        assert_eq!(flow.row_count(), 2);
    }

}
