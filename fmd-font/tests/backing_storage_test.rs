//! Borrowed bundled bytes and owned host bytes must retain identical font
//! behavior, while only the owning path allocates a copy when cloned.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use fmd_font::Font;

const TTF: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
const CFF: &[u8] = include_bytes!("../fonts/test-cff/Bravura.otf");
const VARIABLE: &[u8] = include_bytes!("../fonts/test-variable/FmdPackedDeltasVF.ttf");

#[test]
fn static_font_and_its_clones_borrow_the_callers_bytes() {
    let font = Font::parse_static(TTF).expect("bundled static font");
    assert_eq!(font.as_sfnt().as_ptr(), TTF.as_ptr());
    assert_eq!(font.as_sfnt().len(), TTF.len());

    let mut cloned = font.clone();
    assert_eq!(cloned.as_sfnt().as_ptr(), TTF.as_ptr());
    // Parsed values remain independent even though immutable bytes are shared.
    let original_ascent = font.ascent;
    cloned.ascent += 1;
    assert_eq!(font.ascent, original_ascent);
    drop(font);
    assert_eq!(cloned.as_sfnt(), TTF);
    assert_ne!(cloned.glyph_index('A'), 0);
}

#[test]
fn owned_font_keeps_its_input_allocation_and_clones_its_bytes() {
    let input = TTF.to_vec();
    let input_ptr = input.as_ptr();
    let font = Font::parse(input).expect("owned host font");
    assert_eq!(font.as_sfnt().as_ptr(), input_ptr);

    let cloned = font.clone();
    assert_ne!(cloned.as_sfnt().as_ptr(), font.as_sfnt().as_ptr());
    assert_eq!(cloned.as_sfnt(), font.as_sfnt());
    drop(font);
    assert_eq!(cloned.as_sfnt(), TTF);
    assert_ne!(cloned.glyph_index('A'), 0);
}

fn compare_subsets(bytes: &'static [u8], text: &str) {
    let borrowed = Font::parse_static(bytes).expect("static font");
    let owned = Font::parse(bytes.to_vec()).expect("owned font");
    assert_eq!(borrowed.units_per_em, owned.units_per_em);
    assert_eq!(borrowed.num_glyphs, owned.num_glyphs);
    assert_eq!(borrowed.ascent, owned.ascent);
    assert_eq!(borrowed.descent, owned.descent);
    assert_eq!(borrowed.line_gap, owned.line_gap);
    let chars: Vec<_> = text.chars().collect();
    let glyphs: Vec<_> = chars
        .iter()
        .map(|&ch| {
            let gid = borrowed.glyph_index(ch);
            assert_ne!(gid, 0, "fixture must contain {ch:?}");
            assert_eq!(gid, owned.glyph_index(ch));
            assert_eq!(borrowed.advance_width(gid), owned.advance_width(gid));
            assert_eq!(borrowed.glyph_bbox(gid), owned.glyph_bbox(gid));
            assert_eq!(borrowed.glyph_data(gid), owned.glyph_data(gid));
            gid
        })
        .collect();

    let subset = borrowed
        .try_subset_glyphs(&glyphs, &chars)
        .expect("borrowed font subset");
    assert_eq!(
        subset,
        owned
            .try_subset_glyphs(&glyphs, &chars)
            .expect("owned font subset")
    );
    assert_eq!(
        borrowed.try_subset(&chars).expect("borrowed web subset"),
        owned.try_subset(&chars).expect("owned web subset")
    );

    let subset_ptr = subset.bytes.as_ptr();
    let reparsed = Font::parse(subset.bytes).expect("subset re-parses");
    assert_eq!(reparsed.as_sfnt().as_ptr(), subset_ptr);
    assert_ne!(reparsed.as_sfnt().as_ptr(), bytes.as_ptr());
    for (&ch, &gid) in chars.iter().zip(&glyphs) {
        let new_gid = reparsed.glyph_index(ch);
        assert_ne!(new_gid, 0);
        assert_eq!(reparsed.advance_width(new_gid), borrowed.advance_width(gid));
        assert_eq!(reparsed.glyph_bbox(new_gid), borrowed.glyph_bbox(gid));
    }
    assert_eq!(borrowed.as_sfnt().as_ptr(), bytes.as_ptr());
    assert_eq!(borrowed.as_sfnt(), bytes);
}

#[test]
fn borrowed_truetype_and_cff_subsets_match_owned_output() {
    compare_subsets(TTF, "AVffiéÅ");
    compare_subsets(CFF, "\u{e050}\u{e0a4}\u{e262}");
}

#[test]
fn borrowed_variable_font_instances_produce_independent_owned_bytes() {
    let borrowed = Font::parse_static(VARIABLE).expect("static variable font");
    let owned = Font::parse(VARIABLE.to_vec()).expect("owned variable font");
    for weight in [400.0, 650.0, 900.0] {
        let instance = borrowed.instance(weight).expect("static instance");
        let reference = owned.instance(weight).expect("owned instance");
        assert_eq!(instance.as_sfnt(), reference.as_sfnt());
        assert_ne!(instance.as_sfnt().as_ptr(), VARIABLE.as_ptr());
        assert!(instance.axes().is_empty());
        let cloned = instance.clone();
        assert_ne!(cloned.as_sfnt().as_ptr(), instance.as_sfnt().as_ptr());
        assert_eq!(cloned.as_sfnt(), instance.as_sfnt());
    }
    assert!(!borrowed.axes().is_empty());
    assert_eq!(borrowed.as_sfnt().as_ptr(), VARIABLE.as_ptr());
    assert_eq!(borrowed.as_sfnt(), VARIABLE);
}

#[test]
fn borrowed_malformed_fonts_retain_owned_parser_errors() {
    let cases: [&'static [u8]; 5] = [b"", b"not a font", &TTF[..4], &TTF[..12], &TTF[..64]];
    for bytes in cases {
        let expected = Font::parse(bytes.to_vec()).expect_err("malformed owned font");
        assert_eq!(
            Font::parse_static(bytes).expect_err("malformed static font"),
            expected
        );
    }
}

#[cfg(feature = "cjk-face")]
#[test]
fn bundled_cjk_bytes_are_shared_without_changing_subsets() {
    compare_subsets(fmd_font::bundled::noto_sans_cjk_sc(), "中文あア。！");
}
