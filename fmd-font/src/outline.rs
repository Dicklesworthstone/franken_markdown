//! Glyph outline decoding: `glyf` point data → quadratic-Bézier contours.
//!
//! TrueType outlines are already quadratic, so decoding is zero-loss: every
//! on-curve point becomes an anchor, every off-curve point a control point,
//! and runs of consecutive off-curve points get their implied on-curve
//! midpoints synthesized exactly as the rasterizer contract specifies.
//! Composite glyphs are assembled with their full transform semantics —
//! F2Dot14 scale / 2×2 matrices, XY offsets (scaled only when
//! `SCALED_COMPONENT_OFFSET` asks for it), and anchor-point matching — and
//! metrics honor `USE_MY_METRICS` plus the `hmtx` phantom-point rules
//! (left side bearing from `hmtx`, right side bearing derived from the
//! advance and the `glyf` header bbox).
//!
//! Fonts are untrusted input: every read is bounds-checked, composite
//! recursion is depth-limited, and total decoded points are budgeted, so a
//! hostile font errors quickly instead of hanging or ballooning memory.

use crate::{Font, be_i16, be_u16};

/// Composite glyphs deeper than this are rejected. Real fonts nest two or
/// three levels at most; the cap only exists to bound hostile recursion
/// (including self-referential component cycles, which exhaust depth).
pub const MAX_COMPOSITE_DEPTH: usize = 8;

/// Hard ceiling on the points a single decoded glyph may accumulate across
/// all its components. A well-formed glyph never exceeds the format's
/// 65 536-point space; the budget stops quadratic blow-ups from aliased
/// composite records.
pub const MAX_OUTLINE_POINTS: usize = 65_536;

/// Ceiling on component records walked while decoding one glyph (across
/// the whole recursion), bounding work on hostile `MORE_COMPONENTS` chains.
pub const MAX_COMPONENTS: usize = 512;

/// A point in font design units. Simple glyphs decode to exact integer
/// coordinates; composite transforms (F2Dot14 fractions) and synthesized
/// midpoints introduce the fractional values `f64` carries exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// X in font design units (advance direction).
    pub x: f64,
    /// Y in font design units (baseline-relative, y-up).
    pub y: f64,
}

impl Point {
    fn midpoint(self, other: Point) -> Point {
        Point {
            x: (self.x + other.x) / 2.0,
            y: (self.y + other.y) / 2.0,
        }
    }
}

/// One segment of a closed contour, starting from the previous segment's
/// endpoint (or [`Contour::start`] for the first).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    /// A straight edge to `to`.
    Line {
        /// The segment endpoint.
        to: Point,
    },
    /// A quadratic Bézier through control point `ctrl` to `to`.
    Quad {
        /// The off-curve control point.
        ctrl: Point,
        /// The on-curve endpoint.
        to: Point,
    },
}

impl Segment {
    /// The segment's endpoint.
    #[must_use]
    pub fn to(&self) -> Point {
        match self {
            Self::Line { to } | Self::Quad { to, .. } => *to,
        }
    }
}

/// A closed contour: `segments` walk from `start` back around to `start`
/// (the final segment's endpoint always equals `start`). Winding direction
/// is preserved from the font (TrueType fills non-zero).
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    /// The first on-curve anchor (synthesized as a midpoint when the raw
    /// contour opens off-curve).
    pub start: Point,
    /// The closed segment loop.
    pub segments: Vec<Segment>,
}

/// A decoded glyph: quadratic contours plus phantom-point-correct metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphOutline {
    /// The closed contours, in font order. Empty for blank glyphs (space).
    pub contours: Vec<Contour>,
    /// Advance width in design units, from `hmtx` — or from the flagged
    /// component's `hmtx` entry when a composite sets `USE_MY_METRICS`.
    pub advance: u16,
    /// Left side bearing in design units, same source as `advance`.
    pub lsb: i16,
    /// Right side bearing in design units: `advance − lsb − (xMax − xMin)`
    /// over the `glyf` header bbox (the phantom-point identity). For a
    /// blank glyph this degenerates to `advance − lsb`.
    pub rsb: i32,
    /// The `glyf` header bounding box `[xMin, yMin, xMax, yMax]`, when the
    /// glyph has one (blank glyphs do not).
    pub bbox: Option<[i16; 4]>,
}

impl GlyphOutline {
    /// Exact extents `[x_min, y_min, x_max, y_max]` of the decoded points
    /// (anchors and control points — the same point set the `glyf` header
    /// bbox covers), or `None` when there are no contours.
    #[must_use]
    pub fn extents(&self) -> Option<[f64; 4]> {
        let mut ext: Option<[f64; 4]> = None;
        let mut fold = |p: Point| {
            ext = Some(match ext {
                None => [p.x, p.y, p.x, p.y],
                Some([x0, y0, x1, y1]) => [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
            });
        };
        for c in &self.contours {
            fold(c.start);
            for s in &c.segments {
                if let Segment::Quad { ctrl, .. } = s {
                    fold(*ctrl);
                }
                fold(s.to());
            }
        }
        ext
    }
}

/// Why a glyph failed to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineError {
    /// The font has no `glyf`/`loca` tables (CFF outlines are tiered out).
    NoGlyfOutlines,
    /// The glyph id is out of range or its `loca` entry is unreadable.
    BadGlyphId,
    /// The glyph data is structurally invalid (truncated arrays, offsets
    /// past the record, anchor point numbers out of range, …).
    Malformed,
    /// A resource budget tripped: composite depth, component count, or the
    /// decoded-point ceiling. Well-formed fonts never hit these.
    BudgetExceeded,
}

impl core::fmt::Display for OutlineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoGlyfOutlines => write!(f, "font has no TrueType (glyf) outlines"),
            Self::BadGlyphId => write!(f, "glyph id out of range"),
            Self::Malformed => write!(f, "glyph outline data is malformed"),
            Self::BudgetExceeded => write!(f, "glyph outline exceeds decode budgets"),
        }
    }
}

impl std::error::Error for OutlineError {}

/// A raw outline point (before quadratic conversion).
#[derive(Debug, Clone, Copy)]
struct RawPoint {
    x: f64,
    y: f64,
    on_curve: bool,
}

/// A flattened raw outline: `points` in composition order (anchor-point
/// numbering indexes into this), `contour_ends` the inclusive end index of
/// each contour.
#[derive(Debug, Default)]
struct RawGlyph {
    points: Vec<RawPoint>,
    contour_ends: Vec<usize>,
}

/// Shared decode budgets, threaded through composite recursion.
struct Budget {
    points_left: usize,
    components_left: usize,
}

// Component flag bits (OpenType `glyf` composite description).
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const X_AND_Y_SCALE: u16 = 0x0040;
const TWO_BY_TWO: u16 = 0x0080;
const USE_MY_METRICS: u16 = 0x0200;
const SCALED_COMPONENT_OFFSET: u16 = 0x0800;
const UNSCALED_COMPONENT_OFFSET: u16 = 0x1000;

fn f2dot14(v: i16) -> f64 {
    f64::from(v) / 16384.0
}

impl Font {
    /// Decode glyph `gid` to quadratic contours with phantom-point-correct
    /// metrics. Blank glyphs (space) succeed with empty `contours`.
    ///
    /// # Errors
    ///
    /// [`OutlineError::NoGlyfOutlines`] for CFF-only fonts,
    /// [`OutlineError::BadGlyphId`] for out-of-range ids, and
    /// [`OutlineError::Malformed`] / [`OutlineError::BudgetExceeded`] for
    /// structurally invalid or hostile glyph data.
    pub fn glyph_outline(&self, gid: u16) -> Result<GlyphOutline, OutlineError> {
        if !self.has_glyf_outlines() {
            return Err(OutlineError::NoGlyfOutlines);
        }
        if gid >= self.num_glyphs {
            return Err(OutlineError::BadGlyphId);
        }
        let mut budget = Budget {
            points_left: MAX_OUTLINE_POINTS,
            components_left: MAX_COMPONENTS,
        };
        let raw = decode_raw(self, gid, 0, &mut budget)?;
        let (advance, lsb) = resolve_metrics(self, gid, 0);
        let bbox = self.glyph_bbox(gid);
        let rsb = match bbox {
            Some([x_min, _, x_max, _]) => {
                i32::from(advance) - i32::from(lsb) - (i32::from(x_max) - i32::from(x_min))
            }
            None => i32::from(advance) - i32::from(lsb),
        };
        Ok(GlyphOutline {
            contours: to_contours(&raw),
            advance,
            lsb,
            rsb,
            bbox,
        })
    }
}

/// Metrics for `gid`, honoring `USE_MY_METRICS`: a composite flagging a
/// component takes that component's `hmtx` advance and lsb (recursively).
fn resolve_metrics(font: &Font, gid: u16, depth: usize) -> (u16, i16) {
    if depth < MAX_COMPOSITE_DEPTH
        && let Some(data) = font.glyph_data(gid)
        && let Some(metrics_gid) = use_my_metrics_component(data)
        && metrics_gid != gid
    {
        return resolve_metrics(font, metrics_gid, depth + 1);
    }
    (font.advance_width(gid), font.left_side_bearing(gid))
}

/// The component gid flagged `USE_MY_METRICS` in a composite glyph, if any.
fn use_my_metrics_component(data: &[u8]) -> Option<u16> {
    let num_contours = be_i16(data, 0)?;
    if num_contours >= 0 {
        return None;
    }
    let mut p = 10usize;
    for _ in 0..MAX_COMPONENTS {
        let flags = be_u16(data, p)?;
        let comp = be_u16(data, p.checked_add(2)?)?;
        if flags & USE_MY_METRICS != 0 {
            return Some(comp);
        }
        p = p.checked_add(component_record_len(flags))?;
        if flags & MORE_COMPONENTS == 0 {
            return None;
        }
    }
    None
}

/// Byte length of one component record with the given flags (flags word +
/// glyph index + args + transform).
fn component_record_len(flags: u16) -> usize {
    let args = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
        4
    } else {
        2
    };
    let xform = if flags & WE_HAVE_A_SCALE != 0 {
        2
    } else if flags & X_AND_Y_SCALE != 0 {
        4
    } else if flags & TWO_BY_TWO != 0 {
        8
    } else {
        0
    };
    4 + args + xform
}

/// Decode `gid` (simple or composite) to raw points, recursing through
/// composite components with transforms and anchor matching applied.
fn decode_raw(
    font: &Font,
    gid: u16,
    depth: usize,
    budget: &mut Budget,
) -> Result<RawGlyph, OutlineError> {
    if depth > MAX_COMPOSITE_DEPTH {
        return Err(OutlineError::BudgetExceeded);
    }
    let data = font.glyph_data(gid).ok_or(OutlineError::BadGlyphId)?;
    if data.is_empty() {
        return Ok(RawGlyph::default()); // blank glyph (space)
    }
    let num_contours = be_i16(data, 0).ok_or(OutlineError::Malformed)?;
    if num_contours >= 0 {
        decode_simple(data, num_contours as usize, budget)
    } else {
        decode_composite(font, data, depth, budget)
    }
}

/// Decode a simple glyph's flag/coordinate arrays into raw points.
fn decode_simple(
    data: &[u8],
    num_contours: usize,
    budget: &mut Budget,
) -> Result<RawGlyph, OutlineError> {
    // endPtsOfContours follows the 10-byte header.
    let mut contour_ends = Vec::with_capacity(num_contours);
    let mut prev_end: Option<usize> = None;
    for i in 0..num_contours {
        let off = 10usize
            .checked_add(i.checked_mul(2).ok_or(OutlineError::Malformed)?)
            .ok_or(OutlineError::Malformed)?;
        let end = be_u16(data, off).ok_or(OutlineError::Malformed)? as usize;
        // endPts must be non-decreasing; a decreasing run would desync the
        // point count below.
        if prev_end.is_some_and(|p| end < p) {
            return Err(OutlineError::Malformed);
        }
        prev_end = Some(end);
        contour_ends.push(end);
    }
    let num_points = match contour_ends.last() {
        Some(&last) => last + 1,
        None => {
            return Ok(RawGlyph::default()); // zero contours: blank
        }
    };
    if num_points > budget.points_left {
        return Err(OutlineError::BudgetExceeded);
    }
    budget.points_left -= num_points;

    let instr_off = 10 + num_contours * 2;
    let instr_len = be_u16(data, instr_off).ok_or(OutlineError::Malformed)? as usize;
    let mut p = instr_off
        .checked_add(2)
        .and_then(|v| v.checked_add(instr_len))
        .ok_or(OutlineError::Malformed)?;

    // Flags, run-length encoded via the REPEAT bit.
    const ON_CURVE: u8 = 0x01;
    const X_SHORT: u8 = 0x02;
    const Y_SHORT: u8 = 0x04;
    const REPEAT: u8 = 0x08;
    const X_SAME_OR_POS: u8 = 0x10;
    const Y_SAME_OR_POS: u8 = 0x20;
    let mut flags = Vec::with_capacity(num_points);
    while flags.len() < num_points {
        let f = *data.get(p).ok_or(OutlineError::Malformed)?;
        p += 1;
        flags.push(f);
        if f & REPEAT != 0 {
            let n = *data.get(p).ok_or(OutlineError::Malformed)? as usize;
            p += 1;
            if flags.len() + n > num_points {
                return Err(OutlineError::Malformed);
            }
            for _ in 0..n {
                flags.push(f);
            }
        }
    }

    // X deltas, then Y deltas, each accumulated to absolute coordinates.
    let mut points = Vec::with_capacity(num_points);
    let mut x = 0i32;
    for &f in &flags {
        let dx = if f & X_SHORT != 0 {
            let b = i32::from(*data.get(p).ok_or(OutlineError::Malformed)?);
            p += 1;
            if f & X_SAME_OR_POS != 0 { b } else { -b }
        } else if f & X_SAME_OR_POS != 0 {
            0
        } else {
            let v = i32::from(be_i16(data, p).ok_or(OutlineError::Malformed)?);
            p += 2;
            v
        };
        x += dx;
        points.push(RawPoint {
            x: f64::from(x),
            y: 0.0,
            on_curve: f & ON_CURVE != 0,
        });
    }
    let mut y = 0i32;
    for (i, &f) in flags.iter().enumerate() {
        let dy = if f & Y_SHORT != 0 {
            let b = i32::from(*data.get(p).ok_or(OutlineError::Malformed)?);
            p += 1;
            if f & Y_SAME_OR_POS != 0 { b } else { -b }
        } else if f & Y_SAME_OR_POS != 0 {
            0
        } else {
            let v = i32::from(be_i16(data, p).ok_or(OutlineError::Malformed)?);
            p += 2;
            v
        };
        y += dy;
        if let Some(pt) = points.get_mut(i) {
            pt.y = f64::from(y);
        }
    }

    Ok(RawGlyph {
        points,
        contour_ends,
    })
}

/// Decode a composite glyph by recursively decoding each component and
/// appending its transformed points (anchor-point numbering stays flat
/// across components, which is what anchor matching indexes into).
fn decode_composite(
    font: &Font,
    data: &[u8],
    depth: usize,
    budget: &mut Budget,
) -> Result<RawGlyph, OutlineError> {
    let mut out = RawGlyph::default();
    let mut p = 10usize;
    loop {
        if budget.components_left == 0 {
            return Err(OutlineError::BudgetExceeded);
        }
        budget.components_left -= 1;

        let flags = be_u16(data, p).ok_or(OutlineError::Malformed)?;
        let comp_gid = be_u16(data, p.checked_add(2).ok_or(OutlineError::Malformed)?)
            .ok_or(OutlineError::Malformed)?;
        let record_end = p
            .checked_add(component_record_len(flags))
            .ok_or(OutlineError::Malformed)?;
        if record_end > data.len() {
            return Err(OutlineError::Malformed);
        }

        // Arguments: either an (dx, dy) offset or (parent, child) anchor
        // point numbers, in words or bytes.
        let arg_base = p + 4;
        let (arg1, arg2) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            (
                i32::from(be_i16(data, arg_base).ok_or(OutlineError::Malformed)?),
                i32::from(be_i16(data, arg_base + 2).ok_or(OutlineError::Malformed)?),
            )
        } else {
            let a = *data.get(arg_base).ok_or(OutlineError::Malformed)?;
            let b = *data.get(arg_base + 1).ok_or(OutlineError::Malformed)?;
            if flags & ARGS_ARE_XY_VALUES != 0 {
                (i32::from(a as i8), i32::from(b as i8))
            } else {
                (i32::from(a), i32::from(b))
            }
        };

        // Transform: 2×2 matrix in F2Dot14, defaulting to identity.
        let xf_base = arg_base
            + if flags & ARG_1_AND_2_ARE_WORDS != 0 {
                4
            } else {
                2
            };
        let (a, b, c, d) = if flags & WE_HAVE_A_SCALE != 0 {
            let s = f2dot14(be_i16(data, xf_base).ok_or(OutlineError::Malformed)?);
            (s, 0.0, 0.0, s)
        } else if flags & X_AND_Y_SCALE != 0 {
            (
                f2dot14(be_i16(data, xf_base).ok_or(OutlineError::Malformed)?),
                0.0,
                0.0,
                f2dot14(be_i16(data, xf_base + 2).ok_or(OutlineError::Malformed)?),
            )
        } else if flags & TWO_BY_TWO != 0 {
            (
                f2dot14(be_i16(data, xf_base).ok_or(OutlineError::Malformed)?),
                f2dot14(be_i16(data, xf_base + 2).ok_or(OutlineError::Malformed)?),
                f2dot14(be_i16(data, xf_base + 4).ok_or(OutlineError::Malformed)?),
                f2dot14(be_i16(data, xf_base + 6).ok_or(OutlineError::Malformed)?),
            )
        } else {
            (1.0, 0.0, 0.0, 1.0)
        };
        let apply = |pt: RawPoint, dx: f64, dy: f64| RawPoint {
            x: a * pt.x + c * pt.y + dx,
            y: b * pt.x + d * pt.y + dy,
            on_curve: pt.on_curve,
        };

        let child = decode_raw(font, comp_gid, depth + 1, budget)?;

        let (dx, dy) = if flags & ARGS_ARE_XY_VALUES != 0 {
            // An (dx, dy) offset. Per the spec's dominant (Microsoft/
            // FreeType) interpretation the offset is in the parent's space,
            // unscaled, unless SCALED_COMPONENT_OFFSET explicitly asks for
            // the child transform to apply (UNSCALED_COMPONENT_OFFSET wins
            // when both are set).
            let (dx, dy) = (f64::from(arg1), f64::from(arg2));
            if flags & SCALED_COMPONENT_OFFSET != 0 && flags & UNSCALED_COMPONENT_OFFSET == 0 {
                (a * dx + c * dy, b * dx + d * dy)
            } else {
                (dx, dy)
            }
        } else {
            // Anchor matching: align child point `arg2` (after the matrix,
            // before translation) onto parent point `arg1` (numbered across
            // the points composed so far).
            let parent_ix = usize::try_from(arg1).map_err(|_| OutlineError::Malformed)?;
            let child_ix = usize::try_from(arg2).map_err(|_| OutlineError::Malformed)?;
            let parent_pt = *out.points.get(parent_ix).ok_or(OutlineError::Malformed)?;
            let child_pt = *child.points.get(child_ix).ok_or(OutlineError::Malformed)?;
            let placed = apply(child_pt, 0.0, 0.0);
            (parent_pt.x - placed.x, parent_pt.y - placed.y)
        };

        let base = out.points.len();
        out.points
            .extend(child.points.iter().map(|&pt| apply(pt, dx, dy)));
        out.contour_ends
            .extend(child.contour_ends.iter().map(|&e| base + e));

        if flags & MORE_COMPONENTS == 0 {
            break;
        }
        p = record_end;
    }
    Ok(out)
}

/// Convert raw contours (on/off-curve points) into closed quadratic
/// contours, synthesizing the implied on-curve midpoints between
/// consecutive off-curve points.
fn to_contours(raw: &RawGlyph) -> Vec<Contour> {
    let mut contours = Vec::with_capacity(raw.contour_ends.len());
    let mut start_ix = 0usize;
    for &end in &raw.contour_ends {
        let Some(pts) = raw.points.get(start_ix..=end) else {
            break; // composite budget paths guarantee validity; belt & braces
        };
        start_ix = end + 1;
        if let Some(c) = contour_to_quads(pts) {
            contours.push(c);
        }
    }
    contours
}

/// One raw contour → one closed quadratic contour. Returns `None` for
/// degenerate contours (fewer than two points), which draw nothing.
fn contour_to_quads(pts: &[RawPoint]) -> Option<Contour> {
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len();
    // Choose the starting anchor: the first on-curve point, or (all points
    // off-curve) the midpoint of the last and first control points.
    let first_on = pts.iter().position(|p| p.on_curve);
    let (start, mut pending_ctrl, seq_start) = match first_on {
        Some(i) => {
            let p = pts[i];
            (Point { x: p.x, y: p.y }, None, i + 1)
        }
        None => {
            let last = pts[n - 1];
            let first = pts[0];
            let mid = Point {
                x: (last.x + first.x) / 2.0,
                y: (last.y + first.y) / 2.0,
            };
            (mid, None, 0)
        }
    };

    let mut segments = Vec::with_capacity(n);
    let mut cursor = start;
    // Walk every raw point exactly once, starting after the anchor (or at
    // the first control point in the all-off-curve case), wrapping around.
    let count = if first_on.is_some() { n - 1 } else { n };
    let mut emit = |ctrl: Option<Point>, to: Point, segments: &mut Vec<Segment>| {
        match ctrl {
            Some(ctrl) => segments.push(Segment::Quad { ctrl, to }),
            None => {
                // Skip zero-length line segments (repeated on-curve points);
                // they add nothing and degrade downstream stroke joins.
                if to != cursor {
                    segments.push(Segment::Line { to });
                }
            }
        }
        cursor = to;
    };
    for k in 0..count {
        let p = pts[(seq_start + k) % n];
        let point = Point { x: p.x, y: p.y };
        if p.on_curve {
            emit(pending_ctrl.take(), point, &mut segments);
        } else if let Some(prev_ctrl) = pending_ctrl.replace(point) {
            // Two consecutive off-curve points: the implied on-curve
            // midpoint closes the previous quad.
            emit(Some(prev_ctrl), prev_ctrl.midpoint(point), &mut segments);
            pending_ctrl = Some(point);
        }
    }
    // Close the contour back to the start.
    match pending_ctrl.take() {
        Some(ctrl) => segments.push(Segment::Quad { ctrl, to: start }),
        None => {
            if cursor != start {
                segments.push(Segment::Line { to: start });
            }
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(Contour { start, segments })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod hostile_outline_tests {
    //! Synthetic-font tests for the decoder's untrusted-input posture:
    //! malformed structures error (never panic, never hang), and the
    //! recursion/point budgets trip on hostile composites.

    use super::*;

    fn push16(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    fn push_i16(v: &mut Vec<u8>, x: i16) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    fn push32(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_be_bytes());
    }

    fn sfnt(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        push32(&mut out, 0x0001_0000);
        push16(&mut out, u16::try_from(tables.len()).unwrap());
        out.extend_from_slice(&[0u8; 6]);
        let mut offset = 12 + tables.len() * 16;
        let mut body = Vec::new();
        for (tag, bytes) in tables {
            out.extend_from_slice(&tag[..]);
            push32(&mut out, 0);
            push32(&mut out, u32::try_from(offset).unwrap());
            push32(&mut out, u32::try_from(bytes.len()).unwrap());
            offset += bytes.len();
            body.extend_from_slice(bytes);
        }
        out.extend_from_slice(&body);
        out
    }

    /// A minimal parseable font whose `glyf` holds exactly `glyphs` (raw
    /// per-glyph bytes), with a long `loca` and one hmtx metric per glyph.
    fn font_with_glyphs(glyphs: &[Vec<u8>]) -> Font {
        let n = u16::try_from(glyphs.len()).unwrap();
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[50..52].copy_from_slice(&1u16.to_be_bytes()); // long loca
        let mut maxp = vec![0u8; 6];
        maxp[4..6].copy_from_slice(&n.to_be_bytes());
        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&700i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        hhea[34..36].copy_from_slice(&n.to_be_bytes());
        let mut hmtx = Vec::new();
        for _ in 0..n {
            push16(&mut hmtx, 600);
            push_i16(&mut hmtx, 50);
        }
        // Minimal format-4 cmap: just the mandatory 0xFFFF closing segment.
        let mut cmap = Vec::new();
        push16(&mut cmap, 0);
        push16(&mut cmap, 1);
        push16(&mut cmap, 3);
        push16(&mut cmap, 1);
        push32(&mut cmap, 12);
        push16(&mut cmap, 4); // format
        push16(&mut cmap, 32); // length
        push16(&mut cmap, 0); // language
        push16(&mut cmap, 2); // segCountX2
        push16(&mut cmap, 0);
        push16(&mut cmap, 0);
        push16(&mut cmap, 0);
        push16(&mut cmap, 0xFFFF); // endCode
        push16(&mut cmap, 0); // reservedPad
        push16(&mut cmap, 0xFFFF); // startCode
        push16(&mut cmap, 1); // idDelta
        push16(&mut cmap, 0); // idRangeOffset
        let mut loca = Vec::new();
        let mut glyf = Vec::new();
        push32(&mut loca, 0);
        for g in glyphs {
            glyf.extend_from_slice(g);
            push32(&mut loca, u32::try_from(glyf.len()).unwrap());
        }
        Font::parse(sfnt(&[
            (b"head", head),
            (b"maxp", maxp),
            (b"hhea", hhea),
            (b"hmtx", hmtx),
            (b"cmap", cmap),
            (b"loca", loca),
            (b"glyf", glyf),
        ]))
        .expect("synthetic font parses")
    }

    /// A well-formed one-contour triangle: (0,0) → (500,0) → (250,400).
    fn triangle_glyph() -> Vec<u8> {
        let mut g = Vec::new();
        push_i16(&mut g, 1); // numberOfContours
        push_i16(&mut g, 0); // xMin
        push_i16(&mut g, 0); // yMin
        push_i16(&mut g, 500); // xMax
        push_i16(&mut g, 400); // yMax
        push16(&mut g, 2); // endPtsOfContours[0]
        push16(&mut g, 0); // instructionLength
        g.extend_from_slice(&[0x01, 0x01, 0x01]); // flags: 3 on-curve points
        push_i16(&mut g, 0); // x deltas (16-bit)
        push_i16(&mut g, 500);
        push_i16(&mut g, -250);
        push_i16(&mut g, 0); // y deltas
        push_i16(&mut g, 0);
        push_i16(&mut g, 400);
        g
    }

    /// A composite glyph with one component record (XY offsets, word args).
    fn composite_glyph(component_gid: u16, dx: i16, dy: i16, more: bool) -> Vec<u8> {
        let mut g = Vec::new();
        push_i16(&mut g, -1);
        for _ in 0..4 {
            push_i16(&mut g, 0); // bbox: unread by the decoder
        }
        let mut flags = ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES;
        if more {
            flags |= MORE_COMPONENTS;
        }
        push16(&mut g, flags);
        push16(&mut g, component_gid);
        push_i16(&mut g, dx);
        push_i16(&mut g, dy);
        g
    }

    #[test]
    fn triangle_decodes_closed_with_exact_extents() {
        let font = font_with_glyphs(&[triangle_glyph()]);
        let o = font.glyph_outline(0).expect("triangle decodes");
        assert_eq!(o.contours.len(), 1);
        let c = &o.contours[0];
        assert_eq!(c.segments.last().unwrap().to(), c.start, "contour closes");
        assert_eq!(o.extents(), Some([0.0, 0.0, 500.0, 400.0]));
        assert_eq!(o.bbox, Some([0, 0, 500, 400]));
        assert_eq!((o.advance, o.lsb), (600, 50));
        assert_eq!(o.rsb, i32::from(600u16) - 50 - 500);
    }

    #[test]
    fn blank_glyph_decodes_empty() {
        let font = font_with_glyphs(&[Vec::new()]);
        let o = font.glyph_outline(0).expect("blank glyph decodes");
        assert!(o.contours.is_empty());
        assert_eq!(o.bbox, None);
        assert_eq!(o.rsb, 600 - 50);
    }

    #[test]
    fn out_of_range_gid_is_rejected() {
        let font = font_with_glyphs(&[triangle_glyph()]);
        assert_eq!(font.glyph_outline(7), Err(OutlineError::BadGlyphId));
    }

    #[test]
    fn composite_offsets_translate_the_component() {
        let font = font_with_glyphs(&[triangle_glyph(), composite_glyph(0, 100, -50, false)]);
        let o = font.glyph_outline(1).expect("composite decodes");
        assert_eq!(o.extents(), Some([100.0, -50.0, 600.0, 350.0]));
    }

    #[test]
    fn self_referential_composite_trips_the_depth_budget() {
        let font = font_with_glyphs(&[composite_glyph(0, 10, 10, false)]);
        assert_eq!(font.glyph_outline(0), Err(OutlineError::BudgetExceeded));
    }

    #[test]
    fn mutually_recursive_composites_trip_the_depth_budget() {
        let font = font_with_glyphs(&[
            composite_glyph(1, 0, 0, false),
            composite_glyph(0, 0, 0, false),
        ]);
        assert_eq!(font.glyph_outline(0), Err(OutlineError::BudgetExceeded));
        assert_eq!(font.glyph_outline(1), Err(OutlineError::BudgetExceeded));
    }

    #[test]
    fn anchor_args_out_of_range_are_malformed() {
        // Anchor matching (no ARGS_ARE_XY_VALUES) with point numbers far
        // beyond both glyphs' point counts.
        let mut g = Vec::new();
        push_i16(&mut g, -1);
        for _ in 0..4 {
            push_i16(&mut g, 0);
        }
        push16(&mut g, ARG_1_AND_2_ARE_WORDS); // words, anchors
        push16(&mut g, 0); // component: the triangle
        push_i16(&mut g, 999);
        push_i16(&mut g, 999);
        let font = font_with_glyphs(&[triangle_glyph(), g]);
        assert_eq!(font.glyph_outline(1), Err(OutlineError::Malformed));
    }

    #[test]
    fn truncated_simple_glyph_is_malformed() {
        let full = triangle_glyph();
        // Every proper prefix (past the header's contour count) must error
        // cleanly — flags, deltas, endPts all truncate somewhere in here.
        for cut in 2..full.len() {
            let font = font_with_glyphs(&[full[..cut].to_vec()]);
            let r = font.glyph_outline(0);
            assert!(r.is_err(), "prefix of {cut} bytes must not decode");
        }
    }

    #[test]
    fn decreasing_end_pts_are_malformed() {
        let mut g = Vec::new();
        push_i16(&mut g, 2); // two contours
        for _ in 0..4 {
            push_i16(&mut g, 0);
        }
        push16(&mut g, 5); // endPts[0]
        push16(&mut g, 2); // endPts[1] decreasing: desyncs the point count
        push16(&mut g, 0);
        let font = font_with_glyphs(&[g]);
        assert_eq!(font.glyph_outline(0), Err(OutlineError::Malformed));
    }

    #[test]
    fn flag_repeat_overflow_is_malformed() {
        let mut g = Vec::new();
        push_i16(&mut g, 1);
        for _ in 0..4 {
            push_i16(&mut g, 0);
        }
        push16(&mut g, 2); // 3 points
        push16(&mut g, 0); // no instructions
        g.extend_from_slice(&[0x09, 0xFF]); // on-curve + REPEAT × 255: overflows
        let font = font_with_glyphs(&[g]);
        assert_eq!(font.glyph_outline(0), Err(OutlineError::Malformed));
    }

    #[test]
    fn overlong_component_chain_trips_the_component_budget() {
        // One composite whose records all reference the triangle and chain
        // MORE_COMPONENTS far past the budget.
        let mut g = Vec::new();
        push_i16(&mut g, -1);
        for _ in 0..4 {
            push_i16(&mut g, 0);
        }
        for i in 0..(MAX_COMPONENTS + 8) {
            let last = i == MAX_COMPONENTS + 7;
            let mut flags = ARG_1_AND_2_ARE_WORDS | ARGS_ARE_XY_VALUES;
            if !last {
                flags |= MORE_COMPONENTS;
            }
            push16(&mut g, flags);
            push16(&mut g, 0);
            push_i16(&mut g, 0);
            push_i16(&mut g, 0);
        }
        let font = font_with_glyphs(&[triangle_glyph(), g]);
        assert_eq!(font.glyph_outline(1), Err(OutlineError::BudgetExceeded));
    }

    #[test]
    fn all_off_curve_contour_closes_with_quads() {
        // Four off-curve points forming a diamond-ish TrueType "dot": every
        // anchor is synthesized.
        let mut g = Vec::new();
        push_i16(&mut g, 1);
        for _ in 0..4 {
            push_i16(&mut g, 0);
        }
        push16(&mut g, 3); // 4 points
        push16(&mut g, 0);
        g.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // all off-curve, 16-bit deltas
        // Absolute points: (0,100), (100,200), (200,100), (100,0) — a diamond
        // of control points with every anchor synthesized as a midpoint.
        for dx in [0i16, 100, 100, -100] {
            push_i16(&mut g, dx);
        }
        for dy in [100i16, 100, -100, -100] {
            push_i16(&mut g, dy);
        }
        let font = font_with_glyphs(&[g]);
        let o = font
            .glyph_outline(0)
            .expect("all-off-curve contour decodes");
        assert_eq!(o.contours.len(), 1);
        let c = &o.contours[0];
        assert_eq!(c.segments.len(), 4);
        assert!(
            c.segments.iter().all(|s| matches!(s, Segment::Quad { .. })),
            "an all-off-curve contour is pure quads"
        );
        assert_eq!(c.segments.last().unwrap().to(), c.start);
    }
}
