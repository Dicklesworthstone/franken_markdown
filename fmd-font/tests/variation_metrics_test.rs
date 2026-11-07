//! Independent FontTools-generated HVAR/MVAR static-instance expectations.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use fmd_font::Font;

const IMPLICIT: &[u8] = include_bytes!("../fonts/test-variable/FmdMetricsVF.ttf");
const MAPPED: &[u8] = include_bytes!("../fonts/test-variable/FmdMappedMetricsVF.ttf");
const REFERENCE: &str = include_str!("../fonts/test-variable/metric-variations-reference.tsv");

fn fixture(name: &str) -> &[u8] {
    match name {
        "FmdMetricsVF" => IMPLICIT,
        "FmdMappedMetricsVF" => MAPPED,
        _ => panic!("unknown metric fixture: {name}"),
    }
}

fn table_record(data: &[u8], tag: &[u8; 4]) -> usize {
    let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    (0..count)
        .map(|index| 12 + index * 16)
        .find(|&offset| &data[offset..offset + 4] == tag)
        .expect("fixture table")
}

fn table_range(data: &[u8], tag: &[u8; 4]) -> (usize, usize) {
    let record = table_record(data, tag);
    (
        u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize,
        u32::from_be_bytes(data[record + 12..record + 16].try_into().unwrap()) as usize,
    )
}

fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
    let (offset, length) = table_range(data, tag);
    &data[offset..offset + length]
}

fn value(data: &[u8], offset: usize, kind: &str) -> i32 {
    let bytes = data[offset..offset + 2].try_into().unwrap();
    match kind {
        "i16" => i32::from(i16::from_be_bytes(bytes)),
        "u16" => i32::from(u16::from_be_bytes(bytes)),
        _ => panic!("unknown metric type: {kind}"),
    }
}

fn reference_cases() -> BTreeMap<(&'static str, &'static str), Vec<Vec<&'static str>>> {
    let mut result = BTreeMap::new();
    for line in REFERENCE.lines().filter(|line| !line.starts_with('#')) {
        let row: Vec<_> = line.split('\t').collect();
        assert_eq!(row.len(), 6);
        result.entry((row[0], row[1])).or_insert_with(Vec::new).push(row);
    }
    result
}

#[test]
fn fonttools_horizontal_metrics_match_implicit_and_mapped_hvar() {
    for ((name, weight), rows) in reference_cases() {
        let original = Font::parse(fixture(name).to_vec()).unwrap();
        let instance = original.instance(weight.parse().unwrap()).expect("static metric instance");
        for row in rows.into_iter().filter(|row| row[2] == "hmtx") {
            let glyph = row[3].parse().unwrap();
            assert_eq!(
                (instance.advance_width(glyph), instance.left_side_bearing(glyph)),
                (row[4].parse().unwrap(), row[5].parse().unwrap()),
                "{name} wght={weight} glyph={glyph}: FontTools hmtx oracle",
            );
        }
    }
}

#[test]
fn fonttools_global_metrics_and_outline_summaries_match_static_instances() {
    for ((name, weight), rows) in reference_cases() {
        let original = Font::parse(fixture(name).to_vec()).unwrap();
        let instance = original.instance(weight.parse().unwrap()).expect("static metric instance");
        for row in rows.into_iter().filter(|row| row[2] != "hmtx") {
            let tag: &[u8; 4] = row[2].as_bytes().try_into().unwrap();
            let offset = row[3].parse().unwrap();
            assert_eq!(
                value(table(instance.as_sfnt(), tag), offset, row[4]),
                row[5].parse::<i32>().unwrap(),
                "{name} wght={weight} {} offset={offset}: FontTools global metric oracle", row[2],
            );
        }
    }
}

#[test]
fn gasp_range_variations_follow_the_spec_and_preserve_the_terminal_range() {
    // FontTools 4.61.1 does not implement gsp0..gsp9, so these are explicit
    // OpenType MVAR expectations, not falsely attributed FontTools output.
    // https://learn.microsoft.com/en-us/typography/opentype/spec/mvar
    // The two default maxima (8, 16) have negative deltas (-2, -4), positive
    // deltas (2, 4), and no intermediate delta. The avar-adjusted positive
    // locations at 525 and 650 are 0.375 and 0.75; half ties round upward.
    for data in [IMPLICIT, MAPPED] {
        let original = Font::parse(data.to_vec()).unwrap();
        for (weight, expected) in [
            (100.0, [6, 12, 65_535]),
            (250.0, [7, 14, 65_535]),
            (400.0, [8, 16, 65_535]),
            (525.0, [9, 18, 65_535]),
            (650.0, [10, 19, 65_535]),
            (900.0, [10, 20, 65_535]),
        ] {
            let instance = original.instance(weight).unwrap();
            let gasp = table(instance.as_sfnt(), b"gasp");
            assert_eq!(value(gasp, 2, "u16"), 3);
            for (index, maximum) in expected.into_iter().enumerate() {
                assert_eq!(value(gasp, 4 + 4 * index, "u16"), maximum, "gasp range {index}, weight {weight}");
                assert_eq!(value(gasp, 6 + 4 * index, "u16"), [2, 1, 3][index], "range behavior is unchanged");
            }
        }
    }
}

#[test]
fn gvar_phantoms_keep_empty_and_outlined_metrics_correct_without_hvar() {
    for ((name, weight), rows) in reference_cases() {
        let mut bytes = fixture(name).to_vec();
        let record = table_record(&bytes, b"HVAR");
        // HVAR is optional for TrueType variable fonts. Both independently
        // encoded sources carry matching phantom deltas for every glyph.
        bytes[record..record + 4].copy_from_slice(b"HVAx");
        let original = Font::parse(bytes).unwrap();
        let instance = original.instance(weight.parse().unwrap()).unwrap();
        for row in rows.into_iter().filter(|row| row[2] == "hmtx") {
            let glyph = row[3].parse().unwrap();
            assert_eq!(
                (instance.advance_width(glyph), instance.left_side_bearing(glyph)),
                (row[4].parse().unwrap(), row[5].parse().unwrap()),
                "{name} weight {weight}, glyph {glyph}: gvar-only metrics",
            );
        }
        assert!(instance.glyph_bbox(instance.glyph_index(' ')).is_none());
    }
}

#[test]
fn empty_space_advances_vary_without_creating_an_outline() {
    for data in [IMPLICIT, MAPPED] {
        let original = Font::parse(data.to_vec()).unwrap();
        let space = original.glyph_index(' ');
        let heavy = original.instance(900.0).unwrap();
        assert_eq!(original.advance_width(space), 300);
        assert_eq!(heavy.advance_width(space), 390);
        assert!(original.glyph_bbox(space).is_none());
        assert!(heavy.glyph_bbox(space).is_none());
    }
}

#[test]
fn hvar_advances_replace_original_metrics_without_reapplying_gvar_deltas() {
    for data in [IMPLICIT, MAPPED] {
        let original = Font::parse(data.to_vec()).unwrap();
        let glyph = original.glyph_index('A');
        let heavy = original.instance(900.0).unwrap();
        assert_eq!(original.advance_width(glyph), 600);
        assert_eq!(heavy.advance_width(glyph), 720, "120-unit advance delta is applied once");
        assert_eq!(heavy.left_side_bearing(glyph), 74);
    }
}

#[test]
fn mvar_preserves_intentionally_independent_hhea_vertical_metrics() {
    let mut data = MAPPED.to_vec();
    let (hhea, _) = table_range(&data, b"hhea");
    for (offset, number) in [(4, 830i16), (6, -205), (8, 110)] {
        data[hhea + offset..hhea + offset + 2].copy_from_slice(&number.to_be_bytes());
    }
    let instance = Font::parse(data).unwrap().instance(900.0).unwrap();
    assert_eq!((instance.ascent, instance.descent, instance.line_gap), (830, -205, 110));
    assert_eq!(value(table(instance.as_sfnt(), b"OS/2"), 68, "i16"), 900);
    assert_eq!(value(table(instance.as_sfnt(), b"OS/2"), 70, "i16"), -230);
}

#[test]
fn variable_metric_tables_are_consumed_and_instance_bytes_are_deterministic() {
    let original = Font::parse(MAPPED.to_vec()).unwrap();
    let first = original.instance(525.0).unwrap();
    let second = original.instance(525.0).unwrap();
    assert_eq!(first.as_sfnt(), second.as_sfnt());
    assert!(first.axes().is_empty());
    let count = u16::from_be_bytes(first.as_sfnt()[4..6].try_into().unwrap()) as usize;
    for index in 0..count {
        let tag = &first.as_sfnt()[12 + index * 16..16 + index * 16];
        assert!(![b"HVAR".as_slice(), b"MVAR", b"fvar", b"gvar", b"avar"].contains(&tag));
    }
    assert_eq!(original.as_sfnt(), MAPPED, "instancing never mutates the variable source");
}

#[test]
fn malformed_metric_headers_and_indices_do_not_silently_become_static_defaults() {
    let (hvar, _) = table_range(MAPPED, b"HVAR");
    let (mvar, _) = table_range(MAPPED, b"MVAR");
    let store = hvar + u32::from_be_bytes(MAPPED[hvar + 4..hvar + 8].try_into().unwrap()) as usize;
    let regions = store + u32::from_be_bytes(MAPPED[store + 2..store + 6].try_into().unwrap()) as usize;
    let map = hvar + u32::from_be_bytes(MAPPED[hvar + 8..hvar + 12].try_into().unwrap()) as usize;
    let cases: Vec<(&str, usize, Vec<u8>)> = vec![
        ("HVAR version", hvar, 2u16.to_be_bytes().to_vec()),
        ("HVAR store outside table", hvar + 4, u32::MAX.to_be_bytes().to_vec()),
        ("HVAR axis mismatch", regions, 3u16.to_be_bytes().to_vec()),
        ("HVAR empty explicit map", map + 2, 0u16.to_be_bytes().to_vec()),
        ("HVAR reserved map flags", map + 1, vec![0xc0]),
        ("MVAR record too short", mvar + 6, 7u16.to_be_bytes().to_vec()),
        ("MVAR missing required store", mvar + 10, 0u16.to_be_bytes().to_vec()),
        ("MVAR invalid known record index", mvar + 16, 0xfffeu16.to_be_bytes().to_vec()),
    ];
    for (name, offset, bytes) in cases {
        let mut data = MAPPED.to_vec();
        data[offset..offset + bytes.len()].copy_from_slice(&bytes);
        let font = Font::parse(data).expect("other sfnt tables remain valid");
        assert!(font.instance(650.0).is_none(), "{name} must refuse metric instancing");
    }
}

#[test]
fn metrics_never_read_beyond_the_declared_table_extent() {
    for (tag, length) in [(*b"HVAR", 19u32), (*b"MVAR", 11u32)] {
        let mut data = MAPPED.to_vec();
        let record = table_record(&data, &tag);
        data[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
        let font = Font::parse(data).unwrap();
        assert!(font.instance(650.0).is_none(), "{tag:?} must respect its own table length");
    }
    let mut data = MAPPED.to_vec();
    let record = table_record(&data, b"MVAR");
    data[record + 8..record + 12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(Font::parse(data).unwrap().instance(650.0).is_none());
}

#[test]
fn unrecognized_private_mvar_records_do_not_require_a_target() {
    let mut data = MAPPED.to_vec();
    let (mvar, _) = table_range(&data, b"MVAR");
    // Replace the first tag (cpht); uppercase private tags still precede the
    // standard lowercase tags. Their unspecified target/index is ignored.
    data[mvar + 12..mvar + 16].copy_from_slice(b"TEST");
    data[mvar + 16..mvar + 20].copy_from_slice(&0xfffe_fffeu32.to_be_bytes());
    let instance = Font::parse(data).unwrap().instance(900.0).unwrap();
    assert_eq!(value(table(instance.as_sfnt(), b"OS/2"), 88, "i16"), 700);
    assert_eq!(instance.ascent, 900);
}

#[test]
fn instance_metrics_follow_avar_and_keep_the_other_axis_at_its_default() {
    let font = Font::parse(MAPPED.to_vec()).unwrap();
    assert_eq!(font.axes().len(), 2);
    assert_eq!(font.normalized_axis(*b"wght", 650.0), Some(0.75));
    let instance = font.instance(650.0).unwrap();
    assert_eq!(instance.advance_width(font.glyph_index(' ')), 368);
    assert_eq!(instance.ascent, 875);
    assert_eq!(instance.descent, -222, "negative half ties round toward +infinity");
}

#[test]
fn static_subset_preserves_varied_horizontal_and_face_metrics() {
    let original = Font::parse(MAPPED.to_vec()).unwrap();
    for weight in [100.0, 525.0, 650.0, 900.0] {
        let instance = original.instance(weight).unwrap();
        let subset = instance.try_subset(&[' ', 'A', 'B']).unwrap();
        let checksum = subset.bytes.chunks(4).fold(0_u32, |sum, chunk| {
            let mut word = [0; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        });
        assert_eq!(checksum, 0xb1b0_afba);
        let subset = Font::parse(subset.bytes).unwrap();
        assert_eq!(
            (subset.ascent, subset.descent, subset.line_gap),
            (instance.ascent, instance.descent, instance.line_gap),
        );
        for ch in [' ', 'A', 'B'] {
            let expected = instance.glyph_index(ch);
            let actual = subset.glyph_index(ch);
            assert_ne!(actual, 0);
            assert_eq!(subset.advance_width(actual), instance.advance_width(expected));
            assert_eq!(subset.left_side_bearing(actual), instance.left_side_bearing(expected));
            assert_eq!(subset.glyph_bbox(actual), instance.glyph_bbox(expected));
        }
    }
}

#[test]
fn wide_empty_glyph_affects_maximum_advance_but_never_ink_or_bearing_extrema() {
    // Independently verified by saved/reparsed FontTools instances in
    // generate_metric_variations.py. Extreme empty-glyph bearings exercise
    // the hhea rule that signed extrema consider only glyphs with contours.
    // https://learn.microsoft.com/en-us/typography/opentype/spec/hhea
    for bearing in [-1000i16, 2000] {
        let mut data = MAPPED.to_vec();
        let (hmtx, _) = table_range(&data, b"hmtx");
        data[hmtx + 4..hmtx + 6].copy_from_slice(&1000u16.to_be_bytes());
        data[hmtx + 6..hmtx + 8].copy_from_slice(&bearing.to_be_bytes());
        let font = Font::parse(data).unwrap().instance(900.0).unwrap();
        let space = font.glyph_index(' ');
        assert_eq!((font.advance_width(space), font.left_side_bearing(space)), (1090, bearing));
        assert!(font.glyph_bbox(space).is_none());
        let hhea = table(font.as_sfnt(), b"hhea");
        assert_eq!(value(hhea, 10, "u16"), 1090);
        for (offset, expected) in [(12, 0), (14, 100), (16, 524)] {
            assert_eq!(value(hhea, offset, "i16"), expected);
        }
        let head = table(font.as_sfnt(), b"head");
        for (offset, expected) in [(36, 0), (38, 0), (40, 524), (42, 700)] {
            assert_eq!(value(head, offset, "i16"), expected);
        }
    }
}

#[test]
fn signed_and_unsigned_metric_overflow_is_refused_without_wrapping() {
    for (tag, offset, bytes) in [
        (*b"hmtx", 8, 65_535u16.to_be_bytes()),
        (*b"hmtx", 10, 32_767i16.to_be_bytes()),
        (*b"OS/2", 68, 32_767i16.to_be_bytes()),
        (*b"OS/2", 70, (-32_768i16).to_be_bytes()),
        (*b"OS/2", 74, 65_535u16.to_be_bytes()),
        (*b"gasp", 4, 65_535u16.to_be_bytes()),
    ] {
        let mut data = MAPPED.to_vec();
        let (start, _) = table_range(&data, &tag);
        data[start + offset..start + offset + 2].copy_from_slice(&bytes);
        let font = Font::parse(data).unwrap();
        assert!(font.instance(900.0).is_none(), "{tag:?} offset {offset}: metric overflow");
    }
}

#[test]
fn metric_table_truncations_and_deterministic_mutations_remain_bounded() {
    for tag in [*b"HVAR", *b"MVAR"] {
        let record = table_record(MAPPED, &tag);
        let (offset, length) = table_range(MAPPED, &tag);
        let verify = |data: Vec<u8>| {
            // Parsing ignores these optional metric tables. Instancing must
            // either refuse them or produce a deterministic, parseable static
            // face without exposing bytes outside their declared table spans.
            let font = Font::parse(data).unwrap();
            if let Some(instance) = font.instance(650.0) {
                assert!(instance.axes().is_empty());
                assert!(Font::parse(instance.as_sfnt().to_vec()).is_ok());
                assert_eq!(instance.as_sfnt(), font.instance(650.0).unwrap().as_sfnt());
            }
        };
        for prefix in 0..length {
            let mut data = MAPPED.to_vec();
            data[record + 12..record + 16].copy_from_slice(&(prefix as u32).to_be_bytes());
            verify(data);
        }
        let mut seed = 0x8b2a_501du32;
        for _ in 0..512 {
            let mut data = MAPPED.to_vec();
            for _ in 0..3 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let position = offset + seed as usize % length;
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                data[position] ^= ((seed >> 24) as u8).max(1);
            }
            verify(data);
        }
    }
}
