//! Decimal positioning uses the host's final styled cluster geometry. Extra
//! measurements pass through the same per-call and cumulative shape budgets.

use super::{Direction, FlowInlineRun, FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow};
use crate::table_format::{FootingRule, decimal_anchor};
use crate::text::OwnedTextRun;

#[derive(Clone, Copy, Default)]
pub(super) struct DecimalMeasure {
    pub before: f32,
    pub after: f32,
}

pub(super) fn stroke_count(rule: FootingRule) -> usize {
    match rule {
        FootingRule::None => 0,
        FootingRule::Single => 1,
        FootingRule::Double => 2,
    }
}

pub(super) fn footing_height(rules: &[FootingRule]) -> f32 {
    match rules.iter().copied().map(stroke_count).max().unwrap_or(0) {
        0 => 0.0,
        1 => 3.0,
        _ => 5.0,
    }
}

impl<F: ?Sized> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    pub(super) fn decimal_measure(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        role: FlowTextRole,
    ) -> Result<Option<DecimalMeasure>, FlowLayoutError> {
        // Very long cells retain the bounded-window wrapping path. Decimal
        // positioning cannot justify an unbounded extra shaping call.
        if text.len() > self.options.max_shape_bytes {
            return Ok(None);
        }
        let Some(anchor) = decimal_anchor(text) else {
            return Ok(None);
        };
        let plain = runs.is_empty()
            || runs
                .iter()
                .all(|run| run.style == FlowInlineStyle::default() && run.link.is_none());
        let unstyled = [FlowInlineRun {
            range: 0..text.len(),
            style: FlowInlineStyle::default(),
            link: None,
        }];
        let runs = if plain { &unstyled[..] } else { runs };
        let mut advance = 0.0_f32;
        let mut before = None;
        let mut end = 0;
        for spec in runs {
            if spec.range.start != end || spec.range.start >= spec.range.end {
                return Err(FlowLayoutError::InvalidShapedRun);
            }
            let fragment = text
                .get(spec.range.clone())
                .ok_or(FlowLayoutError::InvalidShapedRun)?;
            let run = self.shaped_with_style(fragment, self.options.body_size, role, spec.style)?;
            if run.context.direction != Direction::LeftToRight {
                return Ok(None);
            }
            if before.is_none() && spec.range.contains(&anchor) {
                let local = anchor - spec.range.start;
                let Some(cluster) = run
                    .clusters
                    .iter()
                    .find(|cluster| cluster.byte_range.start == local)
                else {
                    // A host may shape punctuation into an indivisible
                    // cluster. Keep the original text and right-align it.
                    return Ok(None);
                };
                before = Some(advance + cluster.x_start);
            }
            advance += run.total_advance;
            end = spec.range.end;
            if anchor == end && before.is_none() {
                before = Some(advance);
            }
        }
        if end != text.len() || !advance.is_finite() {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        Ok(before.map(|before| DecimalMeasure {
            before,
            after: (advance - before).max(0.0),
        }))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::display::{AccessibleReadingRole, DisplayItem, VectorShapeType};
    use crate::flow_display::{
        DisplayBlock, FlowDisplayLimits, FlowLayoutOptions, ResumableFlowDisplay,
    };
    use crate::text::FontId;

    const SOURCE: &str = "| Item | Amount |\n| --- | ---. |\n\
        | Revenue | 1,234.5 |\n| Costs | (234.50) |\n| | --- |\n\
        | Count | 17 |\n| Linked | [7.00](https://example.com/amount) |\n\
        | Total | **20**.125 |\n| | === |\n";

    fn engine(source: &str, batch: usize) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        engine
    }

    /// Deliberately proportional digits and a distinct bold face make
    /// character-count alignment or unstyled measurement visibly incorrect.
    fn proportional(
        text: &str,
        size: f32,
        role: FlowTextRole,
        style: FlowInlineStyle,
    ) -> Result<OwnedTextRun, String> {
        let mut run = super::super::tests::measured(text, size, role)?;
        let font = FontId::new(if style.bold { 2 } else { 1 });
        run.context.font_id = font;
        let mut x = 0.0;
        for (cluster, glyph) in run.clusters.iter_mut().zip(&mut run.glyphs) {
            let piece = &text[cluster.byte_range.clone()];
            let width = match piece {
                "1" => 4.0,
                "0" => 9.0,
                "." | "," => 3.0,
                _ => 7.0,
            } * if style.bold { 1.25 } else { 1.0 };
            cluster.font_id = font;
            cluster.x_start = x;
            cluster.x_end = x + width;
            glyph.font_id = font;
            glyph.x_advance = width;
            x += width;
        }
        run.total_advance = x;
        Ok(run)
    }

    #[test]
    fn financial_flow_projection_folds_markers_without_losing_source_rows_or_styles() {
        for batch in [1, 3, 100] {
            let engine = engine(SOURCE, batch);
            let rows: Vec<_> = engine
                .blocks
                .iter()
                .filter(|block| matches!(block, DisplayBlock::TableRow { .. }))
                .collect();
            assert_eq!(rows.len(), 5);
            let source_rows: Vec<_> = engine
                .metadata
                .iter()
                .map(|meta| meta.table_source_row)
                .collect();
            assert_eq!(
                source_rows,
                vec![Some(0), Some(1), Some(2), Some(4), Some(5), Some(6)]
            );
            assert_eq!(engine.metadata[2].table_footing[1], FootingRule::Single);
            assert_eq!(engine.metadata[5].table_footing[1], FootingRule::Double);
            assert!(
                engine.metadata[5].cell_runs[1]
                    .iter()
                    .any(|run| run.style.bold)
            );
            assert!(
                engine
                    .metadata
                    .iter()
                    .all(|meta| meta.span == engine.metadata[0].span)
            );
            let estimate = engine.to_display_list();
            assert_eq!(estimate.reading_order().len(), 6);
            assert_eq!(estimate.items().iter().filter(|item| {
                matches!(item, DisplayItem::Vector(path) if path.shape == VectorShapeType::HorizontalRule)
            }).count(), 3);
        }
    }

    #[test]
    fn financial_flow_uses_proportional_styled_clusters_and_preserves_link_hit_boxes() {
        let engine = engine(SOURCE, 2);
        let list = engine
            .to_styled_display_list(
                FlowLayoutOptions {
                    viewport_width: 360.0,
                    ..FlowLayoutOptions::default()
                },
                proportional,
            )
            .unwrap();
        let mut anchors = Vec::new();
        for item in list.items() {
            let DisplayItem::Text(text) = item else {
                continue;
            };
            let run = text.font_run.as_ref().unwrap();
            if let Some(dot) = text.text.find('.') {
                let cluster = run
                    .clusters
                    .iter()
                    .find(|cluster| cluster.byte_range.start == dot)
                    .unwrap();
                anchors.push(text.bounds.x + cluster.x_start);
            } else if text.text == "17" {
                anchors.push(text.bounds.right());
            }
            if text.text == "7.00" {
                assert!(list.items().iter().any(|item| {
                    matches!(item, DisplayItem::Anchor(anchor)
                        if anchor.anchor_id == "https://example.com/amount"
                            && anchor.bounds == text.bounds)
                }));
            }
        }
        assert_eq!(anchors.len(), 5);
        assert!(
            anchors
                .iter()
                .all(|anchor| (anchor - anchors[0]).abs() < 0.01),
            "{anchors:?}"
        );
        let footings: Vec<_> = list
            .items()
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Vector(path) if path.shape == VectorShapeType::HorizontalRule => {
                    Some(path)
                }
                _ => None,
            })
            .collect();
        assert_eq!(footings.len(), 3);
        assert!((footings[2].bounds.y - footings[1].bounds.y - 2.0).abs() < 0.01);
        assert!(footings.iter().all(|path| path.bounds.right() <= 360.0));
        assert_eq!(
            list.reading_order()
                .iter()
                .filter(|node| { node.role == AccessibleReadingRole::TableRow })
                .count(),
            5
        );
    }

    #[test]
    fn financial_flow_pending_rows_share_one_grid_and_step_sizes_do_not_change_output() {
        let opts = FlowLayoutOptions {
            viewport_width: 360.0,
            ..FlowLayoutOptions::default()
        };
        let complete = engine(SOURCE, 100)
            .to_styled_display_list(opts, proportional)
            .unwrap();
        let mut partial = ResumableFlowDisplay::new(SOURCE, 1);
        while partial.blocks.is_empty() {
            partial.step().unwrap();
        }
        let header = partial.to_styled_display_list(opts, proportional).unwrap();
        let count = header.items().len();
        assert_eq!(header.items(), &complete.items()[..count]);
        partial.process_all().unwrap();
        assert_eq!(
            partial.to_styled_display_list(opts, proportional).unwrap(),
            complete
        );
    }

    #[test]
    fn financial_flow_charges_metadata_shape_and_rule_item_budgets_atomically() {
        let limits = FlowDisplayLimits {
            max_output_bytes: 10,
            ..FlowDisplayLimits::default()
        };
        let mut denied = ResumableFlowDisplay::try_with_limits(SOURCE, 100, 1, limits).unwrap();
        assert!(denied.process_all().is_err());
        assert!(denied.blocks.is_empty());

        let engine = engine(SOURCE, 100);
        let mut measured_bytes = 0;
        let options = FlowLayoutOptions {
            viewport_width: 360.0,
            ..FlowLayoutOptions::default()
        };
        let full = engine
            .to_styled_display_list(options, |text, size, role, style| {
                measured_bytes += text.len();
                proportional(text, size, role, style)
            })
            .unwrap();
        let exhausted = FlowLayoutOptions {
            max_total_shape_bytes: measured_bytes - 1,
            ..options
        };
        assert_eq!(
            engine
                .to_styled_display_list(exhausted, proportional)
                .unwrap_err(),
            FlowLayoutError::BudgetExceeded("total shaping bytes")
        );
        let exhausted = FlowLayoutOptions {
            max_items: full.items().len() - 1,
            ..options
        };
        assert_eq!(
            engine
                .to_styled_display_list(exhausted, proportional)
                .unwrap_err(),
            FlowLayoutError::BudgetExceeded("display items")
        );
    }
}
