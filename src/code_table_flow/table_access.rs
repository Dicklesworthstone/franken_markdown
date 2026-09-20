//! Source-preserving access to virtualized tables. None of these operations
//! change column measurement, scroll state, or the stored cell contents.

use std::ops::Range;

use super::{
    AccessibleReadingNode, AccessibleReadingRole, CodeTableError, ConstrainedTableFlow,
    DisplayRect, MAX_CELL_TEXT_BYTES, MAX_VIEWPORT_CELLS, TableCell,
};

struct TextBudget {
    used: usize,
    max: usize,
}

impl TextBudget {
    fn new(requested: usize) -> Self {
        Self { used: 0, max: requested.min(MAX_CELL_TEXT_BYTES) }
    }

    fn check(&self, additional: usize) -> Result<usize, CodeTableError> {
        let bytes = self.used.saturating_add(additional);
        if additional > self.max.saturating_sub(self.used) {
            return Err(CodeTableError::OutputBudgetExceeded { bytes, max: self.max });
        }
        Ok(bytes)
    }

    fn claim(&mut self, additional: usize) -> Result<(), CodeTableError> {
        self.used = self.check(additional)?;
        Ok(())
    }
}

impl ConstrainedTableFlow {
    /// Borrow a complete source cell without cloning or materializing a tree.
    ///
    /// `None` selects the header; `Some(index)` selects a zero-based body row.
    /// Missing ragged cells and out-of-range indices return `None`. This remains
    /// available even when drawing or copying the cell exceeds an output budget.
    #[must_use]
    pub fn cell(&self, body_row: Option<usize>, column: usize) -> Option<&TableCell> {
        let cells = match body_row {
            Some(row) => self.rows.get(row)?.as_slice(),
            None => self.headers.as_slice(),
        };
        cells.get(column)
    }

    /// Copy a rectangular selection as quoted, tab-separated source text.
    ///
    /// Both ranges are zero-based and half-open; `body_rows` excludes the header.
    /// An optional header precedes the selected body rows. Ragged missing cells
    /// become empty fields, preserving the selected rectangle. Empty column
    /// ranges produce an empty string. Rows use LF separators with no final LF.
    /// A single empty field is quoted so an empty row is not lost on paste.
    /// Fields containing tabs, CR, LF, or quotes are quoted; embedded quotes are
    /// doubled. Unicode, original cell line endings, and formula-like strings are
    /// preserved, not normalized or spreadsheet-sanitized.
    ///
    /// The output limit is `min(max_bytes, 4 MiB)`, including separators and
    /// quoting. Oversized cells are rejected by byte length before scanning their
    /// contents. Errors return no partial clipboard text and never change the
    /// table. Invalid row/reversed ranges return `ArithmeticOverflow`; columns
    /// beyond the table return `InvalidColumnIndex`. Copy is independent of
    /// viewport geometry, horizontal scrolling, and measurement progress.
    pub fn copy_tsv(
        &self,
        body_rows: Range<usize>,
        columns: Range<usize>,
        include_header: bool,
        max_bytes: usize,
    ) -> Result<String, CodeTableError> {
        self.validate_body_range(&body_rows)?;
        if columns.start > columns.end {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        if columns.end > self.column_count() {
            return Err(CodeTableError::InvalidColumnIndex {
                index: columns.start.max(columns.end.saturating_sub(1)),
                len: self.column_count(),
            });
        }
        if columns.is_empty() {
            return Ok(String::new());
        }
        let mut budget = TextBudget::new(max_bytes);
        let mut output = String::new();
        let headers = include_header.then_some(self.headers.as_slice());
        let rows = headers.into_iter().chain(
            self.rows[body_rows].iter().map(Vec::as_slice),
        );
        for (row_number, row) in rows.enumerate() {
            if row_number != 0 {
                budget.claim(1)?;
                output.push('\n');
            }
            for column in columns.clone() {
                if column != columns.start {
                    budget.claim(1)?;
                    output.push('\t');
                }
                let text = row.get(column).map_or("", |cell| cell.text.as_str());
                if columns.end - columns.start == 1 && text.is_empty() {
                    budget.claim(2)?;
                    output.push_str("\"\"");
                } else {
                    push_tsv_field(&mut output, text, &mut budget)?;
                }
            }
        }
        Ok(output)
    }

    /// Materialize complete semantic text for only the requested body rows.
    ///
    /// This is the bounded alternative to `full_accessible_tree`: it neither
    /// truncates cell text to the drawing width nor visits unselected rows.
    /// `body_rows` is a zero-based, half-open range. The optional header and each
    /// body row retain their original table coordinates, row/column labels, and
    /// source spans; scrolling and pinned-header painting do not move them.
    /// The root retains the full table's description and caller-supplied bounds.
    ///
    /// At most 16,384 row/column slots (including the optional header) and
    /// `min(max_bytes, 4 MiB)` UTF-8 text bytes are admitted. Text accounting
    /// includes table, row and cell labels. Even zero-column rows consume a
    /// metadata slot. Over-budget calls fail without returning a partial tree;
    /// use smaller ranges or `cell` for allocation-free access to a huge cell.
    /// Invalid ranges or non-finite/unrepresentable geometry return
    /// `ArithmeticOverflow`. Width refinement is always explicit, never a side
    /// effect of accessibility traversal.
    pub fn accessible_row_range(
        &self,
        origin: DisplayRect,
        body_rows: Range<usize>,
        include_header: bool,
        max_bytes: usize,
    ) -> Result<AccessibleReadingNode, CodeTableError> {
        self.validate_body_range(&body_rows)?;
        self.constraints.validate()?;
        let max_rows = (MAX_VIEWPORT_CELLS / self.column_count().max(1))
            .saturating_sub(usize::from(include_header));
        let count = body_rows.end - body_rows.start;
        if count > max_rows {
            return Err(CodeTableError::RowBudgetExceeded { rows: count, max: max_rows });
        }
        let width = self.total_table_width();
        if [origin.x, origin.y, origin.width, origin.height, origin.right(),
            origin.bottom(), width, origin.x + width]
            .iter().any(|value| !value.is_finite())
            || origin.width < 0.0 || origin.height < 0.0
            || self.column_widths.iter().any(|width| !width.is_finite() || *width <= 0.0)
        {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        let text = format!(
            "Table with {} columns and {} rows ({})",
            self.column_count(), self.row_count(),
            if self.is_fully_measured() { "fully measured" } else { "provisional estimates" },
        );
        let mut budget = TextBudget::new(max_bytes);
        budget.claim(text.len())?;
        let mut children = Vec::with_capacity(count + usize::from(include_header));
        if include_header {
            children.push(self.accessible_source_row(
                &self.headers, None,
                DisplayRect::new(origin.x, origin.y, width, Self::header_height()),
                &mut budget,
            )?);
        }
        let body_top = origin.y + Self::header_height();
        for index in body_rows {
            let row_y = body_top + index as f32 * Self::row_height();
            children.push(self.accessible_source_row(
                &self.rows[index], Some(index),
                DisplayRect::new(origin.x, row_y, width, Self::row_height()),
                &mut budget,
            )?);
        }
        Ok(AccessibleReadingNode {
            role: AccessibleReadingRole::Table,
            text,
            source_span: self.source_span,
            bounds: origin,
            children,
        })
    }

    fn validate_body_range(&self, rows: &Range<usize>) -> Result<(), CodeTableError> {
        if rows.start > rows.end || rows.end > self.row_count() {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        Ok(())
    }

    fn accessible_source_row(
        &self,
        cells: &[TableCell],
        row: Option<usize>,
        bounds: DisplayRect,
        budget: &mut TextBudget,
    ) -> Result<AccessibleReadingNode, CodeTableError> {
        if !bounds.y.is_finite() || !bounds.bottom().is_finite() || bounds.bottom() <= bounds.y {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        let text = match row {
            Some(index) => format!("Row {index}"),
            None => format!("Header row with {} columns", cells.len()),
        };
        budget.claim(text.len())?;
        let mut children = Vec::with_capacity(cells.len());
        let mut x = bounds.x;
        for (column, width) in self.column_widths.iter().copied().enumerate() {
            let cell_bounds = DisplayRect::new(x, bounds.y, width, bounds.height);
            x += width;
            if !x.is_finite() || x <= cell_bounds.x {
                return Err(CodeTableError::ArithmeticOverflow);
            }
            let Some(cell) = cells.get(column) else { continue; };
            let mut label = match row {
                Some(index) => format!("Row {index}, Col {column}: "),
                None => format!("Header Col {column}: "),
            };
            budget.claim(label.len().saturating_add(cell.text.len()))?;
            label.push_str(&cell.text);
            children.push(AccessibleReadingNode {
                role: if row.is_some() {
                    AccessibleReadingRole::TableCell
                } else {
                    AccessibleReadingRole::TableHeaderCell
                },
                text: label,
                source_span: cell.source_span,
                bounds: cell_bounds,
                children: Vec::new(),
            });
        }
        Ok(AccessibleReadingNode {
            role: if row.is_some() {
                AccessibleReadingRole::TableRow
            } else {
                AccessibleReadingRole::TableHeaderRow
            },
            text,
            source_span: self.source_span,
            bounds,
            children,
        })
    }
}

fn push_tsv_field(
    output: &mut String,
    text: &str,
    budget: &mut TextBudget,
) -> Result<(), CodeTableError> {
    // Reject a huge source by its O(1) byte length before inspecting delimiters.
    budget.check(text.len())?;
    let quoted = text.bytes().any(|byte| matches!(byte, b'\t' | b'\r' | b'\n' | b'"'));
    if !quoted {
        budget.claim(text.len())?;
        output.push_str(text);
        return Ok(());
    }
    let quotes = text.bytes().filter(|byte| *byte == b'"').count();
    budget.claim(text.len().saturating_add(quotes).saturating_add(2))?;
    output.push('"');
    for (index, part) in text.split('"').enumerate() {
        if index != 0 { output.push_str("\"\""); }
        output.push_str(part);
    }
    output.push('"');
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::ast::Align;
    use crate::code_table_flow::TableConstraints;
    use crate::span::SourceSpan;

    fn cell(text: &str) -> TableCell { TableCell::new(text, SourceSpan::new(4, 9)) }

    fn sample() -> ConstrainedTableFlow {
        ConstrainedTableFlow::try_new(
            vec![Align::Left; 3], vec![cell("H1"), cell("H2")],
            vec![vec![cell("é"), cell("a\tb"), cell("say \"hi\"")],
                vec![cell("line\r\nnext")]],
            SourceSpan::new(1, 100), TableConstraints::default(),
        ).unwrap()
    }

    #[test]
    fn clipboard_preserves_unicode_line_endings_quotes_and_ragged_shape() {
        let flow = sample();
        assert_eq!(flow.copy_tsv(0..2, 0..3, true, 1024).unwrap(),
            "H1\tH2\t\né\t\"a\tb\"\t\"say \"\"hi\"\"\"\n\"line\r\nnext\"\t\t");
        assert_eq!(flow.copy_tsv(0..2, 1..3, false, 1024).unwrap(),
            "\"a\tb\"\t\"say \"\"hi\"\"\"\n\t");
        assert_eq!(flow.copy_tsv(1..1, 0..2, true, 5).unwrap(), "H1\tH2");
    }

    #[test]
    fn single_empty_fields_remain_rows_instead_of_disappearing() {
        let flow = sample();
        assert_eq!(flow.copy_tsv(1..2, 2..3, false, 2).unwrap(), "\"\"");
        assert!(flow.copy_tsv(1..2, 2..3, false, 1).is_err());
        assert_eq!(flow.copy_tsv(1..2, 2..3, true, 5).unwrap(), "\"\"\n\"\"");
        assert_eq!(flow.copy_tsv(1..1, 2..3, false, 0).unwrap(), "");
    }

    #[test]
    fn clipboard_counts_escaped_bytes_and_separators_exactly() {
        let flow = sample();
        let expected = flow.copy_tsv(0..2, 0..3, true, 1024).unwrap();
        assert_eq!(flow.copy_tsv(0..2, 0..3, true, expected.len()).unwrap(), expected);
        assert!(matches!(flow.copy_tsv(0..2, 0..3, true, expected.len() - 1),
            Err(CodeTableError::OutputBudgetExceeded { .. })));
        assert_eq!(flow.copy_tsv(0..1, 0..1, false, 2).unwrap(), "é");
        assert!(flow.copy_tsv(0..1, 0..1, false, 1).is_err());
        let mut budget = TextBudget::new(4);
        let mut output = String::new();
        push_tsv_field(&mut output, "\"", &mut budget).unwrap();
        assert_eq!(output, "\"\"\"\"");
        assert!(push_tsv_field(&mut String::new(), "\"", &mut TextBudget::new(3)).is_err());
    }

    #[test]
    fn source_cells_are_borrowed_and_copy_does_not_depend_on_layout() {
        let mut flow = sample();
        assert!(std::ptr::eq(flow.cell(None, 0).unwrap(), &flow.headers()[0]));
        assert_eq!(flow.cell(Some(1), 0).unwrap().text, "line\r\nnext");
        assert!(flow.cell(Some(1), 1).is_none());
        assert!(flow.cell(Some(usize::MAX), 0).is_none());
        assert!(flow.cell(None, usize::MAX).is_none());
        flow.scroll_x = f32::NAN;
        flow.constraints_mut().max_col_width = f32::NAN;
        assert_eq!(flow.copy_tsv(0..1, 0..1, false, 2).unwrap(), "é");
        assert_eq!(flow.cell(Some(0), 0).unwrap().source_span, SourceSpan::new(4, 9));
    }

    #[test]
    fn empty_ranges_and_invalid_selections_are_explicit() {
        let flow = sample();
        assert_eq!(flow.copy_tsv(2..2, 0..3, false, 0).unwrap(), "");
        assert_eq!(flow.copy_tsv(0..2, 3..3, true, 0).unwrap(), "");
        assert!(flow.copy_tsv(Range { start: 1, end: 0 }, 0..1, false, 100).is_err());
        assert!(flow.copy_tsv(0..3, 0..1, false, 100).is_err());
        assert!(flow.copy_tsv(0..1, Range { start: 1, end: 0 }, false, 100).is_err());
        assert!(matches!(flow.copy_tsv(0..1, 0..4, false, 100),
            Err(CodeTableError::InvalidColumnIndex { index: 3, len: 3 })));
        assert!(flow.copy_tsv(0..1, 4..4, false, 100).is_err());
    }

    #[test]
    fn full_text_row_ranges_match_the_existing_semantic_tree() {
        let mut flow = sample();
        flow.scroll_x = 999.0;
        let origin = DisplayRect::new(10.0, 20.0, 180.0, 88.0);
        let full = flow.full_accessible_tree(origin);
        let range = flow.accessible_row_range(origin, 1..2, true, 4096).unwrap();
        assert_eq!(range.text, full.text);
        assert_eq!(range.bounds, full.bounds);
        assert_eq!(range.children.len(), 2);
        assert_eq!(range.children[0], full.children[0]);
        assert_eq!(range.children[1], full.children[2]);
        let range = flow.accessible_row_range(origin, 0..2, false, 4096).unwrap();
        assert_eq!(range.children.as_slice(), &full.children[1..]);
        let empty = flow.accessible_row_range(origin, 2..2, false, 4096).unwrap();
        assert!(empty.children.is_empty());
    }

    fn text_bytes(node: &AccessibleReadingNode) -> usize {
        node.text.len() + node.children.iter().map(text_bytes).sum::<usize>()
    }

    #[test]
    fn accessibility_budget_includes_all_labels_and_unicode_bytes() {
        let flow = sample();
        let origin = DisplayRect::default();
        let tree = flow.accessible_row_range(origin, 0..2, true, 4096).unwrap();
        let bytes = text_bytes(&tree);
        assert_eq!(flow.accessible_row_range(origin, 0..2, true, bytes).unwrap(), tree);
        assert!(matches!(flow.accessible_row_range(origin, 0..2, true, bytes - 1),
            Err(CodeTableError::OutputBudgetExceeded { .. })));
        assert!(flow.accessible_row_range(origin, 0..0, false, 0).is_err());
    }

    #[test]
    fn unselected_huge_cells_are_not_copied_and_remain_borrowable() {
        let huge = "x".repeat(MAX_CELL_TEXT_BYTES + 1);
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("H")],
            vec![vec![cell(&huge)], vec![cell("=1+1")]], SourceSpan::default(),
            TableConstraints::default()).unwrap();
        assert_eq!(flow.cell(Some(0), 0).unwrap().text, huge);
        assert!(flow.copy_tsv(0..1, 0..1, false, usize::MAX).is_err());
        assert!(flow.accessible_row_range(DisplayRect::default(), 0..1, false, usize::MAX).is_err());
        assert_eq!(flow.copy_tsv(1..2, 0..1, false, 4).unwrap(), "=1+1");
        let tree = flow.accessible_row_range(DisplayRect::default(), 1..2, false, 256).unwrap();
        assert_eq!(tree.children[0].children[0].text, "Row 1, Col 0: =1+1");
    }

    #[test]
    fn row_range_access_never_advances_width_measurement() {
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("H")],
            vec![vec![cell("Body")]; 150], SourceSpan::default(), TableConstraints::default()).unwrap();
        let width = flow.column_width(0).unwrap();
        let tree = flow.accessible_row_range(DisplayRect::default(), 110..112, true, 1024).unwrap();
        assert_eq!(tree.children[1].text, "Row 110");
        assert_eq!(tree.children[1].bounds.y, 32.0 + 110.0 * 28.0);
        assert_eq!(flow.copy_tsv(110..112, 0..1, false, 9).unwrap(), "Body\nBody");
        assert_eq!(flow.measured_rows_count(), 100);
        assert_eq!(flow.column_width(0).unwrap(), width);
    }

    #[test]
    fn semantic_ranges_reject_invalid_geometry_and_bound_metadata() {
        let flow = sample();
        assert!(flow.accessible_row_range(DisplayRect::default(), 0..3, true, 1024).is_err());
        assert!(flow.accessible_row_range(DisplayRect::default(), Range { start: 1, end: 0 }, true, 1024).is_err());
        for origin in [DisplayRect::new(f32::NAN, 0.0, 0.0, 0.0),
            DisplayRect::new(0.0, f32::INFINITY, 0.0, 0.0),
            DisplayRect::new(0.0, 0.0, -1.0, 1.0),
            DisplayRect::new(1.0e30, 0.0, 0.0, 0.0),
            DisplayRect::new(0.0, 1.0e30, 0.0, 0.0)] {
            assert!(flow.accessible_row_range(origin, 0..1, true, 1024).is_err());
        }
        let flow = ConstrainedTableFlow::try_new(vec![Align::Left], vec![cell("H")],
            vec![vec![]; MAX_VIEWPORT_CELLS], SourceSpan::default(), TableConstraints::default()).unwrap();
        assert!(matches!(flow.accessible_row_range(DisplayRect::default(), 0..MAX_VIEWPORT_CELLS, true, usize::MAX),
            Err(CodeTableError::RowBudgetExceeded { rows: MAX_VIEWPORT_CELLS, max }) if max == MAX_VIEWPORT_CELLS - 1));
    }
}
