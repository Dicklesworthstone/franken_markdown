//! Minimal deterministic ZIP writer (PKWARE APPNOTE stored/DEFLATE subset).
//!
//! Supports stored (method 0) and DEFLATE (method 8) entries. DEFLATE bodies
//! come from the project's own clean-room compressor
//! (`franken_markdown::compress::zlib_compress`) with the 2-byte zlib header
//! and 4-byte Adler-32 trailer stripped, leaving the raw RFC 1951 stream ZIP
//! expects. CRC-32 is our own table-driven IEEE implementation — zero
//! third-party dependencies, as always.
//!
//! Determinism doctrine: entries are written in insertion order, every DOS
//! date/time field is zero, no comments are emitted, and the UTF-8 name flag
//! (general-purpose bit 11) is always set. Identical inputs produce
//! byte-identical archives. Small archives retain the classic ZIP encoding;
//! ZIP64 extra fields and end records are emitted only when needed to represent
//! entry sizes, offsets, central directory size, or entry count without loss.
//!
//! Entry names must fit the ZIP format's 65535-byte UTF-8 name field. ZIP64
//! extends sizes and counts, not the filename-length field.
use franken_markdown::compress::{ZlibCompressScratch, zlib_compress_with_scratch};

// ---------------------------------------------------------------------------
// CRC-32 (IEEE 802.3, reflected polynomial 0xEDB88320), table-driven.

/// CRC-32 lookup table, built at compile time from the IEEE polynomial.
static CRC32_TABLE: [u32; 256] = build_crc32_table();

const fn build_crc32_table() -> [u32; 256] {
    const POLY: u32 = 0xEDB8_8320;
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut bit = 0;
        while bit < 8 {
            c = if c & 1 == 1 { (c >> 1) ^ POLY } else { c >> 1 };
            bit += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// Compute the CRC-32 (IEEE 802.3 / zlib polynomial) of `data`.
///
/// Reference vector: `crc32(b"123456789") == 0xCBF43926`.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    let mut chunks = data.chunks_exact(8);
    for chunk in chunks.by_ref() {
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[0])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[1])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[2])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[3])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[4])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[5])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[6])) & 0xFF) as usize] ^ (crc >> 8);
        crc = CRC32_TABLE[((crc ^ u32::from(chunk[7])) & 0xFF) as usize] ^ (crc >> 8);
    }
    for &b in chunks.remainder() {
        crc = CRC32_TABLE[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

// ---------------------------------------------------------------------------
// Archive layout constants (APPNOTE classic and ZIP64 formats).

const LOCAL_HEADER_SIG: u32 = 0x0403_4B50;
const CENTRAL_HEADER_SIG: u32 = 0x0201_4B50;
const EOCD_SIG: u32 = 0x0605_4B50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4B50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4B50;
const ZIP64_EXTRA_ID: u16 = 0x0001;

/// Compression method: stored, no compression.
const METHOD_STORED: u16 = 0;
/// Compression method: DEFLATE (raw RFC 1951 stream).
const METHOD_DEFLATE: u16 = 8;
/// General-purpose flag bit 11: entry names are UTF-8.
const FLAG_UTF8: u16 = 1 << 11;
/// Version needed to extract: 2.0 covers DEFLATE and is harmless for stored.
const VERSION_NEEDED: u16 = 20;
/// ZIP64 requires version 4.5.
const VERSION_ZIP64: u16 = 45;

#[derive(Debug)]
struct Entry {
    name: String,
    method: u16,
    crc32: u32,
    uncompressed_size: u64,
    /// Stored payload (method 0) or raw DEFLATE body (method 8).
    data: Vec<u8>,
}

impl Entry {
    fn needs_zip64_sizes(&self) -> bool {
        // All-ones values are ZIP64 sentinels, including at the exact boundary.
        self.uncompressed_size >= u64::from(u32::MAX)
            || self.data.len() as u64 >= u64::from(u32::MAX)
    }
}

/// Deterministic ZIP archive writer.
///
/// Entries are serialized in insertion order. DOS timestamps are zero and
/// ZIP64 metadata is emitted only when necessary, so output is byte-identical
/// for identical inputs and classic archives retain their original encoding.
#[derive(Debug, Default)]
pub struct ZipWriter {
    entries: Vec<Entry>,
    /// One LZ77 scratch shared by every `add_deflated` entry of this archive
    /// (byte-equivalent to a fresh scratch per entry by the generation-base
    /// scheme; only table regrowth is skipped).
    scratch: ZlibCompressScratch,
}

// ---------------------------------------------------------------------------
// Scratch-shared DEFLATE entry construction.

/// Build one DEFLATE `Entry` against `scratch`.
///
/// `add_deflated` passes the writer's single long-lived scratch so an entire
/// archive (and every archive an embedder builds in a loop) amortizes the LZ77
/// table regrowth; the compressor's generation-base scratch scheme guarantees
/// the emitted stream is byte-identical to a fresh scratch per call, so this
/// is purely an allocation/locality lever (see `compress::ZlibCompressScratch`
/// and the zip-level equality test below).
fn deflated_entry(name: &str, bytes: &[u8], scratch: &mut ZlibCompressScratch) -> Entry {
    let zlib = zlib_compress_with_scratch(bytes, scratch);
    // zlib layout: 2-byte header | raw deflate body | 4-byte adler32.
    let body = if zlib.len() >= 6 {
        zlib[2..zlib.len() - 4].to_vec()
    } else {
        // Unreachable with the project compressor (it always emits a full
        // header, at least one stored block, and the Adler trailer). Fall
        // back to a valid empty final stored block rather than corrupting
        // the archive layout.
        vec![0x01, 0x00, 0x00, 0xFF, 0xFF]
    };
    Entry {
        name: name.to_string(),
        method: METHOD_DEFLATE,
        crc32: crc32(bytes),
        uncompressed_size: bytes.len() as u64,
        data: body,
    }
}

impl ZipWriter {
    /// Create an empty archive writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an entry stored without compression (ZIP method 0).
    pub fn add_stored(&mut self, name: &str, bytes: &[u8]) {
        self.entries.push(Entry {
            name: name.to_string(),
            method: METHOD_STORED,
            crc32: crc32(bytes),
            uncompressed_size: bytes.len() as u64,
            data: bytes.to_vec(),
        });
    }

    /// Add an entry compressed with DEFLATE (ZIP method 8).
    ///
    /// The raw DEFLATE stream is recovered from the project compressor's zlib
    /// output by stripping the 2-byte zlib header and 4-byte Adler-32 trailer.
    /// All deflated entries of one writer share a single LZ77 scratch; output
    /// bytes are identical to per-entry fresh compression.
    pub fn add_deflated(&mut self, name: &str, bytes: &[u8]) {
        let entry = deflated_entry(name, bytes, &mut self.scratch);
        self.entries.push(entry);
    }

    /// Serialize the archive: local headers in insertion order, then the
    /// central directory, optional ZIP64 end records, and the classic end record.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        let mut zip64_entries = false;
        for entry in &self.entries {
            let local_offset = out.len() as u64;
            zip64_entries |= write_local_entry(&mut out, entry);
            zip64_entries |= write_central_entry(&mut central, entry, local_offset);
        }

        let cd_offset = out.len() as u64;
        let cd_size = central.len() as u64;
        out.extend_from_slice(&central);
        write_end_records(
            &mut out,
            self.entries.len() as u64,
            cd_size,
            cd_offset,
            zip64_entries,
        );
        out
    }
}

/// Write a local header and payload; return whether its sizes require ZIP64.
fn write_local_entry(out: &mut Vec<u8>, entry: &Entry) -> bool {
    let zip64 = entry.needs_zip64_sizes();
    let name = entry.name.as_bytes();
    let compressed_size = entry.data.len() as u64;
    push_u32(out, LOCAL_HEADER_SIG);
    push_u16(
        out,
        if zip64 {
            VERSION_ZIP64
        } else {
            VERSION_NEEDED
        },
    );
    push_u16(out, FLAG_UTF8);
    push_u16(out, entry.method);
    push_u16(out, 0); // mod time: zero for determinism
    push_u16(out, 0); // mod date: zero for determinism
    push_u32(out, entry.crc32);
    push_u32(
        out,
        if zip64 {
            u32::MAX
        } else {
            to_u32(compressed_size)
        },
    );
    push_u32(
        out,
        if zip64 {
            u32::MAX
        } else {
            to_u32(entry.uncompressed_size)
        },
    );
    push_u16(out, to_u16(name.len() as u64));
    push_u16(out, if zip64 { 20 } else { 0 });
    out.extend_from_slice(name);
    if zip64 {
        // Local ZIP64 extra fields always carry BOTH sizes, uncompressed first.
        push_u16(out, ZIP64_EXTRA_ID);
        push_u16(out, 16);
        push_u64(out, entry.uncompressed_size);
        push_u64(out, compressed_size);
    }
    out.extend_from_slice(&entry.data);
    zip64
}

/// Write a central header. ZIP64 values appear only for sentinel fields and in
/// APPNOTE order: uncompressed size, compressed size, then local-header offset.
fn write_central_entry(out: &mut Vec<u8>, entry: &Entry, local_offset: u64) -> bool {
    let zip64_sizes = entry.needs_zip64_sizes();
    let zip64_offset = local_offset >= u64::from(u32::MAX);
    let zip64 = zip64_sizes || zip64_offset;
    let version = if zip64 {
        VERSION_ZIP64
    } else {
        VERSION_NEEDED
    };
    let name = entry.name.as_bytes();
    let compressed_size = entry.data.len() as u64;
    let extra_size = (if zip64_sizes { 16 } else { 0 }) + (if zip64_offset { 8 } else { 0 });

    push_u32(out, CENTRAL_HEADER_SIG);
    push_u16(out, version); // version made by
    push_u16(out, version);
    push_u16(out, FLAG_UTF8);
    push_u16(out, entry.method);
    push_u16(out, 0); // mod time
    push_u16(out, 0); // mod date
    push_u32(out, entry.crc32);
    push_u32(
        out,
        if zip64_sizes {
            u32::MAX
        } else {
            to_u32(compressed_size)
        },
    );
    push_u32(
        out,
        if zip64_sizes {
            u32::MAX
        } else {
            to_u32(entry.uncompressed_size)
        },
    );
    push_u16(out, to_u16(name.len() as u64));
    push_u16(out, if zip64 { 4 + extra_size } else { 0 });
    push_u16(out, 0); // comment length
    push_u16(out, 0); // disk number start
    push_u16(out, 0); // internal attributes
    push_u32(out, 0); // external attributes
    push_u32(out, to_u32(local_offset));
    out.extend_from_slice(name);
    if zip64 {
        push_u16(out, ZIP64_EXTRA_ID);
        push_u16(out, extra_size);
        if zip64_sizes {
            push_u64(out, entry.uncompressed_size);
            push_u64(out, compressed_size);
        }
        if zip64_offset {
            push_u64(out, local_offset);
        }
    }
    zip64
}

fn write_end_records(
    out: &mut Vec<u8>,
    count: u64,
    cd_size: u64,
    cd_offset: u64,
    zip64_entries: bool,
) {
    if zip64_entries
        || count >= u64::from(u16::MAX)
        || cd_size >= u64::from(u32::MAX)
        || cd_offset >= u64::from(u32::MAX)
    {
        let zip64_offset = out.len() as u64;
        push_u32(out, ZIP64_EOCD_SIG);
        push_u64(out, 44); // remaining fixed record size, excluding signature/size
        push_u16(out, VERSION_ZIP64); // version made by
        push_u16(out, VERSION_ZIP64);
        push_u32(out, 0); // this disk
        push_u32(out, 0); // central directory disk
        push_u64(out, count); // entries on this disk
        push_u64(out, count); // total entries
        push_u64(out, cd_size);
        push_u64(out, cd_offset);

        push_u32(out, ZIP64_LOCATOR_SIG);
        push_u32(out, 0); // disk containing the ZIP64 end record
        push_u64(out, zip64_offset);
        push_u32(out, 1); // total disks
    }

    // Saturated fields here are sentinels, backed by the exact ZIP64 values above.
    push_u32(out, EOCD_SIG);
    push_u16(out, 0); // this disk
    push_u16(out, 0); // central directory disk
    push_u16(out, to_u16(count));
    push_u16(out, to_u16(count));
    push_u32(out, to_u32(cd_size));
    push_u32(out, to_u32(cd_offset));
    push_u16(out, 0); // comment length
}

#[inline(always)]
fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn to_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[inline(always)]
fn to_u16(n: u64) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn read_u64(bytes: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }

    fn metadata_entry(uncompressed_size: u64) -> Entry {
        // Metadata-only size tests avoid allocating multi-gigabyte payloads.
        Entry {
            name: "x".to_string(),
            method: METHOD_STORED,
            crc32: 0,
            uncompressed_size,
            data: vec![b'x'],
        }
    }

    #[test]
    fn classic_empty_archive_encoding_is_unchanged() {
        let mut expected = EOCD_SIG.to_le_bytes().to_vec();
        expected.resize(22, 0);
        assert_eq!(ZipWriter::new().finish(), expected);
    }

    #[test]
    fn zip64_size_fields_preserve_exact_boundary_values() {
        let limit = u64::from(u32::MAX);
        for size in [limit - 1, limit, limit + 1] {
            let entry = metadata_entry(size);
            let mut local = Vec::new();
            let mut central = Vec::new();
            let zip64 = size >= limit;
            assert_eq!(write_local_entry(&mut local, &entry), zip64);
            assert_eq!(write_central_entry(&mut central, &entry, 0), zip64);
            assert_eq!(read_u16(&local, 28), if zip64 { 20 } else { 0 });
            assert_eq!(read_u16(&central, 30), if zip64 { 20 } else { 0 });
            if zip64 {
                assert_eq!(read_u16(&local, 4), VERSION_ZIP64);
                assert_eq!(read_u16(&central, 6), VERSION_ZIP64);
                assert_eq!(read_u32(&local, 18), u32::MAX);
                assert_eq!(read_u32(&local, 22), u32::MAX);
                assert_eq!(read_u32(&central, 20), u32::MAX);
                assert_eq!(read_u32(&central, 24), u32::MAX);
                for (bytes, extra) in [(&local, 31), (&central, 47)] {
                    assert_eq!(read_u16(bytes, extra), ZIP64_EXTRA_ID);
                    assert_eq!(read_u16(bytes, extra + 2), 16);
                    assert_eq!(read_u64(bytes, extra + 4), size);
                    assert_eq!(read_u64(bytes, extra + 12), 1);
                }
            } else {
                assert_eq!(read_u16(&local, 4), VERSION_NEEDED);
                assert_eq!(read_u32(&local, 18), 1);
                assert_eq!(read_u32(&local, 22), size as u32);
                assert_eq!(read_u32(&central, 24), size as u32);
            }
        }
    }

    #[test]
    fn zip64_offsets_do_not_require_zip64_sizes() {
        let limit = u64::from(u32::MAX);
        for offset in [limit - 1, limit, limit + 1] {
            let mut central = Vec::new();
            let zip64 = offset >= limit;
            assert_eq!(
                write_central_entry(&mut central, &metadata_entry(1), offset),
                zip64
            );
            assert_eq!(read_u32(&central, 20), 1);
            assert_eq!(read_u32(&central, 24), 1);
            assert_eq!(read_u32(&central, 42), to_u32(offset));
            assert_eq!(read_u16(&central, 30), if zip64 { 12 } else { 0 });
            if zip64 {
                assert_eq!(read_u16(&central, 47), ZIP64_EXTRA_ID);
                assert_eq!(read_u16(&central, 49), 8);
                assert_eq!(read_u64(&central, 51), offset);
            }
        }
    }

    #[test]
    fn zip64_central_extra_orders_sizes_before_offset() {
        let size = u64::from(u32::MAX) + 1;
        let offset = size + 200;
        let mut central = Vec::new();
        assert!(write_central_entry(
            &mut central,
            &metadata_entry(size),
            offset
        ));
        assert_eq!(read_u16(&central, 30), 28);
        assert_eq!(read_u16(&central, 49), 24);
        assert_eq!(read_u64(&central, 51), size);
        assert_eq!(read_u64(&central, 59), 1);
        assert_eq!(read_u64(&central, 67), offset);
    }

    #[test]
    fn end_records_cover_all_zip64_triggers_and_exact_sentinels() {
        let count_limit = u64::from(u16::MAX);
        let size_limit = u64::from(u32::MAX);
        for (count, size, offset, members, zip64) in [
            (count_limit - 1, size_limit - 1, size_limit - 1, false, false),
            (count_limit, 46, 30, false, true),
            (count_limit + 1, 46, 30, false, true),
            (1, size_limit, 30, false, true),
            (1, size_limit + 1, 30, false, true),
            (1, 46, size_limit, false, true),
            (1, 46, size_limit + 1, false, true),
            (1, 46, 30, true, true),
        ] {
            let mut bytes = Vec::new();
            write_end_records(&mut bytes, count, size, offset, members);
            assert_eq!(bytes.len(), if zip64 { 98 } else { 22 });
            let end = bytes.len() - 22;
            assert_eq!(read_u32(&bytes, end), EOCD_SIG);
            assert_eq!(read_u16(&bytes, end + 8), to_u16(count));
            assert_eq!(read_u16(&bytes, end + 10), to_u16(count));
            assert_eq!(read_u32(&bytes, end + 12), to_u32(size));
            assert_eq!(read_u32(&bytes, end + 16), to_u32(offset));
            if zip64 {
                assert_eq!(read_u32(&bytes, 0), ZIP64_EOCD_SIG);
                assert_eq!(read_u64(&bytes, 4), 44);
                assert_eq!(read_u64(&bytes, 24), count);
                assert_eq!(read_u64(&bytes, 32), count);
                assert_eq!(read_u64(&bytes, 40), size);
                assert_eq!(read_u64(&bytes, 48), offset);
                assert_eq!(read_u32(&bytes, 56), ZIP64_LOCATOR_SIG);
                assert_eq!(read_u64(&bytes, 64), 0);
                assert_eq!(read_u32(&bytes, 72), 1);
            }
        }
    }

    #[test]
    fn archive_above_classic_entry_limit_has_complete_directory() {
        let count = usize::from(u16::MAX) + 1;
        let mut writer = ZipWriter::new();
        for index in 0..count {
            writer.add_stored(&format!("entry-{index}"), b"");
        }
        let bytes = writer.finish();
        let end = bytes.len() - 22;
        assert_eq!(read_u16(&bytes, end + 10), u16::MAX);
        let locator = end - 20;
        assert_eq!(read_u32(&bytes, locator), ZIP64_LOCATOR_SIG);
        let record = usize::try_from(read_u64(&bytes, locator + 8)).unwrap();
        assert_eq!(read_u32(&bytes, record), ZIP64_EOCD_SIG);
        assert_eq!(read_u64(&bytes, record + 32), count as u64);
        let mut cursor = usize::try_from(read_u64(&bytes, record + 48)).unwrap();
        let directory_end = cursor + usize::try_from(read_u64(&bytes, record + 40)).unwrap();
        for index in 0..count {
            assert_eq!(read_u32(&bytes, cursor), CENTRAL_HEADER_SIG);
            let name_len = usize::from(read_u16(&bytes, cursor + 28));
            let name = &bytes[cursor + 46..cursor + 46 + name_len];
            assert_eq!(name, format!("entry-{index}").as_bytes());
            let local_offset = read_u32(&bytes, cursor + 42) as usize;
            assert_eq!(read_u32(&bytes, local_offset), LOCAL_HEADER_SIG);
            cursor += 46 + name_len;
        }
        assert_eq!(cursor, directory_end);
        assert_eq!(directory_end, record);
    }

    #[test]
    fn shared_scratch_archive_matches_fresh_per_entry_bytes() {
        // Isomorphism holder for the ZipWriter scratch field (one LZ77 scratch
        // reused by every `add_deflated` entry of an archive — the path EPUB
        // and the wasm book loop take). Build the same archive twice: once via
        // the public API (shared writer scratch, dirty from earlier entries),
        // once with a brand-new scratch per entry, and demand byte-identical
        // archives. Payloads cover empty, tiny, repetitive (fixed-Huffman),
        // and pseudo-random incompressible (stored-block) shapes.
        let lcg: Vec<u8> = {
            let mut state: u64 = 0x243F_6A88_85A3_08D3;
            (0..40_000)
                .map(|_| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    (state >> 33) as u8
                })
                .collect()
        };
        let payloads: Vec<(&str, Vec<u8>)> = vec![
            ("mimetype-ish", b"stored payload".to_vec()),
            ("empty.bin", Vec::new()),
            ("repeat.txt", b"deflate me ".repeat(600)),
            ("rand.bin", lcg),
            ("tiny.txt", b"z".to_vec()),
        ];
        let mut shared = ZipWriter::new();
        let mut fresh = ZipWriter::new();
        for (name, bytes) in &payloads {
            shared.add_deflated(name, bytes);
            fresh
                .entries
                .push(deflated_entry(name, bytes, &mut ZlibCompressScratch::new()));
        }

        // Every shared-scratch entry body equals a fresh one-shot
        // `zlib_compress` of the same payload, header/trailer stripped.
        for (entry, (_, bytes)) in shared.entries.iter().zip(payloads.iter()) {
            let one_shot = franken_markdown::compress::zlib_compress(bytes);
            assert_eq!(entry.method, METHOD_DEFLATE);
            assert_eq!(
                entry.data.as_slice(),
                &one_shot[2..one_shot.len() - 4],
                "entry {} body must equal fresh zlib body",
                entry.name
            );
        }

        // And the whole archive built through one shared scratch is
        // byte-identical to the fresh-scratch-per-entry archive.
        assert_eq!(
            shared.finish(),
            fresh.finish(),
            "archive built with one shared scratch must be byte-identical to fresh-per-entry"
        );
    }
}
