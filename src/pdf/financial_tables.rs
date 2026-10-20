//! Opt-in decimal columns and row-attached footing strokes.
//!
//! The ordinary GFM table path never constructs these decorations. Footings
//! are part of their data row's nonshrinkable height, so pagination cannot
//! strand a subtotal rule or mistake it for a repeated table header.

use super::{CellWrapLine, Faces, Line, TableColumnMetrics, Tok, WidthCache};
use crate::table_format::{FootingRule, decimal_anchor};
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FootingStroke {
    pub x1: f32,
    pub x2: f32,
    /// Distance below the attached row's final text baseline.
    pub below: f32,
}

#[derive(Clone, Copy, Default)]
pub(super) struct DecimalMeasure {
    pub before: f32,
    pub after: f32,
}

/// Measure the anchor in the same merged styled runs the PDF writer emits.
/// No numeric conversion, decimal padding, or character-count measurement is
/// involved: currency signs, parentheses and suffixes retain their advances.
pub(super) fn line_measure(
    line: &CellWrapLine,
    size: f32,
    faces: &Faces,
    widths: &RefCell<WidthCache>,
) -> Option<DecimalMeasure> {
    let text: String = line.runs.iter().map(|run| run.text.as_str()).collect();
    let anchor = decimal_anchor(&text)?;
    let mut remaining = anchor;
    let mut before = 0.0;
    for run in &line.runs {
        if remaining >= run.text.len() {
            before += run.width;
            remaining -= run.text.len();
        } else {
            let prefix = if run.text.get(remaining..)?.starts_with('.') {
                // The actual dot origin includes kerning from the preceding
                // digit. The dot's own advance is removed after measuring the
                // pair in context, matching the PDF TJ adjustment.
                super::text_width_cached(
                    run.text.get(..remaining + 1)?,
                    size,
                    run.slot,
                    faces,
                    widths,
                ) - super::text_width_cached(".", size, run.slot, faces, widths)
            } else {
                super::text_width_cached(run.text.get(..remaining)?, size, run.slot, faces, widths)
            };
            before += prefix;
            break;
        }
    }
    Some(DecimalMeasure {
        before,
        after: (line.width - before).max(0.0),
    })
}

pub(super) fn measure_columns(
    rows: &[Vec<Vec<Tok>>],
    eligible: &[Vec<bool>],
    columns: &mut [TableColumnMetrics],
    size: f32,
    faces: &Faces,
    widths: &RefCell<WidthCache>,
) -> Vec<DecimalMeasure> {
    let mut measures = vec![DecimalMeasure::default(); columns.len()];
    for (row, admission) in rows.iter().zip(eligible) {
        for (column, toks) in row.iter().enumerate() {
            if !admission.get(column).copied().unwrap_or(false) {
                continue;
            }
            let lines = super::wrap_cell_styled(toks, f32::MAX, size, faces, widths);
            let [line] = lines.as_slice() else {
                continue;
            };
            if let Some(measure) = line_measure(line, size, faces, widths)
                && let Some(total) = measures.get_mut(column)
            {
                total.before = total.before.max(measure.before);
                total.after = total.after.max(measure.after);
            }
        }
    }
    for (column, measure) in columns.iter_mut().zip(&measures) {
        let width = measure.before + measure.after;
        // A common anchor may require more room than any individual cell:
        // the longest integer and the longest fraction can occur in different
        // rows. Charge that width to the actual column allocator.
        column.min_content = column.min_content.max(width);
        column.max_content = column.max_content.max(width);
    }
    measures
}

pub(super) fn attach_footing(
    line: &mut Line,
    rules: &[FootingRule],
    starts: &[f32],
    widths: &[f32],
) {
    let below = line.size * 0.4 + super::inline_math_extents(line).1 + 1.0;
    for ((rule, &x1), &width) in rules.iter().zip(starts).zip(widths) {
        if matches!(rule, FootingRule::None) {
            continue;
        }
        line.table_footing.push(FootingStroke {
            x1,
            x2: x1 + width,
            below,
        });
        if matches!(rule, FootingRule::Double) {
            line.table_footing.push(FootingStroke {
                x1,
                x2: x1 + width,
                below: below + 2.0,
            });
        }
    }
}

/// Reserve the furthest stroke including half its thickness and a small
/// clearance. This is leading, not a flexible/discardable inter-block gap.
pub(super) fn footing_depth(line: &Line) -> f32 {
    let bottom = line
        .table_footing
        .iter()
        .map(|stroke| stroke.below + 0.7)
        .fold(0.0, f32::max);
    if bottom == 0.0 {
        0.0
    } else {
        // Inline formulas/images already reserve their additional descent.
        // Extend that reserved depth to the stroke rather than counting the
        // formula's descent for a second time.
        (bottom - super::inline_math_extents(line).1).max(0.0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::pdf::{
        FlowKind, PageGeom, Seg, effective_pdf_options, layout, paginate_lines_for_options,
        text_width,
    };
    use crate::{PdfOptions, parse_markdown};

    fn prepared(source: &str, opts: &PdfOptions) -> (Vec<Line>, Faces, PageGeom) {
        let doc = parse_markdown(source);
        let faces = Faces::load(opts).unwrap();
        let (effective, page) = effective_pdf_options(&doc, opts, &faces);
        (layout(&doc.blocks, &effective, &faces, page), faces, page)
    }

    fn cell_segments(line: &Line, column: u32) -> Vec<&Seg> {
        line.segs
            .iter()
            .zip(&line.table_cols)
            .filter_map(|(seg, &col)| (col == column).then_some(seg))
            .collect()
    }

    /// Coordinate of a specified byte in the actual emitted segment sequence.
    /// Test witnesses supply their own anchor offsets, independent of the
    /// production numeric classifier.
    fn byte_position(segs: &[&Seg], mut byte: usize, size: f32, faces: &Faces) -> f32 {
        for seg in segs {
            if byte <= seg.text.len() {
                let prefix_width = if seg.text[byte..].starts_with('.') {
                    text_width(&seg.text[..byte + 1], size, seg.slot, faces)
                        - text_width(".", size, seg.slot, faces)
                } else {
                    text_width(&seg.text[..byte], size, seg.slot, faces)
                };
                return seg.x + prefix_width;
            }
            byte -= seg.text.len();
        }
        panic!("anchor lies beyond cell text")
    }

    #[test]
    fn financial_decimal_columns_align_actual_styled_run_coordinates() {
        let source = "| Item | Amount |\n| --- | ---. |\n\
                      | Revenue | 1,234.5 |\n\
                      | Costs | (234.50) |\n\
                      | Adjustment | **9.0000** |\n\
                      | Count | 17 |\n\
                      | Currency | $12.25 |\n\
                      | Rate | *2.5%* |\n\
                      | Linked | [7.00](https://example.com/value) |\n\
                      | Mixed | **20**.125 |\n";
        let witnesses = [
            ("1,234.5", 5),
            ("(234.50)", 4),
            ("9.0000", 1),
            ("17", 2),
            ("$12.25", 3),
            ("2.5%", 1),
            ("7.00", 1),
            ("20.125", 2),
        ];
        for font in [
            crate::theme::FontFamily::Sans,
            crate::theme::FontFamily::Serif,
        ] {
            let opts = PdfOptions {
                theme: crate::theme::Theme::default().with_font(font),
                ..PdfOptions::default()
            };
            let (lines, faces, _) = prepared(source, &opts);
            let rows: Vec<_> = lines
                .iter()
                .filter(|line| line.flow.kind == FlowKind::TableRow)
                .collect();
            assert_eq!(rows.len(), witnesses.len());
            let mut anchors = Vec::new();
            let mut right_edges = Vec::new();
            for (row, (text, offset)) in rows.iter().zip(witnesses) {
                let segments = cell_segments(row, 1);
                assert_eq!(
                    segments
                        .iter()
                        .map(|seg| seg.text.as_str())
                        .collect::<String>(),
                    text
                );
                anchors.push(byte_position(&segments, offset, row.size, &faces));
                let last = segments.last().unwrap();
                right_edges.push(last.x + last.width);
            }
            assert!(
                anchors.iter().all(|x| (x - anchors[0]).abs() < 0.03),
                "decimal anchors differ for {font:?}: {anchors:?}"
            );
            assert!(
                right_edges.iter().any(|x| (x - right_edges[0]).abs() > 2.0),
                "mixed precision must share decimal anchors rather than right edges"
            );
            assert!(rows[6].segs.iter().any(|seg| seg.link.is_some()));
            assert!(
                rows[7]
                    .segs
                    .iter()
                    .any(|seg| seg.slot == crate::pdf::F_BOLD)
            );
        }
    }

    #[test]
    fn financial_column_reserves_longest_integer_plus_longest_fraction() {
        let source = "| Name | Amount |\n| --- | ---. |\n\
                      | Long integer | 123456789.0 |\n\
                      | Long fraction | 1.123456789 |\n";
        let (lines, faces, _) = prepared(source, &PdfOptions::default());
        let rows: Vec<_> = lines
            .iter()
            .filter(|line| line.flow.kind == FlowKind::TableRow)
            .collect();
        let first = cell_segments(rows[0], 1);
        let second = cell_segments(rows[1], 1);
        let integer_anchor = byte_position(&first, 9, rows[0].size, &faces);
        let fraction_anchor = byte_position(&second, 1, rows[1].size, &faces);
        assert!((integer_anchor - fraction_anchor).abs() < 0.03);
        for row in rows {
            let label = cell_segments(row, 0);
            let amount = cell_segments(row, 1);
            let label_end = label.last().unwrap();
            assert!(amount[0].x > label_end.x + label_end.width);
        }
    }

    #[test]
    fn financial_footings_follow_wrapped_rows_and_preserve_source_identities() {
        let source = "| Item | Amount |\n| --- | ---. |\n\
                      | A label with enough words to occupy several lines in a narrow table | 10.5 |\n\
                      | | --- |\n\
                      | Total | **10.50** |\n\
                      | | === |\n";
        let mut opts = PdfOptions::default();
        opts.theme.page.size.width_pt = 270.0;
        opts.theme.page.margins.left_pt = 30.0;
        opts.theme.page.margins.right_pt = 30.0;
        let (lines, _, _) = prepared(source, &opts);
        let data: Vec<_> = lines
            .iter()
            .filter(|line| line.flow.kind == FlowKind::TableRow)
            .collect();
        assert!(data.len() > 2, "the long label must wrap");
        for line in &data {
            assert!(matches!(line.table_source_row, Some(1 | 3)));
            if !line.table_footing.is_empty() {
                assert_eq!(line.flow.index + 1, line.flow.count);
            }
        }
        let decorated: Vec<_> = data
            .iter()
            .filter(|line| !line.table_footing.is_empty())
            .collect();
        assert_eq!(decorated.len(), 2);
        assert_eq!(decorated[0].table_footing.len(), 1);
        assert_eq!(decorated[1].table_footing.len(), 2);
        assert_eq!(decorated[0].table_source_row, Some(1));
        assert_eq!(decorated[1].table_source_row, Some(3));
        let double = &decorated[1].table_footing;
        assert_eq!(double[0].x1, double[1].x1);
        assert_eq!(double[0].x2, double[1].x2);
        assert!((double[1].below - double[0].below - 2.0).abs() < 0.001);
        assert!(footing_depth(decorated[1]) > double[1].below);
    }

    #[test]
    fn financial_footings_stay_inside_pages_and_never_repeat_as_headers() {
        let mut source = String::from("| Item | Amount |\n| --- | ---. |\n");
        for row in 0..32 {
            source.push_str(&format!("| Record{row} | {row}.25 |\n| | === |\n"));
        }
        for optimal in [false, true] {
            let mut opts = PdfOptions {
                optimal_pagination: optimal,
                ..PdfOptions::default()
            };
            opts.theme.page.size.height_pt = 220.0;
            opts.theme.page.margins.top_pt = 30.0;
            opts.theme.page.margins.bottom_pt = 30.0;
            let (lines, _, page) = prepared(&source, &opts);
            let pages = paginate_lines_for_options(&lines, page, &opts);
            assert!(pages.len() > 2);
            let mut decorated = 0;
            let mut source_rows = Vec::new();
            for page_lines in &pages {
                assert!(
                    page_lines
                        .iter()
                        .any(|placed| { placed.line.flow.kind == FlowKind::TableHeader }),
                    "every continuation page must repeat the real header"
                );
                for placed in page_lines {
                    if placed.line.flow.kind == FlowKind::TableHeader {
                        assert!(placed.line.table_footing.is_empty());
                        assert_eq!(placed.line.table_source_row, Some(0));
                    }
                    if !placed.line.table_footing.is_empty() {
                        decorated += 1;
                        source_rows.push(placed.line.table_source_row.unwrap());
                        assert_eq!(placed.line.flow.kind, FlowKind::TableRow);
                        for stroke in &placed.line.table_footing {
                            assert!(
                                placed.y - stroke.below - 0.35 >= page.bottom - 0.01,
                                "footing escaped page bottom in optimal={optimal}"
                            );
                        }
                    }
                }
            }
            assert_eq!(decorated, 32);
            assert_eq!(
                source_rows,
                (0..32).map(|row| row * 2 + 1).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn ordinary_tables_keep_literal_marker_rows_and_no_financial_metadata() {
        let source = "| Item | Amount |\n| --- | ---: |\n\
                      | Total | 1.25 |\n| | === |\n";
        let (lines, _, _) = prepared(source, &PdfOptions::default());
        assert!(
            lines
                .iter()
                .all(|line| { line.table_footing.is_empty() && line.table_source_row.is_none() })
        );
        assert!(
            lines
                .iter()
                .any(|line| line.segs.iter().any(|seg| seg.text == "==="))
        );
    }

    #[test]
    fn financial_footings_clear_deep_inline_math_without_double_counting_its_depth() {
        let source = "| Calculation | Amount |\n| --- | ---. |\n\
                      | $\\frac{1}{\\frac{2}{3}}$ | 12.50 |\n| === | === |\n";
        let (lines, _, page) = prepared(source, &PdfOptions::default());
        let row = lines
            .iter()
            .find(|line| !line.table_footing.is_empty())
            .unwrap();
        assert!(row.segs.iter().any(|seg| seg.slot == crate::pdf::F_MATH));
        let (_, extra_descent) = crate::pdf::inline_math_extents(row);
        assert!(
            extra_descent > 0.0,
            "the regression requires a deep formula"
        );
        for stroke in &row.table_footing {
            assert!(stroke.below > extra_descent + row.size * 0.4);
        }
        let furthest = row
            .table_footing
            .iter()
            .map(|stroke| stroke.below)
            .fold(0.0, f32::max);
        assert!((extra_descent + footing_depth(row) - furthest - 0.7).abs() < 0.001);
        let pages = paginate_lines_for_options(&lines, page, &PdfOptions::default());
        let placed = pages
            .iter()
            .flatten()
            .find(|placed| !placed.line.table_footing.is_empty())
            .unwrap();
        assert!(placed.y - furthest > page.bottom);
    }

    #[test]
    fn cached_financial_tables_retain_decimal_positions_and_footing_geometry() {
        let table = "| Item | Amount |\n| --- | ---. |\n\
                     | One | 1.234 |\n| Total | **12.5** |\n| | === |\n";
        let source = format!("{table}\n{table}");
        let (lines, _, _) = prepared(&source, &PdfOptions::default());
        let decorated: Vec<_> = lines
            .iter()
            .filter(|line| !line.table_footing.is_empty())
            .collect();
        assert_eq!(decorated.len(), 2);
        assert_ne!(decorated[0].flow.group, decorated[1].flow.group);
        assert_eq!(decorated[0].table_footing, decorated[1].table_footing);
        assert_eq!(decorated[0].table_source_row, decorated[1].table_source_row);
        for (first, second) in decorated[0].segs.iter().zip(&decorated[1].segs) {
            assert_eq!(first.x, second.x);
            assert_eq!(first.text, second.text);
        }
    }
}
