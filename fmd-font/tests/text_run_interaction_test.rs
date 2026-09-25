//! Atomic text interaction over real shaped Latin and Arabic clusters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::Font;
use fmd_font::shaping::{Direction, ShapeOptions};
use fmd_font::text_run::{CaretAffinity, FontId, FontOrigin, OwnedTextRun, TextRunContext};

const FONT: &[u8] = include_bytes!("../fonts/test-shaping/FmdShaping.ttf");
const REFERENCE: &str = include_str!("../fonts/test-shaping/harfbuzz-reference.tsv");

fn run_at(text: &str, direction: Direction, scale: f32) -> OwnedTextRun {
    let script = if direction == Direction::RightToLeft { *b"arab" } else { *b"latn" };
    let shaped = Font::parse(FONT.to_vec()).unwrap().shape(text, &ShapeOptions {
        script, language: *b"dflt", direction, features: &[],
    }).unwrap();
    OwnedTextRun::from_shaped_run(TextRunContext {
        font_id: FontId::from_font_data(FONT), font_size: 10.0, script,
        language: *b"dflt", direction, font_origin: FontOrigin::BundledFace,
    }, &shaped, scale).unwrap()
}

fn run(text: &str, direction: Direction) -> OwnedTextRun {
    run_at(text, direction, 0.01)
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

#[test]
fn ligature_interiors_snap_every_coordinate_domain_to_a_real_edge() {
    let run = run("fi", Direction::LeftToRight);
    for (affinity, byte, utf16, x) in [
        (CaretAffinity::Leading, 0, 0, 0.0),
        (CaretAffinity::Trailing, 2, 2, 5.0),
    ] {
        let caret = run.caret_at_byte(1, affinity).unwrap();
        assert_eq!((caret.byte_offset, caret.utf16_offset), (byte, utf16));
        close(caret.visual_x, x);
        assert_eq!(run.caret_at_utf16(1, affinity), Some(caret));
    }
    // At endpoints there is no outward cluster to choose.
    let start = run.caret_at_byte(0, CaretAffinity::Trailing).unwrap();
    assert_eq!(start.byte_offset, 0);
    assert_eq!(start.affinity, CaretAffinity::Leading);
    close(start.visual_x, 0.0);
    let end = run.caret_at_byte(2, CaretAffinity::Leading).unwrap();
    assert_eq!(end.byte_offset, 2);
    assert_eq!(end.affinity, CaretAffinity::Trailing);
    close(end.visual_x, 5.0);
    let rects = run.selection_rects(1..2, 0.0, 12.0);
    assert_eq!(rects.len(), 1);
    close(rects[0].width, 5.0);
}

#[test]
fn combining_clusters_do_not_expose_carets_between_base_and_marks() {
    let run = run("a\u{0301}\u{0307}b", Direction::LeftToRight);
    for byte in [1, 3] {
        let leading = run.caret_at_byte(byte, CaretAffinity::Leading).unwrap();
        let trailing = run.caret_at_byte(byte, CaretAffinity::Trailing).unwrap();
        assert_eq!((leading.byte_offset, leading.utf16_offset), (0, 0));
        assert_eq!((trailing.byte_offset, trailing.utf16_offset), (5, 3));
        close(leading.visual_x, 0.0);
        close(trailing.visual_x, 5.0);
    }
    for utf16 in [1, 2] {
        assert_eq!(run.caret_at_utf16(utf16, CaretAffinity::Trailing).unwrap().byte_offset, 5);
    }
    for invalid in [2, 4, 7, usize::MAX] {
        assert!(run.caret_at_byte(invalid, CaretAffinity::Leading).is_none());
    }
    for affinity in [CaretAffinity::Leading, CaretAffinity::Trailing] {
        let boundary = run.caret_at_byte(5, affinity).unwrap();
        assert_eq!((boundary.byte_offset, boundary.utf16_offset), (5, 3));
        assert_eq!(boundary.affinity, affinity);
        close(boundary.visual_x, 5.0);
    }
}

#[test]
fn arabic_ligatures_and_marks_snap_in_logical_not_visual_direction() {
    for (source, interior, end, units, left, right) in [
        ("لا", 2, 4, 2, 0.0, 5.0),
        ("بَب", 2, 4, 2, 5.0, 10.0),
    ] {
        let run = run(source, Direction::RightToLeft);
        let leading = run.caret_at_byte(interior, CaretAffinity::Leading).unwrap();
        let trailing = run.caret_at_byte(interior, CaretAffinity::Trailing).unwrap();
        assert_eq!((leading.byte_offset, leading.utf16_offset), (0, 0));
        assert_eq!((trailing.byte_offset, trailing.utf16_offset), (end, units));
        close(leading.visual_x, right);
        close(trailing.visual_x, left);
        assert_eq!(run.caret_at_utf16(1, CaretAffinity::Trailing), Some(trailing));
    }
}

#[test]
fn hit_test_edges_round_trip_through_byte_and_utf16_caret_queries() {
    let mut cases = 0;
    for line in REFERENCE.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<_> = line.split('\t').collect();
        let rtl = fields[0] == "arab";
        let run = run(fields[1], if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
        for cluster in &run.clusters {
            if cluster.x_start == cluster.x_end { continue; }
            for fraction in [0.25, 0.5, 0.75] {
                let x = cluster.x_start + (cluster.x_end - cluster.x_start) * fraction;
                let hit = run.hit_test(x);
                assert_eq!(hit.cluster_index, cluster.cluster_index);
                assert!([cluster.byte_range.start, cluster.byte_range.end].contains(&hit.caret.byte_offset));
                let from_byte = run.caret_at_byte(hit.caret.byte_offset, hit.caret.affinity).unwrap();
                let from_utf16 = run.caret_at_utf16(hit.caret.utf16_offset, hit.caret.affinity).unwrap();
                assert_eq!(from_byte, hit.caret);
                assert_eq!(from_utf16, hit.caret);
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 14);
}

#[test]
fn adjacent_rtl_selections_union_in_visual_order() {
    let run = run("بببب", Direction::RightToLeft);
    for (range, left, width) in [(0..8, 0.0, 20.0), (0..4, 10.0, 10.0), (2..6, 5.0, 10.0)] {
        let rects = run.selection_rects(range, 3.0, 12.0);
        assert_eq!(rects.len(), 1);
        close(rects[0].x, left);
        close(rects[0].width, width);
        assert_eq!(rects[0].y, 3.0);
        assert_eq!(rects[0].height, 12.0);
    }
}

#[test]
fn host_supplied_visual_permutations_keep_discontiguous_selections() {
    // Explicit host geometry, not a claim of automatic bidi segmentation.
    // Glyphs remain visual; the cluster inventory remains source-ordered.
    let mut run = run("abba", Direction::LeftToRight);
    let original = run.glyphs.clone();
    let mut pen = 0.0;
    for (visual, logical) in [0, 2, 3, 1].into_iter().enumerate() {
        run.glyphs[visual] = original[logical];
        run.clusters[logical].glyph_range = visual..visual + 1;
        run.clusters[logical].x_start = pen;
        pen += run.glyphs[visual].x_advance;
        run.clusters[logical].x_end = pen;
    }
    run.total_advance = pen;
    let disjoint = run.selection_rects(0..2, 0.0, 12.0);
    assert_eq!(disjoint.len(), 2);
    close(disjoint[0].x, 0.0);
    close(disjoint[0].width, 4.6);
    close(disjoint[1].x, 14.4);
    close(disjoint[1].width, 4.8);
    let joined = run.selection_rects(1..4, 0.0, 12.0);
    assert_eq!(joined.len(), 1);
    close(joined[0].x, 4.6);
    close(joined[0].width, 14.6);

    // Effective host cluster rectangles may overlap. Union, do not add widths.
    run.clusters[0].x_end = 6.0;
    let overlap = run.selection_rects(0..3, 0.0, 12.0);
    assert_eq!(overlap.len(), 2);
    close(overlap[0].x, 0.0);
    close(overlap[0].width, 9.4);
    close(overlap[1].x, 14.4);
}

#[test]
fn invalid_selections_return_no_partial_geometry() {
    let mut run = run("a\u{0301}b", Direction::LeftToRight);
    for range in [2..3, 0..2, 0..5, 0..usize::MAX, std::ops::Range { start: 3, end: 1 }] {
        assert!(run.selection_rects(range, 0.0, 12.0).is_empty());
    }
    for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(run.selection_rects(0..4, 0.0, invalid).is_empty());
    }
    assert!(run.selection_rects(0..4, f32::NAN, 12.0).is_empty());
    assert!(run.selection_rects(0..4, f32::MAX, f32::MAX).is_empty());
    assert!(!run.selection_rects(0..4, -4.0, 12.0).is_empty());
    run.clusters[1].x_start = f32::NAN;
    assert!(run.selection_rects(0..4, 0.0, 12.0).is_empty());
}

#[test]
fn nonfinite_hits_and_large_finite_geometry_have_consistent_edges() {
    for (text, direction) in [("ab", Direction::LeftToRight), ("بب", Direction::RightToLeft)] {
        let run = run(text, direction);
        let invalid = run.hit_test(f32::NAN);
        assert!(!invalid.is_exact);
        assert_eq!((invalid.caret.byte_offset, invalid.caret.utf16_offset), (0, 0));
        assert_eq!(run.caret_at_byte(0, CaretAffinity::Leading), Some(invalid.caret));
        for (x, expected) in if direction == Direction::LeftToRight {
            [(f32::NEG_INFINITY, 0), (f32::INFINITY, text.len())]
        } else {
            [(f32::NEG_INFINITY, text.len()), (f32::INFINITY, 0)]
        } {
            let hit = run.hit_test(x);
            assert!(!hit.is_exact);
            assert_eq!(hit.caret.byte_offset, expected);
            assert!(hit.caret.visual_x.is_finite());
        }
    }
    let large = run_at("ab", Direction::LeftToRight, f32::MAX / 1000.0);
    let last = &large.clusters[1];
    assert!(!(last.x_start + last.x_end).is_finite()); // old midpoint overflow
    let x = last.x_start + (last.x_end - last.x_start) * 0.75;
    let hit = large.hit_test(x);
    assert_eq!(hit.caret.byte_offset, 2);
    assert_eq!(hit.caret.visual_x, last.x_end);
}
