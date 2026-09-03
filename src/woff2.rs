//! WOFF2 (Web Open Font Format 2.0) encoder and decoder.
//!
//! Clean-room, zero-dependency WOFF2 implementation conforming to the W3C
//! WOFF File Format 2.0 specification.
//!
//! Uses the clean-room Brotli engine in [`crate::brotli`] to compress concatenated
//! font table payloads, achieving superior compression over WOFF1 and uncompressed TTF/OTF.

use crate::brotli::{brotli_compress_with_scratch, brotli_decompress, BrotliCompressScratch};
use crate::{RenderError, Result};

pub const WOFF2_SIGNATURE: u32 = 0x774F_4632; // "wOF2"
const WOFF2_HEADER_LEN: usize = 48;
const SFNT_HEADER_LEN: usize = 12;
const SFNT_DIR_ENTRY_LEN: usize = 16;

const KNOWN_TABLE_TAGS: [&[u8; 4]; 62] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ", b"fpgm",
    b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp", b"hdmx", b"kern",
    b"LTSH", b"PCLT", b"VDMX", b"vhea", b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"JSTF",
    b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt", b"avar", b"bdat",
    b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar", b"gvar", b"hsty", b"just",
    b"lcar", b"mort", b"morx", b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat", b"Gloc",
    b"Feat", b"Sill",
];

#[derive(Clone, Copy, Debug)]
struct SfntTable {
    tag: [u8; 4],
    _checksum: u32,
    offset: u32,
    length: u32,
}

#[inline(always)]
fn read_u16(buf: &[u8], offset: usize) -> Result<u16> {
    buf.get(offset..offset + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or_else(|| RenderError::InvalidInput("woff2: unexpected EOF reading u16".into()))
}

#[inline(always)]
fn read_u32(buf: &[u8], offset: usize) -> Result<u32> {
    buf.get(offset..offset + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| RenderError::InvalidInput("woff2: unexpected EOF reading u32".into()))
}

fn parse_sfnt_directory(sfnt: &[u8]) -> Result<(u32, Vec<SfntTable>)> {
    if sfnt.len() < SFNT_HEADER_LEN {
        return Err(RenderError::InvalidInput(
            "woff2: input smaller than an sfnt header".into(),
        ));
    }
    let flavor = read_u32(sfnt, 0)?;
    let num_tables = usize::from(read_u16(sfnt, 4)?);
    let dir_end = SFNT_HEADER_LEN + num_tables * SFNT_DIR_ENTRY_LEN;
    if sfnt.len() < dir_end {
        return Err(RenderError::InvalidInput(
            "woff2: sfnt directory overruns input".into(),
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

#[inline(always)]
fn slice_table(sfnt: &[u8], table: SfntTable, is_last: bool) -> Result<&[u8]> {
    let start = usize::try_from(table.offset)
        .map_err(|_| RenderError::InvalidInput("woff2: table offset does not fit usize".into()))?;
    let declared_len = usize::try_from(table.length)
        .map_err(|_| RenderError::InvalidInput("woff2: table length does not fit usize".into()))?;
    let available = sfnt.len().saturating_sub(start);
    let len = if is_last {
        declared_len.min(available)
    } else {
        declared_len
    };
    sfnt.get(start..start + len).ok_or_else(|| {
        RenderError::InvalidInput(format!(
            "woff2: table {:?} range {}..{} out of bounds",
            table.tag,
            start,
            start + len
        ))
    })
}

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

fn encode_base128(mut value: usize, out: &mut Vec<u8>) {
    if value == 0 {
        out.push(0);
        return;
    }
    let mut buf = [0u8; 5];
    let mut len = 0;
    while value > 0 {
        buf[len] = (value & 0x7F) as u8;
        value >>= 7;
        len += 1;
    }
    for i in (1..len).rev() {
        out.push(buf[i] | 0x80);
    }
    out.push(buf[0]);
}

fn decode_base128(data: &[u8], pos: &mut usize) -> Result<u32> {
    let mut result = 0u32;
    for _ in 0..5 {
        let b = *data.get(*pos).ok_or_else(|| {
            RenderError::InvalidInput("woff2: unexpected end of Base128 integer".into())
        })?;
        *pos += 1;
        if result & 0xFE00_0000 != 0 {
            return Err(RenderError::InvalidInput("woff2: Base128 overflow".into()));
        }
        result = (result << 7) | u32::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(RenderError::InvalidInput(
        "woff2: Base128 sequence exceeds 5 bytes".into(),
    ))
}

fn calc_checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    let mut chunks = data.chunks_exact(4);
    for c in &mut chunks {
        sum = sum.wrapping_add(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let mut padded = [0u8; 4];
        padded[..rem.len()].copy_from_slice(rem);
        sum = sum.wrapping_add(u32::from_be_bytes(padded));
    }
    sum
}

/// Encode raw sfnt (TTF or CFF/OTF) font bytes into a WOFF2 container.
pub fn encode_woff2(sfnt: &[u8]) -> Result<Vec<u8>> {
    let mut scratch = BrotliCompressScratch::new();
    encode_woff2_with_scratch(sfnt, &mut scratch)
}

/// Encode raw sfnt font bytes into a WOFF2 container using caller-provided scratch.
pub fn encode_woff2_with_scratch(
    sfnt: &[u8],
    scratch: &mut BrotliCompressScratch,
) -> Result<Vec<u8>> {
    let (flavor, tables) = parse_sfnt_directory(sfnt)?;
    if tables.is_empty() {
        return Err(RenderError::InvalidInput(
            "woff2: font has zero tables".into(),
        ));
    }
    let num_tables = u16::try_from(tables.len())
        .map_err(|_| RenderError::InvalidInput("woff2: too many tables".into()))?;

    let (major_version, minor_version) = font_revision(sfnt, &tables);

    // Compute total sfnt uncompressed size and collect uncompressed table stream
    let mut total_sfnt_size = SFNT_HEADER_LEN + usize::from(num_tables) * SFNT_DIR_ENTRY_LEN;
    let mut uncompressed_stream = Vec::new();
    let mut table_dir_bytes = Vec::new();

    let num_t = tables.len();
    for (i, &table) in tables.iter().enumerate() {
        let data = slice_table(sfnt, table, i + 1 == num_t)?;
        let orig_len = data.len();
        total_sfnt_size += (orig_len + 3) & !3;

        // Match against known table tags
        let known_idx = KNOWN_TABLE_TAGS
            .iter()
            .position(|&&tag| tag == table.tag);

        let (flags, custom_tag) = match known_idx {
            Some(idx) => {
                let transform_bits = if &table.tag == b"glyf" || &table.tag == b"loca" {
                    0xC0 // transform version 3 = untransformed
                } else {
                    0x00 // transform version 0 = untransformed
                };
                ((idx as u8) | transform_bits, None)
            }
            None => (63u8, Some(table.tag)),
        };

        table_dir_bytes.push(flags);
        if let Some(tag) = custom_tag {
            table_dir_bytes.extend_from_slice(&tag);
        }
        encode_base128(orig_len, &mut table_dir_bytes);

        uncompressed_stream.extend_from_slice(data);
    }

    let compressed_stream = brotli_compress_with_scratch(&uncompressed_stream, scratch);

    let total_sfnt_size_u32 = u32::try_from(total_sfnt_size)
        .map_err(|_| RenderError::InvalidInput("woff2: sfnt size overflow".into()))?;
    let compressed_len_u32 = u32::try_from(compressed_stream.len())
        .map_err(|_| RenderError::InvalidInput("woff2: compressed stream overflow".into()))?;

    let total_file_len = WOFF2_HEADER_LEN + table_dir_bytes.len() + compressed_stream.len();
    let total_file_len_u32 = u32::try_from(total_file_len)
        .map_err(|_| RenderError::InvalidInput("woff2: total file length overflow".into()))?;

    let mut out = Vec::with_capacity(total_file_len);

    // 48-byte WOFF2 Header
    out.extend_from_slice(&WOFF2_SIGNATURE.to_be_bytes()); // 0..4
    out.extend_from_slice(&flavor.to_be_bytes()); // 4..8
    out.extend_from_slice(&total_file_len_u32.to_be_bytes()); // 8..12
    out.extend_from_slice(&num_tables.to_be_bytes()); // 12..14
    out.extend_from_slice(&0u16.to_be_bytes()); // 14..16 (reserved)
    out.extend_from_slice(&total_sfnt_size_u32.to_be_bytes()); // 16..20
    out.extend_from_slice(&compressed_len_u32.to_be_bytes()); // 20..24
    out.extend_from_slice(&major_version.to_be_bytes()); // 24..26
    out.extend_from_slice(&minor_version.to_be_bytes()); // 26..28
    out.extend_from_slice(&0u32.to_be_bytes()); // 28..32 metaOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // 32..36 metaLength
    out.extend_from_slice(&0u32.to_be_bytes()); // 36..40 metaOrigLength
    out.extend_from_slice(&0u32.to_be_bytes()); // 40..44 privOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // 44..48 privLength

    // Table Directory
    out.extend_from_slice(&table_dir_bytes);

    // Compressed Table Data Stream
    out.extend_from_slice(&compressed_stream);

    Ok(out)
}

/// Decode a WOFF2 container back into reconstructed sfnt font bytes.
pub fn decode_woff2(woff2: &[u8]) -> Result<Vec<u8>> {
    if woff2.len() < WOFF2_HEADER_LEN {
        return Err(RenderError::InvalidInput(
            "woff2: input smaller than header".into(),
        ));
    }
    let signature = read_u32(woff2, 0)?;
    if signature != WOFF2_SIGNATURE {
        return Err(RenderError::InvalidInput(
            "woff2: invalid signature".into(),
        ));
    }
    let flavor = read_u32(woff2, 4)?;
    let length = read_u32(woff2, 8)? as usize;
    if woff2.len() < length {
        return Err(RenderError::InvalidInput(
            "woff2: file length exceeds input".into(),
        ));
    }
    let num_tables = usize::from(read_u16(woff2, 12)?);
    let total_sfnt_size = read_u32(woff2, 16)? as usize;
    let total_compressed_size = read_u32(woff2, 20)? as usize;

    let mut pos = WOFF2_HEADER_LEN;
    struct Woff2TableEntry {
        tag: [u8; 4],
        orig_length: usize,
    }

    let mut entries = Vec::with_capacity(num_tables);
    for _ in 0..num_tables {
        let flags = *woff2.get(pos).ok_or_else(|| {
            RenderError::InvalidInput("woff2: unexpected EOF in table directory".into())
        })?;
        pos += 1;
        let tag_idx = (flags & 0x3F) as usize;
        let tag = if tag_idx == 63 {
            let s = woff2.get(pos..pos + 4).ok_or_else(|| {
                RenderError::InvalidInput("woff2: unexpected EOF reading custom tag".into())
            })?;
            pos += 4;
            [s[0], s[1], s[2], s[3]]
        } else {
            *KNOWN_TABLE_TAGS.get(tag_idx).copied().ok_or_else(|| {
                RenderError::InvalidInput("woff2: unknown tag index".into())
            })?
        };

        let orig_len = decode_base128(woff2, &mut pos)? as usize;
        let transform_version = flags >> 6;
        if (&tag != b"glyf" && &tag != b"loca" && transform_version != 0)
            || ((&tag == b"glyf" || &tag == b"loca") && transform_version != 3)
        {
            // Transformed tables declare transformLength
            let _transform_len = decode_base128(woff2, &mut pos)?;
        }
        entries.push(Woff2TableEntry {
            tag,
            orig_length: orig_len,
        });
    }

    let compressed_data = woff2.get(pos..pos + total_compressed_size).ok_or_else(|| {
        RenderError::InvalidInput("woff2: compressed data out of bounds".into())
    })?;

    let uncompressed_stream = brotli_decompress(compressed_data, total_sfnt_size)?;

    // Reconstruct sfnt
    let mut sfnt_out = Vec::with_capacity(total_sfnt_size);
    sfnt_out.extend_from_slice(&flavor.to_be_bytes());
    sfnt_out.extend_from_slice(&(num_tables as u16).to_be_bytes());

    let entry_selector = (num_tables as u32).checked_ilog2().unwrap_or(0);
    let search_range = (1u32 << entry_selector) * 16;
    let range_shift = (num_tables as u32) * 16 - search_range;

    sfnt_out.extend_from_slice(&(search_range as u16).to_be_bytes());
    sfnt_out.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    sfnt_out.extend_from_slice(&(range_shift as u16).to_be_bytes());

    // Allocate directory entries placeholder
    let dir_start = sfnt_out.len();
    sfnt_out.resize(dir_start + num_tables * SFNT_DIR_ENTRY_LEN, 0);

    let mut stream_offset = 0usize;
    let mut head_offset_in_sfnt = None;

    for (i, entry) in entries.iter().enumerate() {
        let table_data = uncompressed_stream
            .get(stream_offset..stream_offset + entry.orig_length)
            .ok_or_else(|| {
                RenderError::InvalidInput("woff2: uncompressed stream truncated".into())
            })?;
        stream_offset += entry.orig_length;

        let table_offset = sfnt_out.len() as u32;
        if &entry.tag == b"head" {
            head_offset_in_sfnt = Some(table_offset as usize);
        }

        let checksum = calc_checksum(table_data);
        sfnt_out.extend_from_slice(table_data);
        while sfnt_out.len() % 4 != 0 {
            sfnt_out.push(0);
        }

        let record_offset = dir_start + i * SFNT_DIR_ENTRY_LEN;
        sfnt_out[record_offset..record_offset + 4].copy_from_slice(&entry.tag);
        sfnt_out[record_offset + 4..record_offset + 8].copy_from_slice(&checksum.to_be_bytes());
        sfnt_out[record_offset + 8..record_offset + 12]
            .copy_from_slice(&table_offset.to_be_bytes());
        sfnt_out[record_offset + 12..record_offset + 16]
            .copy_from_slice(&(entry.orig_length as u32).to_be_bytes());
    }

    // Fix head.checkSumAdjustment if head exists
    if let Some(head_off) = head_offset_in_sfnt {
        if sfnt_out.len() >= head_off + 12 {
            // Clear checkSumAdjustment before checksumming
            sfnt_out[head_off + 8..head_off + 12].copy_from_slice(&0u32.to_be_bytes());
            let full_sum = calc_checksum(&sfnt_out);
            let adjustment = 0xB1B0_AFBAu32.wrapping_sub(full_sum);
            sfnt_out[head_off + 8..head_off + 12].copy_from_slice(&adjustment.to_be_bytes());
        }
    }

    Ok(sfnt_out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_woff2_encode_decode_round_trip() {
        let ttf = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
        let woff2 = encode_woff2(ttf).expect("encode woff2");
        assert_eq!(&woff2[..4], b"wOF2");

        // Assert significant compression
        assert!(woff2.len() < ttf.len());
        let ratio = (woff2.len() as f64) / (ttf.len() as f64);
        assert!(ratio < 0.70, "woff2 ratio is {}", ratio);

        let decoded = decode_woff2(&woff2).expect("decode woff2");
        let (_, orig_tables) = parse_sfnt_directory(ttf).expect("parse ttf");
        let (_, dec_tables) = parse_sfnt_directory(&decoded).expect("parse decoded");
        assert_eq!(orig_tables.len(), dec_tables.len());

        let num_t = orig_tables.len();
        for (i, (t_orig, t_dec)) in orig_tables.iter().zip(&dec_tables).enumerate() {
            assert_eq!(t_orig.tag, t_dec.tag);
            assert_eq!(t_orig.length, t_dec.length);
            let s_orig = slice_table(ttf, *t_orig, i + 1 == num_t).expect("slice orig");
            let s_dec = slice_table(&decoded, *t_dec, i + 1 == num_t).expect("slice dec");
            if &t_orig.tag == b"head" {
                assert_eq!(&s_orig[..8], &s_dec[..8]);
                assert_eq!(&s_orig[12..], &s_dec[12..]);
            } else {
                assert_eq!(s_orig, s_dec, "table {:?} data mismatch", t_orig.tag);
            }
        }
        assert_eq!(calc_checksum(&decoded), 0xB1B0_AFBA);
    }
}
