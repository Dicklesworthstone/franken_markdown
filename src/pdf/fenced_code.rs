//! Measured containing-width policy for fenced code and text diagrams.
//!
//! Ordinary code wraps at its requested size whenever one source cluster and
//! the optional number gutter fit. Diagram rows stay intact by reducing the
//! whole block uniformly. The writer emits font sizes to hundredths of a
//! point, so fitting searches that same finite set instead of relying on a
//! nominal minimum or a size that rounds upward when painted.

use super::{
    CODE_LINE_NUMBER_GAP, CodeFontFitSpec, CodeFrag, F_MONO, Faces, code_fragments, font_size_of,
    split_code_slot_runs, text_composition, text_positioning,
};

pub(super) const MIN_EMITTED_SIZE: f32 = 0.01;
const EDGE_RESERVE: f32 = 0.012;

fn emitted_size(size: f32) -> f32 {
    (size * 100.0).round() / 100.0
}

/// Advance in the PDF's own 1/1000-em font widths and TJ pair adjustments.
/// Avoid summing per-glyph millipoint truncation: that underestimates a long
/// row, especially after reducing its font size. Positioned mark runs already
/// paint exactly the quantized advances returned by their shared adapter.
fn run_width(text: &str, slot: u8, size: f32, faces: &Faces) -> f32 {
    let face = faces.face(slot);
    let size = emitted_size(size);
    if let Some(run) = text_positioning::prepare(face, text).run {
        return run.width(font_size_of(size)).to_points_f32();
    }
    let units = if text.is_ascii()
        && !text
            .bytes()
            .any(|byte| face.ascii_tables().lig_start[usize::from(byte)])
    {
        let tables = face.ascii_tables();
        let mut units = 0i64;
        for (index, &byte) in text.as_bytes().iter().enumerate() {
            units += i64::from(tables.advances[usize::from(byte)]);
            if let Some(&next) = text.as_bytes().get(index + 1) {
                units += i64::from(tables.kern[usize::from(byte) * 128 + usize::from(next)]);
            }
        }
        units
    } else {
        let text = text_composition::glyph_text(text, &face.font);
        let raw: Vec<_> = text.chars().map(|ch| face.glyph_index(ch)).collect();
        let glyphs = face.lig.substitute(&raw);
        let upm = i64::from(face.font.units_per_em.max(1));
        let mut units = 0i64;
        for (index, &glyph) in glyphs.iter().enumerate() {
            units += i64::from(face.glyph_advance_1000(glyph));
            if let Some(&next) = glyphs.get(index + 1) {
                units += i64::from(face.kern.pair(glyph, next)) * 1000 / upm;
            }
        }
        units
    };
    (units.max(0) as f64 * f64::from(size) / 1000.0) as f32
}

pub(super) fn text_width(text: &str, size: f32, faces: &Faces) -> f32 {
    if text.is_ascii() {
        return run_width(text, F_MONO, size, faces);
    }
    split_code_slot_runs(text, faces)
        .iter()
        .map(|(slot, text)| run_width(text, *slot, size, faces))
        .sum()
}

pub(super) fn row_width(frags: &[CodeFrag], size: f32, faces: &Faces) -> f32 {
    frags
        .iter()
        .map(|frag| text_width(&frag.text, size, faces))
        .sum()
}

pub(super) fn widest_cluster(text: &str, size: f32, faces: &Faces) -> f32 {
    if text.is_ascii() {
        let mut seen = [false; 128];
        for byte in text.bytes().filter(|byte| !matches!(byte, b'\r' | b'\n')) {
            seen[usize::from(if byte == b'\t' { b' ' } else { byte })] = true;
        }
        return seen
            .iter()
            .enumerate()
            .filter(|(_, present)| **present)
            .map(|(byte, _)| {
                let bytes = [byte as u8];
                // Every retained byte is ASCII.
                run_width(
                    std::str::from_utf8(&bytes).unwrap_or(""),
                    F_MONO,
                    size,
                    faces,
                )
            })
            .fold(0.0, f32::max);
    }
    text.lines()
        .flat_map(text_composition::source_clusters)
        .map(|cluster| text_width(if cluster == "\t" { " " } else { cluster }, size, faces))
        .fold(0.0, f32::max)
}

fn gutter(size: f32, spec: &CodeFontFitSpec<'_>) -> f32 {
    if spec.line_numbers {
        text_width(&"9".repeat(spec.digits), size, spec.faces) + CODE_LINE_NUMBER_GAP
    } else {
        0.0
    }
}

fn fit_size(default: f32, measure: f32, mut required: impl FnMut(f32) -> f32) -> Option<f32> {
    let available = (measure - EDGE_RESERVE).max(0.0);
    if required(default) <= available {
        return Some(default);
    }
    if required(MIN_EMITTED_SIZE) > available {
        return None;
    }
    let mut low = 1u32;
    let mut high = (default * 100.0).floor().max(1.0) as u32;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if required(mid as f32 / 100.0) <= available {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Some(low as f32 / 100.0)
}

pub(super) fn diagram_size(code: &str, spec: &CodeFontFitSpec<'_>) -> Option<f32> {
    let mut scratch = Vec::new();
    fit_size(spec.default_size, spec.code_area_width, |size| {
        let longest = code
            .lines()
            .map(|line| {
                row_width(
                    &code_fragments(spec.lang, line, &mut scratch),
                    size,
                    spec.faces,
                )
            })
            .fold(0.0, f32::max);
        longest + gutter(size, spec)
    })
}

pub(super) fn wrapped_size(code: &str, spec: &CodeFontFitSpec<'_>) -> f32 {
    fit_size(spec.default_size, spec.code_area_width, |size| {
        widest_cluster(code, size, spec.faces) + gutter(size, spec)
    })
    .unwrap_or(MIN_EMITTED_SIZE)
}

/// Hanging continuation rows must still admit the widest indivisible source
/// cluster. A minimum text-width floor would undo this calculation and paint
/// into the margin, particularly beside line numbers in nested code blocks.
pub(super) fn continuation_indent(
    frags: &[CodeFrag],
    measure: f32,
    size: f32,
    faces: &Faces,
) -> f32 {
    let widest = frags
        .iter()
        .map(|frag| widest_cluster(&frag.text, size, faces))
        .fold(0.0, f32::max);
    super::CODE_HANGING_INDENT
        .min((measure * 0.35).max(0.0))
        .min((measure - widest - EDGE_RESERVE).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_uses_emitted_precision_and_has_no_readability_floor() {
        assert_eq!(fit_size(9.5, 40.0, |size| size * 200.0), Some(0.19));
        assert!(fit_size(9.5, 40.0, |size| size * 5000.0).is_none());
        assert_eq!(fit_size(9.5, 40.0, |size| size * 2.0), Some(9.5));
    }
}
