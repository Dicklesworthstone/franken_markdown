//! Bundled font registry.
//!
//! The vendored OFL font families (see `fonts/`) are compiled into the binary via
//! `include_bytes!` and exposed as raw TTF bytes plus a parse helper. There is no
//! filesystem or system-font access, so this is WASM-safe; the PDF embedder
//! subsets each face per document (see [`crate::text::Font::subset`]) so output
//! stays tiny even though the full faces ship in the binary.
//!
//! Theme → family mapping:
//! * [`FontFamily::Sans`] → IBM Plex Sans (the default body face)
//! * [`FontFamily::Serif`] → Computer Modern (the classic LaTeX serif)
//! * monospace / code → CM Typewriter

use crate::text::{Font, FontError};
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

// IBM Plex Sans — default sans body face. SIL OFL 1.1, © IBM Corp.
const PLEX_REGULAR: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
const PLEX_BOLD: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf");
const PLEX_ITALIC: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf");
const PLEX_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-BoldItalic.ttf");

// Computer Modern — the classic LaTeX serif body face. SIL OFL 1.1.
const CM_REGULAR: &[u8] = include_bytes!("../fonts/computer-modern/cmunrm.ttf");
const CM_BOLD: &[u8] = include_bytes!("../fonts/computer-modern/cmunbx.ttf");
const CM_ITALIC: &[u8] = include_bytes!("../fonts/computer-modern/cmunti.ttf");
const CM_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/computer-modern/cmunbi.ttf");

// CM Typewriter — monospace / code face. SIL OFL 1.1.
const MONO_REGULAR: &[u8] = include_bytes!("../fonts/computer-modern/cmuntt.ttf");

static PLEX_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_BOLD_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static PLEX_BOLD_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_BOLD_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static CM_BOLD_ITALIC_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();
static MONO_REGULAR_FONT: OnceLock<Result<Font, FontError>> = OnceLock::new();

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

fn cached_font(
    cache: &'static OnceLock<Result<Font, FontError>>,
    bytes: &'static [u8],
) -> Result<&'static Font, FontError> {
    match cache.get_or_init(|| Font::parse(bytes.to_vec())) {
        Ok(font) => Ok(font),
        Err(err) => Err(err.clone()),
    }
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

#[cfg(test)]
mod tests {
    use super::{FontError, FontStyle, body_bytes, body_font, load_body, load_mono, mono_font};
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
        Ok(())
    }
}
