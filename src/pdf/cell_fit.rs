//! Fit cell/TOC/fallback text to its actual shaped measure.
//!
//! Fit overlong tokens before emitting rows. Separately measured tokens are useful estimates,
//! but joining them can introduce positive kerning or different ligatures.
//! Only overlong tokens and overfull merged rows enter these paths.

use super::{
    CellRun, CellWrapLine, Faces, Tok, WidthCache, cjk_break_prohibited, is_breakable_whitespace,
    is_inline_object, text_composition, text_width_cached,
};
use std::cell::RefCell;

#[derive(Clone, Copy)]
struct Position {
    run: usize,
    byte: usize,
}

pub(super) fn repair(
    lines: Vec<CellWrapLine>,
    max_width: f32,
    size: f32,
    faces: &Faces,
    widths: &RefCell<WidthCache>,
) -> Vec<CellWrapLine> {
    if !max_width.is_finite()
        || max_width <= 0.0
        || lines.iter().all(|line| line.width <= max_width + 0.01)
    {
        return lines;
    }
    let mut fitted = Vec::with_capacity(lines.len());
    for line in lines {
        if line.width <= max_width + 0.01 || line.runs.is_empty() {
            fitted.push(line);
        } else {
            split_row(&line, max_width, size, faces, widths, &mut fitted);
        }
    }
    fitted
}

/// An overlong source token owns all its clusters until every full line is
/// emitted. Restarting from an approximate row would strand its final few
/// glyphs on a separate line instead of fitting them alongside the next ones.
pub(super) fn wrap_token(
    token: &Tok,
    max_width: f32,
    size: f32,
    faces: &Faces,
    widths: &RefCell<WidthCache>,
    completed: &mut Vec<CellWrapLine>,
) -> (Tok, f32) {
    let text = token.text.as_str();
    // ASCII cluster indices are byte indices; long identifiers need no index.
    let edges = (!text.is_ascii()).then(|| {
        let mut offsets = vec![0];
        let mut byte = 0;
        for cluster in text_composition::source_clusters(text) {
            byte += cluster.len();
            offsets.push(byte);
        }
        offsets
    });
    let last = edges
        .as_ref()
        .map_or(text.len(), |offsets| offsets.len() - 1);
    let byte = |at: usize| edges.as_ref().map_or(at, |offsets| offsets[at]);
    let mut start = 0;
    loop {
        let from = byte(start);
        let measure =
            |end| text_width_cached(&text[from..byte(end)], size, token.slot, faces, widths);
        let end = fitted_end(start, last, max_width, measure, |at| {
            let offset = byte(at);
            (
                text[..offset].chars().next_back(),
                text[offset..].chars().next(),
            )
        });
        let to = byte(end);
        let width = measure(end);
        let piece = token.source_slice(from, to);
        if end == last {
            return (piece, width);
        }
        completed.push(CellWrapLine {
            width,
            runs: vec![CellRun {
                text: piece.text,
                slot: piece.slot,
                link: piece.link,
                strike: piece.strike,
                list_label: piece.list_label,
                note_spans: piece.note_spans,
                width,
            }],
        });
        start = end;
    }
}

fn fitted_end(
    start: usize,
    last: usize,
    max_width: f32,
    measure: impl Fn(usize) -> f32,
    neighbours: impl Fn(usize) -> (Option<char>, Option<char>),
) -> usize {
    // Start locally and grow exponentially. Repeatedly measuring the
    // entire remaining suffix would be quadratic for a font whose positive
    // kerning leaves only a few clusters per row. This search measures at
    // most O(n log n) source across the repaired row; it never recurses.
    let mut end = start + 1;
    if measure(end) <= max_width {
        let mut step = 1usize;
        while end < last {
            step = step.saturating_mul(2);
            let candidate = start.saturating_add(step).min(last);
            if measure(candidate) <= max_width {
                end = candidate;
                continue;
            }
            let mut high = candidate;
            while end + 1 < high {
                let middle = end + (high - end) / 2;
                if measure(middle) <= max_width {
                    end = middle;
                } else {
                    high = middle;
                }
            }
            break;
        }
    }
    // Prefer an existing word boundary, then a legal CJK boundary. Do not
    // assume shorter source always means narrower glyphs: negative kerns
    // and ligatures can make a shorter prefix wider, so recheck the choice.
    if end < last {
        let word = ((start + 1)..=end).rev().find(|&at| {
            let (before, _) = neighbours(at);
            before.is_some_and(is_breakable_whitespace)
        });
        let preferred = word.or_else(|| {
            ((start + 1)..=end).rev().find(|&at| {
                let (before, after) = neighbours(at);
                !before
                    .zip(after)
                    .is_some_and(|(a, b)| cjk_break_prohibited(a, b))
            })
        });
        if let Some(preferred) = preferred
            && measure(preferred) <= max_width
        {
            end = preferred;
        }
    }
    end
}

fn split_row(
    line: &CellWrapLine,
    max_width: f32,
    size: f32,
    faces: &Faces,
    widths: &RefCell<WidthCache>,
    fitted: &mut Vec<CellWrapLine>,
) {
    let mut edges = vec![Position { run: 0, byte: 0 }];
    for (index, run) in line.runs.iter().enumerate() {
        if is_inline_object(run.slot) {
            // Image/formula encodings are not source text. Their complete
            // fitted box is one indivisible unit, just as in normal wrapping.
            edges.push(Position {
                run: index + 1,
                byte: 0,
            });
            continue;
        }
        let mut byte = 0;
        for cluster in text_composition::source_clusters(&run.text) {
            byte += cluster.len();
            edges.push(if byte == run.text.len() {
                Position {
                    run: index + 1,
                    byte: 0,
                }
            } else {
                Position { run: index, byte }
            });
        }
    }
    let last = edges.len() - 1;
    let mut start = 0;
    while start < last {
        let measure = |end| range_width(line, edges[start], edges[end], size, faces, widths);
        let mut end = fitted_end(start, last, max_width, measure, |at| {
            neighbours(line, edges[at])
        });
        if end < last {
            // A chosen discretionary hyphen is already visible in the run.
            // Keep it beside the preceding whole source cluster when both
            // resulting rows fit, rather than creating a lone '-' continuation.
            if end + 1 == last
                && end > start + 1
                && neighbours(line, edges[last]).0 == Some('-')
                && neighbours(line, edges[end])
                    .0
                    .is_some_and(|ch| !is_breakable_whitespace(ch))
                && measure(end - 1) <= max_width
                && range_width(line, edges[end - 1], edges[last], size, faces, widths) <= max_width
            {
                end -= 1;
            }
        }
        // An indivisible source cluster wider than the whole measure keeps its
        // original glyphs and size. Splitting marks or silently shrinking text
        // would corrupt the source; this is the existing emergency policy.
        let mut runs = Vec::new();
        for_each_piece(line, edges[start], edges[end], |run, from, to| {
            let text = &run.text[from..to];
            runs.push(CellRun {
                slot: run.slot,
                text: text.to_owned(),
                link: run.link,
                strike: run.strike,
                list_label: run.list_label,
                note_spans: run.note_spans.slice(from, to),
                width: text_width_cached(text, size, run.slot, faces, widths),
            });
        });
        let width = runs.iter().map(|run| run.width).sum();
        fitted.push(CellWrapLine { runs, width });
        start = end;
    }
}

fn for_each_piece(
    line: &CellWrapLine,
    start: Position,
    end: Position,
    mut visit: impl FnMut(&CellRun, usize, usize),
) {
    for (index, run) in line
        .runs
        .iter()
        .enumerate()
        .take(end.run.saturating_add(1))
        .skip(start.run)
    {
        let from = if index == start.run { start.byte } else { 0 };
        let to = if index == end.run {
            end.byte
        } else {
            run.text.len()
        };
        if from < to {
            visit(run, from, to);
        }
    }
}

fn range_width(
    line: &CellWrapLine,
    start: Position,
    end: Position,
    size: f32,
    faces: &Faces,
    widths: &RefCell<WidthCache>,
) -> f32 {
    let mut width = 0.0;
    for_each_piece(line, start, end, |run, from, to| {
        width += if from == 0 && to == run.text.len() {
            run.width
        } else {
            text_width_cached(&run.text[from..to], size, run.slot, faces, widths)
        };
    });
    width
}

fn neighbours(line: &CellWrapLine, at: Position) -> (Option<char>, Option<char>) {
    let before = if at.byte > 0 {
        line.runs.get(at.run).and_then(|run| {
            (!is_inline_object(run.slot))
                .then(|| run.text[..at.byte].chars().next_back())
                .flatten()
        })
    } else {
        at.run
            .checked_sub(1)
            .and_then(|index| line.runs.get(index))
            .and_then(|run| {
                (!is_inline_object(run.slot))
                    .then(|| run.text.chars().next_back())
                    .flatten()
            })
    };
    let after = line.runs.get(at.run).and_then(|run| {
        (!is_inline_object(run.slot))
            .then(|| run.text[at.byte..].chars().next())
            .flatten()
    });
    (before, after)
}

#[cfg(test)]
mod tests;
