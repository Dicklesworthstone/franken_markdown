//! Bounded intrinsic measurement for long table cells. Final layout uses the
//! same continuation renderer as prose/code; measurement must not reject a
//! cell solely because its physical line is larger than one shaping call.
//! Like prose continuation, this requires enough word-boundary context in a
//! window; it never guesses a split inside an unknown shaper cluster.

use super::{FlowInlineRun, FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow};
use super::styled::{measurement_window_end, validate_ranges};
use crate::text::OwnedTextRun;
use std::ops::Range;

struct MetricCluster {
    bytes: Range<usize>,
    advance: f32,
    separator: bool,
}

impl<F> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    /// Only a window's cluster metrics survive each shaping call. Every cell
    /// is still inspected, including late wide clusters and pending rows.
    ///
    /// Preferred widths may saturate at the whole table's available width:
    /// water filling cannot assign any column more than that. Minimum cluster
    /// widths MUST NOT saturate or stop being measured when this cap is met.
    /// This also avoids large full-line floating-point sums for giant cells.
    pub(super) fn windowed_cell_measures(
        &mut self, text: &str, runs: &[FlowInlineRun], role: FlowTextRole, available: f32,
    ) -> Result<(f64, f64), FlowLayoutError> {
        let plain = runs.is_empty()
            || runs.iter().all(|run| run.style == FlowInlineStyle::default() && run.link.is_none());
        let unstyled = [FlowInlineRun {
            range: 0..text.len(), style: FlowInlineStyle::default(), link: None,
        }];
        let runs = if plain { &unstyled[..] } else { runs };
        validate_ranges(text, runs)?;
        let cap = f64::from(available);
        let mut minimum = 1.0_f64;
        let mut preferred = 1.0_f64;
        let mut offset = 0;
        for physical in text.split_terminator('\n') {
            let physical_end = offset + physical.len();
            let mut start = offset;
            let mut advance = 0.0_f64;
            while start < physical_end {
                let end = measurement_window_end(text, start, physical_end, self.options.max_shape_bytes)?;
                let clusters = self.cell_window(text, runs, start..end, role)?;
                let settled = if end == physical_end {
                    clusters.len()
                } else {
                    // Retain the last word as right-hand shaping context. A
                    // work boundary is neither a word break nor an intrinsic
                    // line boundary. Cut only AFTER a shaped separator cluster,
                    // never inside an indivisible host-provided cluster.
                    clusters.iter().rposition(|cluster| cluster.separator)
                        .map(|index| index + 1)
                        .ok_or(FlowLayoutError::BudgetExceeded("table shaping window cannot settle a word"))?
                };
                for cluster in &clusters[..settled] {
                    minimum = minimum.max(f64::from(cluster.advance));
                    advance = (advance + f64::from(cluster.advance)).min(cap);
                }
                let next = clusters.get(settled).map_or(end, |cluster| cluster.bytes.start);
                if next <= start {
                    return Err(FlowLayoutError::BudgetExceeded("table shaping window made no progress"));
                }
                start = next;
            }
            preferred = preferred.max(advance);
            offset = physical_end + 1;
        }
        Ok((minimum, preferred.max(minimum)))
    }

    fn cell_window(
        &mut self, text: &str, runs: &[FlowInlineRun], window: Range<usize>, role: FlowTextRole,
    ) -> Result<Vec<MetricCluster>, FlowLayoutError> {
        let mut clusters = Vec::new();
        let first = runs.partition_point(|run| run.range.end <= window.start);
        for spec in runs.iter().skip(first) {
            if spec.range.start >= window.end { break; }
            let start = spec.range.start.max(window.start);
            let end = spec.range.end.min(window.end);
            if start == end { continue; }
            let size = if spec.style.code { self.options.code_size } else { self.options.body_size };
            let measured = self.shaped_with_style(&text[start..end], size, role, spec.style)?;
            let mut logical: Vec<_> = measured.clusters.iter().collect();
            logical.sort_by_key(|cluster| cluster.byte_range.start);
            for cluster in logical {
                let bytes = start + cluster.byte_range.start..start + cluster.byte_range.end;
                let separator = text[bytes.clone()].chars().all(|ch| ch == ' ' || ch == '\t');
                clusters.push(MetricCluster { bytes, advance: cluster.advance(), separator });
            }
        }
        Ok(clusters)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use super::super::{FlowLayoutOptions, table_column_edges, tests::measured};
    use crate::display::{AccessibleReadingRole, DisplayItem, DisplayList, DisplayTextRun};
    use crate::flow_display::ResumableFlowDisplay;
    use crate::fonts::BundledFlowFonts;
    use crate::theme::FontFamily;

    fn engine(source: &str, batch: usize) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        engine
    }

    fn options(width: f32, window: usize) -> FlowLayoutOptions {
        FlowLayoutOptions { viewport_width: width, max_shape_bytes: window, ..FlowLayoutOptions::default() }
    }

    fn text_items(list: &DisplayList) -> Vec<&DisplayTextRun> {
        list.items().iter().filter_map(|item| match item {
            DisplayItem::Text(run) => Some(run), _ => None,
        }).collect()
    }

    fn style_shape(text: &str, size: f32, role: FlowTextRole, style: FlowInlineStyle)
        -> Result<OwnedTextRun, String>
    {
        let mut run = measured(text, size, role)?;
        if style.bold {
            for cluster in &mut run.clusters { cluster.x_start *= 1.5; cluster.x_end *= 1.5; }
            for glyph in &mut run.glyphs { glyph.x_advance *= 1.5; }
            run.total_advance *= 1.5;
        }
        Ok(run)
    }

    #[test]
    fn long_narrative_cells_render_without_raising_the_default_shape_limit() {
        let narrative = "alpha beta fi café ".repeat(1200);
        assert!(narrative.len() > FlowLayoutOptions::default().max_shape_bytes);
        let source = format!("| ID | Description |\n| --- | --- |\n| 7 | {narrative} |\n");
        let engine = engine(&source, 8);
        let expected = engine.to_shaped_display_list(options(200.0, source.len()), measured).unwrap();
        for window in [64, 127, 16 * 1024] {
            let mut charged = 0;
            let actual = engine.to_shaped_display_list(options(200.0, window), |text, size, role| {
                assert!(text.len() <= window);
                charged += text.len();
                measured(text, size, role)
            }).unwrap();
            assert_eq!(actual, expected);
            assert!(charged < source.len() * 8);
            assert_eq!(actual.reading_order()[1].children[0].bounds.width, 28.0);
            assert_eq!(actual.reading_order()[1].children[1].text, narrative.trim_end());
        }
    }

    #[test]
    fn long_styled_cells_keep_links_faces_and_column_alignment() {
        let cell = "start [**bold** fi](#target) ~~gone~~ `code` next ".repeat(90);
        let source = format!("| ID | Description |\n| :--- | ---: |\n| 1 | {cell} |\n\nafter");
        let engine = engine(&source, 1);
        let expected = engine.to_styled_display_list(options(190.0, source.len()), style_shape).unwrap();
        for window in [64, 97, 256] {
            let actual = engine.to_styled_display_list(options(190.0, window), |text, size, role, style| {
                assert!(text.len() <= window);
                style_shape(text, size, role, style)
            }).unwrap();
            assert_eq!(actual, expected);
            let body = &actual.reading_order()[1].children[1];
            let links: Vec<_> = actual.anchors().filter(|anchor| !anchor.is_heading).collect();
            assert!(!links.is_empty());
            for link in links {
                assert_eq!(link.anchor_id, "#target");
                assert!(link.bounds.x >= body.bounds.x + 4.0);
                assert!(link.bounds.right() <= body.bounds.right() - 4.0);
                assert_eq!(actual.link_at_point(link.bounds.x + link.bounds.width * 0.5,
                    link.bounds.y + 1.0).unwrap().anchor_id, "#target");
            }
        }
    }

    #[test]
    fn pending_long_rows_fix_the_column_grid_before_the_header_is_presented() {
        let source = format!("| ID | Narrative |\n| --- | --- |\n| x | short |\n| 123 | {} |\n", "many words ".repeat(1600));
        let opts = options(220.0, 128);
        let expected = engine(&source, usize::MAX).to_shaped_display_list(opts, measured).unwrap();
        let mut partial = ResumableFlowDisplay::new(&source, 1);
        while partial.blocks().is_empty() { partial.step().unwrap(); }
        assert_eq!(partial.blocks().len(), 1);
        let header = partial.to_shaped_display_list(opts, measured).unwrap();
        assert_eq!(header.reading_order()[0], expected.reading_order()[0]);
        partial.process_all().unwrap();
        assert_eq!(partial.to_shaped_display_list(opts, measured).unwrap(), expected);
    }

    #[test]
    fn unicode_cell_clusters_survive_small_windows_and_emission_batches() {
        let source = format!("| H |\n| :---: |\n| {} |\n", "fi a\u{0301} 東京 😀 ".repeat(100));
        let expected = engine(&source, 8).to_shaped_display_list(options(83.0, source.len()), measured).unwrap();
        for batch in [1, 2, 7] {
            let actual = engine(&source, batch).to_shaped_display_list(options(83.0, 63), measured).unwrap();
            assert_eq!(actual, expected);
            let body = &actual.reading_order()[1];
            let text: String = text_items(&actual).iter().filter(|run| run.bounds.y >= body.bounds.y)
                .map(|run| run.text.as_str()).collect();
            assert_eq!(text, body.children[0].text);
            for run in text_items(&actual) {
                assert!(!run.text.starts_with('\u{0301}'));
                assert_ne!(run.text, "f");
                assert_ne!(run.text, "i");
            }
        }
    }

    #[test]
    fn late_wide_clusters_are_measured_even_after_preferred_width_saturates() {
        let text = format!("{} Ω tail", "short ".repeat(200));
        let mut shape = |text: &str, size, role, _| {
            let mut run = measured(text, size, role)?;
            if text.contains('Ω') {
                for cluster in &mut run.clusters { cluster.x_start *= 4.0; cluster.x_end *= 4.0; }
                for glyph in &mut run.glyphs { glyph.x_advance *= 4.0; }
                run.total_advance *= 4.0;
            }
            Ok(run)
        };
        let mut layout = Reflow::new(options(80.0, 64), &mut shape).unwrap();
        let (minimum, preferred) = layout.windowed_cell_measures(&text, &[], FlowTextRole::TableCell, 80.0).unwrap();
        assert_eq!(minimum, 40.0);
        assert_eq!(preferred, 80.0);
        assert!(layout.list.items().is_empty());
    }

    #[test]
    fn intrinsic_line_widths_reset_at_hard_breaks_and_do_not_sum_separate_lines() {
        let text = "alpha beta\n\nfi a\u{0301}\n";
        let mut shape = |text: &str, size, role, _| measured(text, size, role);
        let mut layout = Reflow::new(options(500.0, 64), &mut shape).unwrap();
        let actual = layout.windowed_cell_measures(text, &[], FlowTextRole::TableCell, 500.0).unwrap();
        let expected = layout.cell_measures(text, &[], FlowTextRole::TableCell).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual, (10.0, 100.0));
    }

    #[test]
    fn capping_preferred_widths_at_available_space_does_not_change_water_filling() {
        let mut seed = 1_u64;
        for _ in 0..1000 {
            let mut minima = Vec::new();
            let mut preferred = Vec::new();
            for _ in 0..4 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let minimum = 1.0 + (seed % 17) as f64;
                let ideal = minimum + ((seed >> 16) % 10_000) as f64;
                minima.push(minimum);
                preferred.push(ideal);
            }
            let available = 400.0_f32;
            let capped: Vec<_> = preferred.iter().map(|value| value.min(f64::from(available))).collect();
            assert_eq!(table_column_edges(&minima, &preferred, available).unwrap(),
                table_column_edges(&minima, &capped, available).unwrap());
        }
    }

    #[test]
    fn table_windows_charge_shared_work_budgets_and_late_failures_are_atomic() {
        let source = format!("| H |\n| --- |\n| {} |\n", "many words ".repeat(200));
        let engine = engine(&source, 1);
        let before = engine.to_display_list();
        let opts = options(100.0, 64);
        for opts in [FlowLayoutOptions { max_total_shape_bytes: 100, ..opts },
            FlowLayoutOptions { max_lines: 2, ..opts }, FlowLayoutOptions { max_items: 2, ..opts }] {
            assert!(matches!(engine.to_shaped_display_list(opts, measured), Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
        let mut calls = 0;
        let failed = engine.to_shaped_display_list(opts, |text, size, role| {
            calls += 1;
            if calls == 20 { return Err("late table font failure".to_owned()); }
            measured(text, size, role)
        });
        assert!(matches!(failed, Err(FlowLayoutError::Shaping(message)) if message == "late table font failure"));
        assert_eq!(calls, 20);
        assert_eq!(engine.to_display_list(), before);
    }

    #[test]
    fn long_code_lines_preserve_cluster_wrapping_not_prose_word_wrapping() {
        let raw = format!("  {}\n\nlast\n", "aa bbbb \tfi ".repeat(1600));
        assert!(raw.len() > FlowLayoutOptions::default().max_shape_bytes);
        let source = format!("```text\n{raw}```\n");
        let engine = engine(&source, 1);
        let expected = engine.to_shaped_display_list(options(50.0, source.len()), measured).unwrap();
        for window in [64, 127, 16 * 1024] {
            let actual = engine.to_shaped_display_list(options(50.0, window), |text, size, role| {
                assert!(text.len() <= window);
                measured(text, size, role)
            }).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(actual.reading_order()[0].role, AccessibleReadingRole::CodeBlock);
            assert_eq!(actual.reading_order()[0].text, raw);
            assert!(text_items(&actual).iter().all(|run| run.color_role == "code"));
            assert!(text_items(&actual).iter().any(|run| run.text == "bbb \t"));
        }
    }

    #[test]
    fn long_code_unicode_clusters_and_final_line_reshaping_keep_exact_reading_bytes() {
        let raw = "fi a\u{0301} 東京 😀 ".repeat(120);
        let source = format!("```text\n{raw}\n```\n");
        let expected = engine(&source, 8).to_shaped_display_list(options(50.0, source.len()), measured).unwrap();
        for batch in [1, 3] {
            let actual = engine(&source, batch).to_shaped_display_list(options(50.0, 63), measured).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(text_items(&actual).iter().map(|run| run.text.as_str()).collect::<String>(), raw);
            for run in text_items(&actual) {
                assert!(!run.text.starts_with('\u{0301}'));
                assert!(run.bounds.width <= 50.0);
            }
        }
    }

    #[test]
    fn long_code_preserves_blank_physical_lines_without_phantom_terminal_lines() {
        let source = format!("```text\n{}\n\n\nend\n```", "ab cd ".repeat(90));
        let engine = engine(&source, 8);
        let expected = engine.to_shaped_display_list(options(40.0, source.len()), measured).unwrap();
        let actual = engine.to_shaped_display_list(options(40.0, 64), measured).unwrap();
        assert_eq!(actual, expected);
        let runs = text_items(&actual);
        let end = runs.last().unwrap();
        let before_blanks = runs[runs.len() - 2];
        assert_eq!(end.text, "end");
        assert_eq!(end.bounds.y - before_blanks.bounds.y, 60.0);
        assert_eq!(actual.reading_order()[0].bounds.height, end.bounds.bottom());
    }

    #[test]
    fn unresolvable_words_remain_errors_instead_of_splitting_unknown_shaper_clusters() {
        for source in [format!("```text\n{}\n```", "x".repeat(100)),
            format!("| H |\n| --- |\n| {} |\n", "x".repeat(100))] {
            let engine = engine(&source, 1);
            let before = engine.to_display_list();
            assert!(matches!(engine.to_shaped_display_list(options(80.0, 32), measured),
                Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
    }

    #[test]
    fn long_code_cannot_reset_work_or_output_limits_between_windows() {
        let source = format!("```text\n{}\n```", "one two three ".repeat(300));
        let engine = engine(&source, 1);
        let before = engine.to_display_list();
        let opts = options(60.0, 64);
        for opts in [FlowLayoutOptions { max_total_shape_bytes: 100, ..opts },
            FlowLayoutOptions { max_lines: 2, ..opts }, FlowLayoutOptions { max_items: 2, ..opts }] {
            assert!(matches!(engine.to_shaped_display_list(opts, measured),
                Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
        let mut calls = 0;
        let result = engine.to_shaped_display_list(opts, |text, size, role| {
            calls += 1;
            if calls == 20 { return Err("late code font failure".to_owned()); }
            measured(text, size, role)
        });
        assert!(matches!(result, Err(FlowLayoutError::Shaping(message)) if message == "late code font failure"));
        assert_eq!(calls, 20);
        assert_eq!(engine.to_display_list(), before);
    }

    #[test]
    fn overwide_late_table_clusters_are_refused_before_any_grid_is_published() {
        let source = format!("| H |\n| --- |\n| {} Ω tail |\n", "short ".repeat(100));
        let engine = engine(&source, 1);
        let before = engine.to_display_list();
        let result = engine.to_shaped_display_list(options(35.0, 64), |text, size, role| {
            let mut run = measured(text, size, role)?;
            if text.contains('Ω') {
                for cluster in &mut run.clusters { cluster.x_start *= 4.0; cluster.x_end *= 4.0; }
                for glyph in &mut run.glyphs { glyph.x_advance *= 4.0; }
                run.total_advance *= 4.0;
            }
            Ok(run)
        });
        assert!(matches!(result, Err(FlowLayoutError::TableTooNarrow { .. })));
        assert_eq!(engine.to_display_list(), before);
    }

    #[test]
    fn bundled_fonts_render_long_code_and_narrative_tables_with_bounded_calls() {
        let source = format!("| ID | Narrative |\n| --- | --- |\n| 1 | {} |\n\n```text\n{}\n```",
            "office AV and ordinary prose ".repeat(700), "let value = 42; ".repeat(1200));
        let engine = engine(&source, 8);
        let fonts = BundledFlowFonts::new(FontFamily::Sans).unwrap();
        for window in [256, 16 * 1024] {
            let list = engine.to_styled_display_list(options(240.0, window), |text, size, role, style| {
                assert!(text.len() <= window);
                fonts.shape(text, size, role, style)
            }).unwrap();
            assert!(list.reading_order().iter().any(|node| node.role == AccessibleReadingRole::CodeBlock));
            assert!(list.reading_order().iter().any(|node| node.role == AccessibleReadingRole::TableRow));
            assert!(text_items(&list).iter().all(|run| run.bounds.right() <= 240.01));
            assert!(text_items(&list).iter().all(|run| run.font_run.as_ref().is_some_and(|font|
                font.glyphs.iter().all(|glyph| fonts.font_bytes(glyph.font_id).is_some()))));
        }
    }
}
