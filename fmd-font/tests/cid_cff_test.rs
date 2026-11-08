#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::{EmbeddingFormat, Font, MISSING_GLYPH_REMAP, cff::Command};

const FORMAT0: &[u8] = include_bytes!("../fonts/test-cid-cff/FmdCidFormat0.otf");
const FORMAT3: &[u8] = include_bytes!("../fonts/test-cid-cff/FmdCidFormat3.otf");

fn outline_hash(commands: &[Command]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    };
    for command in commands {
        let (op, points): (u8, &[_]) = match command {
            Command::Move(point) => (b'M', std::slice::from_ref(point)),
            Command::Line(point) => (b'L', std::slice::from_ref(point)),
            Command::Curve(a, b, c) => (b'C', &[*a, *b, *c]),
            Command::Close => (b'Z', &[]),
        };
        feed(&[op]);
        for point in points {
            for value in [point.x, point.y] {
                feed(&(if value == 0.0 { 0.0 } else { value }).to_le_bytes());
            }
        }
    }
    hash
}

#[test]
fn cid_font_dicts_and_nested_subrs_match_independent_fonttools() {
    let mut decoded = Vec::new();
    for bytes in [FORMAT0, FORMAT3] {
        let font = Font::parse_static(bytes).unwrap();
        assert_eq!(font.num_glyphs, 4);
        let mut outlines = Vec::new();
        for row in include_str!("../fonts/test-cid-cff/fonttools-reference.txt").lines() {
            let columns: Vec<_> = row.split_whitespace().collect();
            let gid = columns[0].parse().unwrap();
            let outline = font.cff_outline(gid).unwrap();
            assert_eq!(outline.advance, columns[1].parse::<u16>().unwrap());
            assert_eq!(outline.lsb, columns[2].parse::<i16>().unwrap());
            assert_eq!(
                outline_hash(&outline.commands),
                u64::from_str_radix(columns[3], 16).unwrap()
            );
            let bounds = (columns[4] != "none")
                .then(|| std::array::from_fn(|i| columns[4 + i].parse().unwrap()));
            assert_eq!(font.cff_glyph_bbox(gid).unwrap(), bounds);
            outlines.push(outline);
        }
        // Both glyphs call the same global subroutine, which then calls local
        // subroutine zero. The active FD must survive both call levels.
        assert_ne!(outlines[1].commands, outlines[2].commands);
        decoded.push(outlines);
    }
    assert_eq!(decoded[0], decoded[1]);
}

#[test]
fn cid_subsets_preserve_glyph_identity_unicode_and_normalized_outlines() {
    let mut outputs = Vec::new();
    for bytes in [FORMAT0, FORMAT3] {
        let font = Font::parse(bytes.to_vec()).unwrap();
        // CIDs in the fixture are 25, 1000, 7. Unicode cmap and FDSelect both
        // address source GIDs, and the output uses fresh ascending GIDs.
        assert_eq!(font.glyph_index('A'), 1);
        assert_eq!(font.glyph_index('B'), 2);
        assert_eq!(font.glyph_index('日'), 3);
        let subset = font.try_subset(&['日', 'B']).unwrap();
        assert_eq!(subset.format, EmbeddingFormat::OpenTypeCff);
        assert_eq!(subset.glyph_map, [0, MISSING_GLYPH_REMAP, 1, 2]);
        let checksum = subset.bytes.chunks(4).fold(0u32, |sum, chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        });
        assert_eq!(checksum, 0xb1b0afba);
        let compact = Font::parse(subset.bytes.clone()).unwrap();
        assert_eq!(compact.num_glyphs, 3);
        assert_eq!(compact.glyph_index('A'), 0);
        assert_eq!(compact.glyph_index('B'), 1);
        assert_eq!(compact.glyph_index('日'), 2);
        for (new, old) in [0, 2, 3].into_iter().enumerate() {
            assert_eq!(
                font.cff_outline(old).unwrap(),
                compact.cff_outline(new as u16).unwrap()
            );
        }
        assert_eq!(compact.font_bbox(), Some([0, 0, 500, 500]));
        assert_eq!(subset, font.try_subset(&['B', '日', 'B']).unwrap());
        outputs.push(subset.bytes);
    }
    // Source selector compression does not leak into the normalized output.
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn cid_shared_directory_is_safe_for_parallel_outline_consumers() {
    let font = std::sync::Arc::new(Font::parse_static(FORMAT3).unwrap());
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let font = std::sync::Arc::clone(&font);
            scope.spawn(move || {
                for _ in 0..32 {
                    assert_eq!(font.cff_glyph_bbox(1).unwrap(), Some([100, 0, 300, 300]));
                    assert_eq!(font.cff_glyph_bbox(2).unwrap(), Some([100, 0, 500, 500]));
                }
            });
        }
    });
}
