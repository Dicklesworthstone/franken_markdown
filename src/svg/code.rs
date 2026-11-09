//! Whole-fence syntax colors over the existing measured code glyphs.
//!
//! Classification precedes tab expansion and wrapping. Only paint changes:
//! token boundaries never become shaping or line-breaking boundaries.

use super::{Ink, Op, Poster, SvgWarning, Theme};
use franken_markdown::highlight::{Span, Tok, highlight, is_supported};

const MAX_LANGUAGE_BYTES: usize = 256;
const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_SPANS: usize = 65_536;
const MAX_EXPANDED_BYTES: usize = 4 * MAX_SOURCE_BYTES;
const TOTAL_INPUT_BYTES: usize = 4 * 1024 * 1024;
const TOTAL_SPANS: usize = 262_144;

pub(super) fn bounded_language(language: &str) -> bool {
    language.len() <= MAX_LANGUAGE_BYTES
}

const TOKENS: [Tok; 9] = [
    Tok::Plain,
    Tok::Keyword,
    Tok::Type,
    Tok::Func,
    Tok::Str,
    Tok::Number,
    Tok::Comment,
    Tok::Operator,
    Tok::Punct,
];

/// Resolve the shared palette once, not once per emitted glyph. Token indices
/// are private storage details; all color choices belong to `Theme::code_color`.
pub(super) struct Palette([String; 9]);

impl Palette {
    fn new(theme: &Theme) -> Self {
        let colors = theme.effective_colors(false, false);
        Self(TOKENS.map(|token| theme.code_color(colors, token).to_owned()))
    }

    pub(super) fn color(&self, token: Tok) -> &str {
        let index = match token {
            Tok::Plain => 0,
            Tok::Keyword => 1,
            Tok::Type => 2,
            Tok::Func => 3,
            Tok::Str => 4,
            Tok::Number => 5,
            Tok::Comment => 6,
            Tok::Operator => 7,
            Tok::Punct => 8,
        };
        &self.0[index]
    }
}

pub(super) struct Highlighting {
    pub(super) palette: Palette,
    remaining_input: usize,
    remaining_spans: usize,
}

impl Highlighting {
    pub(super) fn new(theme: &Theme) -> Self {
        Self {
            palette: Palette::new(theme),
            remaining_input: TOTAL_INPUT_BYTES,
            remaining_spans: TOTAL_SPANS,
        }
    }

    fn prepare(&mut self, code: &str, language: Option<&str>) -> Result<Option<Plan>, SvgWarning> {
        let Some(language) = language else {
            return Ok(None);
        };
        // Host-constructed ASTs need the same protection as parsed fences.
        // Check the language before lexer lookup can normalize or copy it.
        if !bounded_language(language) {
            return Err(limited());
        }
        if code.is_empty() || !is_supported(language) {
            return Ok(None);
        }
        if code.len() > MAX_SOURCE_BYTES {
            return Err(limited());
        }
        let input = code.len() + language.len();
        if input > self.remaining_input {
            return Err(limited());
        }
        // Charge attempted classification too: a rejected span inventory must
        // not permit repeated fences to bypass the whole-poster work bound.
        self.remaining_input -= input;
        let spans = highlight(language, code);
        if spans.len() > MAX_SPANS || spans.len() > self.remaining_spans {
            return Err(limited());
        }
        self.remaining_spans -= spans.len();
        Plan::from_source(code, &spans).map(Some)
    }
}

struct Plan {
    /// Byte offsets in concatenated expanded physical lines, excluding their
    /// terminators. The ordinary code wrapper retains exactly those bytes.
    spans: Vec<Span>,
    bytes: usize,
}

impl Plan {
    fn from_source(code: &str, spans: &[Span]) -> Result<Self, SvgWarning> {
        let mut end = 0;
        for span in spans {
            if span.start != end
                || span.end <= span.start
                || span.end > code.len()
                || !code.is_char_boundary(span.start)
                || !code.is_char_boundary(span.end)
            {
                return Err(invalid());
            }
            end = span.end;
        }
        if end != code.len() {
            return Err(invalid());
        }
        let mut projected: Vec<Span> = Vec::with_capacity(spans.len());
        let mut source_start = 0;
        let mut cursor = 0;
        let mut bytes = 0;
        for line in code.lines() {
            let mut column = 0usize;
            for (offset, ch) in line.char_indices() {
                let source_byte = source_start + offset;
                while spans
                    .get(cursor)
                    .is_some_and(|span| span.end <= source_byte)
                {
                    cursor += 1;
                }
                let Some(span) = spans.get(cursor) else {
                    return Err(invalid());
                };
                let count = if ch == '\t' { 4 - column % 4 } else { 1 };
                let added = if ch == '\t' { count } else { ch.len_utf8() };
                if added > MAX_EXPANDED_BYTES - bytes {
                    return Err(limited());
                }
                let start = bytes;
                bytes += added;
                if let Some(last) = projected.last_mut().filter(|last| last.kind == span.kind) {
                    last.end = bytes;
                } else {
                    projected.push(Span {
                        kind: span.kind,
                        start,
                        end: bytes,
                    });
                }
                column = column.saturating_add(count);
            }
            source_start += line.len();
            let tail = &code[source_start..];
            source_start += if tail.starts_with("\r\n") {
                2
            } else {
                usize::from(tail.starts_with('\n'))
            };
        }
        Ok(Self {
            spans: projected,
            bytes,
        })
    }

    fn ink(&self, byte: usize, cursor: &mut usize) -> Ink {
        while self.spans.get(*cursor).is_some_and(|span| span.end <= byte) {
            *cursor += 1;
        }
        self.spans
            .get(*cursor)
            .map_or(Ink::Fg, |span| Ink::Syntax(span.kind))
    }
}

fn limited() -> SvgWarning {
    SvgWarning {
        code: "svg_code_highlight_limit",
        message: "Code syntax highlighting language, source or span budget exceeded; retained the complete fence with plain code styling.".to_owned(),
    }
}

fn invalid() -> SvgWarning {
    SvgWarning {
        code: "svg_code_highlight_invalid",
        message: "Code syntax spans did not cover the complete source; retained the complete fence with plain code styling.".to_owned(),
    }
}

impl Poster {
    pub(super) fn fenced_code_panel(&mut self, code: &str, language: Option<&str>, l: f64, r: f64) {
        let mut plan = match self.code.prepare(code, language) {
            Ok(plan) => plan,
            Err(warning) => {
                self.warnings.push(warning);
                None
            }
        };
        let size = f64::from(self.scale.code);
        let leading = size * 1.45;
        let pad = 8.0 * self.unit_scale();
        let inset = (12.0 * self.unit_scale()).min((r - l).max(0.0) * 0.25);
        let lines = self.code_words(code, size, r - l - 2.0 * inset);
        if let Some(colors) = &plan
            && lines
                .iter()
                .try_fold(0usize, |total, line| total.checked_add(line.text.len()))
                != Some(colors.bytes)
        {
            // Complete admission precedes painting, including the projection
            // through the wrapper's existing whitespace-preserving contract.
            self.warnings.push(invalid());
            plan = None;
        }
        let h = lines
            .iter()
            .map(|line| {
                self.line_metrics(std::slice::from_ref(line), size * 0.8, leading)
                    .1
            })
            .sum::<f64>()
            + 2.0 * pad;
        self.ops.push(Op::Rect {
            x: l,
            y: self.y,
            w: r - l,
            h,
            fill: Ink::CodeBg,
            stroke: Some(Ink::BorderMuted),
        });
        let mut ly = self.y + pad;
        let mut source_byte = 0;
        let mut span_cursor = 0;
        for line in &lines {
            let row = std::slice::from_ref(line);
            let (ascent, height) = self.line_metrics(row, size * 0.8, leading);
            if let (Some(colors), Some(shaped)) = (&plan, &line.shaped) {
                if let Some(warning) = &line.warning {
                    self.warnings.push(warning.clone());
                }
                shaped.paint_colored(self, l + inset, ly + ascent, line.style, size, |byte| {
                    colors.ink(source_byte + byte, &mut span_cursor)
                });
            } else {
                self.draw_words(row, l + inset, ly + ascent, size);
            }
            if plan.is_some() {
                source_byte += line.text.len();
            }
            ly += height;
        }
        self.y += h + f64::from(self.scale.body) * 0.7;
    }
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
