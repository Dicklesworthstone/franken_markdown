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
