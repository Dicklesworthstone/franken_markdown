//! The bundled OFL faces, shipped in-crate (feature `bundled-faces`).
//!
//! Chosen as the sovereign default set (nothing depends on host fonts):
//! **Computer Modern** roman/bold/italic/bold-italic — the 3Blue1Brown
//! typographic identity and the default face downstream — plus **CM
//! Typewriter** (code), **IBM Plex Sans** (UI text), the **CM math** faces
//! TeX sets mathematics in (math italic, symbols and extension at their
//! optical sizes, converted from the AMS Type 1 release and renamed; see
//! `fonts/cm-math/PROVENANCE.json`), and the curated **Noto Sans Math**
//! symbol-fallback subset. All are SIL OFL; the license texts ship
//! alongside the binaries under `fonts/` and must accompany redistribution.
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

/// CM math italic at 10 pt (TeX's cmmi10, converted and renamed).
pub const CM_MATH_ITALIC_10: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathItalic10.ttf");
/// CM math italic at 7 pt (cmmi7): first-order scripts.
pub const CM_MATH_ITALIC_7: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathItalic7.ttf");
/// CM math italic at 5 pt (cmmi5): second-order scripts.
pub const CM_MATH_ITALIC_5: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathItalic5.ttf");
/// CM math symbols at 10 pt (cmsy10).
pub const CM_MATH_SYMBOLS_10: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathSymbols10.ttf");
/// CM math symbols at 7 pt (cmsy7).
pub const CM_MATH_SYMBOLS_7: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathSymbols7.ttf");
/// CM math symbols at 5 pt (cmsy5).
pub const CM_MATH_SYMBOLS_5: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathSymbols5.ttf");
/// CM math extension (cmex10): big operators, delimiter sizes, extensible pieces.
pub const CM_MATH_EXTENSION_10: &[u8] = include_bytes!("../fonts/cm-math/FmdCmMathExtension10.ttf");
/// CM roman at 7 pt (cmr7): upright script characters.
pub const CM_ROMAN_7: &[u8] = include_bytes!("../fonts/cm-math/FmdCmRoman7.ttf");
/// CM roman at 5 pt (cmr5): upright second-order script characters.
pub const CM_ROMAN_5: &[u8] = include_bytes!("../fonts/cm-math/FmdCmRoman5.ttf");

/// The curated monochrome Noto Emoji subset (emoji fallback face). Regenerated
/// via `cargo run --example gen_emoji_fallback_font`.
#[cfg(feature = "emoji-face")]
pub const NOTO_EMOJI_CURATED: &[u8] = include_bytes!("../fonts/noto-emoji/NotoEmojiCurated.ttf");

/// Every bundled face as `(stable name, bytes)`, in registry order.
pub const ALL_FACES: [(&str, &[u8]); 19] = [
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
    ("cm-math-italic-10", CM_MATH_ITALIC_10),
    ("cm-math-italic-7", CM_MATH_ITALIC_7),
    ("cm-math-italic-5", CM_MATH_ITALIC_5),
    ("cm-math-symbols-10", CM_MATH_SYMBOLS_10),
    ("cm-math-symbols-7", CM_MATH_SYMBOLS_7),
    ("cm-math-symbols-5", CM_MATH_SYMBOLS_5),
    ("cm-math-extension-10", CM_MATH_EXTENSION_10),
    ("cm-roman-7", CM_ROMAN_7),
    ("cm-roman-5", CM_ROMAN_5),
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

    /// The CM math faces carry TeX's glyphs where fmd-math looks for them:
    /// Unicode for the characters TeX sets from that face, and every TFM
    /// code position at U+F000 + code (cmex's display integral is 0x5A).
    #[test]
    fn cm_math_faces_map_unicode_and_tex_code_positions() {
        let parse = |bytes: &[u8]| Font::parse(bytes.to_vec()).expect("cm-math face parses");
        let italic = parse(super::CM_MATH_ITALIC_10);
        let symbols = parse(super::CM_MATH_SYMBOLS_10);
        let extension = parse(super::CM_MATH_EXTENSION_10);
        let roman7 = parse(super::CM_ROMAN_7);
        for ch in [
            'x', 'f', 'α', 'θ', 'ϑ', 'ϕ', 'φ', 'ϵ', 'ε', '∂', 'ℓ', '<', ',',
        ] {
            assert_ne!(italic.glyph_index(ch), 0, "cmmi10 maps {ch:?}");
        }
        for ch in ['−', '≤', '≥', '∞', '∈', '→', '⋅', '×', '{', '|', '∇'] {
            assert_ne!(symbols.glyph_index(ch), 0, "cmsy10 maps {ch:?}");
        }
        // Calligraphic capitals and upright Greek stay out of the Unicode
        // map: TeX sets `A` from cmmi and `\Gamma` from the roman face.
        assert_eq!(symbols.glyph_index('A'), 0);
        assert_eq!(italic.glyph_index('Γ'), 0);
        for ch in ['∑', '∫', '∏'] {
            assert_ne!(extension.glyph_index(ch), 0, "cmex10 maps text-size {ch:?}");
        }
        let display_integral = extension.glyph_index('\u{F05A}');
        assert_ne!(display_integral, 0);
        assert_ne!(display_integral, extension.glyph_index('∫'));
        assert_eq!(
            extension.glyph_index('('),
            0,
            "cmex delimiter sizes are code-addressed only"
        );
        assert_ne!(roman7.glyph_index('2'), 0);
        assert_ne!(roman7.glyph_index('Γ'), 0);
    }
}
