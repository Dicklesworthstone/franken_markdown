//! Supported Latin mark positioning, shared by PDF measurement and painting.
//!
//! Canonically composable clusters keep the historical PDF path. Remaining
//! marks use the existing strict OpenType shaper in their actual face. Words
//! are separate shaping units: adding unrelated words to a painted segment
//! cannot exhaust a previously admitted word's budget or make a failed word
//! disable positioning in its neighbours. This is not a bidi/script engine.

use super::{Face, FontSize, LayoutUnit, Line, RenderWarning, text_composition};
use crate::text::shaping::{Direction, ShapeErrorKind, ShapeOptions};
use std::collections::BTreeMap;
use std::sync::Arc;

const MAX_WORD_BYTES: usize = 64 * 1024;
const MAX_WORD_SCALARS: usize = 4096;
const MAX_CACHE_ENTRIES: usize = 1024;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Glyph {
    pub id: u16,
    /// Design units multiplied by 1000. The shared denominator preserves both
    /// strict font coordinates and the legacy 1/1000-em glyph/pair metrics.
    pub advance: i64,
    pub offset_x: i64,
    pub offset_y: i64,
    pub above: i64,
    pub below: i64,
    /// Spacing belongs after the complete source cluster, never between a
    /// base and its zero-advance marks. Ligatures form one retained cluster.
    pub cluster_end: bool,
    pub word_space: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Run {
    pub glyphs: Vec<Glyph>,
    pub unicode: Vec<(u16, String)>,
    denominator: i64,
}

impl Run {
    pub fn spacing_gaps(&self) -> (usize, usize) {
        self.glyphs
            .iter()
            .take(self.glyphs.len().saturating_sub(1))
            .filter(|glyph| glyph.cluster_end)
            .fold((0, 0), |(letters, words), glyph| {
                (letters + 1, words + usize::from(glyph.word_space))
            })
    }

    pub fn width(&self, size: FontSize) -> LayoutUnit {
        self.glyphs
            .iter()
            .fold(LayoutUnit::ZERO, |width, glyph| {
                width + self.measure(glyph.advance, size)
            })
            .max(LayoutUnit::ZERO)
    }

    pub fn measure(&self, value: i64, size: FontSize) -> LayoutUnit {
        let points =
            i128::from(value) * i128::from(size.milli_points()) / i128::from(self.denominator);
        LayoutUnit::from_milli_points(points.clamp(i32::MIN as i128, i32::MAX as i128) as i32)
    }

    pub fn ink(&self, size: FontSize) -> (f32, f32) {
        let above = self
            .glyphs
            .iter()
            .map(|glyph| glyph.above)
            .max()
            .unwrap_or(0);
        let below = self
            .glyphs
            .iter()
            .map(|glyph| glyph.below)
            .max()
            .unwrap_or(0);
        // Round ink outward, rather than allowing a sub-millipoint rounding
        // error to defeat the enclosing line's containment guarantee.
        let ceil = |value: i64| {
            let value = i128::from(value.max(0)) * i128::from(size.milli_points());
            let denominator = i128::from(self.denominator);
            let points = (value + denominator - 1) / denominator;
            points.min(i32::MAX as i128) as f32 / 1000.0
        };
        (ceil(above), ceil(below))
    }
}

#[derive(Clone)]
struct Failure {
    limit: bool,
    source: String,
    reason: String,
}

#[derive(Clone, Default)]
pub(super) struct Prepared {
    pub run: Option<Arc<Run>>,
    failures: Vec<Failure>,
}

impl Prepared {
    pub fn warnings(&self, out: &mut Vec<RenderWarning>) {
        for failure in &self.failures {
            let warning = if failure.limit {
                RenderWarning::TextShapingLimit {
                    source: failure.source.clone(),
                    reason: failure.reason.clone(),
                }
            } else {
                RenderWarning::TextShapingUnsupported {
                    source: failure.source.clone(),
                    reason: failure.reason.clone(),
                }
            };
            if !out.contains(&warning) {
                out.push(warning);
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Cache {
    entries: BTreeMap<String, Prepared>,
    bytes: usize,
}

pub(super) fn prepare(face: &Face, source: &str) -> Prepared {
    if source.is_ascii() || !source.chars().any(text_composition::is_mark) {
        return Prepared::default();
    }
    if let Some(prepared) = face.positioned.borrow().entries.get(source) {
        return prepared.clone();
    }
    let text = text_composition::glyph_text(source, &face.font);
    if !text.chars().any(text_composition::is_mark) {
        return Prepared::default();
    }
    let prepared = prepare_uncached(face, &text);
    let failure_bytes = prepared.failures.iter().fold(0usize, |bytes, failure| {
        bytes
            .saturating_add(std::mem::size_of::<Failure>())
            .saturating_add(failure.source.len())
            .saturating_add(failure.reason.len())
    });
    let bytes = source
        .len()
        .saturating_add(std::mem::size_of::<Prepared>())
        .saturating_add(failure_bytes)
        .saturating_add(prepared.run.as_ref().map_or(0, |run| {
            run.glyphs
                .len()
                .saturating_mul(std::mem::size_of::<Glyph>())
                + run
                    .unicode
                    .iter()
                    .map(|(_, text)| text.len())
                    .sum::<usize>()
        }));
    let mut cache = face.positioned.borrow_mut();
    if cache.entries.len() < MAX_CACHE_ENTRIES
        && cache.bytes.saturating_add(bytes) <= MAX_CACHE_BYTES
    {
        cache.bytes += bytes;
        cache.entries.insert(source.to_owned(), prepared.clone());
    }
    prepared
}

fn prepare_uncached(face: &Face, source: &str) -> Prepared {
    let mut result = Prepared::default();
    let upm = i64::from(face.font.units_per_em.max(1));
    let mut run = Run {
        glyphs: Vec::new(),
        unicode: Vec::new(),
        denominator: upm * 1000,
    };
    let mut positioned_end = 0;
    // Keep whitespace as its own source unit. This makes both admission and
    // the font shaper's bounded work independent of segment merging/cache hits.
    let mut chars = source.char_indices().peekable();
    while let Some((start, first)) = chars.next() {
        let space = first.is_whitespace();
        while chars
            .peek()
            .is_some_and(|(_, ch)| ch.is_whitespace() == space)
        {
            chars.next();
        }
        let end = chars.peek().map_or(source.len(), |(offset, _)| *offset);
        let text = &source[start..end];
        if text.chars().any(text_composition::is_mark) {
            let mut word = Run {
                glyphs: Vec::new(),
                unicode: Vec::new(),
                denominator: run.denominator,
            };
            match positioned_word(face, text, &mut word) {
                Ok(()) => {
                    // Delay legacy allocation until there is an admitted
                    // word. A refused giant word needs no second full glyph
                    // array just to discover that the old path will paint it.
                    let first = run.glyphs.len();
                    append_legacy(face, &source[positioned_end..start], &mut run);
                    bridge_pair(face, &mut run, first);
                    let first = run.glyphs.len();
                    run.glyphs.extend(word.glyphs);
                    run.unicode.extend(word.unicode);
                    bridge_pair(face, &mut run, first);
                    positioned_end = end;
                }
                Err((limit, reason)) => {
                    // The strict adapter is transactional: it appends no
                    // glyphs on refusal, preserving the entire literal word.
                    let sample = text.chars().take(80).collect();
                    if !result.failures.iter().any(|failure| {
                        failure.limit == limit
                            && failure.source == sample
                            && failure.reason == reason
                    }) {
                        result.failures.push(Failure {
                            limit,
                            source: sample,
                            reason,
                        });
                    }
                }
            }
        }
    }
    if positioned_end > 0 {
        let first = run.glyphs.len();
        append_legacy(face, &source[positioned_end..], &mut run);
        bridge_pair(face, &mut run, first);
        result.run = Some(Arc::new(run));
    }
    result
}

fn bridge_pair(face: &Face, run: &mut Run, first: usize) {
    // Existing pair positioning at source-unit boundaries remains part of
    // the measured run; no mark is attached across a space/style face.
    if first > 0 && first < run.glyphs.len() {
        let upm = i64::from(face.font.units_per_em.max(1));
        let right = run.glyphs[first].id;
        let left = &mut run.glyphs[first - 1];
        let adjustment = i64::from(face.kern.pair(left.id, right)) * 1000 / upm;
        left.advance += adjustment * upm;
    }
}

fn positioned_word(face: &Face, source: &str, run: &mut Run) -> Result<(), (bool, String)> {
    if source.len() > MAX_WORD_BYTES
        || source.chars().take(MAX_WORD_SCALARS + 1).count() > MAX_WORD_SCALARS
    {
        return Err((
            true,
            "marked word exceeds the 64 KiB/4096-scalar shaping limit".into(),
        ));
    }
    let shaped = face
        .font
        .shape(source, &ShapeOptions::default())
        .map_err(|error| {
            (
                error.kind == ShapeErrorKind::BudgetExceeded,
                error.to_string(),
            )
        })?;
    if shaped.direction != Direction::LeftToRight || shaped.logical_text() != source {
        return Err((
            false,
            "strict Latin shaper returned an inconsistent source run".into(),
        ));
    }
    let mut glyphs = Vec::with_capacity(shaped.glyphs.len());
    let mut unicode = Vec::new();
    let mut previous = None;
    let mut covered = 0;
    for (index, glyph) in shaped.glyphs.iter().enumerate() {
        let Some(cluster) = source
            .get(glyph.cluster.clone())
            .filter(|text| !text.is_empty())
        else {
            return Err((
                false,
                "strict Latin shaper returned an invalid source cluster".into(),
            ));
        };
        if glyph.glyph_id == 0
            || glyph.glyph_id >= face.font.num_glyphs
            || glyph.x_advance < 0
            || glyph.y_advance != 0
            || glyph.x_offset.unsigned_abs() > 1_000_000
            || glyph.y_offset.unsigned_abs() > 1_000_000
            || glyph.x_advance > 1_000_000
        {
            return Err((
                false,
                "strict Latin positioning exceeds the supported horizontal metrics".into(),
            ));
        }
        if previous.as_ref() != Some(&glyph.cluster) {
            if glyph.cluster.start != covered {
                return Err((
                    false,
                    "strict Latin shaper returned noncontiguous source clusters".into(),
                ));
            }
            covered = glyph.cluster.end;
            previous = Some(glyph.cluster.clone());
        }
        let bbox = face.font.glyph_bbox(glyph.glyph_id).unwrap_or([0; 4]);
        glyphs.push(Glyph {
            id: glyph.glyph_id,
            advance: i64::from(glyph.x_advance) * 1000,
            offset_x: i64::from(glyph.x_offset) * 1000,
            offset_y: i64::from(glyph.y_offset) * 1000,
            above: (i64::from(glyph.y_offset) + i64::from(bbox[3])).max(0) * 1000,
            below: (-i64::from(glyph.y_offset) - i64::from(bbox[1])).max(0) * 1000,
            cluster_end: shaped
                .glyphs
                .get(index + 1)
                .is_none_or(|next| next.cluster != glyph.cluster),
            word_space: cluster.chars().any(|ch| matches!(ch, ' ' | '\u{00a0}')),
        });
        // Alternate/ligature glyphs absent from the source cmap still need a
        // ToUnicode entry. ActualText on this occurrence retains exact source;
        // never override a normal base/mark glyph's global Unicode mapping.
        if !cluster
            .chars()
            .any(|ch| face.font.glyph_index(ch) == glyph.glyph_id)
        {
            unicode.push((glyph.glyph_id, cluster.to_owned()));
        }
    }
    if covered != source.len() {
        return Err((
            false,
            "strict Latin shaper did not cover the complete source".into(),
        ));
    }
    run.glyphs.extend(glyphs);
    run.unicode.extend(unicode);
    Ok(())
}

fn append_legacy(face: &Face, source: &str, run: &mut Run) {
    let chars: Vec<_> = source.chars().collect();
    let raw: Vec<_> = chars.iter().map(|&ch| face.font.glyph_index(ch)).collect();
    let shaped = face.lig.substitute_with_spans(&raw);
    let upm = i64::from(face.font.units_per_em.max(1));
    let mut source_at = 0;
    let mut word_space = false;
    for (index, &(id, count)) in shaped.iter().enumerate() {
        let advance = i64::from(super::glyph_advance_1000(&face.font, id)) * upm;
        let pair = shaped.get(index + 1).map_or(0, |&(next, _)| {
            i64::from(face.kern.pair(id, next)) * 1000 / upm * upm
        });
        let bbox = face.font.glyph_bbox(id).unwrap_or([0; 4]);
        word_space |= chars
            .get(source_at..source_at + count)
            .unwrap_or(&[])
            .iter()
            .any(|ch| matches!(ch, ' ' | '\u{00a0}'));
        let cluster_end = chars
            .get(source_at + count)
            .is_none_or(|&ch| !text_composition::is_mark(ch));
        run.glyphs.push(Glyph {
            id,
            advance: advance + pair,
            offset_x: 0,
            offset_y: 0,
            above: i64::from(bbox[3]).max(0) * 1000,
            below: (-i64::from(bbox[1])).max(0) * 1000,
            cluster_end,
            word_space,
        });
        if cluster_end {
            word_space = false;
        }
        if count > 1 {
            let text = chars
                .get(source_at..source_at + count)
                .unwrap_or(&[])
                .iter()
                .collect();
            run.unicode.push((id, text));
        }
        source_at += count;
    }
}

/// Stamp actual positioned glyph ink before any note admission/page planning.
/// Cached/alternate lines retain these metrics along with their source text.
pub(super) fn stamp_lines(lines: &mut [Line], faces: &super::Faces) {
    for line in lines {
        line.text_ink = (0.0, 0.0);
        for seg in &line.segs {
            if super::is_inline_object(seg.slot) {
                continue;
            }
            if let Some(run) = prepare(faces.face(seg.slot), &seg.text).run {
                let (above, below) = run.ink(super::font_size_of(line.size));
                line.text_ink.0 = line.text_ink.0.max(above);
                line.text_ink.1 = line.text_ink.1.max(below);
            }
        }
    }
}

/// KP boxes may measure emergency pieces of a word independently. Rejoining
/// them can change strict-shaper admission (for example a later unsupported
/// mark makes the complete word fall back to nonzero hmtx mark advances).
/// Final lines must fit their actual joined glyph run before pagination.
/// Long prose words and fixed-width inline code can also make a visibly
/// overfull line cheaper than several underfull no-glue lines in the optimizer.
/// Recover those lines without changing source spacing or shrinking glyphs.
/// Ordinary fitting KP output is retained exactly. Existing cluster-aware cell
/// wrapping keeps styles, links, and every source cluster together.
pub(super) fn append_fitted_line(
    out: &mut Vec<Line>,
    line: Line,
    left: f32,
    right: f32,
    cx: &super::LayoutCx<'_>,
) {
    let overfull = line
        .segs
        .last()
        .is_some_and(|seg| seg.x + seg.width > right + 0.01);
    if !overfull {
        out.push(line);
        return;
    }
    let mut toks: Vec<super::Tok> = Vec::new();
    for token in &cx.line_toks {
        let tok = &token.tok;
        if let Some(last) = toks.last_mut()
            && !last.space
            && !tok.space
            && !super::is_inline_object(tok.slot)
            && last.slot == tok.slot
            && last.link == tok.link
            && last.strike == tok.strike
            && last.literal == tok.literal
        {
            last.text.push_str(&tok.text);
        } else {
            toks.push(tok.clone());
        }
    }
    let wrapped = super::wrap_cell_styled(
        &toks,
        (right - left).max(1.0),
        line.size,
        cx.faces,
        &cx.width_cache,
    );
    if wrapped.is_empty() {
        out.push(line);
        return;
    }
    let last = wrapped.len() - 1;
    for (index, row) in wrapped.into_iter().enumerate() {
        let mut repaired = line.clone();
        repaired.segs.clear();
        repaired.gap_after = if index == last { line.gap_after } else { 0.0 };
        repaired.hyphenated = index == last && line.hyphenated;
        let mut x = left;
        for run in row.runs {
            let width = run.width;
            repaired.segs.push(super::Seg {
                x,
                slot: run.slot,
                text_hash: super::seg_text_hash(&run.text),
                text: run.text,
                link: super::resolve_seg_link(&cx.links.targets, run.link),
                fill: if run.link.is_some() {
                    super::Fill::Link
                } else {
                    super::Fill::Black
                },
                strike: run.strike,
                task: None,
                width,
                expansion_permille: 0,
            });
            x += width;
        }
        out.push(repaired);
    }
}

/// Paint exactly the quantized advances used by `Run::width`, with the same
/// retained offsets. Absolute glyph matrices avoid accumulating PDF font-width
/// rounding, and work for SVG text's rotations/shears as well as normal prose.
pub(super) fn append_glyphs(
    out: &mut String,
    run: &Run,
    map: &[u16],
    matrix: super::SvgTextMatrix,
    letter_adjust: i32,
    word_adjust: i32,
) {
    let size = super::font_size_of(matrix.size);
    let mut pen = LayoutUnit::ZERO;
    let mut spacing = 0.0;
    for (index, glyph) in run.glyphs.iter().enumerate() {
        let x = (pen + run.measure(glyph.offset_x, size)).to_points_f32() + spacing;
        let y = run.measure(glyph.offset_y, size).to_points_f32();
        for component in [matrix.a, matrix.b, matrix.c, matrix.d] {
            append_matrix_coefficient(out, component);
            out.push(' ');
        }
        super::append_pdf_fixed2(out, matrix.x + matrix.a * x + matrix.c * y);
        out.push(' ');
        super::append_pdf_fixed2(out, matrix.y + matrix.b * x + matrix.d * y);
        out.push_str(" Tm <");
        super::append_hex_u16(out, map.get(usize::from(glyph.id)).copied().unwrap_or(0));
        out.push_str("> Tj ");
        pen += run.measure(glyph.advance, size);
        if glyph.cluster_end && index + 1 < run.glyphs.len() {
            let word = if glyph.word_space { word_adjust } else { 0 };
            spacing -= (letter_adjust.saturating_add(word) as f32 / 1000.0) * matrix.size;
        }
    }
}

/// Matrix coefficients are dimensionless: rounding them to the hundredth of
/// a point used for translations would erase milliem expansion and distort
/// SVG rotations. Six places retain those scales while trimming ordinary
/// identity coefficients back to their compact historical spelling.
fn append_matrix_coefficient(out: &mut String, value: f32) {
    super::append_pdf_fixed(out, value, 1_000_000);
    while out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_text(
    out: &mut String,
    run: &Run,
    map: &[u16],
    slot: u8,
    size: f32,
    x: f32,
    y: f32,
    expansion_permille: i16,
    render_mode: Option<u8>,
) {
    out.push_str("BT /F");
    super::append_decimal_u64_string(out, u64::from(slot));
    out.push(' ');
    super::append_pdf_fixed2(out, size);
    out.push_str(" Tf ");
    if let Some(mode) = render_mode {
        super::append_decimal_u64_string(out, u64::from(mode));
        out.push_str(" Tr ");
    }
    append_glyphs(
        out,
        run,
        map,
        super::SvgTextMatrix {
            a: 1.0 + f32::from(expansion_permille) / 1000.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            x,
            y,
            size,
        },
        0,
        0,
    );
    if render_mode.is_some() {
        out.push_str("0 Tr ");
    }
    out.push_str("ET\n");
}
