//! Cluster-boundary emergency wrapping, including contextual boundary repair.

use super::{Poster, RStyle, ShapedText, Shaper, SvgWarning, TextFlow, Word};
use std::ops::Range;

impl Poster {
    /// Preserve code whitespace and four-column source tab stops, while keeping
    /// combining clusters intact and reserving their actual positioned ink.
    pub(in super::super) fn code_words(&self, code: &str, size: f64, width: f64) -> Vec<Word> {
        let style = RStyle { mono: true, ..RStyle::BODY };
        let width = width.max(1.0);
        let shaper = Shaper::new(self);
        let mut lines = Vec::new();
        for source in code.lines() {
            let mut expanded = String::new();
            let mut column = 0usize;
            for ch in source.chars() {
                let count = if ch == '\t' { 4 - column % 4 } else { 1 };
                let ch = if ch == '\t' { ' ' } else { ch };
                for _ in 0..count { expanded.push(ch); }
                column = column.saturating_add(count);
            }
            let prepared = shaper.shape(&expanded, style, size);
            let word = text_word(expanded, style, None, prepared);
            if word.w <= width {
                lines.push(word);
            } else {
                let mut flow = TextFlow::default();
                let mut word = word;
                if let Some(prepared) = word.shaped.take() {
                    place_shaped(&mut flow, word, &prepared, &shaper, size, width);
                }
                if !flow.line.is_empty() { flow.new_line(); }
                lines.extend(flow.lines.into_iter().flatten());
            }
        }
        if lines.is_empty() {
            lines.push(text_word(String::new(), style, None, ShapedText::default()));
        }
        lines
    }

    #[cfg(test)]
    pub(in super::super) fn code_lines(&self, code: &str, size: f64, width: f64) -> Vec<String> {
        self.code_words(code, size, width).into_iter().map(|word| word.text).collect()
    }
}

pub(super) fn place_shaped(
    flow: &mut TextFlow, run: Word, shaped: &ShapedText, shaper: &Shaper<'_>, size: f64, width: f64,
) {
    if shaped.contextual {
        place_contextual(flow, run, shaped, shaper, size, width);
        return;
    }
    let mut start = 0;
    let mut end = 0;
    let mut used = 0.0;
    let mut warning = run.warning;
    while end < shaped.clusters.len() {
        let next = shaped.extend_width(start, end, used);
        if flow.line_width + next > width && (end > start || !flow.line.is_empty()) {
            if start < end {
                let bytes = source_range(shaped, start..end);
                push_prepared(flow, run.text[bytes].to_owned(), run.style,
                    warning.take(), shaped.slice(start..end));
            }
            flow.new_line();
            start = end;
            used = 0.0;
            continue;
        }
        // Preserve a single overwide cluster rather than truncating source.
        used = next;
        end += 1;
    }
    if start < end {
        let bytes = source_range(shaped, start..end);
        push_prepared(flow, run.text[bytes].to_owned(), run.style,
            warning.take(), shaped.slice(start..end));
    }
}

fn source_range(shaped: &ShapedText, range: Range<usize>) -> Range<usize> {
    shaped.clusters[range.start].bytes.start..shaped.clusters[range.end - 1].bytes.end
}

/// A GPOS boundary can change advances/offsets in both adjacent glyphs. Prepare
/// the actual isolated source substring instead of subtracting a guessed pair.
/// Backtracking has a separate ceiling; exhausted repair retains the remainder
/// as one explicitly diagnosed overwide run, never as incomplete text.
fn place_contextual(
    flow: &mut TextFlow, run: Word, shaped: &ShapedText, shaper: &Shaper<'_>, size: f64, width: f64,
) {
    const MAX_REPAIRS: usize = 64;
    let mut repairs = 0;
    let mut start = 0;
    let mut warning = run.warning;
    while start < shaped.clusters.len() {
        let mut end = start;
        let mut used = 0.0;
        while end < shaped.clusters.len() {
            let next = shaped.extend_width(start, end, used);
            if end > start && flow.line_width + next > width { break; }
            used = next;
            end += 1;
        }
        loop {
            let bytes = source_range(shaped, start..end);
            let prepared = shaper.shape(&run.text[bytes.clone()], run.style, size);
            if flow.line_width + prepared.width() <= width || end == start + 1 {
                if flow.line_width + prepared.width() > width && !flow.line.is_empty() {
                    flow.new_line();
                    // A fresh line has more room: choose its provisional end
                    // again, rather than strand a single glyph on that line.
                    break;
                }
                push_prepared(flow, run.text[bytes].to_owned(), run.style, warning.take(), prepared);
                start = end;
                if start < shaped.clusters.len() { flow.new_line(); }
                break;
            }
            if repairs >= MAX_REPAIRS {
                if !flow.line.is_empty() { flow.new_line(); }
                let bytes = source_range(shaped, start..shaped.clusters.len());
                let prepared = shaper.shape(&run.text[bytes.clone()], run.style, size);
                // Keep a prior resource warning as well as the wrapping warning.
                let mut prepared = prepared;
                prepared.add_wrap_warning();
                push_prepared(flow, run.text[bytes].to_owned(), run.style, warning.take(), prepared);
                return;
            }
            repairs += 1;
            end -= 1;
        }
    }
}

fn text_word(text: String, style: RStyle, warning: Option<SvgWarning>, prepared: ShapedText) -> Word {
    Word {
        text, style, w: prepared.width(), gap: 0.0,
        formula: None, image: None, warning, shaped: Some(prepared),
    }
}

fn push_prepared(
    flow: &mut TextFlow, text: String, style: RStyle, warning: Option<SvgWarning>, prepared: ShapedText,
) {
    let word = text_word(text, style, warning, prepared);
    flow.line_width += word.w;
    flow.line.push(word);
}
