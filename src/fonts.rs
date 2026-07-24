//! Bundled font registry.
//!
//! The vendored OFL font families (see `fmd-font/fonts/`) are compiled into the binary via
//! `include_bytes!` and exposed as raw TTF bytes plus a parse helper. There is no
//! filesystem or system-font access, so this is WASM-safe; the PDF embedder
//! subsets each face per document (see [`crate::text::Font::subset`]) so output
//! stays tiny even though the full faces ship in the binary.
//!
//! Theme → family mapping:
//! * [`FontFamily::Sans`] → IBM Plex Sans (the default body face)
//! * [`FontFamily::Serif`] → Computer Modern (the classic LaTeX serif)
//! * monospace / code → CM Typewriter
//! * symbol fallback → Noto Sans Math (curated subset; backs characters the
//!   primary faces cannot map, e.g. `⇒`, `≠`, `∑`)

use crate::text::{Font, FontError, Kerning, Ligatures};
use crate::theme::FontFamily;
use std::sync::OnceLock;

/// Weight + slant of a bundled face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    /// Resolve a style from bold/italic flags (as inline emphasis produces them).
    #[must_use]
    pub fn new(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => Self::Regular,
            (true, false) => Self::Bold,
            (false, true) => Self::Italic,
            (true, true) => Self::BoldItalic,
        }
    }
}

// The bundled faces come from the `fmd-font` crate rather than `include_bytes!`
// on a `../fmd-font/fonts/...` path. Those relative paths reach OUTSIDE this
// package, so the font files were absent from the published tarball and
// `cargo publish` failed verification with 10 "couldn't read ... No such file"
// errors — which is why releases stalled at 0.2.0. `fmd-font` ships the same
// files itself and re-exports them, so consuming them through the crate keeps
// the packaged build self-contained. Byte-for-byte identical assets.

// IBM Plex Sans — default sans body face. SIL OFL 1.1, © IBM Corp.
const PLEX_REGULAR: &[u8] = fmd_font::bundled::PLEX_REGULAR;
const PLEX_BOLD: &[u8] = fmd_font::bundled::PLEX_BOLD;
const PLEX_ITALIC: &[u8] = fmd_font::bundled::PLEX_ITALIC;
const PLEX_BOLD_ITALIC: &[u8] = fmd_font::bundled::PLEX_BOLD_ITALIC;

// Computer Modern — the classic LaTeX serif body face. SIL OFL 1.1.
const CM_REGULAR: &[u8] = fmd_font::bundled::CM_REGULAR;
const CM_BOLD: &[u8] = fmd_font::bundled::CM_BOLD;
const CM_ITALIC: &[u8] = fmd_font::bundled::CM_ITALIC;
const CM_BOLD_ITALIC: &[u8] = fmd_font::bundled::CM_BOLD_ITALIC;

// CM Typewriter — monospace / code face. SIL OFL 1.1.
const MONO_REGULAR: &[u8] = fmd_font::bundled::CM_TYPEWRITER;

// Noto Sans Math (curated subset) — symbol fallback face for characters the
// primary body/mono faces cannot map (arrows, math operators, …). SIL OFL 1.1.
// Regenerated via `cargo run --example gen_symbol_fallback_font`.
const SYMBOL_REGULAR: &[u8] = fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS;

static PLEX_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_BOLD_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_BOLD_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_BOLD_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_BOLD_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static MONO_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static SYMBOL_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_REGULAR_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static PLEX_BOLD_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static PLEX_ITALIC_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static PLEX_BOLD_ITALIC_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static CM_REGULAR_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static CM_BOLD_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static CM_ITALIC_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static CM_BOLD_ITALIC_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static MONO_REGULAR_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();
static SYMBOL_REGULAR_LAYOUT: OnceLock<OpenTypeLayoutTables> = OnceLock::new();

/// Parsed OpenType layout tables for a bundled face.
pub(crate) struct OpenTypeLayoutTables {
    pub(crate) kern: Kerning,
    pub(crate) lig: Ligatures,
}

/// Raw TTF bytes for a proportional body face.
#[must_use]
pub fn body_bytes(family: FontFamily, style: FontStyle) -> &'static [u8] {
    match (family, style) {
        (FontFamily::Sans, FontStyle::Regular) => PLEX_REGULAR,
        (FontFamily::Sans, FontStyle::Bold) => PLEX_BOLD,
        (FontFamily::Sans, FontStyle::Italic) => PLEX_ITALIC,
        (FontFamily::Sans, FontStyle::BoldItalic) => PLEX_BOLD_ITALIC,
        (FontFamily::Serif, FontStyle::Regular) => CM_REGULAR,
        (FontFamily::Serif, FontStyle::Bold) => CM_BOLD,
        (FontFamily::Serif, FontStyle::Italic) => CM_ITALIC,
        (FontFamily::Serif, FontStyle::BoldItalic) => CM_BOLD_ITALIC,
    }
}

/// Raw TTF bytes for the monospace (code) face. CM Typewriter ships a single
/// upright weight, so every style currently resolves to it.
#[must_use]
pub fn mono_bytes(_style: FontStyle) -> &'static [u8] {
    MONO_REGULAR
}

/// Parsed bundled proportional body font.
///
/// # Errors
/// Returns [`FontError`] only if a bundled font is malformed. The parsed font is
/// cached process-locally, but every caller still decides its own per-document
/// subset, so output stays deterministic.
pub(crate) fn body_font(family: FontFamily, style: FontStyle) -> Result<&'static Font, FontError> {
    let (cache, bytes) = match (family, style) {
        (FontFamily::Sans, FontStyle::Regular) => (&PLEX_REGULAR_FONT, PLEX_REGULAR),
        (FontFamily::Sans, FontStyle::Bold) => (&PLEX_BOLD_FONT, PLEX_BOLD),
        (FontFamily::Sans, FontStyle::Italic) => (&PLEX_ITALIC_FONT, PLEX_ITALIC),
        (FontFamily::Sans, FontStyle::BoldItalic) => (&PLEX_BOLD_ITALIC_FONT, PLEX_BOLD_ITALIC),
        (FontFamily::Serif, FontStyle::Regular) => (&CM_REGULAR_FONT, CM_REGULAR),
        (FontFamily::Serif, FontStyle::Bold) => (&CM_BOLD_FONT, CM_BOLD),
        (FontFamily::Serif, FontStyle::Italic) => (&CM_ITALIC_FONT, CM_ITALIC),
        (FontFamily::Serif, FontStyle::BoldItalic) => (&CM_BOLD_ITALIC_FONT, CM_BOLD_ITALIC),
    };
    cached_font(cache, bytes)
}

/// Parsed bundled monospace font.
///
/// # Errors
/// See [`body_font`].
pub(crate) fn mono_font(_style: FontStyle) -> Result<&'static Font, FontError> {
    cached_font(&MONO_REGULAR_FONT, MONO_REGULAR)
}

/// Raw TTF bytes for the symbol fallback face (single regular weight).
#[must_use]
pub fn symbol_bytes() -> &'static [u8] {
    SYMBOL_REGULAR
}

/// Parsed bundled symbol fallback font.
///
/// # Errors
/// See [`body_font`].
pub(crate) fn symbol_font() -> Result<&'static Font, FontError> {
    cached_font(&SYMBOL_REGULAR_FONT, SYMBOL_REGULAR)
}

/// Cached GPOS/GSUB tables for the bundled symbol fallback font. The curated
/// subset carries no GPOS/GSUB, so these tables are empty; keeping the same
/// shape as the other faces lets the PDF writer treat every slot uniformly.
///
/// # Errors
/// See [`body_layout_tables`].
pub(crate) fn symbol_layout_tables() -> Result<&'static OpenTypeLayoutTables, FontError> {
    let font = symbol_font()?;
    Ok(cached_layout_tables(&SYMBOL_REGULAR_LAYOUT, font))
}

/// Cached GPOS/GSUB tables for a bundled proportional body font.
///
/// # Errors
/// Returns [`FontError`] only if a bundled font is malformed.
pub(crate) fn body_layout_tables(
    family: FontFamily,
    style: FontStyle,
) -> Result<&'static OpenTypeLayoutTables, FontError> {
    let font = body_font(family, style)?;
    let cache = match (family, style) {
        (FontFamily::Sans, FontStyle::Regular) => &PLEX_REGULAR_LAYOUT,
        (FontFamily::Sans, FontStyle::Bold) => &PLEX_BOLD_LAYOUT,
        (FontFamily::Sans, FontStyle::Italic) => &PLEX_ITALIC_LAYOUT,
        (FontFamily::Sans, FontStyle::BoldItalic) => &PLEX_BOLD_ITALIC_LAYOUT,
        (FontFamily::Serif, FontStyle::Regular) => &CM_REGULAR_LAYOUT,
        (FontFamily::Serif, FontStyle::Bold) => &CM_BOLD_LAYOUT,
        (FontFamily::Serif, FontStyle::Italic) => &CM_ITALIC_LAYOUT,
        (FontFamily::Serif, FontStyle::BoldItalic) => &CM_BOLD_ITALIC_LAYOUT,
    };
    Ok(cached_layout_tables(cache, font))
}

/// Cached GPOS/GSUB tables for the bundled monospace font.
///
/// # Errors
/// See [`body_layout_tables`].
pub(crate) fn mono_layout_tables(
    style: FontStyle,
) -> Result<&'static OpenTypeLayoutTables, FontError> {
    let font = mono_font(style)?;
    Ok(cached_layout_tables(&MONO_REGULAR_LAYOUT, font))
}

fn cached_font(
    cache: &'static OnceLock<Result<Font, FontError>>,
    bytes: &'static [u8],
) -> Result<&'static Font, FontError> {
    match cache.get_or_init(|| Font::parse(bytes.to_vec())) {
        Ok(font) => Ok(font),
        Err(err) => Err(err.clone()),
    }
}

fn cached_layout_tables(
    cache: &'static OnceLock<OpenTypeLayoutTables>,
    font: &'static Font,
) -> &'static OpenTypeLayoutTables {
    cache.get_or_init(|| OpenTypeLayoutTables {
        kern: font.gpos_kerning(),
        lig: font.gsub_ligatures(),
    })
}

/// Parse a bundled proportional body font.
///
/// # Errors
/// Returns [`FontError`] only if a bundled font is malformed — which the registry
/// tests guard against, so in practice this is infallible.
pub fn load_body(family: FontFamily, style: FontStyle) -> Result<Font, FontError> {
    body_font(family, style).cloned()
}

/// Parse the bundled monospace font.
///
/// # Errors
/// See [`load_body`].
pub fn load_mono(style: FontStyle) -> Result<Font, FontError> {
    mono_font(style).cloned()
}

/// Parse the bundled symbol fallback font (curated Noto Sans Math subset).
///
/// # Errors
/// See [`load_body`].
pub fn load_symbol() -> Result<Font, FontError> {
    symbol_font().cloned()
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::{
        FontError, FontStyle, body_bytes, body_font, body_layout_tables, load_body, load_mono,
        mono_font, mono_layout_tables,
    };
    use crate::FontFamily;

    #[test]
    fn cached_body_fonts_match_loaded_fonts() -> Result<(), FontError> {
        for family in [FontFamily::Sans, FontFamily::Serif] {
            for style in [
                FontStyle::Regular,
                FontStyle::Bold,
                FontStyle::Italic,
                FontStyle::BoldItalic,
            ] {
                let cached = body_font(family, style)?;
                let loaded = load_body(family, style)?;
                assert_eq!(cached.units_per_em, loaded.units_per_em);
                assert_eq!(cached.num_glyphs, loaded.num_glyphs);
                assert_eq!(cached.ascent, loaded.ascent);
                assert_eq!(cached.descent, loaded.descent);
                assert_eq!(cached.subset(&['A', 'b']), loaded.subset(&['A', 'b']));
                let tables = body_layout_tables(family, style)?;
                assert_eq!(tables.kern.pair(0, 0), loaded.gpos_kerning().pair(0, 0));
                assert_eq!(tables.lig.is_empty(), loaded.gsub_ligatures().is_empty());
                assert!(!body_bytes(family, style).is_empty());
            }
        }
        Ok(())
    }

    #[test]
    fn cached_mono_font_matches_loaded_font() -> Result<(), FontError> {
        let cached = mono_font(FontStyle::Regular)?;
        let loaded = load_mono(FontStyle::Regular)?;
        assert_eq!(cached.units_per_em, loaded.units_per_em);
        assert_eq!(cached.num_glyphs, loaded.num_glyphs);
        assert_eq!(cached.subset(&['A', 'b']), loaded.subset(&['A', 'b']));
        let tables = mono_layout_tables(FontStyle::Regular)?;
        assert_eq!(tables.kern.pair(0, 0), loaded.gpos_kerning().pair(0, 0));
        assert_eq!(tables.lig.is_empty(), loaded.gsub_ligatures().is_empty());
        Ok(())
    }
}
