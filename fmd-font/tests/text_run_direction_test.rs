//! Exercise the real Font::shape -> OwnedTextRun bridge against saved HarfBuzz
//! data, not a second constructor used as an expected-value oracle.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::Font;
use fmd_font::shaping::{Direction, ShapeOptions, ShapedRun};
use fmd_font::text_run::{
    CaretAffinity, FontId, FontOrigin, OwnedTextRun, TextRunContext,
};

const FONT: &[u8] = include_bytes!("../fonts/test-shaping/FmdShaping.ttf");
const REFERENCE: &str = include_str!("../fonts/test-shaping/harfbuzz-reference.tsv");

fn context(direction: Direction, size: f32) -> TextRunContext {
    TextRunContext {
        font_id: FontId::from_font_data(FONT),
        font_size: size,
        script: if direction == Direction::RightToLeft { *b"arab" } else { *b"latn" },
        language: *b"dflt",
        direction,
        font_origin: FontOrigin::BundledFace,
    }
}

fn shape(text: &str, direction: Direction) -> ShapedRun {
    let ctx = context(direction, 10.0);
    Font::parse(FONT.to_vec()).unwrap().shape(text, &ShapeOptions {
        script: ctx.script, language: ctx.language, direction, features: &[],
    }).unwrap()
}

fn run(text: &str, direction: Direction) -> OwnedTextRun {
    OwnedTextRun::from_shaped_run(context(direction, 10.0), &shape(text, direction), 0.01).unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

#[test]
fn rtl_keeps_asymmetric_visual_advances_and_reindexes_logical_clusters() {
    let run = run("بب", Direction::RightToLeft);
    assert_eq!(run.logical_text, "بب");
    assert_eq!(run.glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>(), [14, 12]);
    assert_eq!(run.clusters.len(), 2);
    assert_eq!(run.clusters[0].byte_range, 0..2);
    assert_eq!(run.clusters[0].utf16_range, 0..1);
    assert_eq!(run.clusters[0].glyph_range, 1..2);
    close(run.clusters[0].x_start, 5.0);
    close(run.clusters[0].x_end, 9.6);
    assert_eq!(run.clusters[1].byte_range, 2..4);
    assert_eq!(run.clusters[1].utf16_range, 1..2);
    assert_eq!(run.clusters[1].glyph_range, 0..1);
    close(run.clusters[1].x_start, 0.0);
    close(run.clusters[1].x_end, 5.0);
    assert_eq!(run.glyphs[0].cluster_index, 1);
    assert_eq!(run.glyphs[1].cluster_index, 0);
    close(run.total_advance, 9.6);

    for x in [-1.0, 0.0] {
        let hit = run.hit_test(x);
        assert_eq!((hit.caret.byte_offset, hit.caret.utf16_offset), (4, 2));
        assert_eq!(hit.cluster_index, 1);
        assert_eq!(hit.caret.affinity, CaretAffinity::Trailing);
        close(hit.caret.visual_x, 0.0);
    }
    for x in [run.total_advance, run.total_advance + 1.0] {
        let hit = run.hit_test(x);
        assert_eq!((hit.caret.byte_offset, hit.caret.utf16_offset), (0, 0));
        assert_eq!(hit.cluster_index, 0);
        assert_eq!(hit.caret.affinity, CaretAffinity::Leading);
        close(hit.caret.visual_x, run.total_advance);
    }
    let start = run.caret_at_byte(0, CaretAffinity::Leading).unwrap();
    close(start.visual_x, 9.6);
    let end = run.caret_at_utf16(2, CaretAffinity::Trailing).unwrap();
    assert_eq!(end.byte_offset, 4);
    close(end.visual_x, 0.0);
}

#[test]
fn rtl_marks_keep_shared_clusters_visual_offsets_and_glyph_membership() {
    let run = run("بَب", Direction::RightToLeft);
    assert_eq!(run.clusters[0].byte_range, 0..4);
    assert_eq!(run.clusters[0].utf16_range, 0..2);
    assert_eq!(run.clusters[0].glyph_range, 1..3);
    assert_eq!(run.clusters[1].byte_range, 4..6);
    close(run.clusters[0].x_start, 5.0);
    close(run.clusters[0].x_end, 10.0);
    let mark = &run.glyphs[1];
    assert_eq!(mark.glyph_id, 21);
    assert_eq!(mark.cluster_index, 0);
    close(mark.x_advance, 0.0);
    close(mark.x_offset, 1.5);
    close(mark.y_offset, 5.0);
    assert_eq!(run.glyphs[2].cluster_index, 0);
    let selected = run.selection_rects(0..4, 0.0, 12.0);
    assert_eq!(selected.len(), 1);
    close(selected[0].x, 5.0);
    close(selected[0].width, 5.0);
}

#[test]
fn all_saved_harfbuzz_cases_keep_visual_positions_and_complete_logical_coverage() {
    let mut cases = 0;
    for line in REFERENCE.lines().filter(|line| !line.is_empty()) {
        let columns: Vec<_> = line.split('\t').collect();
        let direction = if columns[0] == "arab" { Direction::RightToLeft } else { Direction::LeftToRight };
        let text = columns[1];
        let expected: Vec<Vec<i32>> = columns[2].split(';')
            .map(|g| g.split(',').map(|v| v.parse().unwrap()).collect()).collect();
        let shaped = shape(text, direction);
        let before = shaped.clone();
        for size in [10.0, 15.625, 32.0] {
            let scale = size / 1000.0;
            let run = OwnedTextRun::from_shaped_run(context(direction, size), &shaped, scale).unwrap();
            assert_eq!(run.glyphs.len(), expected.len());
            assert_eq!(run.logical_text, text);
            let mut positions = vec![0.0_f32];
            for (index, actual) in run.glyphs.iter().enumerate() {
                let e = &expected[index];
                assert_eq!(actual.glyph_id, e[0] as u16);
                close(actual.x_advance, e[3] as f32 * scale);
                close(actual.y_advance, e[4] as f32 * scale);
                close(actual.x_offset, e[5] as f32 * scale);
                close(actual.y_offset, e[6] as f32 * scale);
                positions.push(positions[index] + e[3] as f32 * scale);
                let parent = &run.clusters[actual.cluster_index];
                assert_eq!(parent.byte_range, e[1] as usize..e[2] as usize);
                assert!(parent.glyph_range.contains(&index));
            }
            let mut byte = 0;
            let mut utf16 = 0;
            for (index, cluster) in run.clusters.iter().enumerate() {
                assert_eq!(cluster.cluster_index, index);
                assert_eq!(cluster.byte_range.start, byte);
                assert_eq!(cluster.utf16_range.start, utf16);
                byte = cluster.byte_range.end;
                utf16 += text[cluster.byte_range.clone()].encode_utf16().count();
                assert_eq!(cluster.utf16_range.end, utf16);
                close(cluster.x_start, positions[cluster.glyph_range.start]);
                close(cluster.x_end, positions[cluster.glyph_range.end]);
                assert_eq!(cluster.font_id, run.context.font_id);
            }
            assert_eq!(byte, text.len());
            assert_eq!(utf16, text.encode_utf16().count());
            close(run.total_advance, *positions.last().unwrap());
        }
        assert_eq!(shaped, before, "conversion must never mutate the shaper's output");
        cases += 1;
    }
    assert_eq!(cases, 14);
}

#[test]
fn context_direction_must_match_the_actual_shaping_direction() {
    for (text, actual, wrong) in [
        ("ab", Direction::LeftToRight, Direction::RightToLeft),
        ("بب", Direction::RightToLeft, Direction::LeftToRight),
    ] {
        let shaped = shape(text, actual);
        assert!(OwnedTextRun::from_shaped_run(context(wrong, 10.0), &shaped, 0.01).is_err());
    }
}

#[test]
fn invalid_sizes_scales_and_glyph_arithmetic_are_refused() {
    let shaped = shape("ab", Direction::LeftToRight);
    for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(OwnedTextRun::from_shaped_run(context(Direction::LeftToRight, invalid), &shaped, 0.01).is_err());
        assert!(OwnedTextRun::from_shaped_run(context(Direction::LeftToRight, 10.0), &shaped, invalid).is_err());
    }
    for mutation in 0..3 {
        let mut broken = shaped.clone();
        match mutation {
            0 => broken.glyphs[0].glyph_id = 0,
            1 => broken.glyphs[0].x_advance = -1,
            _ => broken.glyphs[0].x_advance = i32::MIN,
        }
        assert!(OwnedTextRun::from_shaped_run(context(Direction::LeftToRight, 10.0), &broken, 0.01).is_err());
    }
    // Each individual advance fits; the accumulated third advance does not.
    let three = shape("aaa", Direction::LeftToRight);
    assert!(OwnedTextRun::from_shaped_run(context(Direction::LeftToRight, 10.0), &three, f32::MAX / 1000.0).is_err());
    let mut displaced = shaped.clone();
    displaced.glyphs[0].x_offset = i32::MAX;
    assert!(OwnedTextRun::from_shaped_run(context(Direction::LeftToRight, 10.0), &displaced, f32::MAX / 1000.0).is_err());
}

#[test]
fn non_monotone_overlapping_empty_and_missing_source_clusters_are_refused() {
    for direction in [Direction::LeftToRight, Direction::RightToLeft] {
        let good = shape(if direction == Direction::LeftToRight { "abba" } else { "بببب" }, direction);
        for mutation in 0..7 {
            let mut broken = good.clone();
            match mutation {
                0 => broken.glyphs.clear(),
                1 => { broken.glyphs.remove(0); }
                2 => { broken.glyphs.pop(); }
                3 => broken.glyphs.swap(0, 1),
                4 => broken.glyphs[1].cluster = broken.glyphs[0].cluster.start..broken.glyphs[1].cluster.end,
                5 => broken.glyphs[0].cluster = 0..0,
                _ => { let range = broken.glyphs[0].cluster.clone(); broken.glyphs[3].cluster = range; }
            }
            assert!(OwnedTextRun::from_shaped_run(context(direction, 10.0), &broken, 0.01).is_err(),
                "accepted mutation {mutation} in {direction:?}");
        }
    }
    let mut middle_of_scalar = shape("a\u{0301}b", Direction::LeftToRight);
    middle_of_scalar.glyphs[0].cluster.end = 2;
    assert!(OwnedTextRun::from_shaped_run(context(Direction::LeftToRight, 10.0), &middle_of_scalar, 0.01).is_err());
}

#[test]
fn empty_shaped_input_remains_a_valid_empty_owned_run() {
    for direction in [Direction::LeftToRight, Direction::RightToLeft] {
        let shaped = shape("", direction);
        let run = OwnedTextRun::from_shaped_run(context(direction, 10.0), &shaped, 0.01).unwrap();
        assert!(run.logical_text.is_empty());
        assert!(run.glyphs.is_empty());
        assert!(run.clusters.is_empty());
        assert_eq!(run.total_advance, 0.0);
    }
}
