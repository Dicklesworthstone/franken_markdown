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

/// Parse a bundled proportional body font.
///
/// # Errors
/// Returns [`FontError`] only if a bundled font is malformed — which the registry
/// tests guard against, so in practice this is infallible.
pub fn load_body(family: FontFamily, style: FontStyle) -> Result<Font, FontError> {
    Font::parse(body_bytes(family, style).to_vec())
}

/// Parse the bundled monospace font.
///
/// # Errors
/// See [`load_body`].
pub fn load_mono(style: FontStyle) -> Result<Font, FontError> {
    Font::parse(mono_bytes(style).to_vec())
}
