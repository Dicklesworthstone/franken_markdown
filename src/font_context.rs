//! Shared context fallback-font and raster extensions (FCB-075.A).
//!
//! Platform-neutral data model for font run identity, fallback chains,
//! glyph clustering, and bounded shaping-context checkpoints. No unsafe
//! code, no external dependencies — the base engine remains
//! platform-neutral; native adapters (FCB-075.B) consume this API.

#![forbid(unsafe_code)]

/// Identity of a font run: family, style and pixel size.
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
/// emoji ZWJ sequences, and ligatures. The `start`/`end` byte offsets are
/// into the original source text.
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
    /// Emoji (includes ZWJ sequences).
    Emoji,
    /// Combining diacritical mark (attaches to previous cluster).
    Combining,
    /// Whitespace or control character.
    Whitespace,
    /// Anything else (punctuation, symbols, etc.).
    Other,
}

/// Classify a character into a cluster-relevant category.
pub fn classify_char(c: char) -> CharClass {
    let cp = c as u32;
    if c.is_whitespace() {
        CharClass::Whitespace
    } else if (0x1F300..=0x1F9FF).contains(&cp) || cp == 0x200D {
        // Emoji and ZWJ.
        CharClass::Emoji
    } else if (0x0300..=0x036F).contains(&cp)
        || (0x1AB0..=0x1AFF).contains(&cp)
        || (0x20D0..=0x20FF).contains(&cp)
    {
        // Combining diacritical marks.
        CharClass::Combining
    } else if (0x0590..=0x05FF).contains(&cp)
        || (0x0600..=0x06FF).contains(&cp)
        || (0x0700..=0x074F).contains(&cp)
        || (0xFB1D..=0xFB4F).contains(&cp)
    {
        // RTL scripts: Hebrew, Arabic, Syriac, etc.
        CharClass::Rtl
    } else if (0x2E80..=0x9FFF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0x20000..=0x2FA1F).contains(&cp)
    {
        // CJK: CJK Unified Ideographs, Hangul, Compatibility Ideographs, ext.
        CharClass::Cjk
    } else if c.is_alphanumeric() {
        CharClass::Alphanumeric
    } else {
        CharClass::Other
    }
}

/// Segment text into glyph clusters for shaping.
///
/// Combining marks attach to the previous cluster. Emoji ZWJ sequences form
/// single clusters. RTL characters form clusters that are marked `is_rtl`.
pub fn segment_clusters(text: &str) -> Vec<GlyphCluster> {
    let mut clusters = Vec::new();
    let mut start = 0usize;
    let mut prev_class: Option<CharClass> = None;
    let mut has_rtl = false;
    let mut has_emoji = false;

    for (offset, ch) in text.char_indices() {
        let class = classify_char(ch);
        match class {
            CharClass::Combining => {
                // Combining mark attaches to the previous cluster.
                has_rtl = has_rtl || prev_class == Some(CharClass::Rtl);
                has_emoji = has_emoji || prev_class == Some(CharClass::Emoji);
                continue;
            }
            _ => {
                // Flush the previous cluster if we have one.
                if prev_class.is_some() && offset > start {
                    clusters.push(GlyphCluster {
                        start,
                        end: offset,
                        is_rtl: has_rtl,
                        is_emoji: has_emoji,
                    });
                }
                start = offset;
                prev_class = Some(class);
                has_rtl = class == CharClass::Rtl;
                has_emoji = class == CharClass::Emoji;
            }
        }
    }

    // Flush the final cluster.
    if prev_class.is_some() && text.len() > start {
        clusters.push(GlyphCluster {
            start,
            end: text.len(),
            is_rtl: has_rtl,
            is_emoji: has_emoji,
        });
    }

    clusters
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

/// Maximum checkpoint size in bytes.
pub const MAX_CHECKPOINT_BYTES: usize = 4096;
/// Checkpoint format version.
pub const CHECKPOINT_VERSION: u32 = 1;

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

/// Serialize a shaping context to a bounded checkpoint blob.
pub fn checkpoint_context(
    runs: &[FontRunIdentity],
    clusters: &[GlyphCluster],
) -> Result<Vec<u8>, CheckpointError> {
    let mut out = Vec::new();
    // Version header (4 bytes, little-endian).
    out.extend_from_slice(&CHECKPOINT_VERSION.to_le_bytes());
    // Run count (2 bytes).
    out.extend_from_slice(&(runs.len() as u16).to_le_bytes());
    for run in runs {
        let family_bytes = run.family.as_bytes();
        // Family length (2 bytes) + family bytes.
        out.extend_from_slice(&(family_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(family_bytes);
        out.push(u8::from(run.bold));
        out.push(u8::from(run.italic));
        out.extend_from_slice(&run.size_px.to_le_bytes());
    }
    // Cluster count (2 bytes).
    out.extend_from_slice(&(clusters.len() as u16).to_le_bytes());
    for cluster in clusters {
        out.extend_from_slice(&(cluster.start as u32).to_le_bytes());
        out.extend_from_slice(&(cluster.end as u32).to_le_bytes());
        out.push(u8::from(cluster.is_rtl));
        out.push(u8::from(cluster.is_emoji));
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
    let version = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    if version != CHECKPOINT_VERSION {
        return Err(CheckpointError::UnknownVersion);
    }
    let mut pos = 4usize;
    if bytes.len() < pos + 2 {
        return Err(CheckpointError::Malformed);
    }
    let run_count = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
    pos += 2;
    let mut run_identities = Vec::with_capacity(run_count);
    for _ in 0..run_count {
        if bytes.len() < pos + 2 {
            return Err(CheckpointError::Malformed);
        }
        let fam_len = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
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
        let size_px = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
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
    let cluster_count = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
    pos += 2;
    let mut cluster_offsets = Vec::with_capacity(cluster_count);
    for _ in 0..cluster_count {
        if bytes.len() < pos + 10 {
            return Err(CheckpointError::Malformed);
        }
        let start = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let end = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        pos += 2; // is_rtl + is_emoji flags
        cluster_offsets.push((start, end));
    }
    Ok(ShapingCheckpoint {
        version,
        run_identities,
        cluster_offsets,
    })
}

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
