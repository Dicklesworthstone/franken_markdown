//! Independent HarfBuzz metrics for the bundled document faces.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::Font;

fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

#[test]
fn bundled_kerning_matches_independent_harfbuzz_for_every_printable_ascii_pair() {
    // HarfBuzz 10.2.0, at each face's native unitsPerEm, with kern=1,
    // liga=0 and clig=0. Each of 9,025 pairs retains its two cmap glyphs;
    // the oracle subtracts hmtx from the first shaped glyph's xAdvance.
    // These hashes cover the complete signed i16 LE matrices, including zeros.
    // Source-font hashes keep an unrelated font replacement from silently
    // reusing an oracle for different input bytes.
    let reference = include_str!("../fonts/test-shaping/harfbuzz-kerning-reference.tsv");
    let mut checked = 0;
    for row in reference.lines().filter(|row| !row.starts_with('#')) {
        let columns: Vec<_> = row.split('\t').collect();
        assert_eq!(columns.len(), 4, "{row}");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fonts")
            .join(columns[0]);
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(
            fnv1a(bytes.iter().copied()),
            u64::from_str_radix(columns[1], 16).unwrap(),
            "oracle font changed: {}",
            columns[0]
        );
        let font = Font::parse(bytes).unwrap();
        let kerning = font.gpos_kerning();
        let mut enumerated = vec![0i16; 128 * 128];
        let mut emitted = vec![false; 128 * 128];
        kerning.for_each_ascii_pair(
            |byte| font.glyph_index(char::from(byte)),
            |left, right, adjustment| {
                assert_ne!(adjustment, 0);
                let index = usize::from(left) * 128 + usize::from(right);
                assert!(!emitted[index], "duplicate ASCII cache cell");
                emitted[index] = true;
                enumerated[index] = adjustment;
            },
        );
        let mut values = Vec::with_capacity(95 * 95 * 2);
        let mut nonzero = 0;
        for left in 32..=126u8 {
            for right in 32..=126u8 {
                let adjustment = kerning.pair(
                    font.glyph_index(char::from(left)),
                    font.glyph_index(char::from(right)),
                );
                assert_eq!(
                    enumerated[usize::from(left) * 128 + usize::from(right)],
                    adjustment,
                    "ASCII cache mismatch: {} ({left}, {right})",
                    columns[0]
                );
                values.extend_from_slice(&adjustment.to_le_bytes());
                nonzero += usize::from(adjustment != 0);
            }
        }
        assert_eq!(values.len(), 9_025 * 2);
        assert_eq!(
            fnv1a(values),
            u64::from_str_radix(columns[2], 16).unwrap(),
            "HarfBuzz kerning mismatch: {}",
            columns[0]
        );
        assert_eq!(nonzero, columns[3].parse::<usize>().unwrap());
        checked += 1;
    }
    assert_eq!(checked, 8, "all four Sans and Serif styles need an oracle");
}

#[test]
fn later_plex_lookups_restore_punctuation_and_letter_spacing() {
    let font = Font::parse_static(include_bytes!(
        "../fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"
    ))
    .unwrap();
    let kerning = font.gpos_kerning();
    for (left, right, expected) in [('"', ',', -90), ('"', 'A', -50), ('"', 'C', -10)] {
        assert_eq!(
            kerning.pair(font.glyph_index(left), font.glyph_index(right)),
            expected,
            "independent HarfBuzz value for {left:?}/{right:?}"
        );
    }
}
