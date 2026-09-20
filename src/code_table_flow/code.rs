//! Indexed code-fence layout. Source bytes are retained once; sparse column
//! checkpoints bound work when scrolling through a very long physical line.
//! Geometry uses the existing scalar-cell estimate, not shaped glyph metrics.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use super::{CODE_FONT_SIZE, CODE_LINE_HEIGHT_FACTOR, CodeTableError};
use crate::display::{
    AccessibleReadingNode, AccessibleReadingRole, DisplayClip, DisplayItem, DisplayList,
    DisplayRect, DisplayTextRun,
};
use crate::span::SourceSpan;

const CELL_WIDTH: f32 = CODE_FONT_SIZE * 0.6;
const LINE_HEIGHT: f32 = CODE_FONT_SIZE * CODE_LINE_HEIGHT_FACTOR;
const PADDING_X: f32 = 12.0;
const PADDING_Y: f32 = 8.0;
const TAB_WIDTH: usize = 4;
const CHECKPOINT_COLUMNS: usize = 64;
const MAX_VISIBLE_ROWS: usize = 16_384;
const MAX_VISIBLE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Checkpoint {
    column: usize,
    byte: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    bytes: Range<usize>,
    columns: usize,
    checkpoints: Vec<Checkpoint>,
}

#[derive(Debug)]
struct WrappedRows {
    columns: usize,
    // One prefix per physical line, followed by the total visual row count.
    starts: Vec<usize>,
}

/// A code fence with independent horizontal scrolling and optional wrapping.
///
/// Tabs use four-column stops in display geometry. Clipboard access always
/// returns the original bytes, including tabs, CRLF and trailing newlines.
/// Unicode uses one cell per scalar, matching the previous approximate model;
/// this is not a replacement for font-backed grapheme shaping.
#[derive(Debug)]
pub struct CodeFenceFlow {
    raw_code: String,
    lang: Option<String>,
    source_span: SourceSpan,
    lines: Vec<Line>,
    max_line_columns: usize,
    /// Independent horizontal scroll position in points; ignored when wrapping.
    pub scroll_x: f32,
    /// Wrap at the available content width instead of scrolling horizontally.
    pub wrap_lines: bool,
    // Only the most recently used wrap width is retained. Mutex preserves the
    // existing Send + Sync contract; cached geometry is not semantic state.
    wrapped: Mutex<Option<Arc<WrappedRows>>>,
}

impl Clone for CodeFenceFlow {
    fn clone(&self) -> Self {
        Self {
            raw_code: self.raw_code.clone(),
            lang: self.lang.clone(),
            source_span: self.source_span,
            lines: self.lines.clone(),
            max_line_columns: self.max_line_columns,
            scroll_x: self.scroll_x,
            wrap_lines: self.wrap_lines,
            wrapped: Mutex::new(None),
        }
    }
}

impl PartialEq for CodeFenceFlow {
    fn eq(&self, other: &Self) -> bool {
        self.raw_code == other.raw_code
            && self.lang == other.lang
            && self.source_span == other.source_span
            && self.scroll_x == other.scroll_x
            && self.wrap_lines == other.wrap_lines
    }
}

impl CodeFenceFlow {
    #[must_use]
    pub fn new(lang: Option<String>, code: String, source_span: SourceSpan) -> Self {
        let mut lines = Vec::new();
        let mut offset = 0;
        let mut max_line_columns = 0;
        for chunk in code.split_inclusive('\n') {
            let text = if let Some(text) = chunk.strip_suffix('\n') {
                text.strip_suffix('\r').unwrap_or(text)
            } else {
                chunk
            };
            let mut checkpoints = vec![Checkpoint { column: 0, byte: offset }];
            let mut columns = 0;
            let mut last_checkpoint = 0;
            for (byte, ch) in text.char_indices() {
                if columns - last_checkpoint >= CHECKPOINT_COLUMNS {
                    checkpoints.push(Checkpoint { column: columns, byte: offset + byte });
                    last_checkpoint = columns;
                }
                columns += if ch == '\t' { TAB_WIDTH - columns % TAB_WIDTH } else { 1 };
            }
            max_line_columns = max_line_columns.max(columns);
            lines.push(Line { bytes: offset..offset + text.len(), columns, checkpoints });
            offset += chunk.len();
        }
        if lines.is_empty() {
            lines.push(Line {
                bytes: 0..0,
                columns: 0,
                checkpoints: vec![Checkpoint { column: 0, byte: 0 }],
            });
        }
        Self {
            raw_code: code,
            lang,
            source_span,
            lines,
            max_line_columns,
            scroll_x: 0.0,
            wrap_lines: false,
            wrapped: Mutex::new(None),
        }
    }

    /// Authoritative source bytes, without visual wrapping or tab expansion.
    #[must_use]
    pub fn exact_code_copy(&self) -> &str {
        &self.raw_code
    }

    #[must_use]
    pub fn lang(&self) -> Option<&str> {
        self.lang.as_deref()
    }

    /// Physical line count, independent of the current viewport width.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    #[must_use]
    pub fn source_span(&self) -> SourceSpan {
        self.source_span
    }

    #[must_use]
    pub fn intrinsic_content_width(&self) -> f32 {
        (self.max_line_columns as f32 * CELL_WIDTH).max(100.0) + 32.0
    }

    /// Historical unwrapped height. Width-aware hosts should use
    /// [`Self::total_height_for_width`] when wrapping is enabled.
    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.lines.len() as f32 * LINE_HEIGHT + 2.0 * PADDING_Y
    }

    /// Visual line count at this width, reusing the last wrapped line index.
    pub fn visual_line_count(&self, available_width: f32) -> Result<usize, CodeTableError> {
        self.wrap_index(available_width).map(|index| {
            index.as_ref().map_or(self.lines.len(), |index| {
                index.starts.last().copied().unwrap_or(0)
            })
        })
    }

    /// Height consistent with the rows emitted by [`Self::materialize_viewport`].
    pub fn total_height_for_width(&self, available_width: f32) -> Result<f32, CodeTableError> {
        let height = self.visual_line_count(available_width)? as f32 * LINE_HEIGHT
            + 2.0 * PADDING_Y;
        if !height.is_finite() {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        Ok(height)
    }

    fn wrap_index(&self, width: f32) -> Result<Option<Arc<WrappedRows>>, CodeTableError> {
        if !width.is_finite() || width <= 0.0 {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        if !self.wrap_lines {
            return Ok(None);
        }
        let columns = (((width - 2.0 * PADDING_X).max(CELL_WIDTH) / CELL_WIDTH).floor()
            as usize).max(1);
        let mut cache = self.wrapped.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(index) = cache.as_ref().filter(|index| index.columns == columns) {
            return Ok(Some(Arc::clone(index)));
        }
        let mut starts = Vec::with_capacity(self.lines.len() + 1);
        let mut total = 0usize;
        starts.push(total);
        for line in &self.lines {
            let rows = line.columns.div_ceil(columns).max(1);
            total = total.checked_add(rows).ok_or(CodeTableError::ArithmeticOverflow)?;
            starts.push(total);
        }
        let index = Arc::new(WrappedRows { columns, starts });
        *cache = Some(Arc::clone(&index));
        Ok(Some(index))
    }

    /// Materialize a window in the same coordinate space as `bounds.y`.
    ///
    /// Both drawing and accessibility output are windowed. Clipping covers
    /// every emitted item; offscreen and zero-area windows emit nothing. A
    /// viewport may emit at most 16,384 rows and 4 MiB of text. Non-finite or
    /// negative geometry is rejected before float-to-integer conversions.
    /// Horizontal slicing uses sparse checkpoints rather than scanning or
    /// copying a whole long line. Wrapped-row lookup uses a cached prefix index.
    pub fn materialize_viewport(
        &self,
        bounds: DisplayRect,
        viewport_local_top: f32,
        viewport_height: f32,
    ) -> Result<DisplayList, CodeTableError> {
        let values = [bounds.x, bounds.y, bounds.width, bounds.height, bounds.right(),
            bounds.bottom(), viewport_local_top, viewport_height,
            viewport_local_top + viewport_height, self.scroll_x];
        if values.iter().any(|value| !value.is_finite())
            || bounds.width < 0.0 || bounds.height < 0.0
            || viewport_height < 0.0 || self.scroll_x < 0.0
        {
            return Err(CodeTableError::ArithmeticOverflow);
        }
        let mut dl = DisplayList::new();
        let top = bounds.y.max(viewport_local_top);
        let bottom = bounds.bottom().min(viewport_local_top + viewport_height);
        if bounds.width == 0.0 || top >= bottom {
            return Ok(dl);
        }
        let wrapped = self.wrap_index(bounds.width)?;
        let row_count = wrapped.as_ref().map_or(self.lines.len(), |index| {
            index.starts.last().copied().unwrap_or(0)
        });
        let content_top = bounds.y + PADDING_Y;
        let first = (((top - content_top).max(0.0) / LINE_HEIGHT).floor() as usize).min(row_count);
        let end = (((bottom - content_top).max(0.0) / LINE_HEIGHT).ceil() as usize).min(row_count);
        if first >= end {
            return Ok(dl);
        }
        if end - first > MAX_VISIBLE_ROWS {
            return Err(CodeTableError::RowBudgetExceeded { rows: end - first, max: MAX_VISIBLE_ROWS });
        }
        let scroll = if self.wrap_lines { 0.0 } else { self.scroll_x };
        let text_x = bounds.x + PADDING_X - scroll;
        let left_column = (((bounds.x - text_x).max(0.0) / CELL_WIDTH).floor() as usize)
            .min(self.max_line_columns);
        // Retain at most one checkpoint's clipped prefix for small scrolls.
        // This preserves historical text origins without allowing unbounded
        // offscreen copies; deep scrolling still starts at the indexed window.
        let left_column = if left_column < CHECKPOINT_COLUMNS { 0 } else { left_column };
        let right_column = (((bounds.right() - text_x).max(0.0) / CELL_WIDTH).ceil() as usize)
            .min(self.max_line_columns);
        let clip = DisplayRect::new(bounds.x, top, bounds.width, bottom - top);
        dl.push_item(DisplayItem::Clip(DisplayClip { bounds: clip, child_count: end - first }));
        let mut reading = String::new();
        for visual in first..end {
            let (physical, row_start, row_end) = if let Some(index) = wrapped.as_ref() {
                let physical = index.starts.partition_point(|start| *start <= visual).saturating_sub(1);
                let start = (visual - index.starts[physical]) * index.columns;
                (physical, start, start.saturating_add(index.columns))
            } else {
                (visual, 0, self.lines[visual].columns)
            };
            let line = &self.lines[physical];
            let start = row_start.saturating_add(left_column).min(line.columns);
            let end_column = row_start.saturating_add(right_column).min(row_end).min(line.columns);
            let text = self.window_text(line, start..end_column, MAX_VISIBLE_BYTES.saturating_sub(reading.len()))?;
            let separator = usize::from(visual != first);
            let requested = reading.len().saturating_add(separator).saturating_add(text.len());
            if requested > MAX_VISIBLE_BYTES {
                return Err(CodeTableError::OutputBudgetExceeded { bytes: requested, max: MAX_VISIBLE_BYTES });
            }
            if separator != 0 { reading.push('\n'); }
            reading.push_str(&text);
            let x = text_x + left_column as f32 * CELL_WIDTH;
            let y = content_top + visual as f32 * LINE_HEIGHT;
            let width = end_column.saturating_sub(start) as f32 * CELL_WIDTH;
            if !x.is_finite() || !y.is_finite() || !width.is_finite() {
                return Err(CodeTableError::ArithmeticOverflow);
            }
            dl.push_item(DisplayItem::Text(DisplayTextRun {
                bounds: DisplayRect::new(x, y, width, LINE_HEIGHT),
                text,
                font_run: None,
                color_role: "code".to_string(),
                source_span: self.source_span,
                font_size: CODE_FONT_SIZE,
            }));
        }
        dl.push_reading_node(AccessibleReadingNode {
            role: AccessibleReadingRole::CodeBlock,
            text: reading,
            source_span: self.source_span,
            bounds: clip,
            children: Vec::new(),
        });
        Ok(dl)
    }

    fn window_text(&self, line: &Line, columns: Range<usize>, budget: usize) -> Result<String, CodeTableError> {
        if columns.start >= columns.end { return Ok(String::new()); }
        let index = line.checkpoints.partition_point(|point| point.column <= columns.start).saturating_sub(1);
        let point = &line.checkpoints[index];
        let mut column = point.column;
        let mut text = String::new();
        for ch in self.raw_code[point.byte..line.bytes.end].chars() {
            if column >= columns.end { break; }
            let advance = if ch == '\t' { TAB_WIDTH - column % TAB_WIDTH } else { 1 };
            let next = column + advance;
            let visible = next.min(columns.end).saturating_sub(column.max(columns.start));
            if visible != 0 {
                let bytes = if ch == '\t' { visible } else { ch.len_utf8() };
                if bytes > budget.saturating_sub(text.len()) {
                    return Err(CodeTableError::OutputBudgetExceeded {
                        bytes: MAX_VISIBLE_BYTES.saturating_add(1), max: MAX_VISIBLE_BYTES,
                    });
                }
                if ch == '\t' {
                    for _ in 0..visible { text.push(' '); }
                } else { text.push(ch); }
            }
            column = next;
        }
        Ok(text)
    }

    /// Explicit complete reading representation for assistive technology.
    /// Unlike viewport rendering, this intentionally copies the entire source.
    #[must_use]
    pub fn full_accessible_tree(&self, bounds: DisplayRect) -> AccessibleReadingNode {
        AccessibleReadingNode {
            role: AccessibleReadingRole::CodeBlock,
            text: self.raw_code.clone(),
            source_span: self.source_span,
            bounds,
            children: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn flow(code: &str) -> CodeFenceFlow {
        CodeFenceFlow::new(Some("rust".into()), code.into(), SourceSpan::new(10, 20))
    }

    fn text(dl: &DisplayList) -> String {
        dl.items().iter().filter_map(|item| match item {
            DisplayItem::Text(run) => Some(run.text.as_str()), _ => None,
        }).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn wrapping_is_width_aware_unicode_safe_and_preserves_source() {
        let source = "aé中🙂bc\r\n\txy\n";
        let mut code = flow(source);
        code.wrap_lines = true;
        let width = 2.0 * PADDING_X + 3.25 * CELL_WIDTH;
        assert_eq!(code.visual_line_count(width).unwrap(), 4);
        let height = code.total_height_for_width(width).unwrap();
        let dl = code.materialize_viewport(DisplayRect::new(0.0, 0.0, width, height), 0.0, height).unwrap();
        assert_eq!(text(&dl), "aé中\n🙂bc\n   \n xy");
        assert_eq!(code.exact_code_copy(), source);
        assert_eq!(code.line_count(), 2);
        assert_eq!(code.full_accessible_tree(DisplayRect::default()).text, source);
        assert_eq!(code.source_span(), SourceSpan::new(10, 20));
    }

    #[test]
    fn width_changes_replace_cache_without_changing_semantic_equality() {
        let mut code = flow("abcdefghij");
        code.wrap_lines = true;
        let copy = code.clone();
        let narrow = 2.0 * PADDING_X + 2.25 * CELL_WIDTH;
        let wide = 2.0 * PADDING_X + 5.25 * CELL_WIDTH;
        assert_eq!(code.visual_line_count(narrow).unwrap(), 5);
        assert_eq!(code.visual_line_count(wide).unwrap(), 2);
        assert_eq!(code.visual_line_count(narrow).unwrap(), 5);
        assert_eq!(code, copy);
        code.wrap_lines = false;
        assert_eq!(code.total_height_for_width(narrow).unwrap(), code.total_height());
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CodeFenceFlow>();
    }

    #[test]
    fn giant_single_line_is_windowed_horizontally_and_for_accessibility() {
        let source = format!("{}VISIBLE{}", "a".repeat(100_000), "z".repeat(100_000));
        let mut code = flow(&source);
        code.scroll_x = 100_000.0 * CELL_WIDTH + PADDING_X;
        let dl = code.materialize_viewport(DisplayRect::new(0.0, 0.0, 200.0, 100.0), 0.0, 100.0).unwrap();
        let displayed = text(&dl);
        assert!(displayed.contains("VISIBLE"));
        assert!(displayed.len() < 40);
        assert!(dl.reading_order()[0].text.len() < 40);
        assert_eq!(code.exact_code_copy(), source);
    }

    #[test]
    fn viewport_does_not_clone_offscreen_rows_and_clip_counts_its_children() {
        let code = flow(&"let value = 42;\n".repeat(10_000));
        let bounds = DisplayRect::new(20.0, 100.0, 200.0, code.total_height());
        let dl = code.materialize_viewport(bounds, 500.0, 100.0).unwrap();
        assert!(dl.items().len() < 10);
        assert!(dl.reading_order()[0].text.len() < 200);
        match &dl.items()[0] {
            DisplayItem::Clip(clip) => {
                assert_eq!(clip.child_count, dl.items().len() - 1);
                assert_eq!(clip.bounds.y, 500.0);
                assert_eq!(clip.bounds.height, 100.0);
            }
            _ => panic!("expected clip"),
        }
        for (top, height) in [(0.0, 50.0), (bounds.bottom() + 1.0, 100.0), (500.0, 0.0)] {
            let empty = code.materialize_viewport(bounds, top, height).unwrap();
            assert!(empty.items().is_empty());
            assert!(empty.reading_order().is_empty());
        }
    }

    #[test]
    fn invalid_geometry_and_materialization_budgets_are_recoverable() {
        let code = flow("example");
        let bounds = DisplayRect::new(0.0, 0.0, 200.0, 100.0);
        for top in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(code.materialize_viewport(bounds, top, 10.0).is_err());
        }
        assert!(code.materialize_viewport(bounds, 0.0, -1.0).is_err());
        assert!(code.total_height_for_width(f32::NAN).is_err());
        let many = flow(&"x\n".repeat(MAX_VISIBLE_ROWS + 1));
        let height = many.total_height();
        assert!(matches!(many.materialize_viewport(DisplayRect::new(0.0, 0.0, 200.0, height), 0.0, height),
            Err(CodeTableError::RowBudgetExceeded { .. })));
        let huge = flow(&"x".repeat(MAX_VISIBLE_BYTES + 1));
        let width = huge.intrinsic_content_width();
        assert!(matches!(huge.materialize_viewport(DisplayRect::new(0.0, 0.0, width, 100.0), 0.0, 100.0),
            Err(CodeTableError::OutputBudgetExceeded { .. })));
    }

    #[test]
    fn empty_lines_crlf_and_trailing_newline_follow_physical_line_contract() {
        for (source, count) in [("", 1), ("\n", 1), ("\n\n", 2), ("a\r\nb\n", 2), ("a\r", 1)] {
            let code = flow(source);
            assert_eq!(code.line_count(), count);
            assert_eq!(code.exact_code_copy(), source);
        }
    }

    #[test]
    fn small_scroll_preserves_origin_without_copying_the_rest_of_a_long_line() {
        let mut code = flow(&"x".repeat(100_000));
        code.scroll_x = 50.0;
        let dl = code.materialize_viewport(DisplayRect::new(0.0, 0.0, 300.0, 100.0), 0.0, 100.0).unwrap();
        let run = dl.items().iter().find_map(|item| match item {
            DisplayItem::Text(run) => Some(run), _ => None,
        }).unwrap();
        assert_eq!(run.bounds.x, -38.0);
        assert!(run.text.len() < 50);
    }
}
