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

#[test]
fn malformed_required_table_offsets_fail_without_unwinding() {
    let result = std::panic::catch_unwind(|| Font::parse(font_with_bad_required_table_offsets()));

    assert!(result.is_ok(), "malformed font offsets must not panic");
    assert!(matches!(
        result.unwrap(),
        Err(FontError::Truncated | FontError::NoUnicodeCmap)
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
    assert_eq!(font.kerning('A', 'B'), 0);
}

fn font_with_bad_required_table_offsets() -> Vec<u8> {
    let tags = [b"head", b"maxp", b"hhea", b"hmtx", b"cmap"];
    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    font.extend_from_slice(&(tags.len() as u16).to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes());
    font.extend_from_slice(&0u16.to_be_bytes());
    for tag in tags {
        font.extend_from_slice(tag);
        font.extend_from_slice(&0u32.to_be_bytes());
        font.extend_from_slice(&u32::MAX.to_be_bytes());
        font.extend_from_slice(&64u32.to_be_bytes());
    }
    font
}

#[test]
fn parses_synthetic_kern_format0_pair() {
    let font = Font::parse(build_synthetic_font_with_kern()).expect("synthetic font parses");

    assert_eq!(font.glyph_index('A'), 1);
    assert_eq!(font.glyph_index('B'), 2);
    assert_eq!(font.kerning_between_glyphs(1, 2), -80);
    assert_eq!(font.kerning('A', 'B'), -80);
    assert_eq!(font.kerning_1000('A', 'B'), -80);
    assert_eq!(font.kerning('B', 'A'), 0);
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

#[test]
fn synthetic_font_has_no_glyf_outlines() {
    let font = Font::parse(build_synthetic_font()).unwrap();
    assert!(!font.has_glyf_outlines());
    assert!(font.glyph_data(1).is_none());
    assert!(font.glyph_bbox(1).is_none());
    assert!(!font.is_composite(1));
    assert!(font.glyph_components(1).is_empty());
}

#[test]
fn truncated_composite_glyph_does_not_read_past_its_loca_range() {
    let font = Font::parse(build_synthetic_truncated_composite_font_with_end(11)).unwrap();

    assert!(font.has_glyf_outlines());
    assert!(
        font.is_composite(1),
        "the glyph header marks gid 1 as composite"
    );
    assert_eq!(
        font.glyph_data(1).map(<[u8]>::len),
        Some(11),
        "loca intentionally exposes a glyph ending inside the first component record"
    );
    assert!(
        font.glyph_components(1).is_empty(),
        "component parsing must stop at the glyph range, not keep reading trailing glyf bytes"
    );
}

#[test]
fn truncated_composite_glyph_does_not_accept_partial_component_args() {
    let font = Font::parse(build_synthetic_truncated_composite_font_with_end(14)).unwrap();

    assert!(font.is_composite(1));
    assert_eq!(
        font.glyph_data(1).map(<[u8]>::len),
        Some(14),
        "loca exposes flags and component id but omits the required argument bytes"
    );
    assert!(
        font.glyph_components(1).is_empty(),
        "component parsing must reject records whose full argument/transform payload is truncated"
    );
}

#[test]
fn reads_dejavu_glyf_outlines_when_available() {
    let path = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("skipping: {path} not present (glyf outline validation)");
        return;
    };
    let font = Font::parse(bytes).unwrap();
    assert!(font.has_glyf_outlines(), "DejaVu has TrueType outlines");

    // 'A' is a non-empty, simple glyph with a sensible bounding box.
    let a = font.glyph_index('A');
    let data = font.glyph_data(a).expect("'A' has glyf data");
    assert!(!data.is_empty());
    let bbox = font.glyph_bbox(a).expect("'A' has a bbox");
    assert!(
        bbox[2] > bbox[0] && bbox[3] > bbox[1],
        "xMax>xMin, yMax>yMin"
    );
    assert!(!font.is_composite(a));
    assert!(font.glyph_components(a).is_empty());

    // Space is an empty glyph (advance only, no contours).
    let sp = font.glyph_index(' ');
    assert_eq!(font.glyph_data(sp).map(<[u8]>::len), Some(0));
    assert!(font.glyph_bbox(sp).is_none());

    // The composite parser: find a composite glyph and verify its components are
    // valid glyph ids (accented letters are typically base + diacritic).
    let mut found_composite = false;
    for gid in 0..font.num_glyphs.min(2000) {
        if font.is_composite(gid) {
            let comps = font.glyph_components(gid);
            assert!(
                !comps.is_empty(),
                "composite glyph must reference components"
            );
            assert!(comps.iter().all(|&c| c < font.num_glyphs));
            found_composite = true;
            break;
        }
    }
    assert!(found_composite, "DejaVu should contain composite glyphs");
}

/// The vendored Plex + Computer Modern fonts (committed under fonts/) must parse
/// with the clean-room reader — this is what the PDF embedder will subset.
#[test]
fn parses_bundled_plex_and_cm_fonts() {
    for (path, name) in [
        ("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf", "Plex Sans"),
        ("fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf", "Plex Sans Bold"),
        ("fonts/computer-modern/cmunrm.ttf", "CM Roman"),
        ("fonts/computer-modern/cmuntt.ttf", "CM Typewriter"),
    ] {
        let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("missing bundled font {path}"));
        let font = Font::parse(bytes).unwrap_or_else(|_| panic!("{name} should parse"));
        assert!(font.units_per_em > 0, "{name} upm");
        assert!(font.num_glyphs > 100, "{name} glyph count");
        assert!(
            font.has_glyf_outlines(),
            "{name} has glyf outlines (subsettable)"
        );
        let a = font.glyph_index('A');
        assert_ne!(a, 0, "{name} maps 'A'");
        assert!(
            font.glyph_data(a).is_some_and(|d| !d.is_empty()),
            "{name} 'A' has outline data"
        );
        assert!(font.advance_1000('A') > 0, "{name} 'A' advance");
    }
    // CM Typewriter is a monospaced typewriter face: every advance is equal.
    let mono = Font::parse(std::fs::read("fonts/computer-modern/cmuntt.ttf").unwrap()).unwrap();
    assert_eq!(
        mono.advance_1000('i'),
        mono.advance_1000('M'),
        "CM Typewriter is monospaced"
    );
    // Plex Sans is proportional: 'i' is narrower than 'M'.
    let sans =
        Font::parse(std::fs::read("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf").unwrap()).unwrap();
    assert!(
        sans.advance_1000('i') < sans.advance_1000('M'),
        "Plex Sans is proportional"
    );
}

/// The subsetter must produce a much smaller font that our OWN reader can
/// re-parse, with the kept characters' glyphs + metrics preserved and dropped
/// characters mapping to `.notdef`.
#[test]
fn subsets_plex_sans_and_reparses() {
    let orig = std::fs::read("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf").unwrap();
    let font = Font::parse(orig.clone()).unwrap();
    let keep = ['H', 'e', 'l', 'o', ' ', 'W', 'r', 'd', 'A', 'g', 'é'];
    let sub = font.subset(&keep).expect("subset produced");

    assert!(
        sub.len() < orig.len() / 3,
        "subset should be much smaller: {} vs {}",
        sub.len(),
        orig.len()
    );

    let re = Font::parse(sub).expect("subset re-parses with our reader");
    assert_eq!(re.units_per_em, font.units_per_em, "upm preserved");

    for &ch in &keep {
        if ch == ' ' {
            continue;
        }
        let g = re.glyph_index(ch);
        assert_ne!(g, 0, "subset still maps {ch:?}");
        assert!(
            re.glyph_data(g).is_some_and(|d| !d.is_empty()),
            "{ch:?} keeps its outline"
        );
        assert_eq!(
            re.advance_1000(ch),
            font.advance_1000(ch),
            "{ch:?} advance preserved"
        );
    }
    assert_eq!(re.glyph_index('Z'), 0, "dropped char -> .notdef");
    // notdef + kept glyphs (+ any composite components), but nowhere near the
    // full face.
    assert!(
        re.num_glyphs >= 9 && re.num_glyphs < 60,
        "glyph count {}",
        re.num_glyphs
    );
}

#[test]
fn subset_glyphs_ignores_out_of_range_seed_glyph_ids() {
    let orig = std::fs::read("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf").unwrap();
    let font = Font::parse(orig).unwrap();

    let invalid_at_count = font.num_glyphs;
    let (sub, remap) = font
        .subset_glyphs(&[invalid_at_count, u16::MAX], &[])
        .expect("subset with only invalid external glyph ids still keeps .notdef");

    assert_eq!(remap.get(&0), Some(&0));
    assert!(
        !remap.contains_key(&invalid_at_count),
        "gid == num_glyphs is outside the original face"
    );
    assert!(
        !remap.contains_key(&u16::MAX),
        "arbitrary out-of-range glyph ids must not enter the subset map"
    );
    let reparsed = Font::parse(sub).expect("subset re-parses");
    assert_eq!(reparsed.num_glyphs, 1, "only .notdef remains");
}

/// DejaVu's accented letters are composite glyphs — exercise the transitive
/// closure + component-id rewriting end to end.
#[test]
fn subsets_composite_glyphs_when_dejavu_available() {
    let path = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";
    let Ok(orig) = std::fs::read(path) else {
        eprintln!("skipping: {path} not present (composite subset validation)");
        return;
    };
    let font = Font::parse(orig).unwrap();
    let g_old = font.glyph_index('é');
    let was_composite = font.is_composite(g_old);

    let sub = font.subset(&['é', 'A']).expect("subset produced");
    let re = Font::parse(sub).expect("subset re-parses");
    let g = re.glyph_index('é');
    assert_ne!(g, 0, "subset maps 'é'");
    if was_composite {
        assert!(re.is_composite(g), "'é' stays composite after subsetting");
        // Renumbered component ids must be valid within the subset.
        let comps = re.glyph_components(g);
        assert!(!comps.is_empty());
        assert!(
            comps.iter().all(|&c| c < re.num_glyphs),
            "components renumbered in range"
        );
    }
}

#[test]
fn subset_strips_composite_glyph_instructions() {
    let font = Font::parse(build_synthetic_composite_instruction_font()).unwrap();
    let sub = font.subset(&['A']).expect("subset produced");
    let re = Font::parse(sub).expect("subset re-parses");
    let g = re.glyph_index('A');

    assert_ne!(g, 0, "subset maps synthetic composite glyph");
    assert!(re.is_composite(g), "composite glyph survives subsetting");
    let data = re.glyph_data(g).expect("subset exposes composite data");
    let mut p = 10usize;
    let mut component_count = 0usize;
    loop {
        let flags = rd_u16(data, p);
        assert_eq!(
            flags & 0x0100,
            0,
            "WE_HAVE_INSTRUCTIONS must be cleared from every component record"
        );
        component_count += 1;
        p += 4;
        p += if flags & 0x0001 != 0 { 4 } else { 2 };
        if flags & 0x0008 != 0 {
            p += 2;
        } else if flags & 0x0040 != 0 {
            p += 4;
        } else if flags & 0x0080 != 0 {
            p += 8;
        }
        if flags & 0x0020 == 0 {
            break;
        }
    }
    assert_eq!(component_count, 2, "synthetic glyph has two components");
    assert!(
        data.len() <= 28,
        "stripped composite should contain header and two component records, got {} bytes",
        data.len()
    );
    assert!(
        !data.windows(3).any(|w| w == [0xAA, 0xBB, 0xCC]),
        "subset glyph data must not retain stripped instruction bytes"
    );
}

// ---- synthetic font builder -------------------------------------------------

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}
fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

fn build_synthetic_font() -> Vec<u8> {
    build_synthetic_font_with_extra_tables(Vec::new())
}

fn build_synthetic_font_with_kern() -> Vec<u8> {
    build_synthetic_font_with_extra_tables(vec![(b"kern", build_synthetic_kern_table())])
}

fn build_synthetic_truncated_composite_font_with_end(gid1_end: u32) -> Vec<u8> {
    // head: long loca format so gid 1 can deliberately end inside a component record.
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&be16(1000));
    head[50..52].copy_from_slice(&be16(1));

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&be32(0x0000_5000));
    maxp.extend_from_slice(&be16(2));

    let mut hhea = vec![0u8; 36];
    hhea[4..6].copy_from_slice(&be16(800));
    hhea[6..8].copy_from_slice(&be16((-200i16) as u16));
    hhea[34..36].copy_from_slice(&be16(2));

    let mut hmtx = Vec::new();
    for aw in [500u16, 600] {
        hmtx.extend_from_slice(&be16(aw));
        hmtx.extend_from_slice(&be16(0));
    }

    let mut cmap = Vec::new();
    cmap.extend_from_slice(&be16(0));
    cmap.extend_from_slice(&be16(1));
    cmap.extend_from_slice(&be16(3));
    cmap.extend_from_slice(&be16(10));
    cmap.extend_from_slice(&be32(12));
    cmap.extend_from_slice(&be16(12));
    cmap.extend_from_slice(&be16(0));
    cmap.extend_from_slice(&be32(16 + 12));
    cmap.extend_from_slice(&be32(0));
    cmap.extend_from_slice(&be32(1));
    cmap.extend_from_slice(&be32(65));
    cmap.extend_from_slice(&be32(65));
    cmap.extend_from_slice(&be32(1));

    // gid0 is empty: loca[0] == loca[1] == 0.
    // gid1 is declared with a caller-controlled end offset. The following bytes
    // remain inside the glyf table as trailing/padding data and must not be
    // interpreted as a component record unless the whole record is in range.
    let mut glyf = Vec::new();
    glyf.extend_from_slice(&(-1i16).to_be_bytes());
    glyf.extend_from_slice(&[0u8; 8]);
    glyf.push(0x00);
    glyf.extend_from_slice(&[0x20, 0x12, 0x34, 0x56, 0x78]);

    let mut loca = Vec::new();
    for off in [0u32, 0, gid1_end] {
        loca.extend_from_slice(&be32(off));
    }

    assemble_synthetic_font(vec![
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
    ])
}

fn build_synthetic_composite_instruction_font() -> Vec<u8> {
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&be16(1000));
    head[50..52].copy_from_slice(&be16(1));

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&be32(0x0000_5000));
    maxp.extend_from_slice(&be16(2));

    let mut hhea = vec![0u8; 36];
    hhea[4..6].copy_from_slice(&be16(800));
    hhea[6..8].copy_from_slice(&be16((-200i16) as u16));
    hhea[34..36].copy_from_slice(&be16(2));

    let mut hmtx = Vec::new();
    for aw in [500u16, 600] {
        hmtx.extend_from_slice(&be16(aw));
        hmtx.extend_from_slice(&be16(0));
    }

    let mut cmap = Vec::new();
    cmap.extend_from_slice(&be16(0));
    cmap.extend_from_slice(&be16(1));
    cmap.extend_from_slice(&be16(3));
    cmap.extend_from_slice(&be16(10));
    cmap.extend_from_slice(&be32(12));
    cmap.extend_from_slice(&be16(12));
    cmap.extend_from_slice(&be16(0));
    cmap.extend_from_slice(&be32(16 + 12));
    cmap.extend_from_slice(&be32(0));
    cmap.extend_from_slice(&be32(1));
    cmap.extend_from_slice(&be32(65));
    cmap.extend_from_slice(&be32(65));
    cmap.extend_from_slice(&be32(1));

    let mut glyf = Vec::new();
    glyf.extend_from_slice(&(-1i16).to_be_bytes()); // composite glyph
    glyf.extend_from_slice(&[0u8; 8]); // bbox
    glyf.extend_from_slice(&be16(0x0121)); // ARG_WORDS | MORE | WE_HAVE_INSTRUCTIONS
    glyf.extend_from_slice(&be16(0)); // component glyph id
    glyf.extend_from_slice(&[0u8; 4]); // word args
    glyf.extend_from_slice(&be16(0x0101)); // ARG_WORDS | WE_HAVE_INSTRUCTIONS
    glyf.extend_from_slice(&be16(0)); // component glyph id
    glyf.extend_from_slice(&[0u8; 4]); // word args
    glyf.extend_from_slice(&be16(3)); // instructionLength
    glyf.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // instructions to strip
    let gid1_end = glyf.len() as u32;

    let mut loca = Vec::new();
    for off in [0u32, 0, gid1_end] {
        loca.extend_from_slice(&be32(off));
    }

    assemble_synthetic_font(vec![
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
    ])
}

fn build_synthetic_font_with_extra_tables(extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
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

    let mut tables: Vec<(&'static [u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"maxp", maxp),
    ];
    tables.extend(extra);

    assemble_synthetic_font(tables)
}

fn assemble_synthetic_font(mut tables: Vec<(&'static [u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by(|a, b| a.0.cmp(b.0));
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

fn build_synthetic_kern_table() -> Vec<u8> {
    let mut kern = Vec::new();
    kern.extend_from_slice(&be16(0)); // table version
    kern.extend_from_slice(&be16(1)); // subtable count

    let subtable_len = 6 + 8 + 6;
    kern.extend_from_slice(&be16(0)); // subtable version
    kern.extend_from_slice(&be16(subtable_len));
    kern.extend_from_slice(&be16(0x0001)); // format 0 + horizontal
    kern.extend_from_slice(&be16(1)); // nPairs
    kern.extend_from_slice(&be16(6)); // searchRange
    kern.extend_from_slice(&be16(0)); // entrySelector
    kern.extend_from_slice(&be16(0)); // rangeShift
    kern.extend_from_slice(&be16(1)); // left glyph A
    kern.extend_from_slice(&be16(2)); // right glyph B
    kern.extend_from_slice(&(-80i16).to_be_bytes());
    kern
}

// ===========================================================================
// Real-font edge-path coverage (mock-free).
//
// These tests drive the reader's harder-to-reach branches with REAL inputs:
// exact metrics/glyph ids from the bundled faces, exact `FontError` Display
// strings, real kerning/ligature values, and bundled (or system) fonts that
// have been *deliberately damaged* (a truncated table, a corrupted offset,
// length, magic, or format byte) to exercise the bounds-checked rejection
// paths. No synthesized font structs are introduced here — every input is a
// real font, possibly mutated by hand.
// ===========================================================================

const PLEX_REGULAR_PATH: &str = "fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf";
const DEJAVU_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

fn plex_regular_bytes() -> Vec<u8> {
    std::fs::read(PLEX_REGULAR_PATH).unwrap()
}

fn rd_u16(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}
fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn wr_u16(d: &mut [u8], o: usize, v: u16) {
    d[o..o + 2].copy_from_slice(&v.to_be_bytes());
}

/// Offset of a table's 16-byte directory record (where the tag lives).
fn table_record(d: &[u8], tag: &[u8; 4]) -> usize {
    let n = rd_u16(d, 4) as usize;
    for i in 0..n {
        let rec = 12 + i * 16;
        if &d[rec..rec + 4] == tag {
            return rec;
        }
    }
    panic!("table {tag:?} not present in directory");
}

/// `(offset, length)` of a table, read straight from the sfnt directory.
fn table_loc(d: &[u8], tag: &[u8; 4]) -> (usize, usize) {
    let rec = table_record(d, tag);
    (rd_u32(d, rec + 8) as usize, rd_u32(d, rec + 12) as usize)
}

/// `FontError` renders a distinct, human-readable message for each variant.
#[test]
fn font_error_display_messages() {
    assert_eq!(
        FontError::BadMagic.to_string(),
        "not a TrueType/OpenType font"
    );
    assert_eq!(
        FontError::MissingTable("glyf").to_string(),
        "missing required font table: glyf"
    );
    assert_eq!(FontError::Truncated.to_string(), "font data is truncated");
    assert_eq!(
        FontError::NoUnicodeCmap.to_string(),
        "no usable Unicode cmap (format 4/12)"
    );
}

/// Several common BMP code points in the bundled Plex face resolve through the
/// format-4 cmap's `idRangeOffset` glyphIdArray indirection rather than the
/// `idDelta` fast path. Exact glyph ids are stable for the committed font.
#[test]
fn plex_cmap4_idrange_offset_indirection() {
    let font = Font::parse(plex_regular_bytes()).unwrap();
    // (cp, char, segment idRangeOffset != 0)
    assert_eq!(font.glyph_index(':'), 95);
    assert_eq!(font.glyph_index('['), 117);
    assert_eq!(font.glyph_index('{'), 119);
    assert_eq!(font.glyph_index(' '), 3);
    // idDelta fast-path glyph still resolves (cross-check, same subtable).
    assert_eq!(font.glyph_index('A'), 33);
}

/// Plex's GPOS `kern` feature carries a Pair-format-2 subtable whose first-glyph
/// class table is `ClassDef` format 1, covering only U+0390 (ΐ) and U+03CA (ϊ).
/// No earlier subtable covers those glyphs, so the pair is resolved there —
/// exercising the format-1 class lookup for an in-range (`== start`) glyph and a
/// below-start glyph.
#[test]
fn plex_gpos_pair_uses_classdef_format1() {
    let font = Font::parse(plex_regular_bytes()).unwrap();
    let kern = font.gpos_kerning();
    let dotted = font.glyph_index('\u{0390}'); // ΐ -> ClassDef start, class 1
    let dotless = font.glyph_index('\u{03CA}'); // ϊ -> below start, class 0
    let apostrophe = font.glyph_index('\'');
    assert_eq!(dotted, 612);
    assert_eq!(dotless, 611);
    assert_eq!(apostrophe, 99);
    // class1=1, class2(apostrophe)=1 -> matrix cell = +40 design units.
    assert_eq!(kern.pair(dotted, apostrophe), 40);
    // class1=0 (glyph below ClassDef start) -> matrix row 0, cell = 0.
    assert_eq!(kern.pair(dotless, apostrophe), 0);
}

/// A real face whose `head.unitsPerEm` is zeroed still parses, but every metric
/// scaled to 1/1000 em must short-circuit to 0 instead of dividing by zero.
#[test]
fn zero_units_per_em_yields_zero_scaled_metrics() {
    let mut bytes = plex_regular_bytes();
    let (head, _) = table_loc(&bytes, b"head");
    assert!(rd_u16(&bytes, head + 18) > 0, "upm starts non-zero");
    wr_u16(&mut bytes, head + 18, 0);
    let font = Font::parse(bytes).unwrap();
    assert_eq!(font.units_per_em, 0);
    assert_eq!(font.advance_1000('A'), 0);
    assert_eq!(font.kerning_1000('A', 'V'), 0);
}

/// Corrupting the `glyf` table tag leaves a parseable font with no usable
/// outlines: `has_glyf_outlines()` is false, glyph data is unavailable, and
/// subsetting (which requires TrueType outlines) returns `None`.
#[test]
fn damaged_glyf_tag_disables_outlines_and_subsetting() {
    let mut bytes = plex_regular_bytes();
    let rec = table_record(&bytes, b"glyf");
    bytes[rec..rec + 4].copy_from_slice(b"XXXX");
    let font = Font::parse(bytes).unwrap();
    assert!(!font.has_glyf_outlines());
    assert!(font.glyph_data(font.glyph_index('A')).is_none());
    assert!(font.subset(&['A', 'B']).is_none());
}

/// A `loca` entry pointing past the `glyf` table must make `glyph_range` reject
/// the glyph (end beyond the table) rather than read out of bounds.
#[test]
fn damaged_loca_out_of_range_glyph_is_rejected() {
    let mut bytes = plex_regular_bytes();
    let (loca, _) = table_loc(&bytes, b"loca");
    let a = Font::parse(bytes.clone()).unwrap().glyph_index('A');
    assert!(
        Font::parse(bytes.clone())
            .unwrap()
            .glyph_data(a)
            .is_some_and(|d| !d.is_empty()),
        "glyph 'A' has outline data before damage"
    );
    // Plex uses the short (offsets/2) loca format; set glyph A's END offset to
    // 0xFFFF -> byte offset 0x1FFFE, far past the glyf table.
    wr_u16(&mut bytes, loca + (a as usize + 1) * 2, 0xFFFF);
    let font = Font::parse(bytes).unwrap();
    assert!(font.glyph_data(a).is_none());
    assert!(font.glyph_bbox(a).is_none());
}

/// Damaging the cmap directory drives `select_cmap` through its lower-priority
/// ranking arms while still leaving a usable subtable selected: a `(3,10)`
/// format-4 record (ranks below `(3,1)`), and a platform-0 *format-6* record
/// (a Unicode subtable of an unsupported format, which is skipped).
#[test]
fn select_cmap_ranks_noncanonical_unicode_subtables() {
    let mut bytes = plex_regular_bytes();
    let (cmap, _) = table_loc(&bytes, b"cmap");
    // record layout after version(2)+count(2): [plat(2) enc(2) offset(4)] each.
    // record 1 = (1,0) format 6 -> set platform 0 (Unicode, unsupported format).
    wr_u16(&mut bytes, cmap + 4 + 8, 0);
    // record 2 = (3,1) format 4 -> set encoding 10 (non-canonical format-4).
    wr_u16(&mut bytes, cmap + 4 + 16 + 2, 10);
    let font = Font::parse(bytes).unwrap();
    // The (0,3) format-4 subtable is still selected; lookups remain intact.
    assert_eq!(font.glyph_index('A'), 33);
    assert_eq!(font.glyph_index('B'), 34);
}

/// Breaking the format-4 terminal segment's `endCode` (0xFFFF sentinel) makes a
/// lookup of the maximum BMP code point exceed every segment and fall through
/// the whole scan, still safely returning `.notdef`.
#[test]
fn damaged_cmap_sentinel_falls_through_segment_scan() {
    let mut bytes = plex_regular_bytes();
    let (cmap, _) = table_loc(&bytes, b"cmap");
    // record 0's subtable offset -> the selected format-4 subtable.
    let sub = cmap + rd_u32(&bytes, cmap + 4 + 4) as usize;
    assert_eq!(rd_u16(&bytes, sub), 4, "record 0 is format 4");
    let seg_count = rd_u16(&bytes, sub + 6) as usize / 2;
    let last_end = sub + 14 + (seg_count - 1) * 2;
    assert_eq!(
        rd_u16(&bytes, last_end),
        0xFFFF,
        "terminal sentinel present"
    );
    wr_u16(&mut bytes, last_end, 0xFFFD); // break the sentinel
    let font = Font::parse(bytes).unwrap();
    assert_eq!(font.glyph_index('\u{FFFF}'), 0); // falls through all segments
    assert_eq!(font.glyph_index('A'), 33); // low segment still maps
}

/// DejaVu ships a legacy `kern` v0 format-0 horizontal table (2727 pairs). The
/// classic pairs tighten, and varied lookups walk both directions of the pair
/// binary search; an absent pair exits with no adjustment.
#[test]
fn dejavu_legacy_kern_binary_search() {
    let Ok(bytes) = std::fs::read(DEJAVU_PATH) else {
        eprintln!("skipping: {DEJAVU_PATH} not present (legacy kern validation)");
        return;
    };
    let font = Font::parse(bytes).unwrap();
    for (l, r) in [
        ('A', 'V'),
        ('A', 'Y'),
        ('A', 'W'),
        ('L', 'T'),
        ('F', '.'),
        ('Y', 'A'),
    ] {
        assert!(
            font.kerning(l, r) < 0,
            "{l}{r} should tighten via legacy kern"
        );
    }
    let (a, v) = (font.glyph_index('A'), font.glyph_index('V'));
    assert!(font.kerning_between_glyphs(a, v) < 0);
    assert!(font.kerning_1000('A', 'V') < 0);
    // notdef/notdef is never a kern pair -> not-found exit of the search.
    assert_eq!(font.kerning_between_glyphs(0, 0), 0);
}

/// Each mutation makes `find_kern0` reject the (otherwise valid) DejaVu legacy
/// kern table at a different bounds check; the font must still parse and report
/// zero kerning, never panic.
#[test]
fn damaged_dejavu_kern_table_rejection_paths() {
    let Ok(orig) = std::fs::read(DEJAVU_PATH) else {
        eprintln!("skipping: {DEJAVU_PATH} not present (kern rejection validation)");
        return;
    };
    let (kern, _) = table_loc(&orig, b"kern");
    let sub = kern + 4; // first (and only) subtable

    let kern_after = |writes: &[(usize, u16)]| -> i16 {
        let mut b = orig.clone();
        for &(o, v) in writes {
            wr_u16(&mut b, o, v);
        }
        let font = Font::parse(b).expect("font still parses with a damaged kern table");
        font.kerning('A', 'V')
    };

    // table version != 0
    assert_eq!(kern_after(&[(kern, 1)]), 0);
    // first subtable is not format 0 (single subtable): loop skips it, ends.
    assert_eq!(kern_after(&[(sub + 4, 0x0101)]), 0);
    // not-format-0 subtable + a claimed second subtable past the table end.
    assert_eq!(kern_after(&[(sub + 4, 0x0101), (kern + 2, 2)]), 0);
    // subtable length larger than the whole kern table.
    assert_eq!(kern_after(&[(sub + 2, 0xFFFF)]), 0);
    // nPairs larger than the subtable can hold.
    assert_eq!(kern_after(&[(sub + 6, 0xFFFF)]), 0);
    // zero-length subtable.
    assert_eq!(kern_after(&[(sub + 2, 0)]), 0);
}

/// Sweep a truncation boundary through the GPOS and GSUB tables of a real font.
/// Every partial read inside the kern/ligature parsers must take its bounds-
/// checked rejection arm and yield a well-formed (possibly empty / degraded)
/// result rather than panicking. All required tables precede GPOS, so parsing
/// still succeeds at every boundary.
#[test]
fn truncated_layout_tables_degrade_without_panic() {
    let full = plex_regular_bytes();
    let (gpos_off, gpos_len) = table_loc(&full, b"GPOS");
    let (gsub_off, gsub_len) = table_loc(&full, b"GSUB");
    let gpos_end = gpos_off + gpos_len;
    let gsub_end = gsub_off + gsub_len;

    let base = Font::parse(full.clone()).unwrap();
    let (av_l, av_r) = (base.glyph_index('A'), base.glyph_index('V'));
    let (fg, ig) = (base.glyph_index('f'), base.glyph_index('i'));
    // Intact tables: full kerning + ligatures work.
    assert!(base.gpos_kerning().pair(av_l, av_r) < 0);
    assert_eq!(base.gsub_ligatures().substitute(&[fg, ig]).len(), 1);

    // GPOS cut boundaries: a fine sweep over the header + script/feature/lookup
    // lists, then a coarse sweep over the rest (lookup subtable data). GSUB sits
    // after GPOS, so these cuts also drop GSUB.
    let mut gpos_cuts: Vec<usize> = Vec::new();
    let mut c = gpos_off;
    while c <= gpos_off + 3000.min(gpos_len) {
        gpos_cuts.push(c);
        c += 1;
    }
    while c <= gpos_end {
        gpos_cuts.push(c);
        c += 13;
    }
    for cut in gpos_cuts {
        let mut bytes = full.clone();
        bytes.truncate(cut);
        let Ok(font) = Font::parse(bytes) else {
            continue;
        };
        // A/V never kerns positive: degraded -> 0, intact -> negative.
        let k = font.gpos_kerning().pair(av_l, av_r);
        assert!(k <= 0, "kern stays a sane i16 at cut {cut} (got {k})");
    }

    // GSUB cut boundaries: the whole (small) GSUB table, byte by byte. GPOS is
    // fully intact here, so we only probe ligature shaping.
    for cut in gsub_off..=gsub_end {
        let mut bytes = full.clone();
        bytes.truncate(cut);
        let Ok(font) = Font::parse(bytes) else {
            continue;
        };
        // Shaping stays well-formed: ligated (1) or passed through (2).
        let shaped = font.gsub_ligatures().substitute(&[fg, ig]);
        assert!(
            (1..=2).contains(&shaped.len()),
            "ligature substitution stays well-formed at cut {cut}"
        );
    }
}
