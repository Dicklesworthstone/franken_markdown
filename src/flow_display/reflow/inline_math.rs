//! Mixed text/formula lines with measured baselines and bounded text windows.
//! Formula source ranges remain atomic; only ordinary or explicit fallback text
//! is sent to the host shaper. No glyph is assigned an invented Markdown caret.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use super::breaks::cjk_boundary;
use super::discretionary;
use super::math::{COMMAND_BYTES, Formula, InlineMetrics, MathFailure};
use super::styled::validate_ranges;
use super::{Align, FlowInlineStyle, FlowLayoutError, FlowTextRole, Reflow};
use crate::display::{
    DisplayItem, DisplayRect, DisplayTextRun, MAX_DISPLAY_TOTAL_PATH_COMMANDS, VectorShapeType,
};
use crate::flow_display::{FlowInlineRun, FlowMathStyle};
use crate::span::SourceSpan;
use crate::text::{Direction, OwnedTextRun, TextRunMetrics};

const MISSING_METRICS: &str =
    "inline math requires measured text ascent and descent; showing the complete formula source";

struct Atom {
    bytes: Range<usize>,
    run_index: usize,
    advance: f32,
    word_break: bool,
    math: bool,
    continuation: Option<usize>,
}

enum Prepared {
    Formula {
        formula: Arc<Formula>,
        metrics: InlineMetrics,
    },
    Fallback(String),
}

#[derive(Default)]
struct Inventory {
    entries: HashMap<usize, Prepared>,
    /// Prepared but not yet emitted commands share the global path allowance.
    pending_commands: usize,
}

impl Inventory {
    fn retire_before(&mut self, runs: &[FlowInlineRun], frontier: usize) {
        self.entries.retain(|index, entry| {
            if runs[*index].range.end > frontier {
                return true;
            }
            if let Prepared::Formula { formula, .. } = entry {
                self.pending_commands -= formula.command_count();
            }
            false
        });
    }

    fn fallback(&mut self, index: usize, reason: &str) {
        if let Some(Prepared::Formula { formula, .. }) = self.entries.remove(&index) {
            self.pending_commands -= formula.command_count();
        }
        self.entries.insert(
            index,
            Prepared::Fallback(reason.chars().take(160).collect()),
        );
    }
}

struct Window {
    end: usize,
    hard_break: bool,
}

enum Fragment {
    Text {
        run: OwnedTextRun,
        run_index: usize,
        size: f32,
        fallback: Option<String>,
    },
    Math {
        formula: Arc<Formula>,
        metrics: InlineMetrics,
        run_index: usize,
    },
}

impl Fragment {
    fn advance(&self) -> f32 {
        match self {
            Self::Text { run, .. } => run.total_advance,
            Self::Math { metrics, .. } => metrics.advance,
        }
    }
}

impl<F: ?Sized> Reflow<'_, F>
where
    F: FnMut(&str, f32, FlowTextRole, FlowInlineStyle) -> Result<OwnedTextRun, String>,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn inline_math_text(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        x: f32,
        y: f32,
        width: f32,
        size: f32,
        leading: f32,
        role: FlowTextRole,
        color: &str,
        span: SourceSpan,
    ) -> Result<f32, FlowLayoutError> {
        validate_ranges(text, runs)?;
        let mut inventory = Inventory::default();
        let mut start = 0usize;
        let mut height = 0.0f32;
        while start < text.len() {
            inventory.retire_before(runs, start);
            let window = self.math_window(text, runs, start, size, width, role, &mut inventory)?;
            if window.end == start {
                if !window.hard_break {
                    return Err(FlowLayoutError::InvalidShapedRun);
                }
                height += self.begin_line(y + height, leading, None)?.height;
                start += 1;
                continue;
            }
            let mut atoms = self.math_atoms(text, runs, start..window.end, size, role, &inventory, true)?;
            let mut first = 0usize;
            let mut retry = false;
            while first < atoms.len() {
                let mut end = first;
                let mut advance = 0.0;
                let mut word_break = None;
                while end < atoms.len() {
                    let next = advance + atoms[end].advance;
                    if next > width && end > first {
                        break;
                    }
                    advance = next;
                    if atoms[end].word_break {
                        word_break = Some(end + 1);
                    }
                    end += 1;
                    if next > width {
                        break;
                    }
                }
                // Carry the last tentative line into the next measurement
                // window. A scratch-space boundary is never a visible break.
                if end == atoms.len() && !window.hard_break && window.end < text.len() {
                    if first == 0 {
                        return Err(FlowLayoutError::BudgetExceeded(
                            "shaping window cannot settle a line",
                        ));
                    }
                    break;
                }
                let measured_end = end;
                if end < atoms.len() && role != FlowTextRole::Code {
                    end = word_break.unwrap_or(end);
                }
                let mut minimum_end = first + 1;
                if role != FlowTextRole::Code {
                    while minimum_end < atoms.len()
                        && discretionary::prohibited_boundary(text, runs, atoms[minimum_end].bytes.start, role)
                    {
                        minimum_end += 1;
                    }
                    end = end.max(minimum_end);
                    while end > minimum_end
                        && end < atoms.len()
                        && discretionary::prohibited_boundary(text, runs, atoms[end].bytes.start, role)
                    {
                        end -= 1;
                    }
                }
                // A protected CJK group may extend the initial candidate.
                // Keep its right-hand shaping context until a following
                // window or the physical line end makes the decision final.
                if end == atoms.len() && !window.hard_break && window.end < text.len() {
                    if first == 0 {
                        return Err(FlowLayoutError::BudgetExceeded(
                            "shaping window cannot settle a line",
                        ));
                    }
                    break;
                }
                let mut preferred = discretionary::at_boundary(
                    text, runs, atoms[end - 1].bytes.end, role,
                ).is_some();
                let mut emergency_end = measured_end.max(minimum_end);
                while emergency_end > minimum_end
                    && discretionary::prohibited_boundary(
                        text, runs, atoms[emergency_end - 1].bytes.end, role,
                    )
                {
                    emergency_end -= 1;
                }
                let mut emergency = (preferred && emergency_end > end).then_some(emergency_end);
                let (fragments, final_width) = loop {
                    let fragments = self.math_fragments(
                        text,
                        runs,
                        &atoms[first..end],
                        size,
                        role,
                        &inventory,
                    )?;
                    let final_width: f32 = fragments.iter().map(Fragment::advance).sum();
                    if !final_width.is_finite() {
                        return Err(FlowLayoutError::InvalidShapedRun);
                    }
                    if final_width <= width {
                        break (fragments, final_width);
                    }
                    let Some(next) = discretionary::next_line_end(
                        minimum_end, end, &mut preferred, &mut emergency,
                        |candidate| atoms[candidate - 1].word_break,
                    ) else {
                        return Err(FlowLayoutError::ClusterTooWide {
                            advance: final_width,
                            available: width,
                        });
                    };
                    end = next;
                    if role != FlowTextRole::Code {
                        while end > minimum_end && discretionary::prohibited_boundary(text, runs, atoms[end].bytes.start, role)
                        {
                            end -= 1;
                        }
                    }
                };
                let has_math = fragments
                    .iter()
                    .any(|part| matches!(part, Fragment::Math { .. }));
                let direction = fragments
                    .iter()
                    .find_map(|part| match part {
                        Fragment::Text { run, .. } if !run.glyphs.is_empty() => Some(run.context.direction),
                        Fragment::Text { .. } => None,
                        Fragment::Math { .. } => None,
                    })
                    .unwrap_or(Direction::LeftToRight);
                if fragments.iter().any(|part| matches!(part, Fragment::Text { run, .. } if !run.glyphs.is_empty() && run.context.direction != direction))
                    || (has_math && direction == Direction::RightToLeft)
                {
                    return Err(FlowLayoutError::Shaping(
                        "mixed-direction inline mathematics requires paragraph-level bidi resolution"
                            .to_owned(),
                    ));
                }
                let measured = fragments.iter().all(|part| match part {
                    Fragment::Text { run, .. } => run.glyphs.is_empty() || run.vertical_metrics.is_some(),
                    Fragment::Math { .. } => true,
                });
                if has_math && !measured {
                    // Rebuild this unpublished line with literal source atoms.
                    // Earlier complete lines stay valid; no partial formula is
                    // emitted and unknown text metrics are never guessed.
                    for part in &fragments {
                        if let Fragment::Math { run_index, .. } = part {
                            inventory.fallback(*run_index, MISSING_METRICS);
                        }
                    }
                    start = atoms[first].bytes.start;
                    retry = true;
                    break;
                }
                let mut ascent = 0.0f32;
                let mut descent = 0.0f32;
                if measured {
                    for part in &fragments {
                        let (above, below) = match part {
                            Fragment::Text { run, .. } if run.glyphs.is_empty() => (0.0, 0.0),
                            Fragment::Text { run, .. } => {
                                let metrics = run
                                    .vertical_metrics
                                    .ok_or(FlowLayoutError::InvalidShapedRun)?;
                                if !metrics.is_valid() {
                                    return Err(FlowLayoutError::InvalidShapedRun);
                                }
                                (metrics.ascent, metrics.descent)
                            }
                            Fragment::Math { metrics, .. } => (metrics.ascent, metrics.descent),
                        };
                        ascent = ascent.max(above);
                        descent = descent.max(below);
                    }
                }
                let line_y = y + height;
                let placement = self.begin_line(
                    line_y,
                    leading,
                    measured.then_some(TextRunMetrics { ascent, descent }),
                )?;
                let line_height = placement.height;
                let baseline = placement.baseline_y;
                let rtl = direction == Direction::RightToLeft;
                let mut cursor = if rtl
                    && matches!(self.alignment, Align::None | Align::Right | Align::Decimal)
                {
                    x + width
                } else {
                    let left = self.line_start(x, width, final_width, direction);
                    if rtl { left + final_width } else { left }
                };
                for part in fragments {
                    let advance = part.advance();
                    if rtl {
                        cursor -= advance;
                    }
                    let bounds = DisplayRect::new(cursor, line_y, advance, line_height);
                    let (index, anchor_bounds) = match part {
                        Fragment::Text {
                            run,
                            run_index,
                            size,
                            fallback,
                        } => {
                            let spec = &runs[run_index];
                            let ink = if spec.active_link_target().is_some() {
                                "link"
                            } else if (spec.style.code && self.inline_styles) || fallback.is_some() {
                                "code"
                            } else {
                                color
                            };
                            self.item(DisplayItem::Text(DisplayTextRun {
                                fill: None,
                                bounds,
                                text: run.logical_text.clone(),
                                font_run: Some(run),
                                color_role: ink.to_owned(),
                                source_span: span,
                                font_size: size,
                                baseline_y: baseline,
                                math_fallback: fallback,
                            }))?;
                            (run_index, bounds)
                        }
                        Fragment::Math {
                            formula,
                            metrics,
                            run_index,
                        } => {
                            let baseline = baseline.ok_or(FlowLayoutError::InvalidShapedRun)?;
                            let placed = formula
                                .place_inline(cursor, baseline, metrics, span)
                                .map_err(FlowLayoutError::Shaping)?;
                            inventory.pending_commands -= placed.commands;
                            inventory.entries.remove(&run_index);
                            self.path_commands = self
                                .path_commands
                                .checked_add(placed.commands)
                                .filter(|count| *count <= MAX_DISPLAY_TOTAL_PATH_COMMANDS)
                                .ok_or(FlowLayoutError::BudgetExceeded("math path commands"))?;
                            self.list.include_layout_bounds(placed.bounds);
                            for mut path in placed.paths {
                                if runs[run_index].active_link_target().is_some()
                                    && path.fill.is_none()
                                {
                                    path.color_role = "link".to_owned();
                                }
                                self.item(DisplayItem::Path(path))?;
                            }
                            (run_index, placed.bounds)
                        }
                    };
                    let spec = &runs[index];
                    if spec.style.strikethrough && advance > 0.0 {
                        self.vector(
                            DisplayRect::new(
                                cursor,
                                baseline.map_or(line_y + line_height * 0.5, |y| y - size * 0.3),
                                advance,
                                1.0,
                            ),
                            VectorShapeType::HorizontalRule,
                            "strikethrough",
                            span,
                        )?;
                    }
                    if let Some(target) = spec.active_link_target() {
                        self.link_anchor(target, anchor_bounds, span)?;
                    }
                    if !rtl {
                        cursor += advance;
                    }
                }
                height += line_height;
                first = end;
                if let Some(next) = atoms.get(first)
                    && let Some(context_end) = next.continuation
                {
                    let context_start = next.bytes.start;
                    let replacement = self.math_atoms(
                        text, runs, context_start..context_end, size, role, &inventory, true,
                    )?;
                    let tail = atoms.partition_point(|part| part.bytes.end <= context_end);
                    atoms.splice(first..tail, replacement);
                }
            }
            if retry {
                continue;
            }
            if let Some(atom) = atoms.get(first) {
                start = atom.bytes.start;
            } else {
                start = window.end + usize::from(window.hard_break);
            }
        }
        if height == 0.0 {
            height = self.begin_line(y, leading, None)?.height;
        }
        Ok(height)
    }

    /// Column measurement sees formulas at natural width, but a formula's
    /// minimum allows uniform fitting. Ordinary text retains its true largest
    /// indivisible cluster, including unsupported formula source fallback.
    pub(super) fn inline_math_measures(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        role: FlowTextRole,
        available_width: f32,
    ) -> Result<(f64, f64), FlowLayoutError> {
        validate_ranges(text, runs)?;
        let mut intrinsic = text.chars().any(discretionary::is_hint)
            .then(|| discretionary::Intrinsic::new(self.options.max_shape_bytes));
        let mut inventory = Inventory::default();
        let mut start = 0usize;
        let mut minimum = 1.0f64;
        let mut preferred = 1.0f64;
        let mut physical_advance = 0.0f64;
        let mut group_advance = 0.0f64;
        let mut group_start = 0;
        while start < text.len() {
            inventory.retire_before(runs, start);
            let window = self.math_window(
                text,
                runs,
                start,
                self.options.body_size,
                available_width,
                role,
                &mut inventory,
            )?;
            let mut next = window.end;
            if window.end > start {
                let atoms = self.math_atoms(
                    text,
                    runs,
                    start..window.end,
                    self.options.body_size,
                    role,
                    &inventory,
                    false,
                )?;
                let settled = if window.hard_break || window.end == text.len() {
                    atoms.len()
                } else {
                    // Keep the final text word as right-hand shaping context,
                    // just as ordinary long-cell measurement does. A complete
                    // formula is also a safe boundary: its glyphs cannot join
                    // to the next host-shaped text fragment.
                    atoms
                        .iter()
                        .enumerate()
                        .rposition(|(index, atom)| {
                            atom.math
                                || text[atom.bytes.clone()]
                                    .chars()
                                    .all(|ch| ch == ' ' || ch == '\t')
                                || (index + 1 < atoms.len()
                                    && (discretionary::word_boundary(
                                        text, runs, atom.bytes.clone(), role,
                                    ) || discretionary::contains_boundary(
                                        text, runs, atom.bytes.clone(), role,
                                    )))
                        })
                        .map(|index| index + 1)
                        .ok_or(FlowLayoutError::BudgetExceeded(
                            "table shaping window cannot settle a word",
                        ))?
                };
                for atom in &atoms[..settled] {
                    if !discretionary::prohibited_boundary(text, runs, atom.bytes.start, role) {
                        group_advance = 0.0;
                        group_start = atom.bytes.start;
                    }
                    if atom.math {
                        let Some(Prepared::Formula { formula, .. }) =
                            inventory.entries.get(&atom.run_index)
                        else {
                            return Err(FlowLayoutError::InvalidShapedRun);
                        };
                        physical_advance += formula.natural_advance(self.options.body_size);
                        // Formula minimums retain uniform fitting; adjoining
                        // punctuation still consumes real text advance.
                        group_advance += 1.0;
                    } else {
                        group_advance += f64::from(atom.advance);
                        physical_advance += f64::from(atom.advance);
                    }
                    minimum = minimum.max(group_advance);
                    if let Some(intrinsic) = &mut intrinsic {
                        let cost = self.table_hint_cost(
                            text, runs, group_start..atom.bytes.end, group_advance, role,
                        )?;
                        intrinsic.push(
                            if atom.math { 1.0 } else { f64::from(atom.advance) }, cost,
                            &mut self.shape_bytes, self.options.max_total_shape_bytes,
                        )?;
                    }
                }
                next = atoms
                    .get(settled)
                    .map_or(window.end, |atom| atom.bytes.start);
            } else if !window.hard_break {
                return Err(FlowLayoutError::InvalidShapedRun);
            }
            if !physical_advance.is_finite() {
                return Err(FlowLayoutError::InvalidShapedRun);
            }
            preferred = preferred.max(physical_advance);
            if window.hard_break {
                physical_advance = 0.0;
                group_advance = 0.0;
                if let Some(intrinsic) = &mut intrinsic {
                    intrinsic.finish_line();
                }
                next += 1;
            }
            if next <= start {
                return Err(FlowLayoutError::BudgetExceeded(
                    "table shaping window made no progress",
                ));
            }
            start = next;
        }
        if let Some(intrinsic) = &intrinsic {
            minimum = minimum.max(intrinsic.minimum());
        }
        Ok((minimum, preferred.max(minimum)))
    }

    fn prepare_inline_math(
        &mut self,
        text: &str,
        spec: &FlowInlineRun,
        index: usize,
        size: f32,
        width: f32,
        inventory: &mut Inventory,
    ) -> Result<(), FlowLayoutError> {
        if inventory.entries.contains_key(&index) {
            return Ok(());
        }
        let Some(style) = spec.math else {
            return Ok(());
        };
        let source = &text[spec.range.clone()];
        self.shape_bytes = self
            .shape_bytes
            .checked_add(source.len())
            .filter(|bytes| *bytes <= self.options.max_total_shape_bytes)
            .ok_or(FlowLayoutError::BudgetExceeded("total shaping bytes"))?;
        let remaining_bytes = self.options.max_total_shape_bytes - self.shape_bytes;
        let remaining_commands = MAX_DISPLAY_TOTAL_PATH_COMMANDS
            .checked_sub(self.path_commands)
            .and_then(|count| count.checked_sub(inventory.pending_commands))
            .ok_or(FlowLayoutError::BudgetExceeded("math path commands"))?;
        let limit = remaining_commands.min(remaining_bytes / COMMAND_BYTES);
        let style = match style {
            FlowMathStyle::Text => fmd_math::Style::Text,
            FlowMathStyle::Display => fmd_math::Style::Display,
        };
        match self.math.typeset_style(source, style, limit) {
            Ok(formula) => {
                let metrics = match formula.inline_metrics(size, width) {
                    Ok(metrics) => metrics,
                    Err(reason) => {
                        inventory.fallback(index, &reason);
                        return Ok(());
                    }
                };
                self.shape_bytes += formula.command_count() * COMMAND_BYTES;
                inventory.pending_commands += formula.command_count();
                inventory.entries.insert(
                    index,
                    Prepared::Formula {
                        formula: Arc::new(formula),
                        metrics,
                    },
                );
            }
            Err(MathFailure::Fallback(reason)) => inventory.fallback(index, &reason),
            Err(MathFailure::Budget) => {
                return Err(FlowLayoutError::BudgetExceeded(
                    if remaining_bytes / COMMAND_BYTES < remaining_commands {
                        "total shaping bytes"
                    } else {
                        "math path commands"
                    },
                ));
            }
        }
        Ok(())
    }

    /// Count an admitted formula as one scratch atom, not as thousands of TeX
    /// bytes or false hard breaks. Text still obeys the per-shaper byte ceiling.
    #[allow(clippy::too_many_arguments)]
    fn math_window(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        start: usize,
        size: f32,
        width: f32,
        role: FlowTextRole,
        inventory: &mut Inventory,
    ) -> Result<Window, FlowLayoutError> {
        let mut used = 0usize;
        let mut safe = None;
        let first = runs.partition_point(|run| run.range.end <= start);
        for (index, spec) in runs.iter().enumerate().skip(first) {
            let position = start.max(spec.range.start);
            self.prepare_inline_math(text, spec, index, size, width, inventory)?;
            if matches!(
                inventory.entries.get(&index),
                Some(Prepared::Formula { .. })
            ) {
                if position != spec.range.start {
                    return Err(FlowLayoutError::InvalidShapedRun);
                }
                if used >= self.options.max_shape_bytes.max(1) && position > start {
                    return Ok(Window {
                        end: position,
                        hard_break: false,
                    });
                }
                used = used.saturating_add(1);
                safe = Some(spec.range.end);
                // Inspect the following ordinary span even when the window is
                // full: an immediate hard break can settle this exact line.
                continue;
            }
            let source = &text[position..spec.range.end];
            let newline = source.find('\n').map(|offset| position + offset);
            let end = newline.unwrap_or(spec.range.end);
            let available = self.options.max_shape_bytes.saturating_sub(used);
            let admitted = end.min(position.saturating_add(available));
            if position > start && cjk_boundary(text, position)
                && !discretionary::prohibited_boundary(text, runs, position, role)
            {
                safe = Some(position);
            }
            if let Some(candidate) = discretionary::measurement_boundary(text, runs, role, position, end, available) {
                safe = Some(candidate);
            }
            if admitted < end {
                let end =
                    safe.filter(|end| *end > start)
                        .ok_or(FlowLayoutError::BudgetExceeded(
                            "word exceeds shaping window",
                        ))?;
                return Ok(Window {
                    end,
                    hard_break: false,
                });
            }
            used += end - position;
            if newline.is_some() {
                return Ok(Window {
                    end,
                    hard_break: true,
                });
            }
        }
        Ok(Window {
            end: text.len(),
            hard_break: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn math_atoms(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        window: Range<usize>,
        size: f32,
        role: FlowTextRole,
        inventory: &Inventory,
        split_hints: bool,
    ) -> Result<Vec<Atom>, FlowLayoutError> {
        let first = runs.partition_point(|run| run.range.end <= window.start);
        let mut atoms = Vec::new();
        for (index, spec) in runs.iter().enumerate().skip(first) {
            if spec.range.start >= window.end {
                break;
            }
            let start = spec.range.start.max(window.start);
            let end = spec.range.end.min(window.end);
            if let Some(Prepared::Formula { metrics, .. }) = inventory.entries.get(&index) {
                if start != spec.range.start || end != spec.range.end {
                    return Err(FlowLayoutError::InvalidShapedRun);
                }
                atoms.push(Atom {
                    bytes: start..end,
                    run_index: index,
                    advance: metrics.advance,
                    word_break: cjk_boundary(text, end),
                    math: true,
                    continuation: None,
                });
                continue;
            }
            let mut style = spec.style;
            if spec.math.is_some() {
                style.code = true;
            }
            let measured = self.shaped_with_style(
                &text[start..end],
                if spec.math.is_some() {
                    self.options.code_size * (size / self.options.body_size)
                } else {
                    self.inline_size(size, style)
                },
                role,
                style,
            )?;
            for cluster in discretionary::measured_clusters(
                text, runs, start, &measured, role, split_hints,
            ) {
                let word_break = spec.math.is_none()
                    && discretionary::word_boundary(text, runs, cluster.bytes.clone(), role);
                atoms.push(Atom {
                    bytes: cluster.bytes,
                    run_index: index,
                    advance: cluster.advance,
                    word_break,
                    math: false,
                    continuation: cluster.continuation,
                });
            }
        }
        Ok(atoms)
    }

    fn math_fragments(
        &mut self,
        text: &str,
        runs: &[FlowInlineRun],
        atoms: &[Atom],
        size: f32,
        role: FlowTextRole,
        inventory: &Inventory,
    ) -> Result<Vec<Fragment>, FlowLayoutError> {
        let mut fragments = Vec::new();
        let selected = atoms
            .last()
            .and_then(|atom| discretionary::at_boundary(text, runs, atom.bytes.end, role))
            .and_then(|hint| hint.hyphen);
        let mut cursor = 0usize;
        while cursor < atoms.len() {
            let atom = &atoms[cursor];
            let index = atom.run_index;
            if atom.math {
                let Some(Prepared::Formula { formula, metrics }) = inventory.entries.get(&index)
                else {
                    return Err(FlowLayoutError::InvalidShapedRun);
                };
                fragments.push(Fragment::Math {
                    formula: Arc::clone(formula),
                    metrics: *metrics,
                    run_index: index,
                });
                cursor += 1;
                continue;
            }
            let start = atom.bytes.start;
            let mut end = atom.bytes.end;
            cursor += 1;
            while cursor < atoms.len() && !atoms[cursor].math && atoms[cursor].run_index == index {
                end = atoms[cursor].bytes.end;
                cursor += 1;
            }
            let spec = &runs[index];
            let fallback = match inventory.entries.get(&index) {
                Some(Prepared::Fallback(reason)) => Some(reason.clone()),
                _ => None,
            };
            let mut style = spec.style;
            if fallback.is_some() {
                style.code = true;
            }
            let size = if fallback.is_some() {
                self.options.code_size * (size / self.options.body_size)
            } else {
                self.inline_size(size, style)
            };
            let run = self.shaped_discretionary(
                &text[start..end],
                size,
                role,
                style,
                selected.filter(|at| (start..end).contains(at)).map(|at| at - start),
            )?;
            fragments.push(Fragment::Text {
                run,
                run_index: index,
                size,
                fallback,
            });
        }
        Ok(fragments)
    }
}
