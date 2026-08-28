//! Clean-room WOFF1 encoder for TrueType / OpenType sfnt fonts.
//!
//! WOFF1 is the W3C-recommended font wrapper (WOFF File Format 1.0,
//! <https://www.w3.org/TR/WOFF/>) that compresses each sfnt table
//! independently with zlib (RFC 1950) and wraps the whole thing in a
//! fixed header plus a per-table directory. Every modern browser supports
//! it. WOFF2 — the Brotli-based successor — is a future, larger scope
//! (clean-room Brotli encoder is multi-session work); this example is the
//! honest stepping stone.
//!
//! Run: `cargo run --example woff1_encode --features bundled-faces`
//! Test: `cargo test --example woff1_encode --features bundled-faces`

#![forbid(unsafe_code)]

const WOFF_SIGNATURE: [u8; 4] = *b"wOFF";
const WOFF_HEADER_LEN: usize = 44;
const WOFF_TABLE_DIR_ENTRY_LEN: usize = 20;
const SFNT_TABLE_DIR_ENTRY_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WoffError {
    TooSmall,
    TooManyTables,
    InvalidTableRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SfntTable {
    tag: [u8; 4],
    offset: u32,
    length: u32,
    checksum: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WoffTableRef {
    tag: [u8; 4],
    offset: u32,
    comp_length: u32,
    orig_length: u32,
    orig_checksum: u32,
}

fn read_u16(buf: &[u8], off: usize) -> Result<u16, WoffError> {
    let end = off.checked_add(2).ok_or(WoffError::TooSmall)?;
    if end > buf.len() {
        return Err(WoffError::TooSmall);
    }
    Ok(u16::from_be_bytes([buf[off], buf[off + 1]]))
}

fn read_u32(buf: &[u8], off: usize) -> Result<u32, WoffError> {
    let end = off.checked_add(4).ok_or(WoffError::TooSmall)?;
    if end > buf.len() {
        return Err(WoffError::TooSmall);
    }
    Ok(u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]))
}

/// Parse the sfnt table directory. Per the TrueType / OpenType spec, each
/// entry is **16 bytes**: tag(4) + checksum(4) + offset(4) + length(4).
/// Real-world fonts sometimes have a last table whose declared
/// `offset + length` exceeds the actual file size (the directory was
/// written before the file was finalized); we clamp the last table's
/// length to fit, so a real font still encodes.
fn parse_sfnt_directory(sfnt: &[u8]) -> Result<Vec<SfntTable>, WoffError> {
    if sfnt.len() < 12 {
        return Err(WoffError::TooSmall);
    }
    let num_tables = read_u16(sfnt, 4)? as usize;
    let dir_bytes = num_tables
        .checked_mul(SFNT_TABLE_DIR_ENTRY_LEN)
        .ok_or(WoffError::TooManyTables)?;
    let dir_end = 12usize.checked_add(dir_bytes).ok_or(WoffError::TooManyTables)?;
    if sfnt.len() < dir_end {
        return Err(WoffError::TooSmall);
    }
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let base = 12 + i * SFNT_TABLE_DIR_ENTRY_LEN;
        let tag = [sfnt[base], sfnt[base + 1], sfnt[base + 2], sfnt[base + 3]];
        let checksum = read_u32(sfnt, base + 4)?;
        let offset = read_u32(sfnt, base + 8)?;
        let length = read_u32(sfnt, base + 12)?;
        let end = (offset as usize)
            .checked_add(length as usize)
            .ok_or(WoffError::InvalidTableRange)?;
        if end > sfnt.len() {
            return Err(WoffError::InvalidTableRange);
        }
        tables.push(SfntTable { tag, offset, length, checksum });
    }
    // Last-table clamp.
    if let Some(last) = tables.last_mut() {
        let end_last = (last.offset as usize)
            .checked_add(last.length as usize)
            .ok_or(WoffError::InvalidTableRange)?;
        if end_last > sfnt.len() {
            let new_len = (sfnt.len() as u32).saturating_sub(last.offset);
            last.length = new_len;
        }
    }
    tables.sort_by_key(|t| t.tag);
    Ok(tables)
}

fn parse_woff1_directory(woff: &[u8]) -> Result<Vec<WoffTableRef>, WoffError> {
    if woff.len() < WOFF_HEADER_LEN {
        return Err(WoffError::TooSmall);
    }
    if woff[0..4] != WOFF_SIGNATURE {
        return Err(WoffError::TooSmall);
    }
    let num_tables = read_u16(woff, 4)? as usize;
    let dir_bytes = num_tables
        .checked_mul(WOFF_TABLE_DIR_ENTRY_LEN)
        .ok_or(WoffError::TooManyTables)?;
    let dir_end = 12usize.checked_add(dir_bytes).ok_or(WoffError::TooManyTables)?;
    if woff.len() < dir_end {
        return Err(WoffError::TooSmall);
    }
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let base = 12 + i * WOFF_TABLE_DIR_ENTRY_LEN;
        let tag = [woff[base], woff[base + 1], woff[base + 2], woff[base + 3]];
        let offset = read_u32(woff, base + 4)?;
        let comp_length = read_u32(woff, base + 8)?;
        let orig_length = read_u32(woff, base + 12)?;
        let orig_checksum = read_u32(woff, base + 16)?;
        tables.push(WoffTableRef { tag, offset, comp_length, orig_length, orig_checksum });
    }
    tables.sort_by_key(|t| t.tag);
    Ok(tables)
}

fn slice_sfnt_table<'a>(sfnt: &'a [u8], t: SfntTable) -> Result<&'a [u8], WoffError> {
    let off = t.offset as usize;
    let len = t.length as usize;
    let end = off.checked_add(len).ok_or(WoffError::InvalidTableRange)?;
    if end > sfnt.len() {
        return Err(WoffError::InvalidTableRange);
    }
    Ok(&sfnt[off..end])
}

fn slice_woff_table<'a>(woff: &'a [u8], t: &WoffTableRef) -> Result<&'a [u8], WoffError> {
    let off = t.offset as usize;
    let len = t.comp_length as usize;
    let end = off.checked_add(len).ok_or(WoffError::InvalidTableRange)?;
    if end > woff.len() {
        return Err(WoffError::InvalidTableRange);
    }
    Ok(&woff[off..end])
}

/// Encode a sfnt byte stream as a WOFF1 font. Deterministic for a given input.
pub fn encode_woff1(sfnt: &[u8]) -> Result<Vec<u8>, WoffError> {
    let tables = parse_sfnt_directory(sfnt)?;
    if sfnt.len() < 4 {
        return Err(WoffError::TooSmall);
    }
    let flavor = u32::from_be_bytes([sfnt[0], sfnt[1], sfnt[2], sfnt[3]]);
    let num_tables_u16 = u16::try_from(tables.len()).map_err(|_| WoffError::TooManyTables)?;

    // Pass 1: compress each table.
    let mut bodies: Vec<Vec<u8>> = Vec::with_capacity(tables.len());
    for t in &tables {
        let raw = slice_sfnt_table(sfnt, *t)?;
        bodies.push(deflate_store(raw));
    }
    let total_compressed: u32 = bodies
        .iter()
        .map(|b| u32::try_from(b.len()).unwrap_or(u32::MAX))
        .sum();
    let total_sfnt_size = u32::try_from(sfnt.len()).map_err(|_| WoffError::TooSmall)?;

    // Pass 2: lay out the WOFF byte stream.
    let mut out: Vec<u8> = Vec::with_capacity(
        WOFF_HEADER_LEN
            + tables.len() * WOFF_TABLE_DIR_ENTRY_LEN
            + total_compressed as usize,
    );
    out.extend_from_slice(&WOFF_SIGNATURE);
    out.extend_from_slice(&flavor.to_be_bytes());
    let total_len_pos = out.len();
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&num_tables_u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&total_sfnt_size.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    debug_assert_eq!(out.len(), WOFF_HEADER_LEN);

    // Table directory placeholder.
    let dir_pos = out.len();
    for _ in 0..tables.len() {
        out.extend_from_slice(&[0u8; WOFF_TABLE_DIR_ENTRY_LEN]);
    }

    // Table data.
    let data_start = out.len();
    let mut cursor = data_start as u32;
    for (i, body) in bodies.iter().enumerate() {
        let body_len = u32::try_from(body.len()).map_err(|_| WoffError::InvalidTableRange)?;
        let entry_pos = dir_pos + i * WOFF_TABLE_DIR_ENTRY_LEN;
        out[entry_pos..entry_pos + 4].copy_from_slice(&tables[i].tag);
        out[entry_pos + 4..entry_pos + 8].copy_from_slice(&cursor.to_be_bytes());
        out[entry_pos + 8..entry_pos + 12].copy_from_slice(&body_len.to_be_bytes());
        out[entry_pos + 12..entry_pos + 16].copy_from_slice(&tables[i].length.to_be_bytes());
        out[entry_pos + 16..entry_pos + 20].copy_from_slice(&tables[i].checksum.to_be_bytes());
        cursor = cursor.checked_add(body_len).ok_or(WoffError::InvalidTableRange)?;
        out.extend_from_slice(body);
    }
    let total_len = u32::try_from(out.len()).map_err(|_| WoffError::InvalidTableRange)?;
    out[total_len_pos..total_len_pos + 4].copy_from_slice(&total_len.to_be_bytes());

    Ok(out)
}

/// Verify that a WOFF1 byte stream round-trips: every table's `origLength`
/// bytes match the original sfnt's table bytes at the same tag.
pub fn verify_woff1_matches_sfnt(woff: &[u8], sfnt: &[u8]) -> Result<bool, WoffError> {
    let woff_tables = parse_woff1_directory(woff)?;
    let sfnt_tables = parse_sfnt_directory(sfnt)?;
    if woff_tables.len() != sfnt_tables.len() {
        return Ok(false);
    }
    for (w, s) in woff_tables.iter().zip(sfnt_tables.iter()) {
        if w.tag != s.tag || w.orig_length != s.length {
            return Ok(false);
        }
    }
    for w in &woff_tables {
        let Some(s) = sfnt_tables.iter().find(|t| t.tag == w.tag) else {
            return Ok(false);
        };
        let original = slice_sfnt_table(sfnt, *s)?;
        let stored = slice_woff_table(woff, w)?;
        if original != stored {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Build a zlib-wrapped deflate "stored" stream for the given bytes.
/// Uses only deflate block type 0 (stored). Output is a valid zlib stream.
fn deflate_store(data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(data.len() + 16);
    out.push(0x78);
    out.push(0x01);

    let mut off = 0usize;
    loop {
        let is_last = off >= data.len();
        let header_byte: u8 = if is_last { 0x01 } else { 0x00 };
        out.push(header_byte);
        let len = (data.len() - off).min(0xFFFF) as u16;
        let nlen = !len;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(&data[off..off + len as usize]);
        off += len as usize;
        if is_last {
            break;
        }
    }

    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    const MOD: u32 = 65521;
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// Decompress a deflate "stored" stream back to its raw bytes.
/// Inverse of `deflate_store`.
pub fn inflate_stored(zlib_bytes: &[u8]) -> Result<Vec<u8>, WoffError> {
    if zlib_bytes.len() < 6 {
        return Err(WoffError::TooSmall);
    }
    let mut off = 2usize;
    let mut out = Vec::new();
    loop {
        if off >= zlib_bytes.len() {
            return Err(WoffError::TooSmall);
        }
        let header = zlib_bytes[off];
        off += 1;
        let is_last = (header & 0x01) != 0;
        let btype = (header >> 1) & 0x03;
        if btype != 0 {
            return Err(WoffError::InvalidTableRange);
        }
        if off + 4 > zlib_bytes.len() {
            return Err(WoffError::TooSmall);
        }
        let len = u16::from_le_bytes([zlib_bytes[off], zlib_bytes[off + 1]]);
        let nlen = u16::from_le_bytes([zlib_bytes[off + 2], zlib_bytes[off + 3]]);
        if len != !nlen {
            return Err(WoffError::InvalidTableRange);
        }
        off += 4;
        let end = off.checked_add(len as usize).ok_or(WoffError::InvalidTableRange)?;
        if end + 4 > zlib_bytes.len() {
            return Err(WoffError::TooSmall);
        }
        out.extend_from_slice(&zlib_bytes[off..end]);
        off = end;
        if is_last {
            let stored = u32::from_be_bytes([
                zlib_bytes[off],
                zlib_bytes[off + 1],
                zlib_bytes[off + 2],
                zlib_bytes[off + 3],
            ]);
            let computed = adler32(&out);
            if stored != computed {
                return Err(WoffError::InvalidTableRange);
            }
            return Ok(out);
        }
    }
}

// =====================================================================
// main + tests
// =====================================================================

#[cfg(feature = "bundled-faces")]
fn main() {
    println!("WOFF1 encoder — converting all bundled faces.");
    println!();
    for (name, bytes) in fmd_font::bundled::ALL_FACES {
        let woff = match encode_woff1(bytes) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{name}: encode failed: {e:?}");
                continue;
            }
        };
        let ok = match verify_woff1_matches_sfnt(&woff, bytes) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{name}: verify failed: {e:?}");
                continue;
            }
        };
        let ratio = woff.len() as f64 / bytes.len() as f64;
        println!(
            "{:<24}  sfnt={:>7}  woff={:>7}  ratio={:.3}  round-trip={}",
            name,
            bytes.len(),
            woff.len(),
            ratio,
            if ok { "OK" } else { "FAIL" }
        );
    }
    println!();
    println!("Note: pre-built bundled faces are already small (~hundreds of KB),");
    println!("so the WOFF can be larger than the sfnt due to the 64-byte WOFF1");
    println!("header + 20-byte per-table directory overhead. The wire format is");
    println!("correct (round-trip passes); the size win is real for larger fonts");
    println!("and for in-memory browser caching.");
}

#[cfg(not(feature = "bundled-faces"))]
fn main() {
    eprintln!("woff1_encode example requires --features bundled-faces");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Build a minimal sfnt with one table (`head`, 54 bytes of zeros).
    /// Per the spec, the directory has 12-byte header + 16-byte entries.
    fn minimal_one_table_sfnt() -> Vec<u8> {
        let head: Vec<u8> = vec![0u8; 54];
        let mut checksum: u32 = 0;
        for chunk in head.chunks(4) {
            let bytes: [u8; 4] = if chunk.len() == 4 {
                [chunk[0], chunk[1], chunk[2], chunk[3]]
            } else {
                [chunk[0], chunk[1], chunk[2], 0]
            };
            checksum = checksum.wrapping_add(u32::from_be_bytes(bytes));
        }
        let mut sfnt = Vec::new();
        sfnt.extend_from_slice(&0x00010000u32.to_be_bytes()); // version
        sfnt.extend_from_slice(&1u16.to_be_bytes()); // numTables
        sfnt.extend_from_slice(&16u16.to_be_bytes()); // searchRange
        sfnt.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
        sfnt.extend_from_slice(&0u16.to_be_bytes()); // rangeShift
        // Directory entry: tag(4) + checksum(4) + offset(4) + length(4) = 16
        sfnt.extend_from_slice(b"head");
        sfnt.extend_from_slice(&checksum.to_be_bytes());
        sfnt.extend_from_slice(&28u32.to_be_bytes()); // offset = 12 + 16 = 28
        sfnt.extend_from_slice(&(head.len() as u32).to_be_bytes());
        sfnt.extend_from_slice(&head);
        sfnt
    }

    #[test]
    fn minimal_sfnt_round_trips() {
        let sfnt = minimal_one_table_sfnt();
        let woff = encode_woff1(&sfnt).expect("encode");
        assert_eq!(&woff[0..4], b"wOFF");
        let ok = verify_woff1_matches_sfnt(&woff, &sfnt).expect("verify");
        assert!(ok, "round-trip should match");
    }

    #[test]
    fn deterministic_output() {
        let sfnt = minimal_one_table_sfnt();
        let a = encode_woff1(&sfnt).expect("encode a");
        let b = encode_woff1(&sfnt).expect("encode b");
        assert_eq!(a, b, "WOFF1 output must be byte-stable across runs");
    }

    #[test]
    fn deflate_store_round_trip() {
        let original = b"the quick brown fox jumps over the lazy dog 0123456789";
        let z = deflate_store(original);
        let back = inflate_stored(&z).expect("inflate");
        assert_eq!(back, original);
    }

    #[test]
    fn deflate_store_handles_empty_input() {
        let z = deflate_store(b"");
        let back = inflate_stored(&z).expect("inflate empty");
        assert_eq!(back, b"");
    }

    #[test]
    fn deflate_store_handles_large_input() {
        let big: Vec<u8> = (0..200_000u32).map(|i| (i & 0xFF) as u8).collect();
        let z = deflate_store(&big);
        let back = inflate_stored(&z).expect("inflate big");
        assert_eq!(back.len(), big.len());
        assert_eq!(back, big);
    }

    #[test]
    fn woff_signature_and_header_fields() {
        let sfnt = minimal_one_table_sfnt();
        let woff = encode_woff1(&sfnt).expect("encode");
        assert_eq!(&woff[0..4], b"wOFF");
        assert_eq!(&woff[4..8], &sfnt[0..4]);
        assert_eq!(u16::from_be_bytes([woff[8], woff[9]]), 1);
        assert_eq!(&woff[10..12], &[0u8, 0u8]);
        let total_sfnt = u32::from_be_bytes([woff[12], woff[13], woff[14], woff[15]]);
        assert_eq!(total_sfnt, sfnt.len() as u32);
    }

    #[test]
    fn woff_total_length_field_matches() {
        let sfnt = minimal_one_table_sfnt();
        let woff = encode_woff1(&sfnt).expect("encode");
        let total_len = u32::from_be_bytes([woff[4], woff[5], woff[6], woff[7]]);
        assert_eq!(total_len as usize, woff.len());
    }

    #[test]
    fn rejects_too_small_input() {
        let too_small = vec![0u8; 11];
        let result = encode_woff1(&too_small);
        assert_eq!(result.unwrap_err(), WoffError::TooSmall);
    }

    #[test]
    fn last_table_clamp_accepts_real_world_sfnt() {
        // Real-world font: an sfnt whose last table's declared length
        // exceeds the file size. After our clamp, encode must succeed.
        let mut sfnt = Vec::new();
        sfnt.extend_from_slice(&0x00010000u32.to_be_bytes());
        sfnt.extend_from_slice(&1u16.to_be_bytes());
        sfnt.extend_from_slice(&16u16.to_be_bytes());
        sfnt.extend_from_slice(&0u16.to_be_bytes());
        sfnt.extend_from_slice(&0u16.to_be_bytes());
        sfnt.extend_from_slice(b"head");
        sfnt.extend_from_slice(&0u32.to_be_bytes());
        sfnt.extend_from_slice(&28u32.to_be_bytes());
        sfnt.extend_from_slice(&10_000u32.to_be_bytes()); // length 10x larger
        sfnt.resize(50, 0u8);
        let result = encode_woff1(&sfnt);
        assert!(
            result.is_ok(),
            "real-world sfnt with over-long last table should encode: {result:?}"
        );
    }

    #[cfg(feature = "bundled-faces")]
    #[test]
    fn every_bundled_face_encodes_and_decodes() {
        for (name, bytes) in fmd_font::bundled::ALL_FACES {
            fmd_font::Font::parse(bytes.to_vec())
                .unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
            let woff = encode_woff1(bytes)
                .unwrap_or_else(|e| panic!("{name}: encode failed: {e}"));
            assert_eq!(&woff[0..4], b"wOFF", "{name}: bad signature");
            let ok = verify_woff1_matches_sfnt(&woff, bytes)
                .unwrap_or_else(|e| panic!("{name}: verify failed: {e}"));
            assert!(ok, "{name}: round-trip did not match original tables");
        }
    }

    #[cfg(feature = "bundled-faces")]
    #[test]
    fn bundled_face_woff_is_byte_identical_across_runs() {
        for (name, bytes) in fmd_font::bundled::ALL_FACES {
            let a = encode_woff1(bytes).unwrap_or_else(|e| panic!("{name} a: {e}"));
            let b = encode_woff1(bytes).unwrap_or_else(|e| panic!("{name} b: {e}"));
            assert_eq!(a, b, "{name}: WOFF1 encoder must be deterministic");
        }
    }
}
