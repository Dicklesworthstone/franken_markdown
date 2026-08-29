//! Minimal deterministic ZIP writer (PKWARE APPNOTE classic-format subset).
//!
//! Supports stored (method 0) and DEFLATE (method 8) entries. DEFLATE bodies
//! come from the project's own clean-room compressor
//! (`franken_markdown::compress::zlib_compress`) with the 2-byte zlib header
//! and 4-byte Adler-32 trailer stripped, leaving the raw RFC 1951 stream ZIP
//! expects. CRC-32 is our own table-driven IEEE implementation — zero
//! third-party dependencies, as always.
//!
//! Determinism doctrine: entries are written in insertion order, every DOS
//! date/time field is zero, no extra fields or comments are emitted, and the
//! UTF-8 name flag (general-purpose bit 11) is always set. Identical inputs
//! produce byte-identical archives.
//!
//! Precondition (EPUB-scale use): each entry and the whole archive stay below
//! 4 GiB and the entry count stays below 65536 — the classic non-ZIP64 format
//! cannot express larger values. Length conversions saturate rather than
//! panic, so an absurd input yields a corrupt-but-safe archive instead of a
//! crash.

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
// Archive layout constants (APPNOTE classic format).

const LOCAL_HEADER_SIG: u32 = 0x0403_4B50;
const CENTRAL_HEADER_SIG: u32 = 0x0201_4B50;
const EOCD_SIG: u32 = 0x0605_4B50;

/// Compression method: stored, no compression.
const METHOD_STORED: u16 = 0;
/// Compression method: DEFLATE (raw RFC 1951 stream).
const METHOD_DEFLATE: u16 = 8;
/// General-purpose flag bit 11: entry names are UTF-8.
const FLAG_UTF8: u16 = 1 << 11;
/// Version needed to extract: 2.0 covers DEFLATE and is harmless for stored.
const VERSION_NEEDED: u16 = 20;

#[derive(Debug)]
struct Entry {
    name: String,
    method: u16,
    crc32: u32,
    uncompressed_size: u32,
    /// Stored payload (method 0) or raw DEFLATE body (method 8).
    data: Vec<u8>,
}

/// Deterministic ZIP archive writer.
///
/// Entries are serialized in insertion order. DOS timestamps are zero and no
/// extra fields or comments are emitted, so output is byte-identical for
/// identical inputs.
#[derive(Debug, Default)]
pub struct ZipWriter {
    entries: Vec<Entry>,
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
            uncompressed_size: to_u32(bytes.len()),
            data: bytes.to_vec(),
        });
    }

    /// Add an entry compressed with DEFLATE (ZIP method 8).
    ///
    /// The raw DEFLATE stream is recovered from the project compressor's zlib
    /// output by stripping the 2-byte zlib header and 4-byte Adler-32 trailer.
    pub fn add_deflated(&mut self, name: &str, bytes: &[u8]) {
        let zlib = franken_markdown::compress::zlib_compress(bytes);
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
        self.entries.push(Entry {
            name: name.to_string(),
            method: METHOD_DEFLATE,
            crc32: crc32(bytes),
            uncompressed_size: to_u32(bytes.len()),
            data: body,
        });
    }

    /// Serialize the archive: local headers in insertion order, then the
    /// central directory, then the end-of-central-directory record.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for entry in &self.entries {
            let local_offset = to_u32(out.len());
            let name = entry.name.as_bytes();
            let name_len = to_u16(name.len());
            let compressed_size = to_u32(entry.data.len());

            // Local file header.
            push_u32(&mut out, LOCAL_HEADER_SIG);
            push_u16(&mut out, VERSION_NEEDED);
            push_u16(&mut out, FLAG_UTF8);
            push_u16(&mut out, entry.method);
            push_u16(&mut out, 0); // mod time: zero for determinism
            push_u16(&mut out, 0); // mod date: zero for determinism
            push_u32(&mut out, entry.crc32);
            push_u32(&mut out, compressed_size);
            push_u32(&mut out, entry.uncompressed_size);
            push_u16(&mut out, name_len);
            push_u16(&mut out, 0); // extra field length
            out.extend_from_slice(name);
            out.extend_from_slice(&entry.data);

            // Central directory header.
            push_u32(&mut central, CENTRAL_HEADER_SIG);
            push_u16(&mut central, VERSION_NEEDED); // version made by
            push_u16(&mut central, VERSION_NEEDED);
            push_u16(&mut central, FLAG_UTF8);
            push_u16(&mut central, entry.method);
            push_u16(&mut central, 0); // mod time
            push_u16(&mut central, 0); // mod date
            push_u32(&mut central, entry.crc32);
            push_u32(&mut central, compressed_size);
            push_u32(&mut central, entry.uncompressed_size);
            push_u16(&mut central, name_len);
            push_u16(&mut central, 0); // extra field length
            push_u16(&mut central, 0); // comment length
            push_u16(&mut central, 0); // disk number start
            push_u16(&mut central, 0); // internal attributes
            push_u32(&mut central, 0); // external attributes
            push_u32(&mut central, local_offset);
            central.extend_from_slice(name);
        }

        let cd_offset = to_u32(out.len());
        let cd_size = to_u32(central.len());
        out.extend_from_slice(&central);

        let count = to_u16(self.entries.len());
        push_u32(&mut out, EOCD_SIG);
        push_u16(&mut out, 0); // this disk
        push_u16(&mut out, 0); // central directory disk
        push_u16(&mut out, count);
        push_u16(&mut out, count);
        push_u32(&mut out, cd_size);
        push_u32(&mut out, cd_offset);
        push_u16(&mut out, 0); // comment length
        out
    }
}

fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Classic ZIP size fields are 32-bit. EPUB-scale archives never approach the
/// limit; saturate rather than panic (see module precondition note).
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}
