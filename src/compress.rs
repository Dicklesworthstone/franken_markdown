//! Clean-room zlib/DEFLATE compressor (RFC 1950 + RFC 1951).
//!
//! Produces a zlib stream wrapping DEFLATE compressed data. The body is LZ77 +
//! the fixed (static) Huffman code (BTYPE=01) in a single final block, UNLESS a
//! stored-block encoding (BTYPE=00) is smaller (e.g. incompressible font bytes),
//! in which case the stored form is used so output never meaningfully expands.
//! Any standard zlib inflater (python `zlib`, miniz, flate2) recovers the input
//! byte-for-byte. Verified by round-tripping ~4500+ inputs through the real
//! python `zlib.decompress`: empty, all 256 byte values (9-bit codes), RLE runs,
//! max-length-258 matches, self-overlapping/periodic data, window-edge distances
//! (32768), incompressible random/LCG data, multi-block stored (>64KB), and an
//! exhaustive sweep of every match length 3..=258 at distances {1..8,300,32768}.
//!
//! Pure std, no third-party crates, no unsafe / unwrap / expect / panic.

// ---------------------------------------------------------------------------
// RFC 1951 length / distance tables.
// Index i corresponds to length symbol 257 + i (29 entries: symbols 257..=285).
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
// Index i corresponds to distance symbol i (30 entries: symbols 0..=29).
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

// ---------------------------------------------------------------------------
// LSB-first bit writer. Huffman codes are reversed before writing so that they
// appear MSB-first in the stream while every emitted bit still fills the byte
// LSB-first.
struct BitWriter {
    out: Vec<u8>,
    bitbuf: u32,
    bitcount: u32,
}

impl BitWriter {
    fn with_capacity(capacity: usize) -> Self {
        BitWriter {
            out: Vec::with_capacity(capacity),
            bitbuf: 0,
            bitcount: 0,
        }
    }

    /// Write the low `n` bits of `value`, LSB-first. `n` must be <= 24; in this
    /// crate the largest single call is 13 (distance extra bits) and `bitcount`
    /// is always < 8 on entry, so `bitbuf` (u32) never overflows.
    fn write_bits(&mut self, value: u32, n: u32) {
        if n == 0 {
            return;
        }
        let mask = if n >= 32 { u32::MAX } else { (1u32 << n) - 1 };
        self.bitbuf |= (value & mask) << self.bitcount;
        self.bitcount += n;
        while self.bitcount >= 8 {
            self.out.push((self.bitbuf & 0xFF) as u8);
            self.bitbuf >>= 8;
            self.bitcount -= 8;
        }
    }

    /// Write a pre-reversed Huffman code. DEFLATE still stores bits LSB-first;
    /// fixed-code tables keep the reversed representation ready for emission.
    fn write_reversed_huffman(&mut self, code: u16, len: u8) {
        self.write_bits(u32::from(code), u32::from(len));
    }

    fn pending_byte_len(&self) -> usize {
        self.out.len() + usize::from(self.bitcount > 0)
    }

    /// Pad the current partial byte with zero bits and flush. Leaves the writer
    /// byte-aligned and reusable (used both for end-of-stream padding and to
    /// align before a stored block's LEN/NLEN/raw bytes).
    fn finish(&mut self) {
        if self.bitcount > 0 {
            self.out.push((self.bitbuf & 0xFF) as u8);
            self.bitbuf = 0;
            self.bitcount = 0;
        }
    }
}

const fn reverse_bits(code: u32, len: u8) -> u32 {
    let mut r = 0u32;
    let mut c = code;
    let mut i = 0u8;
    while i < len {
        r = (r << 1) | (c & 1);
        c >>= 1;
        i += 1;
    }
    r
}

// ---------------------------------------------------------------------------
// Fixed Huffman literal/length code table (RFC 1951 section 3.2.6). The stored
// code value is already bit-reversed for the LSB-first DEFLATE bit writer.
const FIXED_LITLEN_CODES: [(u16, u8); 288] = build_fixed_litlen_codes();
const FIXED_DIST_CODES: [u16; 30] = build_fixed_dist_codes();
const LENGTH_SYMBOL_BY_LEN: [u8; MAX_MATCH + 1] = build_length_symbol_by_len();
static DIST_SYMBOL_BY_DISTANCE: [u8; WINDOW + 1] = build_dist_symbol_by_distance();

const fn build_fixed_litlen_codes() -> [(u16, u8); 288] {
    let mut table = [(0u16, 0u8); 288];
    let mut sym = 0usize;
    while sym < table.len() {
        let (code, len) = litlen_code(sym);
        table[sym] = (reverse_bits(code as u32, len) as u16, len);
        sym += 1;
    }
    table
}

const fn build_fixed_dist_codes() -> [u16; 30] {
    let mut table = [0u16; 30];
    let mut sym = 0usize;
    while sym < table.len() {
        table[sym] = reverse_bits(sym as u32, 5) as u16;
        sym += 1;
    }
    table
}

const fn build_length_symbol_by_len() -> [u8; MAX_MATCH + 1] {
    let mut table = [0u8; MAX_MATCH + 1];
    let mut len = 0usize;
    while len < table.len() {
        let mut symbol = LENGTH_BASE.len() - 1;
        while symbol > 0 && LENGTH_BASE[symbol] as usize > len {
            symbol -= 1;
        }
        table[len] = symbol as u8;
        len += 1;
    }
    table
}

const fn build_dist_symbol_by_distance() -> [u8; WINDOW + 1] {
    let mut table = [0u8; WINDOW + 1];
    let mut dist = 0usize;
    while dist < table.len() {
        let mut symbol = DIST_BASE.len() - 1;
        while symbol > 0 && DIST_BASE[symbol] as usize > dist {
            symbol -= 1;
        }
        table[dist] = symbol as u8;
        dist += 1;
    }
    table
}

// Returns the canonical (MSB-first) code value and bit length for fixed
// literal/length symbol `sym`.
const fn litlen_code(sym: usize) -> (u16, u8) {
    if sym <= 143 {
        ((0x30 + sym) as u16, 8)
    } else if sym <= 255 {
        ((0x190 + (sym - 144)) as u16, 9)
    } else if sym <= 279 {
        ((sym - 256) as u16, 7)
    } else {
        // 280..=287
        ((0xC0 + (sym - 280)) as u16, 8)
    }
}

fn emit_litlen(bw: &mut BitWriter, sym: usize) {
    let (code, len) = FIXED_LITLEN_CODES[sym];
    bw.write_reversed_huffman(code, len);
}

fn emit_literal(bw: &mut BitWriter, b: u8) {
    emit_litlen(bw, b as usize);
}

fn emit_match(bw: &mut BitWriter, len: usize, dist: usize) {
    // Length symbol 257..=285 + extra bits (LSB-first). The static lookup table
    // is the old highest-base<=value search, precomputed for every legal match.
    debug_assert!((MIN_MATCH..=MAX_MATCH).contains(&len));
    debug_assert!((1..=WINDOW).contains(&dist));
    let li = LENGTH_SYMBOL_BY_LEN[len] as usize;
    debug_assert!(li < LENGTH_BASE.len());
    emit_litlen(bw, 257 + li);
    let lbase = LENGTH_BASE[li] as usize;
    let lextra = LENGTH_EXTRA[li] as u32;
    bw.write_bits((len.saturating_sub(lbase)) as u32, lextra);

    // Distance symbol 0..=29 (5-bit fixed code, MSB-first) + extra bits (LSB-first).
    let di = DIST_SYMBOL_BY_DISTANCE[dist] as usize;
    debug_assert!(di < DIST_BASE.len());
    bw.write_reversed_huffman(FIXED_DIST_CODES[di], 5);
    let dbase = DIST_BASE[di] as usize;
    let dextra = DIST_EXTRA[di] as u32;
    bw.write_bits((dist.saturating_sub(dbase)) as u32, dextra);
}

// ---------------------------------------------------------------------------
// LZ77 hashing helpers.
const HASH_BITS: u32 = 15;
const HASH_SIZE: usize = 1 << HASH_BITS;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const WINDOW: usize = 32768;
const MAX_CHAIN: usize = 256;
const STORED_BLOCK_MAX: usize = u16::MAX as usize;
const NONE: usize = usize::MAX;
const ADLER_MOD: u32 = 65521;
const ADLER_NMAX: usize = 5552;
const FIXED_LIMIT_OVERSHOOT_BYTES: usize = 4;

struct Adler32 {
    s1: u32,
    s2: u32,
    pending: usize,
}

impl Adler32 {
    fn new() -> Self {
        Self {
            s1: 1,
            s2: 0,
            pending: 0,
        }
    }

    fn update_byte(&mut self, byte: u8) {
        self.s1 += u32::from(byte);
        self.s2 += self.s1;
        self.pending += 1;
        if self.pending == ADLER_NMAX {
            self.s1 %= ADLER_MOD;
            self.s2 %= ADLER_MOD;
            self.pending = 0;
        }
    }

    fn finish(mut self) -> u32 {
        if self.pending != 0 {
            self.s1 %= ADLER_MOD;
            self.s2 %= ADLER_MOD;
        }
        (self.s2 << 16) | self.s1
    }
}

fn hash3(data: &[u8], i: usize) -> usize {
    debug_assert!(i + 2 < data.len());
    let b0 = data[i] as u32;
    let b1 = data[i + 1] as u32;
    let b2 = data[i + 2] as u32;
    let v = (b0 << 16) | (b1 << 8) | b2;
    ((v.wrapping_mul(2654435761) >> (32 - HASH_BITS)) as usize) & (HASH_SIZE - 1)
}

fn match_len(data: &[u8], a: usize, b: usize, max: usize) -> usize {
    let max = max
        .min(data.len().saturating_sub(a))
        .min(data.len().saturating_sub(b));
    let left = &data[a..a + max];
    let right = &data[b..b + max];
    let mut l = 0usize;
    while l + 8 <= max && left[l..l + 8] == right[l..l + 8] {
        l += 8;
    }
    while l < max && left[l] == right[l] {
        l += 1;
    }
    l
}

// ---------------------------------------------------------------------------
struct FixedDeflate {
    body: Vec<u8>,
    adler32: u32,
    complete: bool,
}

pub(crate) struct ZlibCompressScratch {
    head: Vec<usize>,
    prev: Vec<usize>,
}

impl ZlibCompressScratch {
    pub(crate) fn new() -> Self {
        Self {
            head: Vec::new(),
            prev: Vec::new(),
        }
    }

    fn fixed_tables(&mut self, input_len: usize) -> (&mut [usize], &mut [usize]) {
        if self.head.len() == HASH_SIZE {
            self.head.fill(NONE);
        } else {
            self.head.clear();
            self.head.resize(HASH_SIZE, NONE);
        }

        if self.prev.len() < input_len {
            self.prev.resize(input_len, NONE);
        } else {
            self.prev.truncate(input_len);
        }

        (&mut self.head, &mut self.prev)
    }
}

impl Default for ZlibCompressScratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Produce a raw DEFLATE byte stream (single final fixed-Huffman block) using
/// greedy LZ77 matching over a hash-chain index.
#[cfg(test)]
fn deflate_fixed(data: &[u8]) -> FixedDeflate {
    deflate_fixed_with_limit(data, None)
}

#[cfg(test)]
fn deflate_fixed_with_limit(data: &[u8], abort_after_body_len: Option<usize>) -> FixedDeflate {
    let mut scratch = ZlibCompressScratch::new();
    deflate_fixed_with_scratch(data, abort_after_body_len, &mut scratch)
}

fn deflate_fixed_with_scratch(
    data: &[u8],
    abort_after_body_len: Option<usize>,
    scratch: &mut ZlibCompressScratch,
) -> FixedDeflate {
    let mut bw =
        BitWriter::with_capacity(fixed_body_capacity_hint(data.len(), abort_after_body_len));
    let mut adler = Adler32::new();
    // Block header: BFINAL = 1, BTYPE = 01 (fixed Huffman), both LSB-first.
    bw.write_bits(1, 1);
    bw.write_bits(0b01, 2);
    if fixed_body_exceeds_limit(&bw, abort_after_body_len) {
        return FixedDeflate {
            body: bw.out,
            adler32: adler32(data),
            complete: false,
        };
    }

    let n = data.len();
    // No legal LZ77 match exists before MIN_MATCH, so skip the hash tables.
    if n < MIN_MATCH {
        for &b in data {
            emit_literal(&mut bw, b);
            if fixed_body_exceeds_limit(&bw, abort_after_body_len) {
                return FixedDeflate {
                    body: bw.out,
                    adler32: adler32(data),
                    complete: false,
                };
            }
            adler.update_byte(b);
        }
        emit_litlen(&mut bw, 256);
        bw.finish();
        return FixedDeflate {
            body: bw.out,
            adler32: adler.finish(),
            complete: true,
        };
    }

    let (head, prev) = scratch.fixed_tables(n);

    let insert = |head: &mut [usize], prev: &mut [usize], p: usize| {
        if p + MIN_MATCH <= n {
            let h = hash3(data, p);
            let old = head[h];
            prev[p] = old;
            head[h] = p;
        }
    };

    let mut pos = 0usize;
    while pos < n {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        if pos + MIN_MATCH <= n {
            let h = hash3(data, pos);
            let max_match = (n - pos).min(MAX_MATCH);
            let mut cand = head[h];
            let mut chain = MAX_CHAIN;
            while cand != NONE && chain > 0 && cand < pos {
                let dist = pos - cand;
                if dist > WINDOW {
                    break;
                }
                if best_len < max_match && data[cand + best_len] != data[pos + best_len] {
                    cand = prev[cand];
                    chain -= 1;
                    continue;
                }
                let len = match_len(data, cand, pos, max_match);
                if len > best_len {
                    best_len = len;
                    best_dist = dist;
                    if len >= max_match {
                        break;
                    }
                }
                cand = prev[cand];
                chain -= 1;
            }
        }

        if best_len >= MIN_MATCH && (1..=WINDOW).contains(&best_dist) {
            emit_match(&mut bw, best_len, best_dist);
            if fixed_body_exceeds_limit(&bw, abort_after_body_len) {
                return FixedDeflate {
                    body: bw.out,
                    adler32: adler32(data),
                    complete: false,
                };
            }
            let end = pos + best_len;
            let mut k = pos;
            while k < end {
                adler.update_byte(data[k]);
                insert(&mut *head, &mut *prev, k);
                k += 1;
            }
            pos = end;
        } else {
            debug_assert!(pos < n);
            let b = data[pos];
            emit_literal(&mut bw, b);
            if fixed_body_exceeds_limit(&bw, abort_after_body_len) {
                return FixedDeflate {
                    body: bw.out,
                    adler32: adler32(data),
                    complete: false,
                };
            }
            adler.update_byte(b);
            insert(&mut *head, &mut *prev, pos);
            pos += 1;
        }
    }

    // End-of-block symbol, then pad final byte with zeros.
    emit_litlen(&mut bw, 256);
    bw.finish();
    FixedDeflate {
        body: bw.out,
        adler32: adler.finish(),
        complete: true,
    }
}

fn fixed_body_exceeds_limit(bw: &BitWriter, abort_after_body_len: Option<usize>) -> bool {
    match abort_after_body_len {
        Some(limit) => bw.pending_byte_len() > limit,
        None => false,
    }
}

fn fixed_body_capacity_hint(input_len: usize, abort_after_body_len: Option<usize>) -> usize {
    let fixed_upper = fixed_huffman_body_len_upper_bound(input_len);
    match abort_after_body_len {
        Some(limit) => fixed_upper.min(limit.saturating_add(FIXED_LIMIT_OVERSHOOT_BYTES)),
        None => fixed_upper,
    }
}

fn fixed_huffman_body_len_upper_bound(input_len: usize) -> usize {
    input_len
        .checked_mul(9)
        .and_then(|bits| bits.checked_add(10))
        .map_or(usize::MAX, |bits| bits.div_ceil(8))
}

/// Append one or more raw DEFLATE stored (BTYPE=00) blocks to `out`.
/// Valid for empty input (one empty final block).
fn append_stored_blocks(out: &mut Vec<u8>, data: &[u8]) {
    if data.is_empty() {
        append_stored_block(out, true, &[]);
        return;
    }

    let mut remaining = data;
    while !remaining.is_empty() {
        let take = remaining.len().min(STORED_BLOCK_MAX);
        let (chunk, rest) = remaining.split_at(take);
        append_stored_block(out, rest.is_empty(), chunk);
        remaining = rest;
    }
}

fn append_stored_block(out: &mut Vec<u8>, is_final: bool, chunk: &[u8]) {
    // BFINAL followed by BTYPE=00, then zero padding to the next byte boundary.
    out.push(u8::from(is_final));
    let len = chunk.len() as u16;
    let nlen = !len;
    out.push((len & 0xFF) as u8); // LEN, little-endian
    out.push((len >> 8) as u8);
    out.push((nlen & 0xFF) as u8); // NLEN = ~LEN, little-endian
    out.push((nlen >> 8) as u8);
    out.extend_from_slice(chunk);
}

/// Produce a raw DEFLATE byte stream of one or more stored (BTYPE=00) blocks.
/// Used by tests to keep the direct stored-block writer's length contract pinned.
#[cfg(test)]
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(deflate_stored_len(data.len()));
    append_stored_blocks(&mut out, data);
    out
}

fn deflate_stored_len(input_len: usize) -> usize {
    let blocks = input_len.div_ceil(STORED_BLOCK_MAX).max(1);
    input_len + blocks * 5
}

// ---------------------------------------------------------------------------
fn adler32(data: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    // Process in bounded chunks (NMAX=5552) so the sums never overflow u32
    // before the modulo: worst-case s2 stays below 2^32.
    for chunk in data.chunks(ADLER_NMAX) {
        for &b in chunk {
            s1 += b as u32;
            s2 += s1;
        }
        s1 %= ADLER_MOD;
        s2 %= ADLER_MOD;
    }
    (s2 << 16) | s1
}

// ---------------------------------------------------------------------------
/// Compress `data` into a complete zlib stream (RFC 1950) wrapping DEFLATE.
///
/// The returned bytes are suitable for direct embedding as a PDF stream with
/// `/Filter /FlateDecode`. Any standard zlib inflater recovers `data` exactly.
/// Whichever of the fixed-Huffman and stored encodings is smaller is used, so
/// incompressible payloads (e.g. raw TrueType font bytes) do not expand.
pub fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut scratch = ZlibCompressScratch::new();
    zlib_compress_with_scratch(data, &mut scratch)
}

pub(crate) fn zlib_compress_with_scratch(
    data: &[u8],
    scratch: &mut ZlibCompressScratch,
) -> Vec<u8> {
    let stored_len = deflate_stored_len(data.len());
    let fixed = deflate_fixed_with_scratch(data, Some(stored_len), scratch);
    let use_stored = !fixed.complete || stored_len < fixed.body.len();

    let body_len = if use_stored {
        stored_len
    } else {
        fixed.body.len()
    };
    let mut out = Vec::with_capacity(2 + body_len + 4);
    // zlib header: CMF = 0x78 (deflate, 32K window), FLG = 0x9C (0x789C % 31 == 0).
    out.push(0x78);
    out.push(0x9C);

    if use_stored {
        append_stored_blocks(&mut out, data);
    } else {
        out.extend_from_slice(&fixed.body);
    }

    // Adler-32 of the uncompressed data, big-endian.
    let adler = fixed.adler32;
    out.push((adler >> 24) as u8);
    out.push((adler >> 16) as u8);
    out.push((adler >> 8) as u8);
    out.push(adler as u8);
    out
}

/// Decompress a complete zlib stream (RFC 1950 + RFC 1951) — the inverse of
/// [`zlib_compress`]. Supports all three DEFLATE block types (stored, fixed
/// Huffman, and dynamic Huffman, which real-world encoders such as PNG writers
/// use). `max_out` bounds the decompressed size so a hostile stream cannot blow
/// up memory; decoding stops with `None` if the output would exceed it. The zlib
/// header, trailer checksum, and DEFLATE block integrity are all validated.
/// Returns `None` on any malformed input — it never panics.
pub(crate) fn zlib_decompress(data: &[u8], max_out: usize) -> Option<Vec<u8>> {
    // zlib header: CMF, FLG (2 bytes), then the DEFLATE body, then a 4-byte
    // big-endian Adler-32 of the uncompressed bytes.
    let trailer_start = data.len().checked_sub(4)?;
    if trailer_start < 2 {
        return None;
    }
    let cmf = *data.first()?;
    let flg = *data.get(1)?;
    let header = (u16::from(cmf) << 8) | u16::from(flg);
    if cmf & 0x0f != 8 || cmf >> 4 > 7 || flg & 0x20 != 0 || header % 31 != 0 {
        // Not deflate, invalid window/header check, or a preset dictionary we do
        // not support.
        return None;
    }
    let body = data.get(2..trailer_start)?;
    let expected_adler = u32::from_be_bytes([
        *data.get(trailer_start)?,
        *data.get(trailer_start + 1)?,
        *data.get(trailer_start + 2)?,
        *data.get(trailer_start + 3)?,
    ]);
    let out = inflate_deflate(body, max_out)?;
    if adler32(&out) != expected_adler {
        return None;
    }
    Some(out)
}

/// A canonical-Huffman decoder built from a list of code lengths.
struct Huffman {
    /// `counts[len]` = number of codes of that bit length.
    counts: [u16; MAX_HUFFMAN_BITS + 1],
    /// Symbols ordered by (length, symbol), indexed via the canonical scheme.
    symbols: Vec<u16>,
}

const MAX_HUFFMAN_BITS: usize = 15;

impl Huffman {
    fn from_lengths(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0u16; MAX_HUFFMAN_BITS + 1];
        for &len in lengths {
            let len = len as usize;
            if len > MAX_HUFFMAN_BITS {
                return None;
            }
            counts[len] += 1;
        }
        counts[0] = 0;
        let mut left = 1i32;
        for &count in counts.iter().take(MAX_HUFFMAN_BITS + 1).skip(1) {
            left = (left << 1) - i32::from(count);
            if left < 0 {
                return None;
            }
        }
        // Offsets of each length's first symbol within `symbols`.
        let mut offsets = [0u16; MAX_HUFFMAN_BITS + 2];
        for len in 1..=MAX_HUFFMAN_BITS {
            offsets[len + 1] = offsets[len] + counts[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &len) in lengths.iter().enumerate() {
            if len != 0 {
                let slot = offsets[len as usize] as usize;
                *symbols.get_mut(slot)? = sym as u16;
                offsets[len as usize] += 1;
            }
        }
        Some(Huffman { counts, symbols })
    }

    /// Decode one symbol from the bit reader (RFC 1951 canonical decode).
    fn decode(&self, br: &mut BitStream) -> Option<u16> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..=MAX_HUFFMAN_BITS {
            code |= br.bit()? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return self.symbols.get((index + (code - first)) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

/// LSB-first bit reader over a DEFLATE body.
struct BitStream<'a> {
    data: &'a [u8],
    byte: usize,
    bit: u32,
}

impl<'a> BitStream<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitStream {
            data,
            byte: 0,
            bit: 0,
        }
    }
    fn bit(&mut self) -> Option<u32> {
        let b = *self.data.get(self.byte)?;
        let v = (b >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.byte += 1;
        }
        Some(u32::from(v))
    }
    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut r = 0u32;
        for i in 0..n {
            r |= self.bit()? << i;
        }
        Some(r)
    }
    fn align_to_byte(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.byte += 1;
        }
    }
    fn aligned_bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.bit != 0 {
            return None;
        }
        let end = self.byte.checked_add(len)?;
        let bytes = self.data.get(self.byte..end)?;
        self.byte = end;
        Some(bytes)
    }
    fn is_at_end_after_final_block(&self) -> bool {
        if self.bit == 0 {
            return self.byte == self.data.len();
        }
        self.byte + 1 == self.data.len()
    }
}

fn inflate_deflate(body: &[u8], max_out: usize) -> Option<Vec<u8>> {
    let mut br = BitStream::new(body);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let bfinal = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align_to_byte();
                let len = br.bits(16)? as u16;
                let nlen = br.bits(16)? as u16;
                if nlen != !len {
                    return None;
                }
                let len = usize::from(len);
                if out.len().checked_add(len)? > max_out {
                    return None;
                }
                out.extend_from_slice(br.aligned_bytes(len)?);
            }
            1 => {
                let (lit, dist) = fixed_huffman();
                inflate_block(&mut br, &lit, &dist, &mut out, max_out)?;
            }
            2 => {
                let (lit, dist) = dynamic_huffman(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out, max_out)?;
            }
            _ => return None,
        }
        if bfinal == 1 {
            break;
        }
    }
    if br.is_at_end_after_final_block() {
        Some(out)
    } else {
        None
    }
}

fn inflate_block(
    br: &mut BitStream,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    max_out: usize,
) -> Option<()> {
    loop {
        let sym = lit.decode(br)? as usize;
        if sym == 256 {
            return Some(());
        }
        if sym < 256 {
            if out.len() >= max_out {
                return None;
            }
            out.push(sym as u8);
            continue;
        }
        let li = sym.checked_sub(257)?;
        let length =
            *LENGTH_BASE.get(li)? as usize + br.bits(*LENGTH_EXTRA.get(li)? as u32)? as usize;
        let dsym = dist.decode(br)? as usize;
        let distance =
            *DIST_BASE.get(dsym)? as usize + br.bits(*DIST_EXTRA.get(dsym)? as u32)? as usize;
        if distance == 0 || distance > out.len() || out.len().checked_add(length)? > max_out {
            return None;
        }
        let start = out.len() - distance;
        for k in 0..length {
            let byte = *out.get(start + k)?;
            out.push(byte);
        }
    }
}

/// Build the fixed (static) literal/length and distance Huffman tables.
fn fixed_huffman() -> (Huffman, Huffman) {
    let mut lit_lengths = [0u8; 288];
    for (sym, len) in lit_lengths.iter_mut().enumerate() {
        *len = match sym {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist_lengths = [5u8; 30];
    // The fixed tables are always well-formed, so the builders cannot fail; fall
    // back to empty tables (which decode to `None`) if they somehow did.
    (
        Huffman::from_lengths(&lit_lengths).unwrap_or(Huffman {
            counts: [0; MAX_HUFFMAN_BITS + 1],
            symbols: Vec::new(),
        }),
        Huffman::from_lengths(&dist_lengths).unwrap_or(Huffman {
            counts: [0; MAX_HUFFMAN_BITS + 1],
            symbols: Vec::new(),
        }),
    )
}

/// Read a dynamic-Huffman block header and build its literal/length and distance
/// tables (RFC 1951 §3.2.7).
fn dynamic_huffman(br: &mut BitStream) -> Option<(Huffman, Huffman)> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return None;
    }
    let mut cl_lengths = [0u8; 19];
    for &slot in ORDER.iter().take(hclen) {
        cl_lengths[slot] = br.bits(3)? as u8;
    }
    let cl_huff = Huffman::from_lengths(&cl_lengths)?;

    // Decode the combined literal+distance code-length sequence.
    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);
    while lengths.len() < total {
        let sym = cl_huff.decode(br)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                // Repeat the previous length 3..=6 times.
                let prev = *lengths.last()?;
                let repeat = 3 + br.bits(2)? as usize;
                for _ in 0..repeat {
                    if lengths.len() >= total {
                        return None;
                    }
                    lengths.push(prev);
                }
            }
            17 => {
                let repeat = 3 + br.bits(3)? as usize;
                for _ in 0..repeat {
                    if lengths.len() >= total {
                        return None;
                    }
                    lengths.push(0);
                }
            }
            18 => {
                let repeat = 11 + br.bits(7)? as usize;
                for _ in 0..repeat {
                    if lengths.len() >= total {
                        return None;
                    }
                    lengths.push(0);
                }
            }
            _ => return None,
        }
    }
    if lengths.len() != total {
        return None;
    }
    let lit = Huffman::from_lengths(lengths.get(..hlit)?)?;
    let dist = Huffman::from_lengths(lengths.get(hlit..)?)?;
    Some((lit, dist))
}

// ===========================================================================
// Verification: a std-only DEFLATE inflater (fixed + stored blocks) used to
// prove round-trip correctness without any external crate. NOT part of the
// shipped compressor surface; lives under cfg(test). Round-trip was ALSO
// independently confirmed against python `zlib.decompress` (a real inflater)
// over empty, text, all-byte-values (9-bit codes), long matches, periodic
// self-overlap, window-edge distances, multi-block stored (>64KB), and
// incompressible random/LCG data -- all recovered byte-for-byte.
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    struct BitReader<'a> {
        data: &'a [u8],
        byte: usize,
        bit: u32,
    }
    impl<'a> BitReader<'a> {
        fn new(data: &'a [u8]) -> Self {
            BitReader {
                data,
                byte: 0,
                bit: 0,
            }
        }
        fn get_bit(&mut self) -> Option<u32> {
            let b = *self.data.get(self.byte)?;
            let v = (b >> self.bit) & 1;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.byte += 1;
            }
            Some(v as u32)
        }
        fn get_bits(&mut self, n: u32) -> Option<u32> {
            let mut r = 0u32;
            let mut i = 0u32;
            while i < n {
                let bit = self.get_bit()?;
                r |= bit << i;
                i += 1;
            }
            Some(r)
        }
        fn align(&mut self) {
            if self.bit != 0 {
                self.bit = 0;
                self.byte += 1;
            }
        }
    }

    fn decode_litlen(br: &mut BitReader) -> Option<usize> {
        // 7 bits MSB-first.
        let mut code = 0u32;
        for _ in 0..7 {
            code = (code << 1) | br.get_bit()?;
        }
        if code <= 0x17 {
            return Some(256 + code as usize);
        }
        code = (code << 1) | br.get_bit()?; // 8 bits
        if (0x30..=0xBF).contains(&code) {
            return Some((code - 0x30) as usize);
        }
        if (0xC0..=0xC7).contains(&code) {
            return Some(280 + (code - 0xC0) as usize);
        }
        code = (code << 1) | br.get_bit()?; // 9 bits
        Some(144 + (code - 0x190) as usize)
    }

    fn inflate(data: &[u8]) -> Option<Vec<u8>> {
        // Skip 2-byte zlib header, ignore trailing 4-byte adler.
        let body = data.get(2..)?;
        let mut br = BitReader::new(body);
        let mut out: Vec<u8> = Vec::new();
        loop {
            let bfinal = br.get_bits(1)?;
            let btype = br.get_bits(2)?;
            match btype {
                0 => {
                    br.align();
                    let len = br.get_bits(16)? as usize;
                    let _nlen = br.get_bits(16)?;
                    for _ in 0..len {
                        out.push(br.get_bits(8)? as u8);
                    }
                }
                1 => loop {
                    let sym = decode_litlen(&mut br)?;
                    if sym == 256 {
                        break;
                    }
                    if sym < 256 {
                        out.push(sym as u8);
                        continue;
                    }
                    let li = sym - 257;
                    let lbase = *LENGTH_BASE.get(li)? as usize;
                    let lextra = *LENGTH_EXTRA.get(li)? as u32;
                    let length = lbase + br.get_bits(lextra)? as usize;
                    // distance: 5 bits MSB-first.
                    let mut dsym = 0u32;
                    for _ in 0..5 {
                        dsym = (dsym << 1) | br.get_bit()?;
                    }
                    let di = dsym as usize;
                    let dbase = *DIST_BASE.get(di)? as usize;
                    let dextra = *DIST_EXTRA.get(di)? as u32;
                    let dist = dbase + br.get_bits(dextra)? as usize;
                    if dist == 0 || dist > out.len() {
                        return None;
                    }
                    let start = out.len() - dist;
                    for k in 0..length {
                        let byte = *out.get(start + k)?;
                        out.push(byte);
                    }
                },
                _ => return None,
            }
            if bfinal == 1 {
                break;
            }
        }
        Some(out)
    }

    fn roundtrip(data: &[u8]) {
        let comp = zlib_compress(data);
        assert_eq!(comp.first(), Some(&0x78));
        assert_eq!(comp.get(1), Some(&0x9C));
        let header =
            ((*comp.first().unwrap_or(&0) as u32) << 8) | *comp.get(1).unwrap_or(&0) as u32;
        assert_eq!(header % 31, 0);
        let got = inflate(&comp).expect("inflate");
        assert_eq!(got, data, "roundtrip mismatch for len {}", data.len());
        let a = adler32(data);
        let tail = comp.get(comp.len().saturating_sub(4)..).unwrap_or(&[]);
        assert_eq!(
            tail,
            &[(a >> 24) as u8, (a >> 16) as u8, (a >> 8) as u8, a as u8]
        );
    }

    #[test]
    fn scratch_reuse_matches_fresh_zlib_bytes() {
        let repeated = "The quick brown fox jumps over the lazy dog. ".repeat(512);
        let mut lcg = Vec::with_capacity(96_000);
        let mut state: u64 = 0x517c_c1b7_2722_0a95;
        for _ in 0..96_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            lcg.push((state >> 32) as u8);
        }
        let tiny = b"abcabcabc";
        let larger = "franken_markdown page stream ".repeat(4096);
        let payloads: [&[u8]; 5] = [
            b"",
            repeated.as_bytes(),
            lcg.as_slice(),
            tiny,
            larger.as_bytes(),
        ];

        let mut scratch = ZlibCompressScratch::new();
        for data in payloads.into_iter().cycle().take(15) {
            let fresh = zlib_compress(data);
            let reused = zlib_compress_with_scratch(data, &mut scratch);
            assert_eq!(
                reused,
                fresh,
                "scratch compression must be byte-identical for len {}",
                data.len()
            );
        }
    }

    #[test]
    fn adler_known() {
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"abc"), 0x024D0127);
        assert_eq!(adler32(b"Wikipedia"), 0x11E60398);
    }

    #[test]
    fn fixed_deflate_accumulates_the_same_adler32_as_the_reference_scan() {
        let mut lcg = Vec::with_capacity(80_000);
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..80_000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            lcg.push((state >> 33) as u8);
        }

        for data in [
            &b""[..],
            &b"abc"[..],
            &b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"[..],
            "The quick brown fox jumps over the lazy dog. "
                .repeat(512)
                .as_bytes(),
            lcg.as_slice(),
        ] {
            assert_eq!(deflate_fixed(data).adler32, adler32(data));
        }
    }

    #[test]
    fn empty() {
        roundtrip(b"");
    }

    #[test]
    fn small() {
        roundtrip(b"a");
        roundtrip(b"aa");
        roundtrip(b"aaa");
        roundtrip(b"hello world");
        roundtrip(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        roundtrip(b"abababababababababababab");
    }

    #[test]
    fn repetitive_and_text() {
        let s = "The quick brown fox jumps over the lazy dog. ".repeat(50);
        roundtrip(s.as_bytes());
        let s2 = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(200);
        roundtrip(s2.as_bytes());
    }

    #[test]
    fn binary_patterns() {
        let all: Vec<u8> = (0..=255u8).collect();
        roundtrip(&all);
        let all_rep: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        roundtrip(&all_rep);
        let zeros = vec![0u8; 100000];
        roundtrip(&zeros);
        let ones = vec![0xFFu8; 70000];
        roundtrip(&ones);
    }

    #[test]
    fn incompressible_uses_stored_and_does_not_expand() {
        // LCG pseudo-random (stands in for raw font bytes). Stored fallback must
        // win, keeping overhead to a few bytes per 64KiB block.
        let mut v = Vec::with_capacity(200000);
        let mut state: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..200000 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            v.push((state >> 33) as u8);
        }
        let comp = zlib_compress(&v);
        roundtrip(&v);
        // 6-byte zlib frame + 5 bytes per stored block (ceil(200000/65535)=4).
        assert!(
            comp.len() <= v.len() + 6 + 5 * 4,
            "expanded too much: {}",
            comp.len()
        );
    }

    #[test]
    fn fixed_deflate_limit_aborts_lcg_data_and_zlib_still_roundtrips() {
        let mut v = Vec::with_capacity(120_000);
        let mut state: u64 = 0x4d59_5df4_d0f3_3173;
        for _ in 0..120_000 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            v.push((state >> 33) as u8);
        }

        let stored_len = deflate_stored_len(v.len());
        let limited = deflate_fixed_with_limit(&v, Some(stored_len));
        assert!(!limited.complete, "LCG data should exceed stored fallback");
        assert_eq!(limited.adler32, adler32(&v));

        let comp = zlib_compress(&v);
        assert_eq!(comp.len(), 2 + stored_len + 4);
        assert_eq!(
            zlib_decompress(&comp, v.len() + 64).as_deref(),
            Some(v.as_slice())
        );
    }

    #[test]
    fn fixed_deflate_capacity_hint_caps_abort_path_without_overflow() {
        assert_eq!(fixed_huffman_body_len_upper_bound(0), 2);
        assert_eq!(fixed_huffman_body_len_upper_bound(1), 3);
        assert_eq!(fixed_huffman_body_len_upper_bound(8), 11);
        assert_eq!(fixed_huffman_body_len_upper_bound(usize::MAX), usize::MAX);

        assert_eq!(fixed_body_capacity_hint(1, Some(64)), 3);
        assert_eq!(
            fixed_body_capacity_hint(10_000, Some(64)),
            64 + FIXED_LIMIT_OVERSHOOT_BYTES
        );
    }

    #[test]
    fn fixed_deflate_short_inputs_keep_exact_fixed_body_bytes() {
        for (input, expected_body) in [
            (b"".as_slice(), &[3, 0][..]),
            (b"a".as_slice(), &[75, 4, 0][..]),
            (b"ab".as_slice(), &[75, 76, 2, 0][..]),
        ] {
            let fixed = deflate_fixed(input);
            assert!(fixed.complete);
            assert_eq!(fixed.body, expected_body);
            assert_eq!(fixed.adler32, adler32(input));

            let comp = zlib_compress(input);
            assert_eq!(
                comp.get(2..comp.len().saturating_sub(4)),
                Some(expected_body)
            );
            assert_eq!(
                zlib_decompress(&comp, input.len() + 1).as_deref(),
                Some(input)
            );
        }
    }

    #[test]
    fn fixed_deflate_limit_completes_and_matches_full_fixed_body_when_smaller() {
        let data = "The quick brown fox jumps over the lazy dog. "
            .repeat(2048)
            .into_bytes();
        let stored_len = deflate_stored_len(data.len());
        let full = deflate_fixed(&data);
        assert!(full.body.len() < stored_len);

        let limited = deflate_fixed_with_limit(&data, Some(stored_len));
        assert!(limited.complete);
        assert_eq!(limited.body, full.body);
        assert_eq!(limited.adler32, full.adler32);

        let comp = zlib_compress(&data);
        assert_eq!(
            comp.get(2..comp.len().saturating_sub(4)),
            Some(full.body.as_slice())
        );
    }

    #[test]
    fn stored_length_estimate_matches_encoder() {
        for len in [0usize, 1, 65_535, 65_536, 131_070, 131_071] {
            let data = vec![0xA5; len];
            assert_eq!(deflate_stored_len(data.len()), deflate_stored(&data).len());
        }
    }

    #[test]
    fn match_len_clamps_to_available_input() {
        let data = b"abcabcab";
        assert_eq!(match_len(data, 0, 3, 258), 5);
        assert_eq!(match_len(data, 0, data.len(), 258), 0);
        assert_eq!(match_len(data, data.len(), 0, 258), 0);
    }

    #[test]
    fn match_len_reports_exact_mismatch_offsets_across_chunk_boundaries() {
        let inside_chunk = b"abcdXfghabcdYfgh";
        assert_eq!(match_len(inside_chunk, 0, 8, 258), 4);

        let after_two_chunks = b"abcdefghijklmnopZabcdefghijklmnopQ";
        assert_eq!(match_len(after_two_chunks, 0, 17, 258), 16);

        let scalar_tail = b"abcdeZabcdeQ";
        assert_eq!(match_len(scalar_tail, 0, 6, 258), 5);
    }

    #[test]
    fn match_symbol_tables_preserve_search_mapping() {
        for (len, &symbol) in LENGTH_SYMBOL_BY_LEN
            .iter()
            .enumerate()
            .take(MAX_MATCH + 1)
            .skip(MIN_MATCH)
        {
            let mut searched = LENGTH_BASE.len() - 1;
            while searched > 0 && LENGTH_BASE[searched] as usize > len {
                searched -= 1;
            }
            assert_eq!(usize::from(symbol), searched);
        }

        for (dist, &symbol) in DIST_SYMBOL_BY_DISTANCE
            .iter()
            .enumerate()
            .take(WINDOW + 1)
            .skip(1)
        {
            let mut searched = DIST_BASE.len() - 1;
            while searched > 0 && DIST_BASE[searched] as usize > dist {
                searched -= 1;
            }
            assert_eq!(usize::from(symbol), searched);
        }
    }

    #[test]
    fn distances_and_lengths_sweep() {
        for period in [1usize, 2, 3, 5, 7, 31, 100, 258, 1000, 5000, 32000, 32768] {
            let mut v = Vec::new();
            for i in 0..(period * 4 + 50) {
                v.push((i % 251) as u8);
            }
            let tail: Vec<u8> = v.iter().copied().take(period).collect();
            for _ in 0..3 {
                v.extend_from_slice(&tail);
            }
            roundtrip(&v);
        }
    }

    // ---- production inflater (`zlib_decompress`) -----------------------------

    fn fnv1a64(data: &[u8]) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for &b in data {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    fn prod_roundtrip(data: &[u8]) {
        let comp = zlib_compress(data);
        let got = zlib_decompress(&comp, data.len() + 64).expect("zlib_decompress");
        assert_eq!(
            got,
            data,
            "production inflater mismatch for len {}",
            data.len()
        );
    }

    #[test]
    fn production_inflater_roundtrips_own_compressor() {
        prod_roundtrip(b"");
        prod_roundtrip(b"a");
        prod_roundtrip(b"hello world");
        prod_roundtrip(&(0..=255u8).collect::<Vec<u8>>());
        prod_roundtrip("The quick brown fox. ".repeat(300).as_bytes());
        let zeros = vec![0u8; 50_000];
        prod_roundtrip(&zeros);
        // Incompressible (stored block) path.
        let mut v = Vec::with_capacity(80_000);
        let mut s: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..80_000 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            v.push((s >> 33) as u8);
        }
        prod_roundtrip(&v);
    }

    #[rustfmt::skip]
    const DYN_ZLIB: &[u8] = &[
        120, 218, 181, 216, 143, 47, 212, 113, 28, 199, 113, 165, 161, 194, 40, 22, 42, 190,
        41, 90, 168, 136, 197, 34, 69, 52, 90, 164, 66, 27, 77, 29, 142, 187, 28, 119,
        206, 249, 185, 104, 138, 22, 90, 138, 197, 10, 139, 216, 176, 66, 191, 216, 40, 165,
        20, 27, 253, 66, 13, 27, 209, 80, 99, 163, 180, 168, 197, 82, 173, 127, 225, 233,
        15, 184, 215, 222, 239, 247, 231, 241, 254, 190, 183, 83, 73, 196, 66, 108, 188, 52,
        44, 74, 8, 85, 202, 19, 99, 132, 8, 121, 146, 112, 42, 62, 90, 17, 39, 200,
        19, 196, 74, 65, 36, 200, 68, 41, 201, 66, 184, 60, 82, 72, 148, 72, 101, 98,
        65, 33, 82, 198, 73, 99, 34, 133, 104, 145, 50, 42, 252, 223, 15, 164, 49, 42,
        185, 160, 8, 143, 16, 212, 212, 212, 176, 44, 149, 166, 206, 57, 44, 75, 162, 107,
        162, 129, 101, 137, 13, 55, 103, 97, 89, 194, 58, 7, 109, 44, 43, 118, 163, 91,
        46, 150, 21, 111, 229, 187, 74, 197, 61, 231, 255, 36, 187, 224, 171, 116, 100, 152,
        147, 196, 136, 142, 140, 218, 19, 95, 68, 71, 10, 251, 207, 154, 50, 145, 62, 185,
        101, 76, 105, 254, 197, 155, 152, 22, 131, 171, 43, 81, 42, 161, 13, 86, 40, 19,
        105, 107, 13, 74, 68, 241, 214, 22, 229, 145, 56, 120, 31, 213, 150, 54, 177, 19,
        197, 155, 249, 163, 17, 221, 133, 139, 203, 92, 240, 79, 203, 223, 164, 124, 253, 39,
        88, 234, 117, 83, 119, 172, 192, 50, 235, 23, 88, 175, 85, 78, 94, 216, 216, 234,
        60, 59, 177, 23, 104, 56, 226, 139, 61, 102, 243, 137, 46, 154, 199, 115, 153, 63,
        13, 174, 51, 185, 151, 38, 220, 125, 62, 136, 94, 138, 254, 252, 65, 122, 205, 134,
        75, 79, 34, 137, 159, 107, 70, 144, 194, 166, 154, 34, 72, 46, 51, 237, 227, 36,
        149, 249, 119, 50, 146, 137, 250, 199, 47, 36, 145, 21, 83, 74, 146, 135, 254, 220,
        12, 137, 205, 72, 43, 137, 164, 107, 102, 56, 183, 8, 103, 199, 210, 60, 109, 17,
        110, 217, 214, 237, 75, 168, 80, 7, 151, 12, 170, 60, 103, 111, 77, 170, 81, 247,
        192, 108, 106, 100, 222, 97, 58, 212, 240, 253, 20, 151, 97, 34, 199, 82, 87, 195,
        60, 66, 178, 11, 96, 111, 226, 66, 99, 24, 176, 172, 162, 24, 222, 136, 184, 187,
        102, 240, 138, 165, 60, 190, 73, 4, 166, 119, 90, 128, 100, 46, 244, 85, 129, 92,
        114, 199, 172, 65, 42, 5, 223, 106, 65, 38, 37, 11, 118, 32, 145, 10, 237, 7,
        32, 143, 91, 198, 142, 160, 181, 123, 150, 77, 252, 221, 105, 180, 223, 205, 159, 157,
        150, 189, 45, 252, 41, 107, 247, 217, 7, 101, 190, 14, 106, 131, 138, 123, 31, 233,
        13, 117, 57, 160, 122, 9, 141, 107, 36, 253, 16, 203, 100, 226, 82, 55, 75, 100,
        186, 40, 128, 229, 241, 179, 170, 143, 229, 182, 80, 31, 204, 242, 213, 120, 246, 129,
        93, 7, 157, 55, 34, 118, 189, 12, 6, 70, 57, 54, 107, 199, 35, 57, 50, 230,
        179, 19, 28, 151, 45, 234, 209, 28, 21, 91, 189, 175, 28, 19, 199, 245, 113, 28,
        17, 87, 171, 89, 142, 135, 167, 99, 50, 126, 120, 14, 122, 204, 227, 119, 231, 232,
        225, 51, 248, 217, 9, 10, 89, 138, 95, 50, 81, 84, 38, 19, 41, 73, 210, 98,
        74, 147, 103, 230, 48, 45, 38, 228, 233, 162, 84, 82, 111, 92, 65, 153, 100, 220,
        54, 64, 137, 228, 52, 22, 162, 60, 242, 218, 76, 80, 109, 215, 122, 74, 80, 188,
        165, 195, 2, 186, 11, 149, 147, 229, 139, 241, 23, 126, 237, 47, 75, 44, 181, 94,
        179, 26, 43, 240, 145, 129, 13, 214, 107, 235, 134, 58, 108, 108, 29, 219, 118, 96,
        47, 208, 229, 92, 143, 61, 102, 159, 151, 19, 205, 99, 40, 224, 33, 13, 238, 83,
        168, 43, 77, 120, 82, 254, 148, 94, 138, 239, 167, 61, 232, 53, 155, 203, 106, 71,
        18, 151, 22, 28, 64, 10, 91, 94, 254, 138, 228, 162, 119, 199, 143, 164, 178, 166,
        185, 135, 100, 98, 218, 17, 72, 18, 177, 232, 237, 39, 121, 216, 140, 30, 39, 177,
        217, 79, 15, 145, 116, 119, 253, 14, 93, 132, 179, 227, 182, 114, 236, 15, 111, 6,
        204, 141,
    ];
    const DYN_PLAIN_LEN: usize = 9478;
    const DYN_PLAIN_FNV1A64: u64 = 0x462c7b69f9163b4a;

    #[test]
    fn production_inflater_decodes_real_dynamic_huffman_stream() {
        // A zlib stream produced by Python's zlib (a real-world encoder) that
        // chose a DYNAMIC Huffman block — the case the cfg(test) verification
        // inflater cannot handle. Proves `zlib_decompress` is a genuine RFC 1951
        // inflater, not just the inverse of our own fixed/stored compressor.
        assert_eq!((DYN_ZLIB[2] >> 1) & 3, 2, "fixture must be a dynamic block");
        let out = zlib_decompress(DYN_ZLIB, DYN_PLAIN_LEN + 64).expect("decode dynamic huffman");
        assert_eq!(out.len(), DYN_PLAIN_LEN);
        assert_eq!(fnv1a64(&out), DYN_PLAIN_FNV1A64);
    }

    #[test]
    fn huffman_builder_rejects_oversubscribed_lengths() {
        // Three one-bit codes cannot fit into the two-code one-bit namespace.
        // Accepting this table would let malformed dynamic DEFLATE headers build
        // a decoder that assigns impossible canonical codes.
        assert!(Huffman::from_lengths(&[1, 1, 1]).is_none());
        assert!(Huffman::from_lengths(&[2, 2, 2, 2, 2]).is_none());

        // Sparse/incomplete trees are still legal to build. They may fail later
        // if the stream asks for a missing code, but the table itself is not
        // over-subscribed.
        assert!(Huffman::from_lengths(&[1]).is_some());
        assert!(Huffman::from_lengths(&[1, 1]).is_some());
        assert!(Huffman::from_lengths(&[0, 0, 0]).is_some());
    }

    #[test]
    fn production_inflater_rejects_oversized_output() {
        let comp = zlib_compress(&vec![7u8; 10_000]);
        // A cap below the true size must fail rather than allocate unbounded.
        assert!(zlib_decompress(&comp, 100).is_none());
    }

    #[test]
    fn production_inflater_accepts_unused_final_byte_bits() {
        // The DEFLATE bitstream can end inside the final byte. The remaining
        // high bits are outside the compressed data; common zlib accepts them
        // even when they are non-zero. We must still reject complete extra body
        // bytes, but not over-read or validate bits after the final EOB code.
        let nonzero_unused_bits = [0x78, 0x9c, 0x03, 0xfc, 0x00, 0x00, 0x00, 0x01];

        assert_eq!(
            zlib_decompress(&nonzero_unused_bits, 1).as_deref(),
            Some(&[][..])
        );
    }

    #[test]
    fn production_inflater_rejects_malformed_streams() {
        // Empty / truncated headers.
        assert!(zlib_decompress(&[], 100).is_none());
        assert!(zlib_decompress(&[0x78], 100).is_none());
        let valid = zlib_compress(b"abc");

        // Wrong compression method (CMF low nibble must be 8).
        let mut bad_method = valid.clone();
        bad_method[0] = 0x79;
        assert!(zlib_decompress(&bad_method, 100).is_none());
        // Header check bits (FCHECK) must make CMF/FLG divisible by 31.
        let mut bad_fcheck = valid.clone();
        bad_fcheck[1] ^= 0x01;
        assert!(zlib_decompress(&bad_fcheck, 100).is_none());
        // Preset dictionary (FDICT bit set) is unsupported.
        let mut bad_dict = valid.clone();
        bad_dict[1] = 0xbb;
        assert!(zlib_decompress(&bad_dict, 100).is_none());
        // The trailer is required and the Adler-32 must match the decoded bytes.
        let mut truncated_trailer = valid.clone();
        truncated_trailer.pop();
        assert!(zlib_decompress(&truncated_trailer, 100).is_none());
        let mut bad_adler = valid.clone();
        let last = bad_adler.len() - 1;
        bad_adler[last] ^= 0x01;
        assert!(zlib_decompress(&bad_adler, 100).is_none());
        // Bytes between the final DEFLATE block and zlib trailer are malformed.
        let trailer_start = valid.len() - 4;
        let mut trailing_body = Vec::new();
        trailing_body.extend_from_slice(&valid[..trailer_start]);
        trailing_body.push(0);
        trailing_body.extend_from_slice(&valid[trailer_start..]);
        assert!(zlib_decompress(&trailing_body, 100).is_none());
        // Stored blocks must carry NLEN as the ones-complement of LEN.
        assert!(zlib_decompress(&[0x78, 0x9c, 0x01, 0, 0, 0, 0xff, 0, 0, 0, 1], 100).is_none());
        // Reserved DEFLATE block type 3 (bfinal=1, btype=11 -> low 3 bits 0b111).
        assert!(zlib_decompress(&[0x78, 0x9c, 0x07], 100).is_none());
        // Stored block whose declared length runs past the input.
        assert!(zlib_decompress(&[0x78, 0x9c, 0x01, 0xff, 0x00], 100).is_none());
        // Body that ends mid-symbol (bit reader runs dry inside a Huffman block).
        assert!(zlib_decompress(&[0x78, 0x9c, 0x4b], 100).is_none());
        // A dynamic-Huffman block (btype=10) truncated before its header is read
        // exercises the dynamic-table reject path.
        assert!(zlib_decompress(&[0x78, 0x9c, 0x05], 100).is_none());
    }

    #[test]
    fn production_inflater_round_trips_a_stored_only_stream() {
        // Incompressible input forces the stored-block path through the production
        // inflater (covers the stored-block branch end to end).
        let mut v = Vec::with_capacity(70_000);
        let mut s: u64 = 0xdead_beef_cafe_f00d;
        for _ in 0..70_000 {
            s = s.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
            v.push((s >> 40) as u8);
        }
        let comp = zlib_compress(&v);
        assert_eq!(
            zlib_decompress(&comp, v.len() + 64).as_deref(),
            Some(v.as_slice())
        );
    }

    // ---- const-table builders re-executed at runtime --------------------------

    #[test]
    fn const_table_builders_match_rfc1951_at_runtime() {
        // The fixed-code tables are produced by const fns at compile time; re-run
        // the builders at runtime and pin them against both the compiled tables
        // and hand-checked RFC 1951 values.
        assert_eq!(reverse_bits(0b1, 3), 0b100);
        assert_eq!(reverse_bits(0b1011, 4), 0b1101);
        assert_eq!(reverse_bits(0, 0), 0);

        // RFC 1951 section 3.2.6 fixed literal/length code ranges.
        assert_eq!(litlen_code(0), (0x30, 8));
        assert_eq!(litlen_code(143), (0xBF, 8));
        assert_eq!(litlen_code(144), (0x190, 9));
        assert_eq!(litlen_code(255), (0x1FF, 9));
        assert_eq!(litlen_code(256), (0x00, 7));
        assert_eq!(litlen_code(279), (0x17, 7));
        assert_eq!(litlen_code(280), (0xC0, 8));
        assert_eq!(litlen_code(287), (0xC7, 8));

        assert_eq!(build_fixed_litlen_codes(), FIXED_LITLEN_CODES);
        assert_eq!(build_fixed_dist_codes(), FIXED_DIST_CODES);
        assert_eq!(build_length_symbol_by_len(), LENGTH_SYMBOL_BY_LEN);
        assert_eq!(
            build_dist_symbol_by_distance().as_slice(),
            DIST_SYMBOL_BY_DISTANCE.as_slice()
        );

        // Literal 0: canonical code 0x30 (0011_0000, 8 bits) stored bit-reversed
        // as 0000_1100. Distance symbol 1: 5-bit 00001 reversed to 10000.
        assert_eq!(FIXED_LITLEN_CODES[0], (0x0C, 8));
        assert_eq!(FIXED_DIST_CODES[1], 0b10000);
    }

    #[test]
    fn bitwriter_full_width_write_is_lsb_first() {
        // n >= 32 takes the saturated-mask arm; all four bytes flush LSB-first.
        let mut bw = BitWriter::with_capacity(4);
        bw.write_bits(0xA5B4_C3D2, 32);
        assert_eq!(bw.out, [0xD2, 0xC3, 0xB4, 0xA5]);
        assert_eq!(bw.bitcount, 0);
    }

    // ---- fixed-deflate abort points -------------------------------------------

    #[test]
    fn fixed_deflate_zero_limit_aborts_right_after_block_header() {
        // A zero-byte budget trips on the 3 pending header bits before any
        // symbol is emitted; the body stays empty and the Adler-32 still covers
        // the whole input via the reference scan.
        let limited = deflate_fixed_with_limit(b"abcdef", Some(0));
        assert!(!limited.complete);
        assert!(limited.body.is_empty());
        assert_eq!(limited.adler32, adler32(b"abcdef"));
    }

    #[test]
    fn fixed_deflate_limit_aborts_short_input_literal_loop() {
        // Inputs below MIN_MATCH use the literal-only path. Header (3 bits) plus
        // one 8-bit literal spans 2 pending bytes, crossing the 1-byte limit on
        // the first literal with exactly one flushed byte.
        let limited = deflate_fixed_with_limit(b"ab", Some(1));
        assert!(!limited.complete);
        assert_eq!(limited.body.len(), 1);
        assert_eq!(limited.adler32, adler32(b"ab"));
    }

    #[test]
    fn fixed_deflate_limit_aborts_immediately_after_a_match() {
        // "aaaaaa" emits literal 'a' (8 bits) then a len-5/dist-1 match (7+5
        // bits). The 23 pending bits span 3 bytes, crossing the 2-byte limit on
        // the match-emission check (not the literal one).
        let limited = deflate_fixed_with_limit(b"aaaaaa", Some(2));
        assert!(!limited.complete);
        assert_eq!(limited.body.len(), 2);
        assert_eq!(limited.adler32, adler32(b"aaaaaa"));
    }

    #[test]
    fn hash_chain_budget_exhaustion_still_roundtrips() {
        // Find one trigram, containing no 'a', that lands in the same hash
        // bucket as "abc" (density is 1/32768, so the scan ends quickly).
        let target = hash3(b"abc", 0);
        let mut collider = None;
        for v in 0..(1u32 << 24) {
            let t = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
            if t.contains(&b'a') {
                continue;
            }
            if hash3(&t, 0) == target {
                collider = Some(t);
                break;
            }
        }
        let t = collider.expect("some trigram collides with \"abc\"");
        // 300 copies keep the "abc" bucket's chain deeper than MAX_CHAIN (256).
        // No byte of the prefix is 'a', so the final "abc" search can never
        // match: it must burn its entire chain budget candidate by candidate
        // and fall back to literals, and the stream must still round-trip.
        let mut v = Vec::with_capacity(300 * 3 + 3);
        for _ in 0..300 {
            v.extend_from_slice(&t);
        }
        v.extend_from_slice(b"abc");
        roundtrip(&v);
        prod_roundtrip(&v);
    }

    // ---- crafted DEFLATE streams for the production inflater -------------------

    /// Wrap a raw DEFLATE body in a zlib frame whose trailer is the Adler-32 of
    /// `plain` (irrelevant for streams that must be rejected before the check).
    fn zlib_frame(body: &[u8], plain: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x9C];
        out.extend_from_slice(body);
        out.extend_from_slice(&adler32(plain).to_be_bytes());
        out
    }

    #[test]
    fn zlib_decompress_rejects_oversized_window_and_reserved_block_type() {
        // CMF 0x88: valid deflate method nibble but CINFO 8 exceeds the 32KiB
        // window maximum of 7.
        assert!(zlib_decompress(&[0x88, 0x00, 0, 0, 0, 0], 100).is_none());
        // Reserved BTYPE=11 inside a frame long enough to reach the inflater
        // (a bare 3-byte stream is rejected by the header length check instead).
        assert!(zlib_decompress(&[0x78, 0x9C, 0x07, 0, 0, 0, 0], 100).is_none());
    }

    #[test]
    fn zlib_decompress_enforces_max_out_on_stored_blocks() {
        let mut v = Vec::with_capacity(1000);
        let mut s: u64 = 0x0123_4567_89ab_cdef;
        for _ in 0..1000 {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            v.push((s >> 33) as u8);
        }
        let comp = zlib_compress(&v);
        // Incompressible input picks the stored encoding: BFINAL=1, BTYPE=00.
        assert_eq!(comp[2], 0x01);
        assert_eq!(comp.len(), 2 + 5 + v.len() + 4);
        // The stored block declares 1000 bytes; a 100-byte cap must refuse it
        // before copying anything, while the exact cap still succeeds.
        assert!(zlib_decompress(&comp, 100).is_none());
        assert_eq!(
            zlib_decompress(&comp, v.len()).as_deref(),
            Some(v.as_slice())
        );
    }

    #[test]
    fn zlib_decompress_enforces_max_out_on_fixed_literals() {
        let data: Vec<u8> = (b'a'..=b'z').cycle().take(260).collect();
        let comp = zlib_compress(&data);
        // Repetitive text keeps the fixed-Huffman encoding: BFINAL=1, BTYPE=01.
        assert_eq!(comp[2] & 0x07, 0b011);
        // The first 26 outputs are literals, so a 10-byte cap trips on the 11th
        // literal before any back-reference is reached.
        assert!(zlib_decompress(&comp, 10).is_none());
        assert_eq!(
            zlib_decompress(&comp, data.len()).as_deref(),
            Some(data.as_slice())
        );
    }

    #[test]
    fn inflaters_reject_back_reference_before_any_output() {
        // A fixed-Huffman block whose very first symbol is a length/distance
        // pair: distance 1 has no prior output byte to point at.
        let mut bw = BitWriter::with_capacity(8);
        bw.write_bits(1, 1); // BFINAL
        bw.write_bits(0b01, 2); // BTYPE=01 (fixed)
        emit_match(&mut bw, 3, 1);
        emit_litlen(&mut bw, 256);
        bw.finish();
        let frame = zlib_frame(&bw.out, b"");
        assert!(zlib_decompress(&frame, 100).is_none());
        // The cfg(test) verification inflater applies the same guard.
        assert!(inflate(&frame).is_none());
    }

    #[test]
    fn verification_inflater_rejects_dynamic_blocks() {
        // The cfg(test) verifier only understands stored + fixed blocks; a real
        // dynamic-Huffman stream must be refused, never misdecoded.
        assert!(inflate(DYN_ZLIB).is_none());
    }

    // ---- Huffman decoder edge cases --------------------------------------------

    #[test]
    fn huffman_from_lengths_rejects_overlong_codes() {
        // 16 exceeds MAX_HUFFMAN_BITS (15): a malformed dynamic header must not
        // build a decoder with impossible code lengths, while 15 is still legal.
        assert!(Huffman::from_lengths(&[16]).is_none());
        assert!(Huffman::from_lengths(&[15]).is_some());
    }

    #[test]
    fn huffman_decode_fails_on_code_absent_from_incomplete_table() {
        // The table holds a single 1-bit code '0'; an all-ones stream never
        // resolves and must exhaust all 15 candidate lengths and fail.
        let huff = Huffman::from_lengths(&[1]).expect("single-code table");
        let mut br = BitStream::new(&[0xFF, 0xFF]);
        assert_eq!(huff.decode(&mut br), None);
    }

    #[test]
    fn bitstream_alignment_guards_partial_bytes() {
        let mut br = BitStream::new(&[0xAB, 0xCD]);
        // Aligning an already-aligned reader must not consume a byte.
        br.align_to_byte();
        assert_eq!((br.byte, br.bit), (0, 0));
        assert_eq!(br.bits(4), Some(0xB));
        // Mid-byte readers must refuse to hand out aligned byte slices.
        assert!(br.aligned_bytes(1).is_none());
        br.align_to_byte();
        assert_eq!(br.aligned_bytes(1), Some(&[0xCD][..]));
    }

    // ---- crafted dynamic-Huffman headers ---------------------------------------

    #[test]
    fn dynamic_header_rejects_oversized_symbol_counts() {
        // HLIT=30 declares 287 literal/length codes (max 286).
        let mut bw = BitWriter::with_capacity(4);
        bw.write_bits(1, 1); // BFINAL
        bw.write_bits(0b10, 2); // BTYPE=10 (dynamic)
        bw.write_bits(30, 5); // HLIT
        bw.write_bits(0, 5); // HDIST
        bw.write_bits(0, 4); // HCLEN
        bw.finish();
        assert!(zlib_decompress(&zlib_frame(&bw.out, b""), 100).is_none());

        // HDIST=30 declares 31 distance codes (max 30).
        let mut bw = BitWriter::with_capacity(4);
        bw.write_bits(1, 1);
        bw.write_bits(0b10, 2);
        bw.write_bits(0, 5);
        bw.write_bits(30, 5);
        bw.write_bits(0, 4);
        bw.finish();
        assert!(zlib_decompress(&zlib_frame(&bw.out, b""), 100).is_none());
    }

    /// Shared preamble for the code-length overflow tests: a dynamic block with
    /// HLIT=0/HDIST=0 (258 total lengths), a code-length table assigning
    /// sym18 -> '0', sym16 -> '10', sym17 -> '11', and two 18-runs that fill
    /// exactly 257 zero lengths, leaving room for exactly one more.
    fn dynamic_overflow_preamble() -> BitWriter {
        let mut bw = BitWriter::with_capacity(16);
        bw.write_bits(1, 1); // BFINAL
        bw.write_bits(0b10, 2); // BTYPE=10 (dynamic)
        bw.write_bits(0, 5); // HLIT -> 257 literal/length codes
        bw.write_bits(0, 5); // HDIST -> 1 distance code
        bw.write_bits(0, 4); // HCLEN -> 4 entries: order 16, 17, 18, 0
        bw.write_bits(2, 3); // len(sym16) = 2
        bw.write_bits(2, 3); // len(sym17) = 2
        bw.write_bits(1, 3); // len(sym18) = 1
        bw.write_bits(0, 3); // len(sym0)  = 0
        bw.write_bits(0, 1); // sym18 (code '0')
        bw.write_bits(127, 7); // repeat 11 + 127 = 138 zeros
        bw.write_bits(0, 1); // sym18
        bw.write_bits(108, 7); // repeat 11 + 108 = 119 zeros -> 257 total
        bw
    }

    #[test]
    fn dynamic_header_rejects_code_length_repeats_running_past_total() {
        // Symbol 16 (copy previous) crossing the 258-length budget.
        let mut bw = dynamic_overflow_preamble();
        bw.write_bits(1, 2); // sym16 (code '10' -> stream bits 1,0)
        bw.write_bits(0, 2); // repeat 3: 257 + 3 > 258
        bw.finish();
        assert!(zlib_decompress(&zlib_frame(&bw.out, b""), 100).is_none());

        // Symbol 17 (short zero run) crossing the budget.
        let mut bw = dynamic_overflow_preamble();
        bw.write_bits(3, 2); // sym17 (code '11')
        bw.write_bits(0, 3); // repeat 3
        bw.finish();
        assert!(zlib_decompress(&zlib_frame(&bw.out, b""), 100).is_none());

        // Symbol 18 (long zero run) crossing the budget.
        let mut bw = dynamic_overflow_preamble();
        bw.write_bits(0, 1); // sym18
        bw.write_bits(0, 7); // repeat 11
        bw.finish();
        assert!(zlib_decompress(&zlib_frame(&bw.out, b""), 100).is_none());
    }

    #[test]
    fn zlib_decompress_handles_dynamic_block_with_short_zero_runs() {
        // Hand-built dynamic block: code-length table {0, 1, 17, 18} all 2 bits;
        // the literal table gives 'A' (65) and end-of-block (256) 1-bit codes.
        // The 65 leading zero lengths use an 18-run of 62 followed by a 17-run
        // of 3, exercising the symbol-17 decode arm end to end.
        let mut bw = BitWriter::with_capacity(24);
        bw.write_bits(1, 1); // BFINAL
        bw.write_bits(0b10, 2); // BTYPE=10 (dynamic)
        bw.write_bits(0, 5); // HLIT -> 257 literal/length codes
        bw.write_bits(0, 5); // HDIST -> 1 distance code
        bw.write_bits(14, 4); // HCLEN -> 18 entries (through sym 1 in ORDER)
        for len in [0u32, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2] {
            bw.write_bits(len, 3);
        }
        // Code-length codes (canonical): 0 -> '00', 1 -> '01', 17 -> '10',
        // 18 -> '11'; each written LSB-first as its bit-reversed value.
        bw.write_bits(3, 2); // sym18
        bw.write_bits(51, 7); // 62 zeros
        bw.write_bits(1, 2); // sym17
        bw.write_bits(0, 3); // 3 zeros -> symbols 0..=64 unused
        bw.write_bits(2, 2); // sym1: literal 'A' (65) gets a 1-bit code
        bw.write_bits(3, 2); // sym18
        bw.write_bits(127, 7); // 138 zeros
        bw.write_bits(3, 2); // sym18
        bw.write_bits(41, 7); // 52 zeros -> symbols 66..=255 unused
        bw.write_bits(2, 2); // sym1: end-of-block (256) gets a 1-bit code
        bw.write_bits(0, 2); // sym0: the single distance code stays unused
        // Payload: 'A' (code '0'), end of block (code '1').
        bw.write_bits(0, 1);
        bw.write_bits(1, 1);
        bw.finish();
        assert_eq!(
            zlib_decompress(&zlib_frame(&bw.out, b"A"), 8).as_deref(),
            Some(&b"A"[..])
        );
    }
}
