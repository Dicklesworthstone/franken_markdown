//! Validated, reusable CFF1 directories. INDEX objects retain offsets into the
//! immutable source table, so decoding one glyph never rebuilds the font's
//! complete CharStrings or subroutine arrays.
//!
//! CID structure follows Adobe Technical Note 5176, sections 13, 18 and 19:
//! https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf

use super::{dictionary, err, integer, malformed, slice};
use crate::{Font, SubsetError, SubsetErrorKind as Kind, be_u16, find_table_full};
use std::collections::BTreeMap;
use std::ops::Range;

const DIRECTORY_WORK: usize = 2_000_000;
const MAX_FONT_DICTS: usize = 256;

type Dict = BTreeMap<u16, Vec<f64>>;

fn spend(budget: &mut usize, work: usize, at: usize) -> Result<(), SubsetError> {
    *budget = budget
        .checked_sub(work)
        .ok_or(err(Kind::BudgetExceeded, at))?;
    Ok(())
}

/// A validated INDEX view: constant retained space regardless of glyph count.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Index {
    offsets: usize,
    base: usize,
    size: usize,
    pub(super) count: usize,
    pub(super) end: usize,
}

impl Index {
    pub(super) fn parse(d: &[u8], start: usize, budget: &mut usize) -> Result<Self, SubsetError> {
        let count = usize::from(be_u16(d, start).ok_or(malformed(start))?);
        spend(budget, count + 1, start)?;
        if count == 0 {
            return Ok(Self {
                end: start + 2,
                ..Self::default()
            });
        }
        let size = usize::from(*d.get(start + 2).ok_or(malformed(start + 2))?);
        if !(1..=4).contains(&size) {
            return Err(malformed(start + 2));
        }
        let offsets = start.checked_add(3).ok_or(malformed(start))?;
        let base = offsets
            .checked_add((count + 1) * size)
            .ok_or(malformed(start))?;
        // Prove the complete offset array exists before any indexed access.
        slice(d, offsets, (count + 1) * size)?;
        let mut result = Self {
            offsets,
            base,
            size,
            count,
            end: base,
        };
        let mut previous = result.offset(d, 0).ok_or(malformed(start))?;
        if previous != base {
            return Err(malformed(start));
        }
        for i in 1..=count {
            let next = result.offset(d, i).ok_or(malformed(start))?;
            if next < previous || next > d.len() {
                return Err(malformed(offsets + i * size));
            }
            previous = next;
        }
        result.end = previous;
        Ok(result)
    }

    fn offset(&self, d: &[u8], i: usize) -> Option<usize> {
        let offset = self.offsets.checked_add(i.checked_mul(self.size)?)?;
        let bytes = d.get(offset..offset.checked_add(self.size)?)?;
        let value = bytes
            .iter()
            .fold(0usize, |value, &byte| (value << 8) | usize::from(byte));
        self.base.checked_add(value.checked_sub(1)?)
    }

    pub(super) fn object<'a>(&self, d: &'a [u8], i: usize) -> Option<&'a [u8]> {
        if i >= self.count {
            return None;
        }
        d.get(self.offset(d, i)?..self.offset(d, i + 1)?)
    }
}

fn read_dict(d: &[u8], budget: &mut usize, at: usize) -> Result<Dict, SubsetError> {
    spend(budget, d.len(), at)?;
    dictionary(d)
}

fn one(dict: &Dict, op: u16, at: usize) -> Result<usize, SubsetError> {
    let values = dict
        .get(&op)
        .filter(|values| values.len() == 1)
        .ok_or(malformed(at))?;
    integer(values[0], at)
}

fn canonical_matrix(dict: &Dict, upem: u16, at: usize) -> Result<(), SubsetError> {
    if let Some(matrix) = dict.get(&0x0c07) {
        let unit = 1.0 / f64::from(upem);
        if matrix.len() != 6
            || matrix
                .iter()
                .zip([unit, 0.0, 0.0, unit, 0.0, 0.0])
                .any(|(a, b)| (a - b).abs() > 1e-12)
        {
            return Err(err(Kind::UnsupportedFormat, at));
        }
    } else if upem != 1000 {
        return Err(err(Kind::UnsupportedFormat, at));
    }
    Ok(())
}

fn private_subrs(
    d: &[u8],
    dict: &Dict,
    required: bool,
    budget: &mut usize,
    at: usize,
) -> Result<Index, SubsetError> {
    let Some(private) = dict.get(&18) else {
        return if required {
            Err(malformed(at))
        } else {
            Ok(Index::default())
        };
    };
    if private.len() != 2 {
        return Err(malformed(at));
    }
    let size = integer(private[0], at)?;
    let base = integer(private[1], at)?;
    let private = read_dict(slice(d, base, size)?, budget, base)?;
    if !private.contains_key(&19) {
        return Ok(Index::default());
    }
    let subrs = base
        .checked_add(one(&private, 19, base)?)
        .ok_or(malformed(base))?;
    Index::parse(d, subrs, budget)
}

fn cid_charset(
    d: &[u8],
    at: usize,
    glyphs: usize,
    cid_count: usize,
    budget: &mut usize,
) -> Result<(), SubsetError> {
    // CID fonts have no predefined charsets. Glyph 0 is implicitly CID 0.
    if at <= 2 || cid_count == 0 || cid_count > 65_536 {
        return Err(malformed(at));
    }
    spend(budget, glyphs, at)?;
    let format = *d.get(at).ok_or(malformed(at))?;
    if !matches!(format, 0..=2) {
        return Err(err(Kind::UnsupportedFormat, at));
    }
    let mut seen = [0u64; 1024];
    seen[0] = 1;
    let mut add = |cid: usize| -> Result<(), SubsetError> {
        if cid >= cid_count || cid > u16::MAX as usize {
            return Err(malformed(at));
        }
        let mask = 1u64 << (cid % 64);
        let word = &mut seen[cid / 64];
        if *word & mask != 0 {
            return Err(malformed(at));
        }
        *word |= mask;
        Ok(())
    };
    let mut p = at + 1;
    let mut remaining = glyphs.saturating_sub(1);
    while remaining > 0 {
        let first = usize::from(be_u16(d, p).ok_or(malformed(p))?);
        p += 2;
        let left = match format {
            0 => 0,
            1 => {
                let value = usize::from(*d.get(p).ok_or(malformed(p))?);
                p += 1;
                value
            }
            _ => {
                let value = usize::from(be_u16(d, p).ok_or(malformed(p))?);
                p += 2;
                value
            }
        };
        if left >= remaining || first + left > usize::from(u16::MAX) {
            return Err(malformed(p));
        }
        for cid in first..=first + left {
            add(cid)?;
        }
        remaining -= left + 1;
    }
    Ok(())
}

fn fd_select(
    d: &[u8],
    at: usize,
    glyphs: usize,
    font_dicts: usize,
    budget: &mut usize,
) -> Result<Vec<u8>, SubsetError> {
    spend(budget, glyphs, at)?;
    match *d.get(at).ok_or(malformed(at))? {
        0 => {
            let fds = slice(d, at + 1, glyphs)?;
            if fds.iter().any(|&fd| usize::from(fd) >= font_dicts) {
                return Err(malformed(at));
            }
            Ok(fds.to_vec())
        }
        3 => {
            let count = usize::from(be_u16(d, at + 1).ok_or(malformed(at + 1))?);
            if count == 0 || count > glyphs {
                return Err(malformed(at + 1));
            }
            let ranges = slice(d, at + 3, count * 3 + 2)?;
            if be_u16(ranges, 0) != Some(0)
                || be_u16(ranges, count * 3).map(usize::from) != Some(glyphs)
            {
                return Err(malformed(at));
            }
            let mut fds = vec![0; glyphs];
            for i in 0..count {
                let first = usize::from(be_u16(ranges, i * 3).ok_or(malformed(at))?);
                let end = usize::from(be_u16(ranges, (i + 1) * 3).ok_or(malformed(at))?);
                let fd = ranges[i * 3 + 2];
                if first >= end || end > glyphs || usize::from(fd) >= font_dicts {
                    return Err(malformed(at + 3 + i * 3));
                }
                fds[first..end].fill(fd);
            }
            Ok(fds)
        }
        _ => Err(err(Kind::UnsupportedFormat, at)),
    }
}

/// No references into `Font`: clones can share this immutable validated state.
#[derive(Debug)]
pub(crate) struct Directory {
    pub(super) table: Range<usize>,
    pub(super) chars: Index,
    pub(super) global: Index,
    locals: Vec<Index>,
    select: Vec<u8>,
    upem: u16,
}

impl Directory {
    pub(super) fn parse(font: &Font) -> Result<Self, SubsetError> {
        // The cache is a property of immutable font bytes. Public metric
        // fields can be edited by a caller; they must neither choose its
        // interpretation nor permanently cache an error for valid bytes.
        let metric = |tag: &[u8; 4], position: usize| -> Result<u16, SubsetError> {
            let (start, len) = find_table_full(&font.data, tag).ok_or(SubsetError::new(
                Kind::MissingTable,
                *tag,
                None,
                None,
            ))?;
            be_u16(slice(&font.data, start, len)?, position).ok_or(SubsetError::new(
                Kind::Malformed,
                *tag,
                None,
                Some(position),
            ))
        };
        let upem = metric(b"head", 18)?;
        let glyphs = usize::from(metric(b"maxp", 4)?);
        if upem == 0 || glyphs == 0 {
            return Err(malformed(0));
        }
        let (offset, length) = find_table_full(&font.data, b"CFF ").ok_or(SubsetError::new(
            Kind::UnsupportedFormat,
            *b"sfnt",
            None,
            None,
        ))?;
        let d = slice(&font.data, offset, length)?;
        if d.first() != Some(&1) {
            return Err(err(Kind::UnsupportedFormat, 0));
        }
        let header = usize::from(*d.get(2).ok_or(malformed(2))?);
        if header < 4 || !matches!(d.get(3), Some(1..=4)) {
            return Err(malformed(2));
        }
        let mut budget = DIRECTORY_WORK;
        let names = Index::parse(d, header, &mut budget)?;
        if names.count != 1 {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        let top = Index::parse(d, names.end, &mut budget)?;
        if top.count != 1 {
            return Err(malformed(names.end));
        }
        let strings = Index::parse(d, top.end, &mut budget)?;
        let global = Index::parse(d, strings.end, &mut budget)?;
        let dict = read_dict(
            top.object(d, 0).ok_or(malformed(header))?,
            &mut budget,
            header,
        )?;
        if dict.get(&0x0c05).is_some_and(|v| v.as_slice() != [0.0])
            || dict.get(&0x0c06).is_some_and(|v| v.as_slice() != [2.0])
        {
            return Err(err(Kind::UnsupportedFormat, header));
        }
        canonical_matrix(&dict, upem, header)?;
        let chars = Index::parse(d, one(&dict, 17, header)?, &mut budget)?;
        if chars.count != glyphs {
            return Err(malformed(header));
        }
        let (locals, select) = if let Some(ros) = dict.get(&0x0c1e) {
            if ros.len() != 3 {
                return Err(malformed(header));
            }
            for &sid in &ros[..2] {
                let sid = integer(sid, header)?;
                if sid >= 391 + strings.count || sid >= 65_000 {
                    return Err(malformed(header));
                }
            }
            integer(ros[2], header)?;
            if dict.get(&0x0c21).is_some_and(|v| v.as_slice() != [0.0])
                || dict.contains_key(&0x0c14)
            {
                return Err(err(Kind::UnsupportedFormat, header));
            }
            if dict.contains_key(&16) || dict.contains_key(&18) {
                return Err(malformed(header));
            }
            let cid_count = if dict.contains_key(&0x0c22) {
                one(&dict, 0x0c22, header)?
            } else {
                8720
            };
            cid_charset(
                d,
                one(&dict, 15, header)?,
                chars.count,
                cid_count,
                &mut budget,
            )?;
            let fd_at = one(&dict, 0x0c24, header)?;
            let fds = Index::parse(d, fd_at, &mut budget)?;
            if fds.count == 0 || fds.count > MAX_FONT_DICTS {
                return Err(malformed(fd_at));
            }
            let select = fd_select(
                d,
                one(&dict, 0x0c25, header)?,
                chars.count,
                fds.count,
                &mut budget,
            )?;
            let mut locals = Vec::with_capacity(fds.count);
            for i in 0..fds.count {
                let fd = read_dict(
                    fds.object(d, i).ok_or(malformed(fd_at))?,
                    &mut budget,
                    fd_at,
                )?;
                // FD matrices can change the effective coordinate system for
                // individual glyphs. This profile uses one canonical top
                // matrix; never silently discard a per-FD transformation.
                if fd.contains_key(&0x0c07) {
                    return Err(err(Kind::UnsupportedFormat, fd_at));
                }
                locals.push(private_subrs(d, &fd, true, &mut budget, fd_at)?);
            }
            (locals, select)
        } else {
            (
                vec![private_subrs(d, &dict, false, &mut budget, header)?],
                Vec::new(),
            )
        };
        Ok(Self {
            table: offset..offset + length,
            chars,
            global,
            locals,
            select,
            upem,
        })
    }

    pub(super) fn matches_metrics(&self, font: &Font) -> bool {
        self.upem == font.units_per_em && self.chars.count == usize::from(font.num_glyphs)
    }

    pub(super) fn local(&self, gid: u16) -> Option<Index> {
        let fd = if self.select.is_empty() {
            0
        } else {
            usize::from(*self.select.get(usize::from(gid))?)
        };
        self.locals.get(fd).copied()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const CID: &[u8] = include_bytes!("../../fonts/test-cid-cff/FmdCidFormat3.otf");

    #[test]
    fn selectors_validate_complete_ordered_coverage_and_every_dictionary() {
        let valid = [3, 0, 3, 0, 0, 0, 0, 2, 1, 0, 3, 0, 0, 4];
        assert_eq!(fd_select(&valid, 0, 4, 2, &mut 100).unwrap(), [0, 0, 1, 0]);
        assert_eq!(
            fd_select(&[0, 0, 0, 1, 0], 0, 4, 2, &mut 100).unwrap(),
            [0, 0, 1, 0]
        );
        for (offset, value) in [(2, 0), (4, 1), (7, 0), (10, 2), (8, 2), (13, 3), (13, 5)] {
            let mut data = valid;
            data[offset] = value;
            assert_eq!(
                fd_select(&data, 0, 4, 2, &mut 100).unwrap_err().kind,
                Kind::Malformed
            );
        }
        for data in [&valid[..13], &[0, 0, 0, 1][..], &[0, 0, 0, 1, 2][..]] {
            assert_eq!(
                fd_select(data, 0, 4, 2, &mut 100).unwrap_err().kind,
                Kind::Malformed
            );
        }
        assert_eq!(
            fd_select(&[4], 0, 4, 2, &mut 100).unwrap_err().kind,
            Kind::UnsupportedFormat
        );
    }

    #[test]
    fn cid_charsets_check_nonidentity_ranges_duplicates_and_bounds() {
        let formats = [
            vec![0, 0, 0, 0, 0, 25, 3, 232, 0, 7],
            vec![0, 0, 0, 1, 0, 25, 0, 3, 232, 0, 0, 7, 0],
            vec![0, 0, 0, 2, 0, 25, 0, 0, 3, 232, 0, 0, 0, 7, 0, 0],
        ];
        for data in &formats {
            cid_charset(data, 3, 4, 1001, &mut 100).unwrap();
        }
        for data in [
            vec![0, 0, 0, 0, 0, 0, 0, 2, 0, 3], // Duplicate implicit .notdef.
            vec![0, 0, 0, 0, 0, 7, 0, 7, 0, 3], // Duplicate explicit CID.
            vec![0, 0, 0, 1, 0, 1, 3],          // Range exceeds glyph count.
            vec![0, 0, 0, 2, 255, 255, 0, 2],   // CID range overflow.
            vec![0, 0, 0, 0, 0, 1],             // Truncated charset.
        ] {
            assert_eq!(
                cid_charset(&data, 3, 4, 1001, &mut 100).unwrap_err().kind,
                Kind::Malformed
            );
        }
        assert_eq!(
            cid_charset(&formats[0], 3, 4, 1000, &mut 100)
                .unwrap_err()
                .kind,
            Kind::Malformed
        );
        assert_eq!(
            cid_charset(&formats[0], 0, 4, 1001, &mut 100)
                .unwrap_err()
                .kind,
            Kind::Malformed
        );
    }

    #[test]
    fn indexes_refuse_malformed_offsets_and_bound_aliased_directory_work() {
        for bytes in [
            &[0, 2, 1, 1, 3, 2, 0, 0][..], // Decreasing offset.
            &[0, 1, 1, 0, 1][..],          // First offset must be one.
            &[0, 1, 1, 1, 5, 0][..],       // Data beyond table.
            &[0, 1, 4, 0, 0][..],          // Truncated offset array.
        ] {
            assert_eq!(
                Index::parse(bytes, 0, &mut 100).unwrap_err().kind,
                Kind::Malformed
            );
        }
        let bytes = [0, 2, 1, 1, 2, 3, 11, 14];
        let mut budget = 5;
        let index = Index::parse(&bytes, 0, &mut budget).unwrap();
        assert_eq!(index.object(&bytes, 0), Some(&[11][..]));
        assert_eq!(index.object(&bytes, 1), Some(&[14][..]));
        assert_eq!(index.object(&bytes, 2), None);
        assert_eq!(
            Index::parse(&bytes, 0, &mut budget).unwrap_err().kind,
            Kind::BudgetExceeded
        );
        assert_eq!(
            read_dict(&[0; 16], &mut 15, 0).unwrap_err().kind,
            Kind::BudgetExceeded
        );
    }

    fn changed_fixture(change: impl FnOnce(&mut [u8], &Dict)) -> Font {
        let mut bytes = CID.to_vec();
        let (start, length) = find_table_full(&bytes, b"CFF ").unwrap();
        let data = &mut bytes[start..start + length];
        let mut budget = DIRECTORY_WORK;
        let names = Index::parse(data, usize::from(data[2]), &mut budget).unwrap();
        let top = Index::parse(data, names.end, &mut budget).unwrap();
        let dict = dictionary(top.object(data, 0).unwrap()).unwrap();
        change(data, &dict);
        Font::parse(bytes).unwrap()
    }

    #[test]
    fn malformed_unselected_cid_structures_fail_before_any_glyph_is_emitted() {
        let bad_selector = changed_fixture(|data, top| {
            let at = one(top, 0x0c25, 0).unwrap();
            data[at + 8] = 2; // Glyph 2's FD is outside the two-entry FDArray.
        });
        let bad_private = changed_fixture(|data, top| {
            let mut budget = DIRECTORY_WORK;
            let fds = Index::parse(data, one(top, 0x0c24, 0).unwrap(), &mut budget).unwrap();
            let fd = dictionary(fds.object(data, 1).unwrap()).unwrap();
            let base = fd[&18][1] as usize;
            let private = dictionary(&data[base..base + fd[&18][0] as usize]).unwrap();
            let subrs = base + one(&private, 19, 0).unwrap();
            data[subrs..subrs + 2].copy_from_slice(&u16::MAX.to_be_bytes());
        });
        for font in [bad_selector, bad_private] {
            let error = font.cff_outline(0).unwrap_err();
            assert_eq!(error.kind, Kind::Malformed);
            assert_eq!(error.table, *b"CFF ");
            assert_eq!(font.cff_outline(0).unwrap_err(), error);
            assert_eq!(font.try_subset_glyphs(&[0], &[]).unwrap_err(), error);
            assert!(font.cff_directory.get().unwrap().is_err());
        }
    }

    #[test]
    fn per_dictionary_matrices_are_explicitly_refused() {
        let font = changed_fixture(|data, top| {
            let mut budget = DIRECTORY_WORK;
            let fds = Index::parse(data, one(top, 0x0c24, 0).unwrap(), &mut budget).unwrap();
            let start = fds.offset(data, 1).unwrap();
            // Replace FD FontName with FontMatrix; no ignored transform.
            assert_eq!(&data[start + 5..start + 7], &[12, 38]);
            data[start + 6] = 7;
        });
        assert_eq!(
            font.cff_outline(0).unwrap_err().kind,
            Kind::UnsupportedFormat
        );
    }

    #[test]
    fn parsed_directories_are_reused_by_outline_bounds_subset_and_clones() {
        let font = Font::parse_static(CID).unwrap();
        assert!(font.cff_directory.get().is_none());
        font.cff_outline(1).unwrap();
        let directory = font.cff_directory.get().unwrap().as_ref().unwrap();
        font.cff_glyph_bbox(2).unwrap();
        font.try_subset(&['A', '日']).unwrap();
        let cloned = font.clone();
        cloned.cff_outline(3).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            directory,
            cloned.cff_directory.get().unwrap().as_ref().unwrap()
        ));
        assert!(std::sync::Arc::ptr_eq(
            directory,
            font.cff_directory.get().unwrap().as_ref().unwrap()
        ));
    }

    #[test]
    fn edited_public_metrics_cannot_poison_the_source_directory_cache() {
        let mut font = Font::parse_static(CID).unwrap();
        font.units_per_em = 999;
        assert_eq!(font.cff_outline(0).unwrap_err().kind, Kind::Malformed);
        // Even this first failed call parsed only the original immutable
        // metrics. Restoring the public field, including on a clone, works.
        assert!(font.cff_directory.get().unwrap().is_ok());
        let mut cloned = font.clone();
        for face in [&mut font, &mut cloned] {
            face.units_per_em = 1000;
            assert_eq!(face.cff_glyph_bbox(2).unwrap(), Some([100, 0, 500, 500]));
            face.units_per_em = 2000;
            assert_eq!(face.cff_outline(2).unwrap_err().kind, Kind::Malformed);
            face.units_per_em = 1000;
            face.num_glyphs = 3;
            assert_eq!(face.cff_outline(2).unwrap_err().kind, Kind::Malformed);
            face.num_glyphs = 4;
            assert_eq!(face.cff_glyph_bbox(3).unwrap(), Some([0, 0, 200, 150]));
        }
    }
}
