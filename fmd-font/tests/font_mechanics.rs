#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use fmd_font::{
    EmbeddingFormat, Font, MISSING_GLYPH_REMAP, SubsetErrorKind,
    cff::{CffOutline, Command},
    shaping::{Direction, Feature, ShapeErrorKind, ShapeOptions},
};
const CFF: &[u8] = include_bytes!("../fonts/test-cff/Bravura.otf");
const SHAPING: &[u8] = include_bytes!("../fonts/test-shaping/FmdShaping.ttf");
fn hash(o: &CffOutline) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut feed = |v: &[u8]| {
        for &b in v {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
    };
    for command in &o.commands {
        let (op, points) = match command {
            Command::Move(p) => (b'M', vec![p]),
            Command::Line(p) => (b'L', vec![p]),
            Command::Curve(a, b, c) => (b'C', vec![a, b, c]),
            Command::Close => (b'Z', vec![]),
        };
        feed(&[op]);
        for p in points {
            for v in [p.x, p.y] {
                feed(&(if v == 0.0 { 0.0 } else { v }).to_le_bytes());
            }
        }
    }
    hash
}
#[test]
fn every_bravura_glyph_matches_independent_fonttools() {
    let f = Font::parse(CFF.to_vec()).unwrap();
    let reference = include_str!("../fonts/test-cff/fonttools-reference.txt");
    assert_eq!(reference.lines().count(), usize::from(f.num_glyphs));
    for line in reference.lines() {
        let p: Vec<_> = line.split_whitespace().collect();
        let gid = p[0].parse().unwrap();
        let o = f.cff_outline(gid).unwrap();
        assert_eq!(o.advance, p[1].parse::<u16>().unwrap(), "glyph {gid}");
        assert_eq!(o.lsb, p[2].parse::<i16>().unwrap());
        assert_eq!(
            hash(&o),
            u64::from_str_radix(p[3], 16).unwrap(),
            "glyph {gid}"
        );
    }
}
#[test]
fn sparse_cff_is_deterministic_and_explicitly_typed() {
    let f = Font::parse(CFF.to_vec()).unwrap();
    let keep = [3000, 1, 5, 73, 100, 500, 1000, 2000, 73];
    let a = f.try_subset_glyphs(&keep, &['\u{e050}']).unwrap();
    let b = f
        .try_subset_glyphs(&[1, 5, 73, 100, 500, 1000, 2000, 3000], &['\u{e050}'])
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.format, EmbeddingFormat::OpenTypeCff);
    assert_eq!(&a.bytes[..4], b"OTTO");
    assert!(a.bytes.len() < CFF.len() / 10);
    let s = Font::parse(a.bytes).unwrap();
    assert_eq!(s.num_glyphs, 9);
    for old in 0..f.num_glyphs {
        let new = a.glyph_map[usize::from(old)];
        if new != MISSING_GLYPH_REMAP {
            assert_eq!(f.cff_outline(old).unwrap(), s.cff_outline(new).unwrap());
        }
    }
}
#[test]
fn shaping_matches_independent_harfbuzz_with_logical_recovery() {
    let f = Font::parse(SHAPING.to_vec()).unwrap();
    for line in include_str!("../fonts/test-shaping/harfbuzz-reference.tsv").lines() {
        let columns: Vec<_> = line.split('\t').collect();
        let options = ShapeOptions {
            script: columns[0].as_bytes().try_into().unwrap(),
            direction: if columns[0] == "arab" {
                Direction::RightToLeft
            } else {
                Direction::LeftToRight
            },
            ..ShapeOptions::default()
        };
        let run = f.shape(columns[1], &options).unwrap();
        assert_eq!(run.logical_text(), columns[1]);
        assert_eq!(run, f.shape(columns[1], &options).unwrap());
        let actual: Vec<_> = run
            .glyphs
            .iter()
            .map(|g| {
                vec![
                    i32::from(g.glyph_id),
                    g.cluster.start as i32,
                    g.cluster.end as i32,
                    g.x_advance,
                    g.y_advance,
                    g.x_offset,
                    g.y_offset,
                ]
            })
            .collect();
        let expected: Vec<Vec<i32>> = columns[2]
            .split(';')
            .map(|g| g.split(',').map(|n| n.parse().unwrap()).collect())
            .collect();
        assert_eq!(actual, expected, "{}", columns[1]);
        for g in &run.glyphs {
            assert!(columns[1].is_char_boundary(g.cluster.start));
            assert!(columns[1].is_char_boundary(g.cluster.end));
            assert!(g.cluster.start < g.cluster.end);
        }
    }
}
#[test]
fn shaping_errors_are_distinct_and_features_are_explicit() {
    let f = Font::parse(SHAPING.to_vec()).unwrap();
    let opts = ShapeOptions::default();
    assert_eq!(
        f.shape("z", &opts).unwrap_err().kind,
        ShapeErrorKind::MissingGlyph
    );
    assert_eq!(
        f.shape("あ", &opts).unwrap_err().kind,
        ShapeErrorKind::UnsupportedScript
    );
    assert_eq!(
        f.shape(
            "a",
            &ShapeOptions {
                script: *b"deva",
                ..opts.clone()
            }
        )
        .unwrap_err()
        .kind,
        ShapeErrorKind::UnsupportedScript
    );
    assert_eq!(
        f.shape(
            "a",
            &ShapeOptions {
                language: *b"ZZZ ",
                ..opts.clone()
            }
        )
        .unwrap_err()
        .kind,
        ShapeErrorKind::UnsupportedLanguage
    );
    assert_eq!(
        f.shape(
            "a",
            &ShapeOptions {
                features: &[Feature {
                    tag: *b"zzzz",
                    enabled: true
                }],
                ..opts.clone()
            }
        )
        .unwrap_err()
        .kind,
        ShapeErrorKind::UnsupportedFeature
    );
    assert_eq!(
        f.shape(
            "a\u{301}",
            &ShapeOptions {
                features: &[Feature {
                    tag: *b"mark",
                    enabled: false
                }],
                ..opts.clone()
            }
        )
        .unwrap_err()
        .kind,
        ShapeErrorKind::UnpositionedMark
    );
    assert_eq!(
        f.shape(
            "fi",
            &ShapeOptions {
                features: &[Feature {
                    tag: *b"liga",
                    enabled: false
                }],
                ..opts.clone()
            }
        )
        .unwrap()
        .glyphs
        .len(),
        2
    );
    assert_eq!(
        f.shape(&"a".repeat(4097), &opts).unwrap_err().kind,
        ShapeErrorKind::BudgetExceeded
    );
    let mut data = SHAPING.to_vec();
    let (gsub, _, _) = table(&data, b"GSUB");
    data[gsub + 4..gsub + 6].copy_from_slice(&65535u16.to_be_bytes());
    assert_eq!(
        Font::parse(data)
            .unwrap()
            .shape("a", &opts)
            .unwrap_err()
            .kind,
        ShapeErrorKind::MalformedFont
    );
}
fn table(data: &[u8], tag: &[u8; 4]) -> (usize, usize, usize) {
    let n = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    for i in 0..n {
        let p = 12 + i * 16;
        if &data[p..p + 4] == tag {
            return (
                u32::from_be_bytes(data[p + 8..p + 12].try_into().unwrap()) as usize,
                u32::from_be_bytes(data[p + 12..p + 16].try_into().unwrap()) as usize,
                p,
            );
        }
    }
    panic!("missing table")
}
#[test]
fn strict_subset_errors_and_truetype_success_keep_legacy_bytes() {
    let f = Font::parse(SHAPING.to_vec()).unwrap();
    let keep = [f.glyph_index('a'), f.glyph_index('b')];
    let strict = f.try_subset_glyphs(&keep, &['a', 'b']).unwrap();
    let old = f.subset_glyphs_with_lookup(&keep, &['a', 'b']).unwrap();
    assert_eq!(strict.bytes, old.0);
    assert_eq!(strict.glyph_map, old.1);
    assert_eq!(strict.format, EmbeddingFormat::TrueType);
    assert_eq!(
        f.try_subset_glyphs(&[f.num_glyphs], &[]).unwrap_err().kind,
        SubsetErrorKind::InvalidGlyph
    );
    let mut unsupported = SHAPING.to_vec();
    for tag in [b"glyf", b"loca"] {
        let (_, _, p) = table(&unsupported, tag);
        unsupported[p..p + 4].copy_from_slice(b"none");
    }
    assert_eq!(
        Font::parse(unsupported)
            .unwrap()
            .try_subset_glyphs(&[], &[])
            .unwrap_err()
            .kind,
        SubsetErrorKind::UnsupportedFormat
    );
    let mut bad = SHAPING.to_vec();
    let (glyf, _, _) = table(&bad, b"glyf");
    bad[glyf..glyf + 2].copy_from_slice(&(-1i16).to_be_bytes());
    bad[glyf + 10..glyf + 12].copy_from_slice(&0x23u16.to_be_bytes());
    bad[glyf + 12..glyf + 14].copy_from_slice(&u16::MAX.to_be_bytes());
    let err = Font::parse(bad)
        .unwrap()
        .try_subset_glyphs(&[0], &[])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        SubsetErrorKind::Malformed | SubsetErrorKind::InvalidGlyph
    ));
    assert_eq!(err.glyph, Some(0));
    assert_eq!(err.table, *b"glyf");
    let mut truncated = SHAPING.to_vec();
    let (_, _, p) = table(&truncated, b"loca");
    truncated[p + 12..p + 16].copy_from_slice(&2u32.to_be_bytes());
    assert_eq!(
        Font::parse(truncated)
            .unwrap()
            .try_subset_glyphs(&[0], &[])
            .unwrap_err()
            .table,
        *b"loca"
    );
}
#[test]
fn hostile_font_mutations_are_bounded_and_do_not_panic() {
    let mut seed = 1u64;
    for original in [SHAPING, CFF] {
        for _ in 0..128 {
            let mut bytes = original.to_vec();
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let p = (seed as usize) % bytes.len();
            bytes[p] ^= (seed >> 32) as u8;
            if let Ok(font) = Font::parse(bytes) {
                let _ = font.try_subset_glyphs(&[0, 1], &['a']);
                let _ = font.cff_outline(0);
                let _ = font.shape("a", &ShapeOptions::default());
            }
        }
    }
}
