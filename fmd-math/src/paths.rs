//! Output paths: resolve a [`Layout`] into pure quadratic contours, and
//! dump them canonically for goldens.
//!
//! Glyph outlines come from fmd-font's glyf decoder — already quadratic —
//! and are scaled from font units to ems and translated to their placed
//! positions; rules become four-line rectangle contours; drawn paths pass
//! through. The result is the shared path model every consumer speaks
//! (franken_manim's geometry kernel builds VMobjects from it; fmd renders
//! it to HTML/PDF vectors).
//!
//! **Determinism.** The transform is pure f64 multiply/add over the
//! decoded integer coordinates, and the canonical dump prints fixed
//! six-decimal fixed-point — same string + faces ⇒ identical bytes, on
//! every platform.

use crate::error::MathError;
use crate::layout::Engine;
use crate::mbox::{Layout, PathContour, PathSeg};
use crate::node::Span;

/// Resolve every primitive of a layout into closed quadratic contours, in
/// ems, y-up, baseline at 0.
///
/// # Errors
///
/// [`MathError::UnmappedChar`] if a placed glyph's outline cannot be
/// decoded (a face-table corruption surfaces as the glyph's character).
pub fn resolve_paths(engine: &Engine, layout: &Layout) -> Result<Vec<PathContour>, MathError> {
    let mut out = Vec::new();
    for glyph in &layout.glyphs {
        let Some(font) = engine.faces().font(glyph.face) else {
            return Err(MathError::UnmappedChar {
                ch: glyph.ch,
                span: glyph.span,
            });
        };
        let outline = font
            .glyph_outline(glyph.gid)
            .map_err(|_| MathError::UnmappedChar {
                ch: glyph.ch,
                span: glyph.span,
            })?;
        let upm = f64::from(font.units_per_em.max(1));
        let s = glyph.size / upm;
        for contour in &outline.contours {
            let mut segments = Vec::new();
            let start = (glyph.x + contour.start.x * s, glyph.y + contour.start.y * s);
            for seg in &contour.segments {
                segments.push(match seg {
                    fmd_font::outline::Segment::Line { to } => PathSeg::Line {
                        to: (glyph.x + to.x * s, glyph.y + to.y * s),
                    },
                    fmd_font::outline::Segment::Quad { ctrl, to } => PathSeg::Quad {
                        ctrl: (glyph.x + ctrl.x * s, glyph.y + ctrl.y * s),
                        to: (glyph.x + to.x * s, glyph.y + to.y * s),
                    },
                });
            }
            out.push(PathContour { start, segments });
        }
    }
    for rule in &layout.rules {
        let x0 = rule.x;
        let y0 = rule.y;
        let x1 = rule.x + rule.width;
        let y1 = rule.y + rule.height;
        out.push(PathContour {
            start: (x0, y0),
            segments: vec![
                PathSeg::Line { to: (x1, y0) },
                PathSeg::Line { to: (x1, y1) },
                PathSeg::Line { to: (x0, y1) },
                PathSeg::Line { to: (x0, y0) },
            ],
        });
    }
    for path in &layout.paths {
        out.extend(path.contours.iter().cloned());
    }
    Ok(out)
}

/// The canonical text dump of resolved contours: one line per element,
/// fixed six-decimal coordinates — the golden format, byte-stable across
/// platforms and runs.
#[must_use]
pub fn canonical_dump(contours: &[PathContour]) -> String {
    use core::fmt::Write as _;
    let mut out = String::new();
    for (i, contour) in contours.iter().enumerate() {
        let _ = writeln!(
            out,
            "contour {} start {:.6} {:.6}",
            i, contour.start.0, contour.start.1
        );
        for seg in &contour.segments {
            match seg {
                PathSeg::Line { to } => {
                    let _ = writeln!(out, "  line {:.6} {:.6}", to.0, to.1);
                }
                PathSeg::Quad { ctrl, to } => {
                    let _ = writeln!(
                        out,
                        "  quad {:.6} {:.6} {:.6} {:.6}",
                        ctrl.0, ctrl.1, to.0, to.1
                    );
                }
            }
        }
    }
    out
}

/// A compact structural dump of a layout (glyph/rule inventory + metrics),
/// for goldens that want placement without full outlines.
#[must_use]
pub fn layout_dump(layout: &Layout) -> String {
    use core::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "layout w {:.6} h {:.6} d {:.6}",
        layout.width, layout.height, layout.depth
    );
    for g in &layout.glyphs {
        let _ = writeln!(
            out,
            "glyph face {} gid {} ch U+{:04X} x {:.6} y {:.6} size {:.6} span {}..{}",
            g.face.0, g.gid, g.ch as u32, g.x, g.y, g.size, g.span.start, g.span.end
        );
    }
    for r in &layout.rules {
        let _ = writeln!(
            out,
            "rule x {:.6} y {:.6} w {:.6} h {:.6} span {}..{}",
            r.x, r.y, r.width, r.height, r.span.start, r.span.end
        );
    }
    for p in &layout.paths {
        let _ = writeln!(
            out,
            "path contours {} span {}..{}",
            p.contours.len(),
            p.span.start,
            p.span.end
        );
    }
    out
}

/// Every primitive's span must sit inside the source string — the §11.3
/// provenance invariant, checkable by consumers.
#[must_use]
pub fn spans_cover(layout: &Layout, source_len: usize) -> bool {
    let ok = |s: &Span| s.end <= source_len && s.start <= s.end;
    layout.glyphs.iter().all(|g| ok(&g.span))
        && layout.rules.iter().all(|r| ok(&r.span))
        && layout.paths.iter().all(|p| ok(&p.span))
}
