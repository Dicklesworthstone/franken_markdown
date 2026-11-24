//! Presentation shared with PDF/HTML, measured from retained SVG glyph runs.

use super::{Inline, Op, Poster, RStyle, Word, text};
use crate::table_format::{FootingRule, decimal_anchor, decimal_cell_text_with_footnotes};

#[derive(Clone, Copy, Default)]
pub(super) struct DecimalMeasure {
    pub before: f64,
    pub after: f64,
}

pub(super) fn cell_measure(poster: &Poster, cell: &[Inline], size: f64) -> Option<DecimalMeasure> {
    {
        let navigation = poster.navigation.borrow();
        let numeric = decimal_cell_text_with_footnotes(cell, |inline| {
            matches!(
                navigation.inline_note(inline),
                Some(franken_markdown::EndnoteTarget::Note(_))
            )
        })?;
        decimal_anchor(&numeric)?;
    }
    let mut pieces = Vec::new();
    poster.flatten(cell, RStyle::BODY, &mut pieces);
    let lines = poster.wrap(&pieces, size, f64::MAX);
    let [line] = lines.as_slice() else {
        return None;
    };
    line_measure(line)
}

/// Retain the exact painted prefix while excluding only verified trailing
/// citation spans from numeric admission. A navigation boundary may lie inside
/// one shaped word; looking only at its first style would treat a note label as
/// part of the amount, or discard the amount along with its citation.
fn numeric_line_text(line: &[Word]) -> Option<String> {
    let mut text = String::new();
    let mut cited = false;
    for (index, word) in line.iter().enumerate() {
        if index > 0 && word.gap > 0.0 {
            text.push(' ');
        }
        for (source, citation) in text::citation_spans(word) {
            if citation {
                cited = true;
            } else if cited && !source.chars().all(char::is_whitespace) {
                return None;
            } else {
                text.push_str(source);
            }
        }
    }
    Some(text)
}

pub(super) fn line_measure(line: &[Word]) -> Option<DecimalMeasure> {
    let text = numeric_line_text(line)?;
    let anchor = decimal_anchor(&text)?;
    let dot = text[anchor..].starts_with('.');
    let mut byte = 0;
    let mut pen = 0.0;
    let mut before = None;
    for (index, word) in line.iter().enumerate() {
        if index > 0 && word.gap > 0.0 {
            byte += 1;
        }
        pen += word.gap;
        let end = byte + word.text.len();
        if before.is_none() && (byte..end).contains(&anchor) {
            before = Some(
                pen + word
                    .shaped
                    .as_ref()?
                    .decimal_anchor_position(anchor - byte, dot)?,
            );
        }
        pen += word.w;
        if anchor == end && !dot && before.is_none() {
            before = Some(pen);
        }
        byte = end;
    }
    before.map(|before| DecimalMeasure {
        before,
        after: (pen - before).max(0.0),
    })
}

/// Place decoration below the row's complete ink-aware height. The extra
/// clearance belongs to the row, including when another cell wraps or has a
/// deep equation. Strokes are ordinary vector rules, never visible marker text.
pub(super) fn draw_footings(
    poster: &mut Poster,
    rules: &[FootingRule],
    widths: &[f64],
    left: f64,
    top: f64,
    padding: f64,
) -> f64 {
    let mut x = left;
    let mut extra = 0.0_f64;
    for (&rule, &width) in rules.iter().zip(widths) {
        let count = match rule {
            FootingRule::None => 0,
            FootingRule::Single => 1,
            FootingRule::Double => 2,
        };
        if count > 0 {
            extra = extra.max(2.0 * f64::from(count) + 1.0);
            let inset = padding.min(width * 0.25);
            for stroke in 0..count {
                let y = top + f64::from(stroke) * 2.0;
                poster.ops.push(Op::Rule {
                    x1: x + inset,
                    y1: y,
                    x2: x + width - inset,
                    y2: y,
                    ink: super::Ink::Border,
                    w: 0.7,
                });
            }
        }
        x += width;
    }
    extra
}

#[cfg(test)]
#[path = "financial_tables_tests.rs"]
mod tests;
