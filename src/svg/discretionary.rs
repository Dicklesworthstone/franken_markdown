//! Authored prose breaks without invisible glyphs or lost cross-hint shaping.
//!
//! The joined word remains authoritative. Fragment widths propose shared
//! Knuth-Plass breaks; only reshaped, fitting physical lines may be installed.

use super::paragraph::{self, Group, upper_units};
use super::{Poster, RStyle, Shaper, SvgWarning, TextFlow, Word};
use franken_markdown::layout::{
    FORCED_BREAK_PENALTY, Glue, LayoutUnit, ParagraphItem, ParagraphLayoutScratch, Penalty,
    TextBox, break_paragraph_into,
};
use std::ops::Range;

const MAX_HINTS: usize = 2048;
const MAX_CANDIDATES: usize = 2048;
const MAX_REPAIR_WORK: usize = 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Hint {
    offset: usize,
    hyphen: bool,
    style: RStyle,
}

pub(super) struct Authored {
    pub(super) hints: Vec<Hint>,
    pub(super) source_bytes: usize,
}

pub(super) fn is_hint(ch: char) -> bool {
    matches!(ch, '\u{00ad}' | '\u{200b}')
}

fn prose(run: &Word) -> bool {
    !run.style.mono && run.formula.is_none() && run.image.is_none() && run.warning.is_none()
}

// Objects occupy one indivisible logical unit; their source strings are never
// sliced or interpreted as prose. All other offsets are UTF-8 text boundaries.
fn length(run: &Word) -> usize {
    if run.formula.is_some() || run.image.is_some() {
        1
    } else {
        run.text.len()
    }
}

fn text_run(text: String, style: RStyle) -> Word {
    Word {
        text,
        style,
        w: 0.0,
        gap: 0.0,
        formula: None,
        image: None,
        warning: None,
        shaped: None,
    }
}

fn join_text(runs: &mut Vec<Word>, text: &str, style: RStyle) {
    if let Some(last) = runs
        .last_mut()
        .filter(|run| prose(run) && run.style == style)
    {
        last.text.push_str(text);
    } else {
        runs.push(text_run(text.to_owned(), style));
    }
}

fn retain_hint(pending: &mut Option<Hint>, hints: &mut Vec<Hint>, mark: bool, limited: &mut bool) {
    if let Some(hint) = pending.take()
        && !mark
    {
        if hints.len() < MAX_HINTS {
            hints.push(hint);
        } else {
            *limited = true;
        }
    }
}

/// Keep instructions separate from the text before shaping. Repeated/edge
/// markers create neither empty words nor spaces; a mark stays with its base.
pub(super) fn normalize(runs: &mut Vec<Word>) -> Authored {
    let source_bytes = runs
        .iter()
        .fold(0usize, |total, run| total.saturating_add(run.text.len()));
    let mut authored = Authored {
        hints: Vec::new(),
        source_bytes,
    };
    if !runs
        .iter()
        .any(|run| prose(run) && run.text.chars().any(is_hint))
    {
        return authored;
    }
    let mut visible = Vec::with_capacity(runs.len());
    let mut pending: Option<Hint> = None;
    let mut offset = 0usize;
    let mut limited = false;
    for run in std::mem::take(runs) {
        if !prose(&run) {
            if length(&run) != 0 {
                retain_hint(&mut pending, &mut authored.hints, false, &mut limited);
                offset = offset.saturating_add(length(&run));
            }
            visible.push(run);
            continue;
        }
        let mut start = 0;
        for (byte, ch) in run.text.char_indices() {
            if !is_hint(ch) {
                continue;
            }
            if start < byte {
                let text = &run.text[start..byte];
                retain_hint(
                    &mut pending,
                    &mut authored.hints,
                    starts_with_mark(text),
                    &mut limited,
                );
                join_text(&mut visible, text, run.style);
                offset = offset.saturating_add(text.len());
            }
            if offset != 0 && !pending.is_some_and(|hint| hint.hyphen) {
                pending = Some(Hint {
                    offset,
                    hyphen: ch == '\u{00ad}',
                    style: run.style,
                });
            }
            start = byte + ch.len_utf8();
        }
        if start < run.text.len() {
            let text = &run.text[start..];
            retain_hint(
                &mut pending,
                &mut authored.hints,
                starts_with_mark(text),
                &mut limited,
            );
            join_text(&mut visible, text, run.style);
            offset = offset.saturating_add(text.len());
        }
    }
    if limited {
        limit_warning(&mut visible);
    }
    *runs = visible;
    authored
}

fn starts_with_mark(text: &str) -> bool {
    // The canonical and positioned SVG mark shapers share this supported range.
    text.chars()
        .next()
        .is_some_and(|ch| matches!(ch as u32, 0x0300..=0x036f))
}

fn prepare(run: &mut Word, shaper: &Shaper<'_>, size: f64) {
    let shaped = shaper.shape(&run.text, run.style, size);
    run.w = shaped.width();
    run.shaped = Some(shaped);
}

fn fragment(
    runs: &[Word],
    range: Range<usize>,
    hyphen: Option<Hint>,
    shaper: &Shaper<'_>,
    size: f64,
) -> Vec<Word> {
    let mut result = Vec::new();
    let mut offset = 0;
    for run in runs {
        let end = offset + length(run);
        if end > range.start && offset < range.end {
            let start = range.start.saturating_sub(offset);
            let stop = range.end.min(end) - offset;
            let mut part = if start == 0 && stop == length(run) {
                run.clone()
            } else {
                let mut part = text_run(run.text[start..stop].to_owned(), run.style);
                if start == 0 {
                    part.warning = run.warning.clone();
                }
                prepare(&mut part, shaper, size);
                part
            };
            part.gap = 0.0;
            result.push(part);
        } else if end == offset && range.start == 0 {
            // A diagnostic-only run belongs to the first fragment, exactly once.
            result.push(run.clone());
        }
        offset = end;
    }
    if let Some(hint) = hyphen.filter(|hint| hint.hyphen) {
        join_text(&mut result, "-", hint.style);
        if let Some(last) = result.last_mut() {
            prepare(last, shaper, size);
        }
    }
    result
}

fn width(runs: &[Word]) -> f64 {
    runs.iter().map(|run| run.w).sum()
}

fn append(line: &mut Vec<Word>, runs: Vec<Word>, gap: f64) {
    let gap = if line.is_empty() { 0.0 } else { gap };
    for (index, mut run) in runs.into_iter().enumerate() {
        run.gap = if index == 0 { gap } else { 0.0 };
        line.push(run);
    }
}

struct Atom {
    group: usize,
    range: Range<usize>,
    tail: Option<Hint>,
    width: f64,
}

fn candidate_lines(
    groups: &[Group],
    shaper: &Shaper<'_>,
    size: f64,
    measure: f64,
) -> Option<Vec<Vec<Word>>> {
    let candidates = groups.iter().try_fold(0usize, |count, group| {
        count.checked_add(1 + group.hints.len())
    })?;
    if candidates > MAX_CANDIDATES || !measure.is_finite() {
        return None;
    }
    let units = (measure * 1000.0).floor();
    if units < 1.0 || units > f64::from(i32::MAX) {
        return None;
    }
    let mut atoms = Vec::with_capacity(candidates);
    for (index, group) in groups.iter().enumerate() {
        let total = group.runs.iter().map(length).sum();
        let mut start = 0;
        for hint in group
            .hints
            .iter()
            .copied()
            .map(Some)
            .chain(std::iter::once(None))
        {
            let end = hint.map_or(total, |hint| hint.offset);
            let runs = fragment(&group.runs, start..end, None, shaper, size);
            let natural = width(&runs);
            if runs.is_empty() || upper_units(natural)?.milli_points() > units as i32 {
                return None;
            }
            atoms.push(Atom {
                group: index,
                range: start..end,
                tail: hint,
                width: natural,
            });
            start = end;
        }
    }
    let mut items = Vec::with_capacity(atoms.len() * 2);
    for (index, atom) in atoms.iter().enumerate() {
        if index != 0 {
            if atoms[index - 1].group == atom.group {
                let hint = atoms[index - 1].tail?;
                let hyphen = if hint.hyphen {
                    shaper.shape("-", hint.style, size).width()
                } else {
                    0.0
                };
                items.push(ParagraphItem::Penalty(Penalty {
                    width: upper_units(hyphen)?,
                    penalty: if hint.hyphen { 25 } else { 0 },
                    flagged: hint.hyphen,
                }));
            } else {
                items.push(ParagraphItem::Glue(Glue {
                    width: upper_units(groups[atom.group].gap)?,
                    stretch: LayoutUnit::ZERO,
                    shrink: LayoutUnit::ZERO,
                }));
            }
        }
        items.push(ParagraphItem::Box(TextBox {
            text: String::new(),
            runs: Default::default(),
            width: upper_units(atom.width)?,
            protrusion: Default::default(),
        }));
    }
    items.push(ParagraphItem::Penalty(Penalty {
        width: LayoutUnit::ZERO,
        penalty: FORCED_BREAK_PENALTY,
        flagged: false,
    }));
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_justified(false);
    scratch.set_expansion_permilli(0);
    let mut breaks = Vec::new();
    break_paragraph_into(
        &items,
        LayoutUnit::from_milli_points(units as i32),
        &mut scratch,
        &mut breaks,
    );
    let mut next = 0;
    let mut lines = Vec::with_capacity(breaks.len());
    for boundary in breaks {
        if boundary.start != next * 2
            || boundary.end % 2 != 1
            || boundary.end >= items.len()
            || boundary.next != boundary.end + 1
        {
            return None;
        }
        let end = boundary.end.div_ceil(2);
        if end <= next || end > atoms.len() {
            return None;
        }
        let mut line = Vec::new();
        let mut first = next;
        while first < end {
            let mut last = first + 1;
            while last < end && atoms[last].group == atoms[first].group {
                last += 1;
            }
            let group = &groups[atoms[first].group];
            let tail = (last == end).then_some(atoms[last - 1].tail).flatten();
            let runs = fragment(
                &group.runs,
                atoms[first].range.start..atoms[last - 1].range.end,
                tail,
                shaper,
                size,
            );
            append(
                &mut line,
                runs,
                if atoms[first].range.start == 0 {
                    group.gap
                } else {
                    0.0
                },
            );
            first = last;
        }
        let used: f64 = line.iter().map(|run| run.gap + run.w).sum();
        if line.is_empty() || !used.is_finite() || used > measure {
            return None;
        }
        lines.push(line);
        next = end;
    }
    (next == atoms.len()).then_some(lines)
}

fn joined_plan(groups: &[Group], measure: f64) -> Option<Vec<Range<usize>>> {
    if let Some(plan) = paragraph::plan(groups, measure) {
        return Some(plan);
    }
    // The integer planner rounds outwards. A word can still fit exactly in
    // the painter's original f64 geometry, as the ordinary greedy path allows.
    // Retain that complete-word alternative instead of inventing an authored
    // break solely because a fractional milli-point was rounded away.
    let mut plan = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (index, group) in groups.iter().enumerate() {
        let natural = width(&group.runs);
        if !natural.is_finite()
            || natural < 0.0
            || natural > measure
            || !group.gap.is_finite()
            || group.gap < 0.0
        {
            return None;
        }
        let gap = if index == start { 0.0 } else { group.gap };
        if index != start && used + gap + natural > measure {
            plan.push(start..index);
            start = index;
            used = natural;
        } else {
            used += gap + natural;
        }
    }
    if start < groups.len() {
        plan.push(start..groups.len());
    }
    Some(plan)
}

pub(super) fn planned_lines(
    groups: &[Group],
    shaper: &Shaper<'_>,
    size: f64,
    width: f64,
) -> Option<Vec<Vec<Word>>> {
    let ordinary = joined_plan(groups, width);
    let candidates = candidate_lines(groups, shaper, size, width);
    // Isolated fragment estimates can lose a ligature or pair adjustment.
    // Never add a physical line when the fully joined source already fits.
    if let Some(plan) = ordinary
        && candidates
            .as_ref()
            .is_none_or(|lines| plan.len() < lines.len())
    {
        return Some(
            plan.into_iter()
                .map(|range| {
                    let mut line = Vec::new();
                    for group in &groups[range] {
                        append(&mut line, group.runs.clone(), group.gap);
                    }
                    line
                })
                .collect(),
        );
    }
    candidates
}

/// Bounded repair/fallback. A hint is chosen only when its real prefix and
/// optional hyphen fit together; a hyphen can never become a line by itself.
#[allow(clippy::too_many_arguments)]
pub(super) fn place(
    poster: &Poster,
    flow: &mut TextFlow,
    shaper: &Shaper<'_>,
    group: Group,
    size: f64,
    measure: f64,
) {
    if group.hints.is_empty() {
        poster.place_prepared(flow, shaper, group.runs, group.gap, size, measure);
        return;
    }
    let total = group.runs.iter().map(length).sum();
    let mut start = 0;
    let mut gap = group.gap;
    let mut work = 0usize;
    let already_warned = group.runs.iter().any(|run| {
        run.warning
            .as_ref()
            .is_some_and(|warning| warning.code == "svg_discretionary_limit")
    });
    loop {
        let remaining = fragment(&group.runs, start..total, None, shaper, size);
        work = work
            .saturating_add(total - start)
            .saturating_add(group.runs.len());
        if work > MAX_REPAIR_WORK {
            let mut remaining = remaining;
            if !already_warned {
                limit_warning(&mut remaining);
            }
            poster.place_prepared(flow, shaper, remaining, gap, size, measure);
            return;
        }
        let available = measure - flow.line_width - if flow.line.is_empty() { 0.0 } else { gap };
        if width(&remaining) <= available {
            poster.place_prepared(flow, shaper, remaining, gap, size, measure);
            return;
        }
        let mut selected = None;
        for hint in group
            .hints
            .iter()
            .copied()
            .filter(|hint| hint.offset > start)
        {
            work = work
                .saturating_add(hint.offset - start)
                .saturating_add(group.runs.len());
            if work > MAX_REPAIR_WORK {
                let mut remaining = remaining;
                if !already_warned {
                    limit_warning(&mut remaining);
                }
                poster.place_prepared(flow, shaper, remaining, gap, size, measure);
                return;
            }
            let prefix = fragment(&group.runs, start..hint.offset, Some(hint), shaper, size);
            if width(&prefix) <= available {
                selected = Some((hint.offset, prefix));
            }
        }
        if let Some((end, prefix)) = selected {
            poster.place_prepared(flow, shaper, prefix, gap, size, measure);
            flow.new_line();
            start = end;
            gap = 0.0;
        } else if !flow.line.is_empty() {
            flow.new_line();
            gap = 0.0;
        } else {
            // No authored prefix fits even an empty line. Keep the complete
            // joined remainder on the existing cluster-safe emergency path.
            poster.place_prepared(flow, shaper, remaining, 0.0, size, measure);
            return;
        }
    }
}

fn limit_warning(runs: &mut Vec<Word>) {
    let warning = SvgWarning { code: "svg_discretionary_limit",
        message: "Authored line-break candidate or repair budget exceeded; remaining visible text uses cluster-safe wrapping.".into() };
    if let Some(run) = runs.iter_mut().find(|run| run.warning.is_none()) {
        run.warning = Some(warning);
    } else {
        let mut run = text_run(String::new(), RStyle::BODY);
        run.warning = Some(warning);
        runs.push(run);
    }
}
