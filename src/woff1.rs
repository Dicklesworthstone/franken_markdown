//! Clean-room WOFF1 encoder for TrueType sfnt fonts (bead ge1t).
//!
//! WOFF1 (W3C File Format 1.0) wraps an sfnt font in a fixed 44-byte header
//! plus a per-table directory, compressing each table with zlib (RFC 1950).
//! The compression here is the renderer's own hand-rolled DEFLATE
//! ([`crate::compress::zlib_compress`]) — the same deterministic compressor the
//! PDF stream path and the determinism gate already exercise — so HTML font
//! embedding gains a real size win without any new dependency or algorithm.
//!
//! WOFF2 (Brotli-based) is deliberately out of scope: a clean-room Brotli
//! encoder is multi-session work, and on the small per-document subsets this
//! renderer emits, WOFF1 already captures the bulk of the win. Every modern
//! browser decodes WOFF1.
//!
//! Determinism: table order is sorted by tag (spec-required), compression is
//! the deterministic zlib path, and every computed field derives only from the
//! input bytes. Same input ⇒ identical output bytes.

use crate::compress::zlib_compress;
use crate::{RenderError, Result};

const WOFF_SIGNATURE: u32 = 0x774F_4646; // "wOFF"
const WOFF_HEADER_LEN: usize = 44;
const WOFF_DIR_ENTRY_LEN: usize = 20;
const SFNT_DIR_ENTRY_LEN: usize = 16;
const SFNT_HEADER_LEN: usize = 12;

#[derive(Debug, Clone, Copy)]
struct SfntTable {
    tag: [u8; 4],
    checksum: u32,
    offset: u32,
    length: u32,
}

fn read_u16(buf: &[u8], off: usize) -> Result<u16> {
    let bytes = buf.get(off..off + 2).ok_or_else(|| {
        RenderError::InvalidInput("woff1: truncated sfnt (u16 read out of bounds)".into())
    })?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32> {
    let bytes = buf.get(off..off + 4).ok_or_else(|| {
        RenderError::InvalidInput("woff1: truncated sfnt (u32 read out of bounds)".into())
    })?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// Parse the sfnt table directory: tag(4) + checksum(4) + offset(4) +
/// length(4) per entry. Real-world fonts sometimes declare a last table whose
/// `offset + length` overruns the file (directory written before finalization);
/// clamp that final table to fit so real fonts still encode.
fn parse_sfnt_directory(sfnt: &[u8]) -> Result<(u32, Vec<SfntTable>)> {
    if sfnt.len() < SFNT_HEADER_LEN {
        return Err(RenderError::InvalidInput(
            "woff1: input smaller than an sfnt header".into(),
        ));
    }
    let flavor = read_u32(sfnt, 0)?;
    let num_tables = usize::from(read_u16(sfnt, 4)?);
    let dir_end = SFNT_HEADER_LEN + num_tables * SFNT_DIR_ENTRY_LEN;
    if sfnt.len() < dir_end {
        return Err(RenderError::InvalidInput(
            "woff1: sfnt directory overruns input".into(),
        ));
    }
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let off = SFNT_HEADER_LEN + i * SFNT_DIR_ENTRY_LEN;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&sfnt[off..off + 4]);
        tables.push(SfntTable {
            tag,
            checksum: read_u32(sfnt, off + 4)?,
            offset: read_u32(sfnt, off + 8)?,
            length: read_u32(sfnt, off + 12)?,
        });
    }
    tables.sort_by_key(|t| t.tag);
    Ok((flavor, tables))
}

/// Slice one table's bytes, clamping an over-long final table to the file end.
fn slice_table(sfnt: &[u8], table: SfntTable, is_last: bool) -> Result<&[u8]> {
    let start = usize::try_from(table.offset)
        .map_err(|_| RenderError::InvalidInput("woff1: table offset does not fit usize".into()))?;
    let declared_len = usize::try_from(table.length)
        .map_err(|_| RenderError::InvalidInput("woff1: table length does not fit usize".into()))?;
    let available = sfnt.len().saturating_sub(start);
    let len = if is_last {
        declared_len.min(available)
    } else {
        declared_len
    };
    sfnt.get(start..start + len).ok_or_else(|| {
        RenderError::InvalidInput(format!(
            "woff1: table {:?} range {}..{} out of bounds",
            table.tag,
            start,
            start + len
        ))
    })
}

/// Read `head.fontRevision` (fixed 16.16) for the WOFF version fields.
/// Absent or malformed head falls back to 1.0 — cosmetic metadata only.
fn font_revision(sfnt: &[u8], tables: &[SfntTable]) -> (u16, u16) {
    let Some(head) = tables.iter().find(|t| &t.tag == b"head") else {
        return (1, 0);
    };
    let start = head.offset as usize;
    let Ok(revision) = read_u32(sfnt, start + 4) else {
        return (1, 0);
    };
    ((revision >> 16) as u16, revision as u16)
}

/// Encode sfnt font bytes as a WOFF1 font. Output is deterministic for a
/// given input and decompresses to byte-identical table data.
///
/// # Errors
/// Returns [`RenderError::InvalidInput`] when the input is not a parseable
/// sfnt directory (truncated, zero tables, or out-of-bounds table ranges).
pub fn encode_woff1(sfnt: &[u8]) -> Result<Vec<u8>> {
    let (flavor, tables) = parse_sfnt_directory(sfnt)?;
    if tables.is_empty() {
        return Err(RenderError::InvalidInput(
            "woff1: sfnt declares zero tables".into(),
        ));
    }

    // Per the WOFF spec, a table whose compressed form is not smaller is
    // stored raw (compLength == origLength).
    let last_idx = tables.len() - 1;
    let mut payloads: Vec<(Vec<u8>, u32)> = Vec::with_capacity(tables.len());
    for (idx, table) in tables.iter().enumerate() {
        let raw = slice_table(sfnt, *table, idx == last_idx)?;
        let compressed = zlib_compress(raw);
        if compressed.len() < raw.len() {
            payloads.push((compressed, u32_len(raw)?));
        } else {
            payloads.push((raw.to_vec(), u32_len(raw)?));
        }
    }

    let num_tables = tables.len();
    let dir_len = num_tables * WOFF_DIR_ENTRY_LEN;
    let data_start = WOFF_HEADER_LEN + dir_len;
    let total_sfnt_size = (SFNT_HEADER_LEN + num_tables * SFNT_DIR_ENTRY_LEN)
        + tables
            .iter()
            .map(|t| padded4(usize_from_u32(t.length)))
            .sum::<usize>();
    let total_len = data_start
        + payloads
            .iter()
            .map(|(bytes, _)| padded4(bytes.len()))
            .sum::<usize>();
    let (major, minor) = font_revision(sfnt, &tables);

    let mut out = Vec::with_capacity(total_len);
    push_u32(&mut out, WOFF_SIGNATURE);
    push_u32(&mut out, flavor);
    push_u32(&mut out, u32_from_usize(total_len)?);
    push_u16(&mut out, num_tables as u16);
    push_u16(&mut out, 0); // reserved
    push_u32(&mut out, u32_from_usize(total_sfnt_size)?);
    push_u16(&mut out, major);
    push_u16(&mut out, minor);
    push_u32(&mut out, 0); // metaOffset
    push_u32(&mut out, 0); // metaLength
    push_u32(&mut out, 0); // metaOrigLength
    push_u32(&mut out, 0); // privOffset
    push_u32(&mut out, 0); // privLength

    let mut offset = data_start;
    for (table, (payload, orig_len)) in tables.iter().zip(payloads.iter()) {
        out.extend_from_slice(&table.tag);
        push_u32(&mut out, u32_from_usize(offset)?);
        push_u32(&mut out, u32_from_usize(payload.len())?);
        push_u32(&mut out, *orig_len);
        push_u32(&mut out, table.checksum);
        offset += padded4(payload.len());
    }
    for (payload, _) in &payloads {
        out.extend_from_slice(payload);
        let pad = padded4(payload.len()) - payload.len();
        out.extend_from_slice(&[0u8; 3][..pad]);
    }

    debug_assert_eq!(out.len(), total_len);
    Ok(out)
}

fn padded4(len: usize) -> usize {
    (len + 3) & !3
}

fn usize_from_u32(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

fn u32_len(slice: &[u8]) -> Result<u32> {
    u32::try_from(slice.len())
        .map_err(|_| RenderError::InvalidInput("woff1: table larger than 4 GiB".into()))
}

fn u32_from_usize(v: usize) -> Result<u32> {
    u32::try_from(v)
        .map_err(|_| RenderError::InvalidInput("woff1: output larger than 4 GiB".into()))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn parse_woff_dir(woff: &[u8]) -> Vec<([u8; 4], u32, u32, u32, u32)> {
        let num = u16::from_be_bytes([woff[12], woff[13]]) as usize;
        let mut out = Vec::new();
        for i in 0..num {
            let off = WOFF_HEADER_LEN + i * WOFF_DIR_ENTRY_LEN;
            let mut tag = [0u8; 4];
            tag.copy_from_slice(&woff[off..off + 4]);
            let read =
                |o: usize| u32::from_be_bytes([woff[o], woff[o + 1], woff[o + 2], woff[o + 3]]);
            out.push((
                tag,
                read(off + 4),
                read(off + 8),
                read(off + 12),
                read(off + 16),
            ));
        }
        out
    }

    #[test]
    fn woff1_round_trips_every_table_byte_for_byte() {
        let sfnt = fmd_font::bundled::PLEX_REGULAR;
        let woff = encode_woff1(sfnt).expect("encode");
        assert!(woff.len() < sfnt.len(), "woff1 must beat raw ttf");
        assert_eq!(&woff[0..4], b"wOFF");

        let (_, tables) = parse_sfnt_directory(sfnt).expect("sfnt dir");
        let dir = parse_woff_dir(&woff);
        assert_eq!(dir.len(), tables.len(), "same table count");
        for (entry, table) in dir.iter().zip(tables.iter()) {
            let (tag, offset, comp_len, orig_len, checksum) = *entry;
            assert_eq!(&tag, &table.tag, "directory stays sorted by tag");
            assert_eq!(checksum, table.checksum, "checksum preserved");
            let payload = &woff[offset as usize..offset as usize + comp_len as usize];
            let decoded = if comp_len == orig_len {
                payload.to_vec()
            } else {
                crate::compress::zlib_decompress(payload, orig_len as usize + 64)
                    .expect("valid zlib stream")
            };
            let is_last = std::ptr::eq(table, tables.last().unwrap());
            let expected = slice_table(sfnt, *table, is_last).expect("slice");
            assert_eq!(decoded, expected, "table {:?} round-trips", table.tag);
        }
    }

    #[test]
    fn woff1_is_deterministic() {
        let sfnt = fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS;
        let a = encode_woff1(sfnt).expect("encode a");
        let b = encode_woff1(sfnt).expect("encode b");
        assert_eq!(a, b);
    }

    #[test]
    fn woff1_rejects_garbage() {
        assert!(encode_woff1(b"").is_err());
        assert!(encode_woff1(&[0u8; 8]).is_err());
        // Header says 2 tables but directory is absent.
        let mut bogus = vec![0u8; 12];
        bogus[5] = 2;
        assert!(encode_woff1(&bogus).is_err());
    }
}
