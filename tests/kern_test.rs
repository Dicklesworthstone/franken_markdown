//! GPOS pair-kerning tests against the bundled fonts (which kern via GPOS, not
//! a legacy `kern` table).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::text::Font;

fn load(path: &str) -> Font {
    Font::parse(std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn plex_and_cm_kern_common_pairs_via_gpos() {
    // Pairs essentially every Latin text face tightens (negative x-advance).
    let pairs = [
        ('A', 'V'),
        ('A', 'W'),
        ('A', 'Y'),
        ('V', 'A'),
        ('W', 'A'),
        ('Y', 'A'),
        ('T', 'o'),
        ('T', 'a'),
        ('T', 'e'),
        ('W', 'a'),
        ('Y', 'o'),
        ('P', '.'),
        ('F', '.'),
        ('V', 'o'),
    ];
    for path in [
        "fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf",
        "fmd-font/fonts/computer-modern/cmunrm.ttf",
    ] {
        let font = load(path);
        let kern = font.gpos_kerning();
        let mut negative = 0;
        for (l, r) in pairs {
            if kern.pair(font.glyph_index(l), font.glyph_index(r)) < 0 {
                negative += 1;
            }
        }
        assert!(
            negative >= 3,
            "{path}: expected several negative GPOS kern pairs, found {negative}"
        );
    }
}

#[test]
fn subset_font_has_no_gpos_kerning() {
    // Font::subset emits no GPOS, so a subset font kerns nothing — confirms both
    // the empty-Kerning path and that subsetting drops layout tables.
    let font = load("fmd-font/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
    // The full face kerns A/V; the subset must not (no GPOS in the subset).
    assert!(
        font.gpos_kerning()
            .pair(font.glyph_index('A'), font.glyph_index('V'))
            < 0
    );

    let sub = font.subset(&['A', 'V', 'o', 'T']).unwrap();
    let subfont = Font::parse(sub).unwrap();
    let k = subfont.gpos_kerning();
    assert_eq!(
        k.pair(subfont.glyph_index('A'), subfont.glyph_index('V')),
        0,
        "subset font carries no GPOS kerning"
    );
}
