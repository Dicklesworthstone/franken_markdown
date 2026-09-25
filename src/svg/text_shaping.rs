//! A single glyph-space contract for poster wrapping and painting.
//!
//! Uses the same clean-room GSUB ligature and GPOS pair readers as PDF. The
//! cache is scoped to a layout/drawing pass and built from the actual supplied
//! face, never from a family name that could identify a replaced font.

use std::cell::OnceCell;
use std::ops::Range;

use franken_markdown::text::{Kerning, Ligatures};
use super::{Op, Poster, RStyle, SvgWarning};

struct Tables {
    ligatures: Ligatures,
    kerning: Kerning,
}

pub(super) struct Shaper<'a> {
    poster: &'a Poster,
    tables: [OnceCell<Tables>; 6],
}

#[derive(Clone, Debug)]
pub(super) struct Glyph {
    pub(super) slot: usize,
    pub(super) id: u16,
    /// Natural advance, excluding the adjustment to the following glyph.
    advance: f64,
    /// Kept separate so the adjustment is discarded at a visual line break.
    kern_after: f64,
    visible: bool,
    missing: usize,
}

#[derive(Clone, Debug)]
pub(super) struct Cluster {
    pub(super) bytes: Range<usize>,
    advance: f64,
    trailing_kern: f64,
}

impl Cluster {
    fn advance(&self) -> f64 { self.advance }
    fn trailing_kern(&self) -> f64 { self.trailing_kern }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShapedText {
    pub(super) clusters: Vec<Cluster>,
    pub(super) glyphs: Vec<Glyph>,
    adjusted: bool,
}

impl ShapedText {
    pub(super) fn width(&self) -> f64 {
        self.width_of(0..self.clusters.len())
    }

    pub(super) fn width_of(&self, range: Range<usize>) -> f64 {
        let clusters = &self.clusters[range];
        clusters.iter().map(Cluster::advance).sum::<f64>()
            - clusters.last().map_or(0.0, Cluster::trailing_kern)
    }

    /// Width added to a slice when another cluster joins its right edge.
    /// A prefix's previous edge adjustment now becomes an interior adjustment.
    pub(super) fn extend_width(&self, start: usize, end: usize, width: f64) -> f64 {
        let restored = if end > start { self.clusters[end - 1].trailing_kern() } else { 0.0 };
        width + restored + self.clusters[end].advance() - self.clusters[end].trailing_kern()
    }

    pub(super) fn paint(&self, poster: &mut Poster, x: f64, baseline: f64, style: RStyle, size: f64) -> f64 {
        if self.adjusted {
            poster.warnings.push(SvgWarning {
                code: "svg_shaping_adjusted",
                message: "invalid optional font substitution or reversing pair adjustment; preserved source glyphs with bounded advances".to_owned(),
            });
        }
        let mut pen = x;
        for (index, glyph) in self.glyphs.iter().enumerate() {
            poster.missing += glyph.missing;
            if glyph.visible && glyph.id != 0 {
                poster.ops.push(Op::Glyph {
                    slot: glyph.slot, gid: glyph.id, x: pen, y: baseline,
                    size, ink: style.ink,
                });
            }
            pen += glyph.advance;
            if index + 1 < self.glyphs.len() { pen += glyph.kern_after; }
        }
        if style.strike && pen > x {
            poster.ops.push(Op::Rule {
                x1: x, y1: baseline - size * 0.28,
                x2: pen, y2: baseline - size * 0.28,
                ink: style.ink, w: (size * 0.05).max(0.5),
            });
        }
        pen
    }
}

impl<'a> Shaper<'a> {
    pub(super) fn new(poster: &'a Poster) -> Self {
        Self { poster, tables: std::array::from_fn(|_| OnceCell::new()) }
    }

    pub(super) fn shape(&self, text: &str, style: RStyle, size: f64) -> ShapedText {
        let source: Vec<_> = text.char_indices().map(|(byte, ch)| {
            let (slot, id) = self.poster.resolve(ch, style);
            (byte, ch, slot, id)
        }).collect();
        let mut shaped = ShapedText::default();
        let mut cursor = 0;
        while cursor < source.len() {
            let (byte, ch, slot, id) = source[cursor];
            // Code deliberately retains its existing unligated, unkerned hmtx
            // measurement. Missing glyphs and whitespace cannot be swallowed
            // by a ligature and keep their original reporting/spacing policy.
            let Some(face) = self.poster.faces[slot].as_ref()
                .filter(|_| !style.mono && id != 0 && !ch.is_whitespace()) else {
                push_cluster(&mut shaped, byte..byte + ch.len_utf8(), Glyph {
                    slot, id, advance: self.poster.advance(slot, ch, size),
                    kern_after: 0.0, visible: !ch.is_whitespace(),
                    missing: usize::from(id == 0 && !ch.is_whitespace()),
                });
                cursor += 1;
                continue;
            };
            let mut end = cursor + 1;
            while end < source.len() && source[end].2 == slot && source[end].3 != 0
                && !source[end].1.is_whitespace()
            {
                end += 1;
            }
            let tables = self.tables[slot].get_or_init(|| Tables {
                ligatures: face.gsub_ligatures(), kerning: face.gpos_kerning(),
            });
            let ids: Vec<_> = source[cursor..end].iter().map(|scalar| scalar.3).collect();
            let substituted = tables.ligatures.substitute_with_spans(&ids);
            // The shared reader is tolerant of malformed optional tables. A
            // malformed substitution must not erase source or invent a glyph.
            let valid = face.units_per_em != 0
                && substituted.iter().all(|&(gid, count)| gid != 0 && gid < face.num_glyphs && count > 0)
                && substituted.iter().try_fold(0usize, |n, &(_, count)| n.checked_add(count)) == Some(end - cursor);
            if !valid {
                shaped.adjusted = true;
                for &(byte, ch, slot, id) in &source[cursor..end] {
                    push_cluster(&mut shaped, byte..byte + ch.len_utf8(), Glyph {
                        slot, id, advance: self.poster.advance(slot, ch, size),
                        kern_after: 0.0, visible: true, missing: 0,
                    });
                }
                cursor = end;
                continue;
            }
            let factor = size / f64::from(face.units_per_em);
            let mut consumed = cursor;
            for (index, &(gid, count)) in substituted.iter().enumerate() {
                let next = consumed + count;
                let range = source[consumed].0..source[next - 1].0 + source[next - 1].1.len_utf8();
                let advance = f64::from(face.advance_width(gid)) * factor;
                let pair = substituted.get(index + 1)
                    .map_or(0.0, |&(following, _)| f64::from(tables.kerning.pair(gid, following)) * factor);
                // Never allow hostile optional kerning to reverse the pen.
                let kern_after = pair.max(-advance);
                shaped.adjusted |= kern_after != pair;
                push_cluster(&mut shaped, range, Glyph {
                    slot, id: gid, advance, kern_after, visible: true, missing: 0,
                });
                consumed = next;
            }
            cursor = end;
        }
        shaped
    }
}

fn push_cluster(shaped: &mut ShapedText, bytes: Range<usize>, glyph: Glyph) {
    let advance = glyph.advance + glyph.kern_after;
    let trailing_kern = glyph.kern_after;
    let zero = glyph.advance == 0.0;
    shaped.glyphs.push(glyph);
    // Zero-advance marks stay with their predecessor in emergency wrapping.
    // This is not a general mark-positioning or bidirectional shaping engine.
    if zero {
        if let Some(previous) = shaped.clusters.last_mut() {
            previous.bytes.end = bytes.end;
            previous.advance += advance;
            previous.trailing_kern = trailing_kern;
            return;
        }
    }
    shaped.clusters.push(Cluster { bytes, advance, trailing_kern });
}
