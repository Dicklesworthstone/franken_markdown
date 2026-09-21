//! Mixed-face wrapping and interaction geometry. The parent owns all shaper,
//! line and output admission, so the rich path cannot bypass those budgets.

use super::{Align, FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow};
use crate::display::{DisplayItem, DisplayRect, DisplaySemanticAnchor, DisplayTextRun, VectorShapeType};
use crate::flow_display::FlowInlineRun;
use crate::span::SourceSpan;
use crate::text::{Direction, OwnedTextRun};
use std::ops::Range;

struct Cluster {
    bytes: Range<usize>,
    run_index: usize,
    advance: f32,
    word_break: bool,
}

struct Fragment {
    run: OwnedTextRun,
    run_index: usize,
    size: f32,
}

impl<F> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn inline_text(
        &mut self, text: &str, runs: &[FlowInlineRun], x: f32, y: f32, width: f32,
        size: f32, leading: f32, role: FlowTextRole, color: &str, span: SourceSpan,
    ) -> Result<f32, FlowLayoutError> {
        // Keep the short unstyled path unchanged. Long paragraphs need bounded
        // measurement windows, not a larger per-call budget. Coalesce default
        // spans just as the legacy path does, so they cannot split ligatures.
        let plain = runs.is_empty()
            || runs.iter().all(|r| r.style == FlowInlineStyle::default() && r.link.is_none());
        if plain && text.split_terminator('\n').all(|line| line.len() <= self.options.max_shape_bytes) {
            return self.text(text, x, y, width, size, leading, role, color, span);
        }
        let unstyled = [FlowInlineRun {
            range: 0..text.len(), style: FlowInlineStyle::default(), link: None,
        }];
        let runs = if plain { &unstyled[..] } else { runs };
        validate_ranges(text, runs)?;
        let leading = if runs.iter().any(|run| run.style.code) {
            leading.max(self.inline_size(size, FlowInlineStyle { code: true, ..FlowInlineStyle::default() }))
        } else { leading };
        let mut height = 0.0;
        let mut offset = 0;
        for physical in text.split_terminator('\n') {
            if physical.is_empty() {
                self.line()?;
                height += leading;
                offset += 1;
                continue;
            }
            let physical_end = offset + physical.len();
            let mut window_start = offset;
            while window_start < physical_end {
                let window_end = measurement_window_end(text, window_start, physical_end,
                    self.options.max_shape_bytes)?;
                let first_style = runs.partition_point(|run| run.range.end <= window_start);
                let mut clusters = Vec::new();
                for (index, spec) in runs.iter().enumerate().skip(first_style) {
                    if spec.range.start >= window_end { break; }
                    let start = spec.range.start.max(window_start);
                    let end = spec.range.end.min(window_end);
                    if start == end { continue; }
                    let run_size = self.inline_size(size, spec.style);
                    let measured = self.shaped_with_style(&text[start..end], run_size, role, spec.style)?;
                    let mut logical: Vec<_> = measured.clusters.iter().collect();
                    logical.sort_by_key(|cluster| cluster.byte_range.start);
                    for cluster in logical {
                        let bytes = start + cluster.byte_range.start..start + cluster.byte_range.end;
                        let word_break = text[bytes.clone()].chars().all(|c| c == ' ' || c == '\t');
                        clusters.push(Cluster { bytes, run_index: index, advance: cluster.advance(), word_break });
                    }
                }
                let mut first = 0;
                while first < clusters.len() {
                    let mut end = first;
                    let mut advance = 0.0;
                    let mut word_break = None;
                    while end < clusters.len() {
                        let next = advance + clusters[end].advance;
                        if next > width && end > first { break; }
                        advance = next;
                        if clusters[end].word_break { word_break = Some(end + 1); }
                        end += 1;
                        if next > width { break; }
                    }
                    // A measurement boundary is NOT a line break. Carry the last
                    // tentative line into the next window, with enough context to
                    // measure the next word. This also keeps the boundary's last
                    // shaped cluster out of published output until it is remeasured.
                    if end == clusters.len() && window_end < physical_end {
                        if first == 0 {
                            return Err(FlowLayoutError::BudgetExceeded("shaping window cannot settle a line"));
                        }
                        break;
                    }
                    if end < clusters.len() { end = word_break.unwrap_or(end); }
                    // Re-shape every final fragment. Kerning/ligatures may change
                    // at a line boundary; never trust only the full-run measurement.
                    let (fragments, final_width) = loop {
                        let fragments = self.fragments(text, runs, &clusters[first..end], size, role)?;
                        let final_width: f32 = fragments.iter().map(|part| part.run.total_advance).sum();
                        if !final_width.is_finite() { return Err(FlowLayoutError::InvalidShapedRun); }
                        if final_width <= width { break (fragments, final_width); }
                        if end == first + 1 {
                            return Err(FlowLayoutError::ClusterTooWide { advance: final_width, available: width });
                        }
                        end -= 1;
                    };
                    let Some(initial) = fragments.first() else { return Err(FlowLayoutError::InvalidShapedRun); };
                    let direction = initial.run.context.direction;
                    if fragments.iter().any(|part| part.run.context.direction != direction) {
                        return Err(FlowLayoutError::Shaping(
                            "mixed-direction styled line requires paragraph-level bidi resolution".to_owned(),
                        ));
                    }
                    self.line()?;
                    let rtl = direction == Direction::RightToLeft;
                    // Align the complete final line before emitting any fragments,
                    // so glyphs, strike marks and link targets share one placement.
                    // Preserve the exact existing RTL right edge without a lossy
                    // subtract/add round trip when alignment is unspecified.
                    let mut cursor = if rtl && matches!(self.alignment, Align::None | Align::Right) {
                        x + width
                    } else {
                        let start = self.line_start(x, width, final_width, direction);
                        if rtl { start + final_width } else { start }
                    };
                    for fragment in fragments {
                        let spec = &runs[fragment.run_index];
                        let advance = fragment.run.total_advance;
                        if rtl { cursor -= advance; }
                        let bounds = DisplayRect::new(cursor, y + height, advance, leading);
                        // Owned runs identify the actual face; color_role describes
                        // ink rather than pretending to encode the complete style.
                        let ink = if spec.active_link_target().is_some() { "link" }
                            else if spec.style.code { "code" } else { color };
                        self.item(DisplayItem::Text(DisplayTextRun {
                            bounds, text: fragment.run.logical_text.clone(), font_run: Some(fragment.run),
                            color_role: ink.to_owned(), source_span: span, font_size: fragment.size,
                        }))?;
                        if spec.style.strikethrough && advance > 0.0 {
                            self.vector(DisplayRect::new(cursor, y + height + leading * 0.5, advance, 1.0),
                                VectorShapeType::HorizontalRule, "strikethrough", span)?;
                        }
                        if let Some(target) = spec.active_link_target() {
                            self.link_anchor(target, bounds, span)?;
                        }
                        if !rtl { cursor += advance; }
                    }
                    debug_assert!(final_width <= width);
                    height += leading;
                    first = end;
                }
                window_start = clusters.get(first).map_or(window_end, |cluster| cluster.bytes.start);
            }
            offset = physical_end + 1;
        }
        if height == 0.0 { self.line()?; height = leading; }
        Ok(height)
    }

    fn inline_size(&self, size: f32, style: FlowInlineStyle) -> f32 {
        if style.code { self.options.code_size * (size / self.options.body_size) } else { size }
    }

    fn fragments(
        &mut self, text: &str, runs: &[FlowInlineRun], clusters: &[Cluster], size: f32, role: FlowTextRole,
    ) -> Result<Vec<Fragment>, FlowLayoutError> {
        let mut out = Vec::new();
        let mut cursor = 0;
        while cursor < clusters.len() {
            let index = clusters[cursor].run_index;
            let start = clusters[cursor].bytes.start;
            let mut end = clusters[cursor].bytes.end;
            cursor += 1;
            while cursor < clusters.len() && clusters[cursor].run_index == index {
                end = clusters[cursor].bytes.end;
                cursor += 1;
            }
            let spec = &runs[index];
            let size = self.inline_size(size, spec.style);
            let run = self.shaped_with_style(&text[start..end], size, role, spec.style)?;
            out.push(Fragment { run, run_index: index, size });
        }
        Ok(out)
    }

    pub(super) fn link_anchor(&mut self, target: &str, bounds: DisplayRect, span: SourceSpan)
        -> Result<(), FlowLayoutError>
    {
        if bounds.width == 0.0 || bounds.height == 0.0 { return Ok(()); }
        self.link_bytes = self.link_bytes.checked_add(target.len())
            .filter(|bytes| *bytes <= self.options.max_total_shape_bytes)
            .ok_or(FlowLayoutError::BudgetExceeded("link target bytes"))?;
        self.item(DisplayItem::Anchor(DisplaySemanticAnchor {
            bounds, anchor_id: target.to_owned(), is_heading: false, level: 0, source_span: span,
        }))
    }
}

// Cut BEFORE an ASCII separator, never through a UTF-8 scalar or a word.
// The final tentative line is carried, not emitted at this boundary. Keeping
// only one window's clusters bounds scratch space even for millions of tiny
// style spans. Words/lines too large for the window still fail explicitly;
// silently splitting unknown shaper clusters or inventing breaks is not safe.
fn measurement_window_end(text: &str, start: usize, end: usize, limit: usize)
    -> Result<usize, FlowLayoutError>
{
    if end - start <= limit { return Ok(end); }
    text.as_bytes()[start..start + limit].iter()
        .rposition(|&byte| matches!(byte, b' ' | b'\t'))
        .filter(|offset| *offset > 0)
        .map(|offset| start + offset)
        .ok_or(FlowLayoutError::BudgetExceeded("word exceeds shaping window"))
}

fn validate_ranges(text: &str, runs: &[FlowInlineRun]) -> Result<(), FlowLayoutError> {
    let mut end = 0;
    for run in runs {
        if run.range.start != end || run.range.start >= run.range.end || text.get(run.range.clone()).is_none() {
            return Err(FlowLayoutError::InvalidShapedRun);
        }
        end = run.range.end;
    }
    if end != text.len() { return Err(FlowLayoutError::InvalidShapedRun); }
    Ok(())
}

impl crate::display::DisplayList {
    /// Hit-test active inline/image links, ignoring non-interactive heading
    /// destinations and decoration overlays. The target is data for the host's
    /// authorization/navigation layer; this operation never opens the link.
    #[must_use]
    pub fn link_at_point(&self, x: f32, y: f32) -> Option<&DisplaySemanticAnchor> {
        self.anchors().filter(|anchor| !anchor.is_heading && anchor.bounds.contains_point(x, y)).last()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use super::super::{FlowLayoutOptions, tests::measured};
    use crate::flow_display::ResumableFlowDisplay;
    use crate::text::FontId;

    // Synthetic face metrics make lost style, wrong font identity, and
    // measurement/render disagreement observable. This is not a font oracle.
    fn shape(text: &str, size: f32, role: FlowTextRole, style: FlowInlineStyle) -> Result<OwnedTextRun, String> {
        let mut run = measured(text, size, role)?;
        let factor = if style.bold { 1.5 } else if style.code { 0.8 } else { 1.0 };
        let font = FontId::new(1 + u64::from(style.bold) + 2 * u64::from(style.italic) + 4 * u64::from(style.code));
        run.context.font_id = font;
        for cluster in &mut run.clusters {
            cluster.font_id = font;
            cluster.x_start *= factor;
            cluster.x_end *= factor;
        }
        for glyph in &mut run.glyphs { glyph.font_id = font; glyph.x_advance *= factor; }
        run.total_advance *= factor;
        Ok(run)
    }

    fn engine(source: &str, batch: usize) -> ResumableFlowDisplay {
        let mut engine = ResumableFlowDisplay::new(source, batch);
        engine.process_all().unwrap();
        engine
    }

    fn options(width: f32) -> FlowLayoutOptions {
        FlowLayoutOptions { viewport_width: width, ..FlowLayoutOptions::default() }
    }

    fn text_items(list: &crate::display::DisplayList) -> Vec<&DisplayTextRun> {
        list.items().iter().filter_map(|item| match item { DisplayItem::Text(run) => Some(run), _ => None }).collect()
    }

    #[test]
    fn mixed_faces_share_lines_and_wrap_using_their_actual_advances() {
        let engine = engine("aa **bb** cc", 1);
        let list = engine.to_styled_display_list(options(70.0), shape).unwrap();
        let runs = text_items(&list);
        assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "aa bb cc");
        let bold = runs.iter().find(|run| run.text == "bb").unwrap();
        assert_eq!(bold.bounds.x, 30.0);
        assert_eq!(bold.bounds.width, 30.0);
        assert_eq!(bold.bounds.y, 0.0);
        assert_eq!(bold.font_run.as_ref().unwrap().context.font_id, FontId::new(2));
        assert_eq!(runs.last().unwrap().text, "cc");
        assert_eq!(runs.last().unwrap().bounds.y, 20.0);
        assert!(runs.iter().all(|run| run.bounds.right() <= 70.0));
        assert_eq!(list.reading_order()[0].text, "aa bb cc");
        assert_eq!(list.reading_order()[0].bounds.height, 40.0);
    }

    #[test]
    fn font_changes_do_not_introduce_word_breaks_or_split_shaped_clusters() {
        let list = engine("ab**cd**ef", 2).to_styled_display_list(options(70.0), shape).unwrap();
        assert!(text_items(&list).iter().all(|run| run.bounds.y == 0.0));
        for source in ["**fi**fi", "**a\u{0301}**b", "*東京*é", "**😀**é"] {
            let list = engine(source, 1).to_styled_display_list(options(15.0), shape).unwrap();
            for run in text_items(&list) {
                assert!(!run.text.starts_with('\u{0301}'));
                assert_ne!(run.text, "f");
                assert_ne!(run.text, "i");
                assert!(run.bounds.width <= 15.0);
            }
        }
    }

    #[test]
    fn inline_and_image_links_are_clickable_at_their_actual_wrapped_bounds() {
        let source = "# [Heading](#dest)\n\nbefore [alpha **beta** gamma](https://example.com) after\n\n[![image](x.png)](#dest)";
        let list = engine(source, 1).to_styled_display_list(options(80.0), shape).unwrap();
        let links: Vec<_> = list.anchors().filter(|anchor| !anchor.is_heading).collect();
        assert!(links.len() >= 4);
        for link in &links {
            let hit = list.link_at_point(link.bounds.x + link.bounds.width * 0.5,
                link.bounds.y + link.bounds.height * 0.5).unwrap();
            assert_eq!(hit.anchor_id, link.anchor_id);
            assert!(!hit.source_span.is_empty());
            assert!(hit.bounds.right() <= 80.0);
        }
        assert!(list.link_at_point(1000.0, 1000.0).is_none());
        assert!(list.anchors().any(|anchor| anchor.is_heading && anchor.anchor_id == "heading"));
    }

    #[test]
    fn blocked_links_remain_readable_and_code_links_never_activate() {
        let source = "[bad](javascript:alert) [local](file:///private) `[literal](https://example.com)`";
        let list = engine(source, 2).to_styled_display_list(options(800.0), shape).unwrap();
        assert_eq!(list.anchors().filter(|anchor| !anchor.is_heading).count(), 0);
        let joined = text_items(&list).iter().map(|run| run.text.as_str()).collect::<String>();
        assert_eq!(joined, "bad local [literal](https://example.com)");
        assert!(text_items(&list).iter().any(|run| run.font_run.as_ref().unwrap().context.font_id == FontId::new(5)));
    }

    #[test]
    fn nested_fonts_strikes_and_table_cell_styles_survive_to_display_items() {
        let source = "***both*** ~~gone~~ `mono`\n\n| *head* |\n| --- |\n| **cell** |";
        let list = engine(source, 1).to_styled_display_list(options(200.0), shape).unwrap();
        let runs = text_items(&list);
        let both = runs.iter().find(|run| run.text == "both").unwrap();
        assert_eq!(both.font_run.as_ref().unwrap().context.font_id, FontId::new(4));
        let mono = runs.iter().find(|run| run.text == "mono").unwrap();
        assert_eq!(mono.font_size, 13.0);
        assert_eq!(mono.color_role, "code");
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Vector(path) if path.color_role == "strikethrough")));
        assert!(runs.iter().any(|run| run.text == "head" && run.font_run.as_ref().unwrap().context.font_id == FontId::new(3)));
        assert!(runs.iter().any(|run| run.text == "cell" && run.font_run.as_ref().unwrap().context.font_id == FontId::new(2)));
    }

    #[test]
    fn styled_output_budgets_and_shaping_failures_publish_nothing() {
        let engine = engine("[**bold**](https://example.com)", 1);
        let before = engine.to_display_list();
        assert!(matches!(engine.to_styled_display_list(FlowLayoutOptions { max_items: 1, ..options(200.0) }, shape),
            Err(FlowLayoutError::BudgetExceeded(_))));
        assert!(matches!(engine.to_styled_display_list(FlowLayoutOptions { max_total_shape_bytes: 10, ..options(200.0) }, shape),
            Err(FlowLayoutError::BudgetExceeded(_))));
        assert!(matches!(engine.to_styled_display_list(options(200.0), |_, _, _, _| Err("unavailable face".to_owned())),
            Err(FlowLayoutError::Shaping(_))));
        assert_eq!(engine.to_display_list(), before);
    }

    #[test]
    fn right_to_left_styled_fragments_keep_logical_order_and_reverse_visual_placement() {
        let list = engine("אב**גד**", 1).to_styled_display_list(options(60.0), |text, size, role, style| {
            let mut run = shape(text, size, role, style)?;
            run.context.direction = Direction::RightToLeft;
            for cluster in &mut run.clusters {
                let start = cluster.x_start;
                cluster.x_start = run.total_advance - cluster.x_end;
                cluster.x_end = run.total_advance - start;
            }
            Ok(run)
        }).unwrap();
        let runs = text_items(&list);
        assert_eq!(runs[0].text, "אב");
        assert_eq!(runs[1].text, "גד");
        assert_eq!(runs[0].bounds.right(), 60.0);
        assert_eq!(runs[1].bounds.right(), runs[0].bounds.x);
        let mixed = engine("a **b**", 1).to_styled_display_list(options(100.0), |text, size, role, style| {
            let mut run = shape(text, size, role, style)?;
            if style.bold { run.context.direction = Direction::RightToLeft; }
            Ok(run)
        });
        assert!(matches!(mixed, Err(FlowLayoutError::Shaping(message)) if message.contains("bidi")));
    }

    #[test]
    fn styled_display_is_deterministic_across_emission_steps_and_reflows() {
        let source = "## *Title*\n\n- **bold** and [link](#title)\n- `code`\n\n| Header |\n| --- |\n| ~~cell~~ |";
        let whole = engine(source, usize::MAX);
        let expected = whole.to_styled_display_list(options(160.0), shape).unwrap();
        assert_eq!(whole.to_styled_display_list(options(160.0), shape).unwrap(), expected);
        for batch in [1, 2, 3, 8] {
            assert_eq!(engine(source, batch).to_styled_display_list(options(160.0), shape).unwrap(), expected);
        }
        // Existing callers still get their prior, explicitly unstyled API.
        let plain = whole.to_shaped_display_list(options(160.0), measured).unwrap();
        assert_eq!(plain.anchors().filter(|anchor| !anchor.is_heading).count(), 0);
    }

    #[test]
    fn centered_cell_faces_links_and_strikes_share_final_line_geometry() {
        let source = "| H |\n| :---: |\n| [aa **bb** ~~cc~~](https://example.com) |\n";
        let list = engine(source, 1).to_styled_display_list(options(200.0), shape).unwrap();
        let row = &list.reading_order()[1];
        let runs: Vec<_> = text_items(&list).into_iter()
            .filter(|run| run.bounds.y >= row.bounds.y).collect();
        assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "aa bb cc");
        assert_eq!(runs.iter().map(|run| run.bounds.width).sum::<f32>(), 90.0);
        assert_eq!(runs.first().unwrap().bounds.x, 55.0);
        assert_eq!(runs.last().unwrap().bounds.right(), 145.0);
        for run in &runs {
            assert_eq!(run.bounds.y, row.bounds.y + 4.0);
            let link = list.link_at_point(run.bounds.x + run.bounds.width * 0.5,
                run.bounds.y + run.bounds.height * 0.5).unwrap();
            assert_eq!(link.anchor_id, "https://example.com");
            assert_eq!(link.bounds, run.bounds);
        }
        let struck = runs.iter().find(|run| run.text == "cc").unwrap();
        assert!(list.items().iter().any(|item| matches!(item, DisplayItem::Vector(path)
            if path.color_role == "strikethrough" && path.bounds.x == struck.bounds.x
                && path.bounds.width == struck.bounds.width)));
        assert!(list.link_at_point(5.0, row.bounds.y + 10.0).is_none());
    }

    #[test]
    fn right_aligned_wrapped_links_remain_clickable_on_every_line() {
        let source = "| H |\n| ---: |\n| [alpha **beta** gamma](#dest) |\n\nafter";
        let expected = engine(source, usize::MAX).to_styled_display_list(options(73.0), shape).unwrap();
        for batch in [1, 2, 8] {
            let list = engine(source, batch).to_styled_display_list(options(73.0), shape).unwrap();
            assert_eq!(list, expected);
            let row = &list.reading_order()[1];
            let runs: Vec<_> = text_items(&list).into_iter().filter(|run|
                run.bounds.y >= row.bounds.y && run.bounds.y < row.bounds.bottom()).collect();
            assert_eq!(runs.iter().map(|run| run.text.as_str()).collect::<String>(), "alpha beta gamma");
            let mut lines = Vec::<(f32, f32)>::new();
            for run in runs {
                let link = list.link_at_point(run.bounds.x + run.bounds.width * 0.5,
                    run.bounds.y + 10.0).unwrap();
                assert_eq!(link.anchor_id, "#dest");
                assert_eq!(link.bounds, run.bounds);
                if let Some(line) = lines.last_mut().filter(|line| line.0 == run.bounds.y) {
                    line.1 = line.1.max(run.bounds.right());
                } else { lines.push((run.bounds.y, run.bounds.right())); }
            }
            assert!(lines.len() > 1);
            assert!(lines.iter().all(|line| line.1 == 69.0));
            let after = text_items(&list).into_iter().find(|run| run.text == "after").unwrap();
            assert_eq!(after.bounds.x, 0.0);
        }
    }

    #[test]
    fn rtl_styled_cells_align_the_whole_line_without_reordering_logical_text() {
        for (delimiter, left) in [(":---", 4.0), (":---:", 50.0), ("---:", 96.0), ("---", 96.0)] {
            let source = format!("| H |\n| {delimiter} |\n| אב**גד** |\n");
            let list = engine(&source, 1).to_styled_display_list(options(150.0), |text, size, role, style| {
                let mut run = shape(text, size, role, style)?;
                run.context.direction = Direction::RightToLeft;
                for cluster in &mut run.clusters {
                    let start = cluster.x_start;
                    cluster.x_start = run.total_advance - cluster.x_end;
                    cluster.x_end = run.total_advance - start;
                }
                Ok(run)
            }).unwrap();
            let row = &list.reading_order()[1];
            let runs: Vec<_> = text_items(&list).into_iter()
                .filter(|run| run.bounds.y >= row.bounds.y).collect();
            assert_eq!(runs.len(), 2);
            assert_eq!(runs[0].text, "אב");
            assert_eq!(runs[1].text, "גד");
            assert_eq!(runs[1].bounds.x, left);
            assert_eq!(runs[0].bounds.x, left + 30.0);
            assert_eq!(runs[0].bounds.right(), left + 50.0);
            assert_eq!(row.children[0].text, "אבגד");
        }
    }

    #[test]
    fn long_plain_paragraphs_use_bounded_windows_in_both_public_apis() {
        let source = "alpha beta fi café ".repeat(1200);
        assert!(source.len() > FlowLayoutOptions::default().max_shape_bytes);
        let engine = engine(&source, 8);
        let expected = engine.to_shaped_display_list(FlowLayoutOptions {
            max_shape_bytes: source.len(), ..options(90.0)
        }, measured).unwrap();
        for limit in [64, 128, 16 * 1024] {
            let opts = FlowLayoutOptions { max_shape_bytes: limit, ..options(90.0) };
            let mut charged = 0;
            let actual = engine.to_shaped_display_list(opts, |text, size, role| {
                assert!(text.len() <= limit);
                charged += text.len();
                measured(text, size, role)
            }).unwrap();
            assert_eq!(actual, expected);
            assert!(charged <= source.len() * 4);
            assert_eq!(engine.to_styled_display_list(opts, shape).unwrap(), expected);
        }
    }

    #[test]
    fn windows_keep_styles_links_and_hard_breaks_without_artificial_lines() {
        let source = format!("{}  \nlast **line**\n\nafter",
            "start [**bold** fi](#target) ~~gone~~ `code` next ".repeat(100));
        let engine = engine(&source, 1);
        let expected = engine.to_styled_display_list(options(95.0), shape).unwrap();
        for limit in [48, 64, 127] {
            let actual = engine.to_styled_display_list(FlowLayoutOptions {
                max_shape_bytes: limit, ..options(95.0)
            }, |text, size, role, style| {
                assert!(text.len() <= limit);
                shape(text, size, role, style)
            }).unwrap();
            assert_eq!(actual, expected);
            for link in actual.anchors().filter(|anchor| !anchor.is_heading) {
                assert_eq!(actual.link_at_point(link.bounds.x + link.bounds.width * 0.5,
                    link.bounds.y + 1.0).unwrap().anchor_id, "#target");
            }
        }
    }

    #[test]
    fn windows_preserve_unicode_clusters_and_do_not_slice_scalars() {
        let source = "fi a\u{0301} 東京 😀 ".repeat(80);
        let engine = engine(&source, 2);
        let expected = engine.to_shaped_display_list(options(50.0), measured).unwrap();
        for limit in [31, 40, 63] {
            let actual = engine.to_shaped_display_list(FlowLayoutOptions {
                max_shape_bytes: limit, ..options(50.0)
            }, measured).unwrap();
            assert_eq!(actual, expected);
            assert_eq!(text_items(&actual).iter().map(|run| run.text.as_str()).collect::<String>(), actual.reading_order()[0].text);
            for run in text_items(&actual) {
                assert!(!run.text.starts_with('\u{0301}'));
                assert_ne!(run.text, "f");
                assert_ne!(run.text, "i");
            }
        }
    }

    #[test]
    fn long_rtl_styled_text_carries_logical_order_across_windows() {
        let source = "אב **גד** דה ".repeat(80);
        let engine = engine(&source, 2);
        let rtl = |text: &str, size, role, style| {
            let mut run = shape(text, size, role, style)?;
            run.context.direction = Direction::RightToLeft;
            for cluster in &mut run.clusters {
                let start = cluster.x_start;
                cluster.x_start = run.total_advance - cluster.x_end;
                cluster.x_end = run.total_advance - start;
            }
            Ok(run)
        };
        let expected = engine.to_styled_display_list(options(70.0), rtl).unwrap();
        let actual = engine.to_styled_display_list(FlowLayoutOptions {
            max_shape_bytes: 43, ..options(70.0)
        }, rtl).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn windowed_reflow_is_stable_across_source_emission_batches() {
        let source = format!("## Heading\n\n{}\n\n> {}", "alpha **beta** gamma ".repeat(100),
            "one two three ".repeat(60));
        let opts = FlowLayoutOptions { max_shape_bytes: 64, ..options(100.0) };
        let expected = engine(&source, usize::MAX).to_styled_display_list(opts, shape).unwrap();
        for batch in [1, 2, 7] {
            assert_eq!(engine(&source, batch).to_styled_display_list(opts, shape).unwrap(), expected);
        }
    }

    #[test]
    fn insufficient_window_context_is_an_error_not_a_fake_line_break() {
        for (source, width) in [("word ".repeat(100), 100_000.0), ("x".repeat(100), 40.0),
            (format!("{}{}", "small ".repeat(40), "x".repeat(70)), 40.0)] {
            let engine = engine(&source, 8);
            let before = engine.to_display_list();
            let result = engine.to_shaped_display_list(FlowLayoutOptions {
                max_shape_bytes: 32, ..options(width)
            }, |text, size, role| {
                assert!(text.len() <= 32);
                measured(text, size, role)
            });
            assert!(matches!(result, Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
    }

    #[test]
    fn windowing_cannot_reset_total_shape_line_or_item_budgets() {
        let engine = engine(&"one **two** three ".repeat(100), 8);
        let before = engine.to_display_list();
        let opts = FlowLayoutOptions { max_shape_bytes: 64, ..options(70.0) };
        for constrained in [FlowLayoutOptions { max_total_shape_bytes: 128, ..opts },
            FlowLayoutOptions { max_lines: 2, ..opts }, FlowLayoutOptions { max_items: 2, ..opts }] {
            assert!(matches!(engine.to_styled_display_list(constrained, shape),
                Err(FlowLayoutError::BudgetExceeded(_))));
            assert_eq!(engine.to_display_list(), before);
        }
        let mut calls = 0;
        let failed = engine.to_styled_display_list(opts, |text, size, role, style| {
            calls += 1;
            if calls == 20 { return Err("late shaper failure".to_owned()); }
            shape(text, size, role, style)
        });
        assert!(matches!(failed, Err(FlowLayoutError::Shaping(message)) if message == "late shaper failure"));
        assert_eq!(calls, 20);
        assert_eq!(engine.to_display_list(), before);
    }

    #[test]
    fn bundled_fonts_render_long_paragraphs_without_raising_default_limits() {
        let source = "office AV café and ordinary prose ".repeat(700);
        let engine = engine(&source, 8);
        let fonts = crate::fonts::BundledFlowFonts::new(crate::theme::FontFamily::Sans).unwrap();
        let expected = fonts.render(&engine, options(240.0)).unwrap();
        assert!(source.len() > FlowLayoutOptions::default().max_shape_bytes);
        assert!(!expected.items().is_empty());
        for limit in [256, 16 * 1024] {
            assert_eq!(fonts.render(&engine, FlowLayoutOptions {
                max_shape_bytes: limit, ..options(240.0)
            }).unwrap(), expected);
        }
    }
}
