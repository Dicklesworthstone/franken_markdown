//! Bounded OpenType item-variation metrics for static TrueType instances.
//!
//! HVAR advances replace the original `hmtx` values plus their deltas; adding
//! them to `gvar` phantom-point advances would apply the same variation twice.
//! MVAR is reduced to small patches against the original metric tables before
//! the variable tables are removed. Outline interpolation remains in `gvar`.
//!
//! Formats: <https://learn.microsoft.com/en-us/typography/opentype/spec/otvarcommonformats>,
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/hvar>, and
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/mvar>.

use std::collections::BTreeMap;

use crate::{Font, be_i16, be_u16, be_u32, find_table_full};

// Bound the aggregate, including aliased offsets, rather than trusting each
// individually small count. Rows remain borrowed and are never expanded into
// a glyph-by-region matrix. Repeated mappings share their evaluated result.
const MAX_COLUMNS: usize = 262_144;
const MAX_WORK: usize = 8 * 1024 * 1024;

struct Budget(usize);

impl Budget {
    fn spend(&mut self, count: usize) -> Option<()> {
        self.0 = self.0.checked_sub(count)?;
        Some(())
    }
}

struct Column {
    offset: usize,
    width: usize,
    scalar: f64,
}

struct ItemData<'a> {
    rows: &'a [u8],
    count: usize,
    stride: usize,
    columns: Vec<Column>,
}

struct ItemStore<'a> {
    tables: Vec<Option<ItemData<'a>>>,
    cache: BTreeMap<(u16, u16), f64>,
}

fn range(data: &[u8], offset: usize, length: usize) -> Option<&[u8]> {
    data.get(offset..offset.checked_add(length)?)
}

fn table<'a>(font: &'a Font, tag: &[u8; 4]) -> Option<&'a [u8]> {
    let (offset, length) = find_table_full(font.raw_bytes(), tag)?;
    range(font.raw_bytes(), offset, length)
}

fn optional_table<'a>(font: &'a Font, tag: &[u8; 4]) -> Option<Option<&'a [u8]>> {
    match find_table_full(font.raw_bytes(), tag) {
        Some((offset, length)) => Some(Some(range(font.raw_bytes(), offset, length)?)),
        None => Some(None),
    }
}

fn region_scalar(data: &[u8], location: &[f32]) -> Option<f64> {
    let mut scalar = 1.0;
    for (axis, &coordinate) in location.iter().enumerate() {
        if !coordinate.is_finite() || !(-1.0..=1.0).contains(&coordinate) {
            return None;
        }
        let start = f64::from(be_i16(data, axis * 6)?) / 16384.0;
        let peak = f64::from(be_i16(data, axis * 6 + 2)?) / 16384.0;
        let end = f64::from(be_i16(data, axis * 6 + 4)?) / 16384.0;
        if !(-1.0..=1.0).contains(&start)
            || !(-1.0..=1.0).contains(&peak)
            || !(-1.0..=1.0).contains(&end)
            || start > peak
            || peak > end
            || (peak != 0.0 && start < 0.0 && end > 0.0)
        {
            return None;
        }
        // A zero peak excludes this axis from the region's scalar, including
        // when the other axes put the instance away from the default.
        if peak == 0.0 {
            continue;
        }
        let coordinate = f64::from(coordinate);
        if coordinate == peak {
            continue;
        }
        if coordinate <= start || coordinate >= end {
            scalar = 0.0;
        } else if coordinate < peak {
            scalar *= (coordinate - start) / (peak - start);
        } else {
            scalar *= (end - coordinate) / (end - peak);
        }
    }
    Some(scalar)
}

impl<'a> ItemStore<'a> {
    fn parse(data: &'a [u8], location: &[f32], budget: &mut Budget) -> Option<Self> {
        if be_u16(data, 0)? != 1
            || location.is_empty()
            || location.len() > 64
            || location.iter().any(|value| !value.is_finite() || !(-1.0..=1.0).contains(value))
        {
            return None;
        }
        let region_offset = usize::try_from(be_u32(data, 2)?).ok()?;
        let table_count = usize::from(be_u16(data, 6)?);
        let header_end = 8usize.checked_add(table_count.checked_mul(4)?)?;
        range(data, 0, header_end)?;
        if region_offset < header_end {
            return None;
        }
        let axis_count = usize::from(be_u16(data, region_offset)?);
        let region_count = usize::from(be_u16(data, region_offset.checked_add(2)?)?);
        if axis_count != location.len() || region_count >= 32_768 {
            return None;
        }
        let region_stride = axis_count.checked_mul(6)?;
        let regions = range(
            data,
            region_offset.checked_add(4)?,
            region_count.checked_mul(region_stride)?,
        )?;
        budget.spend(region_count.checked_mul(axis_count)?.checked_add(table_count)?)?;
        let mut scalars = Vec::with_capacity(region_count);
        for coordinates in regions.chunks_exact(region_stride) {
            scalars.push(region_scalar(coordinates, location)?);
        }
        let mut tables = Vec::with_capacity(table_count);
        let mut total_columns = 0usize;
        for index in 0..table_count {
            let offset = usize::try_from(be_u32(data, 8 + index * 4)?).ok()?;
            if offset == 0 {
                // A null outer-index entry is explicitly a no-variation entry;
                // its inner index is ignored by the OpenType format.
                tables.push(None);
                continue;
            }
            if offset < header_end {
                return None;
            }
            let count = usize::from(be_u16(data, offset)?);
            let packed_words = be_u16(data, offset.checked_add(2)?)?;
            let word_count = usize::from(packed_words & 0x7fff);
            let region_count = usize::from(be_u16(data, offset.checked_add(4)?)?);
            if word_count > region_count {
                return None;
            }
            total_columns = total_columns.checked_add(region_count)?;
            if total_columns > MAX_COLUMNS {
                return None;
            }
            budget.spend(region_count)?;
            let indexes = range(data, offset.checked_add(6)?, region_count.checked_mul(2)?)?;
            let short_width = if packed_words & 0x8000 != 0 { 2 } else { 1 };
            let stride = region_count.checked_add(word_count)?.checked_mul(short_width)?;
            let rows = range(
                data,
                offset.checked_add(6)?.checked_add(indexes.len())?,
                count.checked_mul(stride)?,
            )?;
            let mut columns = Vec::new();
            let mut column_offset = 0;
            for column in 0..region_count {
                let scalar = *scalars.get(usize::from(be_u16(indexes, column * 2)?))?;
                let width = if column < word_count { short_width * 2 } else { short_width };
                if scalar != 0.0 {
                    columns.push(Column { offset: column_offset, width, scalar });
                }
                column_offset += width;
            }
            tables.push(Some(ItemData { rows, count, stride, columns }));
        }
        Some(Self { tables, cache: BTreeMap::new() })
    }

    fn delta(&mut self, index: (u16, u16), budget: &mut Budget) -> Option<f64> {
        if index == (u16::MAX, u16::MAX) {
            return Some(0.0);
        }
        if let Some(&delta) = self.cache.get(&index) {
            return Some(delta);
        }
        let Some(table) = self.tables.get(usize::from(index.0))? else {
            return Some(0.0);
        };
        let row = usize::from(index.1);
        if row >= table.count {
            return None;
        }
        budget.spend(table.columns.len().checked_add(1)?)?;
        let row = range(table.rows, row.checked_mul(table.stride)?, table.stride)?;
        let mut delta = 0.0;
        for column in &table.columns {
            let value = match column.width {
                1 => i32::from(*row.get(column.offset)? as i8),
                2 => i32::from(be_i16(row, column.offset)?),
                4 => i32::from_be_bytes(range(row, column.offset, 4)?.try_into().ok()?),
                _ => return None,
            };
            delta += f64::from(value) * column.scalar;
        }
        self.cache.insert(index, delta);
        Some(delta)
    }
}

struct DeltaMap<'a> {
    entries: &'a [u8],
    width: usize,
    inner_bits: u32,
    count: usize,
}

impl<'a> DeltaMap<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        // HVAR uses format 0 (16-bit mapCount). The extended format 1 is not
        // used by HVAR; do not reinterpret its different header as format 0.
        if *data.first()? != 0 {
            return None;
        }
        let format = *data.get(1)?;
        if format & 0xc0 != 0 {
            return None;
        }
        let width = usize::from((format >> 4) & 3) + 1;
        let inner_bits = u32::from(format & 15) + 1;
        let count = usize::from(be_u16(data, 2)?);
        if count == 0 {
            return None;
        }
        Some(Self {
            entries: range(data, 4, width.checked_mul(count)?)?,
            width,
            inner_bits,
            count,
        })
    }

    fn index(&self, glyph: u16) -> Option<(u16, u16)> {
        let entry = usize::from(glyph).min(self.count.checked_sub(1)?);
        let bytes = range(self.entries, entry.checked_mul(self.width)?, self.width)?;
        let packed = bytes.iter().fold(0u32, |value, &byte| (value << 8) | u32::from(byte));
        let outer = u16::try_from(packed >> self.inner_bits).ok()?;
        let inner = u16::try_from(packed & ((1u32 << self.inner_bits) - 1)).ok()?;
        Some((outer, inner))
    }
}

struct MetricPatch {
    tag: [u8; 4],
    offset: usize,
    bytes: [u8; 2],
}

/// Fully admitted metric changes for one normalized variation location.
pub(crate) struct InstanceMetrics {
    horizontal: Option<Vec<(u16, Option<i16>)>>,
    patches: Vec<MetricPatch>,
}

impl InstanceMetrics {
    pub(crate) fn new(font: &Font, location: &[f32]) -> Option<Self> {
        let mut budget = Budget(MAX_WORK);
        let horizontal = if let Some(data) = optional_table(font, b"HVAR")? {
            Some(horizontal_metrics(font, data, location, &mut budget)?)
        } else {
            None
        };
        let patches = if let Some(data) = optional_table(font, b"MVAR")? {
            global_metrics(font, data, location, &mut budget)?
        } else {
            Vec::new()
        };
        Some(Self { horizontal, patches })
    }

    pub(crate) fn apply_horizontal(&self, metrics: &mut [(u16, i16)]) -> Option<()> {
        if let Some(horizontal) = &self.horizontal {
            if horizontal.len() != metrics.len() {
                return None;
            }
            for (metric, &(advance, side_bearing)) in metrics.iter_mut().zip(horizontal) {
                metric.0 = advance;
                if let Some(side_bearing) = side_bearing {
                    metric.1 = side_bearing;
                }
            }
        }
        Some(())
    }

    pub(crate) fn apply_table(&self, tag: &[u8; 4], bytes: &mut [u8]) -> Option<()> {
        for patch in self.patches.iter().filter(|patch| &patch.tag == tag) {
            bytes.get_mut(patch.offset..patch.offset.checked_add(2)?)?.copy_from_slice(&patch.bytes);
        }
        Some(())
    }
}

fn rounded(value: f64) -> Option<i32> {
    // OpenType integer rounding sends half-unit ties towards +infinity.
    let value = (value + 0.5).floor();
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return None;
    }
    Some(value as i32)
}

fn horizontal_metrics(
    font: &Font,
    data: &[u8],
    location: &[f32],
    budget: &mut Budget,
) -> Option<Vec<(u16, Option<i16>)>> {
    range(data, 0, 20)?;
    if be_u16(data, 0)? != 1 || be_u16(data, 2)? != 0 {
        return None;
    }
    let store_offset = usize::try_from(be_u32(data, 4)?).ok()?;
    if store_offset < 20 {
        return None;
    }
    let mut store = ItemStore::parse(data.get(store_offset..)?, location, budget)?;
    let map = |offset| -> Option<Option<DeltaMap<'_>>> {
        let start = usize::try_from(be_u32(data, offset)?).ok()?;
        if start == 0 {
            Some(None)
        } else if start < 20 {
            None
        } else {
            Some(Some(DeltaMap::parse(data.get(start..)?)?))
        }
    };
    let advances = map(8)?;
    let bearings = map(12)?;
    let right_bearings = map(16)?;
    let mut metrics = Vec::with_capacity(usize::from(font.num_glyphs));
    for glyph in 0..font.num_glyphs {
        let index = if let Some(map) = &advances { map.index(glyph)? } else { (0, glyph) };
        let advance = u16::try_from(rounded(
            f64::from(font.advance_width(glyph)) + store.delta(index, budget)?,
        )?).ok()?;
        let bearing = if let Some(map) = &bearings {
            Some(i16::try_from(rounded(
                f64::from(font.left_side_bearing(glyph)) + store.delta(map.index(glyph)?, budget)?,
            )?).ok()?)
        } else {
            None
        };
        if let Some(map) = &right_bearings {
            // Static TrueType stores advance and LSB, with RSB implied by the
            // instanced outline. Validate this optional mapping as well, without
            // applying a second, redundant bearing change to the glyph.
            store.delta(map.index(glyph)?, budget)?;
        }
        metrics.push((advance, bearing));
    }
    Some(metrics)
}

fn metric_target(tag: &[u8; 4]) -> Option<([u8; 4], usize, bool)> {
    // The final flag denotes an unsigned field; all other values are FWORDs.
    let (table, offset, unsigned) = match tag {
        b"hasc" => (*b"OS/2", 68, false),
        b"hdsc" => (*b"OS/2", 70, false),
        b"hlgp" => (*b"OS/2", 72, false),
        b"hcla" => (*b"OS/2", 74, true),
        b"hcld" => (*b"OS/2", 76, true),
        b"xhgt" => (*b"OS/2", 86, false),
        b"cpht" => (*b"OS/2", 88, false),
        b"sbxs" => (*b"OS/2", 10, false),
        b"sbys" => (*b"OS/2", 12, false),
        b"sbxo" => (*b"OS/2", 14, false),
        b"sbyo" => (*b"OS/2", 16, false),
        b"spxs" => (*b"OS/2", 18, false),
        b"spys" => (*b"OS/2", 20, false),
        b"spxo" => (*b"OS/2", 22, false),
        b"spyo" => (*b"OS/2", 24, false),
        b"strs" => (*b"OS/2", 26, false),
        b"stro" => (*b"OS/2", 28, false),
        b"hcrs" => (*b"hhea", 18, false),
        b"hcrn" => (*b"hhea", 20, false),
        b"hcof" => (*b"hhea", 22, false),
        b"vasc" => (*b"vhea", 4, false),
        b"vdsc" => (*b"vhea", 6, false),
        b"vlgp" => (*b"vhea", 8, false),
        b"vcrs" => (*b"vhea", 18, false),
        b"vcrn" => (*b"vhea", 20, false),
        b"vcof" => (*b"vhea", 22, false),
        b"undo" => (*b"post", 8, false),
        b"unds" => (*b"post", 10, false),
        [b'g', b's', b'p', digit @ b'0'..=b'9'] => (*b"gasp", 4 + usize::from(*digit - b'0') * 4, true),
        _ => return None,
    };
    Some((table, offset, unsigned))
}

fn global_metrics(
    font: &Font,
    data: &[u8],
    location: &[f32],
    budget: &mut Budget,
) -> Option<Vec<MetricPatch>> {
    if be_u16(data, 0)? != 1 || be_u16(data, 4)? != 0 {
        return None;
    }
    let size = usize::from(be_u16(data, 6)?);
    let count = usize::from(be_u16(data, 8)?);
    let store_offset = usize::from(be_u16(data, 10)?);
    if size == 0 || (count > 0 && size < 8) {
        return None;
    }
    let records = range(data, 12, size.checked_mul(count)?)?;
    if count == 0 {
        return (store_offset == 0).then(Vec::new);
    }
    if store_offset < 12usize.checked_add(records.len())? {
        return None;
    }
    budget.spend(count)?;
    let mut store = ItemStore::parse(data.get(store_offset..)?, location, budget)?;
    let sync_horizontal = table(font, b"OS/2").zip(table(font, b"hhea")).is_some_and(|(os2, hhea)| {
        [0, 2, 4].iter().all(|&offset| {
            be_i16(os2, 68 + offset).zip(be_i16(hhea, 4 + offset)).is_some_and(|(a, b)| a == b)
        })
    });
    let mut patches = Vec::new();
    let mut previous = None;
    for record in records.chunks_exact(size) {
        let tag: [u8; 4] = range(record, 0, 4)?.try_into().ok()?;
        if previous.is_some_and(|old| old >= tag) {
            return None;
        }
        previous = Some(tag);
        let Some((target, offset, unsigned)) = metric_target(&tag) else {
            // Unknown/private tags have no specified target in this engine.
            continue;
        };
        let original = table(font, &target)?;
        if &target == b"gasp" && (offset - 4) / 4 + 1 >= usize::from(be_u16(original, 2)?) {
            return None;
        }
        if matches!(&tag, b"xhgt" | b"cpht") && be_u16(original, 0)? < 2 {
            return None;
        }
        let base = if unsigned { i32::from(be_u16(original, offset)?) } else { i32::from(be_i16(original, offset)?) };
        let delta = store.delta((be_u16(record, 4)?, be_u16(record, 6)?), budget)?;
        let value = rounded(f64::from(base) + delta)?;
        let bytes = if unsigned { u16::try_from(value).ok()?.to_be_bytes() } else { i16::try_from(value).ok()?.to_be_bytes() };
        patches.push(MetricPatch { tag: target, offset, bytes });
        // Keep initially equal horizontal and typographic metrics synchronized,
        // as static FontTools instances do. Distinct hhea metrics remain intact.
        if sync_horizontal && matches!(&tag, b"hasc" | b"hdsc" | b"hlgp") {
            patches.push(MetricPatch { tag: *b"hhea", offset: offset - 64, bytes });
        }
    }
    Some(patches)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn store_bytes(regions: &[Vec<[i16; 3]>], rows: &[Vec<i32>], words: u16) -> Vec<u8> {
        let axes = regions[0].len();
        let data_offset = 12 + 4 + regions.len() * axes * 6;
        let mut out = Vec::new();
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&12u32.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&(data_offset as u32).to_be_bytes());
        out.extend_from_slice(&(axes as u16).to_be_bytes());
        out.extend_from_slice(&(regions.len() as u16).to_be_bytes());
        for region in regions {
            for axis in region {
                for coordinate in axis {
                    out.extend_from_slice(&coordinate.to_be_bytes());
                }
            }
        }
        out.extend_from_slice(&(rows.len() as u16).to_be_bytes());
        out.extend_from_slice(&words.to_be_bytes());
        out.extend_from_slice(&(regions.len() as u16).to_be_bytes());
        for region in 0..regions.len() {
            out.extend_from_slice(&(region as u16).to_be_bytes());
        }
        let short = if words & 0x8000 != 0 { 2 } else { 1 };
        for row in rows {
            assert_eq!(row.len(), regions.len());
            for (column, value) in row.iter().enumerate() {
                let width = if column < usize::from(words & 0x7fff) { short * 2 } else { short };
                match width {
                    1 => out.extend_from_slice(&i8::try_from(*value).unwrap().to_be_bytes()),
                    2 => out.extend_from_slice(&i16::try_from(*value).unwrap().to_be_bytes()),
                    4 => out.extend_from_slice(&value.to_be_bytes()),
                    _ => unreachable!(),
                }
            }
        }
        out
    }

    fn mixed_store() -> Vec<u8> {
        store_bytes(
            &[
                vec![[0, 16384, 16384], [0, 0, 0]],
                vec![[4096, 8192, 12288], [0, 0, 0]],
                vec![[-16384, 0, 16384], [0, 16384, 16384]],
            ],
            &[vec![300, -5, 12], vec![-300, 127, -128]],
            1,
        )
    }

    #[test]
    fn mixed_signed_rows_interpolate_multiple_axes_and_intermediate_regions() {
        let bytes = mixed_store();
        let mut budget = Budget(MAX_WORK);
        let mut store = ItemStore::parse(&bytes, &[0.5, 0.25], &mut budget).unwrap();
        assert_eq!(store.delta((0, 0), &mut budget), Some(148.0));
        assert_eq!(store.delta((0, 1), &mut budget), Some(-55.0));
        assert!(store.delta((0, 2), &mut budget).is_none());
        assert!(store.delta((1, 0), &mut budget).is_none());
        assert_eq!(store.delta((u16::MAX, u16::MAX), &mut budget), Some(0.0));
    }

    #[test]
    fn negative_regions_and_peak_boundaries_have_exact_scalars() {
        let bytes = store_bytes(&[vec![[-16384, -16384, 0]]], &[vec![101]], 0);
        for (coordinate, expected) in [(-1.0, 101.0), (-0.5, 50.5), (0.0, 0.0), (1.0, 0.0)] {
            let mut budget = Budget(MAX_WORK);
            let mut store = ItemStore::parse(&bytes, &[coordinate], &mut budget).unwrap();
            assert_eq!(store.delta((0, 0), &mut budget), Some(expected));
        }
    }

    #[test]
    fn shared_store_supports_long_words_without_signed_truncation() {
        let bytes = store_bytes(
            &[vec![[0, 16384, 16384]], vec![[4096, 8192, 12288]]],
            &[vec![70_000, -200]],
            0x8001,
        );
        let mut budget = Budget(MAX_WORK);
        let mut store = ItemStore::parse(&bytes, &[0.5], &mut budget).unwrap();
        assert_eq!(store.delta((0, 0), &mut budget), Some(34_800.0));
    }

    #[test]
    fn absent_outer_subtables_and_no_variation_sentinel_are_zero() {
        let mut bytes = store_bytes(&[vec![[0, 16384, 16384]]], &[vec![12]], 0);
        let old_data = be_u32(&bytes, 8).unwrap();
        bytes.splice(8..8, 0u32.to_be_bytes());
        bytes[2..6].copy_from_slice(&16u32.to_be_bytes());
        bytes[6..8].copy_from_slice(&2u16.to_be_bytes());
        bytes[12..16].copy_from_slice(&(old_data + 4).to_be_bytes());
        let mut budget = Budget(MAX_WORK);
        let mut store = ItemStore::parse(&bytes, &[0.5], &mut budget).unwrap();
        assert_eq!(store.delta((0, 65_000), &mut budget), Some(0.0));
        assert_eq!(store.delta((1, 0), &mut budget), Some(6.0));
    }

    #[test]
    fn index_maps_decode_widths_outer_indices_and_repeated_last_entries() {
        for width in 1..=4 {
            let format = (((width - 1) as u8) << 4) | 1;
            let mut bytes = vec![0, format, 0, 2];
            for index in [0u32, 13] {
                bytes.extend_from_slice(&index.to_be_bytes()[4 - width..]);
            }
            let map = DeltaMap::parse(&bytes).unwrap();
            assert_eq!(map.index(0), Some((0, 0)));
            assert_eq!(map.index(1), Some((3, 1)));
            assert_eq!(map.index(u16::MAX), Some((3, 1)));
        }
        let map = DeltaMap::parse(&[0, 0x3f, 0, 1, 255, 255, 255, 255]).unwrap();
        assert_eq!(map.index(0), Some((u16::MAX, u16::MAX)));
        let map = DeltaMap::parse(&[0, 0x0f, 0, 1, 42]).unwrap();
        assert_eq!(map.index(0), Some((0, 42)), "narrow entries may zero-extend their inner index");
    }

    #[test]
    fn mapped_delta_cache_prevents_repeated_region_evaluation() {
        let bytes = mixed_store();
        let mut store = ItemStore::parse(&bytes, &[0.5, 0.25], &mut Budget(MAX_WORK)).unwrap();
        let mut budget = Budget(4);
        assert_eq!(store.delta((0, 0), &mut budget), Some(148.0));
        assert_eq!(budget.0, 0);
        for _ in 0..1000 {
            assert_eq!(store.delta((0, 0), &mut budget), Some(148.0));
        }
        assert!(store.delta((0, 1), &mut budget).is_none());
    }

    #[test]
    fn truncation_counts_indices_and_work_budget_are_checked_before_use() {
        let bytes = mixed_store();
        for length in 0..bytes.len() {
            assert!(ItemStore::parse(&bytes[..length], &[0.5, 0.25], &mut Budget(MAX_WORK)).is_none());
        }
        assert!(ItemStore::parse(&bytes, &[0.5, 0.25], &mut Budget(1)).is_none());
        for location in [vec![], vec![0.5], vec![f32::NAN, 0.25], vec![1.01, 0.0]] {
            assert!(ItemStore::parse(&bytes, &location, &mut Budget(MAX_WORK)).is_none());
        }
        let item = be_u32(&bytes, 8).unwrap() as usize;
        for (offset, value) in [(item, u16::MAX), (item + 2, 4), (item + 6, 3), (12, 3), (14, 0x8000)] {
            let mut invalid = bytes.clone();
            invalid[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
            assert!(ItemStore::parse(&invalid, &[0.5, 0.25], &mut Budget(MAX_WORK)).is_none());
        }
    }

    #[test]
    fn aliased_subtables_cannot_expand_unbounded_column_storage() {
        let copies = MAX_COLUMNS / 1024 + 1;
        let region_offset = 8 + copies * 4;
        let item_offset = region_offset + 10;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&(region_offset as u32).to_be_bytes());
        bytes.extend_from_slice(&(copies as u16).to_be_bytes());
        for _ in 0..copies { bytes.extend_from_slice(&(item_offset as u32).to_be_bytes()); }
        for value in [1u16, 1, 0, 16384, 16384, 0, 0, 1024] { bytes.extend_from_slice(&value.to_be_bytes()); }
        bytes.resize(bytes.len() + 2048, 0);
        assert!(ItemStore::parse(&bytes, &[1.0], &mut Budget(MAX_WORK)).is_none());
    }

    #[test]
    fn integer_rounding_preserves_negative_half_ties_and_refuses_overflow() {
        assert_eq!(rounded(67.5), Some(68));
        assert_eq!(rounded(-22.5), Some(-22));
        assert_eq!(rounded(-22.5001), Some(-23));
        assert!(rounded(f64::INFINITY).is_none());
        assert!(rounded(f64::NAN).is_none());
        assert!(rounded(f64::from(i32::MAX) + 1.0).is_none());
    }
}
