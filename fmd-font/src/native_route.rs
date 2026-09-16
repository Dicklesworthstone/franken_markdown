//! Safe native shaping route contract and platform adapter interface (FCB-016.B).
//!
//! Implements Plan §13.2, §13.7, §10.9, and §27.5:
//! - Optional native platform adapter translates owned platform results into the
//!   shared [`OwnedTextRun`] representation without polluting the base engine or
//!   introducing foreign runtime dependencies.
//! - Capability records distinguish deterministic bundled-face runs from
//!   system-shaped fallback runs (`is_pixel_deterministic`).
//! - Preserves exact fallback font identity: a native glyph ID has meaning only
//!   with the actual fallback font that produced it (Plan §13.7).
//! - Bounded preparation pass enforcing context limits (`max_paragraph_bytes`),
//!   preventing unbounded foreign calls on interactive threads (Plan §10.9).
//! - Headless contract builds and runs without AppKit/CoreText runtimes.

use crate::shaping::Direction;
use crate::text_run::{
    byte_to_utf16, FontId, FontOrigin, OwnedTextRun, RunGlyph, TextCluster, TextRunContext,
};
use std::fmt;
use std::ops::Range;

/// The architectural kind of shaping route.
///
/// Plan §13.2: "The capability record distinguishes deterministic bundled-face
/// runs from system-shaped fallback runs. Full cross-machine pixel determinism
/// is not promised for the latter."
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapingRouteKind {
    /// Internal deterministic shaper using bundled font faces.
    BundledShaper,
    /// Native system platform route (CoreText on macOS via safe system bridge).
    SystemPlatform,
    /// Headless simulated platform route for testing and portable qualification.
    SimulatedPlatform,
}

impl ShapingRouteKind {
    /// Whether this route kind guarantees cross-machine bit-identical pixel results.
    #[must_use]
    pub const fn is_pixel_deterministic(self) -> bool {
        matches!(self, Self::BundledShaper)
    }
}

/// Declared capabilities and operating boundaries of a shaping route.
///
/// Plan §10.9 & §13.2: Honest capability reporting and context bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapingRouteCapabilities {
    /// Route classification.
    pub route_kind: ShapingRouteKind,
    /// Whether runs from this route are bit-identical across machines/OS versions.
    pub is_pixel_deterministic: bool,
    /// Whether this route can resolve and substitute platform fallback fonts.
    pub supports_fallback_fonts: bool,
    /// Whether this route supports color emoji or bitmap glyphs.
    pub supports_color_emoji: bool,
    /// Whether this route handles bidirectional text (LTR and RTL).
    pub supports_bidi: bool,
    /// Maximum byte length allowed for a single shaping invocation (Plan §10.9).
    ///
    /// Exceeding this budget yields [`NativeShapingError::ContextBudgetExceeded`].
    pub max_paragraph_bytes: usize,
}

impl Default for ShapingRouteCapabilities {
    fn default() -> Self {
        Self {
            route_kind: ShapingRouteKind::SimulatedPlatform,
            is_pixel_deterministic: false,
            supports_fallback_fonts: true,
            supports_color_emoji: true,
            supports_bidi: true,
            max_paragraph_bytes: 64 * 1024, // 64 KiB context boundary
        }
    }
}

/// Fallback font identity metadata retained from a native system platform pass.
///
/// Plan §13.7: "A native glyph ID has meaning only with the actual fallback font/run
/// that produced it. The native adapter retains or converts that font identity..."
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FallbackFace {
    /// Unique deterministic identifier for the fallback font.
    pub font_id: FontId,
    /// Font family name (e.g. `"PingFang SC"`, `"Apple Color Emoji"`).
    pub family_name: String,
    /// PostScript name of the font face.
    pub postscript_name: String,
    /// Font design units per em.
    pub units_per_em: u16,
    /// Whether this face is a color emoji or bitmap font.
    pub is_color_emoji: bool,
}

/// A raw glyph produced by a platform shaper before assembly into a run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlatformRunGlyph {
    /// Platform glyph index.
    pub glyph_id: u16,
    /// Identifier of the specific font face (primary or fallback) that produced this glyph.
    pub font_id: FontId,
    /// Start offset in the original logical UTF-8 bytes.
    pub cluster_byte_offset: usize,
    /// Length in original logical UTF-8 bytes covered by this cluster.
    pub cluster_byte_len: usize,
    /// Horizontal advance in layout points.
    pub x_advance: f32,
    /// Vertical advance in layout points.
    pub y_advance: f32,
    /// Horizontal placement offset in layout points.
    pub x_offset: f32,
    /// Vertical placement offset in layout points.
    pub y_offset: f32,
}

/// Raw output produced by a native platform shaper or safe bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct PlatformShapedOutput {
    /// Original logical text shaped by the platform.
    pub logical_text: String,
    /// Direction of the shaped text.
    pub direction: Direction,
    /// Font size in points.
    pub font_size: f32,
    /// Sequence of platform-positioned glyphs.
    pub glyphs: Vec<PlatformRunGlyph>,
    /// Fallback font faces utilized during shaping (empty if primary font sufficed).
    pub fallback_faces: Vec<FallbackFace>,
    /// Route kind that generated this output.
    pub route_kind: ShapingRouteKind,
}

/// Request parameters submitted to a native shaping route.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeShapingRequest<'a> {
    /// The logical text slice to shape.
    pub text: &'a str,
    /// The requested primary font identity.
    pub primary_font_id: FontId,
    /// Layout font size in points.
    pub font_size: f32,
    /// Text direction.
    pub direction: Direction,
    /// OpenType script tag (e.g. `*b"latn"`, `*b"hani"`).
    pub script: [u8; 4],
    /// OpenType language tag (e.g. `*b"dflt"`).
    pub language: [u8; 4],
    /// Whether the shaper is permitted to substitute platform fallback faces
    /// for characters missing from the primary font.
    pub allow_system_fallback: bool,
}

/// Errors arising from native shaping operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeShapingError {
    /// The input text exceeds the maximum allowable context budget for a single call (Plan §10.9).
    ContextBudgetExceeded {
        /// Actual byte length of the input.
        length: usize,
        /// Maximum allowed bytes under the active work budget.
        max_allowed: usize,
    },
    /// The input text was empty.
    EmptyText,
    /// An offset was out of bounds for the input text.
    InvalidByteRange {
        /// Out of bounds offset.
        offset: usize,
        /// Total text length.
        text_len: usize,
    },
    /// An offset fell within a UTF-8 scalar sequence rather than on a scalar boundary.
    MidScalarBoundary {
        /// Invalid offset.
        offset: usize,
    },
    /// The text contains characters missing from the primary font, but system fallback was disabled.
    FallbackRequired {
        /// Offset of the unshaped character in the original bytes.
        unshaped_byte_offset: usize,
    },
    /// The native platform bridge or required system framework is unavailable.
    PlatformUnavailable(String),
    /// The platform shaper reported an internal error during layout.
    AdapterError(String),
}

impl fmt::Display for NativeShapingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextBudgetExceeded { length, max_allowed } => {
                write!(
                    f,
                    "text context length ({length} bytes) exceeds maximum budget ({max_allowed} bytes)"
                )
            }
            Self::EmptyText => write!(f, "cannot shape empty text"),
            Self::InvalidByteRange { offset, text_len } => {
                write!(
                    f,
                    "byte offset {offset} is out of bounds for text of length {text_len}"
                )
            }
            Self::MidScalarBoundary { offset } => {
                write!(f, "byte offset {offset} is inside a multi-byte UTF-8 scalar")
            }
            Self::FallbackRequired { unshaped_byte_offset } => {
                write!(
                    f,
                    "system fallback required at byte offset {unshaped_byte_offset} but fallback was disabled"
                )
            }
            Self::PlatformUnavailable(msg) => write!(f, "platform shaper unavailable: {msg}"),
            Self::AdapterError(msg) => write!(f, "platform adapter error: {msg}"),
        }
    }
}

impl std::error::Error for NativeShapingError {}

/// Safely assemble raw platform output into an authoritative [`OwnedTextRun`].
///
/// Validates cluster boundaries, maps UTF-16 code unit positions, preserves fallback
/// font identities per glyph and cluster, and establishes accurate hit-testing metrics.
pub fn assemble_platform_run(
    primary_context: TextRunContext,
    output: PlatformShapedOutput,
) -> Result<OwnedTextRun, NativeShapingError> {
    if output.logical_text.is_empty() {
        return Err(NativeShapingError::EmptyText);
    }

    let text = &output.logical_text;
    let text_len = text.len();
    let is_rtl = output.direction == Direction::RightToLeft;

    let mut clusters: Vec<TextCluster> = Vec::new();
    let mut run_glyphs: Vec<RunGlyph> = Vec::with_capacity(output.glyphs.len());

    let mut cur_x = 0.0f32;
    let mut i = 0;

    while i < output.glyphs.len() {
        let first = &output.glyphs[i];
        let byte_start = first.cluster_byte_offset;
        let byte_len = first.cluster_byte_len;
        let byte_end = byte_start.saturating_add(byte_len);

        // Validation 1: Range bounds
        if byte_end > text_len {
            return Err(NativeShapingError::InvalidByteRange {
                offset: byte_end,
                text_len,
            });
        }

        // Validation 2: Scalar boundary check
        if !text.is_char_boundary(byte_start) {
            return Err(NativeShapingError::MidScalarBoundary { offset: byte_start });
        }
        if !text.is_char_boundary(byte_end) {
            return Err(NativeShapingError::MidScalarBoundary { offset: byte_end });
        }

        let cluster_start_glyph = run_glyphs.len();
        let cluster_x_start = cur_x;
        let cluster_font_id = first.font_id;

        // Group consecutive glyphs sharing this cluster byte range
        while i < output.glyphs.len() {
            let g = &output.glyphs[i];
            if g.cluster_byte_offset != byte_start || g.cluster_byte_len != byte_len {
                break;
            }

            run_glyphs.push(RunGlyph {
                glyph_id: g.glyph_id,
                font_id: g.font_id,
                cluster_index: clusters.len(),
                x_advance: g.x_advance,
                y_advance: g.y_advance,
                x_offset: g.x_offset,
                y_offset: g.y_offset,
            });

            cur_x += g.x_advance;
            i += 1;
        }

        let cluster_x_end = cur_x;

        let utf16_start = match byte_to_utf16(text, byte_start) {
            Some(o) => o,
            None => return Err(NativeShapingError::MidScalarBoundary { offset: byte_start }),
        };
        let utf16_end = match byte_to_utf16(text, byte_end) {
            Some(o) => o,
            None => return Err(NativeShapingError::MidScalarBoundary { offset: byte_end }),
        };

        clusters.push(TextCluster {
            cluster_index: clusters.len(),
            byte_range: byte_start..byte_end,
            utf16_range: utf16_start..utf16_end,
            glyph_range: cluster_start_glyph..run_glyphs.len(),
            x_start: cluster_x_start,
            x_end: cluster_x_end,
            font_id: cluster_font_id,
        });
    }

    let total_advance = cur_x;

    // Flip visual coordinate bounds if RTL
    if is_rtl {
        for cluster in &mut clusters {
            let old_start = cluster.x_start;
            let old_end = cluster.x_end;
            cluster.x_start = (total_advance - old_end).max(0.0);
            cluster.x_end = (total_advance - old_start).max(0.0);
        }
    }

    // Honest origin classification: if any fallback face was involved, mark as SystemFallbackFace
    let font_origin = if output.fallback_faces.is_empty() {
        primary_context.font_origin
    } else {
        FontOrigin::SystemFallbackFace
    };

    let context = TextRunContext {
        font_id: primary_context.font_id,
        font_size: output.font_size,
        script: primary_context.script,
        language: primary_context.language,
        direction: output.direction,
        font_origin,
    };

    Ok(OwnedTextRun {
        context,
        logical_text: output.logical_text,
        clusters,
        glyphs: run_glyphs,
        total_advance,
    })
}

/// Abstract contract for safe native shaping routes.
///
/// Both real system bridges (CoreText on macOS) and headless simulated routes
/// implement this trait.
pub trait NativeShapingRoute: Send + Sync {
    /// Return the route classification.
    fn route_kind(&self) -> ShapingRouteKind;

    /// Return declared capability parameters and operational limits.
    fn capabilities(&self) -> ShapingRouteCapabilities;

    /// Shape a slice of text into an owned text run.
    fn shape_run(&self, req: &NativeShapingRequest<'_>) -> Result<OwnedTextRun, NativeShapingError>;
}

/// Headless simulated native shaping route for test qualification.
///
/// Implements [`NativeShapingRoute`] entirely in safe Rust with zero foreign or
/// platform runtime dependencies, enabling full qualification of fallback font
/// preservation, context limits, and cluster mapping.
pub struct SimulatedNativeRoute {
    capabilities: ShapingRouteCapabilities,
    fallback_rules: Vec<SimulatedFallbackRule>,
}

/// Rule defining simulated fallback behavior for a Unicode scalar range.
#[derive(Clone, Debug)]
pub struct SimulatedFallbackRule {
    /// Unicode scalar range that triggers this fallback rule.
    pub range: Range<u32>,
    /// Fallback font identity.
    pub fallback_face: FallbackFace,
    /// Default glyph advance for this fallback font in em units.
    pub advance_per_em: f32,
}

impl SimulatedNativeRoute {
    /// Create a new simulated native route with default capabilities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            capabilities: ShapingRouteCapabilities::default(),
            fallback_rules: Vec::new(),
        }
    }

    /// Create a simulated native route with explicit capabilities.
    #[must_use]
    pub fn with_capabilities(capabilities: ShapingRouteCapabilities) -> Self {
        Self {
            capabilities,
            fallback_rules: Vec::new(),
        }
    }

    /// Register a fallback rule for characters falling within `range`.
    pub fn register_fallback(&mut self, rule: SimulatedFallbackRule) {
        self.fallback_rules.push(rule);
    }
}

impl Default for SimulatedNativeRoute {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeShapingRoute for SimulatedNativeRoute {
    fn route_kind(&self) -> ShapingRouteKind {
        self.capabilities.route_kind
    }

    fn capabilities(&self) -> ShapingRouteCapabilities {
        self.capabilities.clone()
    }

    fn shape_run(&self, req: &NativeShapingRequest<'_>) -> Result<OwnedTextRun, NativeShapingError> {
        // Enforce empty text guard
        if req.text.is_empty() {
            return Err(NativeShapingError::EmptyText);
        }

        // Enforce context work budget (Plan §10.9)
        if req.text.len() > self.capabilities.max_paragraph_bytes {
            return Err(NativeShapingError::ContextBudgetExceeded {
                length: req.text.len(),
                max_allowed: self.capabilities.max_paragraph_bytes,
            });
        }

        let mut platform_glyphs: Vec<PlatformRunGlyph> = Vec::new();
        let mut used_fallbacks: Vec<FallbackFace> = Vec::new();

        let primary_advance = (req.font_size * 0.5).max(1.0); // 0.5 em default advance

        for (byte_idx, ch) in req.text.char_indices() {
            let cp = ch as u32;
            let ch_len = ch.len_utf8();

            // Check if this character matches any registered fallback rule
            let matched_fallback = self
                .fallback_rules
                .iter()
                .find(|r| r.range.contains(&cp));

            if let Some(rule) = matched_fallback {
                if !req.allow_system_fallback {
                    return Err(NativeShapingError::FallbackRequired {
                        unshaped_byte_offset: byte_idx,
                    });
                }

                if !used_fallbacks.iter().any(|f| f.font_id == rule.fallback_face.font_id) {
                    used_fallbacks.push(rule.fallback_face.clone());
                }

                let adv = req.font_size * rule.advance_per_em;
                let glyph_id = (cp % 1000) as u16;

                platform_glyphs.push(PlatformRunGlyph {
                    glyph_id,
                    font_id: rule.fallback_face.font_id,
                    cluster_byte_offset: byte_idx,
                    cluster_byte_len: ch_len,
                    x_advance: adv,
                    y_advance: 0.0,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
            } else {
                // Primary font mapping
                let glyph_id = (cp % 500) as u16;
                platform_glyphs.push(PlatformRunGlyph {
                    glyph_id,
                    font_id: req.primary_font_id,
                    cluster_byte_offset: byte_idx,
                    cluster_byte_len: ch_len,
                    x_advance: primary_advance,
                    y_advance: 0.0,
                    x_offset: 0.0,
                    y_offset: 0.0,
                });
            }
        }

        let output = PlatformShapedOutput {
            logical_text: req.text.to_string(),
            direction: req.direction,
            font_size: req.font_size,
            glyphs: platform_glyphs,
            fallback_faces: used_fallbacks,
            route_kind: self.capabilities.route_kind,
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
