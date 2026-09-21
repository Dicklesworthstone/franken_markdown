//! Font-measured list marker gutters shared by every item of an admitted list.
//! AST identities, not source spans or digit-count guesses, define ownership.

use super::{BlockMeta, FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow};
use crate::text::OwnedTextRun;
use std::collections::HashMap;

const MIN_GUTTER: f32 = 20.0;
const MARKER_GAP: f32 = 4.0;
const CHECKBOX_WIDTH: f32 = 16.0;

pub(super) struct ListGutters {
    widths: HashMap<u64, f32>,
}

impl ListGutters {
    pub(super) fn indent(&self, meta: &BlockMeta) -> f32 {
        // Keep legacy geometry exact for ordinary bullets and preserve the
        // extra indentation used by non-list structures such as definitions.
        let mut x = f32::from(meta.list_depth) * MIN_GUTTER + f32::from(meta.quote_depth) * 16.0;
        for frame in meta.list_path.iter() {
            x += self.widths.get(&frame.list_id).copied().unwrap_or(MIN_GUTTER) - MIN_GUTTER;
        }
        x
    }

    pub(super) fn width(&self, meta: &BlockMeta) -> f32 {
        meta.list_path.last().and_then(|frame| self.widths.get(&frame.list_id))
            .copied().unwrap_or(MIN_GUTTER)
    }
}

impl<F> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    pub(super) fn measure_list_gutters<'b, I>(
        &mut self, emitted: &'b [BlockMeta], pending: I,
    ) -> Result<ListGutters, FlowLayoutError>
    where
        I: Iterator<Item = &'b BlockMeta>,
    {
        let mut widths = HashMap::<u64, f32>::new();
        // Only lists which own emitted content can affect this display. A
        // wholly unshown later list must not consume shaping work or fail it.
        for meta in emitted {
            for frame in meta.list_path.iter() {
                let full = widths.len() >= self.options.max_items;
                if let std::collections::hash_map::Entry::Vacant(entry) = widths.entry(frame.list_id) {
                    if full { return Err(FlowLayoutError::BudgetExceeded("list gutter identities")); }
                    entry.insert(MIN_GUTTER);
                }
            }
        }
        if widths.is_empty() { return Ok(ListGutters { widths }); }
        // Borrow marker strings; do not clone them or retain glyph arrays.
        // Repeated bullets/numbers reuse one metric within this layout only.
        let mut measured = HashMap::<&'b str, f32>::new();
        for meta in emitted.iter().chain(pending) {
            let Some(frame) = meta.list_path.last() else { continue; };
            let Some(width) = widths.get_mut(&frame.list_id) else { continue; };
            let Some(marker) = meta.marker.as_deref() else { continue; };
            let advance = if meta.task.is_some() {
                CHECKBOX_WIDTH
            } else if let Some(advance) = measured.get(marker) {
                *advance
            } else {
                if measured.len() >= self.options.max_items {
                    return Err(FlowLayoutError::BudgetExceeded("list marker metrics"));
                }
                let advance = self.shaped(marker, self.options.body_size, FlowTextRole::Marker)?.total_advance;
                measured.insert(marker, advance);
                advance
            };
            *width = (*width).max(gutter_for(advance));
        }
        Ok(ListGutters { widths })
    }
}

// Round outward: adding the gap in f32 can otherwise round down, making the
// final marker fail its own exact width check despite identical shaper output.
fn gutter_for(advance: f32) -> f32 {
    let mut gutter = (advance + MARKER_GAP).max(MIN_GUTTER);
    if f64::from(gutter) < f64::from(advance) + f64::from(MARKER_GAP) {
        gutter = f32::from_bits(gutter.to_bits() + 1);
    }
    gutter
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use super::super::{FlowLayoutOptions, tests::measured};
    use crate::display::{DisplayItem, DisplayList, DisplayTextRun, VectorShapeType};
    use crate::flow_display::ResumableFlowDisplay;

    fn engine(source: &str, batch: usize) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        engine
    }

    fn options(width: f32) -> FlowLayoutOptions {
        FlowLayoutOptions { viewport_width: width, ..FlowLayoutOptions::default() }
    }

    fn text<'a>(list: &'a DisplayList, value: &str) -> &'a DisplayTextRun {
        list.items().iter().find_map(|item| match item {
            DisplayItem::Text(run) if run.text == value => Some(run), _ => None,
        }).unwrap()
    }

    #[test]
    fn ordered_markers_share_measured_gutters_across_digit_transitions() {
        let list = engine("9. nine\n10. ten\n11. eleven", 1)
            .to_shaped_display_list(options(200.0), measured).unwrap();
        for word in ["nine", "ten", "eleven"] { assert_eq!(text(&list, word).bounds.x, 34.0); }
        for marker in ["9.", "10.", "11."] {
            let bounds = text(&list, marker).bounds;
            assert!(bounds.x >= 0.0);
            assert_eq!(bounds.right(), 30.0);
        }
    }

    #[test]
    fn nested_lists_sum_ancestor_gutters_without_borrowing_the_parent_marker_space() {
        let source = "99. root\n\n    1000. child\n\n100. last";
        let list = engine(source, 1).to_shaped_display_list(options(240.0), measured).unwrap();
        assert_eq!(text(&list, "root").bounds.x, 44.0);
        assert_eq!(text(&list, "last").bounds.x, 44.0);
        assert_eq!(text(&list, "child").bounds.x, 98.0);
        assert_eq!(text(&list, "1000.").bounds.x, 44.0);
        assert_eq!(text(&list, "1000.").bounds.right(), 94.0);
    }

    #[test]
    fn separate_lists_and_quotes_do_not_share_gutter_ownership() {
        let source = "1000. wide\n\nbreak\n\n1. narrow\n\n> 1. quoted";
        let list = engine(source, 8).to_shaped_display_list(options(240.0), measured).unwrap();
        assert_eq!(text(&list, "wide").bounds.x, 54.0);
        assert_eq!(text(&list, "break").bounds.x, 0.0);
        assert_eq!(text(&list, "narrow").bounds.x, 24.0);
        assert_eq!(text(&list, "quoted").bounds.x, 40.0);
    }

    #[test]
    fn code_tables_images_and_continuation_paragraphs_inherit_their_item_gutter() {
        let source = "10. paragraph\n\n    continuation\n\n    ```text\n    code\n    ```\n\n    | H |\n    | --- |\n    | cell |\n\n    ![image](pic.png)";
        let list = engine(source, 1).to_shaped_display_list(options(200.0), measured).unwrap();
        assert_eq!(text(&list, "paragraph").bounds.x, 34.0);
        assert_eq!(text(&list, "continuation").bounds.x, 34.0);
        assert_eq!(text(&list, "code").bounds.x, 34.0);
        assert!(list.reading_order().len() >= 6);
        assert!(list.reading_order().iter().all(|node| node.bounds.x == 34.0));
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Image(image) if image.bounds.x == 34.0)));
    }

    #[test]
    fn pending_siblings_fix_the_gutter_before_the_first_item_is_presented() {
        let source = "9. nine\n10. ten\n11. eleven";
        let expected = engine(source, usize::MAX).to_shaped_display_list(options(200.0), measured).unwrap();
        let mut partial = ResumableFlowDisplay::new(source, 1);
        while partial.blocks().is_empty() { partial.step().unwrap(); }
        assert_eq!(partial.blocks().len(), 1);
        let first = partial.to_shaped_display_list(options(200.0), measured).unwrap();
        assert_eq!(text(&first, "nine").bounds, text(&expected, "nine").bounds);
        partial.process_all().unwrap();
        assert_eq!(partial.to_shaped_display_list(options(200.0), measured).unwrap(), expected);
        for batch in [2, 7] {
            assert_eq!(engine(source, batch).to_shaped_display_list(options(200.0), measured).unwrap(), expected);
        }
    }

    #[test]
    fn wholly_unshown_lists_do_not_trigger_marker_shaping() {
        let mut partial = ResumableFlowDisplay::new("intro\n\n999999999. hidden", 1);
        while partial.blocks().is_empty() { partial.step().unwrap(); }
        let list = partial.to_shaped_display_list(options(80.0), |text, size, role| {
            assert_ne!(role, FlowTextRole::Marker);
            measured(text, size, role)
        }).unwrap();
        assert_eq!(text(&list, "intro").bounds.x, 0.0);
    }

    #[test]
    fn checkbox_gutters_use_vector_widths_and_leave_bullets_unchanged() {
        let list = engine("- [x] done\n- [ ] todo", 1)
            .to_shaped_display_list(options(160.0), |text, size, role| {
                assert_ne!(role, FlowTextRole::Marker);
                measured(text, size, role)
            }).unwrap();
        assert_eq!(text(&list, "done").bounds.x, 20.0);
        assert_eq!(text(&list, "todo").bounds.x, 20.0);
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Vector(path)
            if path.shape == VectorShapeType::CheckboxOutline && path.bounds.x == 0.0)));
        let bullet = engine("- ordinary", 1).to_shaped_display_list(options(160.0), measured).unwrap();
        assert_eq!(text(&bullet, "ordinary").bounds.x, 20.0);
    }

    #[test]
    fn marker_measurement_budgets_and_failures_leave_the_engine_unchanged() {
        let engine = engine("10. item", 1);
        let before = engine.to_display_list();
        for opts in [FlowLayoutOptions { max_shape_bytes: 1, ..options(160.0) },
            FlowLayoutOptions { max_total_shape_bytes: 2, ..options(160.0) },
            FlowLayoutOptions { max_items: 0, ..options(160.0) }] {
            assert!(matches!(engine.to_shaped_display_list(opts, measured), Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
        assert!(matches!(engine.to_shaped_display_list(options(160.0), |_, _, _| Err("font unavailable".to_owned())),
            Err(FlowLayoutError::Shaping(_))));
        assert!(matches!(engine.to_shaped_display_list(options(30.0), measured), Err(FlowLayoutError::InvalidOptions)));
        assert_eq!(engine.to_display_list(), before);
    }

    #[test]
    fn final_marker_metrics_cannot_expand_into_an_ancestor_gutter() {
        let engine = engine("- parent\n  - child", 1);
        let mut calls = 0;
        let result = engine.to_shaped_display_list(options(160.0), |text, size, role| {
            let mut run = measured(text, size, role)?;
            if role == FlowTextRole::Marker {
                calls += 1;
                // Prepass and parent fit. The child's changed metrics must be
                // checked against its own 16-point slot, not global indentation.
                if calls == 3 {
                    for cluster in &mut run.clusters { cluster.x_start *= 3.0; cluster.x_end *= 3.0; }
                    for glyph in &mut run.glyphs { glyph.x_advance *= 3.0; }
                    run.total_advance *= 3.0;
                }
            }
            Ok(run)
        });
        assert!(matches!(result, Err(FlowLayoutError::ClusterTooWide { advance: 30.0, available: 16.0 })));
    }

    #[test]
    fn bundled_numbered_lists_and_styled_links_use_the_measured_gutter() {
        let engine = engine("98. first\n99. second\n100. third", 1);
        let fonts = crate::fonts::BundledFlowFonts::new(crate::theme::FontFamily::Sans).unwrap();
        let list = fonts.render(&engine, options(240.0)).unwrap();
        let x = text(&list, "first").bounds.x;
        assert!(x >= 20.0);
        assert_eq!(text(&list, "second").bounds.x, x);
        assert_eq!(text(&list, "third").bounds.x, x);
        for marker in ["98.", "99.", "100."] {
            assert!(text(&list, marker).bounds.x >= 0.0);
            assert!(text(&list, marker).bounds.right() <= x);
        }
        let mut linked = ResumableFlowDisplay::new("12. [alpha](#dest)", 1);
        linked.process_all().unwrap();
        let list = linked.to_styled_display_list(options(160.0), |text, size, role, _| measured(text, size, role)).unwrap();
        let bounds = text(&list, "alpha").bounds;
        assert_eq!(bounds.x, 34.0);
        assert_eq!(list.link_at_point(bounds.x + 1.0, bounds.y + 1.0).unwrap().anchor_id, "#dest");
    }

    #[test]
    fn fractional_gutters_round_outward_instead_of_rejecting_their_own_markers() {
        for advance in [0.0, 16.000002, 27.999998, 31.999998, 63.999996, 101.12345] {
            let gutter = gutter_for(advance);
            assert!(gutter >= MIN_GUTTER);
            assert!(f64::from(gutter) >= f64::from(advance) + f64::from(MARKER_GAP));
            assert!(gutter - MARKER_GAP >= advance);
        }
    }
}
