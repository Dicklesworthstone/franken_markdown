//! GSUB standard-ligature (`liga`) tests against the bundled fonts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::Font;

fn load(path: &str) -> Font {
    Font::parse(std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn plex_and_cm_ligate_fi() {
    for path in [
        "fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf",
        "fmd-font/fonts/computer-modern/cmunrm.ttf",
    ] {
        let f = load(path);
        let lig = f.gsub_ligatures();
        assert!(!lig.is_empty(), "{path} should define standard ligatures");

        // f + i -> a single 'fi' ligature glyph.
        let fi = lig.substitute(&[f.glyph_index('f'), f.glyph_index('i')]);
        assert_eq!(fi.len(), 1, "{path}: f+i collapses to one glyph");
        assert_ne!(fi[0], 0, "{path}: ligature glyph exists");
        assert_ne!(fi[0], f.glyph_index('f'), "{path}: ligature != 'f'");

        // A non-ligating pair is returned unchanged.
        let ab = lig.substitute(&[f.glyph_index('a'), f.glyph_index('b')]);
        assert_eq!(ab, vec![f.glyph_index('a'), f.glyph_index('b')]);
    }
}

#[test]
fn cm_ligates_fl_ff_and_collapses_ffi() {
    let f = load("fmd-font/fonts/computer-modern/cmunrm.ttf");
    let lig = f.gsub_ligatures();
    assert_eq!(
        lig.substitute(&[f.glyph_index('f'), f.glyph_index('l')])
            .len(),
        1,
        "f+l -> fl"
    );
    assert_eq!(
        lig.substitute(&[f.glyph_index('f'), f.glyph_index('f')])
            .len(),
        1,
        "f+f -> ff"
    );
    // f f i collapses via greedy longest match (ffi, or ff + i) — never 3 glyphs.
    let ffi = lig.substitute(&[f.glyph_index('f'), f.glyph_index('f'), f.glyph_index('i')]);
    assert!(ffi.len() <= 2, "ffi collapses (got {})", ffi.len());
}

#[test]
fn subset_font_has_no_ligatures() {
    // Font::subset drops GSUB, so a subset font has no ligatures.
    let f = load("fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
    assert!(!f.gsub_ligatures().is_empty());
    let sub = Font::parse(f.subset(&['f', 'i', 'l']).unwrap()).unwrap();
    assert!(sub.gsub_ligatures().is_empty(), "subset carries no GSUB");
}
