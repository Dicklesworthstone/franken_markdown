//! Comprehensive test suite for safe native shaping route contract (FCB-016.B).
//!
//! Verifies:
//! - Optional adapter translates owned platform results without polluting base engine.
//! - Capability reporting distinguishes deterministic bundled faces from platform fallback faces.
//! - Fallback font identities are strictly preserved per-glyph and per-cluster (Plan §13.7).
//! - Negative control: context work budget exceeded triggers [`NativeShapingError::ContextBudgetExceeded`] (Plan §10.9).
//! - Negative control: disabled fallback triggers [`NativeShapingError::FallbackRequired`].
//! - Negative control: mid-scalar / out-of-bounds platform offsets safely rejected.
//! - Bidi / RTL layout and hit testing through the platform route.
//! - Complete headless execution without AppKit/CoreText runtime.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::native_route::{
    assemble_platform_run, FallbackFace, NativeShapingError, NativeShapingRequest,
    NativeShapingRoute, PlatformRunGlyph, PlatformShapedOutput, ShapingRouteCapabilities,
    ShapingRouteKind, SimulatedFallbackRule, SimulatedNativeRoute,
};
use fmd_font::shaping::Direction;
use fmd_font::text_run::{CaretAffinity, FontId, FontOrigin, TextRunContext};

#[test]
fn route_capabilities_honesty_and_defaults() {
    let route = SimulatedNativeRoute::new();
    let caps = route.capabilities();

    assert_eq!(caps.route_kind, ShapingRouteKind::SimulatedPlatform);
    assert!(!caps.is_pixel_deterministic);
    assert!(caps.supports_fallback_fonts);
    assert!(caps.supports_color_emoji);
    assert!(caps.supports_bidi);
    assert_eq!(caps.max_paragraph_bytes, 65536);

    assert!(ShapingRouteKind::BundledShaper.is_pixel_deterministic());
    assert!(!ShapingRouteKind::SystemPlatform.is_pixel_deterministic());
    assert!(!ShapingRouteKind::SimulatedPlatform.is_pixel_deterministic());
}

#[test]
fn pure_primary_font_shaping_preserves_bundled_origin() {
    let route = SimulatedNativeRoute::new();
    let primary_font_id = FontId::new(101);

    let text = "fn main() { println!(); }";
    let req = NativeShapingRequest {
        text,
        primary_font_id,
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = route.shape_run(&req).expect("shape pure primary text");

    assert_eq!(run.logical_text, text);
    assert_eq!(run.context.font_origin, FontOrigin::BundledFace);
    assert_eq!(run.context.font_id, primary_font_id);
    assert!(run.total_advance > 0.0);

    // Every glyph and cluster must be attributed to primary font
    for glyph in &run.glyphs {
        assert_eq!(glyph.font_id, primary_font_id);
    }
    for cluster in &run.clusters {
        assert_eq!(cluster.font_id, primary_font_id);
    }

    // Hit test at start
    let hit_start = run.hit_test(0.0);
    assert_eq!(hit_start.caret.byte_offset, 0);
    assert_eq!(hit_start.caret.affinity, CaretAffinity::Leading);

    // Hit test at end
    let hit_end = run.hit_test(run.total_advance + 10.0);
    assert_eq!(hit_end.caret.byte_offset, text.len());
    assert_eq!(hit_end.caret.affinity, CaretAffinity::Trailing);
}

#[test]
fn mixed_fallback_font_preserves_distinct_fallback_identities() {
    let mut route = SimulatedNativeRoute::new();
    let primary_font_id = FontId::new(1000);
    let cjk_font_id = FontId::new(2000);
    let emoji_font_id = FontId::new(3000);

    // Register CJK fallback rule (U+4E00..=U+9FFF)
    route.register_fallback(SimulatedFallbackRule {
        range: 0x4E00..0xA000,
        fallback_face: FallbackFace {
            font_id: cjk_font_id,
            family_name: "PingFang SC".to_string(),
            postscript_name: "PingFangSC-Regular".to_string(),
            units_per_em: 1000,
            is_color_emoji: false,
        },
        advance_per_em: 1.0, // CJK full-width
    });

    // Register Emoji fallback rule (U+1F600..=U+1F64F)
    route.register_fallback(SimulatedFallbackRule {
        range: 0x1F600..0x1F650,
        fallback_face: FallbackFace {
            font_id: emoji_font_id,
            family_name: "Apple Color Emoji".to_string(),
            postscript_name: "AppleColorEmoji".to_string(),
            units_per_em: 1000,
            is_color_emoji: true,
        },
        advance_per_em: 1.0,
    });

    let text = "Rust 语言 😀";
    // byte layout:
    // 'R'(0..1), 'u'(1..2), 's'(2..3), 't'(3..4), ' '(4..5),
    // '语'(5..8, 3 bytes), '言'(8..11, 3 bytes), ' '(11..12),
    // '😀'(12..16, 4 bytes)
    assert_eq!(text.len(), 16);

    let req = NativeShapingRequest {
        text,
        primary_font_id,
        font_size: 16.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = route.shape_run(&req).expect("shape mixed text");

    // Origin is honestly downgraded to SystemFallbackFace because fallback was used
    assert_eq!(run.context.font_origin, FontOrigin::SystemFallbackFace);
    assert_eq!(run.logical_text, text);

    // Verify individual cluster and glyph font identities (Plan §13.7)
    let cjk1 = run
        .clusters
        .iter()
        .find(|c| c.byte_range == (5..8))
        .expect("cjk cluster 1");
    assert_eq!(cjk1.font_id, cjk_font_id);
    assert_eq!(run.glyphs[cjk1.glyph_range.start].font_id, cjk_font_id);

    let cjk2 = run
        .clusters
        .iter()
        .find(|c| c.byte_range == (8..11))
        .expect("cjk cluster 2");
    assert_eq!(cjk2.font_id, cjk_font_id);
    assert_eq!(run.glyphs[cjk2.glyph_range.start].font_id, cjk_font_id);

    let emoji = run
        .clusters
        .iter()
        .find(|c| c.byte_range == (12..16))
        .expect("emoji cluster");
    assert_eq!(emoji.font_id, emoji_font_id);
    assert_eq!(run.glyphs[emoji.glyph_range.start].font_id, emoji_font_id);

    // ASCII clusters must still retain primary font
    let ascii_r = run
        .clusters
        .iter()
        .find(|c| c.byte_range == (0..1))
        .expect("ascii 'R'");
    assert_eq!(ascii_r.font_id, primary_font_id);
    assert_eq!(run.glyphs[ascii_r.glyph_range.start].font_id, primary_font_id);

    // Check UTF-16 code units: emoji 😀 is 2 UTF-16 surrogate code units
    // Text UTF-16 len: 'R'(1) + 'u'(1) + 's'(1) + 't'(1) + ' '(1) + '语'(1) + '言'(1) + ' '(1) + '😀'(2) = 10 units
    assert_eq!(emoji.utf16_range, 8..10);

    // Selection rectangles across mixed fallback characters
    let rects = run.selection_rects(5..16, 0.0, 20.0);
    assert!(!rects.is_empty());
}

#[test]
fn fallback_disabled_yields_fallback_required_error() {
    let mut route = SimulatedNativeRoute::new();
    let primary_font_id = FontId::new(1000);
    let cjk_font_id = FontId::new(2000);

    route.register_fallback(SimulatedFallbackRule {
        range: 0x4E00..0xA000,
        fallback_face: FallbackFace {
            font_id: cjk_font_id,
            family_name: "PingFang SC".to_string(),
            postscript_name: "PingFangSC-Regular".to_string(),
            units_per_em: 1000,
            is_color_emoji: false,
        },
        advance_per_em: 1.0,
    });

    let text = "Hello 世界";
    let req = NativeShapingRequest {
        text,
        primary_font_id,
        font_size: 16.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: false, // Disallow fallback!
    };

    let err = route.shape_run(&req).unwrap_err();
    assert_eq!(
        err,
        NativeShapingError::FallbackRequired {
            unshaped_byte_offset: 6,
        }
    );
}

#[test]
fn context_work_budget_enforcement_negative_control() {
    let custom_caps = ShapingRouteCapabilities {
        max_paragraph_bytes: 128, // 128-byte strict work budget
        ..ShapingRouteCapabilities::default()
    };
    let route = SimulatedNativeRoute::with_capabilities(custom_caps);
    let primary_font_id = FontId::new(555);

    // 129 bytes: exceeds the 128 byte budget
    let giant_text = "A".repeat(129);
    let req = NativeShapingRequest {
        text: &giant_text,
        primary_font_id,
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let err = route.shape_run(&req).unwrap_err();
    assert_eq!(
        err,
        NativeShapingError::ContextBudgetExceeded {
            length: 129,
            max_allowed: 128,
        }
    );

    // 128 bytes: exactly on the budget, must succeed
    let valid_text = "A".repeat(128);
    let valid_req = NativeShapingRequest {
        text: &valid_text,
        primary_font_id,
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };
    let run = route.shape_run(&valid_req).expect("within budget");
    assert_eq!(run.clusters.len(), 128);
}

#[test]
fn empty_text_guard_negative_control() {
    let route = SimulatedNativeRoute::new();
    let req = NativeShapingRequest {
        text: "",
        primary_font_id: FontId::new(1),
        font_size: 12.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let err = route.shape_run(&req).unwrap_err();
    assert_eq!(err, NativeShapingError::EmptyText);
}

#[test]
fn mid_scalar_and_out_of_bounds_assembly_rejection() {
    let primary_context = TextRunContext {
        font_id: FontId::new(1),
        font_size: 14.0,
        script: *b"latn",
        language: *b"dflt",
        direction: Direction::LeftToRight,
        font_origin: FontOrigin::BundledFace,
    };

    // Case 1: out-of-bounds cluster range
    let bad_output_oob = PlatformShapedOutput {
        logical_text: "abc".to_string(),
        direction: Direction::LeftToRight,
        font_size: 14.0,
        glyphs: vec![PlatformRunGlyph {
            glyph_id: 1,
            font_id: FontId::new(1),
            cluster_byte_offset: 0,
            cluster_byte_len: 10, // beyond length 3
            x_advance: 10.0,
            y_advance: 0.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        fallback_faces: Vec::new(),
        route_kind: ShapingRouteKind::SimulatedPlatform,
    };
    let err_oob = assemble_platform_run(primary_context.clone(), bad_output_oob).unwrap_err();
    assert_eq!(
        err_oob,
        NativeShapingError::InvalidByteRange {
            offset: 10,
            text_len: 3,
        }
    );

    // Case 2: mid-scalar byte boundary
    // "café" has 'é' at bytes 3..5. Offset 4 is mid-scalar!
    let bad_output_scalar = PlatformShapedOutput {
        logical_text: "café".to_string(),
        direction: Direction::LeftToRight,
        font_size: 14.0,
        glyphs: vec![PlatformRunGlyph {
            glyph_id: 1,
            font_id: FontId::new(1),
            cluster_byte_offset: 4, // mid-scalar
            cluster_byte_len: 1,
            x_advance: 10.0,
            y_advance: 0.0,
            x_offset: 0.0,
            y_offset: 0.0,
        }],
        fallback_faces: Vec::new(),
        route_kind: ShapingRouteKind::SimulatedPlatform,
    };
    let err_scalar = assemble_platform_run(primary_context, bad_output_scalar).unwrap_err();
    assert_eq!(
        err_scalar,
        NativeShapingError::MidScalarBoundary { offset: 4 }
    );
}

#[test]
fn rtl_platform_route_layout_and_hit_testing() {
    let route = SimulatedNativeRoute::new();
    let primary_font_id = FontId::new(777);

    let text = "مرحبا";
    let req = NativeShapingRequest {
        text,
        primary_font_id,
        font_size: 16.0,
        direction: Direction::RightToLeft,
        script: *b"arab",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = route.shape_run(&req).expect("shape rtl");
    assert_eq!(run.context.direction, Direction::RightToLeft);
    assert!(run.total_advance > 0.0);

    // Hit testing RTL text
    let hit_right = run.hit_test(run.total_advance);
    assert_eq!(hit_right.caret.byte_offset, 0);

    let hit_left = run.hit_test(0.0);
    assert!(hit_left.caret.byte_offset <= text.len());
}
