//! Readable column layout and complete rich cells for SVG publication tables.

use super::{Align, Ink, Inline, Op, Poster, RStyle, Table, Word, financial_tables};

impl Poster {
    pub(super) fn table(&mut self, table: &Table, l: f64, r: f64) {
        let ncols = table
            .align
            .len()
            .max(table.head.len())
            .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
        if ncols == 0 {
            return;
        }
        let size = f64::from(self.scale.table);
        let leading = size * 1.35;
        let pad_x = self.table_pad_x;
        let pad_y = self.table_pad_y;
        let avail = r - l;
        let financial = crate::table_format::financial_rows(table);
        let rows: Vec<&[Vec<Inline>]> = match &financial {
            Some(rows) => rows.iter().map(|row| row.cells).collect(),
            None => table.rows.iter().map(Vec::as_slice).collect(),
        };
        let aligns: Vec<Align> = (0..ncols)
            .map(|c| table.align.get(c).copied().unwrap_or(Align::None))
            .collect();
        let mut decimal = vec![financial_tables::DecimalMeasure::default(); ncols];
        let measured: Vec<Vec<Option<financial_tables::DecimalMeasure>>> = if financial.is_some() {
            rows.iter()
                .map(|row| {
                    (0..ncols)
                        .map(|c| {
                            let value = if aligns[c] == Align::Decimal {
                                row.get(c).and_then(|cell| {
                                    financial_tables::cell_measure(self, cell, size)
                                })
                            } else {
                                None
                            };
                            if let Some(value) = value {
                                decimal[c].before = decimal[c].before.max(value.before);
                                decimal[c].after = decimal[c].after.max(value.after);
                            }
                            value
                        })
                        .collect()
                })
                .collect()
        } else {
            Vec::new()
        };

        // Measure each source column once. Panel selection borrows cells, so
        // rich content and financial marker rows keep their source identity.
        let mut natural = vec![size; ncols];
        let consider = |col: usize, cells: &[Inline], bold: bool, natural: &mut Vec<f64>| {
            let mut pieces = Vec::new();
            self.flatten(
                cells,
                RStyle {
                    bold,
                    ..RStyle::BODY
                },
                &mut pieces,
            );
            let w = self
                .wrap(&pieces, size, f64::MAX)
                .iter()
                .map(|line| self.words_width(line, size))
                .fold(0.0_f64, f64::max)
                + 2.0 * pad_x;
            natural[col] = natural[col].max(w.min(avail * 0.6));
        };
        for (c, cell) in table.head.iter().enumerate().take(ncols) {
            consider(c, cell, true, &mut natural);
        }
        for row in &rows {
            for (c, cell) in row.iter().enumerate().take(ncols) {
                consider(c, cell, false, &mut natural);
            }
        }
        if financial.is_some() {
            // Different rows can supply the longest integer and fractional
            // portions. Reserve their combined width before scaling a narrow
            // table, so each fitting numeric cell shares the same anchor.
            for (c, value) in decimal.iter().enumerate() {
                if aligns[c] == Align::Decimal {
                    natural[c] = natural[c].max(value.before + value.after + 2.0 * pad_x);
                }
            }
        }
        let total: f64 = natural.iter().sum();
        // A normal fitting grid retains its historical placement. Once it
        // needs compression, reserve two table ems plus the requested padding
        // per column rather than squeezing distinct cells onto the same ink.
        let minimum = 2.0 * size + 2.0 * pad_x;
        let capacity = if total <= avail {
            ncols
        } else {
            ((avail / minimum) as usize).max(1)
        };
        let panelled = capacity < ncols;
        for columns in Panels::new(ncols, capacity) {
            let floor = minimum.min(avail / columns.len() as f64);
            let preferred: Vec<f64> = columns
                .iter()
                .map(|&column| {
                    if panelled {
                        natural[column].max(floor)
                    } else {
                        natural[column]
                    }
                })
                .collect();
            let widths = column_widths(preferred, avail, floor);

            let draw_row = |poster: &mut Self,
                            cells: &[Vec<Inline>],
                            bold: bool,
                            top: f64,
                            row_index: Option<usize>|
             -> f64 {
                let st = RStyle {
                    bold,
                    ..RStyle::BODY
                };
                let wrapped: Vec<Vec<Vec<Word>>> = (0..columns.len())
                    .map(|c| {
                        let mut pieces = Vec::new();
                        if let Some(cell) = cells.get(columns[c]) {
                            poster.flatten(cell, st, &mut pieces);
                        }
                        let padding = cell_padding(widths[c], size, pad_x);
                        poster.wrap(&pieces, size, (widths[c] - 2.0 * padding).max(1.0))
                    })
                    .collect();
                let row_h = wrapped
                    .iter()
                    .map(|lines| {
                        lines
                            .iter()
                            .map(|line| poster.line_metrics(line, size * 0.8, leading).1)
                            .sum::<f64>()
                    })
                    .fold(leading, f64::max)
                    + 2.0 * pad_y;
                let mut cx = l;
                for (c, cell_lines) in wrapped.iter().enumerate() {
                    let source_column = columns[c];
                    let padding = cell_padding(widths[c], size, pad_x);
                    let cell_w = widths[c] - 2.0 * padding;
                    let mut line_top = top + pad_y;
                    for line in cell_lines {
                        let line_w = poster.words_width(line, size);
                        let x = match aligns[source_column] {
                            Align::Center => cx + padding + (cell_w - line_w) / 2.0,
                            Align::Decimal => {
                                let anchor = row_index
                                    .and_then(|index| measured.get(index))
                                    .and_then(|row| row[source_column])
                                    .filter(|_| {
                                        // Compare outer widths in the same order
                                        // used by the allocator. Subtracting the
                                        // padding first can lose one rounding unit
                                        // and reject an exactly fitting column.
                                        cell_lines.len() == 1
                                            && decimal[source_column].before
                                                + decimal[source_column].after
                                                + 2.0 * padding
                                                <= widths[c]
                                            && line_w + 2.0 * padding <= widths[c]
                                    })
                                    .and_then(|_| financial_tables::line_measure(line));
                                match anchor {
                                    Some(value) => {
                                        cx + padding + cell_w
                                            - decimal[source_column].after
                                            - value.before
                                    }
                                    None => cx + padding + (cell_w - line_w),
                                }
                            }
                            Align::Right => cx + padding + (cell_w - line_w),
                            Align::None | Align::Left => cx + padding,
                        };
                        let (ascent, height) = poster.line_metrics(line, size * 0.8, leading);
                        poster.draw_words(line, x, line_top + ascent, size);
                        line_top += height;
                    }
                    cx += widths[c];
                }
                row_h
            };

            let table_top = self.y;
            // Header row on a subtle stripe: record the insertion point so the
            // background rectangle precedes the header glyphs in painter's order
            // without calling draw_row twice.
            let ops_start = self.ops.len();
            let header_h = draw_row(self, &table.head, true, table_top, None);
            self.ops.insert(
                ops_start,
                Op::Rect {
                    x: l,
                    y: table_top,
                    w: avail,
                    h: header_h,
                    fill: Ink::BgSubtle,
                    stroke: None,
                },
            );
            let mut row_top = table_top + header_h;
            let mut bottoms = vec![row_top];
            for (index, row) in rows.iter().enumerate() {
                let h = draw_row(self, row, false, row_top, Some(index));
                row_top += h;
                if let Some(rows) = &financial {
                    let rules: Vec<_> = columns
                        .iter()
                        .map(|&column| rows[index].rules.get(column).copied().unwrap_or_default())
                        .collect();
                    row_top +=
                        financial_tables::draw_footings(self, &rules, &widths, l, row_top, pad_x);
                }
                bottoms.push(row_top);
            }
            let table_bottom = row_top;

            // Grid: horizontal separators, column separators, outer border.
            for &by in &bottoms {
                self.ops.push(Op::Rule {
                    x1: l,
                    y1: by,
                    x2: r,
                    y2: by,
                    ink: Ink::BorderMuted,
                    w: 0.5,
                });
            }
            let mut cx = l;
            for w in widths.iter().take(widths.len().saturating_sub(1)) {
                cx += w;
                self.ops.push(Op::Rule {
                    x1: cx,
                    y1: table_top,
                    x2: cx,
                    y2: table_bottom,
                    ink: Ink::BorderMuted,
                    w: 0.5,
                });
            }
            self.ops.push(Op::Rule {
                x1: l,
                y1: table_top,
                x2: r,
                y2: table_top,
                ink: Ink::Border,
                w: 0.75,
            });
            self.ops.push(Op::Rule {
                x1: l,
                y1: table_bottom,
                x2: r,
                y2: table_bottom,
                ink: Ink::Border,
                w: 0.75,
            });
            self.y = table_bottom + f64::from(self.scale.body) * 0.7;
        }
    }
}

/// Yield one bounded column selection at a time. Context repetition consumes
/// one slot only when another source column can still make progress.
struct Panels {
    columns: usize,
    capacity: usize,
    next: usize,
}

impl Panels {
    fn new(columns: usize, capacity: usize) -> Self {
        Self {
            columns,
            capacity: capacity.max(1),
            next: 0,
        }
    }
}

impl Iterator for Panels {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.columns {
            return None;
        }
        let repeat = self.next != 0 && self.capacity > 1;
        let end = self
            .next
            .saturating_add(self.capacity - usize::from(repeat))
            .min(self.columns);
        let mut columns = Vec::with_capacity(end - self.next + usize::from(repeat));
        if repeat {
            columns.push(0);
        }
        columns.extend(self.next..end);
        self.next = end;
        Some(columns)
    }
}

/// Keep ordinary proportions when every column remains readable. Otherwise
/// assign the common floor first and share only the remaining width in
/// proportion to each column's unmet natural width.
fn column_widths(preferred: Vec<f64>, available: f64, floor: f64) -> Vec<f64> {
    let total: f64 = preferred.iter().sum();
    if total <= available {
        return preferred;
    }
    let proportional: Vec<f64> = preferred
        .iter()
        .map(|width| width * available / total)
        .collect();
    if proportional.iter().all(|&width| width >= floor) {
        return proportional;
    }
    let remaining = (available - floor * preferred.len() as f64).max(0.0);
    let excess: f64 = preferred.iter().map(|width| (width - floor).max(0.0)).sum();
    preferred
        .into_iter()
        .map(|width| {
            floor
                + if excess > 0.0 {
                    (width - floor).max(0.0) * remaining / excess
                } else {
                    0.0
                }
        })
        .collect()
}

/// Deeply nested tables may have only one text em left. Reduce decoration
/// before sacrificing that glyph's containing width; never change type size.
fn cell_padding(width: f64, size: f64, requested: f64) -> f64 {
    requested
        .min(width * 0.25)
        .min(((width - size) * 0.5).max(0.0))
}

#[cfg(test)]
#[path = "table_tests.rs"]
mod tests;
