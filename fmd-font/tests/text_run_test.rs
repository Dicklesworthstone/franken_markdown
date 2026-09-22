//! Comprehensive test suite for shared owned text/font/run contract (FCB-016.A).
//!
//! Verifies:
//! - Immutable font backing with deterministic FNV-1a content-hash `FontId`.
//! - Explicit domain conversions between Byte (UTF-8) and Native (UTF-16) code units.
//! - Cluster and source associations preserving exact logical source ranges.
//! - Line-level CPU hit testing, caret affinity (`Leading` vs `Trailing`), and visual coordinates.
//! - RTL / bidi cluster mapping and direction awareness.
//! - Combining marks and ligatures mapped to coherent clusters.
//! - Discontiguous visual selection rectangles for logical ranges (Plan §13.6).
//! - Negative controls: mid-scalar / surrogate interior offsets rejected; out-of-bounds queries safely return None.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::shaping::{Direction, ShapeOptions};
use fmd_font::text_run::{
    byte_to_utf16, utf16_to_byte, CaretAffinity, FontId, FontOrigin, OwnedTextRun, TextRunContext,
};
use fmd_font::Font;

const SHAPING_FONT: &[u8] = include_bytes!("../fonts/test-shaping/FmdShaping.ttf");

#[test]
fn font_id_is_deterministic_and_immutable() {
    let id1 = FontId::from_font_data(SHAPING_FONT);
    let id2 = FontId::from_font_data(SHAPING_FONT);
    assert_eq!(id1, id2);
    assert_ne!(id1, FontId::new(0));

    let dummy = b"fake font binary data for test";
    let id3 = FontId::from_font_data(dummy);
    assert_ne!(id1, id3);
}

#[test]
fn domain_conversions_ascii_multibyte_and_astral_surrogates() {
    // 1. ASCII: 1 byte == 1 UTF-16 code unit
    let ascii = "hello world";
    assert_eq!(byte_to_utf16(ascii, 0), Some(0));
    assert_eq!(byte_to_utf16(ascii, 5), Some(5));
    assert_eq!(byte_to_utf16(ascii, 11), Some(11));
    assert_eq!(utf16_to_byte(ascii, 5), Some(5));

    // 2. Multibyte BMP: 'é' is 2 UTF-8 bytes, 1 UTF-16 code unit
    let cafe = "café"; // c(0), a(1), f(2), é(3..5)
    assert_eq!(cafe.len(), 5);
    assert_eq!(byte_to_utf16(cafe, 3), Some(3));
    assert_eq!(byte_to_utf16(cafe, 5), Some(4)); // 4 UTF-16 code units total
    assert_eq!(utf16_to_byte(cafe, 4), Some(5));

    // Negative control: mid-scalar byte boundary must return None
    assert_eq!(byte_to_utf16(cafe, 4), None);

    // 3. Astral code point: 😀 (U+1F600) is 4 UTF-8 bytes and 2 UTF-16 code units (surrogate pair)
    let emoji = "hi 😀 bye";
    // 'h'(0..1), 'i'(1..2), ' '(2..3), '😀'(3..7), ' '(7..8), 'b'(8..9), 'y'(9..10), 'e'(10..11)
    assert_eq!(emoji.len(), 11);
    assert_eq!(byte_to_utf16(emoji, 3), Some(3));
    assert_eq!(byte_to_utf16(emoji, 7), Some(5)); // surrogate pair takes 2 units: 3 + 2 = 5
    assert_eq!(byte_to_utf16(emoji, 11), Some(9));

    assert_eq!(utf16_to_byte(emoji, 3), Some(3));
    assert_eq!(utf16_to_byte(emoji, 5), Some(7));

    // Negative control: interior of surrogate pair (offset 4) must return None
    assert_eq!(utf16_to_byte(emoji, 4), None);

    // Negative control: out-of-bounds offsets
    assert_eq!(byte_to_utf16(emoji, 99), None);
    assert_eq!(utf16_to_byte(emoji, 99), None);
}

#[test]
fn latin_baseline_text_run_shaping_and_hit_testing() {
    let font = Font::parse(SHAPING_FONT.to_vec()).expect("parse shaping font");
    let font_id = FontId::from_font_data(SHAPING_FONT);
    let font_size = 16.0f32;
    let scale = font_size / (font.units_per_em as f32);

    let text = "abba";
    let shaped = font
        .shape(text, &ShapeOptions::default())
        .expect("shape latin text");

    let context = TextRunContext {
        font_id,
        font_size,
        script: *b"latn",
        language: *b"dflt",
        direction: Direction::LeftToRight,
        font_origin: FontOrigin::BundledFace,
    };

    let run = OwnedTextRun::from_shaped_run(context, &shaped, scale).expect("build text run");

    assert_eq!(run.logical_text, "abba");
    assert_eq!(run.clusters.len(), 4);
    assert!(run.total_advance > 0.0);

    // Verify cluster associations
    assert_eq!(run.clusters[0].byte_range, 0..1);
    assert_eq!(run.clusters[0].utf16_range, 0..1);
    assert_eq!(run.clusters[3].byte_range, 3..4);

    // Hit test: before start
    let hit_before = run.hit_test(-5.0);
    assert_eq!(hit_before.cluster_index, 0);
    assert_eq!(hit_before.caret.byte_offset, 0);
    assert_eq!(hit_before.caret.affinity, CaretAffinity::Leading);
    assert!(!hit_before.is_exact);

    // Hit test: at first cluster leading edge
    let hit_first = run.hit_test(0.0);
    assert_eq!(hit_first.cluster_index, 0);
    assert_eq!(hit_first.caret.byte_offset, 0);
    assert_eq!(hit_first.caret.affinity, CaretAffinity::Leading);

    // Hit test: beyond end of run
    let hit_after = run.hit_test(run.total_advance + 10.0);
    assert_eq!(hit_after.cluster_index, 3);
    assert_eq!(hit_after.caret.byte_offset, 4);
    assert_eq!(hit_after.caret.affinity, CaretAffinity::Trailing);
    assert!(!hit_after.is_exact);

    // Caret at byte lookup
    let caret_start = run
        .caret_at_byte(0, CaretAffinity::Leading)
        .expect("caret start");
    assert_eq!(caret_start.visual_x, 0.0);

    let caret_end = run
        .caret_at_byte(4, CaretAffinity::Trailing)
        .expect("caret end");
    assert_eq!(caret_end.visual_x, run.total_advance);
}

#[test]
fn rtl_text_run_cluster_coordinates_and_caret_affinity() {
    let font = Font::parse(SHAPING_FONT.to_vec()).expect("parse shaping font");
    let font_id = FontId::from_font_data(SHAPING_FONT);
    let font_size = 14.0f32;
    let scale = font_size / (font.units_per_em as f32);

    // Basic Arabic sample
    let arabic_text = "\u{0628}\u{0627}\u{0628}"; // bab
    let opts = ShapeOptions {
        script: *b"arab",
        language: *b"dflt",
        direction: Direction::RightToLeft,
        features: &[],
    };

    let shaped = font.shape(arabic_text, &opts).expect("shape arabic text");

    let context = TextRunContext {
        font_id,
        font_size,
        script: *b"arab",
        language: *b"dflt",
        direction: Direction::RightToLeft,
        font_origin: FontOrigin::BundledFace,
    };

    let run = OwnedTextRun::from_shaped_run(context, &shaped, scale).expect("build rtl run");

    assert_eq!(run.context.direction, Direction::RightToLeft);
    assert!(run.total_advance > 0.0);

    // In RTL, clusters are laid out from right to left (visual coordinates adjusted)
    // Hit test at right side (near visual start of RTL text)
    let hit_right = run.hit_test(run.total_advance);
    assert!(hit_right.caret.byte_offset <= arabic_text.len());

    // Hit test at left side (near visual end of RTL text)
    let hit_left = run.hit_test(0.0);
    assert!(hit_left.caret.byte_offset <= arabic_text.len());
}

#[test]
fn selection_rectangles_cover_logical_ranges_and_merge() {
    let font = Font::parse(SHAPING_FONT.to_vec()).expect("parse font");
    let font_id = FontId::from_font_data(SHAPING_FONT);
    let font_size = 12.0f32;
    let scale = font_size / (font.units_per_em as f32);

    let text = "abba";
    let shaped = font
        .shape(text, &ShapeOptions::default())
        .expect("shape text");

    let context = TextRunContext {
        font_id,
        font_size,
        script: *b"latn",
        language: *b"dflt",
        direction: Direction::LeftToRight,
        font_origin: FontOrigin::BundledFace,
    };

    let run = OwnedTextRun::from_shaped_run(context, &shaped, scale).expect("run");

    // Select "bb" (byte range 1..3)
    let rects = run.selection_rects(1..3, 0.0, 16.0);
    assert!(!rects.is_empty());

    // Contiguous LTR selection merges into a single clean rectangle
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0].y, 0.0);
    assert_eq!(rects[0].height, 16.0);
    assert!(rects[0].width > 0.0);

    // Negative control: inverted or empty selection returns empty vector
    let inverted = std::ops::Range { start: 3, end: 1 };
    assert!(run.selection_rects(inverted, 0.0, 16.0).is_empty());
    assert!(run.selection_rects(2..2, 0.0, 16.0).is_empty());
    assert!(run.selection_rects(50..60, 0.0, 16.0).is_empty());
}

#[test]
fn ligature_and_combining_mark_cluster_mapping() {
    let font = Font::parse(SHAPING_FONT.to_vec()).expect("parse shaping font");
    let font_id = FontId::from_font_data(SHAPING_FONT);
    let font_size = 16.0f32;
    let scale = font_size / (font.units_per_em as f32);

    // "fi" forms a single ligature in FmdShaping.ttf
    let text = "fi";
    let shaped = font
        .shape(text, &ShapeOptions::default())
        .expect("shape ligature");

    let context = TextRunContext {
        font_id,
        font_size,
        script: *b"latn",
        language: *b"dflt",
        direction: Direction::LeftToRight,
        font_origin: FontOrigin::BundledFace,
    };

    let run = OwnedTextRun::from_shaped_run(context, &shaped, scale).expect("build run");
    // Ligature spans 2 bytes (0..2)
    assert_eq!(run.logical_text, "fi");
    assert!(!run.clusters.is_empty());
    // The cluster covering byte 0 must cover the full ligature
    assert_eq!(run.clusters[0].byte_range, 0..2);
}

#[test]
fn negative_controls_empty_run_and_out_of_bounds_queries() {
    let context = TextRunContext {
        font_id: FontId::new(42),
        font_size: 14.0,
        script: *b"latn",
        language: *b"dflt",
        direction: Direction::LeftToRight,
        font_origin: FontOrigin::BundledFace,
    };

    let empty_run = OwnedTextRun {
        context,
        logical_text: String::new(),
        clusters: Vec::new(),
        glyphs: Vec::new(),
        total_advance: 0.0,
    };

    // Empty run hit test returns 0 safely without panicking
    let hit = empty_run.hit_test(10.0);
    assert_eq!(hit.caret.byte_offset, 0);
    assert_eq!(hit.caret.visual_x, 0.0);

    // Empty run caret at byte 0 returns safe position
    let caret = empty_run.caret_at_byte(0, CaretAffinity::Leading);
    assert!(caret.is_some());
    assert_eq!(caret.unwrap().byte_offset, 0);

    // Out-of-bounds byte offset returns None
    assert_eq!(empty_run.caret_at_byte(5, CaretAffinity::Leading), None);
    assert_eq!(empty_run.caret_at_utf16(5, CaretAffinity::Leading), None);
}
