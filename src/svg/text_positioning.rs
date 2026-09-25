//! Bounded Latin mark positioning through the existing strict OpenType engine.
//! The complete base/mark sequence uses one actual face. Glyphs stay in visual
//! order with their original source clusters; nothing is normalized or guessed.

use super::{Cluster, Glyph, RStyle, ShapedText, Shaper, SvgWarning, composition};
use franken_markdown::text::shaping::{Direction, Feature, ShapeErrorKind, ShapeOptions};

const MAX_BYTES: usize = 64 * 1024;
const MAX_SCALARS: usize = 4096;
const MAX_COORD: f64 = 1_000_000.0;

pub(super) fn shape(
    shaper: &Shaper<'_>, text: &str, style: RStyle, size: f64,
) -> Result<ShapedText, SvgWarning> {
    if text.len() > MAX_BYTES || text.chars().take(MAX_SCALARS + 1).count() > MAX_SCALARS {
        return Err(warning("svg_text_shaping_limit", "positioned text exceeds the 64 KiB/4096-scalar run limit"));
    }
    let mut result = ShapedText::default();
    let mut chars = text.char_indices().peekable();
    while let Some((start, first)) = chars.next() {
        let latin = latin_scalar(first);
        while chars.peek().is_some_and(|(_, ch)| latin_scalar(*ch) == latin) {
            chars.next();
        }
        let end = chars.peek().map_or(text.len(), |(offset, _)| *offset);
        let source = &text[start..end];
        let mut part = if source.chars().any(composition::is_mark) {
            if let Some(composed) = composition::shape(shaper, source, style, size) {
                composed
            } else {
                positioned(shaper, source, style, size)?
            }
        } else {
            shaper.shape_uncomposed(source, style, size)
        };
        let glyph_start = result.glyphs.len();
        for cluster in &mut part.clusters {
            cluster.bytes.start += start;
            cluster.bytes.end += start;
            cluster.glyphs.start += glyph_start;
            cluster.glyphs.end += glyph_start;
        }
        result.contextual |= part.contextual;
        result.adjusted |= part.adjusted;
        result.clusters.extend(part.clusters);
        result.glyphs.extend(part.glyphs);
    }
    Ok(result)
}

fn latin_scalar(ch: char) -> bool {
    matches!(ch as u32, 0x20..=0x024f | 0x0300..=0x036f)
}

fn positioned(
    shaper: &Shaper<'_>, text: &str, style: RStyle, size: f64,
) -> Result<ShapedText, SvgWarning> {
    let primary = if style.mono { 4 } else { usize::from(style.bold) + 2 * usize::from(style.italic) };
    let slot = [primary, 5].into_iter().find(|&slot| {
        shaper.poster.faces[slot].as_ref().is_some_and(|face| {
            face.units_per_em != 0 && text.chars().all(|ch| {
                let id = face.glyph_index(ch);
                id != 0 && id < face.num_glyphs
            })
        })
    }).ok_or_else(|| warning("svg_text_shaping_unsupported", "no active face covers the entire combining sequence; retained raw glyph fallback"))?;
    let face = shaper.poster.faces[slot].as_ref().ok_or_else(invalid)?;
    let code_features = [
        Feature { tag: *b"liga", enabled: false },
        Feature { tag: *b"kern", enabled: false },
    ];
    let options = ShapeOptions {
        features: if style.mono { &code_features } else { &[] },
        ..ShapeOptions::default()
    };
    let shaped = face.shape(text, &options).map_err(|error| {
        let code = if error.kind == ShapeErrorKind::BudgetExceeded {
            "svg_text_shaping_limit"
        } else {
            "svg_text_shaping_unsupported"
        };
        warning(code, &format!("strict Latin positioning refused: {error}; retained raw glyph fallback"))
    })?;
    if shaped.direction != Direction::LeftToRight || shaped.logical_text() != text {
        return Err(invalid());
    }
    let factor = size / f64::from(face.units_per_em);
    let mut result = ShapedText { contextual: true, ..ShapedText::default() };
    let mut index = 0;
    let mut byte = 0;
    let mut pen = 0.0;
    while index < shaped.glyphs.len() {
        let bytes = shaped.glyphs[index].cluster.clone();
        let source = text.get(bytes.clone()).filter(|s| !s.is_empty()).ok_or_else(invalid)?;
        if bytes.start != byte { return Err(invalid()); }
        let first = result.glyphs.len();
        let left = pen;
        while index < shaped.glyphs.len() && shaped.glyphs[index].cluster == bytes {
            let glyph = &shaped.glyphs[index];
            let advance = f64::from(glyph.x_advance) * factor;
            let offset_x = f64::from(glyph.x_offset) * factor;
            let offset_y = f64::from(glyph.y_offset) * factor;
            if glyph.glyph_id == 0 || glyph.glyph_id >= face.num_glyphs
                || glyph.y_advance != 0 || advance < 0.0
                || !valid(advance) || !valid(offset_x) || !valid(offset_y)
                || !valid(pen + advance) || !valid(pen + offset_x)
            {
                return Err(invalid());
            }
            result.glyphs.push(Glyph {
                slot, id: glyph.glyph_id, advance, offset_x, offset_y,
                visible: !source.chars().all(char::is_whitespace),
                ..Glyph::default()
            });
            pen += advance;
            index += 1;
        }
        byte = bytes.end;
        result.clusters.push(Cluster {
            bytes, glyphs: first..result.glyphs.len(), advance: pen - left, trailing_kern: 0.0,
        });
    }
    if byte != text.len() { return Err(invalid()); }
    Ok(result)
}

fn warning(code: &'static str, message: &str) -> SvgWarning {
    SvgWarning { code, message: message.to_owned() }
}

fn invalid() -> SvgWarning {
    warning("svg_text_shaping_unsupported", "invalid positioned text geometry or source coverage; retained raw glyph fallback")
}

fn valid(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_COORD
}
