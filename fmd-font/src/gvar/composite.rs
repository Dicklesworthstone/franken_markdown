//! Composite variation pseudo-points, lossless offset encoding, and final ink.

use super::{
    ARG_WORDS, ARGS_ARE_XY, Font, GvarHeader, MORE_COMPONENTS, PHANTOMS, apply_store, be_i16,
    be_u16, component_args, component_record_len, count_components, hmtx_from_phantoms,
};

// The outline decoder separately bounds one glyph's depth, components and
// points. This allowance also prevents a font containing many overlapping
// composite expansions from multiplying that maximum by its glyph count.
const MAX_BOUND_WORK: usize = 16 * 1024 * 1024;

pub(super) fn instance(
    font: &Font,
    data: &[u8],
    gvar: Option<&GvarHeader>,
    gid: u16,
    location: &[f32],
) -> Option<(Vec<u8>, u16, i16)> {
    let count = count_components(data)?;
    let xmin = i32::from(be_i16(data, 2)?);
    let ymin = i32::from(be_i16(data, 4)?);
    let ymax = i32::from(be_i16(data, 8)?);
    let lsb = i32::from(font.left_side_bearing(gid));
    let advance = i32::from(font.advance_width(gid));

    // OpenType gvar: pseudo-points 0..componentCount are components, then
    // come the four phantom points. This ordering applies to both explicit
    // point lists and implicit all-point tuples.
    let mut points = vec![(0, 0); count + PHANTOMS];
    points[count] = (xmin - lsb, 0);
    points[count + 1] = (xmin - lsb + advance, 0);
    points[count + 2] = (0, ymax);
    points[count + 3] = (0, ymin);
    let mut offset = 10;
    for point in &mut points[..count] {
        let flags = be_u16(data, offset)?;
        let (x, y, length) = component_args(data, offset, flags)?;
        data.get(offset..offset.checked_add(length)?)?;
        if flags & ARGS_ARE_XY != 0 {
            *point = (x, y);
        }
        offset += length;
    }
    if let Some(gvar) = gvar {
        apply_store(
            font.raw_bytes(),
            gvar,
            gid,
            location,
            &mut points,
            None,
            None,
        );
    }

    let mut out = data.get(..10)?.to_vec();
    let mut offset = 10;
    for &(x, y) in &points[..count] {
        let flags = be_u16(data, offset)?;
        let length = component_record_len(flags);
        let end = offset.checked_add(length)?;
        let record = data.get(offset..end)?;
        if flags & ARGS_ARE_XY == 0 {
            // Point attachments retain their unsigned point indices; gvar
            // deltas do not change such a component's position.
            out.extend_from_slice(record);
        } else {
            let x = i16::try_from(x).ok()?;
            let y = i16::try_from(y).ok()?;
            let words =
                flags & ARG_WORDS != 0 || i8::try_from(x).is_err() || i8::try_from(y).is_err();
            let new_flags = if words { flags | ARG_WORDS } else { flags };
            out.extend_from_slice(&new_flags.to_be_bytes());
            out.extend_from_slice(record.get(2..4)?); // component glyph ID
            if words {
                out.extend_from_slice(&x.to_be_bytes());
                out.extend_from_slice(&y.to_be_bytes());
            } else {
                out.push(x as u8);
                out.push(y as u8);
            }
            let old_args_end = if flags & ARG_WORDS != 0 { 8 } else { 6 };
            out.extend_from_slice(record.get(old_args_end..)?); // unchanged transform
        }
        offset = end;
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }
    // Instructions, when present, remain after the final component record,
    // even when an earlier record grew from byte to word arguments.
    out.extend_from_slice(data.get(offset..)?);
    let (advance, lsb) = hmtx_from_phantoms(&[(xmin, 0)], &points[count..]);
    Some((out, advance, lsb))
}

/// Finalize dependent bounds only after every descendant has its new outline.
/// The shared outline decoder handles nested transforms and point attachments;
/// glyph order therefore has no bearing on the result. Updating hmtx by the
/// change in xMin preserves each glyph's already-interpolated left phantom.
pub(super) fn refresh_bounds(
    glyf: &mut [u8],
    loca: &[u32],
    metrics: &mut [(u16, i16)],
) -> Option<()> {
    let mut work_left = MAX_BOUND_WORK;
    for (index, (_, lsb)) in metrics.iter_mut().enumerate() {
        let start = usize::try_from(*loca.get(index)?).ok()?;
        let end = usize::try_from(*loca.get(index + 1)?).ok()?;
        let data = glyf.get(start..end)?;
        if data.is_empty() || be_i16(data, 0)? >= 0 {
            continue;
        }
        let old_xmin = be_i16(data, 2)?;
        let bounds = crate::outline::bounds_from_glyph_data(
            u16::try_from(index).ok()?,
            &|gid| {
                let first = usize::try_from(*loca.get(usize::from(gid))?).ok()?;
                let last = usize::try_from(*loca.get(usize::from(gid) + 1)?).ok()?;
                glyf.get(first..last)
            },
            &mut work_left,
        )
        .ok()?;
        *lsb = i16::try_from(i32::from(*lsb) + i32::from(bounds[0]) - i32::from(old_xmin)).ok()?;
        for (at, value) in bounds.into_iter().enumerate() {
            let offset = start + 2 + at * 2;
            glyf.get_mut(offset..offset + 2)?
                .copy_from_slice(&value.to_be_bytes());
        }
    }
    Some(())
}
