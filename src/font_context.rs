//! Shared context fallback-font and raster extensions (FCB-075.A).
//!
//! Platform-neutral data model for font run identity, fallback chains,
//! glyph clustering, bounded shaping-context checkpoints, color/bitmap
//! raster resources, and exact logical selection.
//! No unsafe code, no external dependencies — the base engine remains
//! platform-neutral; native adapters (FCB-075.B) consume this API.

#![forbid(unsafe_code)]

use std::ops::Range;

/// Maximum checkpoint size in bytes.
pub const MAX_CHECKPOINT_BYTES: usize = 4096;
/// Checkpoint format version 1 (legacy).
pub const CHECKPOINT_VERSION_V1: u32 = 1;
/// Checkpoint format version 2 (includes CJK, tab, ligature, and count flags).
pub const CHECKPOINT_VERSION_V2: u32 = 2;
/// Current checkpoint format version.
pub const CHECKPOINT_VERSION: u32 = CHECKPOINT_VERSION_V2;

/// Maximum dimension (width or height) for a color glyph in pixels.
pub const MAX_COLOR_GLYPH_DIMENSION: u32 = 1024;
/// Maximum buffer byte size for a single color glyph raster (4 MiB).
pub const MAX_COLOR_GLYPH_BYTES: usize = 4 * 1024 * 1024;
/// Maximum dimension (width or height) for a monochrome bitmap glyph.
pub const MAX_BITMAP_GLYPH_DIMENSION: u32 = 2048;
/// Maximum buffer byte size for a monochrome bitmap glyph (1 MiB).
pub const MAX_BITMAP_GLYPH_BYTES: usize = 1024 * 1024;

/// Identity of a font run: family, style, pixel size, and content hash identity.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FontRunIdentity {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub size_px: u16,
}

impl FontRunIdentity {
    /// Create a font run identity.
    pub fn new(family: &str, bold: bool, italic: bool, size_px: u16) -> Self {
        Self {
            family: family.to_owned(),
            bold,
            italic,
            size_px,
        }
    }

    /// Deterministic FNV-1a content hash identifying this font run configuration.
    #[must_use]
    pub fn font_id(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for &b in self.family.as_bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        hash = (hash ^ u64::from(self.bold as u8)).wrapping_mul(0x100000001b3);
        hash = (hash ^ u64::from(self.italic as u8)).wrapping_mul(0x100000001b3);
        hash = (hash ^ u64::from(self.size_px)).wrapping_mul(0x100000001b3);
        hash
    }
}

/// Ordered fallback chain: primary font + fallbacks for missing glyphs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackChain {
    pub primary: FontRunIdentity,
    pub fallbacks: Vec<FontRunIdentity>,
}

impl FallbackChain {
    /// Create a chain with only the primary font.
    pub fn single(primary: FontRunIdentity) -> Self {
        Self {
            primary,
            fallbacks: Vec::new(),
        }
    }

    /// Append a fallback font.
    pub fn push_fallback(&mut self, fallback: FontRunIdentity) {
        self.fallbacks.push(fallback);
    }

    /// Resolve the font for a codepoint by walking the chain in order.
    ///
    /// `coverage` is a predicate that returns `true` when a font covers a
    /// codepoint. Returns `None` when no font in the chain covers it.
    pub fn resolve(
        &self,
        codepoint: char,
        coverage: &dyn Fn(&FontRunIdentity, char) -> bool,
    ) -> Option<&FontRunIdentity> {
        if coverage(&self.primary, codepoint) {
            return Some(&self.primary);
        }
        self.fallbacks
            .iter()
            .find(|f| coverage(f, codepoint))
    }
}

/// A glyph cluster: one or more codepoints that form a single visual unit.
///
/// Clusters are used for CJK ideographs, RTL joins, combining sequences,
/// emoji ZWJ sequences, tabs, and ligatures. The `start`/`end` byte offsets
/// are into the original source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlyphCluster {
    /// Inclusive start byte offset in the source text.
    pub start: usize,
    /// Exclusive end byte offset in the source text.
    pub end: usize,
    /// Whether this cluster is right-to-left.
    pub is_rtl: bool,
    /// Whether this cluster is an emoji or color glyph.
    pub is_emoji: bool,
    /// Whether this cluster is a CJK ideograph or syllable.
    pub is_cjk: bool,
    /// Whether this cluster represents a horizontal tab stop.
    pub is_tab: bool,
    /// Whether this cluster is a typographic or programming ligature.
    pub is_ligature: bool,
    /// Number of Unicode code points comprising this cluster.
    pub char_count: usize,
}

impl GlyphCluster {
    /// Byte length of this cluster.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// Classify a character for cluster detection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharClass {
    /// Standard Latin/ASCII letter or digit.
    Alphanumeric,
    /// CJK ideograph or Hangul syllable.
    Cjk,
    /// Right-to-left character (Arabic, Hebrew, etc.).
    Rtl,
    /// Emoji (includes ZWJ sequences and skin tone modifiers).
    Emoji,
    /// Combining diacritical mark (attaches to previous cluster).
    Combining,
    /// Horizontal tab character.
    Tab,
    /// Whitespace character other than tab.
    Whitespace,
    /// Anything else (punctuation, symbols, etc.).
    Other,
}

/// Classify a character into a cluster-relevant category.
#[must_use]
pub fn classify_char(c: char) -> CharClass {
    let cp = c as u32;
    if c == '\t' {
        CharClass::Tab
    } else if c.is_whitespace() {
        CharClass::Whitespace
    } else if (0x1F300..=0x1F9FF).contains(&cp)
        || (0x1FA00..=0x1FAFF).contains(&cp)
        || (0x2600..=0x27BF).contains(&cp)
        || cp == 0x200D
        || (0x1F1E6..=0x1F1FF).contains(&cp)
    {
        // Emoji and ZWJ.
        CharClass::Emoji
    } else if (0x0300..=0x036F).contains(&cp)
        || (0x1AB0..=0x1AFF).contains(&cp)
        || (0x1DC0..=0x1DFF).contains(&cp)
        || (0x20D0..=0x20FF).contains(&cp)
        || (0xFE20..=0xFE2F).contains(&cp)
    {
        // Combining diacritical marks.
        CharClass::Combining
    } else if (0x0590..=0x05FF).contains(&cp)
        || (0x0600..=0x06FF).contains(&cp)
        || (0x0700..=0x074F).contains(&cp)
        || (0x0750..=0x077F).contains(&cp)
        || (0x08A0..=0x08FF).contains(&cp)
        || (0xFB1D..=0xFB4F).contains(&cp)
        || (0xFB50..=0xFDFF).contains(&cp)
        || (0xFE70..=0xFEFF).contains(&cp)
    {
        // RTL scripts: Hebrew, Arabic, Syriac, etc.
        CharClass::Rtl
    } else if (0x2E80..=0x9FFF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0x20000..=0x2FA1F).contains(&cp)
    {
        // CJK: CJK Unified Ideographs, Hangul, Kana, Compatibility Ideographs.
        CharClass::Cjk
    } else if c.is_alphanumeric() {
        CharClass::Alphanumeric
    } else {
        CharClass::Other
    }
}

/// Check if a byte substring starting at `offset` matches a known ligature prefix.
fn match_ligature_len(remainder: &str) -> Option<usize> {
    const LIGATURES: &[&str] = &[
        "===", "!==", "...",
        "ffi", "ffl",
        "=>", "->", "==", "!=", "<=", ">=",
        "::", "<-", "<->", "<=>",
        "ff", "fi", "fl",
    ];
    for &lig in LIGATURES {
        if remainder.starts_with(lig) {
            return Some(lig.len());
        }
    }
    None
}

/// Segment text into glyph clusters for shaping.
///
/// Combining marks attach to the previous cluster. Emoji ZWJ sequences form
/// single clusters. RTL characters form clusters that are marked `is_rtl`.
/// CJK characters, tabs, and ligatures are explicitly tagged.
pub fn segment_clusters(text: &str) -> Vec<GlyphCluster> {
    let mut clusters = Vec::new();
    let mut byte_idx = 0usize;

    while byte_idx < text.len() {
        let remainder = match text.get(byte_idx..) {
            Some(r) => r,
            None => break,
        };

        // 1. Check for programming/typographical ligatures
        if let Some(lig_len) = match_ligature_len(remainder) {
            let char_count = remainder.get(..lig_len).map(|s| s.chars().count()).unwrap_or(1);
            clusters.push(GlyphCluster {
                start: byte_idx,
                end: byte_idx + lig_len,
                is_rtl: false,
                is_emoji: false,
                is_cjk: false,
                is_tab: false,
                is_ligature: true,
                char_count,
            });
            byte_idx += lig_len;
            continue;
        }

        let ch = match remainder.chars().next() {
            Some(c) => c,
            None => break,
        };
        let ch_len = ch.len_utf8();
        let class = classify_char(ch);

        match class {
            CharClass::Tab => {
                clusters.push(GlyphCluster {
                    start: byte_idx,
                    end: byte_idx + ch_len,
                    is_rtl: false,
                    is_emoji: false,
                    is_cjk: false,
                    is_tab: true,
                    is_ligature: false,
                    char_count: 1,
                });
                byte_idx += ch_len;
            }
            CharClass::Cjk => {
                // Each CJK ideograph/syllable forms an atomic cluster
                clusters.push(GlyphCluster {
                    start: byte_idx,
                    end: byte_idx + ch_len,
                    is_rtl: false,
                    is_emoji: false,
                    is_cjk: true,
                    is_tab: false,
                    is_ligature: false,
                    char_count: 1,
                });
                byte_idx += ch_len;
            }
            CharClass::Emoji => {
                // Group contiguous emoji characters, ZWJ sequences, skin tone modifiers
                let mut emoji_end = byte_idx + ch_len;
                let mut char_count = 1;

                while emoji_end < text.len() {
                    let next_rem = match text.get(emoji_end..) {
                        Some(r) => r,
                        None => break,
                    };
                    let next_ch = match next_rem.chars().next() {
                        Some(c) => c,
                        None => break,
                    };
                    let next_class = classify_char(next_ch);
                    let next_cp = next_ch as u32;

                    // Extend if emoji, ZWJ (0x200D), skin tone modifier (0x1F3FB..=0x1F3FF),
                    // variation selector (0xFE0E..=0xFE0F), or regional indicator
                    let is_continuation = next_class == CharClass::Emoji
                        || next_cp == 0x200D
                        || (0x1F3FB..=0x1F3FF).contains(&next_cp)
                        || (0xFE00..=0xFE0F).contains(&next_cp);

                    if is_continuation {
                        emoji_end += next_ch.len_utf8();
                        char_count += 1;
                    } else {
                        break;
                    }
                }

                clusters.push(GlyphCluster {
                    start: byte_idx,
                    end: emoji_end,
                    is_rtl: false,
                    is_emoji: true,
                    is_cjk: false,
                    is_tab: false,
                    is_ligature: false,
                    char_count,
                });
                byte_idx = emoji_end;
            }
            CharClass::Combining => {
                // Attach combining mark to previous cluster if present, otherwise standalone
                if let Some(last) = clusters.last_mut() {
                    last.end += ch_len;
                    last.char_count += 1;
                } else {
                    clusters.push(GlyphCluster {
                        start: byte_idx,
                        end: byte_idx + ch_len,
                        is_rtl: false,
                        is_emoji: false,
                        is_cjk: false,
                        is_tab: false,
                        is_ligature: false,
                        char_count: 1,
                    });
                }
                byte_idx += ch_len;
            }
            CharClass::Rtl => {
                // Gather contiguous RTL characters
                let mut rtl_end = byte_idx + ch_len;
                let mut char_count = 1;

                while rtl_end < text.len() {
                    let next_rem = match text.get(rtl_end..) {
                        Some(r) => r,
                        None => break,
                    };
                    let next_ch = match next_rem.chars().next() {
                        Some(c) => c,
                        None => break,
                    };
                    let next_class = classify_char(next_ch);

                    if next_class == CharClass::Rtl || next_class == CharClass::Combining {
                        rtl_end += next_ch.len_utf8();
                        char_count += 1;
                    } else {
                        break;
                    }
                }

                clusters.push(GlyphCluster {
                    start: byte_idx,
                    end: rtl_end,
                    is_rtl: true,
                    is_emoji: false,
                    is_cjk: false,
                    is_tab: false,
                    is_ligature: false,
                    char_count,
                });
                byte_idx = rtl_end;
            }
            CharClass::Alphanumeric | CharClass::Whitespace | CharClass::Other => {
                // Gather base cluster plus any directly following combining marks
                let mut cluster_end = byte_idx + ch_len;
                let mut char_count = 1;

                while cluster_end < text.len() {
                    let next_rem = match text.get(cluster_end..) {
                        Some(r) => r,
                        None => break,
                    };
                    let next_ch = match next_rem.chars().next() {
                        Some(c) => c,
                        None => break,
                    };
                    if classify_char(next_ch) == CharClass::Combining {
                        cluster_end += next_ch.len_utf8();
                        char_count += 1;
                    } else {
                        break;
                    }
                }

                clusters.push(GlyphCluster {
                    start: byte_idx,
                    end: cluster_end,
                    is_rtl: false,
                    is_emoji: false,
                    is_cjk: false,
                    is_tab: false,
                    is_ligature: false,
                    char_count,
                });
                byte_idx = cluster_end;
            }
        }
    }

    clusters
}

/// Result of resolving a logical byte range selection against shaped clusters.
///
/// Ensures selections snap cleanly to cluster boundaries and never slice
/// combining marks or multi-byte UTF-8 scalars (Plan §13.6).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalSelection {
    /// Index range in the cluster slice covering this selection.
    pub cluster_range: Range<usize>,
    /// Exact byte range in the logical source text.
    pub byte_range: Range<usize>,
    /// Total number of Unicode characters within the selected clusters.
    pub char_count: usize,
    /// Whether any cluster in the selection is right-to-left.
    pub has_rtl: bool,
    /// Whether any cluster in the selection is an emoji.
    pub has_emoji: bool,
    /// Whether any cluster in the selection is CJK.
    pub has_cjk: bool,
}

/// Select a logical byte range over shaped clusters, snapping to complete cluster boundaries.
#[must_use]
pub fn select_logical_range(
    clusters: &[GlyphCluster],
    requested_bytes: Range<usize>,
) -> Option<LogicalSelection> {
    if requested_bytes.is_empty() || clusters.is_empty() {
        return None;
    }

    let mut first_idx = None;
    let mut last_idx = None;

    for (idx, c) in clusters.iter().enumerate() {
        if c.end > requested_bytes.start && c.start < requested_bytes.end {
            if first_idx.is_none() {
                first_idx = Some(idx);
            }
            last_idx = Some(idx);
        }
    }

    let (first, last) = match (first_idx, last_idx) {
        (Some(f), Some(l)) => (f, l),
        _ => return None,
    };

    let selected_clusters = match clusters.get(first..=last) {
        Some(s) => s,
        None => return None,
    };

    let byte_start = match clusters.get(first) {
        Some(c) => c.start,
        None => return None,
    };
    let byte_end = match clusters.get(last) {
        Some(c) => c.end,
        None => return None,
    };

    let mut char_count = 0;
    let mut has_rtl = false;
    let mut has_emoji = false;
    let mut has_cjk = false;

    for c in selected_clusters {
        char_count += c.char_count;
        has_rtl |= c.is_rtl;
        has_emoji |= c.is_emoji;
        has_cjk |= c.is_cjk;
    }

    Some(LogicalSelection {
        cluster_range: first..last + 1,
        byte_range: byte_start..byte_end,
        char_count,
        has_rtl,
        has_emoji,
        has_cjk,
    })
}

/// Errors arising from color or bitmap glyph resource construction or decoding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColorGlyphError {
    /// Dimension exceeds the maximum allowed bound.
    DimensionTooLarge { dimension: u32, max_allowed: u32 },
    /// Buffer byte length exceeds the maximum allowed memory budget.
    BytesTooLarge { bytes: usize, max_allowed: usize },
    /// Buffer length does not match expected dimensions.
    BufferLengthMismatch { expected: usize, actual: usize },
    /// Width or height was zero.
    ZeroDimension,
    /// Pixel buffer was empty.
    Empty,
}

impl std::fmt::Display for ColorGlyphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DimensionTooLarge { dimension, max_allowed } => {
                write!(f, "glyph dimension ({dimension}) exceeds maximum ({max_allowed})")
            }
            Self::BytesTooLarge { bytes, max_allowed } => {
                write!(f, "glyph buffer size ({bytes} bytes) exceeds maximum ({max_allowed} bytes)")
            }
            Self::BufferLengthMismatch { expected, actual } => {
                write!(f, "glyph buffer length mismatch: expected {expected} bytes, got {actual}")
            }
            Self::ZeroDimension => write!(f, "glyph width and height must be non-zero"),
            Self::Empty => write!(f, "glyph pixel buffer is empty"),
        }
    }
}

impl std::error::Error for ColorGlyphError {}

/// A color glyph resource (emoji or color font bitmap).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorGlyphResource {
    /// Raw pixel data (RGBA8).
    pub pixels: Vec<u8>,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// The codepoint this glyph represents.
    pub codepoint: char,
}

impl ColorGlyphResource {
    /// Construct a verified RGBA8 color glyph resource with strict bound validation.
    pub fn try_new_rgba(
        codepoint: char,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<Self, ColorGlyphError> {
        if width == 0 || height == 0 {
            return Err(ColorGlyphError::ZeroDimension);
        }
        if width > MAX_COLOR_GLYPH_DIMENSION {
            return Err(ColorGlyphError::DimensionTooLarge {
                dimension: width,
                max_allowed: MAX_COLOR_GLYPH_DIMENSION,
            });
        }
        if height > MAX_COLOR_GLYPH_DIMENSION {
            return Err(ColorGlyphError::DimensionTooLarge {
                dimension: height,
                max_allowed: MAX_COLOR_GLYPH_DIMENSION,
            });
        }
        if pixels.len() > MAX_COLOR_GLYPH_BYTES {
            return Err(ColorGlyphError::BytesTooLarge {
                bytes: pixels.len(),
                max_allowed: MAX_COLOR_GLYPH_BYTES,
            });
        }
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|px| px.checked_mul(4))
            .ok_or(ColorGlyphError::BytesTooLarge {
                bytes: usize::MAX,
                max_allowed: MAX_COLOR_GLYPH_BYTES,
            })?;
        if pixels.len() != expected {
            return Err(ColorGlyphError::BufferLengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            pixels,
            width,
            height,
            codepoint,
        })
    }

    /// Retrieve the RGBA pixel value at (x, y) coordinates.
    #[must_use]
    pub fn pixel_at(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        if idx + 4 <= self.pixels.len() {
            Some([
                self.pixels[idx],
                self.pixels[idx + 1],
                self.pixels[idx + 2],
                self.pixels[idx + 3],
            ])
        } else {
            None
        }
    }

    /// Generate a meaningful fallback placeholder raster for missing/unsupported color glyphs (Plan §13.7).
    #[must_use]
    pub fn fallback_placeholder(codepoint: char, size_px: u32) -> Self {
        let size = size_px.clamp(8, 256);
        let len = (size as usize) * (size as usize) * 4;
        let mut pixels = vec![0u8; len];
        // Distinct magenta/outline border with semi-transparent fill
        for y in 0..size {
            for x in 0..size {
                let is_border = x == 0 || x == size - 1 || y == 0 || y == size - 1;
                let idx = ((y as usize) * (size as usize) + (x as usize)) * 4;
                if is_border {
                    pixels[idx] = 220;     // R
                    pixels[idx + 1] = 40;  // G
                    pixels[idx + 2] = 220; // B
                    pixels[idx + 3] = 255; // A
                } else {
                    pixels[idx] = 120;     // R
                    pixels[idx + 1] = 120; // G
                    pixels[idx + 2] = 120; // B
                    pixels[idx + 3] = 80;  // A
                }
            }
        }
        Self {
            pixels,
            width: size,
            height: size,
            codepoint,
        }
    }
}

/// A bitmap glyph resource for non-color (monochrome) bitmaps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BitmapGlyphResource {
    /// 1bpp bitmap rows.
    pub bits: Vec<u8>,
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// The codepoint this glyph represents.
    pub codepoint: char,
}

impl BitmapGlyphResource {
    /// Construct a verified 1bpp bitmap glyph resource with strict bound validation.
    pub fn try_new_1bpp(
        codepoint: char,
        width: u32,
        height: u32,
        bits: Vec<u8>,
    ) -> Result<Self, ColorGlyphError> {
        if width == 0 || height == 0 {
            return Err(ColorGlyphError::ZeroDimension);
        }
        if width > MAX_BITMAP_GLYPH_DIMENSION {
            return Err(ColorGlyphError::DimensionTooLarge {
                dimension: width,
                max_allowed: MAX_BITMAP_GLYPH_DIMENSION,
            });
        }
        if height > MAX_BITMAP_GLYPH_DIMENSION {
            return Err(ColorGlyphError::DimensionTooLarge {
                dimension: height,
                max_allowed: MAX_BITMAP_GLYPH_DIMENSION,
            });
        }
        if bits.len() > MAX_BITMAP_GLYPH_BYTES {
            return Err(ColorGlyphError::BytesTooLarge {
                bytes: bits.len(),
                max_allowed: MAX_BITMAP_GLYPH_BYTES,
            });
        }
        let total_bits = (width as usize)
            .checked_mul(height as usize)
            .ok_or(ColorGlyphError::BytesTooLarge {
                bytes: usize::MAX,
                max_allowed: MAX_BITMAP_GLYPH_BYTES,
            })?;
        let expected = (total_bits + 7) / 8;
        if bits.len() != expected {
            return Err(ColorGlyphError::BufferLengthMismatch {
                expected,
                actual: bits.len(),
            });
        }
        Ok(Self {
            bits,
            width,
            height,
            codepoint,
        })
    }

    /// Retrieve the bit value at (x, y) coordinates.
    #[must_use]
    pub fn bit_at(&self, x: u32, y: u32) -> Option<bool> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let bit_idx = (y as usize) * (self.width as usize) + (x as usize);
        let byte_idx = bit_idx / 8;
        let bit_pos = 7 - (bit_idx % 8);
        self.bits.get(byte_idx).map(|byte| ((byte >> bit_pos) & 1) != 0)
    }
}

/// Bounded shaping-context checkpoint: serialized state for save/restore.
///
/// The checkpoint is a simple versioned binary blob with bounded size. It
/// stores the font run identities and cluster offsets so a shaping pass can
/// resume from a known state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapingCheckpoint {
    pub version: u32,
    pub run_identities: Vec<FontRunIdentity>,
    pub cluster_offsets: Vec<(usize, usize)>,
}

/// Error from checkpoint serialization or deserialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckpointError {
    /// The checkpoint exceeds the maximum bounded size.
    TooLarge,
    /// The checkpoint version is unknown.
    UnknownVersion,
    /// The checkpoint data is malformed.
    Malformed,
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "checkpoint size exceeds maximum {MAX_CHECKPOINT_BYTES} bytes"),
            Self::UnknownVersion => write!(f, "unknown checkpoint format version"),
            Self::Malformed => write!(f, "malformed checkpoint binary blob"),
        }
    }
}

impl std::error::Error for CheckpointError {}

/// Serialize a shaping context to a bounded checkpoint blob.
pub fn checkpoint_context(
    runs: &[FontRunIdentity],
    clusters: &[GlyphCluster],
) -> Result<Vec<u8>, CheckpointError> {
    let mut out = Vec::new();
    // Version header (4 bytes, little-endian).
    out.extend_from_slice(&CHECKPOINT_VERSION.to_le_bytes());
    // Run count (2 bytes).
    let run_count = u16::try_from(runs.len()).map_err(|_| CheckpointError::TooLarge)?;
    out.extend_from_slice(&run_count.to_le_bytes());
    for run in runs {
        let family_bytes = run.family.as_bytes();
        let fam_len = u16::try_from(family_bytes.len()).map_err(|_| CheckpointError::TooLarge)?;
        out.extend_from_slice(&fam_len.to_le_bytes());
        out.extend_from_slice(family_bytes);
        out.push(u8::from(run.bold));
        out.push(u8::from(run.italic));
        out.extend_from_slice(&run.size_px.to_le_bytes());
    }
    // Cluster count (2 bytes).
    let cluster_count = u16::try_from(clusters.len()).map_err(|_| CheckpointError::TooLarge)?;
    out.extend_from_slice(&cluster_count.to_le_bytes());
    for cluster in clusters {
        let start_u32 = u32::try_from(cluster.start).map_err(|_| CheckpointError::TooLarge)?;
        let end_u32 = u32::try_from(cluster.end).map_err(|_| CheckpointError::TooLarge)?;
        out.extend_from_slice(&start_u32.to_le_bytes());
        out.extend_from_slice(&end_u32.to_le_bytes());
        // Pack flags into single byte:
        // bit 0: is_rtl, bit 1: is_emoji, bit 2: is_cjk, bit 3: is_tab, bit 4: is_ligature
        let mut flags = 0u8;
        if cluster.is_rtl { flags |= 1 << 0; }
        if cluster.is_emoji { flags |= 1 << 1; }
        if cluster.is_cjk { flags |= 1 << 2; }
        if cluster.is_tab { flags |= 1 << 3; }
        if cluster.is_ligature { flags |= 1 << 4; }
        out.push(flags);
        let char_count_u16 = u16::try_from(cluster.char_count).unwrap_or(u16::MAX);
        out.extend_from_slice(&char_count_u16.to_le_bytes());
    }
    if out.len() > MAX_CHECKPOINT_BYTES {
        return Err(CheckpointError::TooLarge);
    }
    Ok(out)
}

/// Deserialize a shaping context from a checkpoint blob.
pub fn restore_context(bytes: &[u8]) -> Result<ShapingCheckpoint, CheckpointError> {
    if bytes.len() < 4 {
        return Err(CheckpointError::Malformed);
    }
    let mut ver_arr = [0u8; 4];
    ver_arr.copy_from_slice(&bytes[..4]);
    let version = u32::from_le_bytes(ver_arr);
    if version != CHECKPOINT_VERSION_V1 && version != CHECKPOINT_VERSION_V2 {
        return Err(CheckpointError::UnknownVersion);
    }

    let mut pos = 4usize;
    if bytes.len() < pos + 2 {
        return Err(CheckpointError::Malformed);
    }
    let mut run_count_arr = [0u8; 2];
    run_count_arr.copy_from_slice(&bytes[pos..pos + 2]);
    let run_count = u16::from_le_bytes(run_count_arr) as usize;
    pos += 2;

    let mut run_identities = Vec::with_capacity(run_count);
    for _ in 0..run_count {
        if bytes.len() < pos + 2 {
            return Err(CheckpointError::Malformed);
        }
        let mut fam_len_arr = [0u8; 2];
        fam_len_arr.copy_from_slice(&bytes[pos..pos + 2]);
        let fam_len = u16::from_le_bytes(fam_len_arr) as usize;
        pos += 2;
        if bytes.len() < pos + fam_len + 4 {
            return Err(CheckpointError::Malformed);
        }
        let family = String::from_utf8_lossy(&bytes[pos..pos + fam_len]).into_owned();
        pos += fam_len;
        let bold = bytes[pos] != 0;
        pos += 1;
        let italic = bytes[pos] != 0;
        pos += 1;
        let mut size_arr = [0u8; 2];
        size_arr.copy_from_slice(&bytes[pos..pos + 2]);
        let size_px = u16::from_le_bytes(size_arr);
        pos += 2;
        run_identities.push(FontRunIdentity {
            family,
            bold,
            italic,
            size_px,
        });
    }

    if bytes.len() < pos + 2 {
        return Err(CheckpointError::Malformed);
    }
    let mut cluster_count_arr = [0u8; 2];
    cluster_count_arr.copy_from_slice(&bytes[pos..pos + 2]);
    let cluster_count = u16::from_le_bytes(cluster_count_arr) as usize;
    pos += 2;

    let mut cluster_offsets = Vec::with_capacity(cluster_count);
    for _ in 0..cluster_count {
        if version == CHECKPOINT_VERSION_V1 {
            if bytes.len() < pos + 10 {
                return Err(CheckpointError::Malformed);
            }
            let mut start_arr = [0u8; 4];
            start_arr.copy_from_slice(&bytes[pos..pos + 4]);
            let start = u32::from_le_bytes(start_arr) as usize;
            pos += 4;
            let mut end_arr = [0u8; 4];
            end_arr.copy_from_slice(&bytes[pos..pos + 4]);
            let end = u32::from_le_bytes(end_arr) as usize;
            pos += 4;
            pos += 2; // is_rtl + is_emoji flags in v1
            cluster_offsets.push((start, end));
        } else {
            // Version 2: start (4) + end (4) + flags (1) + char_count (2) = 11 bytes
            if bytes.len() < pos + 11 {
                return Err(CheckpointError::Malformed);
            }
            let mut start_arr = [0u8; 4];
            start_arr.copy_from_slice(&bytes[pos..pos + 4]);
            let start = u32::from_le_bytes(start_arr) as usize;
            pos += 4;
            let mut end_arr = [0u8; 4];
            end_arr.copy_from_slice(&bytes[pos..pos + 4]);
            let end = u32::from_le_bytes(end_arr) as usize;
            pos += 4;
            pos += 1; // packed flags
            pos += 2; // char_count
            cluster_offsets.push((start, end));
        }
    }

    Ok(ShapingCheckpoint {
        version,
        run_identities,
        cluster_offsets,
    })
}
