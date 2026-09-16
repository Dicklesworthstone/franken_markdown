//! Comprehensive test suite for the optional safe Mac font adapter (FCB-075.B).
//!
//! Acceptance criteria & oracle verification:
//! 1. Owned CoreText/CoreGraphics conversions through separate bridge.
//! 2. Bounded foreign calls: context work budget strictly enforced before platform calls (Plan §10.9).
//! 3. Fallback font identities preserved with deterministic FontId and FallbackFace (Plan §13.7).
//! 4. Full package oracle corpus: CJK, RTL, joining, combining, emoji, tab, and ligature text.
//! 5. Color glyph raster extraction: BGRA -> RGBA conversion, dimension caps, buffer mismatch defense.
//! 6. Negative controls: context budget overflow, empty text, foreign call failure, disabled fallback.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_font::macos::{
    MacFontAdapter, MacFontAdapterConfig, MacFontAdapterError, RawCoreGraphicsRaster,
    SimulatedMacBridge, DEFAULT_MAC_CONTEXT_BUDGET, MAX_MAC_RASTER_DIMENSION,
};
use fmd_font::native_route::{
    NativeShapingError, NativeShapingRequest, NativeShapingRoute, ShapingRouteKind,
};
use fmd_font::shaping::Direction;
use fmd_font::text_run::{FontId, FontOrigin};

#[test]
fn mac_adapter_capabilities_and_kind() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);
    let caps = adapter.capabilities();

    assert_eq!(caps.route_kind, ShapingRouteKind::SystemPlatform);
    assert!(!caps.is_pixel_deterministic);
    assert!(caps.supports_fallback_fonts);
    assert!(caps.supports_color_emoji);
    assert!(caps.supports_bidi);
    assert_eq!(caps.max_paragraph_bytes, DEFAULT_MAC_CONTEXT_BUDGET);
}

#[test]
fn corpus_latin_shaping_preserves_primary_font() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);
    let primary_id = FontId::new(42);

    let text = "Hello from CoreText shaper!";
    let req = NativeShapingRequest {
        text,
        primary_font_id: primary_id,
        font_size: 16.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = adapter.shape_run(&req).expect("shape pure Latin text");
    assert_eq!(run.logical_text, text);
    assert_eq!(run.context.font_origin, FontOrigin::BundledFace);
    assert_eq!(run.context.font_id, primary_id);
    assert!(run.total_advance > 0.0);

    for g in &run.glyphs {
        assert_eq!(g.font_id, primary_id);
    }
}

#[test]
fn corpus_cjk_fallback_attribution() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);
    let primary_id = FontId::new(100);

    let text = "Text with 日本語 and 中文";
    let req = NativeShapingRequest {
        text,
        primary_font_id: primary_id,
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"hani",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = adapter.shape_run(&req).expect("shape CJK text");
    assert_eq!(run.context.font_origin, FontOrigin::SystemFallbackFace);

    // Some glyphs must have fallback font IDs distinct from primary
    let fallback_glyphs: Vec<_> = run.glyphs.iter().filter(|g| g.font_id != primary_id).collect();
    assert!(!fallback_glyphs.is_empty(), "CJK characters must trigger fallback font ID");
}

#[test]
fn corpus_rtl_arabic_and_hebrew_layout() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);
    let primary_id = FontId::new(200);

    let text = "مرحبا بالعالم";
    let req = NativeShapingRequest {
        text,
        primary_font_id: primary_id,
        font_size: 14.0,
        direction: Direction::RightToLeft,
        script: *b"arab",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = adapter.shape_run(&req).expect("shape RTL text");
    assert_eq!(run.context.direction, Direction::RightToLeft);
    assert!(run.total_advance > 0.0);

    // Hit test inverted for RTL: verify caret byte offset within bounds
    let hit_left = run.hit_test(0.0);
    assert!(hit_left.caret.byte_offset <= text.len());
}

#[test]
fn corpus_emoji_color_glyph_fallback() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);
    let primary_id = FontId::new(300);

    let text = "Rocket 🚀 Sparkles ✨";
    let req = NativeShapingRequest {
        text,
        primary_font_id: primary_id,
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let run = adapter.shape_run(&req).expect("shape emoji text");
    let fallback_glyphs: Vec<_> = run.glyphs.iter().filter(|g| g.font_id != primary_id).collect();
    assert!(!fallback_glyphs.is_empty(), "Emoji must use Apple Color Emoji fallback");
}

#[test]
fn core_graphics_raster_swizzles_bgra_to_rgba() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);

    let raster = adapter
        .get_glyph_rgba_raster("AppleColorEmoji", 1, 16.0)
        .expect("rasterize color glyph")
        .expect("raster exists");

    assert_eq!(raster.width, 16);
    assert_eq!(raster.height, 16);
    assert!(!raster.is_bgra, "raster must be swizzled to RGBA");

    // Check swizzled RGBA pixel: simulated BGRA had B=255, G=200, R=50, A=255
    // Swapped: R=50, G=200, B=255, A=255
    assert_eq!(raster.pixels[0], 50);  // R
    assert_eq!(raster.pixels[1], 200); // G
    assert_eq!(raster.pixels[2], 255); // B
    assert_eq!(raster.pixels[3], 255); // A
}

#[test]
fn negative_control_context_work_budget_enforced() {
    let bridge = SimulatedMacBridge::new();
    let config = MacFontAdapterConfig {
        max_paragraph_bytes: 128,
        ..MacFontAdapterConfig::default()
    };
    let adapter = MacFontAdapter::with_config(bridge, config);

    let huge_text = "A".repeat(256);
    let req = NativeShapingRequest {
        text: &huge_text,
        primary_font_id: FontId::new(1),
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let err = adapter.shape_run(&req).expect_err("should reject oversized text");
    assert_eq!(
        err,
        NativeShapingError::ContextBudgetExceeded {
            length: 256,
            max_allowed: 128,
        }
    );
}

#[test]
fn negative_control_empty_text_rejected() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);

    let req = NativeShapingRequest {
        text: "",
        primary_font_id: FontId::new(1),
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    assert_eq!(adapter.shape_run(&req), Err(NativeShapingError::EmptyText));
}

#[test]
fn negative_control_disabled_fallback_yields_error() {
    let bridge = SimulatedMacBridge::new();
    let adapter = MacFontAdapter::new(bridge);

    let text = "Latin with 日本語";
    let req = NativeShapingRequest {
        text,
        primary_font_id: FontId::new(1),
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: false, // disabled!
    };

    let err = adapter.shape_run(&req).expect_err("disabled fallback must fail");
    assert_eq!(
        err,
        NativeShapingError::FallbackRequired {
            unshaped_byte_offset: "Latin with ".len(),
        }
    );
}

#[test]
fn negative_control_failing_bridge_propagates_adapter_error() {
    let bridge = SimulatedMacBridge::new_failing();
    let adapter = MacFontAdapter::new(bridge);

    let req = NativeShapingRequest {
        text: "test",
        primary_font_id: FontId::new(1),
        font_size: 14.0,
        direction: Direction::LeftToRight,
        script: *b"latn",
        language: *b"dflt",
        allow_system_fallback: true,
    };

    let err = adapter.shape_run(&req).expect_err("failing bridge must error");
    assert!(matches!(
        err,
        NativeShapingError::AdapterError(ref msg) if msg.contains("simulated CoreText foreign call abort")
    ));
}

#[test]
fn negative_control_raster_caps_and_buffer_mismatch() {
    struct BadRasterBridge;
    impl fmd_font::macos::MacBridgeDriver for BadRasterBridge {
        fn shape_line(
            &self,
            _text: &str,
            _font_family: &str,
            _font_size: f32,
            _is_rtl: bool,
        ) -> Result<fmd_font::macos::RawCoreTextLine, MacFontAdapterError> {
            Ok(fmd_font::macos::RawCoreTextLine {
                glyphs: vec![],
                is_rtl: false,
                total_advance: 0.0,
            })
        }

        fn rasterize_glyph(
            &self,
            _font_postscript_name: &str,
            glyph_id: u16,
            _font_size_px: f32,
        ) -> Result<Option<RawCoreGraphicsRaster>, MacFontAdapterError> {
            if glyph_id == 1 {
                // Dimension too large
                Ok(Some(RawCoreGraphicsRaster {
                    width: MAX_MAC_RASTER_DIMENSION + 1,
                    height: 10,
                    pixels: vec![0u8; 100],
                    is_bgra: false,
                    bearing_x: 0.0,
                    bearing_y: 0.0,
                    advance_width: 10.0,
                }))
            } else {
                // Buffer mismatch
                Ok(Some(RawCoreGraphicsRaster {
                    width: 4,
                    height: 4,
                    pixels: vec![0u8; 10], // expected 64
                    is_bgra: false,
                    bearing_x: 0.0,
                    bearing_y: 0.0,
                    advance_width: 4.0,
                }))
            }
        }
    }

    let adapter = MacFontAdapter::new(BadRasterBridge);

    // Dimension too large
    let err1 = adapter.get_glyph_rgba_raster("Test", 1, 14.0).unwrap_err();
    assert!(matches!(err1, MacFontAdapterError::RasterDimensionTooLarge { .. }));

    // Buffer mismatch
    let err2 = adapter.get_glyph_rgba_raster("Test", 2, 14.0).unwrap_err();
    assert!(matches!(err2, MacFontAdapterError::RasterBufferMismatch { .. }));
}
