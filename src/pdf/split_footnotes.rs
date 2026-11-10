//! Opt-in continuation of paragraph footnotes in the final physical page walk.
//!
//! Pending work consists only of ranges into the immutable measured note
//! region. Every consumer creates its own cursor, so counts, running bands,
//! navigation, verification and both PDF writers see the same physical pages.

use std::collections::VecDeque;
use std::ops::Range;

use super::*;

pub(super) fn validate_options(options: &PdfOptions) -> Result<()> {
    if options.split_footnotes
        && (options.requests_optimal_pagination() || options.theme.page.baseline_grid.is_some())
    {
        return Err(RenderError::InvalidInput(
            "pdf_split_footnotes_incompatible: continuation requires greedy pagination without a baseline grid"
                .to_string(),
        ));
    }
    Ok(())
}

/// Rich, indivisible note blocks retain the established full-fidelity fallback.
/// Inline styles, code and links already have measured, independently paintable
/// paragraph rows; images, formulas and containers need separate split rules.
pub(super) fn eligible(blocks: &[Block]) -> bool {
    fn text(inlines: &[Inline]) -> bool {
        inlines.iter().all(|inline| match inline {
            Inline::Text(_)
            | Inline::Code(_)
            | Inline::SoftBreak
            | Inline::HardBreak
            | Inline::FootnoteRef { .. } => true,
            Inline::Emphasis(inner)
            | Inline::Strong(inner)
            | Inline::Strikethrough(inner)
            | Inline::Link { content: inner, .. } => text(inner),
            _ => false,
        })
    }
    !blocks.is_empty()
        && blocks
            .iter()
            .all(|block| matches!(block, Block::Paragraph(inlines) if text(inlines)))
}

/// An interior paragraph row can become a page's last row. Record real glyph
/// ink and the source font's descent before admission, rather than relying on
/// the final source paragraph's trailing gap to protect the page bottom.
pub(super) fn prepare(lines: &mut [Line], faces: &Faces, capacity: f32) -> bool {
    let mut scratch = ShapeScratch::default();
    for line in lines.iter_mut() {
        if line.image.is_some() || line.rule || line.grid.is_some() {
            return false;
        }
        for seg in &line.segs {
            if is_inline_object(seg.slot) {
                return false;
            }
            let face = faces.face(seg.slot);
            let scale = line.size / f32::from(face.font.units_per_em.max(1));
            line.text_ink.1 = line
                .text_ink
                .1
                .max((-f32::from(face.font.descent) * scale).max(0.0));
            let shaped = shape_run_with_scratch(face, &seg.text, &mut scratch);
            if let Some(positioned) = &shaped.positioned {
                let (above, below) = positioned.ink(font_size_of(line.size));
                line.text_ink.0 = line.text_ink.0.max(above);
                line.text_ink.1 = line.text_ink.1.max(below);
            } else {
                for glyph in shaped.glyphs {
                    if let Some(bounds) = face.font.glyph_bbox(glyph) {
                        line.text_ink.0 =
                            line.text_ink.0.max(f32::from(bounds[3]).max(0.0) * scale);
                        line.text_ink.1 = line
                            .text_ink
                            .1
                            .max((-f32::from(bounds[1])).max(0.0) * scale);
                    }
                }
            }
        }
        let height = fragment_height(std::slice::from_ref(line));
        if !height.is_finite()
            || height <= 0.0
            || height > capacity
            || !line.gap_after.is_finite()
            || line.gap_after < 0.0
        {
            return false;
        }
    }
    !lines.is_empty()
}

fn bottom_guard(line: &Line) -> f32 {
    (line.text_ink.1.max(line.size * INLINE_MATH_DESCENT_ROOM)
        - line_baseline_lift(line)
        - line.gap_after)
        .max(0.0)
}

fn fragment_height(lines: &[Line]) -> f32 {
    lines
        .iter()
        .map(|line| line_leading(line) + line.gap_after)
        .sum::<f32>()
        + lines.last().map_or(0.0, bottom_guard)
}

/// Mandatory space on a note's first citation page. Multiple marks on one
/// physical body row are admitted together against these same measurements.
pub(super) fn minimum_reserve(lines: &[Line]) -> f32 {
    if lines.first().is_some_and(|line| line.flow.split_note) {
        fragment_height(&lines[..1])
    } else {
        fragment_height(lines)
    }
}

#[derive(Default)]
pub(super) struct Cursor {
    pending: VecDeque<Range<usize>>,
}

struct Allocation {
    fragments: Vec<Range<usize>>,
    /// Only a consumed prefix and its shortened final range differ from the
    /// immutable input queue. Probing a page never copies its unconsumed tail.
    consumed_pending: usize,
    continued_pending: Option<Range<usize>>,
    new_pending: Vec<Range<usize>>,
    reserve: f32,
}

impl Cursor {
    fn commit(&mut self, allocation: Allocation) {
        for _ in 0..allocation.consumed_pending {
            self.pending.pop_front();
        }
        if let Some(range) = allocation.continued_pending {
            self.pending.push_front(range);
        }
        self.pending.extend(allocation.new_pending);
    }
}

/// Extend an already admitted prefix without splitting a physical text row.
fn extend(lines: &[Line], fragment: &mut Range<usize>, end: usize, budget: f32) {
    let mut height = fragment_height(&lines[fragment.clone()]);
    while fragment.end < end {
        let previous_guard = if fragment.start >= fragment.end {
            0.0
        } else {
            bottom_guard(&lines[fragment.end - 1])
        };
        let next = &lines[fragment.end];
        let candidate =
            height - previous_guard + line_leading(next) + next.gap_after + bottom_guard(next);
        if candidate > budget + 0.001 {
            break;
        }
        fragment.end += 1;
        height = candidate;
    }
}

/// Reserve every new note's first fragment before spending remaining space on
/// older continuations. The returned queue is speculative until its body break
/// is selected; probing another reserve cannot consume or duplicate content.
fn allocate(
    lines: &[Line],
    notes: &PageNotes,
    cursor: &Cursor,
    start: usize,
    end: usize,
    budget: f32,
) -> Allocation {
    let introduced = notes.notes_in(start, end);
    let mut fresh = Vec::with_capacity(introduced.len());
    let mut used = 0.0;
    for (_, range) in introduced {
        let fragment = if lines[range.start].flow.split_note {
            range.start..range.start + 1
        } else {
            range.clone()
        };
        used += fragment_height(&lines[fragment.clone()]);
        fresh.push(fragment);
    }
    let separator = &lines[notes.separator];
    let separator_height = line_leading(separator) + separator.gap_after;
    let available = (budget - separator_height).max(0.0);
    let mut consumed_pending = 0;
    let mut continued_pending = None;
    let mut new_pending = Vec::new();
    let mut fragments = Vec::new();
    for range in &cursor.pending {
        let mut fragment = range.start..range.start;
        extend(lines, &mut fragment, range.end, (available - used).max(0.0));
        if fragment.start >= fragment.end {
            break;
        }
        used += fragment_height(&lines[fragment.clone()]);
        let next = fragment.end;
        fragments.push(fragment);
        consumed_pending += 1;
        if next < range.end {
            continued_pending = Some(next..range.end);
            break;
        }
    }
    for (fragment, (_, whole)) in fresh.iter_mut().zip(introduced) {
        let before = fragment_height(&lines[fragment.clone()]);
        extend(
            lines,
            fragment,
            whole.end,
            before + (available - used).max(0.0),
        );
        used += fragment_height(&lines[fragment.clone()]) - before;
        if fragment.end < whole.end {
            new_pending.push(fragment.end..whole.end);
        }
    }
    fragments.extend(fresh);
    let reserve = if fragments.is_empty() {
        0.0
    } else {
        separator_height + used
    };
    Allocation {
        fragments,
        consumed_pending,
        continued_pending,
        new_pending,
        reserve,
    }
}

fn body_height(
    lines: &[Line],
    start: usize,
    end: usize,
    page: PageGeom,
    shrink_from: Option<usize>,
) -> f32 {
    let mut height = repeated_table_header_height(lines, start, page);
    for (index, line) in lines.iter().enumerate().take(end).skip(start) {
        height += line_leading(line);
        if index + 1 < end {
            height += match shrink_from {
                Some(from) if index >= from && is_block_boundary(lines, index) => {
                    flexed_gap(line.gap_after)
                }
                _ => line.gap_after,
            };
        }
    }
    height
}

fn place<'a>(
    placed: &mut Vec<Placed<'a>>,
    lines: &'a [Line],
    notes: &PageNotes,
    allocation: &Allocation,
    page: PageGeom,
) {
    if allocation.fragments.is_empty() {
        return;
    }
    let mut y = page.bottom + allocation.reserve;
    let separator = &lines[notes.separator];
    y -= line_leading(separator);
    placed.push(Placed {
        line: separator,
        y: placed_line_baseline(y, separator),
    });
    y -= separator.gap_after;
    for fragment in &allocation.fragments {
        for line in &lines[fragment.clone()] {
            y -= line_leading(line);
            placed.push(Placed {
                line,
                y: placed_line_baseline(y, line),
            });
            y -= line.gap_after;
        }
        if let Some(last) = lines[fragment.clone()].last() {
            y -= bottom_guard(last);
        }
    }
}

pub(super) fn next_page<'a>(
    lines: &'a [Line],
    notes: &PageNotes,
    cursor: &mut Cursor,
    start: &mut usize,
    page: PageGeom,
    emitted_any: &mut bool,
) -> Option<Vec<Placed<'a>>> {
    let body = &lines[..notes.body_end];
    let capacity = (page.top_y() - page.bottom).max(0.0);
    if *start >= body.len() {
        if cursor.pending.is_empty() {
            return None;
        }
        // Once the body is exhausted the whole page is available to notes.
        // Admission proved that every pending physical row fits this space.
        let allocation = allocate(lines, notes, cursor, *start, *start, capacity);
        debug_assert!(!allocation.fragments.is_empty());
        let mut placed = Vec::new();
        place(&mut placed, lines, notes, &allocation, page);
        cursor.commit(allocation);
        *emitted_any = true;
        return Some(placed);
    }

    // Body pages aim at the historical 40% note share. A citation may need
    // more for all its mandatory first fragments, but never loses its row.
    let first = &body[*start];
    let maximum = (capacity - line_leading(first) - first.gap_after).max(0.0);
    let preferred = (capacity * PAGE_NOTE_MAX_SHARE).min(maximum);
    let mut reserve = 0.0;
    let mut chosen: Option<(usize, Option<usize>, PageGeom, Allocation)> = None;
    for _ in 0..8 {
        let geometry = page.with_foot_reserve(reserve);
        let (end, shrink) = choose_page_break_with_void_control(body, *start, geometry);
        let allocation = allocate(lines, notes, cursor, *start, end, preferred);
        let need = allocation.reserve;
        if need <= reserve + 0.001
            && body_height(body, *start, end, geometry, shrink) + need <= capacity + 0.001
        {
            if chosen.as_ref().is_none_or(|(best, _, _, best_allocation)| {
                end > *best || (end == *best && need < best_allocation.reserve)
            }) {
                chosen = Some((end, shrink, geometry, allocation));
            }
            if (need - reserve).abs() <= 0.001 {
                break;
            }
        }
        reserve = need;
    }
    // Reserve oscillations can move a citation between pages. A single body
    // row and all its admitted first fragments always fit and make progress;
    // repeated table headings obey the same shortened-page admission.
    let (end, shrink, geometry, allocation) = chosen.unwrap_or_else(|| {
        let end = *start + 1;
        let allocation = allocate(lines, notes, cursor, *start, end, preferred);
        let geometry = page.with_foot_reserve(allocation.reserve);
        (end, None, geometry, allocation)
    });
    let mut placed = place_lines_shrunk(body, *start, end, geometry, shrink);
    place(&mut placed, lines, notes, &allocation, page);
    cursor.commit(allocation);
    *start = end;
    *emitted_any = true;
    Some(placed)
}
