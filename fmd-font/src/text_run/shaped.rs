//! Convert visual-order shaping output without moving the glyphs a second time.

use super::{Direction, OwnedTextRun, RunGlyph, ShapedRun, TextCluster, TextRunContext};

pub(super) fn build(
    context: TextRunContext,
    shaped: &ShapedRun,
    scale: f32,
) -> Result<OwnedTextRun, String> {
    if !scale.is_finite() || scale <= 0.0
        || !context.font_size.is_finite() || context.font_size <= 0.0
    {
        return Err("text run requires finite positive size and scale".to_owned());
    }
    if context.direction != shaped.direction {
        return Err("text run direction differs from shaping direction".to_owned());
    }
    let logical_text = shaped.logical_text().to_owned();
    let is_rtl = shaped.direction == Direction::RightToLeft;
    let mut byte_cursor = if is_rtl { logical_text.len() } else { 0 };
    let mut utf16_cursor = if is_rtl { logical_text.encode_utf16().count() } else { 0 };
    let mut clusters = Vec::new();
    let mut glyphs = Vec::with_capacity(shaped.glyphs.len());
    let mut pen = 0.0_f32;
    let mut index = 0;

    while index < shaped.glyphs.len() {
        let bytes = shaped.glyphs[index].cluster.clone();
        // A directional run has monotone, non-overlapping source clusters.
        // Repeated ranges are allowed only in one consecutive glyph group.
        // These checks also reject omitted source, empty/reversed ranges, and
        // scalar-interior boundaries before any run can escape to a painter.
        let source = logical_text.get(bytes.clone())
            .filter(|source| !source.is_empty())
            .ok_or_else(|| "invalid text run cluster byte range".to_owned())?;
        if (is_rtl && bytes.end != byte_cursor) || (!is_rtl && bytes.start != byte_cursor) {
            return Err("text run clusters do not cover source in visual order".to_owned());
        }
        // Count each source slice once. Repeated prefix conversion per cluster
        // made the previous constructor quadratic even for ordinary prose.
        let units = source.encode_utf16().count();
        let utf16 = if is_rtl {
            let start = utf16_cursor.checked_sub(units)
                .ok_or_else(|| "invalid text run UTF-16 coverage".to_owned())?;
            let range = start..utf16_cursor;
            utf16_cursor = start;
            byte_cursor = bytes.start;
            range
        } else {
            let end = utf16_cursor.checked_add(units)
                .ok_or_else(|| "text run UTF-16 size overflow".to_owned())?;
            let range = utf16_cursor..end;
            utf16_cursor = end;
            byte_cursor = bytes.end;
            range
        };
        let first_glyph = index;
        let left = pen;
        while index < shaped.glyphs.len() && shaped.glyphs[index].cluster == bytes {
            let glyph = &shaped.glyphs[index];
            let x_advance = glyph.x_advance as f32 * scale;
            let y_advance = glyph.y_advance as f32 * scale;
            let x_offset = glyph.x_offset as f32 * scale;
            let y_offset = glyph.y_offset as f32 * scale;
            let next = pen + x_advance;
            if glyph.glyph_id == 0 || glyph.x_advance < 0
                || !x_advance.is_finite() || !y_advance.is_finite()
                || !x_offset.is_finite() || !y_offset.is_finite()
                || !next.is_finite() || !(pen + x_offset).is_finite()
            {
                return Err("invalid text run glyph or scaled positioning".to_owned());
            }
            glyphs.push(RunGlyph {
                glyph_id: glyph.glyph_id,
                font_id: context.font_id,
                cluster_index: clusters.len(),
                x_advance,
                y_advance,
                x_offset,
                y_offset,
            });
            pen = next;
            index += 1;
        }
        clusters.push(TextCluster {
            cluster_index: clusters.len(),
            byte_range: bytes,
            utf16_range: utf16,
            glyph_range: first_glyph..index,
            x_start: left,
            x_end: pen,
            font_id: context.font_id,
        });
    }
    if byte_cursor != if is_rtl { 0 } else { logical_text.len() } {
        return Err("text run glyphs omit source text".to_owned());
    }

    if is_rtl {
        // ShapedRun already supplies visual glyph order. Reverse only the
        // cluster inventory to honor OwnedTextRun's logical-order contract.
        // Positions, glyph ranges, glyph order and mark offsets stay untouched.
        clusters.reverse();
        for (logical_index, cluster) in clusters.iter_mut().enumerate() {
            cluster.cluster_index = logical_index;
            for glyph in &mut glyphs[cluster.glyph_range.clone()] {
                glyph.cluster_index = logical_index;
            }
        }
    }
    Ok(OwnedTextRun { context, logical_text, clusters, glyphs, total_advance: pen })
}
