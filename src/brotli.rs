//! Clean-room Brotli compressor and decompressor (RFC 7932).
//!
//! Implements Brotli compression and decompression tailored for WOFF2 font
//! table streams and general byte payloads. Follows the zero-dependency doctrine:
//! pure standard library Rust, no `unsafe`, and deterministic output.
//!
//! Output streams conform to RFC 7932 and decompress cleanly with standard
//! decoders (Google Brotli CLI, OTS, browsers).

use crate::{RenderError, Result};

const MAX_HUFFMAN_BITS: usize = 15;
const CODE_LENGTH_MAX_BITS: usize = 5;

// RFC 7932 Section 5: Insert length code extra bits and base values
const INSERT_EXTRA: [u8; 24] = [
    0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 10, 12, 14, 24,
];
const INSERT_BASE: [u32; 24] = [
    0, 1, 2, 3, 4, 5, 6, 8, 10, 14, 18, 26, 34, 50, 66, 98, 130, 194, 322, 578, 1090, 2114, 6210,
    22594,
];

// RFC 7932 Section 5: Copy length code extra bits and base values
const COPY_EXTRA: [u8; 24] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 7, 8, 9, 10, 24,
];
const COPY_BASE: [u32; 24] = [
    2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 18, 22, 30, 38, 54, 70, 102, 134, 198, 326, 582, 1094, 2118,
];

// RFC 7932 Section 3.5: Order of code lengths for the code length alphabet
const CL_ORDER: [usize; 18] = [1, 2, 3, 4, 0, 5, 17, 6, 16, 7, 8, 9, 10, 11, 12, 13, 14, 15];

// Fixed variable-length code for code lengths (RFC 7932 Section 3.5)
// (value, num_bits)
const CL_VLC: [(u32, u8); 6] = [
    (0, 2),  // 0 -> 00
    (7, 4),  // 1 -> 0111
    (3, 3),  // 2 -> 011
    (2, 2),  // 3 -> 10
    (1, 2),  // 4 -> 01
    (15, 4), // 5 -> 1111
];

// ---------------------------------------------------------------------------
// BitWriter (LSB-first)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct BitWriter {
    pub out: Vec<u8>,
    bitbuf: u64,
    bitcount: u32,
}

impl BitWriter {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            out: Vec::with_capacity(capacity),
            bitbuf: 0,
            bitcount: 0,
        }
    }

    #[inline(always)]
    pub fn write_bits(&mut self, value: u32, n: u32) {
        if n == 0 {
            return;
        }
        let mask = if n >= 32 { u32::MAX } else { (1u32 << n) - 1 };
        self.bitbuf |= u64::from(value & mask).wrapping_shl(self.bitcount);
        self.bitcount += n;
        while self.bitcount >= 8 {
            self.out.push((self.bitbuf & 0xFF) as u8);
            self.bitbuf >>= 8;
            self.bitcount -= 8;
        }
    }

    #[inline(always)]
    pub fn finish(&mut self) {
        if self.bitcount > 0 {
            self.out.push((self.bitbuf & 0xFF) as u8);
            self.bitbuf = 0;
            self.bitcount = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// BitReader (LSB-first)
// ---------------------------------------------------------------------------

struct BitReader<'a> {
    data: &'a [u8],
    bitpos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bitpos: 0 }
    }

    #[inline(always)]
    fn read_bits(&mut self, n: usize) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(RenderError::InvalidInput("brotli: read_bits > 32".into()));
        }
        let mut res = 0u32;
        for i in 0..n {
            let total_bit = self.bitpos + i;
            let byte_idx = total_bit >> 3;
            let bit_idx = total_bit & 7;
            let byte = *self.data.get(byte_idx).ok_or_else(|| {
                RenderError::InvalidInput("brotli: unexpected end of stream".into())
            })?;
            let bit = (byte >> bit_idx) & 1;
            res |= u32::from(bit) << i;
        }
        self.bitpos += n;
        Ok(res)
    }
}

// ---------------------------------------------------------------------------
// Canonical Huffman Table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct HuffmanTree {
    single_symbol: Option<u16>,
    counts: [u16; MAX_HUFFMAN_BITS + 1],
    symbols: Vec<u16>,
}

impl HuffmanTree {
    fn from_single(sym: u16) -> Self {
        Self {
            single_symbol: Some(sym),
            counts: [0; MAX_HUFFMAN_BITS + 1],
            symbols: Vec::new(),
        }
    }

    fn from_lengths(lengths: &[u8]) -> Result<Self> {
        let mut counts = [0u16; MAX_HUFFMAN_BITS + 1];
        for &len in lengths {
            let len = len as usize;
            if len > MAX_HUFFMAN_BITS {
                return Err(RenderError::InvalidInput(
                    "brotli: code length exceeds max".into(),
                ));
            }
            counts[len] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; MAX_HUFFMAN_BITS + 2];
        for len in 1..=MAX_HUFFMAN_BITS {
            offsets[len + 1] = offsets[len] + counts[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &len) in lengths.iter().enumerate() {
            if len != 0 {
                let slot = offsets[len as usize] as usize;
                if let Some(target) = symbols.get_mut(slot) {
                    *target = sym as u16;
                }
                offsets[len as usize] += 1;
            }
        }
        Ok(Self {
            single_symbol: None,
            counts,
            symbols,
        })
    }

    #[inline(always)]
    fn decode(&self, br: &mut BitReader<'_>) -> Result<u16> {
        if let Some(sym) = self.single_symbol {
            return Ok(sym);
        }
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..=MAX_HUFFMAN_BITS {
            let bit = br.read_bits(1)? as i32;
            code |= bit;
            let count = self.counts[len] as i32;
            if code - first < count {
                let sym_idx = (index + (code - first)) as usize;
                return self.symbols.get(sym_idx).copied().ok_or_else(|| {
                    RenderError::InvalidInput("brotli: symbol index out of bounds".into())
                });
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(RenderError::InvalidInput(
            "brotli: invalid huffman code in stream".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers for insert/copy/distance mapping
// ---------------------------------------------------------------------------

#[inline(always)]
fn get_insert_code(ins_len: u32) -> (usize, u32, u8) {
    for i in (0..INSERT_BASE.len()).rev() {
        if ins_len >= INSERT_BASE[i] {
            let extra = ins_len - INSERT_BASE[i];
            return (i, extra, INSERT_EXTRA[i]);
        }
    }
    (0, 0, 0)
}

#[inline(always)]
fn get_copy_code(cp_len: u32) -> (usize, u32, u8) {
    for i in (0..COPY_BASE.len()).rev() {
        if cp_len >= COPY_BASE[i] {
            let extra = cp_len - COPY_BASE[i];
            return (i, extra, COPY_EXTRA[i]);
        }
    }
    (0, 0, 0)
}

#[inline(always)]
fn get_dist_code(dist: u32) -> Result<(usize, u32, u8)> {
    if dist == 0 {
        return Err(RenderError::InvalidInput(
            "brotli: zero distance in match".into(),
        ));
    }
    let d_adj = dist - 1;
    for hcode in 0..48 {
        let ndistbits = (1 + (hcode >> 1)) as u8;
        let offset = ((2 + (hcode & 1)) << ndistbits) - 4;
        let span = 1u32 << ndistbits;
        if d_adj >= offset && d_adj < offset + span {
            let dcode = (16 + hcode) as usize;
            let dextra = d_adj - offset;
            return Ok((dcode, dextra, ndistbits));
        }
    }
    Err(RenderError::InvalidInput(
        "brotli: distance too large".into(),
    ))
}

#[inline(always)]
fn get_cmd_code(ins_code: usize, cp_code: usize) -> u16 {
    let ins_cell = ins_code / 8;
    let ins_off = ins_code % 8;
    let cp_cell = cp_code / 8;
    let cp_off = cp_code % 8;
    let cell = match (ins_cell, cp_cell) {
        (0, 0) => 0,
        (0, 1) => 1,
        (1, 0) => 2,
        (1, 1) => 3,
        (0, 2) => 4,
        (2, 0) => 5,
        (1, 2) => 6,
        (2, 1) => 7,
        _ => 8,
    };
    (128 + cell * 64 + (ins_off << 3) + cp_off) as u16
}

// ---------------------------------------------------------------------------
// Huffman Code Length Generator
// ---------------------------------------------------------------------------

fn build_huffman_lengths(freqs: &[u32], max_bits: usize) -> Vec<u8> {
    let non_zeros: Vec<(u32, usize)> = freqs
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, f)| f > 0)
        .map(|(sym, f)| (f, sym))
        .collect();

    let mut lengths = vec![0u8; freqs.len()];
    if non_zeros.is_empty() {
        return lengths;
    }
    if non_zeros.len() == 1 {
        let (_, sym) = non_zeros[0];
        lengths[sym] = 1;
        return lengths;
    }
    if non_zeros.len() == 2 {
        lengths[non_zeros[0].1] = 1;
        lengths[non_zeros[1].1] = 1;
        return lengths;
    }

    #[derive(Clone, Copy)]
    struct Node {
        weight: u32,
        left: u16,
        right: u16,
        sym: u16,
    }

    let mut pool: Vec<Node> = Vec::with_capacity(non_zeros.len() * 2);
    for &(w, sym) in &non_zeros {
        pool.push(Node {
            weight: w,
            left: u16::MAX,
            right: u16::MAX,
            sym: sym as u16,
        });
    }

    let mut heap: Vec<u16> = (0..pool.len() as u16).collect();
    heap.sort_by_key(|&idx| pool[idx as usize].weight);

    while heap.len() > 1 {
        let a = heap.remove(0);
        let b = heap.remove(0);
        let w = pool[a as usize]
            .weight
            .saturating_add(pool[b as usize].weight);
        let new_idx = pool.len() as u16;
        pool.push(Node {
            weight: w,
            left: a,
            right: b,
            sym: u16::MAX,
        });
        let insert_pos = heap
            .binary_search_by_key(&w, |&idx| pool[idx as usize].weight)
            .unwrap_or_else(|pos| pos);
        heap.insert(insert_pos, new_idx);
    }

    if let Some(&root) = heap.first() {
        let mut stack: Vec<(u16, u8)> = Vec::new();
        stack.push((root, 0));
        while let Some((node_idx, depth)) = stack.pop() {
            let node = pool[node_idx as usize];
            if node.sym != u16::MAX {
                let clamped = depth.max(1).min(max_bits as u8);
                lengths[node.sym as usize] = clamped;
            } else {
                if node.right != u16::MAX {
                    stack.push((node.right, depth + 1));
                }
                if node.left != u16::MAX {
                    stack.push((node.left, depth + 1));
                }
            }
        }
    }

    let target = 1u32 << max_bits;
    loop {
        let current_sum: u32 = lengths
            .iter()
            .map(|&l| if l > 0 { target >> l } else { 0 })
            .sum();
        if current_sum == target {
            break;
        }
        if current_sum > target {
            let mut best_pos = None;
            let mut best_val = 0u8;
            for (idx, &l) in lengths.iter().enumerate() {
                if l > 0 && (l as usize) < max_bits && l > best_val {
                    best_val = l;
                    best_pos = Some(idx);
                }
            }
            if let Some(idx) = best_pos {
                lengths[idx] += 1;
            } else {
                break;
            }
        } else {
            let mut best_pos = None;
            let mut best_val = 0u8;
            for (idx, &l) in lengths.iter().enumerate() {
                if l > 1 && l > best_val {
                    best_val = l;
                    best_pos = Some(idx);
                }
            }
            if let Some(idx) = best_pos {
                lengths[idx] -= 1;
            } else {
                break;
            }
        }
    }

    lengths
}

fn build_canonical_codes(lengths: &[u8]) -> Vec<(u32, u8)> {
    let non_zeros: Vec<usize> = lengths
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, l)| l > 0)
        .map(|(s, _)| s)
        .collect();

    let mut codes = vec![(0u32, 0u8); lengths.len()];
    if non_zeros.is_empty() {
        return codes;
    }
    if non_zeros.len() == 1 {
        // RFC 7932 Section 3.4: 0 bits emitted for single symbol
        codes[non_zeros[0]] = (0, 0);
        return codes;
    }

    let mut counts = [0u16; 16];
    for &l in lengths {
        if l > 0 && (l as usize) < counts.len() {
            counts[l as usize] += 1;
        }
    }
    let mut next_code = [0u32; 16];
    let mut code = 0u32;
    for bits in 1..16 {
        code = (code + u32::from(counts[bits - 1])) << 1;
        next_code[bits] = code;
    }
    for (sym, &l) in lengths.iter().enumerate() {
        if l > 0 && (l as usize) < 16 {
            let c = next_code[l as usize];
            next_code[l as usize] += 1;
            let mut rev = 0u32;
            for i in 0..l {
                rev = (rev << 1) | ((c >> i) & 1);
            }
            codes[sym] = (rev, l);
        }
    }
    codes
}

fn write_prefix_code(bw: &mut BitWriter, lengths: &[u8], alphabet_size: usize) {
    let non_zeros: Vec<(usize, u8)> = lengths
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, l)| l > 0)
        .collect();

    if non_zeros.len() <= 4 {
        // Simple prefix code (RFC 7932 Section 3.4)
        bw.write_bits(1, 2); // value 1 indicates Simple Prefix Code
        let nsym = non_zeros.len() as u32;
        bw.write_bits(nsym - 1, 2);
        let alph_bits = (alphabet_size.saturating_sub(1).max(1) as u32)
            .checked_ilog2()
            .unwrap_or(0)
            + 1;
        for &(sym, _) in &non_zeros {
            bw.write_bits(sym as u32, alph_bits);
        }
        if nsym == 4 {
            bw.write_bits(0, 1); // tree_select = 0 (2, 2, 2, 2)
        }
    } else {
        // Complex prefix code (RFC 7932 Section 3.5)
        bw.write_bits(0, 2); // HSKIP = 0

        let last_non_zero_sym = lengths.iter().rposition(|&l| l > 0).unwrap_or(0);
        let active_lengths = &lengths[..=last_non_zero_sym];

        let mut rle: Vec<(u8, u32, u8)> = Vec::new();
        let mut i = 0;
        while i < active_lengths.len() {
            let l = active_lengths[i];
            let mut j = i + 1;
            while j < active_lengths.len() && active_lengths[j] == l {
                j += 1;
            }
            let mut count = (j - i) as u32;
            if l == 0 {
                while count >= 3 {
                    let rep = count.min(10);
                    rle.push((17, rep - 3, 3));
                    count -= rep;
                    if count >= 3 {
                        rle.push((0, 0, 0));
                        count -= 1;
                    }
                }
                for _ in 0..count {
                    rle.push((0, 0, 0));
                }
            } else {
                rle.push((l, 0, 0));
                count -= 1;
                while count >= 3 {
                    let rep = count.min(6);
                    rle.push((16, rep - 3, 2));
                    count -= rep;
                    if count >= 3 {
                        rle.push((l, 0, 0));
                        count -= 1;
                    }
                }
                for _ in 0..count {
                    rle.push((l, 0, 0));
                }
            }
            i = j;
        }

        let mut cl_freqs = [0u32; 18];
        for &(sym, _, _) in &rle {
            if let Some(f) = cl_freqs.get_mut(sym as usize) {
                *f += 1;
            }
        }
        let cl_lengths = build_huffman_lengths(&cl_freqs, CODE_LENGTH_MAX_BITS);

        let mut kraft = 0u32;
        for &sym in &CL_ORDER {
            let l = cl_lengths.get(sym).copied().unwrap_or(0) as usize;
            let (v, nb) = CL_VLC[l.min(5)];
            bw.write_bits(v, u32::from(nb));
            if l > 0 {
                kraft += 32 >> l;
                if kraft >= 32 {
                    break;
                }
            }
        }

        let cl_codes = build_canonical_codes(&cl_lengths);
        for &(sym, extra, nb) in &rle {
            let (code, bitlen) = cl_codes[sym as usize];
            bw.write_bits(code, u32::from(bitlen));
            if nb > 0 {
                bw.write_bits(extra, u32::from(nb));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Scratch Buffer for Repeated Encodings
// ---------------------------------------------------------------------------

pub struct BrotliCompressScratch {
    head: Vec<u32>,
    prev: Vec<u32>,
    commands: Vec<BrotliCommand>,
    lit_freqs: [u32; 256],
    cmd_freqs: [u32; 704],
    dist_freqs: [u32; 64],
}

impl Default for BrotliCompressScratch {
    fn default() -> Self {
        Self {
            head: Vec::new(),
            prev: Vec::new(),
            commands: Vec::new(),
            lit_freqs: [0; 256],
            cmd_freqs: [0; 704],
            dist_freqs: [0; 64],
        }
    }
}

impl BrotliCompressScratch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug)]
struct BrotliCommand {
    lit_start: u32,
    lit_len: u32,
    copy_len: u32,
    distance: u32,
}

// ---------------------------------------------------------------------------
// Brotli Compressor
// ---------------------------------------------------------------------------

/// Compress input bytes with the clean-room Brotli encoder (RFC 7932).
#[must_use]
pub fn brotli_compress(data: &[u8]) -> Vec<u8> {
    let mut scratch = BrotliCompressScratch::new();
    brotli_compress_with_scratch(data, &mut scratch)
}

/// Compress input bytes using a caller-owned scratch buffer to skip reallocations.
pub fn brotli_compress_with_scratch(data: &[u8], scratch: &mut BrotliCompressScratch) -> Vec<u8> {
    if data.is_empty() {
        let mut bw = BitWriter::with_capacity(1);
        bw.write_bits(1, 1); // WBITS flag
        bw.write_bits(6, 3); // WBITS 22
        bw.write_bits(1, 1); // ISLAST = 1
        bw.write_bits(1, 1); // ISLASTEMPTY = 1
        bw.finish();
        return bw.out;
    }

    scratch.lit_freqs.fill(0);
    scratch.cmd_freqs.fill(0);
    scratch.dist_freqs.fill(0);
    scratch.commands.clear();

    let n = data.len();
    const HASH_BITS: usize = 16;
    const HASH_SIZE: usize = 1 << HASH_BITS;
    if scratch.head.len() != HASH_SIZE {
        scratch.head.resize(HASH_SIZE, 0);
    } else {
        scratch.head.fill(0);
    }
    if scratch.prev.len() < n {
        scratch.prev.resize(n, 0);
    }

    let mut pos = 0usize;
    let mut pending_lit_start = 0usize;
    let mut pending_lit_len = 0usize;

    while pos < n {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        if pos + 3 <= n {
            let h = ((u32::from(data[pos])
                ^ (u32::from(data[pos + 1]) << 4)
                ^ (u32::from(data[pos + 2]) << 8))
                .wrapping_mul(0x1e35_a7bd)) as usize
                & (HASH_SIZE - 1);
            let mut cand = scratch.head[h];
            let mut chain = 64;

            while cand > 0 && chain > 0 {
                let c = (cand - 1) as usize;
                if c >= pos {
                    break;
                }
                let dist = pos - c;
                if dist > 4_000_000 {
                    break;
                }
                let max_len = (n - pos).min(10000);
                let mut l = 0usize;
                while l < max_len && data[c + l] == data[pos + l] {
                    l += 1;
                }
                if l >= 3 && l > best_len {
                    best_len = l;
                    best_dist = dist;
                    if l >= 256 {
                        break;
                    }
                }
                cand = scratch.prev[c];
                chain -= 1;
            }
        }

        if best_len >= 3 {
            scratch.commands.push(BrotliCommand {
                lit_start: pending_lit_start as u32,
                lit_len: pending_lit_len as u32,
                copy_len: best_len as u32,
                distance: best_dist as u32,
            });
            for i in 0..best_len {
                let p = pos + i;
                if p + 3 <= n {
                    let h = ((u32::from(data[p])
                        ^ (u32::from(data[p + 1]) << 4)
                        ^ (u32::from(data[p + 2]) << 8))
                        .wrapping_mul(0x1e35_a7bd)) as usize
                        & (HASH_SIZE - 1);
                    scratch.prev[p] = scratch.head[h];
                    scratch.head[h] = (p + 1) as u32;
                }
            }
            pos += best_len;
            pending_lit_start = pos;
            pending_lit_len = 0;
        } else {
            if pos + 3 <= n {
                let h = ((u32::from(data[pos])
                    ^ (u32::from(data[pos + 1]) << 4)
                    ^ (u32::from(data[pos + 2]) << 8))
                    .wrapping_mul(0x1e35_a7bd)) as usize
                    & (HASH_SIZE - 1);
                scratch.prev[pos] = scratch.head[h];
                scratch.head[h] = (pos + 1) as u32;
            }
            pos += 1;
            pending_lit_len += 1;
        }
    }

    if pending_lit_len > 0 || scratch.commands.is_empty() {
        scratch.commands.push(BrotliCommand {
            lit_start: pending_lit_start as u32,
            lit_len: pending_lit_len as u32,
            copy_len: 2,
            distance: 4,
        });
    }

    for cmd in &scratch.commands {
        let lit_s = cmd.lit_start as usize;
        let lit_e = lit_s + cmd.lit_len as usize;
        for &b in &data[lit_s..lit_e] {
            scratch.lit_freqs[b as usize] += 1;
        }
        let (ins_code, _, _) = get_insert_code(cmd.lit_len);
        let (cp_code, _, _) = get_copy_code(cmd.copy_len);
        let cmd_code = get_cmd_code(ins_code, cp_code);
        scratch.cmd_freqs[cmd_code as usize] += 1;

        if let Ok((dcode, _, _)) = get_dist_code(cmd.distance) {
            scratch.dist_freqs[dcode] += 1;
        }
    }

    let lit_lengths = build_huffman_lengths(&scratch.lit_freqs, MAX_HUFFMAN_BITS);
    let cmd_lengths = build_huffman_lengths(&scratch.cmd_freqs, MAX_HUFFMAN_BITS);
    let dist_lengths = build_huffman_lengths(&scratch.dist_freqs, MAX_HUFFMAN_BITS);

    let lit_codes = build_canonical_codes(&lit_lengths);
    let cmd_codes = build_canonical_codes(&cmd_lengths);
    let dist_codes = build_canonical_codes(&dist_lengths);

    let mut bw = BitWriter::with_capacity(n / 2 + 128);

    // Stream header: WBITS = 22
    bw.write_bits(1, 1);
    bw.write_bits(6, 3);

    // Meta-block header (ISLAST = 1)
    bw.write_bits(1, 1);
    bw.write_bits(0, 1); // ISLASTEMPTY = 0

    let m = (n - 1) as u32;
    if n <= 65536 {
        bw.write_bits(0, 2); // 4 nibbles
        bw.write_bits(m, 16);
    } else if n <= 1_048_576 {
        bw.write_bits(1, 2); // 5 nibbles
        bw.write_bits(m, 20);
    } else {
        bw.write_bits(2, 2); // 6 nibbles
        bw.write_bits(m, 24);
    }

    bw.write_bits(0, 1); // NBLTYPESL = 1
    bw.write_bits(0, 1); // NBLTYPESI = 1
    bw.write_bits(0, 1); // NBLTYPESD = 1
    bw.write_bits(0, 2); // NPOSTFIX = 0
    bw.write_bits(0, 4); // NDIRECT = 0
    bw.write_bits(0, 2); // context mode = 0
    bw.write_bits(0, 1); // NTREESL = 1
    bw.write_bits(0, 1); // NTREESD = 1

    write_prefix_code(&mut bw, &lit_lengths, 256);
    write_prefix_code(&mut bw, &cmd_lengths, 704);
    write_prefix_code(&mut bw, &dist_lengths, 64);

    let mut emitted_bytes = 0usize;
    for cmd in &scratch.commands {
        let (ins_code, ins_extra, ins_nb) = get_insert_code(cmd.lit_len);
        let (cp_code, cp_extra, cp_nb) = get_copy_code(cmd.copy_len);
        let cmd_code = get_cmd_code(ins_code, cp_code);

        let (c_code, c_bits) = cmd_codes[cmd_code as usize];
        bw.write_bits(c_code, u32::from(c_bits));
        if ins_nb > 0 {
            bw.write_bits(ins_extra, u32::from(ins_nb));
        }
        if cp_nb > 0 {
            bw.write_bits(cp_extra, u32::from(cp_nb));
        }

        let lit_s = cmd.lit_start as usize;
        let lit_e = lit_s + cmd.lit_len as usize;
        for &b in &data[lit_s..lit_e] {
            let (l_code, l_bits) = lit_codes[b as usize];
            bw.write_bits(l_code, u32::from(l_bits));
        }
        emitted_bytes += cmd.lit_len as usize;
        if emitted_bytes >= n {
            break;
        }

        if let Ok((dcode, dextra, dnb)) = get_dist_code(cmd.distance) {
            let (d_code, d_bits) = dist_codes[dcode];
            bw.write_bits(d_code, u32::from(d_bits));
            if dnb > 0 {
                bw.write_bits(dextra, u32::from(dnb));
            }
        }
        emitted_bytes += cmd.copy_len as usize;
    }

    bw.finish();
    bw.out
}

// ---------------------------------------------------------------------------
// Brotli Decompressor
// ---------------------------------------------------------------------------

fn read_prefix_code_tree(br: &mut BitReader<'_>, alphabet_size: usize) -> Result<HuffmanTree> {
    let first_two = br.read_bits(2)?;
    if first_two == 1 {
        let nsym = (br.read_bits(2)? + 1) as usize;
        let alph_bits = (alphabet_size.saturating_sub(1).max(1) as u32)
            .checked_ilog2()
            .unwrap_or(0)
            + 1;
        let mut symbols = Vec::with_capacity(nsym);
        for _ in 0..nsym {
            symbols.push(br.read_bits(alph_bits as usize)? as usize);
        }
        let mut lengths = vec![0u8; alphabet_size];
        if nsym == 1 {
            return Ok(HuffmanTree::from_single(symbols[0] as u16));
        } else if nsym == 2 {
            symbols.sort_unstable();
            lengths[symbols[0]] = 1;
            lengths[symbols[1]] = 1;
        } else if nsym == 3 {
            lengths[symbols[0]] = 1;
            lengths[symbols[1]] = 2;
            lengths[symbols[2]] = 2;
        } else if nsym == 4 {
            let ts = br.read_bits(1)?;
            if ts == 0 {
                for &s in &symbols {
                    lengths[s] = 2;
                }
            } else {
                lengths[symbols[0]] = 1;
                lengths[symbols[1]] = 2;
                lengths[symbols[2]] = 3;
                lengths[symbols[3]] = 3;
            }
        }
        HuffmanTree::from_lengths(&lengths)
    } else {
        let hskip = first_two as usize;
        let mut cl_lengths = [0u8; 18];
        let mut kraft = 0u32;
        for &sym in &CL_ORDER[hskip..] {
            let b0 = br.read_bits(1)?;
            let b1 = br.read_bits(1)?;
            let two = (b1 << 1) | b0;
            let val = if two == 0 {
                0
            } else if two == 2 {
                3
            } else if two == 1 {
                4
            } else {
                let b2 = br.read_bits(1)?;
                if b2 == 0 {
                    2
                } else {
                    let b3 = br.read_bits(1)?;
                    if b3 == 0 { 1 } else { 5 }
                }
            };
            cl_lengths[sym] = val;
            if val > 0 {
                kraft += 32 >> val;
                if kraft >= 32 {
                    break;
                }
            }
        }

        let cl_tree = HuffmanTree::from_lengths(&cl_lengths)?;
        let mut code_lengths = vec![0u8; alphabet_size];
        let mut space = 32768i32;
        let mut sym_idx = 0usize;
        let mut prev_non_zero = 8u8;

        while space > 0 && sym_idx < alphabet_size {
            let sym = cl_tree.decode(br)?;
            if sym <= 15 {
                let s = sym as u8;
                code_lengths[sym_idx] = s;
                if s > 0 {
                    prev_non_zero = s;
                    space -= 32768 >> s;
                }
                sym_idx += 1;
            } else if sym == 16 {
                let repeat = (br.read_bits(2)? + 3) as usize;
                for _ in 0..repeat {
                    if sym_idx < alphabet_size {
                        code_lengths[sym_idx] = prev_non_zero;
                        space -= 32768 >> prev_non_zero;
                        sym_idx += 1;
                    }
                }
            } else if sym == 17 {
                let repeat = (br.read_bits(3)? + 3) as usize;
                sym_idx += repeat;
            }
        }

        HuffmanTree::from_lengths(&code_lengths)
    }
}

/// Decompress a Brotli stream back to its original bytes.
pub fn brotli_decompress(data: &[u8], max_len: usize) -> Result<Vec<u8>> {
    let mut br = BitReader::new(data);

    let wflag = br.read_bits(1)?;
    if wflag != 0 {
        let n = br.read_bits(3)?;
        if n == 0 {
            let _ = br.read_bits(1)?;
        }
    }

    let islast = br.read_bits(1)?;
    if islast != 0 {
        let isempty = br.read_bits(1)?;
        if isempty != 0 {
            return Ok(Vec::new());
        }
    }

    let mnibbles = br.read_bits(2)?;
    let nibbles = match mnibbles {
        0 => 4,
        1 => 5,
        2 => 6,
        _ => return Err(RenderError::InvalidInput("brotli: invalid mnibbles".into())),
    };
    let mlen = (br.read_bits(nibbles * 4)? + 1) as usize;
    if mlen > max_len {
        return Err(RenderError::InvalidInput(
            "brotli: uncompressed length exceeds max_len".into(),
        ));
    }

    let _ = br.read_bits(1)?;
    let _ = br.read_bits(1)?;
    let _ = br.read_bits(1)?;
    let _ = br.read_bits(2)?;
    let _ = br.read_bits(4)?;
    let _ = br.read_bits(2)?;
    let _ = br.read_bits(1)?;
    let _ = br.read_bits(1)?;

    let lit_tree = read_prefix_code_tree(&mut br, 256)?;
    let cmd_tree = read_prefix_code_tree(&mut br, 704)?;
    let dist_tree = read_prefix_code_tree(&mut br, 64)?;

    let mut dist_ring = [16u32, 15, 11, 4];
    let mut dist_idx = 4usize;

    let mut out = Vec::with_capacity(mlen);

    while out.len() < mlen {
        let cmd = cmd_tree.decode(&mut br)? as usize;
        let (dist_is_zero, ins_code, cp_code) = if cmd < 128 {
            if cmd < 64 {
                (true, (cmd >> 3) & 7, cmd & 7)
            } else {
                (true, ((cmd - 64) >> 3) & 7, 8 + ((cmd - 64) & 7))
            }
        } else {
            let code = cmd - 128;
            let cell = code / 64;
            let off = code % 64;
            let ins_off = (off >> 3) & 7;
            let cp_off = off & 7;
            let (base_ins, base_cp) = match cell {
                0 => (0, 0),
                1 => (0, 8),
                2 => (8, 0),
                3 => (8, 8),
                4 => (0, 16),
                5 => (16, 0),
                6 => (8, 16),
                7 => (16, 8),
                _ => (16, 16),
            };
            (false, base_ins + ins_off, base_cp + cp_off)
        };

        let ins_len = INSERT_BASE[ins_code] + br.read_bits(usize::from(INSERT_EXTRA[ins_code]))?;
        let cp_len = COPY_BASE[cp_code] + br.read_bits(usize::from(COPY_EXTRA[cp_code]))?;

        for _ in 0..ins_len {
            let lit = lit_tree.decode(&mut br)? as u8;
            out.push(lit);
        }

        if out.len() >= mlen {
            break;
        }

        let dist = if dist_is_zero {
            dist_ring[(dist_idx - 1) & 3]
        } else {
            let dcode = dist_tree.decode(&mut br)? as usize;
            if dcode == 0 {
                dist_ring[(dist_idx - 1) & 3]
            } else if dcode == 1 {
                let val = dist_ring[(dist_idx - 2) & 3];
                dist_ring[dist_idx & 3] = val;
                dist_idx += 1;
                val
            } else if dcode == 2 {
                let val = dist_ring[(dist_idx - 3) & 3];
                dist_ring[dist_idx & 3] = val;
                dist_idx += 1;
                val
            } else if dcode == 3 {
                let val = dist_ring[(dist_idx - 4) & 3];
                dist_ring[dist_idx & 3] = val;
                dist_idx += 1;
                val
            } else if dcode < 16 {
                let base_dist = dist_ring[(dist_idx - 1 - ((dcode - 4) / 6)) & 3];
                let offsets = [-1i32, 1, -2, 2, -3, 3];
                let mod_val = offsets[(dcode - 4) % 6];
                let val = (base_dist as i32 + mod_val).max(1) as u32;
                dist_ring[dist_idx & 3] = val;
                dist_idx += 1;
                val
            } else {
                let hcode = dcode - 16;
                let ndistbits = (1 + (hcode >> 1)) as usize;
                let offset = ((2 + (hcode & 1)) << ndistbits) - 4;
                let dextra = br.read_bits(ndistbits)?;
                let val = offset as u32 + dextra + 1;
                dist_ring[dist_idx & 3] = val;
                dist_idx += 1;
                val
            }
        };

        if dist == 0 || dist as usize > out.len() {
            return Err(RenderError::InvalidInput(
                "brotli: distance out of bounds".into(),
            ));
        }

        for _ in 0..cp_len {
            let src_idx = out.len() - dist as usize;
            let b = out[src_idx];
            out.push(b);
        }
    }

    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_empty() {
        let compressed = brotli_compress(b"");
        let decompressed = brotli_decompress(&compressed, 100).expect("decompress");
        assert_eq!(decompressed, b"");
    }

    #[test]
    fn round_trip_simple_text() {
        let text = b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.";
        let compressed = brotli_compress(text);
        let decompressed = brotli_decompress(&compressed, text.len() + 100).expect("decompress");
        assert_eq!(decompressed, text);
    }

    #[test]
    fn round_trip_repeated_bytes() {
        let bytes = vec![0x42u8; 2000];
        let compressed = brotli_compress(&bytes);
        assert!(compressed.len() < bytes.len());
        let decompressed = brotli_decompress(&compressed, bytes.len() + 100).expect("decompress");
        assert_eq!(decompressed, bytes);
    }

    #[test]
    fn round_trip_font_data() {
        let ttf = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
        let chunk = &ttf[..8192];
        let compressed = brotli_compress(chunk);
        assert!(compressed.len() < chunk.len());
        let decompressed = brotli_decompress(&compressed, chunk.len() + 100).expect("decompress");
        assert_eq!(decompressed, chunk);
    }

    #[test]
    fn external_brotli_cli_compatibility() {
        let brotli_path = "/opt/homebrew/bin/brotli";
        if std::path::Path::new(brotli_path).exists() {
            let ttf = include_bytes!("../fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
            let chunk = &ttf[..8192];
            let compressed = brotli_compress(chunk);
            let mut child = std::process::Command::new(brotli_path)
                .arg("-d")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("spawn brotli");
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(&compressed)
                .expect("write");
            let output = child.wait_with_output().expect("wait");
            assert!(
                output.status.success(),
                "system brotli failed to decompress"
            );
            assert_eq!(output.stdout, chunk);
        }
    }
}
