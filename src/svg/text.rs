//! Whitespace-aware poster text flow. Style boundaries are not word boundaries.

use super::{Piece, Poster, RStyle, Word};

impl Poster {
    /// Greedy word wrapping with explicit gaps. A word can span any number of
    /// styles; only source whitespace permits a normal line break. Overlong
    /// words are split at scalar boundaries as an emergency (no content loss).
    pub(super) fn wrap(&self, pieces: &[Piece], size: f64, width: f64) -> Vec<Vec<Word>> {
        let width = width.max(1.0);
        let mut flow = TextFlow::default();
        for piece in pieces {
            match piece {
                Piece::Break => {
                    self.place_word(&mut flow, size, width);
                    flow.new_line();
                    flow.gap = 0.0;
                    flow.trailing_break = true;
                }
                Piece::Text(text, style) => {
                    let mut start = 0;
                    for (offset, ch) in text.char_indices() {
                        // Code spans preserve their internal whitespace. NBSP
                        // and narrow NBSP never become ordinary breakable glue.
                        if !style.mono && breakable_space(ch) {
                            self.append_run(&mut flow, &text[start..offset], *style, size);
                            self.place_word(&mut flow, size, width);
                            if flow.gap == 0.0 {
                                flow.gap = self.space_width(*style, size);
                            }
                            start = offset + ch.len_utf8();
                        }
                    }
                    self.append_run(&mut flow, &text[start..], *style, size);
                }
            }
        }
        self.place_word(&mut flow, size, width);
        if !flow.line.is_empty() || flow.trailing_break {
            flow.new_line();
        }
        flow.lines
    }

    fn append_run(&self, flow: &mut TextFlow, text: &str, style: RStyle, size: f64) {
        if text.is_empty() {
            return;
        }
        let w = self.measure(text, style, size);
        flow.word_width += w;
        flow.trailing_break = false;
        if let Some(last) = flow.word.last_mut().filter(|run| run.style == style) {
            last.text.push_str(text);
            last.w += w;
        } else {
            flow.word.push(Word {
                text: text.to_string(),
                style,
                w,
                gap: 0.0,
            });
        }
    }

    fn place_word(&self, flow: &mut TextFlow, size: f64, width: f64) {
        if flow.word.is_empty() {
            return;
        }
        let mut gap = if flow.line.is_empty() { 0.0 } else { flow.gap };
        if !flow.line.is_empty() && flow.line_width + gap + flow.word_width > width {
            flow.new_line();
            gap = 0.0;
        }
        let word = std::mem::take(&mut flow.word);
        if flow.word_width <= width {
            flow.line_width += gap + flow.word_width;
            for (index, mut run) in word.into_iter().enumerate() {
                run.gap = if index == 0 { gap } else { 0.0 };
                flow.line.push(run);
            }
        } else {
            // The entire word exceeds the measure: consume each scalar once.
            // Zero-advance combining characters stay on their preceding line.
            for run in word {
                let mut chunk = String::new();
                let mut chunk_width = 0.0;
                for ch in run.text.chars() {
                    let advance = self.advance(self.resolve(ch, run.style).0, ch, size);
                    if advance > 0.0 && flow.line_width > 0.0
                        && flow.line_width + advance > width
                    {
                        push_chunk(flow, &mut chunk, &mut chunk_width, run.style);
                        flow.new_line();
                    }
                    chunk.push(ch);
                    chunk_width += advance;
                    flow.line_width += advance;
                }
                push_chunk(flow, &mut chunk, &mut chunk_width, run.style);
            }
        }
        flow.word_width = 0.0;
        flow.gap = 0.0;
    }

    pub(super) fn draw_words(&mut self, words: &[Word], x: f64, baseline: f64, size: f64) {
        let mut pen = x;
        for word in words {
            pen += word.gap;
            pen = self.draw_text(pen, baseline, &word.text, word.style, size);
        }
    }

    pub(super) fn words_width(&self, words: &[Word], _size: f64) -> f64 {
        words.iter().map(|word| word.gap + word.w).sum()
    }

    /// Wrap code without collapsing indentation or dropping spaces. Tabs use
    /// four-column stops in the source line, independent of visual wrapping.
    pub(super) fn code_lines(&self, code: &str, size: f64, width: f64) -> Vec<String> {
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

fn breakable_space(ch: char) -> bool {
    ch.is_whitespace() && !matches!(ch, '\u{00a0}' | '\u{202f}')
}

#[derive(Default)]
struct TextFlow {
    lines: Vec<Vec<Word>>,
    line: Vec<Word>,
    line_width: f64,
    word: Vec<Word>,
    word_width: f64,
    gap: f64,
    trailing_break: bool,
}

impl TextFlow {
    fn new_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
        self.line_width = 0.0;
    }
}

fn push_chunk(flow: &mut TextFlow, text: &mut String, width: &mut f64, style: RStyle) {
    if !text.is_empty() {
        flow.line.push(Word {
            text: std::mem::take(text),
            style,
            w: *width,
            gap: 0.0,
        });
        *width = 0.0;
    }
}
