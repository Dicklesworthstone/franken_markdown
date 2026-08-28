//! Clean-room `gvar` reader and instancer.
//!
//! Applies tuple-variation stores (packed point numbers, packed deltas, IUP)
//! at a pinned `wght` location and emits a static TrueType font.

use crate::{
    Font, be_i16, be_u16, be_u32, find_table_full, off, off_mul, table_checksum, write_u32,
};

const MAX_TUPLES: usize = 256;
const MAX_GVAR_POINTS: usize = 4096;
const PHANTOMS: usize = 4;

const EMBEDDED_PEAK: u16 = 0x8000;
const INTERMEDIATE: u16 = 0x4000;
const PRIVATE_POINTS: u16 = 0x2000;
const TUPLE_INDEX_MASK: u16 = 0x0FFF;

const ON_CURVE: u8 = 0x01;
const X_SHORT: u8 = 0x02;
const Y_SHORT: u8 = 0x04;
const REPEAT: u8 = 0x08;
const X_SAME_OR_POS: u8 = 0x10;
const Y_SAME_OR_POS: u8 = 0x20;

const ARG_WORDS: u16 = 0x0001;
const ARGS_ARE_XY: u16 = 0x0002;
const WE_HAVE_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const X_Y_SCALE: u16 = 0x0040;
const TWO_BY_TWO: u16 = 0x0080;

struct GvarHeader {
    axis_count: usize,
    shared_tuples: Vec<Vec<f32>>,
    glyph_data_off: Vec<usize>,
    table_end: usize,
}

pub(crate) fn instance_font(font: &Font, weight: f32) -> Option<Font> {
    if !font.has_glyf_outlines() {
        return None;
    }
    let n_axes = font.axes().len();
    if n_axes == 0 {
        return None;
    }
    let location = location_for_weight(font, weight)?;
    let data = font.raw_bytes();
    let parsed =
        find_table_full(data, b"gvar").and_then(|(o, len)| parse_gvar(data, o, len, n_axes));

    let mut glyf_bytes = Vec::new();
    let mut loca: Vec<u32> = vec![0];
    for gid in 0..font.num_glyphs {
        let bytes = instance_glyph(font, parsed.as_ref(), gid, &location)
            .or_else(|| font.glyph_data(gid).map(|s| s.to_vec()))
            .unwrap_or_default();
        glyf_bytes.extend_from_slice(&bytes);
        loca.push(u32::try_from(glyf_bytes.len()).ok()?);
    }
    let loca_bytes = encode_loca(&loca);
    let assembled = rebuild_static(font, glyf_bytes, loca_bytes)?;
    Font::parse(assembled).ok()
}

fn location_for_weight(font: &Font, weight: f32) -> Option<Vec<f32>> {
    let mut loc = Vec::with_capacity(font.axes().len());
    let mut saw_wght = false;
    for axis in font.axes() {
        if axis.tag == *b"wght" {
            loc.push(font.normalized_axis(*b"wght", weight)?);
            saw_wght = true;
        } else {
            loc.push(0.0);
        }
    }
    if saw_wght { Some(loc) } else { None }
}

fn parse_gvar(d: &[u8], table_off: usize, table_len: usize, n_axes: usize) -> Option<GvarHeader> {
    let table_end = table_off.checked_add(table_len)?;
    if table_off.checked_add(20)? > table_end {
        return None;
    }
    if be_u16(d, table_off)? != 1 {
        return None;
    }
    let axis_count = be_u16_at(d, table_off, 4)? as usize;
    if axis_count != n_axes || axis_count == 0 {
        return None;
    }
    let shared_tuple_count = be_u16_at(d, table_off, 6)? as usize;
    let shared_tuples_offset = be_u32_at(d, table_off, 8)? as usize;
    let glyph_count = be_u16_at(d, table_off, 12)? as usize;
    let flags = be_u16_at(d, table_off, 14)?;
    let glyph_var_offset = be_u32_at(d, table_off, 16)? as usize;
    let long_off = flags & 1 != 0;

    let mut shared_tuples = Vec::new();
    if shared_tuple_count > 0 {
        let n = shared_tuple_count.min(MAX_TUPLES);
        let base = off(table_off, shared_tuples_offset)?;
        let stride = axis_count.checked_mul(2)?;
        for i in 0..n {
            let rec = off_mul(base, i, stride)?;
            let mut t = Vec::with_capacity(axis_count);
            for a in 0..axis_count {
                t.push(f2dot14(be_i16(d, off_mul(rec, a, 2)?)?));
            }
            shared_tuples.push(t);
        }
    }

    let n_off = glyph_count.checked_add(1)?;
    let off_base = off(table_off, glyph_var_offset)?;
    let mut glyph_data_off = Vec::with_capacity(n_off);
    for i in 0..n_off {
        let rel = if long_off {
            be_u32(d, off_mul(off_base, i, 4)?)? as usize
        } else {
            (be_u16(d, off_mul(off_base, i, 2)?)? as usize).checked_mul(2)?
        };
        glyph_data_off.push(off(off_base, rel).unwrap_or(table_end));
    }

    Some(GvarHeader {
        axis_count,
        shared_tuples,
        glyph_data_off,
        table_end,
    })
}

fn be_u16_at(d: &[u8], base: usize, delta: usize) -> Option<u16> {
    be_u16(d, off(base, delta)?)
}
fn be_u32_at(d: &[u8], base: usize, delta: usize) -> Option<u32> {
    be_u32(d, off(base, delta)?)
}
fn f2dot14(v: i16) -> f32 {
    f32::from(v) / 16384.0
}

fn instance_glyph(
    font: &Font,
    gvar: Option<&GvarHeader>,
    gid: u16,
    location: &[f32],
) -> Option<Vec<u8>> {
    let data = font.glyph_data(gid).unwrap_or(&[]);
    if data.is_empty() {
        return Some(Vec::new());
    }
    let n_contours = be_i16(data, 0)?;
    if n_contours >= 0 {
        instance_simple(
            font.raw_bytes(),
            data,
            n_contours as usize,
            gvar,
            gid,
            location,
        )
    } else {
        instance_composite(font.raw_bytes(), data, gvar, gid, location)
    }
}

struct SimpleGlyph {
    points: Vec<(i32, i32)>,
    on_curve: Vec<bool>,
    contour_ends: Vec<u16>,
}

fn decode_simple(data: &[u8], n_contours: usize) -> Option<SimpleGlyph> {
    if n_contours == 0 {
        return Some(SimpleGlyph {
            points: Vec::new(),
            on_curve: Vec::new(),
            contour_ends: Vec::new(),
        });
    }
    let mut contour_ends = Vec::with_capacity(n_contours);
    let mut prev = 0usize;
    for i in 0..n_contours {
        let end = be_u16(data, off_mul(10, i, 2)?)? as usize;
        if i > 0 && end < prev {
            return None;
        }
        prev = end;
        contour_ends.push(u16::try_from(end).ok()?);
    }
    let n_points = prev.checked_add(1)?;
    if n_points > MAX_GVAR_POINTS {
        return None;
    }
    let instr_off = 10 + n_contours * 2;
    let instr_len = be_u16(data, instr_off)? as usize;
    let mut p = instr_off.checked_add(2)?.checked_add(instr_len)?;
    let mut flags = Vec::with_capacity(n_points);
    while flags.len() < n_points {
        let f = *data.get(p)?;
        p = p.checked_add(1)?;
        flags.push(f);
        if f & REPEAT != 0 {
            let n = *data.get(p)? as usize;
            p = p.checked_add(1)?;
            if flags.len() + n > n_points {
                return None;
            }
            for _ in 0..n {
                flags.push(f);
            }
        }
    }
    let mut xs = Vec::with_capacity(n_points);
    let mut x = 0i32;
    for &f in &flags {
        let dx = coord_delta(data, &mut p, f, X_SHORT, X_SAME_OR_POS)?;
        x += dx;
        xs.push(x);
    }
    let mut ys = Vec::with_capacity(n_points);
    let mut y = 0i32;
    for &f in &flags {
        let dy = coord_delta(data, &mut p, f, Y_SHORT, Y_SAME_OR_POS)?;
        y += dy;
        ys.push(y);
    }
    Some(SimpleGlyph {
        points: xs.into_iter().zip(ys).collect(),
        on_curve: flags.iter().map(|f| f & ON_CURVE != 0).collect(),
        contour_ends,
    })
}

fn coord_delta(data: &[u8], p: &mut usize, f: u8, short: u8, same_or_pos: u8) -> Option<i32> {
    if f & short != 0 {
        let b = i32::from(*data.get(*p)?);
        *p = p.checked_add(1)?;
        Some(if f & same_or_pos != 0 { b } else { -b })
    } else if f & same_or_pos != 0 {
        Some(0)
    } else {
        let v = i32::from(be_i16(data, *p)?);
        *p = p.checked_add(2)?;
        Some(v)
    }
}

fn instance_simple(
    font_bytes: &[u8],
    data: &[u8],
    n_contours: usize,
    gvar: Option<&GvarHeader>,
    gid: u16,
    location: &[f32],
) -> Option<Vec<u8>> {
    let mut simple = decode_simple(data, n_contours)?;
    if let Some(gvar) = gvar {
        apply_store(
            font_bytes,
            gvar,
            gid,
            location,
            &mut simple.points,
            Some(simple.contour_ends.as_slice()),
        );
    }
    Some(encode_simple(&simple))
}

fn instance_composite(
    font_bytes: &[u8],
    data: &[u8],
    gvar: Option<&GvarHeader>,
    gid: u16,
    location: &[f32],
) -> Option<Vec<u8>> {
    let n_comp = count_components(data)?;
    // Variation points: 4 phantoms + one (x,y) origin per component.
    let mut pts = vec![(0i32, 0i32); PHANTOMS + n_comp];
    // Seed component origins from the glyph record.
    let mut p = 10usize;
    for slot in pts.iter_mut().skip(PHANTOMS) {
        let flags = be_u16(data, p)?;
        let (arg1, arg2, rec_len) = component_args(data, p, flags)?;
        if flags & ARGS_ARE_XY != 0 {
            *slot = (arg1, arg2);
        }
        p = p.checked_add(rec_len)?;
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }
    if let Some(gvar) = gvar {
        apply_store(font_bytes, gvar, gid, location, &mut pts, None);
    }
    // Rewrite XY args.
    let mut out = data.to_vec();
    p = 10;
    let mut ci = 0usize;
    loop {
        let flags = be_u16(&out, p)?;
        let rec_len = component_record_len(flags);
        if flags & ARGS_ARE_XY != 0 {
            if let Some(&(x, y)) = pts.get(PHANTOMS + ci) {
                write_component_xy(&mut out, p, flags, x, y)?;
            }
        }
        ci += 1;
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
        p = p.checked_add(rec_len)?;
        if p >= out.len() {
            break;
        }
    }
    Some(out)
}

fn count_components(data: &[u8]) -> Option<usize> {
    let mut p = 10usize;
    let mut n = 0usize;
    loop {
        let flags = be_u16(data, p)?;
        n = n.checked_add(1)?;
        if n > MAX_GVAR_POINTS {
            return None;
        }
        let rec_len = component_record_len(flags);
        if flags & MORE_COMPONENTS == 0 {
            return Some(n);
        }
        p = p.checked_add(rec_len)?;
    }
}

fn component_record_len(flags: u16) -> usize {
    let mut n = 4usize;
    n += if flags & ARG_WORDS != 0 { 4 } else { 2 };
    if flags & TWO_BY_TWO != 0 {
        n += 8;
    } else if flags & X_Y_SCALE != 0 {
        n += 4;
    } else if flags & WE_HAVE_SCALE != 0 {
        n += 2;
    }
    n
}

fn component_args(data: &[u8], p: usize, flags: u16) -> Option<(i32, i32, usize)> {
    let rec_len = component_record_len(flags);
    let arg_base = p.checked_add(4)?;
    let (a, b) = if flags & ARG_WORDS != 0 {
        (
            i32::from(be_i16(data, arg_base)?),
            i32::from(be_i16(data, arg_base.checked_add(2)?)?),
        )
    } else {
        let x = *data.get(arg_base)?;
        let y = *data.get(arg_base.checked_add(1)?)?;
        if flags & ARGS_ARE_XY != 0 {
            (i32::from(x as i8), i32::from(y as i8))
        } else {
            (i32::from(x), i32::from(y))
        }
    };
    Some((a, b, rec_len))
}

fn write_component_xy(out: &mut [u8], p: usize, flags: u16, x: i32, y: i32) -> Option<()> {
    let arg_base = p.checked_add(4)?;
    if flags & ARG_WORDS != 0 {
        let xb = i16::try_from(x.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).ok()?;
        let yb = i16::try_from(y.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).ok()?;
        write_i16(out, arg_base, xb)?;
        write_i16(out, arg_base.checked_add(2)?, yb)?;
    } else {
        *out.get_mut(arg_base)? = x.clamp(-128, 127) as u8;
        *out.get_mut(arg_base.checked_add(1)?)? = y.clamp(-128, 127) as u8;
    }
    Some(())
}

fn write_i16(d: &mut [u8], o: usize, v: i16) -> Option<()> {
    let b = v.to_be_bytes();
    let dst = d.get_mut(o..o.checked_add(2)?)?;
    dst.copy_from_slice(&b);
    Some(())
}

fn apply_store(
    font_bytes: &[u8],
    gvar: &GvarHeader,
    gid: u16,
    location: &[f32],
    points: &mut [(i32, i32)],
    contours: Option<&[u16]>,
) {
    let gid_us = gid as usize;
    let Some(&start) = gvar.glyph_data_off.get(gid_us) else {
        return;
    };
    let Some(&end) = gvar.glyph_data_off.get(gid_us.saturating_add(1)) else {
        return;
    };
    if start >= end || end > gvar.table_end {
        return;
    }
    let Some(tuples) = read_tuples(font_bytes, start, end, gvar, location) else {
        return;
    };
    let n_var = points.len().saturating_add(PHANTOMS);
    apply_tuples(font_bytes, &tuples, points, contours, n_var);
}

fn apply_tuples(
    d: &[u8],
    tuples: &[TupleWork],
    points: &mut [(i32, i32)],
    contours: Option<&[u16]>,
    n_var: usize,
) {
    let n_real = points.len();
    let mut acc_x = vec![0.0f32; n_var];
    let mut acc_y = vec![0.0f32; n_var];
    let mut touched = vec![false; n_var];
    let mut shared_idx: Option<Vec<usize>> = None;
    for t in tuples {
        if t.scalar.abs() < 1e-8 {
            continue;
        }
        let mut cursor = t.data_off;
        if t.shared_points && !t.private_points && shared_idx.is_none() {
            shared_idx = unpack_points(d, &mut cursor, t.data_end, n_var);
        }
        let idx = if t.private_points {
            match unpack_points(d, &mut cursor, t.data_end, n_var) {
                Some(v) => v,
                None => continue,
            }
        } else if t.shared_points {
            match shared_idx.as_ref() {
                Some(v) => v.clone(),
                None => continue,
            }
        } else {
            (0..n_var.min(n_real + PHANTOMS)).collect()
        };
        let Some((dx, dy)) = unpack_xy_deltas(d, &mut cursor, t.data_end, idx.len()) else {
            continue;
        };
        for (k, &pi) in idx.iter().enumerate() {
            if pi >= n_var {
                continue;
            }
            if let (Some(ax), Some(&x)) = (acc_x.get_mut(pi), dx.get(k)) {
                *ax += t.scalar * f32::from(x);
            }
            if let (Some(ay), Some(&y)) = (acc_y.get_mut(pi), dy.get(k)) {
                *ay += t.scalar * f32::from(y);
            }
            if let Some(flag) = touched.get_mut(pi) {
                *flag = true;
            }
        }
    }
    if let Some(ends) = contours {
        iup(&mut acc_x, &touched, ends, n_real);
        iup(&mut acc_y, &touched, ends, n_real);
    }
    for (i, pt) in points.iter_mut().enumerate() {
        let dx = acc_x.get(i).copied().unwrap_or(0.0).round() as i32;
        let dy = acc_y.get(i).copied().unwrap_or(0.0).round() as i32;
        pt.0 = pt.0.saturating_add(dx);
        pt.1 = pt.1.saturating_add(dy);
    }
}

struct TupleWork {
    scalar: f32,
    data_off: usize,
    data_end: usize,
    private_points: bool,
    shared_points: bool,
}

fn read_tuples(
    d: &[u8],
    start: usize,
    end: usize,
    gvar: &GvarHeader,
    location: &[f32],
) -> Option<Vec<TupleWork>> {
    if start.checked_add(4)? > end {
        return Some(Vec::new());
    }
    let packed = be_u16(d, start)?;
    let has_shared_points = packed & 0x8000 != 0;
    let tuple_count = (packed & 0x0FFF) as usize;
    let data_offset = be_u16(d, off(start, 2)?)? as usize;
    let mut serialized = off(start, data_offset)?;
    let mut cursor = off(start, 4)?;
    let n = tuple_count.min(MAX_TUPLES);
    let mut out = Vec::with_capacity(n);
    // Shared point numbers sit at the front of the serialized stream when
    // the high bit of tupleVariationCount is set. Each tuple that does NOT
    // have PRIVATE_POINT_NUMBERS reuses them; we leave them in-stream and
    // let apply_tuples consume them from the first such tuple's data_off.
    for _ in 0..n {
        if cursor.checked_add(4)? > end {
            break;
        }
        let var_data_size = be_u16(d, cursor)? as usize;
        let tuple_index = be_u16(d, off(cursor, 2)?)?;
        cursor = off(cursor, 4)?;
        let peak = if tuple_index & EMBEDDED_PEAK != 0 {
            let mut t = Vec::with_capacity(gvar.axis_count);
            for _ in 0..gvar.axis_count {
                t.push(f2dot14(be_i16(d, cursor)?));
                cursor = off(cursor, 2)?;
            }
            t
        } else {
            let idx = (tuple_index & TUPLE_INDEX_MASK) as usize;
            gvar.shared_tuples.get(idx)?.clone()
        };
        let (start_t, end_t) = if tuple_index & INTERMEDIATE != 0 {
            let mut s = Vec::with_capacity(gvar.axis_count);
            let mut e = Vec::with_capacity(gvar.axis_count);
            for _ in 0..gvar.axis_count {
                s.push(f2dot14(be_i16(d, cursor)?));
                cursor = off(cursor, 2)?;
            }
            for _ in 0..gvar.axis_count {
                e.push(f2dot14(be_i16(d, cursor)?));
                cursor = off(cursor, 2)?;
            }
            (Some(s), Some(e))
        } else {
            (None, None)
        };
        let data_end = serialized.checked_add(var_data_size)?;
        if data_end > end {
            break;
        }
        out.push(TupleWork {
            scalar: tuple_scalar(&peak, start_t.as_deref(), end_t.as_deref(), location),
            data_off: serialized,
            data_end,
            private_points: tuple_index & PRIVATE_POINTS != 0,
            shared_points: has_shared_points,
        });
        serialized = data_end;
    }
    Some(out)
}

fn tuple_scalar(peak: &[f32], start: Option<&[f32]>, end: Option<&[f32]>, loc: &[f32]) -> f32 {
    let mut scalar = 1.0f32;
    for i in 0..peak.len() {
        let p = peak.get(i).copied().unwrap_or(0.0);
        let v = loc.get(i).copied().unwrap_or(0.0);
        if let (Some(s), Some(e)) = (start, end) {
            let a = s.get(i).copied().unwrap_or(0.0);
            let b = e.get(i).copied().unwrap_or(0.0);
            if v < a || v > b {
                return 0.0;
            }
            if (v - p).abs() < 1e-8 {
                continue;
            }
            if v < p {
                let span = p - a;
                if span.abs() < 1e-8 {
                    return 0.0;
                }
                scalar *= (v - a) / span;
            } else {
                let span = b - p;
                if span.abs() < 1e-8 {
                    return 0.0;
                }
                scalar *= (b - v) / span;
            }
        } else {
            if p.abs() < 1e-8 {
                continue;
            }
            if p > 0.0 && (v <= 0.0 || v >= p) && (v - p).abs() >= 1e-8 && (v <= 0.0 || v > p) {
                return 0.0;
            }
            if p < 0.0 && (v >= 0.0 || v <= p) && (v - p).abs() >= 1e-8 && (v >= 0.0 || v < p) {
                return 0.0;
            }
            if (v - p).abs() < 1e-8 {
                continue;
            }
            scalar *= v / p;
        }
    }
    scalar
}

fn unpack_points(d: &[u8], cursor: &mut usize, end: usize, n_var: usize) -> Option<Vec<usize>> {
    if *cursor >= end {
        return None;
    }
    let n0 = *d.get(*cursor)? as usize;
    *cursor = cursor.checked_add(1)?;
    let count = if n0 == 0 {
        return Some((0..n_var).collect());
    } else if n0 & 0x80 != 0 {
        if *cursor >= end {
            return None;
        }
        let n1 = *d.get(*cursor)? as usize;
        *cursor = cursor.checked_add(1)?;
        ((n0 & 0x7F) << 8) | n1
    } else {
        n0
    };
    let count = count.min(MAX_GVAR_POINTS);
    let mut out = Vec::with_capacity(count);
    let mut last = 0usize;
    while out.len() < count {
        if *cursor >= end {
            return None;
        }
        let ctrl = *d.get(*cursor)?;
        *cursor = cursor.checked_add(1)?;
        let words = ctrl & 0x80 != 0;
        let run = usize::from(ctrl & 0x7F) + 1;
        for _ in 0..run {
            if out.len() >= count {
                break;
            }
            let delta = if words {
                let v = be_u16(d, *cursor)? as usize;
                *cursor = cursor.checked_add(2)?;
                v
            } else {
                let v = *d.get(*cursor)? as usize;
                *cursor = cursor.checked_add(1)?;
                v
            };
            last = last.checked_add(delta)?;
            out.push(last);
        }
    }
    Some(out)
}

fn unpack_xy_deltas(
    d: &[u8],
    cursor: &mut usize,
    end: usize,
    n: usize,
) -> Option<(Vec<i16>, Vec<i16>)> {
    let xs = unpack_delta_run(d, cursor, end, n)?;
    let ys = unpack_delta_run(d, cursor, end, n)?;
    Some((xs, ys))
}

fn unpack_delta_run(d: &[u8], cursor: &mut usize, end: usize, n: usize) -> Option<Vec<i16>> {
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        if *cursor >= end {
            return None;
        }
        let ctrl = *d.get(*cursor)?;
        *cursor = cursor.checked_add(1)?;
        let run = usize::from(ctrl & 0x3F) + 1;
        let zeros = ctrl & 0xC0 == 0;
        let bytes = ctrl & 0x40 != 0 && ctrl & 0x80 == 0;
        let words = ctrl & 0x80 != 0;
        for _ in 0..run {
            if out.len() >= n {
                break;
            }
            if zeros {
                out.push(0);
            } else if bytes {
                let b = *d.get(*cursor)? as i8;
                *cursor = cursor.checked_add(1)?;
                out.push(i16::from(b));
            } else if words {
                let v = be_i16(d, *cursor)?;
                *cursor = cursor.checked_add(2)?;
                out.push(v);
            } else {
                out.push(0);
            }
        }
    }
    Some(out)
}

/// IUP: for each contour, interpolate untouched coordinates between the
/// nearest touched neighbours (OpenType `gvar` IUP semantics).
fn iup(delta: &mut [f32], touched: &[bool], ends: &[u16], n_real: usize) {
    if n_real == 0 || ends.is_empty() {
        return;
    }
    let mut start = 0usize;
    for &end in ends {
        let end = end as usize;
        if end >= n_real || start > end {
            break;
        }
        iup_contour(delta, touched, start, end);
        start = end.saturating_add(1);
    }
}

fn iup_contour(delta: &mut [f32], touched: &[bool], start: usize, end: usize) {
    let n = end.saturating_sub(start).saturating_add(1);
    if n == 0 {
        return;
    }
    let mut touched_idx: Vec<usize> = (start..=end)
        .filter(|&i| touched.get(i).copied().unwrap_or(false))
        .collect();
    if touched_idx.is_empty() {
        return;
    }
    if touched_idx.len() == 1 {
        let v = touched_idx
            .first()
            .and_then(|&i| delta.get(i).copied())
            .unwrap_or(0.0);
        for i in start..=end {
            if !touched.get(i).copied().unwrap_or(false) {
                if let Some(slot) = delta.get_mut(i) {
                    *slot = v;
                }
            }
        }
        return;
    }
    let first = *touched_idx.first().unwrap_or(&start);
    touched_idx.push(first); // wrap
    for w in touched_idx.windows(2) {
        let a = w[0];
        let b = w[1];
        let da = delta.get(a).copied().unwrap_or(0.0);
        let db = delta.get(b).copied().unwrap_or(0.0);
        if a == b {
            continue;
        }
        // Walk forward from a to b around the contour.
        let mut i = next_in(a, start, end);
        let span = dist_in(a, b, start, end);
        let mut step = 1usize;
        while i != b && step <= n {
            if !touched.get(i).copied().unwrap_or(false) {
                let t = if span == 0 {
                    0.0
                } else {
                    step as f32 / span as f32
                };
                if let Some(slot) = delta.get_mut(i) {
                    *slot = da + (db - da) * t;
                }
            }
            i = next_in(i, start, end);
            step += 1;
        }
    }
}

fn next_in(i: usize, start: usize, end: usize) -> usize {
    if i >= end { start } else { i + 1 }
}

fn dist_in(a: usize, b: usize, start: usize, end: usize) -> usize {
    if b >= a {
        b - a
    } else {
        (end - a) + (b - start) + 1
    }
}

fn encode_simple(g: &SimpleGlyph) -> Vec<u8> {
    let n = g.points.len();
    let mut out = Vec::new();
    let n_contours = g.contour_ends.len();
    out.extend_from_slice(&(n_contours as i16).to_be_bytes());
    let (xmin, ymin, xmax, ymax) = bbox(&g.points);
    out.extend_from_slice(&xmin.to_be_bytes());
    out.extend_from_slice(&ymin.to_be_bytes());
    out.extend_from_slice(&xmax.to_be_bytes());
    out.extend_from_slice(&ymax.to_be_bytes());
    for &e in &g.contour_ends {
        out.extend_from_slice(&e.to_be_bytes());
    }
    out.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
    let mut flags = Vec::with_capacity(n);
    let mut prev = (0i32, 0i32);
    let mut dxs = Vec::with_capacity(n);
    let mut dys = Vec::with_capacity(n);
    for (i, &(x, y)) in g.points.iter().enumerate() {
        let dx = x.saturating_sub(prev.0);
        let dy = y.saturating_sub(prev.1);
        dxs.push(dx);
        dys.push(dy);
        let mut f = if g.on_curve.get(i).copied().unwrap_or(true) {
            ON_CURVE
        } else {
            0
        };
        if dx == 0 {
            f |= X_SAME_OR_POS;
        }
        if dy == 0 {
            f |= Y_SAME_OR_POS;
        }
        flags.push(f);
        prev = (x, y);
    }
    out.extend_from_slice(&flags);
    for (i, &dx) in dxs.iter().enumerate() {
        let f = flags.get(i).copied().unwrap_or(0);
        if f & X_SAME_OR_POS != 0 && dx == 0 {
            continue;
        }
        out.extend_from_slice(&(dx as i16).to_be_bytes());
    }
    for (i, &dy) in dys.iter().enumerate() {
        let f = flags.get(i).copied().unwrap_or(0);
        if f & Y_SAME_OR_POS != 0 && dy == 0 {
            continue;
        }
        out.extend_from_slice(&(dy as i16).to_be_bytes());
    }
    out
}

fn bbox(points: &[(i32, i32)]) -> (i16, i16, i16, i16) {
    if points.is_empty() {
        return (0, 0, 0, 0);
    }
    let mut xmin = i32::MAX;
    let mut ymin = i32::MAX;
    let mut xmax = i32::MIN;
    let mut ymax = i32::MIN;
    for &(x, y) in points {
        xmin = xmin.min(x);
        ymin = ymin.min(y);
        xmax = xmax.max(x);
        ymax = ymax.max(y);
    }
    (
        clamp_i16(xmin),
        clamp_i16(ymin),
        clamp_i16(xmax),
        clamp_i16(ymax),
    )
}

fn clamp_i16(v: i32) -> i16 {
    v.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn encode_loca(offs: &[u32]) -> Vec<u8> {
    let mut o = Vec::with_capacity(offs.len() * 4);
    for &v in offs {
        o.extend_from_slice(&v.to_be_bytes());
    }
    o
}

fn rebuild_static(font: &Font, glyf: Vec<u8>, loca: Vec<u8>) -> Option<Vec<u8>> {
    let src = font.raw_bytes();
    let num_tables = be_u16(src, 4)? as usize;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    for i in 0..num_tables {
        let rec = off_mul(12, i, 16)?;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(src.get(rec..rec.checked_add(4)?)?);
        if &tag == b"fvar" || &tag == b"avar" || &tag == b"gvar" {
            continue;
        }
        if &tag == b"glyf" {
            tables.push((tag, glyf.clone()));
            continue;
        }
        if &tag == b"loca" {
            tables.push((tag, loca.clone()));
            continue;
        }
        if &tag == b"head" {
            let (o, l) = find_table_full(src, b"head")?;
            let mut head = src.get(o..off(o, l)?)?.to_vec();
            // long loca
            if head.len() >= 52 {
                head[50..52].copy_from_slice(&1u16.to_be_bytes());
            }
            // zero checkSumAdjustment
            if head.len() >= 12 {
                head[8..12].copy_from_slice(&0u32.to_be_bytes());
            }
            tables.push((tag, head));
            continue;
        }
        if &tag == b"maxp" {
            let (o, l) = find_table_full(src, b"maxp")?;
            tables.push((tag, src.get(o..off(o, l)?)?.to_vec()));
            continue;
        }
        let (o, l) = find_table_full(src, &tag)?;
        tables.push((tag, src.get(o..off(o, l)?)?.to_vec()));
    }
    tables.sort_by_key(|a| a.0);
    assemble_sfnt(&tables)
}

fn assemble_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Option<Vec<u8>> {
    let num_tables = tables.len();
    let mut pw = 1usize;
    let mut es = 0u16;
    while pw * 2 <= num_tables {
        pw *= 2;
        es += 1;
    }
    let search_range = (pw as u16).wrapping_mul(16);
    let range_shift = (num_tables as u16)
        .wrapping_mul(16)
        .wrapping_sub(search_range);
    let dir_size = 12 + num_tables * 16;
    let mut body = Vec::new();
    let mut records: Vec<([u8; 4], u32, u32, u32)> = Vec::new();
    let mut head_offset = 0usize;
    for (tag, bytes) in tables {
        while (dir_size + body.len()) % 4 != 0 {
            body.push(0);
        }
        let table_offset = dir_size + body.len();
        if tag == b"head" {
            head_offset = table_offset;
        }
        records.push((
            *tag,
            table_checksum(bytes),
            u32::try_from(table_offset).ok()?,
            u32::try_from(bytes.len()).ok()?,
        ));
        body.extend_from_slice(bytes);
    }
    while body.len() % 4 != 0 {
        body.push(0);
    }
    let mut out = Vec::with_capacity(dir_size + body.len());
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&es.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());
    for (tag, checksum, toff, tlen) in &records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum.to_be_bytes());
        out.extend_from_slice(&toff.to_be_bytes());
        out.extend_from_slice(&tlen.to_be_bytes());
    }
    out.extend_from_slice(&body);
    let adj = 0xB1B0_AFBAu32.wrapping_sub(table_checksum(&out));
    write_u32(&mut out, off(head_offset, 8)?, adj)?;
    Some(out)
}

/// Tiny OFL `wght` variable face: one triangle glyph, peak gvar +50 x on p0.
/// ASCII printable characters map to glyph 0 so host-font tests can embed it.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
pub(crate) fn variable_triangle_fixture() -> Vec<u8> {
    fn push16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    fn push_i16(out: &mut Vec<u8>, v: i16) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    fn push32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let glyph = encode_simple(&SimpleGlyph {
        points: vec![(0, 0), (100, 0), (50, 100)],
        on_curve: vec![true, true, true],
        contour_ends: vec![2],
    });
    let mut loca = Vec::new();
    push32(&mut loca, 0);
    push32(&mut loca, glyph.len() as u32);
    // Identical gvar to the gk3v.2 unit-test builder (private p0 +50 x at peak).
    let mut payload = vec![1, 0, 0, 0x80];
    payload.extend_from_slice(&50i16.to_be_bytes());
    payload.push(0x00);
    let mut gvd = Vec::new();
    push16(&mut gvd, 1);
    push16(&mut gvd, 0);
    push16(&mut gvd, payload.len() as u16);
    push16(&mut gvd, EMBEDDED_PEAK | PRIVATE_POINTS);
    push_i16(&mut gvd, (1.0_f32 * 16384.0).round() as i16);
    let data_off = gvd.len() as u16;
    gvd[2..4].copy_from_slice(&data_off.to_be_bytes());
    gvd.extend_from_slice(&payload);
    let mut gvar = Vec::new();
    push16(&mut gvar, 1);
    push16(&mut gvar, 0);
    push16(&mut gvar, 1);
    push16(&mut gvar, 0);
    push32(&mut gvar, 20);
    push16(&mut gvar, 1);
    push16(&mut gvar, 0);
    push32(&mut gvar, 20);
    let array_bytes = 4usize;
    let start_words = (array_bytes / 2) as u16;
    let end_words = (array_bytes + gvd.len()).div_ceil(2) as u16;
    push16(&mut gvar, start_words);
    push16(&mut gvar, end_words);
    gvar.extend_from_slice(&gvd);
    if gvar.len() % 2 != 0 {
        gvar.push(0);
    }
    let tables: Vec<([u8; 4], Vec<u8>)> = vec![
        (*b"head", fixture_head()),
        (*b"maxp", fixture_maxp(1)),
        (*b"hhea", fixture_hhea(1)),
        (*b"hmtx", fixture_hmtx(1)),
        (*b"cmap", fixture_cmap_ascii()),
        (*b"loca", loca),
        (*b"glyf", glyph),
        (*b"fvar", fixture_fvar_wght()),
        (*b"gvar", gvar),
    ];
    assemble_sfnt(&tables).unwrap_or_default()
}

fn fixture_head() -> Vec<u8> {
    let mut t = vec![0u8; 54];
    t[18..20].copy_from_slice(&1000u16.to_be_bytes());
    t[50..52].copy_from_slice(&1u16.to_be_bytes());
    t
}

fn fixture_maxp(n: u16) -> Vec<u8> {
    let mut t = vec![0u8; 6];
    t[4..6].copy_from_slice(&n.to_be_bytes());
    t
}

fn fixture_hhea(n: u16) -> Vec<u8> {
    let mut t = vec![0u8; 36];
    t[4..6].copy_from_slice(&700i16.to_be_bytes());
    t[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    t[8..10].copy_from_slice(&50i16.to_be_bytes());
    t[34..36].copy_from_slice(&n.to_be_bytes());
    t
}

fn fixture_hmtx(n: u16) -> Vec<u8> {
    let mut t = Vec::new();
    for _ in 0..n {
        t.extend_from_slice(&500u16.to_be_bytes());
        t.extend_from_slice(&0i16.to_be_bytes());
    }
    t
}

fn fixture_cmap_ascii() -> Vec<u8> {
    // Format 4: map U+0020 (space) to glyph 0. Every other character is
    // unmapped and `Font::glyph_index` returns `.notdef` (also gid 0). A
    // range 0x20..=0x7E with idDelta = -0x20 would send 'A' to gid 33, which
    // this 1-glyph face does not have.
    let mut t = Vec::new();
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&3u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&12u32.to_be_bytes());
    t.extend_from_slice(&4u16.to_be_bytes());
    t.extend_from_slice(&32u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&4u16.to_be_bytes());
    t.extend_from_slice(&4u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&0x0020u16.to_be_bytes());
    t.extend_from_slice(&0xFFFFu16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&0x0020u16.to_be_bytes());
    t.extend_from_slice(&0xFFFFu16.to_be_bytes());
    t.extend_from_slice(&(-0x20i16).to_be_bytes());
    t.extend_from_slice(&1i16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn fixture_fvar_wght() -> Vec<u8> {
    let mut t = Vec::new();
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&16u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&1u16.to_be_bytes());
    t.extend_from_slice(&20u16.to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&4u16.to_be_bytes());
    t.extend_from_slice(b"wght");
    let to_fixed = |v: f32| (f64::from(v) * 65536.0).round() as i32;
    t.extend_from_slice(&to_fixed(100.0).to_be_bytes());
    t.extend_from_slice(&to_fixed(400.0).to_be_bytes());
    t.extend_from_slice(&to_fixed(900.0).to_be_bytes());
    t.extend_from_slice(&0u16.to_be_bytes());
    t.extend_from_slice(&256u16.to_be_bytes());
    t
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::Font;

    fn push16(out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    fn push_i16(out: &mut Vec<u8>, v: i16) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    fn push32(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_be_bytes());
    }
    fn f32_to_fixed(v: f32) -> i32 {
        (f64::from(v) * 65536.0).round() as i32
    }
    fn f32_to_f2dot14(v: f32) -> i16 {
        (v * 16384.0).round() as i16
    }

    fn sfnt(tables: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        assemble_sfnt(
            &tables
                .iter()
                .map(|(t, b)| (**t, b.clone()))
                .collect::<Vec<_>>(),
        )
        .expect("assemble")
    }

    fn head_table() -> Vec<u8> {
        let mut t = vec![0u8; 54];
        t[18..20].copy_from_slice(&1000u16.to_be_bytes());
        t[50..52].copy_from_slice(&1u16.to_be_bytes());
        t
    }
    fn maxp_table(n: u16) -> Vec<u8> {
        let mut t = vec![0u8; 6];
        t[4..6].copy_from_slice(&n.to_be_bytes());
        t
    }
    fn hhea_table(n: u16) -> Vec<u8> {
        let mut t = vec![0u8; 36];
        t[4..6].copy_from_slice(&700i16.to_be_bytes());
        t[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        t[8..10].copy_from_slice(&50i16.to_be_bytes());
        t[34..36].copy_from_slice(&n.to_be_bytes());
        t
    }
    fn hmtx(n: u16) -> Vec<u8> {
        let mut t = Vec::new();
        for _ in 0..n {
            push16(&mut t, 500);
            push_i16(&mut t, 0);
        }
        t
    }
    fn cmap4() -> Vec<u8> {
        let mut t = Vec::new();
        push16(&mut t, 0);
        push16(&mut t, 1);
        push16(&mut t, 3);
        push16(&mut t, 1);
        push32(&mut t, 12);
        push16(&mut t, 4);
        push16(&mut t, 24);
        push16(&mut t, 0);
        push16(&mut t, 2);
        push16(&mut t, 0);
        push16(&mut t, 0);
        push16(&mut t, 0);
        push16(&mut t, 0xFFFF);
        push16(&mut t, 0);
        push16(&mut t, 0xFFFF);
        push16(&mut t, 1);
        push16(&mut t, 0);
        t
    }
    fn fvar_wght() -> Vec<u8> {
        let mut t = Vec::new();
        push16(&mut t, 1);
        push16(&mut t, 0);
        push16(&mut t, 16);
        push16(&mut t, 0);
        push16(&mut t, 1);
        push16(&mut t, 20);
        push16(&mut t, 0);
        push16(&mut t, 4);
        t.extend_from_slice(b"wght");
        t.extend_from_slice(&f32_to_fixed(100.0).to_be_bytes());
        t.extend_from_slice(&f32_to_fixed(400.0).to_be_bytes());
        t.extend_from_slice(&f32_to_fixed(900.0).to_be_bytes());
        push16(&mut t, 0);
        push16(&mut t, 256);
        t
    }

    /// Triangle: (0,0), (100,0), (50,100), all on-curve.
    fn triangle_glyph() -> Vec<u8> {
        encode_simple(&SimpleGlyph {
            points: vec![(0, 0), (100, 0), (50, 100)],
            on_curve: vec![true, true, true],
            contour_ends: vec![2],
        })
    }

    /// `gvar` for one glyph, one axis, one tuple at peak +1 with a private
    /// point-0 delta of +50 x.
    fn gvar_move_p0() -> Vec<u8> {
        // One private point (p0). IUP copies that delta across the contour.
        let mut payload = vec![1, 0, 0, 0x80];
        payload.extend_from_slice(&50i16.to_be_bytes());
        payload.push(0x00); // y: one zero

        let mut gvd = Vec::new();
        push16(&mut gvd, 1);
        push16(&mut gvd, 0);
        push16(&mut gvd, u16::try_from(payload.len()).unwrap());
        push16(&mut gvd, EMBEDDED_PEAK | PRIVATE_POINTS);
        push_i16(&mut gvd, f32_to_f2dot14(1.0));
        let data_off = gvd.len();
        gvd[2..4].copy_from_slice(&(data_off as u16).to_be_bytes());
        gvd.extend_from_slice(&payload);
        build_gvar_table(&gvd)
    }

    fn build_gvar_table(gvd: &[u8]) -> Vec<u8> {
        let mut table = Vec::new();
        push16(&mut table, 1);
        push16(&mut table, 0);
        push16(&mut table, 1);
        push16(&mut table, 0);
        push32(&mut table, 20);
        push16(&mut table, 1);
        push16(&mut table, 0); // short offsets
        push32(&mut table, 20);
        // offset array at 20: 2 × u16, values in words from the array start.
        let array_bytes = 4usize;
        let start_words = (array_bytes / 2) as u16; // 2
        let end_bytes = array_bytes + gvd.len();
        let end_words = end_bytes.div_ceil(2) as u16;
        push16(&mut table, start_words);
        push16(&mut table, end_words);
        table.extend_from_slice(gvd);
        if table.len() % 2 != 0 {
            table.push(0);
        }
        table
    }

    fn log_check(id: &str, subject: &str, ok: bool) {
        eprintln!(
            "check id={id} subject={subject} outcome={}",
            if ok { "PASS" } else { "FAIL" }
        );
        assert!(ok, "{id}: {subject}");
    }

    #[test]
    fn fixture_peak_moves_like_the_unit_builder() {
        let font = Font::parse(variable_triangle_fixture()).expect("fixture parses");
        log_check(
            "gk3v.3.fix.cmap",
            "space and letters resolve to gid 0",
            font.glyph_index(' ') == 0 && font.glyph_index('A') == 0 && font.glyph_index('H') == 0,
        );
        let def = font.instance(400.0).expect("400");
        let mid = font.instance(650.0).expect("650");
        let peak = font.instance(900.0).expect("900");
        let d0 = decode_simple(def.glyph_data(0).unwrap(), 1).unwrap();
        let m0 = decode_simple(mid.glyph_data(0).unwrap(), 1).unwrap();
        let p0 = decode_simple(peak.glyph_data(0).unwrap(), 1).unwrap();
        log_check("gk3v.3.fix.default", "p0 origin", d0.points[0] == (0, 0));
        log_check("gk3v.3.fix.peak", "peak +50 x", p0.points[0] == (50, 0));
        log_check(
            "gk3v.3.fix.mid",
            "650 differs from 400",
            m0.points[0] != d0.points[0],
        );
    }

    fn variable_triangle() -> Font {
        let glyph = triangle_glyph();
        let mut loca = Vec::new();
        push32(&mut loca, 0);
        push32(&mut loca, u32::try_from(glyph.len()).unwrap());
        let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"head", head_table()),
            (b"maxp", maxp_table(1)),
            (b"hhea", hhea_table(1)),
            (b"hmtx", hmtx(1)),
            (b"cmap", cmap4()),
            (b"loca", loca),
            (b"glyf", glyph),
            (b"fvar", fvar_wght()),
            (b"gvar", gvar_move_p0()),
        ];
        Font::parse(sfnt(&tables)).expect("variable triangle parses")
    }

    #[test]
    fn instance_moves_private_point_at_peak() {
        let font = variable_triangle();
        let def = font.instance(400.0).expect("default instance");
        let peak = font.instance(900.0).expect("peak instance");
        let def2 = font.instance(400.0).expect("default again");
        log_check(
            "gk3v.2.det",
            "same weight twice is byte-identical",
            def.raw_bytes() == def2.raw_bytes(),
        );
        let d0 = decode_simple(def.glyph_data(0).unwrap(), 1).unwrap();
        let p0 = decode_simple(peak.glyph_data(0).unwrap(), 1).unwrap();
        log_check(
            "gk3v.2.default",
            "default leaves p0 at origin",
            d0.points[0] == (0, 0),
        );
        log_check(
            "gk3v.2.peak",
            "peak wght moves p0 by +50 x",
            p0.points[0] == (50, 0),
        );
        log_check(
            "gk3v.2.untouched",
            "single touched point shifts contour per OpenType IUP",
            p0.points[1] == (150, 0) && p0.points[2] == (100, 100),
        );
        log_check(
            "gk3v.2.static",
            "instance drops fvar/gvar",
            def.axes().is_empty() && find_table_full(def.raw_bytes(), b"gvar").is_none(),
        );
    }

    #[test]
    fn instance_is_deterministic_and_hostile_gvar_never_panics() {
        let font = variable_triangle();
        let a = font.instance(700.0).unwrap();
        let b = font.instance(700.0).unwrap();
        log_check(
            "gk3v.2.mid.det",
            "700 twice",
            a.raw_bytes() == b.raw_bytes(),
        );
        let mut state = 0xA5A5_5A5Au64;
        let mut lcg = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (state >> 33) as usize
        };
        for round in 0..64 {
            let mut mutated = font.raw_bytes().to_vec();
            let (off, len) = find_table_full(&mutated, b"gvar").unwrap();
            for _ in 0..4 {
                let pos = off + (lcg() % len);
                mutated[pos] ^= 1u8 << (lcg() % 8);
            }
            let outcome = std::panic::catch_unwind(move || {
                if let Ok(f) = Font::parse(mutated) {
                    let _ = f.instance(900.0);
                    let _ = f.instance(100.0);
                }
            });
            log_check(
                "gk3v.2.lcg",
                &format!("round {round} no panic"),
                outcome.is_ok(),
            );
        }
    }

    #[test]
    fn composite_origin_delta_is_applied() {
        // Component glyph 0 is the triangle; glyph 1 is a composite of gid 0
        // at (10, 20) with ARGS_ARE_XY | ARG_WORDS.
        let triangle = triangle_glyph();
        let mut composite = Vec::new();
        push_i16(&mut composite, -1);
        for v in [0i16, 0, 100, 100] {
            push_i16(&mut composite, v);
        }
        push16(&mut composite, ARG_WORDS | ARGS_ARE_XY); // no MORE
        push16(&mut composite, 0); // component gid
        push_i16(&mut composite, 10);
        push_i16(&mut composite, 20);

        let mut glyf = Vec::new();
        glyf.extend_from_slice(&triangle);
        let comp_off = glyf.len();
        glyf.extend_from_slice(&composite);
        let mut loca = Vec::new();
        push32(&mut loca, 0);
        push32(&mut loca, u32::try_from(comp_off).unwrap());
        push32(&mut loca, u32::try_from(glyf.len()).unwrap());

        // gvar for gid 1: 5 variation points (4 phantoms + 1 component).
        // Move the component origin +30 x at peak.
        let mut payload = vec![1, 0, 4, 0x80];
        payload.extend_from_slice(&30i16.to_be_bytes());
        payload.push(0x00); // y zeros

        let mut gvd0 = Vec::new(); // empty for gid 0
        push16(&mut gvd0, 0);
        push16(&mut gvd0, 4);

        let mut gvd1 = Vec::new();
        push16(&mut gvd1, 1);
        push16(&mut gvd1, 0);
        push16(&mut gvd1, u16::try_from(payload.len()).unwrap());
        push16(&mut gvd1, EMBEDDED_PEAK | PRIVATE_POINTS);
        push_i16(&mut gvd1, f32_to_f2dot14(1.0));
        let data_off = gvd1.len();
        gvd1[2..4].copy_from_slice(&(data_off as u16).to_be_bytes());
        gvd1.extend_from_slice(&payload);

        let mut gvar = Vec::new();
        push16(&mut gvar, 1);
        push16(&mut gvar, 0);
        push16(&mut gvar, 1);
        push16(&mut gvar, 0);
        push32(&mut gvar, 20);
        push16(&mut gvar, 2); // glyphCount
        push16(&mut gvar, 1); // long offsets
        push32(&mut gvar, 20);
        // 3 long offsets from the array start (byte 20)
        let array = 12usize;
        let o0 = array as u32;
        let o1 = (array + gvd0.len()) as u32;
        let o2 = o1 + gvd1.len() as u32;
        push32(&mut gvar, o0);
        push32(&mut gvar, o1);
        push32(&mut gvar, o2);
        gvar.extend_from_slice(&gvd0);
        gvar.extend_from_slice(&gvd1);

        let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"head", head_table()),
            (b"maxp", maxp_table(2)),
            (b"hhea", hhea_table(2)),
            (b"hmtx", hmtx(2)),
            (b"cmap", cmap4()),
            (b"loca", loca),
            (b"glyf", glyf),
            (b"fvar", fvar_wght()),
            (b"gvar", gvar),
        ];
        let font = Font::parse(sfnt(&tables)).expect("composite VF");
        let inst = font.instance(900.0).expect("instance composite");
        let data = inst.glyph_data(1).unwrap();
        let flags = be_u16(data, 10).unwrap();
        log_check(
            "gk3v.2.comp.flags",
            "composite still ARGS_ARE_XY | ARG_WORDS",
            flags & (ARG_WORDS | ARGS_ARE_XY) == ARG_WORDS | ARGS_ARE_XY,
        );
        let x = be_i16(data, 14).unwrap();
        let y = be_i16(data, 16).unwrap();
        log_check(
            "gk3v.2.comp.delta",
            "component origin 10+30, 20",
            x == 40 && y == 20,
        );
    }
}
