//! Tests for `src/zip.rs` (deterministic ZIP writer) and `src/epub.rs`
//! (EPUB 3 renderer). Included via `#[path]` so they run standalone before
//! the modules are registered in `lib.rs`. Tests may unwrap/panic for
//! clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../src/epub.rs"]
mod epub;
#[path = "../src/zip.rs"]
mod zip;

use franken_markdown::{HtmlOptions, parse_markdown, zlib_decompress};
use zip::{ZipWriter, crc32};

// ---------------------------------------------------------------------------
// Byte helpers.

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &x in data {
        a = (a + u32::from(x)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// Re-wrap a raw DEFLATE body as a zlib stream (header + Adler-32 trailer)
/// so the crate's own validating `zlib_decompress` can decode it. Used where
/// the original bytes are known (the adler must cover the original).
fn wrap_zlib(deflate_body: &[u8], original: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend_from_slice(deflate_body);
    out.extend_from_slice(&adler32(original).to_be_bytes());
    out
}

// ---------------------------------------------------------------------------
// Minimal raw-DEFLATE inflater (stored + fixed-Huffman blocks — the only two
// block types the crate compressor emits) so method-8 ZIP payloads can be
// recovered WITHOUT knowing the original bytes. Recovered output is then
// verified against the central directory's CRC-32 and uncompressed size,
// making this a genuine round-trip check.

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

    /// Read `n` bits LSB-first.
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

/// Canonical-Huffman decode table (puff-style counts + sorted symbols).
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
fn inflate_raw(body: &[u8], max_out: usize) -> Vec<u8> {
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
                let len = u16le(len_bytes, 0) as usize;
                let nlen = u16le(len_bytes, 2);
                assert_eq!(len as u16, !nlen, "stored block LEN/NLEN complement");
                out.extend_from_slice(br.take_bytes(len));
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
                assert!(out.len() <= max_out, "output within declared size");
            },
            other => panic!("unexpected deflate block type {other}"),
        }
        if bfinal == 1 {
            return out;
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal ZIP reader over our own writer's output: parses the central
// directory and local headers back.

struct CentralEntry {
    name: String,
    flags: u16,
    method: u16,
    mod_time: u16,
    mod_date: u16,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    local_offset: u32,
}

fn parse_central_directory(bytes: &[u8]) -> Vec<CentralEntry> {
    let eocd = &bytes[bytes.len() - 22..];
    assert_eq!(u32le(eocd, 0), 0x0605_4B50, "EOCD signature");
    assert_eq!(u16le(eocd, 4), 0, "single disk");
    assert_eq!(u16le(eocd, 6), 0, "single central-dir disk");
    let count = u16le(eocd, 10) as usize;
    assert_eq!(
        u16le(eocd, 8) as usize,
        count,
        "disk/total entry counts agree"
    );
    let cd_size = u32le(eocd, 12) as usize;
    let cd_offset = u32le(eocd, 16) as usize;
    assert_eq!(u16le(eocd, 20), 0, "no archive comment");
    assert_eq!(
        cd_offset + cd_size,
        bytes.len() - 22,
        "central directory bounds"
    );

    let mut entries = Vec::with_capacity(count);
    let mut pos = cd_offset;
    for _ in 0..count {
        assert_eq!(u32le(bytes, pos), 0x0201_4B50, "central header signature");
        let name_len = u16le(bytes, pos + 28) as usize;
        let extra_len = u16le(bytes, pos + 30) as usize;
        let comment_len = u16le(bytes, pos + 32) as usize;
        entries.push(CentralEntry {
            flags: u16le(bytes, pos + 8),
            method: u16le(bytes, pos + 10),
            mod_time: u16le(bytes, pos + 12),
            mod_date: u16le(bytes, pos + 14),
            crc32: u32le(bytes, pos + 16),
            compressed_size: u32le(bytes, pos + 20),
            uncompressed_size: u32le(bytes, pos + 24),
            local_offset: u32le(bytes, pos + 42),
            name: String::from_utf8(bytes[pos + 46..pos + 46 + name_len].to_vec())
                .expect("entry name is UTF-8"),
        });
        pos += 46 + name_len + extra_len + comment_len;
    }
    assert_eq!(pos, cd_offset + cd_size, "central directory fully consumed");
    entries
}

/// Read back a local header + payload; returns the stored payload bytes.
fn local_payload<'a>(bytes: &'a [u8], entry: &CentralEntry) -> &'a [u8] {
    let pos = entry.local_offset as usize;
    assert_eq!(u32le(bytes, pos), 0x0403_4B50, "local header signature");
    assert_eq!(
        u16le(bytes, pos + 6),
        entry.flags,
        "local flags match central"
    );
    assert_eq!(
        u16le(bytes, pos + 8),
        entry.method,
        "local method matches central"
    );
    assert_eq!(u16le(bytes, pos + 10), 0, "zero DOS time");
    assert_eq!(u16le(bytes, pos + 12), 0, "zero DOS date");
    assert_eq!(
        u32le(bytes, pos + 14),
        entry.crc32,
        "local crc matches central"
    );
    let name_len = u16le(bytes, pos + 26) as usize;
    let extra_len = u16le(bytes, pos + 28) as usize;
    assert_eq!(
        &bytes[pos + 30..pos + 30 + name_len],
        entry.name.as_bytes(),
        "local name matches central"
    );
    let start = pos + 30 + name_len + extra_len;
    &bytes[start..start + entry.compressed_size as usize]
}

/// Recover an entry's original bytes, verifying CRC-32 and size against the
/// central directory. Stored payloads pass through; deflated payloads go
/// through the test's own raw-DEFLATE inflater.
fn recover_entry(bytes: &[u8], entry: &CentralEntry) -> Vec<u8> {
    let payload = local_payload(bytes, entry);
    let recovered = match entry.method {
        0 => payload.to_vec(),
        8 => inflate_raw(payload, entry.uncompressed_size as usize),
        other => panic!("unexpected method {other}"),
    };
    assert_eq!(
        recovered.len(),
        entry.uncompressed_size as usize,
        "uncompressed size matches"
    );
    assert_eq!(
        crc32(&recovered),
        entry.crc32,
        "crc32 matches recovered bytes"
    );
    recovered
}

fn entry_text(bytes: &[u8], name: &str) -> String {
    let entries = parse_central_directory(bytes);
    let entry = entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("entry {name} present"));
    String::from_utf8(recover_entry(bytes, entry)).expect("entry is UTF-8")
}

// ---------------------------------------------------------------------------
// Strict XML well-formedness scan (element stack, quoted attributes, entity
// validation, single root). Not a parser — a strict structural check over the
// exact XML/XHTML we emit.

fn assert_well_formed_xml(doc: &str) {
    let bytes = doc.as_bytes();
    let mut stack: Vec<String> = Vec::new();
    let mut closed_root = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => {
                let rest = &doc[i..];
                if let Some(pi) = rest.strip_prefix("<?") {
                    let end = pi.find("?>").expect("unterminated processing instruction");
                    i += 2 + end + 2;
                } else if let Some(comment) = rest.strip_prefix("<!--") {
                    let end = comment.find("-->").expect("unterminated comment");
                    i += 4 + end + 3;
                } else if rest.starts_with("<!DOCTYPE") {
                    let end = rest.find('>').expect("unterminated DOCTYPE");
                    i += end + 1;
                } else if let Some(close) = rest.strip_prefix("</") {
                    let end = close.find('>').expect("unterminated close tag");
                    let name = &close[..end];
                    assert!(
                        !name.is_empty()
                            && !name.contains(|c: char| c.is_whitespace() || c == '<' || c == '/'),
                        "malformed close tag {name:?}"
                    );
                    let top = stack
                        .pop()
                        .unwrap_or_else(|| panic!("unmatched close tag {name:?}"));
                    assert_eq!(top, name, "mismatched close tag");
                    if stack.is_empty() {
                        closed_root = true;
                    }
                    i += 2 + end + 1;
                } else {
                    let end = find_tag_end(doc, i);
                    let inner = doc[i + 1..end].trim_end();
                    let self_closing = inner.ends_with('/');
                    let inner = inner.trim_end_matches('/');
                    let name_len = inner.find(char::is_whitespace).unwrap_or(inner.len());
                    let name = &inner[..name_len];
                    assert!(
                        !name.is_empty() && !name.contains(['<', '"', '/', '=']),
                        "malformed tag name {name:?}"
                    );
                    check_attributes(&inner[name_len..]);
                    if stack.is_empty() {
                        assert!(!closed_root, "second root element {name:?}");
                        if self_closing {
                            closed_root = true;
                        }
                    }
                    if !self_closing {
                        stack.push(name.to_string());
                    }
                    i = end + 1;
                }
            }
            b'&' => {
                check_entity(doc, i);
                let semi = doc[i..].find(';').expect("unterminated entity reference");
                i += semi + 1;
            }
            _ => {
                let ch_len = doc[i..].chars().next().map_or(1, char::len_utf8);
                if closed_root && stack.is_empty() {
                    assert!(
                        doc[i..i + ch_len].trim().is_empty(),
                        "no text content after root element"
                    );
                }
                i += ch_len;
            }
        }
    }
    assert!(stack.is_empty(), "unclosed elements: {stack:?}");
    assert!(closed_root, "document has a root element");
}

fn find_tag_end(doc: &str, start: usize) -> usize {
    let bytes = doc.as_bytes();
    let mut in_quotes = false;
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quotes = !in_quotes,
            b'>' if !in_quotes => return i,
            _ => {}
        }
        i += 1;
    }
    panic!("unterminated tag");
}

fn check_attributes(mut rest: &str) {
    loop {
        rest = rest.trim_start_matches(char::is_whitespace);
        if rest.is_empty() {
            return;
        }
        let eq = rest
            .find('=')
            .unwrap_or_else(|| panic!("attribute without value: {rest:?}"));
        let name = rest[..eq].trim();
        assert!(
            !name.is_empty() && !name.contains(|c: char| c.is_whitespace() || c == '"' || c == '<'),
            "malformed attribute name {name:?}"
        );
        rest = rest[eq + 1..].trim_start();
        let quoted = rest
            .strip_prefix('"')
            .unwrap_or_else(|| panic!("attribute value must be double-quoted: {rest:?}"));
        let end = quoted.find('"').expect("unterminated attribute value");
        let value = &quoted[..end];
        assert!(!value.contains('<'), "raw < in attribute value");
        let mut v = value;
        while let Some(pos) = v.find('&') {
            check_entity(v, pos);
            let semi = v[pos..]
                .find(';')
                .expect("unterminated entity in attribute");
            v = &v[pos + semi + 1..];
        }
        rest = &quoted[end + 1..];
    }
}

fn check_entity(doc: &str, pos: usize) {
    let rest = &doc[pos..];
    const NAMED: [&str; 5] = ["&amp;", "&lt;", "&gt;", "&quot;", "&apos;"];
    if NAMED.iter().any(|e| rest.starts_with(e)) {
        return;
    }
    if let Some(num) = rest.strip_prefix("&#") {
        let semi = num.find(';').expect("unterminated numeric entity");
        let digits = num[..semi].strip_prefix('x').unwrap_or(&num[..semi]);
        assert!(
            !digits.is_empty() && digits.chars().all(|c| c.is_ascii_hexdigit()),
            "malformed numeric entity {digits:?}"
        );
        return;
    }
    panic!("invalid or unescaped & at byte {pos}");
}

// ---------------------------------------------------------------------------
// CRC-32 vectors.

#[test]
fn crc32_matches_reference_vectors() {
    assert_eq!(crc32(b""), 0x0000_0000);
    assert_eq!(crc32(b"a"), 0xE8B7_BE43);
    assert_eq!(crc32(b"hello world"), 0x0D4A_1185);
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(
        crc32(b"The quick brown fox jumps over the lazy dog"),
        0x414F_A339
    );
    // Bytes are checksummed, not chars.
    assert_eq!(
        crc32("héllo ☃".as_bytes()),
        crc32(b"h\xc3\xa9llo \xe2\x98\x83")
    );
}

// ---------------------------------------------------------------------------
// ZIP writer round-trip.

#[test]
fn zip_round_trip_via_own_central_directory() {
    let stored_text = b"stored payload, no compression".as_slice();
    let deflated_text = b"deflate me deflate me deflate me deflate me, please".repeat(8);
    let unicode_text = "unicode entry ☃ héllo".as_bytes();
    let empty: &[u8] = b"";

    let mut writer = ZipWriter::new();
    writer.add_stored("a.txt", stored_text);
    writer.add_deflated("dir/b.txt", &deflated_text);
    writer.add_stored("empty.bin", empty);
    writer.add_deflated("deflated-empty.bin", empty);
    writer.add_deflated("unicodé-☃.md", unicode_text);
    let bytes = writer.finish();

    // Archive opens with a local header and closes with EOCD.
    assert_eq!(u32le(&bytes, 0), 0x0403_4B50, "starts with local header");
    assert_eq!(
        u32le(&bytes, bytes.len() - 22),
        0x0605_4B50,
        "ends with EOCD"
    );

    let entries = parse_central_directory(&bytes);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "a.txt",
            "dir/b.txt",
            "empty.bin",
            "deflated-empty.bin",
            "unicodé-☃.md"
        ],
        "central directory preserves insertion order"
    );
    for entry in &entries {
        assert_eq!(entry.flags, 1 << 11, "UTF-8 name flag set");
        assert_eq!(entry.mod_time, 0, "zero DOS time");
        assert_eq!(entry.mod_date, 0, "zero DOS date");
    }
    let methods: Vec<u16> = entries.iter().map(|e| e.method).collect();
    assert_eq!(methods, [0, 8, 0, 8, 8], "stored/deflated methods recorded");

    // Recover every entry through the test's own inflater, verified by CRC.
    assert_eq!(recover_entry(&bytes, &entries[0]), stored_text);
    assert_eq!(recover_entry(&bytes, &entries[1]), deflated_text);
    assert_eq!(recover_entry(&bytes, &entries[2]), empty);
    assert_eq!(recover_entry(&bytes, &entries[3]), empty);
    assert_eq!(recover_entry(&bytes, &entries[4]), unicode_text);

    // Cross-check one deflated entry through the crate's validating zlib
    // decoder by re-wrapping the raw body with a correct header + adler.
    let wrapped = wrap_zlib(local_payload(&bytes, &entries[1]), &deflated_text);
    let inflated =
        zlib_decompress(&wrapped, deflated_text.len() + 16).expect("crate zlib decoder accepts");
    assert_eq!(inflated, deflated_text, "crate decoder cross-check");

    // Deflate must actually compress repetitive input below stored size.
    assert!(
        entries[1].compressed_size < entries[1].uncompressed_size,
        "deflate shrinks repetitive payload"
    );
}

#[test]
fn zip_finish_is_byte_deterministic() {
    let build = || {
        let mut w = ZipWriter::new();
        w.add_stored("one", b"1");
        w.add_deflated("two", b"22222");
        w.finish()
    };
    assert_eq!(build(), build(), "identical inputs give identical archives");
}

#[test]
fn zip_empty_archive_is_valid() {
    let bytes = ZipWriter::new().finish();
    assert_eq!(bytes.len(), 22, "empty archive is just the EOCD record");
    assert!(parse_central_directory(&bytes).is_empty());
}

// ---------------------------------------------------------------------------
// EPUB rendering.

const FIXTURE: &str = "\
# Intro

Welcome to the *guide*. A [link](https://example.com) and an inline $x^2+y^2$ formula.

## Getting Started

First line.  \nSecond line after a hard break.

- [ ] unchecked task
- [x] checked task

```rust
fn main() { println!(\"hi\"); }
```

| Col A | Col B |
|:------|------:|
| 1     | 2     |

![chart](chart.png \"Chart\")

---

# Intro

A duplicate heading exercises anchor collision suffixes.

> A quote with <b>raw html</b> inside.

Footnote here[^n1].

[^n1]: The note text.
";

fn fixture_epub(opts: &HtmlOptions) -> Vec<u8> {
    let doc = parse_markdown(FIXTURE);
    epub::render_epub(&doc, opts).expect("epub renders")
}

#[test]
fn epub_mimetype_is_first_and_stored() {
    let bytes = fixture_epub(&HtmlOptions::default());

    // First record in the file is the mimetype local header.
    assert_eq!(u32le(&bytes, 0), 0x0403_4B50, "starts with local header");
    assert_eq!(u16le(&bytes, 8), 0, "mimetype is STORED (method 0)");
    assert_eq!(u16le(&bytes, 6), 1 << 11, "UTF-8 flag set");
    let name_len = u16le(&bytes, 26) as usize;
    let extra_len = u16le(&bytes, 28) as usize;
    assert_eq!(extra_len, 0, "no extra field on mimetype");
    assert_eq!(
        &bytes[30..30 + name_len],
        b"mimetype",
        "first entry is mimetype"
    );
    let data = &bytes[30 + name_len..30 + name_len + 20];
    assert_eq!(data, b"application/epub+zip", "exact mimetype payload");
    assert_eq!(
        u32le(&bytes, 14),
        crc32(b"application/epub+zip"),
        "mimetype crc"
    );

    // Fixed entry order overall.
    let entries = parse_central_directory(&bytes);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "mimetype",
            "META-INF/container.xml",
            "OEBPS/content.opf",
            "OEBPS/nav.xhtml",
            "OEBPS/chapter-1.xhtml",
            "OEBPS/style.css"
        ],
        "EPUB entry order is fixed"
    );
}

#[test]
fn epub_container_and_opf_are_well_formed() {
    let bytes = fixture_epub(&HtmlOptions::default());

    let container = entry_text(&bytes, "META-INF/container.xml");
    assert_well_formed_xml(&container);
    assert!(
        container.contains("full-path=\"OEBPS/content.opf\""),
        "container points at the package document"
    );

    let opf = entry_text(&bytes, "OEBPS/content.opf");
    assert_well_formed_xml(&opf);
    assert!(opf.contains("version=\"3.0\""), "EPUB 3 package version");
    assert!(
        opf.contains("unique-identifier=\"bookid\""),
        "unique identifier hook"
    );
    assert!(
        opf.contains("<dc:title>Intro</dc:title>"),
        "title from first heading"
    );
    assert!(
        opf.contains("<dc:language>en</dc:language>"),
        "default language"
    );
    assert!(
        opf.contains("<dc:identifier id=\"bookid\">urn:uuid:"),
        "deterministic uuid identifier"
    );
    assert!(
        opf.contains("<meta property=\"dcterms:modified\">1970-01-01T00:00:00Z</meta>"),
        "schema-mandated modified field pinned to the epoch constant"
    );
    assert!(
        opf.contains("properties=\"nav\""),
        "nav manifest item carries the nav property"
    );
    assert!(
        opf.contains("<itemref idref=\"chapter-1\"/>"),
        "spine references the chapter"
    );
}

#[test]
fn epub_chapter_and_nav_are_well_formed_xhtml() {
    let bytes = fixture_epub(&HtmlOptions::default());

    let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&chapter);
    assert!(
        chapter.contains("<html xmlns=\"http://www.w3.org/1999/xhtml\""),
        "XHTML namespace"
    );
    assert!(
        chapter.contains("lang=\"en\" xml:lang=\"en\""),
        "language attributes"
    );
    assert!(
        chapter.contains("<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>"),
        "stylesheet link"
    );
    // HTML5 void elements rewritten to XML self-closing forms.
    assert!(
        chapter.contains("<hr/>"),
        "thematic break self-closed: {chapter}"
    );
    assert!(chapter.contains("<br/>"), "hard break self-closed");
    assert!(
        chapter.contains("<img src=\"chart.png\" alt=\"chart\" title=\"Chart\"/>"),
        "image self-closed"
    );
    assert!(
        chapter.contains("<input type=\"checkbox\" disabled=\"disabled\"/>"),
        "unchecked task checkbox legalized"
    );
    assert!(
        chapter.contains("<input type=\"checkbox\" disabled=\"disabled\" checked=\"checked\"/>"),
        "checked task checkbox legalized"
    );
    // Raw inline HTML is escaped, not passed through.
    assert!(
        chapter.contains("&lt;b&gt;raw html&lt;/b&gt;"),
        "raw HTML escaped"
    );
    assert!(!chapter.contains("<b>raw html</b>"), "no raw passthrough");

    let nav = entry_text(&bytes, "OEBPS/nav.xhtml");
    assert_well_formed_xml(&nav);
    assert!(
        nav.contains("<nav epub:type=\"toc\" id=\"toc\">"),
        "EPUB 3 nav landmark"
    );
}

#[test]
fn epub_nav_lists_headings_with_matching_chapter_anchors() {
    let bytes = fixture_epub(&HtmlOptions::default());
    let nav = entry_text(&bytes, "OEBPS/nav.xhtml");
    let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");

    for (id, text) in [
        ("intro", "Intro"),
        ("getting-started", "Getting Started"),
        ("intro-2", "Intro"),
    ] {
        let link = format!("<a href=\"chapter-1.xhtml#{id}\">{text}</a>");
        assert!(nav.contains(&link), "nav links to #{id}");
        let anchor = format!("id=\"{id}\"");
        assert!(chapter.contains(&anchor), "chapter carries id={id}");
    }
}

#[test]
fn epub_render_is_byte_deterministic() {
    let a = fixture_epub(&HtmlOptions::default());
    let b = fixture_epub(&HtmlOptions::default());
    assert_eq!(a, b, "two renders of the same document are byte-identical");
}

#[test]
fn epub_identifier_is_content_derived_and_stable() {
    let first = entry_text(&fixture_epub(&HtmlOptions::default()), "OEBPS/content.opf");
    let ident = |opf: &str| {
        let start = opf
            .find("<dc:identifier id=\"bookid\">")
            .expect("identifier")
            + "<dc:identifier id=\"bookid\">".len();
        let end = opf[start..]
            .find("</dc:identifier>")
            .expect("identifier close");
        opf[start..start + end].to_string()
    };
    let id_a = ident(&first);
    assert!(id_a.starts_with("urn:uuid:"), "uuid urn shape");
    assert_eq!(
        id_a.len(),
        "urn:uuid:".len() + 36,
        "canonical 8-4-4-4-12 uuid"
    );

    // Same input → same identifier; different input → different identifier.
    let again = entry_text(&fixture_epub(&HtmlOptions::default()), "OEBPS/content.opf");
    assert_eq!(ident(&again), id_a, "identifier stable across renders");

    let other_doc = parse_markdown("# Something Else Entirely\n\nDifferent body.\n");
    let other = epub::render_epub(&other_doc, &HtmlOptions::default()).expect("renders");
    let other_opf = entry_text(&other, "OEBPS/content.opf");
    assert_ne!(ident(&other_opf), id_a, "identifier tracks content");
}

#[test]
fn epub_title_and_language_are_xml_escaped() {
    let opts = HtmlOptions {
        title: Some("A & B <C> \"Q\" 's'".to_string()),
        lang: Some("fr".to_string()),
        ..HtmlOptions::default()
    };
    let bytes = fixture_epub(&opts);

    let opf = entry_text(&bytes, "OEBPS/content.opf");
    assert_well_formed_xml(&opf);
    assert!(
        opf.contains("<dc:title>A &amp; B &lt;C&gt; \"Q\" 's'</dc:title>"),
        "opf title escaped"
    );
    assert!(
        opf.contains("<dc:language>fr</dc:language>"),
        "language honored"
    );

    let chapter = entry_text(&bytes, "OEBPS/chapter-1.xhtml");
    assert_well_formed_xml(&chapter);
    assert!(
        chapter.contains("lang=\"fr\" xml:lang=\"fr\""),
        "chapter language"
    );
    assert!(
        chapter.contains("<title>A &amp; B &lt;C&gt; \"Q\" 's'</title>"),
        "chapter title escaped"
    );

    let nav = entry_text(&bytes, "OEBPS/nav.xhtml");
    assert_well_formed_xml(&nav);
}

#[test]
fn epub_empty_document_is_valid() {
    let doc = parse_markdown("");
    let bytes = epub::render_epub(&doc, &HtmlOptions::default()).expect("empty doc renders");

    let entries = parse_central_directory(&bytes);
    assert_eq!(entries.len(), 6, "all six EPUB entries present");
    for name in [
        "META-INF/container.xml",
        "OEBPS/content.opf",
        "OEBPS/nav.xhtml",
        "OEBPS/chapter-1.xhtml",
    ] {
        assert_well_formed_xml(&entry_text(&bytes, name));
    }
    let opf = entry_text(&bytes, "OEBPS/content.opf");
    assert!(
        opf.contains("<dc:title>Document</dc:title>"),
        "fallback title"
    );
    let nav = entry_text(&bytes, "OEBPS/nav.xhtml");
    assert!(
        nav.contains("<li><a href=\"chapter-1.xhtml\">Document</a></li>"),
        "nav falls back to a single document link"
    );
}
