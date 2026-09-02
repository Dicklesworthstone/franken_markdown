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

use crate::compress::{ZlibCompressScratch, zlib_compress_with_scratch};
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

#[inline(always)]
fn read_u16(buf: &[u8], off: usize) -> Result<u16> {
    let bytes = buf.get(off..off + 2).ok_or_else(|| {
        RenderError::InvalidInput("woff1: truncated sfnt (u16 read out of bounds)".into())
    })?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

#[inline(always)]
fn read_u32(buf: &[u8], off: usize) -> Result<u32> {
    let bytes = buf.get(off..off + 4).ok_or_else(|| {
        RenderError::InvalidInput("woff1: truncated sfnt (u32 read out of bounds)".into())
    })?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[inline(always)]
fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

#[inline(always)]
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
#[inline(always)]
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
#[inline(always)]
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
    let mut scratch = ZlibCompressScratch::new();
    encode_woff1_with_scratch(sfnt, &mut scratch)
}

/// [`encode_woff1`] with a caller-owned LZ77 scratch reused across every
/// table of this font. The compressor's generation-base scratch scheme
/// (see `ZlibCompressScratch`) makes a reused scratch byte-equivalent to a
/// fresh one per call, so the emitted bytes are identical to
/// [`encode_woff1`]; sharing one scratch across a whole font (or a whole
/// document's faces) only skips the per-call table regrowth.
pub(crate) fn encode_woff1_with_scratch(
    sfnt: &[u8],
    scratch: &mut ZlibCompressScratch,
) -> Result<Vec<u8>> {
    let (flavor, tables) = parse_sfnt_directory(sfnt)?;
    if tables.is_empty() {
        return Err(RenderError::InvalidInput(
            "woff1: sfnt declares zero tables".into(),
        ));
    }

    // Per the WOFF spec, a table whose compressed form is not smaller is
    // stored raw (compLength == origLength).
    let max_offset = tables.iter().map(|t| t.offset).max().unwrap_or(0);
    let mut payloads: Vec<(std::borrow::Cow<'_, [u8]>, u32)> = Vec::with_capacity(tables.len());
    for table in &tables {
        let is_last = table.offset == max_offset;
        let raw = slice_table(sfnt, *table, is_last)?;
        let compressed = zlib_compress_with_scratch(raw, scratch);
        if compressed.len() < raw.len() {
            payloads.push((std::borrow::Cow::Owned(compressed), u32_len(raw)?));
        } else {
            payloads.push((std::borrow::Cow::Borrowed(raw), u32_len(raw)?));
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

#[inline(always)]
const fn padded4(len: usize) -> usize {
    (len + 3) & !3
}

#[inline(always)]
fn usize_from_u32(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

#[inline(always)]
fn u32_len(slice: &[u8]) -> Result<u32> {
    u32::try_from(slice.len())
        .map_err(|_| RenderError::InvalidInput("woff1: table larger than 4 GiB".into()))
}

#[inline(always)]
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
            let max_offset = tables.iter().map(|t| t.offset).max().unwrap_or(0);
            let is_last = table.offset == max_offset;
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
    fn scratch_reuse_matches_fresh_encode_bytes() {
        // Isomorphism holder for the shared-scratch woff1 path (html.rs embeds
        // five faces through ONE scratch; each face compresses ~12-16 tables
        // through the same scratch). The compress.rs generation-base scheme
        // already pins fresh-vs-reused equality per zlib call
        // (`scratch_reuse_matches_fresh_zlib_bytes`); this pins it at the
        // whole-font wrap level, across alternating fonts and repeated rounds
        // so the scratch carries stale state from earlier tables and faces.
        let faces = [
            fmd_font::bundled::PLEX_REGULAR,
            fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS,
            fmd_font::bundled::CM_BOLD_ITALIC,
        ];
        let mut scratch = ZlibCompressScratch::new();
        for face in faces.iter().cycle().take(9) {
            let fresh = encode_woff1(face).expect("fresh encode");
            let reused = encode_woff1_with_scratch(face, &mut scratch).expect("reused encode");
            assert_eq!(
                reused,
                fresh,
                "reused-scratch woff1 must be byte-identical for face len {}",
                face.len()
            );
        }
    }

    #[test]
    fn woff1_clamps_overrunning_physically_last_table_not_alphabetical_last() {
        // Construct an sfnt with 2 tables:
        // Table 'zzzz': offset 44, length 10 (alphabetically last, but starts earlier)
        // Table 'aaaa': offset 54, length 20 (alphabetically first, but physically last)
        // Total buffer length: 64 (so 'aaaa' declared length 20 overruns 54..74 by 10 bytes)
        let mut sfnt = Vec::new();
        // sfnt header: flavor(4), numTables(2), searchRange(2), entrySelector(2), rangeShift(2)
        sfnt.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        sfnt.extend_from_slice(&2u16.to_be_bytes());
        sfnt.extend_from_slice(&32u16.to_be_bytes());
        sfnt.extend_from_slice(&1u16.to_be_bytes());
        sfnt.extend_from_slice(&0u16.to_be_bytes());

        // Entry 1: 'zzzz', checksum 0, offset 44, length 10
        sfnt.extend_from_slice(b"zzzz");
        sfnt.extend_from_slice(&0u32.to_be_bytes());
        sfnt.extend_from_slice(&44u32.to_be_bytes());
        sfnt.extend_from_slice(&10u32.to_be_bytes());

        // Entry 2: 'aaaa', checksum 0, offset 54, length 20 (overruns buffer length 64)
        sfnt.extend_from_slice(b"aaaa");
        sfnt.extend_from_slice(&0u32.to_be_bytes());
        sfnt.extend_from_slice(&54u32.to_be_bytes());
        sfnt.extend_from_slice(&20u32.to_be_bytes());

        // Table 'zzzz' data (10 bytes: 44..54)
        sfnt.extend_from_slice(&[0x11; 10]);
        // Table 'aaaa' data (10 bytes present: 54..64, though 20 declared)
        sfnt.extend_from_slice(&[0x22; 10]);
        assert_eq!(sfnt.len(), 64);

        let woff = encode_woff1(&sfnt).expect("must succeed by clamping overrunning last table");
        assert_eq!(&woff[0..4], b"wOFF");
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
