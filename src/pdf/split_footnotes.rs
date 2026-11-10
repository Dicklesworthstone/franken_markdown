//! Opt-in continuation of text-bearing footnotes in the final physical page walk.
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

/// Text-bearing containers already retain independently paintable rows and
/// source-owned list/quote/code identities. Inline formulas and images remain
/// atomic parts of those rows. Tables, standalone figures/formulas, explicit
/// breaks and generated TOCs need their own split rules and keep the complete
/// ordinary-note/endnote fallback.
pub(super) fn eligible(blocks: &[Block]) -> bool {
    fn text(inlines: &[Inline]) -> bool {
        let mut pending = vec![inlines];
        let mut has_image = false;
        let mut has_content = false;
        while let Some(inlines) = pending.pop() {
            for inline in inlines {
                match inline {
                    Inline::Text(text) => has_content |= !text.trim().is_empty(),
                    Inline::Code(_)
                    | Inline::Math(_)
                    | Inline::DisplayMath(_)
                    | Inline::FootnoteRef { .. } => has_content = true,
                    Inline::Image { .. } => has_image = true,
                    Inline::SoftBreak | Inline::HardBreak => {}
                    Inline::Emphasis(inner)
                    | Inline::Strong(inner)
                    | Inline::Strikethrough(inner)
                    | Inline::Link { content: inner, .. } => pending.push(inner),
                    _ => return false,
                }
            }
        }
        // Image-only paragraphs can become full-size figures or galleries.
        // Preserve their ordinary page-note/endnote admission, including for
        // styled/linked images and assets whose availability changes later.
        !has_image || has_content
    }

    if blocks.is_empty() {
        return false;
    }
    let mut pending = vec![blocks];
    while let Some(blocks) = pending.pop() {
        for block in blocks {
            match block {
                Block::Paragraph(inlines)
                    if toc_marker_depth(inlines).is_none() && text(inlines) => {}
                Block::CodeBlock { lang, code }
                    if !crate::diagrams::is_diagram_code(code, lang.as_deref().unwrap_or("")) => {}
                Block::BlockQuote(inner) => pending.push(inner),
                Block::List(list) => {
                    pending.extend(list.items.iter().map(|item| item.blocks.as_slice()));
                }
                _ => return false,
            }
        }
    }
    true
}

/// An interior row can become a page's first or last row. Record real glyph
/// ink and font descent, retain decoration clearance at internal container
/// seams, and prove each mandatory unit fits a note-only page.
pub(super) fn prepare(lines: &mut [Line], faces: &Faces, capacity: f32) -> bool {
    let mut scratch = ShapeScratch::default();
    for line in lines.iter_mut() {
        if line.image.is_some()
            || line.rule
            || line.grid.is_some()
            || !matches!(
                line.flow.kind,
                FlowKind::Paragraph | FlowKind::Heading | FlowKind::Code
            )
            || !line.gap_after.is_finite()
            || line.gap_after < 0.0
        {
            return false;
        }
        for seg in &line.segs {
            if is_inline_math(seg.slot) {
                let Some(formula) =
                    inline_math_fit::placement(&seg.text, line.size, seg.slot == F_DISPLAY_MATH)
                else {
                    return false;
                };
                line.text_ink.0 = line.text_ink.0.max(formula.ascent);
                line.text_ink.1 = line.text_ink.1.max(formula.descent);
                continue;
            }
            if seg.slot == F_IMAGE {
                let Some(image) = inline_image::image_box(&seg.text, line.size) else {
                    return false;
                };
                line.text_ink.0 = line.text_ink.0.max(image.height - image.descent);
                line.text_ink.1 = line.text_ink.1.max(image.descent);
                continue;
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
            if let Some(done) = seg.task {
                // The checkbox has a minimum physical size even at small
                // note type sizes. Its invisible marker text is not its ink.
                let side = (line.size * 0.72).clamp(5.5, 9.0);
                let stroke = if done {
                    (side * 0.16).max(0.9)
                } else {
                    (side * 0.11).max(0.8)
                } * 0.5;
                line.text_ink.0 = line.text_ink.0.max(side - line.size * 0.07 + stroke);
                line.text_ink.1 = line.text_ink.1.max(line.size * 0.07 + stroke);
            }
        }
    }
    // Backgrounds continue through rows in the same source container. Only
    // a real container boundary needs additional clearance; ordinary prose
    // and interior code rows retain their measured spacing.
    for index in 1..lines.len() {
        let extra = container_clearance(&lines[index - 1], &lines[index]);
        lines[index - 1].gap_after += extra;
    }
    let mut start = 0;
    while start < lines.len() {
        let end = unit_end(lines, start, lines.len());
        let height = fragment_height(&lines[start..end]);
        if !height.is_finite() || height <= 0.0 || height > capacity {
            return false;
        }
        start = end;
    }
    !lines.is_empty()
}

/// The same painted bounds as the writer. A continuing panel/quote is omitted
/// at an internal seam; its background intentionally surrounds both rows.
fn paint_ink(line: &Line, code_edge: bool, quote_edge: bool) -> (f32, f32) {
    let mut above = line.text_ink.0;
    let mut below = line.text_ink.1.max(line.size * INLINE_MATH_DESCENT_ROOM);
    if code_edge && line.bg != 0 {
        above = above.max((line.size * PANEL_ASCENT_FRAC).max(line.text_ink.0) + PANEL_PAD_V);
        below = below.max((line.size * PANEL_DESCENT_FRAC).max(line.text_ink.1) + PANEL_PAD_V);
    }
    if quote_edge && !line.quote_bars.is_empty() {
        let (quote_above, quote_below) = if let Some(image) = &line.image {
            let mut above = image.height_pt;
            let mut below = 0.0_f32;
            if image.marker_size.is_some() {
                let (size, y) = line_text_position(line, 0.0);
                above = above.max(y + size * 0.85);
                below = below.max(size * 0.20 - y);
            }
            (above, below)
        } else if line.spacing.leading.is_some() {
            spacing::table_bounds(line)
        } else {
            (
                (line.size * 0.85).max(line.text_ink.0),
                (line.size * 0.20).max(line.text_ink.1),
            )
        };
        above = above.max(quote_above + QUOTE_BG_PAD_V);
        below = below.max(quote_below + QUOTE_BG_PAD_V);
    }
    (above, below)
}

fn top_guard(line: &Line) -> f32 {
    (paint_ink(line, true, true).0 + line_baseline_lift(line) - line_leading(line)).max(0.0)
}

fn bottom_guard(line: &Line) -> f32 {
    (paint_ink(line, true, true).1 - line_baseline_lift(line) - line.gap_after).max(0.0)
}

fn container_clearance(before: &Line, after: &Line) -> f32 {
    let code_edge = before.bg != after.bg;
    let closes_quote = before
        .quote_bars
        .iter()
        .any(|(id, _)| !after.quote_bars.iter().any(|(next, _)| next == id));
    let opens_quote = after
        .quote_bars
        .iter()
        .any(|(id, _)| !before.quote_bars.iter().any(|(previous, _)| previous == id));
    if !code_edge && !closes_quote && !opens_quote {
        return 0.0;
    }
    let below = paint_ink(before, code_edge, closes_quote).1;
    let above = paint_ink(after, code_edge, opens_quote).0;
    let distance = before.gap_after + line_leading(after) + line_baseline_lift(before)
        - line_baseline_lift(after);
    (below + above - distance).max(0.0)
}

fn fragment_height(lines: &[Line]) -> f32 {
    lines
        .iter()
        .map(|line| line_leading(line) + line.gap_after)
        .sum::<f32>()
        + lines.first().map_or(0.0, top_guard)
        + lines.last().map_or(0.0, bottom_guard)
}

fn note_label_only(line: &Line) -> bool {
    let mut labelled = false;
    for seg in &line.segs {
        for (offset, _) in seg
            .text
            .char_indices()
            .filter(|(_, ch)| !ch.is_whitespace())
        {
            if !matches!(
                seg.note_spans.at(offset),
                Some(note_tags::Mark::Label(number)) if number == line.flow.note_owner
            ) {
                return false;
            }
            labelled = true;
        }
    }
    labelled
}

fn marker_only(line: &Line) -> bool {
    list_labels::has_label(line)
        && line
            .segs
            .iter()
            .all(|seg| seg.list_label || seg.text.trim().is_empty())
}

fn keeps_next(before: &Line, after: &Line) -> bool {
    if before.flow.kind == FlowKind::Heading
        && (before.flow.group == after.flow.group
            || before
                .quote_bars
                .first()
                .is_some_and(|(id, _)| after.quote_bars.iter().any(|(next, _)| next == id)))
    {
        return true;
    }
    if marker_only(before)
        && before.list_path.last().is_some_and(|item| {
            after
                .list_path
                .iter()
                .any(|next| item.list == next.list && item.item == next.item)
        })
    {
        return true;
    }
    // Rich-first notes have an inserted label paragraph. Bind its first
    // source row too, using semantic provenance rather than superscript text.
    // Ordinary paragraph-only notes keep their historical row boundaries.
    (after.bg != 0
        || !after.quote_bars.is_empty()
        || !after.list_path.is_empty()
        || (before.flow.group == after.flow.group && note_label_only(after)))
        && note_label_only(before)
}

/// The smallest consecutive unit that may end a fragment. Heading and label
/// chains are bounded by the immutable note range, including empty containers.
fn unit_end(lines: &[Line], start: usize, end: usize) -> usize {
    let mut next = start + 1;
    while next < end && keeps_next(&lines[next - 1], &lines[next]) {
        next += 1;
    }
    next
}

/// Mandatory space on a note's first citation page. Multiple marks on one
/// physical body row are admitted together against these same measurements.
pub(super) fn minimum_reserve(lines: &[Line]) -> f32 {
    if lines.first().is_some_and(|line| line.flow.split_note) {
        fragment_height(&lines[..unit_end(lines, 0, lines.len())])
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

/// Extend an admitted prefix without separating a heading or label from its
/// first content row. Each source row is measured once per extension probe.
fn extend(lines: &[Line], fragment: &mut Range<usize>, end: usize, budget: f32) {
    let mut height = fragment_height(&lines[fragment.clone()]);
    while fragment.end < end {
        let next_end = unit_end(lines, fragment.end, end);
        let mut candidate = if fragment.start >= fragment.end {
            height + top_guard(&lines[fragment.end])
        } else {
            height - bottom_guard(&lines[fragment.end - 1])
        };
        for line in &lines[fragment.end..next_end] {
            candidate += line_leading(line);
            candidate += line.gap_after;
        }
        candidate += bottom_guard(&lines[next_end - 1]);
        if candidate > budget + 0.001 {
            break;
        }
        fragment.end = next_end;
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
            range.start..unit_end(lines, range.start, range.end)
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
        if let Some(first) = lines[fragment.clone()].first() {
            y -= top_guard(first);
        }
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
        // Admission proved that every pending mandatory unit fits this space.
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
