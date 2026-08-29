//! Test-local raw-DEFLATE inflater (stored + fixed-Huffman blocks — the only
//! two block types the crate compressor emits), mirroring the proven helper
//! in `epub_test.rs`. Test-only helper; panics on malformed input are the
//! failure signal.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn bits(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            let byte = self.data[self.bit_pos / 8];
            let bit = (byte >> (self.bit_pos % 8)) & 1;
            v |= u32::from(bit) << i;
            self.bit_pos += 1;
        }
        v
    }

    fn align_to_byte(&mut self) {
        self.bit_pos = self.bit_pos.div_ceil(8) * 8;
    }

    fn take_bytes(&mut self, n: usize) -> &'a [u8] {
        assert_eq!(self.bit_pos % 8, 0, "byte-aligned");
        let start = self.bit_pos / 8;
        self.bit_pos += n * 8;
        &self.data[start..start + n]
    }
}

struct HuffTab {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffTab {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..15 {
            offs[i + 1] = offs[i] + counts[i];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Self { counts, symbols }
    }

    fn decode(&self, br: &mut BitReader) -> u16 {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= br.bits(1) as i32;
            let count = i32::from(self.counts[len]);
            if code - first < count {
                return self.symbols[(index + (code - first)) as usize];
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        panic!("invalid huffman code");
    }
}

fn fixed_litlen_tab() -> HuffTab {
    let lengths: Vec<u8> = (0..288)
        .map(|i| match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        })
        .collect();
    HuffTab::new(&lengths)
}

/// Inflate a raw DEFLATE stream (stored and fixed-Huffman blocks only).
fn inflate_raw(body: &[u8]) -> Vec<u8> {
    let lit = fixed_litlen_tab();
    let dist = HuffTab::new(&[5u8; 30]);
    let mut br = BitReader::new(body);
    let mut out = Vec::new();
    loop {
        let bfinal = br.bits(1);
        match br.bits(2) {
            0 => {
                br.align_to_byte();
                let len_bytes = br.take_bytes(4);
                let len = u16::from(len_bytes[0]) | (u16::from(len_bytes[1]) << 8);
                let nlen = u16::from(len_bytes[2]) | (u16::from(len_bytes[3]) << 8);
                assert_eq!(len, !nlen, "stored block LEN/NLEN complement");
                out.extend_from_slice(br.take_bytes(len as usize));
            }
            1 => loop {
                let sym = lit.decode(&mut br);
                match sym {
                    0..=255 => out.push(sym as u8),
                    256 => break,
                    257..=285 => {
                        let idx = (sym - 257) as usize;
                        let len = LENGTH_BASE[idx] as usize
                            + br.bits(u32::from(LENGTH_EXTRA[idx])) as usize;
                        let dsym = dist.decode(&mut br) as usize;
                        let d = DIST_BASE[dsym] as usize
                            + br.bits(u32::from(DIST_EXTRA[dsym])) as usize;
                        assert!(d <= out.len(), "distance within window");
                        let start = out.len() - d;
                        for k in 0..len {
                            let b = out[start + k];
                            out.push(b);
                        }
                    }
                    other => panic!("invalid literal/length symbol {other}"),
                }
            },
            other => panic!("unexpected deflate block type {other}"),
        }
        if bfinal == 1 {
            return out;
        }
    }
}

/// Decompress every zlib FlateDecode stream in the PDF (2-byte header, raw
/// DEFLATE body, 4-byte Adler trailer) and concatenate the content bytes.
pub fn decompressed_content(pdf: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(pos) = pdf[i..]
        .windows(b"stream\n".len())
        .position(|w| w == b"stream\n")
    {
        let start = i + pos + b"stream\n".len();
        let Some(end_rel) = pdf[start..]
            .windows(b"endstream".len())
            .position(|w| w == b"endstream")
        else {
            break;
        };
        let raw = &pdf[start..start + end_rel];
        // zlib streams start 0x78; unfiltered payloads (font subsets) skip.
        if raw.first() == Some(&0x78) && raw.len() > 6 {
            out.extend_from_slice(&inflate_raw(&raw[2..raw.len() - 4]));
        }
        i = start + end_rel + b"endstream".len();
    }
    out
}
