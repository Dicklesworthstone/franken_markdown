//! Real OpenType bytes and independent FontTools static-instance coordinates.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::Font;

fn fixture() -> Font {
    Font::parse(include_bytes!("../fonts/test-variable/FmdCompositeVariationsVF.ttf").to_vec())
        .expect("FontTools composite variation fixture")
}

fn points(font: &Font, ch: char) -> Vec<(f64, f64)> {
    font.glyph_outline(font.glyph_index(ch))
        .expect("valid composite outline")
        .contours
        .into_iter()
        .flat_map(|contour| {
            std::iter::once(contour.start)
                .chain(
                    contour
                        .segments
                        .into_iter()
                        .take(2)
                        .map(|segment| segment.to()),
                )
                .map(|point| (point.x, point.y))
        })
        .collect()
}

#[test]
fn component_deltas_precede_phantoms_and_keep_independent_horizontal_metrics() {
    // Coordinates/metrics come from FontTools' saved and reparsed static
    // instances, recorded by generate_composite_variations.py and its JSON.
    let font = fixture();
    for (weight, expected, bbox, metrics) in [
        (
            400.0,
            [
                (10.0, 20.0),
                (110.0, 20.0),
                (60.0, 120.0),
                (100.0, 100.0),
                (140.0, 100.0),
                (120.0, 140.0),
            ],
            [10, 20, 140, 140],
            (500, 10),
        ),
        (
            650.0,
            [
                (35.0, 25.0),
                (140.0, 25.0),
                (88.0, 140.0),
                (130.0, 140.0),
                (180.0, 140.0),
                (155.0, 190.0),
            ],
            [35, 25, 180, 190],
            (560, 35),
        ),
        (
            900.0,
            [
                (60.0, 30.0),
                (170.0, 30.0),
                (115.0, 160.0),
                (160.0, 180.0),
                (220.0, 180.0),
                (190.0, 240.0),
            ],
            [60, 30, 220, 240],
            (620, 60),
        ),
    ] {
        let instance = font.instance(weight).expect("composite instance");
        let gid = instance.glyph_index('Á');
        assert_eq!(points(&instance, 'Á'), expected, "outline at {weight}");
        assert_eq!(instance.glyph_bbox(gid), Some(bbox), "bbox at {weight}");
        assert_eq!(
            instance.advance_width(gid),
            metrics.0,
            "advance at {weight}"
        );
        assert_eq!(
            instance.left_side_bearing(gid),
            metrics.1,
            "LSB at {weight}"
        );
    }
}

#[test]
fn unchanged_parent_recomputes_bounds_and_bearings_from_instanced_descendants() {
    let font = fixture();
    // The parent has no gvar tuple, references a later glyph, and scales it.
    // Updating only composites with their own tuple misses its changed ink.
    for (weight, expected, bbox, lsb) in [
        (
            400.0,
            [
                (-5.0, 60.0),
                (145.0, 60.0),
                (70.0, 210.0),
                (130.0, 180.0),
                (190.0, 180.0),
                (160.0, 240.0),
            ],
            [-5, 60, 190, 240],
            -5,
        ),
        (
            650.0,
            [
                (32.5, 67.5),
                (190.0, 67.5),
                (112.0, 240.0),
                (175.0, 240.0),
                (250.0, 240.0),
                (212.5, 315.0),
            ],
            [33, 68, 250, 315],
            33,
        ),
        (
            900.0,
            [
                (70.0, 75.0),
                (235.0, 75.0),
                (152.5, 270.0),
                (220.0, 300.0),
                (310.0, 300.0),
                (265.0, 390.0),
            ],
            [70, 75, 310, 390],
            70,
        ),
    ] {
        let instance = font.instance(weight).expect("nested instance");
        let gid = instance.glyph_index('Ǻ');
        assert_eq!(
            points(&instance, 'Ǻ'),
            expected,
            "nested outline at {weight}"
        );
        assert_eq!(
            instance.glyph_bbox(gid),
            Some(bbox),
            "nested bbox at {weight}"
        );
        assert_eq!(
            instance.advance_width(gid),
            700,
            "parent advance at {weight}"
        );
        assert_eq!(
            instance.left_side_bearing(gid),
            lsb,
            "parent LSB at {weight}"
        );
    }
}

#[test]
fn component_offsets_can_outgrow_original_byte_encoding() {
    let font = fixture();
    let gid = font.glyph_index('Á');
    let original = font.glyph_data(gid).unwrap();
    let first_flags = u16::from_be_bytes(original[10..12].try_into().unwrap());
    assert_eq!(
        first_flags & 1,
        0,
        "FontTools encoded the first component in bytes"
    );
    let second_offset = 16;
    let second_flags = u16::from_be_bytes(
        original[second_offset..second_offset + 2]
            .try_into()
            .unwrap(),
    );
    assert_eq!(second_flags & 1, 0, "FontTools encoded the accent in bytes");
    let instance = font.instance(900.0).expect("byte offsets promote to words");
    assert_eq!(
        &points(&instance, 'Á')[3..],
        &[(160.0, 180.0), (220.0, 180.0), (190.0, 240.0)]
    );
    assert_eq!(
        font.as_sfnt(),
        fixture().as_sfnt(),
        "source font remains immutable"
    );
    assert_eq!(instance.as_sfnt(), font.instance(900.0).unwrap().as_sfnt());
}

#[test]
fn point_attached_accents_follow_the_instanced_parent_anchor() {
    let font = fixture();
    for (weight, expected, bbox, metrics) in [
        (
            650.0,
            [
                (15.0, 10.0),
                (120.0, 10.0),
                (68.0, 125.0),
                (68.0, 125.0),
                (118.0, 125.0),
                (93.0, 175.0),
            ],
            [15, 10, 120, 175],
            (520, 15),
        ),
        (
            900.0,
            [
                (30.0, 20.0),
                (140.0, 20.0),
                (85.0, 150.0),
                (85.0, 150.0),
                (145.0, 150.0),
                (115.0, 210.0),
            ],
            [30, 20, 145, 210],
            (540, 30),
        ),
    ] {
        let instance = font.instance(weight).expect("point attached instance");
        let gid = instance.glyph_index('Å');
        assert_eq!(points(&instance, 'Å'), expected);
        assert_eq!(instance.glyph_bbox(gid), Some(bbox));
        assert_eq!(instance.advance_width(gid), metrics.0);
        assert_eq!(instance.left_side_bearing(gid), metrics.1);
    }
}

#[test]
fn static_subsets_retain_instanced_composites_and_valid_checksums() {
    let font = fixture().instance(900.0).unwrap();
    let subset = font
        .try_subset(&['Á', 'Ǻ', 'Å'])
        .expect("strict static subset");
    let encoded = subset.bytes;
    let checksum = encoded.chunks(4).fold(0_u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    });
    assert_eq!(checksum, 0xb1b0_afba);
    let subset = Font::parse(encoded).unwrap();
    assert!(subset.axes().is_empty());
    for ch in ['Á', 'Ǻ', 'Å'] {
        let source = font.glyph_index(ch);
        let selected = subset.glyph_index(ch);
        assert_ne!(selected, 0);
        assert_eq!(points(&subset, ch), points(&font, ch));
        assert_eq!(subset.glyph_bbox(selected), font.glyph_bbox(source));
        assert_eq!(subset.advance_width(selected), font.advance_width(source));
        assert_eq!(
            subset.left_side_bearing(selected),
            font.left_side_bearing(source)
        );
    }
}

fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
    let count = usize::from(u16::from_be_bytes(bytes[4..6].try_into().unwrap()));
    (0..count)
        .find_map(|index| {
            let start = 12 + index * 16;
            (&bytes[start..start + 4] == tag).then(|| {
                u32::from_be_bytes(bytes[start + 8..start + 12].try_into().unwrap()) as usize
            })
        })
        .expect("fixture table")
}

fn glyph_offset(bytes: &[u8], gid: u16) -> usize {
    let head = table_offset(bytes, b"head");
    let loca = table_offset(bytes, b"loca");
    let glyf = table_offset(bytes, b"glyf");
    let relative = if bytes[head + 50..head + 52] == [0, 0] {
        let at = loca + usize::from(gid) * 2;
        usize::from(u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap())) * 2
    } else {
        let at = loca + usize::from(gid) * 4;
        u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    };
    glyf + relative
}

#[test]
fn cyclic_composites_refuse_instancing_with_bounded_work() {
    let source = fixture();
    let gid = source.glyph_index('Á');
    let mut bytes = source.as_sfnt().to_vec();
    let start = glyph_offset(&bytes, gid);
    bytes[start + 12..start + 14].copy_from_slice(&gid.to_be_bytes());
    let cycle = Font::parse(bytes).unwrap();
    assert!(cycle.instance(900.0).is_none());
}

#[test]
fn last_use_my_metrics_component_supplies_outline_metrics() {
    // OpenType gvar's composite processing applies USE_MY_METRICS in record
    // order. The last flagged child wins, independently of the parent's hmtx.
    let source = fixture();
    let gid = source.glyph_index('Á');
    let mut bytes = source.as_sfnt().to_vec();
    let start = glyph_offset(&bytes, gid);
    for component in [10, 16] {
        let at = start + component;
        let flags = u16::from_be_bytes(bytes[at..at + 2].try_into().unwrap()) | 0x0200;
        bytes[at..at + 2].copy_from_slice(&flags.to_be_bytes());
    }
    let instance = Font::parse(bytes).unwrap().instance(900.0).unwrap();
    let outline = instance.glyph_outline(gid).unwrap();
    assert_eq!(outline.advance, 120, "accent supplies the final metrics");
    assert_eq!(outline.lsb, 0);
    assert_eq!(
        instance.advance_width(gid),
        620,
        "unhinted parent hmtx stays independent"
    );
}
