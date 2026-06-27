//! Tests for the clean-room TTF/OTF font reader. Tests may use `unwrap`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::{Font, FontError};

#[test]
fn rejects_non_font_bytes() {
    assert!(matches!(
        Font::parse(b"not a font at all".to_vec()),
        Err(FontError::BadMagic)
    ));
    assert!(matches!(
        Font::parse(vec![0u8; 2]),
        Err(FontError::Truncated)
    ));
}

/// Hand-built minimal sfnt: head/maxp/hhea/hmtx + a format-12 cmap mapping
/// 'A'->gid1, 'B'->gid2. Hermetic + deterministic (no external font needed).
#[test]
fn parses_synthetic_format12_font() {
    let font = Font::parse(build_synthetic_font()).expect("synthetic font parses");
    assert_eq!(font.units_per_em, 1000);
    assert_eq!(font.num_glyphs, 3);
    assert_eq!(font.ascent, 800);
    assert_eq!(font.descent, -200);

    assert_eq!(font.glyph_index('A'), 1);
    assert_eq!(font.glyph_index('B'), 2);
    assert_eq!(font.glyph_index('Z'), 0); // unmapped -> .notdef

    // hmtx advances: gid0=500, gid1=600, gid2=700.
    assert_eq!(font.advance_width(1), 600);
    assert_eq!(font.advance_width(2), 700);
    // 'A' is gid1 -> 600/1000 em -> 600/1000.
    assert_eq!(font.advance_1000('A'), 600);
}

#[test]
fn parses_real_dejavu_when_available() {
    let path = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("skipping: {path} not present (real-font validation)");
        return;
    };
    let font = Font::parse(bytes).expect("DejaVu parses");
    assert_eq!(font.units_per_em, 2048, "DejaVu uses 2048 upm");
    assert!(font.num_glyphs > 1000);
    assert!(font.ascent > 0 && font.descent < 0);

    // Distinct, present glyphs.
    assert_ne!(font.glyph_index('A'), 0);
    assert_ne!(font.glyph_index('A'), font.glyph_index('B'));
    assert_eq!(font.glyph_index('\u{1}'), 0); // control char unmapped

    // Proportional metrics: a space is narrower than an 'M'.
    assert!(font.advance_1000(' ') < font.advance_1000('M'));
    assert!(font.advance_1000('i') < font.advance_1000('m'));
    // 'A' advance in a sane proportional range.
    let a = font.advance_1000('A');
    assert!((400..=1200).contains(&a), "unexpected 'A' advance: {a}");
}

// ---- synthetic font builder -------------------------------------------------

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}
fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn build_synthetic_font() -> Vec<u8> {
    // head: 54 bytes, unitsPerEm (u16) at offset 18.
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&be16(1000));

    // maxp v0.5: version (u32) + numGlyphs (u16).
    let mut maxp = Vec::new();
    maxp.extend_from_slice(&be32(0x0000_5000));
    maxp.extend_from_slice(&be16(3));

    // hhea: 36 bytes; ascender@4, descender@6, lineGap@8, numberOfHMetrics@34.
    let mut hhea = vec![0u8; 36];
    hhea[4..6].copy_from_slice(&be16(800)); // ascender
    hhea[6..8].copy_from_slice(&be16((-200i16) as u16)); // descender
    hhea[34..36].copy_from_slice(&be16(3)); // numberOfHMetrics

    // hmtx: 3 longHorMetric (advanceWidth u16 + lsb i16).
    let mut hmtx = Vec::new();
    for aw in [500u16, 600, 700] {
        hmtx.extend_from_slice(&be16(aw));
        hmtx.extend_from_slice(&be16(0));
    }

    // cmap: header + one (3,10) record -> format-12 subtable, group 65..=66 -> gid 1.
    let mut cmap = Vec::new();
    cmap.extend_from_slice(&be16(0)); // version
    cmap.extend_from_slice(&be16(1)); // numTables
    cmap.extend_from_slice(&be16(3)); // platformID = 3 (Windows)
    cmap.extend_from_slice(&be16(10)); // encodingID = 10 (UCS-4)
    cmap.extend_from_slice(&be32(12)); // subtable offset (4 + 8)
    // format-12 subtable:
    cmap.extend_from_slice(&be16(12)); // format
    cmap.extend_from_slice(&be16(0)); // reserved
    cmap.extend_from_slice(&be32(16 + 12)); // length = header + 1 group
    cmap.extend_from_slice(&be32(0)); // language
    cmap.extend_from_slice(&be32(1)); // numGroups
    cmap.extend_from_slice(&be32(65)); // startCharCode 'A'
    cmap.extend_from_slice(&be32(66)); // endCharCode   'B'
    cmap.extend_from_slice(&be32(1)); // startGlyphID

    let tables: [(&[u8; 4], Vec<u8>); 5] = [
        (b"cmap", cmap),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"maxp", maxp),
    ];

    let n = tables.len() as u16;
    let dir_len = 12 + tables.len() * 16;
    let mut body = Vec::new();
    let mut records = Vec::new();
    let mut offset = dir_len;
    for (tag, bytes) in &tables {
        records.push((**tag, offset as u32, bytes.len() as u32));
        body.extend_from_slice(bytes);
        while body.len() % 4 != 0 {
            body.push(0); // 4-byte align the next table
        }
        offset = dir_len + body.len();
    }

    let mut out = Vec::new();
    out.extend_from_slice(&be32(0x0001_0000)); // sfnt version
    out.extend_from_slice(&be16(n)); // numTables
    out.extend_from_slice(&be16(0)); // searchRange
    out.extend_from_slice(&be16(0)); // entrySelector
    out.extend_from_slice(&be16(0)); // rangeShift
    for (tag, off, len) in &records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&be32(0)); // checksum (unchecked by reader)
        out.extend_from_slice(&be32(*off));
        out.extend_from_slice(&be32(*len));
    }
    out.extend_from_slice(&body);
    out
}
