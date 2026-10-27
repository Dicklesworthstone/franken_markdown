//! Horizontal panels for tables whose minimum readable grid exceeds the page.
//!
//! Panels retain the first source column as row context when two columns fit.
//! On the smallest valid measures, one-column panels still make progress and
//! preserve every source column in order. This handles structural grid width;
//! individual inline formulas and images are fitted to their final cell measure
//! by the shared inline-object placement path.

use super::{TABLE_COL_GUTTER, TABLE_MIN_COL_WIDTH};

/// Keep ordinary indentation. Only reclaim it when even one minimum-width
/// column and its gutter would otherwise extend beyond the body measure.
pub(super) fn geometry(content_width: f32, indent: f32) -> (f32, usize) {
    let column_width = TABLE_MIN_COL_WIDTH + TABLE_COL_GUTTER;
    let indent = indent.max(0.0).min((content_width - column_width).max(0.0));
    let available = content_width - indent;
    // This is deliberately conservative: a panel must fit at the nominal
    // gutter before adaptive font scaling, including tables of empty cells.
    let capacity = ((available / column_width) as usize).max(1);
    (indent, capacity)
}

/// Produce only the current panel's indices, bounded by the page capacity.
pub(super) struct Columns {
    columns: usize,
    capacity: usize,
    next: usize,
}

impl Columns {
    pub(super) fn new(columns: usize, capacity: usize) -> Self {
        Self {
            columns,
            capacity: capacity.max(1),
            next: 0,
        }
    }
}

impl Iterator for Columns {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.columns {
            return None;
        }
        let repeat_first = self.next != 0 && self.capacity > 1;
        let room = self.capacity - usize::from(repeat_first);
        let end = self.next.saturating_add(room).min(self.columns);
        let mut panel = Vec::with_capacity(end - self.next + usize::from(repeat_first));
        if repeat_first {
            panel.push(0);
        }
        panel.extend(self.next..end);
        self.next = end;
        Some(panel)
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::pdf::{Faces, FlowKind, Line, PageGeom, effective_pdf_options, layout};
    use crate::{Align, Block, Document, Inline, PdfOptions, Table};
    use std::collections::{BTreeMap, BTreeSet};

    fn text(value: impl Into<String>) -> Vec<Inline> {
        vec![Inline::Text(value.into())]
    }

    fn table(columns: usize, rows: usize) -> Table {
        Table {
            align: vec![Align::Left; columns],
            head: (0..columns)
                .map(|column| text(format!("H{column:02}")))
                .collect(),
            rows: (0..rows)
                .map(|row| {
                    (0..columns)
                        .map(|column| text(format!("{row:02}c{column:02}")))
                        .collect()
                })
                .collect(),
        }
    }

    fn prepared(table: Table, width: f32, indent: f32) -> (Vec<Line>, PageGeom) {
        let options = PdfOptions::default();
        let mut block = Block::Table(table);
        for _ in 0..(indent / 18.0).ceil() as usize {
            block = Block::BlockQuote(vec![block]);
        }
        let document = Document {
            blocks: vec![block],
        };
        let faces = Faces::load(&options).unwrap();
        let (options, mut page) = effective_pdf_options(&document, &options, &faces);
        page.content_w = width;
        page.width = page.left + width + page.right;
        let lines = layout(&document.blocks, &options, &faces, page);
        (lines, page)
    }

    fn cells(lines: &[Line]) -> BTreeMap<(u32, u32, u32), String> {
        let mut cells = BTreeMap::new();
        for line in lines {
            let Some(row) = &line.table_source_row else {
                continue;
            };
            for column in 0..row.columns {
                cells
                    .entry((line.flow.group, row.index, column))
                    .or_insert_with(String::new);
            }
            for (seg, &column) in line.segs.iter().zip(&line.table_cols) {
                cells
                    .entry((line.flow.group, row.index, column))
                    .or_insert_with(String::new)
                    .push_str(&seg.text);
            }
        }
        cells
    }

    fn assert_contained(lines: &[Line], page: PageGeom) {
        for line in lines
            .iter()
            .filter(|line| matches!(line.flow.kind, FlowKind::TableHeader | FlowKind::TableRow))
        {
            for seg in &line.segs {
                assert!(
                    seg.x >= page.left - 0.01,
                    "{} begins outside body",
                    seg.text
                );
                assert!(
                    seg.x + seg.width <= page.left + page.content_w + 0.01,
                    "{} ends at {}, beyond {}",
                    seg.text,
                    seg.x + seg.width,
                    page.left + page.content_w
                );
            }
        }
    }

    #[test]
    fn panels_cover_every_column_and_repeat_only_the_identifier() {
        for count in 0..65 {
            for capacity in 1..17 {
                let panels: Vec<_> = Columns::new(count, capacity).collect();
                let mut seen = vec![0; count];
                for (panel_index, panel) in panels.iter().enumerate() {
                    assert!(!panel.is_empty() && panel.len() <= capacity);
                    assert!(panel.windows(2).all(|pair| pair[0] < pair[1]));
                    if panel_index > 0 && capacity > 1 {
                        assert_eq!(panel[0], 0);
                    }
                    for &column in panel {
                        seen[column] += 1;
                    }
                }
                for (column, times) in seen.into_iter().enumerate() {
                    let expected = if column == 0 && capacity > 1 {
                        panels.len()
                    } else {
                        1
                    };
                    assert_eq!(times, expected);
                }
            }
        }
    }

    #[test]
    fn geometry_uses_real_measure_and_only_reclaims_pathological_indentation() {
        assert_eq!(geometry(468.0, 18.0), (18.0, 14));
        assert_eq!(geometry(40.0, 0.0), (0.0, 1));
        assert_eq!(geometry(40.0, 200.0), (8.0, 1));
        assert_eq!(geometry(468.0, 500.0), (436.0, 1));
    }

    #[test]
    fn wide_grid_keeps_all_cells_inside_body_and_repeats_row_context() {
        let (lines, page) = prepared(table(24, 3), 468.0, 0.0);
        assert_contained(&lines, page);
        let cells = cells(&lines);
        let groups: BTreeSet<_> = cells.keys().map(|key| key.0).collect();
        assert_eq!(groups.len(), 2);
        let mut seen = BTreeSet::new();
        for &group in &groups {
            let headers: Vec<_> = cells
                .iter()
                .filter(|((g, row, _), _)| *g == group && *row == 0)
                .collect();
            assert_eq!(headers[0].1, "H00");
            for ((_, _, column), header) in headers {
                let original: usize = header.trim_start_matches('H').parse().unwrap();
                seen.insert(original);
                for row in 0..3 {
                    assert_eq!(
                        cells[&(group, row + 1, *column)],
                        format!("{row:02}c{original:02}")
                    );
                }
            }
        }
        assert_eq!(seen, (0..24).collect());
    }

    #[test]
    fn forty_point_measure_and_deep_indentation_preserve_every_column() {
        for indent in [0.0, 200.0] {
            let (lines, page) = prepared(table(5, 2), 40.0, indent);
            assert_contained(&lines, page);
            let cells = cells(&lines);
            assert_eq!(
                cells.keys().map(|key| key.0).collect::<BTreeSet<_>>().len(),
                5
            );
            assert_eq!(
                cells
                    .values()
                    .filter(|value| value.starts_with('H'))
                    .count(),
                5
            );
            for column in 0..5 {
                assert!(
                    cells
                        .values()
                        .any(|value| value == &format!("01c{column:02}"))
                );
            }
        }
    }

    #[test]
    fn financial_semantics_and_source_rows_survive_column_projection() {
        let mut table = table(20, 4);
        table.align[19] = Align::Decimal;
        // This is a data row because column 18 contains ordinary text. Its
        // first panel sees only a marker spelling and must not reclassify it.
        table.rows[1] = vec![Vec::new(); 20];
        table.rows[1][5] = text("---");
        table.rows[1][18] = text("keep");
        // The actual footing is in a decimal column outside the first panel.
        table.rows[2] = vec![Vec::new(); 20];
        table.rows[2][19] = text("===");
        let (lines, page) = prepared(table, 468.0, 0.0);
        assert_contained(&lines, page);
        let cells = cells(&lines);
        let groups: Vec<_> = cells
            .keys()
            .map(|key| key.0)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(groups.len(), 2);
        for &group in &groups {
            let rows: BTreeSet<_> = cells
                .keys()
                .filter_map(|&(g, row, _)| (g == group).then_some(row))
                .collect();
            assert_eq!(rows, BTreeSet::from([0, 1, 2, 4]));
        }
        assert_eq!(cells[&(groups[0], 2, 5)], "---");
        assert!(cells.values().any(|value| value == "keep"));
        assert!(!cells.values().any(|value| value == "==="));
        assert!(
            lines
                .iter()
                .filter(|line| line.flow.group == groups[0])
                .all(|line| line.table_footing.is_empty())
        );
        let footing_lines: Vec<_> = lines
            .iter()
            .filter(|line| !line.table_footing.is_empty())
            .collect();
        assert_eq!(footing_lines.len(), 1);
        assert_eq!(footing_lines[0].flow.group, groups[1]);
        assert_eq!(footing_lines[0].table_source_row.as_ref().unwrap().index, 2);
        assert_eq!(footing_lines[0].table_footing.len(), 2);
        for stroke in &footing_lines[0].table_footing {
            assert!(stroke.x1 >= page.left && stroke.x2 <= page.left + page.content_w);
        }
    }
}
