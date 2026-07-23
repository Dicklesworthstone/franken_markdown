//! The bundled OFL faces, shipped in-crate (feature `bundled-faces`).
//!
//! Four families, chosen as the sovereign default set (nothing depends on
//! host fonts): **Computer Modern** roman/bold/italic/bold-italic — the
//! 3Blue1Brown typographic identity and the default face downstream —
//! plus **CM Typewriter** (code), **IBM Plex Sans** (UI text), and the
//! curated **Noto Sans Math** symbol-fallback subset. All are SIL OFL;
//! the license texts ship alongside the binaries under `fonts/` and must
//! accompany redistribution.
//!
//! These constants are the raw TTF bytes — feed them to [`Font::parse`]
//! (`Font::parse(CM_REGULAR.to_vec())`). The feature is default-off so
//! lean builds that bring their own fonts pay none of the ~2 MB.

/// Computer Modern Unicode roman (`cmunrm.ttf`).
pub const CM_REGULAR: &[u8] = include_bytes!("../fonts/computer-modern/cmunrm.ttf");
/// Computer Modern Unicode bold (`cmunbx.ttf`).
pub const CM_BOLD: &[u8] = include_bytes!("../fonts/computer-modern/cmunbx.ttf");
/// Computer Modern Unicode italic (`cmunti.ttf`).
pub const CM_ITALIC: &[u8] = include_bytes!("../fonts/computer-modern/cmunti.ttf");
/// Computer Modern Unicode bold italic (`cmunbi.ttf`).
pub const CM_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/computer-modern/cmunbi.ttf");
/// Computer Modern Typewriter (`cmuntt.ttf`) — the code face.
pub const CM_TYPEWRITER: &[u8] = include_bytes!("../fonts/computer-modern/cmuntt.ttf");

/// IBM Plex Sans regular.
pub const PLEX_REGULAR: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");
/// IBM Plex Sans bold.
pub const PLEX_BOLD: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Bold.ttf");
/// IBM Plex Sans italic.
pub const PLEX_ITALIC: &[u8] = include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf");
/// IBM Plex Sans bold italic.
pub const PLEX_BOLD_ITALIC: &[u8] =
    include_bytes!("../fonts/ibm-plex-sans/IBMPlexSans-BoldItalic.ttf");

/// The curated Noto Sans Math symbol-fallback subset.
pub const NOTO_SANS_MATH_SYMBOLS: &[u8] =
    include_bytes!("../fonts/noto-sans-math/NotoSansMathSymbols.ttf");

/// Every bundled face as `(stable name, bytes)`, in registry order.
pub const ALL_FACES: [(&str, &[u8]); 10] = [
    ("cm-regular", CM_REGULAR),
    ("cm-bold", CM_BOLD),
    ("cm-italic", CM_ITALIC),
    ("cm-bold-italic", CM_BOLD_ITALIC),
    ("cm-typewriter", CM_TYPEWRITER),
    ("plex-regular", PLEX_REGULAR),
    ("plex-bold", PLEX_BOLD),
    ("plex-italic", PLEX_ITALIC),
    ("plex-bold-italic", PLEX_BOLD_ITALIC),
    ("noto-sans-math-symbols", NOTO_SANS_MATH_SYMBOLS),
];

#[cfg(test)]
#[allow(clippy::panic, clippy::expect_used)]
mod tests {
    use super::ALL_FACES;
    use crate::Font;

    #[test]
    fn every_bundled_face_parses() {
        for (name, bytes) in ALL_FACES {
            let font = Font::parse(bytes.to_vec())
                .unwrap_or_else(|e| panic!("bundled face {name} failed to parse: {e}"));
            assert!(font.units_per_em > 0, "{name}: zero units_per_em");
            assert!(font.num_glyphs > 0, "{name}: zero glyphs");
        }
    }
}
