//! Cluster-boundary emergency wrapping and literal code-line wrapping.

use super::{Poster, RStyle, ShapedText, SvgWarning, TextFlow, Word};

impl Poster {
    /// Wrap code without collapsing indentation or dropping spaces. Tabs use
    /// four-column stops in the source line, independent of visual wrapping.
    pub(in super::super) fn code_lines(&self, code: &str, size: f64, width: f64) -> Vec<String> {
        let style = RStyle { mono: true, ..RStyle::BODY };
        let width = width.max(1.0);
        let mut lines = Vec::new();
        for source in code.lines() {
            let mut line = String::new();
            let mut used = 0.0;
            let mut column = 0usize;
            for ch in source.chars() {
                let count = if ch == '\t' { 4 - column % 4 } else { 1 };
                let ch = if ch == '\t' { ' ' } else { ch };
                let advance = self.advance(self.resolve(ch, style).0, ch, size);
                for _ in 0..count {
                    if advance > 0.0 && used > 0.0 && used + advance > width {
                        lines.push(std::mem::take(&mut line));
                        used = 0.0;
                    }
                    line.push(ch);
                    used += advance;
                }
                column = column.saturating_add(count);
            }
            lines.push(line);
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
    }
}

pub(super) fn place_shaped(flow: &mut TextFlow, run: Word, shaped: &ShapedText, width: f64) {
    let mut start = 0;
    let mut end = 0;
    let mut used = 0.0;
    let mut warning = run.warning;
    while end < shaped.clusters.len() {
        let next = shaped.extend_width(start, end, used);
        if flow.line_width + next > width && (end > start || !flow.line.is_empty()) {
            push_chunk(flow, &run.text, shaped, start..end, used, run.style, &mut warning);
            flow.new_line();
            start = end;
            used = 0.0;
            continue;
        }
        // A single indivisible glyph cluster may exceed a very narrow measure,
        // just as a single scalar did before. Preserve it rather than truncate
        // source or split a ligature/zero-advance mark away from its base.
        used = next;
        end += 1;
    }
    push_chunk(flow, &run.text, shaped, start..end, used, run.style, &mut warning);
}

#[allow(clippy::too_many_arguments)]
fn push_chunk(
    flow: &mut TextFlow, source: &str, shaped: &ShapedText, clusters: std::ops::Range<usize>,
    width: f64, style: RStyle, warning: &mut Option<SvgWarning>,
) {
    if clusters.is_empty() { return; }
    let bytes = shaped.clusters[clusters.start].bytes.start
        ..shaped.clusters[clusters.end - 1].bytes.end;
    flow.line.push(Word {
        text: source[bytes].to_owned(), style, w: width, gap: 0.0,
        formula: None, image: None, warning: warning.take(),
    });
    flow.line_width += width;
}

