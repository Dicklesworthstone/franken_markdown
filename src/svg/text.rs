//! Whitespace-aware poster text flow. Style boundaries are not word boundaries.

use super::{Op, Piece, Poster, RStyle, SvgWarning, Word};

#[path = "text_shaping.rs"]
mod shaping;
use shaping::{ShapedText, Shaper};

#[path = "text_wrapping.rs"]
mod wrapping;
use wrapping::place_shaped;

impl Poster {
    /// Greedy wrapping of shaped text. Style boundaries do not introduce spaces
    /// or break opportunities. Emergency breaks retain complete glyph clusters
    /// and discard kerning to a glyph that moved onto the following line.
    pub(super) fn wrap(&self, pieces: &[Piece], size: f64, width: f64) -> Vec<Vec<Word>> {
        let width = width.max(1.0);
        let shaper = Shaper::new(self);
        let mut flow = TextFlow::default();
        for piece in pieces {
            match piece {
                Piece::Break => {
                    self.place_word(&mut flow, &shaper, size, width);
                    flow.new_line();
                    flow.gap = 0.0;
                    flow.trailing_break = true;
                }
                Piece::Image(destination, alt, style) => {
                    flow.word.push(self.image_word(destination, alt, *style, size, width));
                    flow.trailing_break = false;
                }
                Piece::Math(source, display, style) => {
                    flow.word.push(self.math_word(source, *display, *style, size, width));
                    flow.trailing_break = false;
                }
                Piece::Text(text, style) => {
                    let mut start = 0;
                    for (offset, ch) in text.char_indices() {
                        // Code spans preserve their internal whitespace. NBSP
                        // and narrow NBSP never become ordinary breakable glue.
                        if !style.mono && breakable_space(ch) {
                            append_run(&mut flow, &text[start..offset], *style);
                            self.place_word(&mut flow, &shaper, size, width);
                            if flow.gap == 0.0 {
                                flow.gap = self.space_width(*style, size);
                            }
                            start = offset + ch.len_utf8();
                        }
                    }
                    append_run(&mut flow, &text[start..], *style);
                }
            }
        }
        self.place_word(&mut flow, &shaper, size, width);
        if !flow.line.is_empty() || flow.trailing_break {
            flow.new_line();
        }
        flow.lines
    }

    fn place_word(&self, flow: &mut TextFlow, shaper: &Shaper<'_>, size: f64, width: f64) {
        if flow.word.is_empty() {
            return;
        }
        let mut word = std::mem::take(&mut flow.word);
        // Shape after equal-style pieces have coalesced. Adding separately
        // measured widths would miss both ligatures and pairs at their join.
        let shaped: Vec<_> = word.iter_mut().map(|run| {
            if run.formula.is_some() || run.image.is_some() {
                None
            } else {
                let text = shaper.shape(&run.text, run.style, size);
                run.w = text.width();
                Some(text)
            }
        }).collect();
        let word_width: f64 = word.iter().map(|run| run.w).sum();
        let mut gap = if flow.line.is_empty() { 0.0 } else { flow.gap };
        if !flow.line.is_empty() && flow.line_width + gap + word_width > width {
            flow.new_line();
            gap = 0.0;
        }
        if word_width <= width {
            flow.line_width += gap + word_width;
            for (index, mut run) in word.into_iter().enumerate() {
                run.gap = if index == 0 { gap } else { 0.0 };
                flow.line.push(run);
            }
        } else {
            for (run, shaped) in word.into_iter().zip(shaped) {
                if let Some(shaped) = shaped {
                    place_shaped(flow, run, &shaped, width);
                } else {
                    if !flow.line.is_empty() && flow.line_width + run.w > width {
                        flow.new_line();
                    }
                    flow.line_width += run.w;
                    flow.line.push(run);
                }
            }
        }
        flow.gap = 0.0;
    }

    pub(super) fn draw_words(&mut self, words: &[Word], x: f64, baseline: f64, size: f64) {
        // Resolve all text against these exact faces before mutably drawing
        // images/math. One pass-local table cache serves every styled fragment.
        let shaped: Vec<_> = {
            let shaper = Shaper::new(self);
            words.iter().map(|word| {
                (word.image.is_none() && word.formula.is_none())
                    .then(|| shaper.shape(&word.text, word.style, size))
            }).collect()
        };
        let mut pen = x;
        for (word, shaped) in words.iter().zip(shaped) {
            pen += word.gap;
            if let Some(warning) = &word.warning {
                self.warnings.push(warning.clone());
            }
            if let Some(run) = &word.image {
                self.draw_image(run, pen, baseline);
                pen += word.w;
            } else if let Some(run) = &word.formula {
                self.draw_math(run, pen, baseline, word.style.ink);
                if word.style.strike && word.w > 0.0 {
                    self.ops.push(Op::Rule {
                        x1: pen, y1: baseline - size * 0.28,
                        x2: pen + word.w, y2: baseline - size * 0.28,
                        ink: word.style.ink, w: (size * 0.05).max(0.5),
                    });
                }
                pen += word.w;
            } else if let Some(shaped) = shaped {
                pen = shaped.paint(self, pen, baseline, word.style, size);
            }
        }
    }

    pub(super) fn words_width(&self, words: &[Word], _size: f64) -> f64 {
        words.iter().map(|word| word.gap + word.w).sum()
    }


}

fn append_run(flow: &mut TextFlow, text: &str, style: RStyle) {
    if text.is_empty() { return; }
    flow.trailing_break = false;
    if let Some(last) = flow.word.last_mut().filter(|run| {
        run.style == style && run.formula.is_none() && run.image.is_none() && run.warning.is_none()
    }) {
        last.text.push_str(text);
    } else {
        flow.word.push(Word {
            text: text.to_owned(), style, w: 0.0, gap: 0.0,
            formula: None, image: None, warning: None,
        });
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
    gap: f64,
    trailing_break: bool,
}

impl TextFlow {
    fn new_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.line));
        self.line_width = 0.0;
    }
}

#[cfg(test)]
#[path = "text_shaping_tests.rs"]
mod shaping_tests;
