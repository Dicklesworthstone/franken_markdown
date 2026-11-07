//! Original gvar fixture compared with independently saved FontTools instances.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::Font;

fn fixture() -> Font {
    Font::parse(include_bytes!("../fonts/test-variable/FmdSparseTupleVariationsVF.ttf").to_vec())
        .expect("FontTools sparse-tuple fixture")
}

fn points(font: &Font, ch: char) -> Vec<(i32, i32)> {
    font.glyph_outline(font.glyph_index(ch))
        .expect("fixture outline")
        .contours
        .into_iter()
        .flat_map(|contour| {
            std::iter::once(contour.start)
                .chain(
                    contour
                        .segments
                        .into_iter()
                        .take(3)
                        .map(|segment| segment.to()),
                )
                .map(|point| (point.x as i32, point.y as i32))
        })
        .collect()
}

fn assert_reference(ch: char) {
    let font = fixture();
    let source = font.as_sfnt().to_vec();
    let mut checked_weights = Vec::new();
    for line in include_str!("../fonts/test-variable/FmdSparseTupleVariationsVF.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[1].chars().next().unwrap() != ch {
            continue;
        }
        let weight: f32 = fields[0].parse().unwrap();
        checked_weights.push(fields[0].parse::<u16>().unwrap());
        let expected: Vec<(i32, i32)> = fields[2]
            .split(';')
            .map(|point| {
                let (x, y) = point.split_once(',').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        let bounds: Vec<i16> = fields[3]
            .split(',')
            .map(|value| value.parse().unwrap())
            .collect();
        let instance = font.instance(weight).expect("complete static instance");
        let gid = instance.glyph_index(ch);
        assert_eq!(points(&instance, ch), expected, "{ch} points at {weight}");
        assert_eq!(
            instance.glyph_bbox(gid).unwrap().as_slice(),
            bounds,
            "{ch} bounds at {weight}"
        );
        assert_eq!(
            instance.advance_width(gid),
            fields[4].parse::<u16>().unwrap(),
            "{ch} advance at {weight}"
        );
        assert_eq!(
            instance.left_side_bearing(gid),
            fields[5].parse::<i16>().unwrap(),
            "{ch} side bearing at {weight}"
        );
        assert_eq!(
            instance.as_sfnt(),
            font.instance(weight).unwrap().as_sfnt(),
            "deterministic {ch} at {weight}"
        );
        let reparsed = Font::parse(instance.as_sfnt().to_vec()).expect("static sfnt reparses");
        assert_eq!(points(&reparsed, ch), expected);
    }
    assert_eq!(checked_weights, [400, 525, 650, 775, 900]);
    assert_eq!(
        font.as_sfnt(),
        source,
        "caller-owned variable font is unchanged"
    );
}

#[test]
fn sparse_regions_infer_missing_deltas_before_combining() {
    assert_reference('S');
}

#[test]
fn reversed_regions_produce_the_same_independent_outline() {
    assert_reference('R');
}

#[test]
fn dense_regions_do_not_mask_sparse_region_inference() {
    assert_reference('M');
}

#[test]
fn inference_does_not_cross_contours_or_modify_unreferenced_phantoms() {
    assert_reference('C');
}

#[test]
fn negative_half_deltas_round_like_saved_opentype_instances() {
    assert_reference('N');
}
