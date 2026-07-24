//! Parametric drawn-path constructions — the ADR-0005 mainline for
//! delimiters past the uniform-scale ceiling, the drawn radical sign, and
//! the stretchy horizontal constructions (`\widehat`, `\widetilde`,
//! `\overbrace`/`\underbrace`, `\overrightarrow`/`\overleftarrow`).
//!
//! The bundled faces carry no cmex-style extension repertoire (CM Unicode
//! maps none of U+239B…U+23AE and has no size-variant sets — the G0-3
//! probe), so oversized constructions are **drawn**: closed quadratic
//! contours whose stroke weights are calibrated against the authored
//! glyphs, so the mechanism seam at the `1.25×` threshold is invisible at
//! a glance. The paren and surd constructions are ports of the ones the
//! G0-3 spike proved at three sizes; the rest of the family (brackets,
//! braces, bars, angles, floor/ceil, slashes) generalizes the same
//! discipline. No requested size can fail, by construction — §11.4's
//! promise made structural.
//!
//! Everything here is pure f64 add/sub/mul/div geometry — no
//! transcendental calls — so drawn output is bit-identical across
//! platforms (the determinism contract layout.rs declares).
//!
//! Coordinates: ems, y-up. Delimiter contours span `y ∈ [0, total]`;
//! horizontal stretches span `x ∈ [0, width]` with `y ∈ [0, height]`
//! (placement — axis centering, above/below the base — is the caller's).

use crate::mbox::{PathContour, PathSeg};

/// A finished construction: contours plus the advance width the caller
/// should give its box.
pub(crate) struct Drawn {
    /// Advance width, ems.
    pub(crate) width: f64,
    /// Closed contours, box-local.
    pub(crate) contours: Vec<PathContour>,
}

/// A horizontal stretchy construction: contours plus its height band.
pub(crate) struct DrawnBand {
    /// Total height of the band, ems.
    pub(crate) height: f64,
    /// Closed contours, box-local (`x ∈ [0, width]`, `y ∈ [0, height]`).
    pub(crate) contours: Vec<PathContour>,
}

/// Which horizontal stretchy construction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Stretch {
    /// `\widehat` — a tented chevron.
    Hat,
    /// `\widetilde` — an S-wave band.
    Tilde,
    /// `\overbrace` — a downward-hooked horizontal brace (lobe up).
    OverBrace,
    /// `\underbrace` — the mirror (lobe down).
    UnderBrace,
    /// `\overrightarrow` — shaft + right arrowhead.
    RightArrow,
    /// `\overleftarrow` — shaft + left arrowhead.
    LeftArrow,
}

/// Line-segment helper.
fn line(to: (f64, f64)) -> PathSeg {
    PathSeg::Line { to }
}

/// Quadratic-segment helper.
fn quad(ctrl: (f64, f64), to: (f64, f64)) -> PathSeg {
    PathSeg::Quad { ctrl, to }
}

/// One closed contour from a start point and segments.
fn contour(start: (f64, f64), segments: Vec<PathSeg>) -> PathContour {
    PathContour { start, segments }
}

/// A closed axis-aligned rectangle contour.
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PathContour {
    contour(
        (x0, y0),
        vec![
            line((x1, y0)),
            line((x1, y1)),
            line((x0, y1)),
            line((x0, y0)),
        ],
    )
}

/// Mirror a contour horizontally about `x = w/2`.
fn mirror_x(c: &PathContour, w: f64) -> PathContour {
    let mx = |p: (f64, f64)| (w - p.0, p.1);
    PathContour {
        start: mx(c.start),
        segments: c
            .segments
            .iter()
            .map(|s| match s {
                PathSeg::Line { to } => PathSeg::Line { to: mx(*to) },
                PathSeg::Quad { ctrl, to } => PathSeg::Quad {
                    ctrl: mx(*ctrl),
                    to: mx(*to),
                },
            })
            .collect(),
    }
}

/// Mirror a contour vertically about `y = h/2`.
fn mirror_y(c: &PathContour, h: f64) -> PathContour {
    let my = |p: (f64, f64)| (p.0, h - p.1);
    PathContour {
        start: my(c.start),
        segments: c
            .segments
            .iter()
            .map(|s| match s {
                PathSeg::Line { to } => PathSeg::Line { to: my(*to) },
                PathSeg::Quad { ctrl, to } => PathSeg::Quad {
                    ctrl: my(*ctrl),
                    to: my(*to),
                },
            })
            .collect(),
    }
}

/// The drawn construction for a delimiter character covering `total` ems
/// vertically, at style size `em` (stroke weights scale with `em`; the
/// gentle size-growth terms scale with `total`, matching how authored
/// optical sizes thicken). `None` for characters without a construction —
/// the caller keeps uniform scaling for those (nothing regresses).
pub(crate) fn delimiter(ch: char, total: f64, em: f64) -> Option<Drawn> {
    match ch {
        '(' | ')' => Some(paren(ch == ')', total, em)),
        '[' | ']' => Some(bracket(ch == ']', total, em, true, true)),
        '⌈' | '⌉' => Some(bracket(ch == '⌉', total, em, true, false)),
        '⌊' | '⌋' => Some(bracket(ch == '⌋', total, em, false, true)),
        '{' | '}' => Some(brace(ch == '}', total, em)),
        '|' => Some(vert(total, em, false)),
        '‖' => Some(vert(total, em, true)),
        '⟨' | '⟩' | '〈' | '〉' => Some(angle(matches!(ch, '⟩' | '〉'), total, em)),
        '/' | '\\' => Some(slash(ch == '\\', total, em)),
        '√' => Some(surd(total, em)),
        _ => None,
    }
}

/// The spike-proven paren: one closed quadratic contour, stroke calibrated
/// to CM's authored paren (tip `0.035 em`, waist `0.062 em` growing gently
/// with size).
fn paren(closing: bool, h: f64, em: f64) -> Drawn {
    let w = 0.30 * em + 0.06 * h;
    let t_top = 0.035 * em;
    let t_mid = 0.062 * em + 0.01 * h;
    let bulge = 0.16 * w;
    let (x_out, x_in) = (0.0, t_mid);
    let start = (w, h);
    let c = contour(
        start,
        vec![
            quad((x_out - bulge, h / 2.0), (w, 0.0)),
            line((w - t_top, 0.0)),
            quad((x_in - bulge, h / 2.0), (w - t_top, h)),
            line(start),
        ],
    );
    let width = w + 0.04 * em;
    let c = if closing { mirror_x(&c, width) } else { c };
    Drawn {
        width,
        contours: vec![c],
    }
}

/// A square bracket (or floor/ceiling, dropping one foot): stem plus feet
/// as one rectilinear contour. Stem `0.065 em`, feet `0.24 em` with gentle
/// growth.
fn bracket(closing: bool, h: f64, em: f64, top_foot: bool, bottom_foot: bool) -> Drawn {
    let t = 0.065 * em + 0.004 * h;
    let foot = 0.24 * em + 0.01 * h;
    let w = t + foot;
    // Opening form, stem at the left: walk the outline clockwise from the
    // outer top-right corner. Feet are present or absent per the variant
    // (floor/ceiling are brackets missing one foot).
    let mut pts: Vec<(f64, f64)> = Vec::new();
    if top_foot {
        pts.push((w, h));
        pts.push((0.0, h));
    } else {
        pts.push((t, h));
        pts.push((0.0, h));
    }
    pts.push((0.0, 0.0));
    if bottom_foot {
        pts.push((w, 0.0));
        pts.push((w, t));
        pts.push((t, t));
    } else {
        pts.push((t, 0.0));
    }
    if top_foot {
        pts.push((t, h - t));
        pts.push((w, h - t));
    } else {
        pts.push((t, h));
    }
    let start = pts[0];
    let segments: Vec<PathSeg> = pts[1..]
        .iter()
        .copied()
        .map(line)
        .chain(core::iter::once(line(start)))
        .collect();
    let c = contour(start, segments);
    let width = w + 0.06 * em;
    let c = if closing { mirror_x(&c, width) } else { c };
    Drawn {
        width,
        contours: vec![c],
    }
}

/// A curly brace: hooked caps, straight stems, and the waist point, as one
/// closed contour of quads — the classic two-S construction. Stroke
/// `0.055 em` growing gently; hook and waist reach `0.16 em`-ish.
fn brace(closing: bool, h: f64, em: f64) -> Drawn {
    let t = 0.055 * em + 0.006 * h; // stroke
    let reach = 0.16 * em + 0.015 * h; // how far hooks/waist bend
    let w = t + 2.0 * reach; // full advance of the ink
    let mid = h / 2.0;
    let cap = (0.28 * em).min(h * 0.25); // vertical extent of each hook
    let waist = (0.30 * em).min(h * 0.25); // vertical extent of the waist bend
    // Opening brace `{`: stems at x = reach .. reach+t, hooks bend right
    // (toward the content), waist points left. Outer edge runs down the
    // left side; inner edge returns up the right.
    let xs = reach; // outer stem x
    let start = (w, h);
    let c = contour(
        start,
        vec![
            // top hook, outer: from the cap tip curving left-down into the stem
            quad((xs, h), (xs, h - cap)),
            // upper stem, outer
            line((xs, mid + waist)),
            // waist, outer: bend left to the point
            quad((xs, mid), (0.0, mid)),
            // waist back, outer
            quad((xs, mid), (xs, mid - waist)),
            // lower stem, outer
            line((xs, cap)),
            // bottom hook, outer
            quad((xs, 0.0), (w, 0.0)),
            // across the bottom cap tip
            line((w, t)),
            // bottom hook, inner
            quad((xs + t, t), (xs + t, cap)),
            // lower stem, inner
            line((xs + t, mid - waist)),
            // waist, inner (shallower bend than the outer edge)
            quad((xs + t, mid), (t, mid)),
            quad((xs + t, mid), (xs + t, mid + waist)),
            // upper stem, inner
            line((xs + t, h - cap)),
            // top hook, inner
            quad((xs + t, h - t), (w, h - t)),
            // close across the top cap tip
            line(start),
        ],
    );
    let width = w + 0.05 * em;
    let c = if closing { mirror_x(&c, width) } else { c };
    Drawn {
        width,
        contours: vec![c],
    }
}

/// A vertical bar (or double bar): thin rectangles, CM `\vert` weight.
fn vert(h: f64, em: f64, double: bool) -> Drawn {
    let t = 0.045 * em;
    if double {
        let gap = 0.14 * em;
        Drawn {
            width: 2.0 * t + gap + 0.06 * em,
            contours: vec![rect(0.0, 0.0, t, h), rect(t + gap, 0.0, 2.0 * t + gap, h)],
        }
    } else {
        Drawn {
            width: t + 0.06 * em,
            contours: vec![rect(0.0, 0.0, t, h)],
        }
    }
}

/// An angle bracket: two strokes meeting at the apex, drawn as one
/// hexagonal contour whose inner edge is the outer edge shifted right by a
/// fixed horizontal stroke width. (A constant *horizontal* thickness thins
/// the perceived stroke slightly as the bracket steepens — imperceptible
/// over the family's real aspect range, and it keeps the arithmetic to
/// add/sub/mul/div per the crate's determinism doctrine: no `sqrt`-based
/// normal offset.)
fn angle(closing: bool, h: f64, em: f64) -> Drawn {
    let w = 0.24 * em + 0.10 * h; // opening half-width grows with size
    let mid = h / 2.0;
    let shift = 0.085 * em + 0.006 * h; // horizontal stroke width
    let start = (w + shift, h);
    let c = contour(
        start,
        vec![
            line((shift, mid)),
            line((w + shift, 0.0)),
            line((w, 0.0)),
            line((0.0, mid)),
            line((w, h)),
            line(start),
        ],
    );
    let width = w + shift + 0.05 * em;
    let c = if closing { mirror_x(&c, width) } else { c };
    Drawn {
        width,
        contours: vec![c],
    }
}

/// A big solidus/reverse solidus: a thin parallelogram.
fn slash(reverse: bool, h: f64, em: f64) -> Drawn {
    let t = 0.06 * em + 0.004 * h; // horizontal thickness
    let run = 0.28 * em + 0.18 * h; // horizontal run over the height
    let w = run + t;
    // `/`: bottom-left to top-right.
    let c = contour(
        (0.0, 0.0),
        vec![
            line((t, 0.0)),
            line((w, h)),
            line((w - t, h)),
            line((0.0, 0.0)),
        ],
    );
    let width = w + 0.04 * em;
    let c = if reverse { mirror_x(&c, width) } else { c };
    Drawn {
        width,
        contours: vec![c],
    }
}

/// The spike-proven drawn surd: leading tick, heavy down-stroke to the
/// vertex, light up-stroke to the overbar corner, as one closed contour.
/// The top edge sits at `y = total` (the caller aligns it with the
/// overbar); stroke weights match CM's authored sign.
fn surd(total: f64, em: f64) -> Drawn {
    let w = 0.58 * em + 0.02 * total;
    let heavy = 0.058 * em + 0.004 * total;
    let light = 0.056 * em; // ≈1.4 × rule thickness at em = 1
    let vertex_x = 0.30 * em;
    let mid_y = total * 0.42;
    let start = (0.02 * em, mid_y);
    let c = contour(
        start,
        vec![
            line((0.13 * em, mid_y + 0.05 * em)),
            line((vertex_x - heavy * 0.6, 0.18 * em)),
            line((w - light, total)),
            line((w, total)),
            line((vertex_x, 0.0)),
            line((vertex_x - heavy, 0.0)),
            line((0.10 * em, mid_y - 0.02 * em)),
            line(start),
        ],
    );
    Drawn {
        width: w,
        contours: vec![c],
    }
}

/// A horizontal stretchy construction spanning `width` ems at style size
/// `em`. Every kind is total: any width draws.
pub(crate) fn stretch(kind: Stretch, width: f64, em: f64) -> DrawnBand {
    match kind {
        Stretch::Hat => hat(width, em),
        Stretch::Tilde => tilde(width, em),
        Stretch::OverBrace => hbrace(width, em, false),
        Stretch::UnderBrace => hbrace(width, em, true),
        Stretch::RightArrow => arrow(width, em, false),
        Stretch::LeftArrow => arrow(width, em, true),
    }
}

/// `\widehat`: a tented chevron with square ends, apex thickness equal to
/// the stroke.
fn hat(w: f64, em: f64) -> DrawnBand {
    let t = 0.048 * em + 0.010 * w.min(3.0 * em);
    let rise = 0.16 * em + 0.05 * w.min(3.0 * em);
    let h = rise + t;
    let c = contour(
        (0.0, 0.0),
        vec![
            line((0.0, t)),
            line((w / 2.0, h)),
            line((w, t)),
            line((w, 0.0)),
            line((w / 2.0, rise)),
            line((0.0, 0.0)),
        ],
    );
    DrawnBand {
        height: h,
        contours: vec![c],
    }
}

/// `\widetilde`: an S-wave band of two quadratic humps.
fn tilde(w: f64, em: f64) -> DrawnBand {
    let t = 0.055 * em;
    let amp = 0.10 * em + 0.02 * w.min(3.0 * em);
    let h = 2.0 * amp + t;
    // Lower edge: rises from the left end, dips mid-right, ends high; the
    // upper edge is the same wave lifted by t.
    let c = contour(
        (0.0, amp * 0.4),
        vec![
            quad((w * 0.25, amp * 2.0), (w * 0.5, amp)),
            quad((w * 0.75, 0.0), (w, amp * 1.6)),
            line((w, amp * 1.6 + t)),
            quad((w * 0.75, t), (w * 0.5, amp + t)),
            quad((w * 0.25, amp * 2.0 + t), (0.0, amp * 0.4 + t)),
            line((0.0, amp * 0.4)),
        ],
    );
    DrawnBand {
        height: h,
        contours: vec![c],
    }
}

/// `\overbrace`/`\underbrace`: a horizontal brace — end hooks, straight
/// runs, center point — as one closed contour. For `\overbrace` the lobe
/// points up (hooks reach down at the ends); `\underbrace` mirrors.
fn hbrace(w: f64, em: f64, under: bool) -> DrawnBand {
    let t = 0.050 * em; // stroke
    let reach = (0.16 * em).min(w * 0.12); // vertical bend of hooks/point
    let h = t + 2.0 * reach;
    let cap = (0.30 * em).min(w * 0.20); // horizontal extent of each hook
    let waist = (0.32 * em).min(w * 0.20); // horizontal extent of the point
    let mid = w / 2.0;
    let ys = reach; // the runs' lower edge
    // Overbrace form: hooks at the ends bend down, the center point bends
    // up. Outer (upper) edge left→right, inner (lower) edge back.
    let start = (0.0, 0.0);
    let c = contour(
        start,
        vec![
            // left hook, outer: up from the tip into the run
            quad((0.0, ys + t), (cap, ys + t)),
            // left run, outer
            line((mid - waist, ys + t)),
            // center point, outer
            quad((mid, ys + t), (mid, h)),
            quad((mid, ys + t), (mid + waist, ys + t)),
            // right run, outer
            line((w - cap, ys + t)),
            // right hook, outer: down to the tip
            quad((w, ys + t), (w, 0.0)),
            // across the right tip
            line((w - t, 0.0)),
            // right hook, inner
            quad((w - t, ys), (w - cap, ys)),
            // right run, inner
            line((mid + waist, ys)),
            // center point, inner (shallower)
            quad((mid + t, ys), (mid, h - t * 1.2)),
            quad((mid - t, ys), (mid - waist, ys)),
            // left run, inner
            line((cap, ys)),
            // left hook, inner
            quad((t, ys), (t, 0.0)),
            // close across the left tip
            line(start),
        ],
    );
    let c = if under { mirror_y(&c, h) } else { c };
    DrawnBand {
        height: h,
        contours: vec![c],
    }
}

/// `\overrightarrow`/`\overleftarrow`: shaft plus arrowhead as one contour.
fn arrow(w: f64, em: f64, left: bool) -> DrawnBand {
    let t = 0.048 * em; // shaft thickness
    let head_l = (0.32 * em).min(w * 0.5); // head length
    let head_h = 0.16 * em; // head half-height
    let h = 2.0 * head_h;
    let cy = head_h; // shaft centerline
    let c = contour(
        (0.0, cy - t / 2.0),
        vec![
            line((w - head_l, cy - t / 2.0)),
            line((w - head_l, cy - head_h)),
            line((w, cy)),
            line((w - head_l, cy + head_h)),
            line((w - head_l, cy + t / 2.0)),
            line((0.0, cy + t / 2.0)),
            line((0.0, cy - t / 2.0)),
        ],
    );
    let c = if left { mirror_x(&c, w) } else { c };
    DrawnBand {
        height: h,
        contours: vec![c],
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// Every contour must be closed: the last segment returns to the start.
    fn assert_closed(d: &[PathContour]) {
        for c in d {
            let last = match c.segments.last().expect("segments") {
                PathSeg::Line { to } | PathSeg::Quad { to, .. } => *to,
            };
            assert!(
                (last.0 - c.start.0).abs() < 1e-12 && (last.1 - c.start.1).abs() < 1e-12,
                "contour not closed: start {:?}, last {last:?}",
                c.start
            );
        }
    }

    /// On-curve points must stay inside the declared band (control points
    /// may overshoot; the curve stays inside their hull).
    fn assert_y_range(d: &[PathContour], lo: f64, hi: f64) {
        for c in d {
            let check = |y: f64| {
                assert!(
                    y >= lo - 1e-9 && y <= hi + 1e-9,
                    "on-curve y {y} outside [{lo}, {hi}]"
                );
            };
            check(c.start.1);
            for s in &c.segments {
                match s {
                    PathSeg::Line { to } | PathSeg::Quad { to, .. } => check(to.1),
                }
            }
        }
    }

    #[test]
    fn every_delimiter_construction_exists_and_is_closed() {
        for ch in [
            '(', ')', '[', ']', '{', '}', '|', '‖', '⟨', '⟩', '⌈', '⌉', '⌊', '⌋', '/', '\\', '√',
        ] {
            for total in [1.0, 2.5, 7.0, 40.0] {
                let d = delimiter(ch, total, 1.0)
                    .unwrap_or_else(|| panic!("no construction for {ch:?}"));
                assert!(d.width > 0.0, "{ch:?} zero width");
                assert!(!d.contours.is_empty());
                assert_closed(&d.contours);
                assert_y_range(&d.contours, 0.0, total);
            }
        }
    }

    #[test]
    fn unknown_delimiters_have_no_construction() {
        // The caller keeps uniform scaling for these; nothing regresses.
        assert!(delimiter('↑', 3.0, 1.0).is_none());
        assert!(delimiter('x', 3.0, 1.0).is_none());
    }

    #[test]
    fn mirrored_pairs_share_extents() {
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}'), ('⟨', '⟩')] {
            let o = delimiter(open, 5.0, 1.0).unwrap();
            let c = delimiter(close, 5.0, 1.0).unwrap();
            assert!((o.width - c.width).abs() < 1e-12, "{open:?}/{close:?}");
            assert_eq!(o.contours.len(), c.contours.len());
        }
    }

    #[test]
    fn every_stretch_construction_draws_at_any_width() {
        for kind in [
            Stretch::Hat,
            Stretch::Tilde,
            Stretch::OverBrace,
            Stretch::UnderBrace,
            Stretch::RightArrow,
            Stretch::LeftArrow,
        ] {
            for w in [0.2, 1.0, 4.0, 25.0] {
                let b = stretch(kind, w, 1.0);
                assert!(b.height > 0.0);
                assert_closed(&b.contours);
                assert_y_range(&b.contours, 0.0, b.height);
                // The band spans the requested width.
                let max_x = b
                    .contours
                    .iter()
                    .flat_map(|c| {
                        core::iter::once(c.start.0).chain(c.segments.iter().map(|s| match s {
                            PathSeg::Line { to } | PathSeg::Quad { to, .. } => to.0,
                        }))
                    })
                    .fold(0.0_f64, f64::max);
                assert!(
                    (max_x - w).abs() < 1e-9,
                    "{kind:?} at {w}: ink ends at {max_x}"
                );
            }
        }
    }

    #[test]
    fn constructions_are_deterministic() {
        let a = delimiter('{', 7.3, 1.0).unwrap();
        let b = delimiter('{', 7.3, 1.0).unwrap();
        assert_eq!(a.contours, b.contours);
        let x = stretch(Stretch::Tilde, 3.7, 1.0);
        let y = stretch(Stretch::Tilde, 3.7, 1.0);
        assert_eq!(x.contours, y.contours);
    }
}
