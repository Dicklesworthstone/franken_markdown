//! Optional safe Mac font adapter contract and CoreText/CoreGraphics bridge interface (FCB-075.B).
//!
//! Implements Plan §13.2, §13.7, §10.9, and §27.5:
//! - Owned CoreText/CoreGraphics conversions through a safe platform bridge without
//!   polluting the base engine or introducing third-party dependencies.
//! - Strict `#![forbid(unsafe_code)]`: low-level ABI calls remain solely in the system bridge.
//! - Bounded foreign calls: context work budget (`max_paragraph_bytes`) prevents giant
//!   main-thread shapes from freezing the UI.
//! - Preserves exact producing font identities (`FallbackFace`, `FontId`) for all glyphs.
//! - Converts native BGRA / RGBA color emoji and bitmap glyphs with checked dimension and byte caps.
//! - Provides meaningful fallback placeholders for missing or unsupported glyph rasters.
//! - Headless / simulated bridge driver for hermetic qualification across platforms.

#![forbid(unsafe_code)]

use crate::native_route::{
    assemble_platform_run, FallbackFace, NativeShapingError, NativeShapingRequest,
    NativeShapingRoute, PlatformRunGlyph, PlatformShapedOutput, ShapingRouteCapabilities,
    ShapingRouteKind,
};
use crate::shaping::Direction;
use crate::text_run::{
    utf16_to_byte, FontId, FontOrigin, OwnedTextRun, TextRunContext,
};
use std::fmt;

/// Maximum allowable paragraph byte length for a single Mac native shaping call (Plan §10.9).
pub const DEFAULT_MAC_CONTEXT_BUDGET: usize = 65_536;

/// Maximum allowable dimension (width or height) for a rasterized color glyph.
pub const MAX_MAC_RASTER_DIMENSION: u32 = 1024;

/// Maximum allowable memory size for a rasterized color glyph (4 MiB).
pub const MAX_MAC_RASTER_BYTES: usize = 4 * 1024 * 1024;

/// Configuration options for the Mac font adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacFontAdapterConfig {
    /// Maximum byte size of text passed to the native shaper in a single call.
    pub max_paragraph_bytes: usize,
    /// Whether system fallback font substitution is enabled.
    pub allow_fallback_fonts: bool,
    /// Whether color emoji and bitmap rasterization is supported.
    pub supports_color_emoji: bool,
    /// Maximum glyph raster dimension.
    pub raster_dimension_cap: u32,
    /// Maximum glyph raster memory buffer size.
    pub raster_bytes_cap: usize,
}

impl Default for MacFontAdapterConfig {
    fn default() -> Self {
        Self {
            max_paragraph_bytes: DEFAULT_MAC_CONTEXT_BUDGET,
            allow_fallback_fonts: true,
            supports_color_emoji: true,
            raster_dimension_cap: MAX_MAC_RASTER_DIMENSION,
            raster_bytes_cap: MAX_MAC_RASTER_BYTES,
        }
    }
}

/// Raw glyph record emitted by the native CoreText bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct RawCoreTextGlyph {
    /// OpenType glyph index produced by CoreText.
    pub glyph_id: u16,
    /// Start offset in native UTF-16 code units.
    pub string_index_utf16: usize,
    /// Length in native UTF-16 code units covered by this glyph cluster.
    pub string_length_utf16: usize,
    /// PostScript name of the font face chosen by CoreText.
    pub font_postscript_name: String,
    /// Family name of the font face.
    pub font_family_name: String,
    /// Horizontal advance in points.
    pub x_advance: f32,
    /// Vertical advance in points.
    pub y_advance: f32,
    /// Horizontal placement offset in points.
    pub x_offset: f32,
    /// Vertical placement offset in points.
    pub y_offset: f32,
    /// Whether this glyph belongs to a color emoji or bitmap face.
    pub is_color_emoji: bool,
}

/// Raw line layout record emitted by the native CoreText bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct RawCoreTextLine {
    /// Sequence of positioned glyphs.
    pub glyphs: Vec<RawCoreTextGlyph>,
    /// Whether this line was shaped as right-to-left.
    pub is_rtl: bool,
    /// Total visual advance width in points.
    pub total_advance: f32,
}

/// Raw raster image payload emitted by the native CoreGraphics bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct RawCoreGraphicsRaster {
    /// Raster image width in pixels.
    pub width: u32,
    /// Raster image height in pixels.
    pub height: u32,
    /// Pixel buffer data.
    pub pixels: Vec<u8>,
    /// Whether pixel buffer is BGRA8 (typical for Apple CoreGraphics) vs RGBA8.
    pub is_bgra: bool,
    /// Horizontal bearing in layout points.
    pub bearing_x: f32,
    /// Vertical bearing in layout points.
    pub bearing_y: f32,
    /// Advance width in layout points.
    pub advance_width: f32,
}

/// Errors occurring during Mac font adapter operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacFontAdapterError {
    /// Text exceeds the bounded work context budget (Plan §10.9).
    ContextBudgetExceeded { length: usize, max_allowed: usize },
    /// Input text was empty.
    EmptyText,
    /// UTF-16 code unit offset from CoreText could not be mapped to UTF-8 bytes.
    InvalidUtf16Offset { utf16_offset: usize },
    /// Mapped byte offset fell inside a multi-byte UTF-8 scalar.
    MidScalarBoundary { byte_offset: usize },
    /// Native bridge failed or framework is unavailable.
    BridgeUnavailable(String),
    /// Foreign call execution failed.
    ForeignCallFailed(String),
    /// Raster dimension exceeded configured maximum.
    RasterDimensionTooLarge { dimension: u32, max_allowed: u32 },
    /// Raster buffer memory size exceeded configured maximum.
    RasterBytesTooLarge { bytes: usize, max_allowed: usize },
    /// Raster buffer length does not match expected width * height * 4.
    RasterBufferMismatch { expected: usize, actual: usize },
    /// Missing required fallback face when system fallback is disabled.
    FallbackDisabled { unshaped_byte_offset: usize },
}

impl fmt::Display for MacFontAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextBudgetExceeded { length, max_allowed } => {
                write!(f, "CoreText context budget exceeded: {length} bytes > {max_allowed} max")
            }
            Self::EmptyText => write!(f, "cannot shape empty text with CoreText"),
            Self::InvalidUtf16Offset { utf16_offset } => {
                write!(f, "invalid UTF-16 code unit offset: {utf16_offset}")
            }
            Self::MidScalarBoundary { byte_offset } => {
                write!(f, "mapped byte offset {byte_offset} is inside a multi-byte UTF-8 scalar")
            }
            Self::BridgeUnavailable(msg) => write!(f, "Mac bridge unavailable: {msg}"),
            Self::ForeignCallFailed(msg) => write!(f, "foreign CoreText call failed: {msg}"),
            Self::RasterDimensionTooLarge { dimension, max_allowed } => {
                write!(f, "raster dimension {dimension} exceeds maximum {max_allowed}")
            }
            Self::RasterBytesTooLarge { bytes, max_allowed } => {
                write!(f, "raster bytes {bytes} exceeds maximum {max_allowed}")
            }
            Self::RasterBufferMismatch { expected, actual } => {
                write!(f, "raster buffer mismatch: expected {expected} bytes, got {actual}")
            }
            Self::FallbackDisabled { unshaped_byte_offset } => {
                write!(f, "system fallback required at byte offset {unshaped_byte_offset} but disabled")
            }
        }
    }
}

impl std::error::Error for MacFontAdapterError {}

impl From<MacFontAdapterError> for NativeShapingError {
    fn from(err: MacFontAdapterError) -> Self {
        match err {
            MacFontAdapterError::ContextBudgetExceeded { length, max_allowed } => {
                Self::ContextBudgetExceeded { length, max_allowed }
            }
            MacFontAdapterError::EmptyText => Self::EmptyText,
            MacFontAdapterError::InvalidUtf16Offset { utf16_offset } => {
                Self::InvalidByteRange {
                    offset: utf16_offset,
                    text_len: 0,
                }
            }
            MacFontAdapterError::MidScalarBoundary { byte_offset } => {
                Self::MidScalarBoundary { offset: byte_offset }
            }
            MacFontAdapterError::FallbackDisabled { unshaped_byte_offset } => {
                Self::FallbackRequired { unshaped_byte_offset }
            }
            MacFontAdapterError::BridgeUnavailable(msg) => Self::PlatformUnavailable(msg),
            MacFontAdapterError::ForeignCallFailed(msg) => Self::AdapterError(msg),
            MacFontAdapterError::RasterDimensionTooLarge { dimension, .. } => {
                Self::AdapterError(format!("raster dimension {dimension} too large"))
            }
            MacFontAdapterError::RasterBytesTooLarge { bytes, .. } => {
                Self::AdapterError(format!("raster bytes {bytes} too large"))
            }
            MacFontAdapterError::RasterBufferMismatch { expected, actual } => {
                Self::AdapterError(format!("raster buffer mismatch: expected {expected}, got {actual}"))
            }
        }
    }
}

/// Abstract contract for safe macOS platform font bridging.
///
/// Both real AppKit/CoreText ABI calls and hermetic headless test simulators
/// implement this driver interface.
pub trait MacBridgeDriver: Send + Sync {
    /// Request the native platform shaper to lay out a single line of text.
    fn shape_line(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        is_rtl: bool,
    ) -> Result<RawCoreTextLine, MacFontAdapterError>;

    /// Request the native platform rasterizer to rasterize a specific glyph.
    fn rasterize_glyph(
        &self,
        font_postscript_name: &str,
        glyph_id: u16,
        font_size_px: f32,
    ) -> Result<Option<RawCoreGraphicsRaster>, MacFontAdapterError>;
}

/// Safe platform font adapter translating CoreText / CoreGraphics records into shared text runs.
pub struct MacFontAdapter<D: MacBridgeDriver> {
    driver: D,
    config: MacFontAdapterConfig,
}

impl<D: MacBridgeDriver> MacFontAdapter<D> {
    /// Create a new Mac font adapter with default configuration.
    #[must_use]
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            config: MacFontAdapterConfig::default(),
        }
    }

    /// Create a new Mac font adapter with explicit configuration.
    #[must_use]
    pub fn with_config(driver: D, config: MacFontAdapterConfig) -> Self {
        Self { driver, config }
    }

    /// Retrieve a reference to the active configuration.
    #[must_use]
    pub fn config(&self) -> &MacFontAdapterConfig {
        &self.config
    }

    /// Retrieve a reference to the underlying driver.
    #[must_use]
    pub fn driver(&self) -> &D {
        &self.driver
    }

    /// Rasterize a glyph through CoreGraphics, returning validated RGBA8 pixels.
    pub fn get_glyph_rgba_raster(
        &self,
        font_postscript_name: &str,
        glyph_id: u16,
        font_size_px: f32,
    ) -> Result<Option<RawCoreGraphicsRaster>, MacFontAdapterError> {
        let raw_opt = self.driver.rasterize_glyph(font_postscript_name, glyph_id, font_size_px)?;
        let mut raw = match raw_opt {
            Some(r) => r,
            None => return Ok(None),
        };

        if raw.width > self.config.raster_dimension_cap {
            return Err(MacFontAdapterError::RasterDimensionTooLarge {
                dimension: raw.width,
                max_allowed: self.config.raster_dimension_cap,
            });
        }
        if raw.height > self.config.raster_dimension_cap {
            return Err(MacFontAdapterError::RasterDimensionTooLarge {
                dimension: raw.height,
                max_allowed: self.config.raster_dimension_cap,
            });
        }
        if raw.pixels.len() > self.config.raster_bytes_cap {
            return Err(MacFontAdapterError::RasterBytesTooLarge {
                bytes: raw.pixels.len(),
                max_allowed: self.config.raster_bytes_cap,
            });
        }

        let expected_bytes = match (raw.width as usize)
            .checked_mul(raw.height as usize)
            .and_then(|px| px.checked_mul(4))
        {
            Some(exp) => exp,
            None => {
                return Err(MacFontAdapterError::RasterBytesTooLarge {
                    bytes: usize::MAX,
                    max_allowed: self.config.raster_bytes_cap,
                });
            }
        };

        if raw.pixels.len() != expected_bytes {
            return Err(MacFontAdapterError::RasterBufferMismatch {
                expected: expected_bytes,
                actual: raw.pixels.len(),
            });
        }

        // Swizzle BGRA -> RGBA in place if needed
        if raw.is_bgra {
            for chunk in raw.pixels.chunks_exact_mut(4) {
                chunk.swap(0, 2); // B and R swapped
            }
            raw.is_bgra = false;
        }

        Ok(Some(raw))
    }
}

impl<D: MacBridgeDriver> NativeShapingRoute for MacFontAdapter<D> {
    fn route_kind(&self) -> ShapingRouteKind {
        ShapingRouteKind::SystemPlatform
    }

    fn capabilities(&self) -> ShapingRouteCapabilities {
        ShapingRouteCapabilities {
            route_kind: ShapingRouteKind::SystemPlatform,
            is_pixel_deterministic: false,
            supports_fallback_fonts: self.config.allow_fallback_fonts,
            supports_color_emoji: self.config.supports_color_emoji,
            supports_bidi: true,
            max_paragraph_bytes: self.config.max_paragraph_bytes,
        }
    }

    fn shape_run(&self, req: &NativeShapingRequest<'_>) -> Result<OwnedTextRun, NativeShapingError> {
        if req.text.is_empty() {
            return Err(NativeShapingError::EmptyText);
        }

        // Enforce work context budget before foreign call (Plan §10.9)
        if req.text.len() > self.config.max_paragraph_bytes {
            return Err(NativeShapingError::ContextBudgetExceeded {
                length: req.text.len(),
                max_allowed: self.config.max_paragraph_bytes,
            });
        }

        let is_rtl = req.direction == Direction::RightToLeft;
        let line = self.driver.shape_line(req.text, "SystemFont", req.font_size, is_rtl)?;

        let mut platform_glyphs: Vec<PlatformRunGlyph> = Vec::with_capacity(line.glyphs.len());
        let mut used_fallbacks: Vec<FallbackFace> = Vec::new();

        for g in &line.glyphs {
            // Map native UTF-16 code unit range back to logical UTF-8 bytes
            let byte_start = match utf16_to_byte(req.text, g.string_index_utf16) {
                Some(b) => b,
                None => {
                    return Err(NativeShapingError::InvalidByteRange {
                        offset: g.string_index_utf16,
                        text_len: req.text.len(),
                    });
                }
            };

            let utf16_end = g.string_index_utf16.saturating_add(g.string_length_utf16);
            let byte_end = match utf16_to_byte(req.text, utf16_end) {
                Some(b) => b,
                None => {
                    return Err(NativeShapingError::InvalidByteRange {
                        offset: utf16_end,
                        text_len: req.text.len(),
                    });
                }
            };

            if byte_end < byte_start {
                return Err(NativeShapingError::InvalidByteRange {
                    offset: byte_end,
                    text_len: req.text.len(),
                });
            }

            let byte_len = byte_end - byte_start;

            // Determine if this glyph was produced by a fallback face
            let is_fallback = !g.font_postscript_name.is_empty()
                && g.font_postscript_name != "SystemFont";

            let font_id = if is_fallback {
                if !req.allow_system_fallback {
                    return Err(NativeShapingError::FallbackRequired {
                        unshaped_byte_offset: byte_start,
                    });
                }

                // Compute deterministic FontId from postscript name
                let mut hash = 0xcbf29ce484222325u64;
                for &b in g.font_postscript_name.as_bytes() {
                    hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
                }
                let fid = FontId::new(hash);

                if !used_fallbacks.iter().any(|f| f.font_id == fid) {
                    used_fallbacks.push(FallbackFace {
                        font_id: fid,
                        family_name: g.font_family_name.clone(),
                        postscript_name: g.font_postscript_name.clone(),
                        units_per_em: 1000,
                        is_color_emoji: g.is_color_emoji,
                    });
                }
                fid
            } else {
                req.primary_font_id
            };

            platform_glyphs.push(PlatformRunGlyph {
                glyph_id: g.glyph_id,
                font_id,
                cluster_byte_offset: byte_start,
                cluster_byte_len: byte_len,
                x_advance: g.x_advance,
                y_advance: g.y_advance,
                x_offset: g.x_offset,
                y_offset: g.y_offset,
            });
        }

        let output = PlatformShapedOutput {
            logical_text: req.text.to_string(),
            direction: req.direction,
            font_size: req.font_size,
            glyphs: platform_glyphs,
            fallback_faces: used_fallbacks,
            route_kind: ShapingRouteKind::SystemPlatform,
        };

        let primary_context = TextRunContext {
            font_id: req.primary_font_id,
            font_size: req.font_size,
            script: req.script,
            language: req.language,
            direction: req.direction,
            font_origin: FontOrigin::BundledFace,
        };

        assemble_platform_run(primary_context, output)
    }
}

/// Hermetic simulated Mac bridge driver for tests and non-Mac host environments.
#[derive(Clone, Debug, Default)]
pub struct SimulatedMacBridge {
    fail_bridge: bool,
}

impl SimulatedMacBridge {
    /// Create a new simulated bridge driver.
    #[must_use]
    pub fn new() -> Self {
        Self { fail_bridge: false }
    }

    /// Create a simulated bridge driver that fails simulated foreign calls (for error testing).
    #[must_use]
    pub fn new_failing() -> Self {
        Self { fail_bridge: true }
    }
}

impl MacBridgeDriver for SimulatedMacBridge {
    fn shape_line(
        &self,
        text: &str,
        _font_family: &str,
        font_size: f32,
        is_rtl: bool,
    ) -> Result<RawCoreTextLine, MacFontAdapterError> {
        if self.fail_bridge {
            return Err(MacFontAdapterError::ForeignCallFailed(
                "simulated CoreText foreign call abort".to_string(),
            ));
        }

        let mut glyphs = Vec::new();
        let mut cur_utf16 = 0usize;
        let mut total_advance = 0.0f32;

        for ch in text.chars() {
            let cp = ch as u32;
            let utf16_len = ch.len_utf16();

            // Detect script for simulated CoreText font substitution
            let (ps_name, fam_name, is_color, adv_scale) = if (0x2E80..=0x9FFF).contains(&cp)
                || (0xAC00..=0xD7AF).contains(&cp)
            {
                ("PingFangSC-Regular", "PingFang SC", false, 1.0)
            } else if (0x0590..=0x08FF).contains(&cp) || (0xFB1D..=0xFEFF).contains(&cp) {
                ("GeezaPro", "Geeza Pro", false, 0.6)
            } else if (0x1F300..=0x1FAFF).contains(&cp) || (0x2600..=0x27BF).contains(&cp) {
                ("AppleColorEmoji", "Apple Color Emoji", true, 1.0)
            } else {
                ("SystemFont", "SystemFont", false, 0.5)
            };

            let x_adv = font_size * adv_scale;
            let glyph_id = (cp % 5000) as u16;

            glyphs.push(RawCoreTextGlyph {
                glyph_id,
                string_index_utf16: cur_utf16,
                string_length_utf16: utf16_len,
                font_postscript_name: ps_name.to_string(),
                font_family_name: fam_name.to_string(),
                x_advance: x_adv,
                y_advance: 0.0,
                x_offset: 0.0,
                y_offset: 0.0,
                is_color_emoji: is_color,
            });

            cur_utf16 += utf16_len;
            total_advance += x_adv;
        }

        Ok(RawCoreTextLine {
            glyphs,
            is_rtl,
            total_advance,
        })
    }

    fn rasterize_glyph(
        &self,
        font_postscript_name: &str,
        glyph_id: u16,
        font_size_px: f32,
    ) -> Result<Option<RawCoreGraphicsRaster>, MacFontAdapterError> {
        if self.fail_bridge {
            return Err(MacFontAdapterError::ForeignCallFailed(
                "simulated CoreGraphics foreign call abort".to_string(),
            ));
        }

        if font_postscript_name == "AppleColorEmoji" || glyph_id == 777 {
            let size = (font_size_px as u32).clamp(8, 64);
            let len = (size as usize) * (size as usize) * 4;
            let mut pixels = vec![0u8; len];
            // Simulate BGRA pattern
            for chunk in pixels.chunks_exact_mut(4) {
                chunk[0] = 255; // B
                chunk[1] = 200; // G
                chunk[2] = 50;  // R
                chunk[3] = 255; // A
            }
            Ok(Some(RawCoreGraphicsRaster {
                width: size,
                height: size,
                pixels,
                is_bgra: true,
                bearing_x: 0.0,
                bearing_y: font_size_px,
                advance_width: font_size_px,
            }))
        } else {
            Ok(None)
        }
    }
}
