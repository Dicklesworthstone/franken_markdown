//! Face management: the engine's face roster, character→glyph resolution
//! with the TeX math-italic convention and the math-alphabet mappings, and
//! glyph-metric queries in em units.
//!
//! Multi-face layout is structural (G0-3 Verdict 3): `∑ ∫ ∏` do not exist
//! in CM Unicode and resolve through the math-symbol fallback face, so
//! **every positioned glyph names its face** and resolution is data, not a
//! rendering afterthought. Resolution walks a per-request preference chain
//! and falls back across the roster; a character no face maps is a precise
//! [`MathError::UnmappedChar`](crate::MathError).
//!
//! **Italic correction, synthesized.** TFM italic corrections do not exist
//! in the sfnt world; per the ratified measure-and-validate seam they are
//! synthesized from decoded geometry as the ink's right overhang past the
//! advance (`max(0, bbox_xmax − advance)`), which reproduces their role in
//! script placement (slanted `∫` and italic letters push superscripts
//! right).

use crate::mbox::FaceId;
use crate::node::MathFont;

/// The engine's face roster, in [`FaceId`] order. The set mirrors the
/// bundled sovereign default (§11.1): CM regular/italic/bold/bold-italic,
/// CM Typewriter, a sans face, and the math-symbol coverage face.
pub struct FaceSet {
    fonts: Vec<fmd_font::Font>,
}

/// [`FaceId`] of CM Regular (upright letters, digits, most symbols).
pub const FACE_REGULAR: FaceId = FaceId(0);
/// [`FaceId`] of CM Italic (math letters, lowercase Greek).
pub const FACE_ITALIC: FaceId = FaceId(1);
/// [`FaceId`] of CM Bold.
pub const FACE_BOLD: FaceId = FaceId(2);
/// [`FaceId`] of CM Bold Italic.
pub const FACE_BOLD_ITALIC: FaceId = FaceId(3);
/// [`FaceId`] of CM Typewriter.
pub const FACE_TYPEWRITER: FaceId = FaceId(4);
/// [`FaceId`] of the sans face (IBM Plex Sans).
pub const FACE_SANS: FaceId = FaceId(5);
/// [`FaceId`] of the math-symbol coverage face (Noto Sans Math subset).
pub const FACE_SYMBOLS: FaceId = FaceId(6);

impl FaceSet {
    /// Build a face set from parsed fonts, in [`FaceId`] order: regular,
    /// italic, bold, bold-italic, typewriter, sans, symbols.
    #[must_use]
    pub fn from_fonts(fonts: Vec<fmd_font::Font>) -> Self {
        Self { fonts }
    }

    /// The bundled sovereign default: the four CM faces, CM Typewriter,
    /// IBM Plex Sans, and the Noto math-symbol subset.
    ///
    /// # Errors
    ///
    /// Propagates [`fmd_font::FontError`] if a bundled face fails to parse
    /// (which would be a build corruption, not a runtime condition).
    #[cfg(feature = "bundled-faces")]
    pub fn bundled() -> Result<Self, fmd_font::FontError> {
        let fonts = vec![
            fmd_font::Font::parse(fmd_font::bundled::CM_REGULAR.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_ITALIC.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_BOLD.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_BOLD_ITALIC.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_TYPEWRITER.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::PLEX_REGULAR.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS.to_vec())?,
        ];
        Ok(Self::from_fonts(fonts))
    }

    /// The font behind a face id, if present.
    #[must_use]
    pub fn font(&self, id: FaceId) -> Option<&fmd_font::Font> {
        self.fonts.get(id.0)
    }

    /// Number of faces in the roster.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// True when the roster is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Resolve a character against a preference chain, falling back across
    /// the whole roster, then through the documented glyph alternates.
    /// Returns the face and glyph id, or `None` when nothing maps it.
    #[must_use]
    pub fn resolve(&self, ch: char, prefer: &[FaceId]) -> Option<(FaceId, u16)> {
        if let Some(hit) = self.resolve_exact(ch, prefer) {
            return Some(hit);
        }
        char_alternate(ch).and_then(|alt| self.resolve_exact(alt, prefer))
    }

    fn resolve_exact(&self, ch: char, prefer: &[FaceId]) -> Option<(FaceId, u16)> {
        for &id in prefer {
            if let Some(font) = self.font(id) {
                let gid = font.glyph_index(ch);
                if gid != 0 {
                    return Some((id, gid));
                }
            }
        }
        for (i, font) in self.fonts.iter().enumerate() {
            let gid = font.glyph_index(ch);
            if gid != 0 {
                return Some((FaceId(i), gid));
            }
        }
        None
    }
}

/// Documented glyph alternates, applied only when the exact character is
/// unmapped across the whole roster: variant forms whose conventional
/// glyph the bundled faces do carry. The substitution stops firing the
/// moment a face gains the exact codepoint (miss-only), and every pair is
/// a same-letter variant — different-but-fine territory, not a semantic
/// change.
#[must_use]
pub fn char_alternate(ch: char) -> Option<char> {
    Some(match ch {
        'ϵ' => 'ε', // lunate epsilon → epsilon (CM Unicode maps only U+03B5)
        'ϑ' => 'θ',
        'ϱ' => 'ρ',
        'ϖ' => 'π',
        '⌀' => '∅',
        _ => return None,
    })
}

/// Per-glyph metrics in ems of the face's design size.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct GlyphMetrics {
    /// Advance width.
    pub advance: f64,
    /// Ink extent above the baseline (0 for empty glyphs).
    pub height: f64,
    /// Ink extent below the baseline, positive (0 for empty glyphs).
    pub depth: f64,
    /// Synthesized italic correction: right ink overhang past the advance.
    pub italic: f64,
}

/// Measure a glyph in em units.
#[must_use]
pub fn glyph_metrics(font: &fmd_font::Font, gid: u16) -> GlyphMetrics {
    let upm = f64::from(font.units_per_em.max(1));
    let advance = f64::from(font.advance_width(gid)) / upm;
    let (height, depth, italic) = font.glyph_bbox(gid).map_or((0.0, 0.0, 0.0), |bbox| {
        let xmax = f64::from(bbox[2]) / upm;
        (
            f64::from(bbox[3]).max(0.0) / upm,
            (-f64::from(bbox[1])).max(0.0) / upm,
            (xmax - advance).max(0.0),
        )
    });
    GlyphMetrics {
        advance,
        height,
        depth,
        italic,
    }
}

/// Kerning between two glyphs of one face, in ems.
#[must_use]
pub fn kern_em(font: &fmd_font::Font, left: u16, right: u16) -> f64 {
    let upm = f64::from(font.units_per_em.max(1));
    f64::from(font.kerning_between_glyphs(left, right)) / upm
}

/// The TeX math-italic convention for a direct character with no alphabet
/// override: Latin letters and lowercase Greek render from the italic
/// face; everything else (digits, uppercase Greek, operators, symbols)
/// from the regular face — with the roster as fallback either way.
#[must_use]
pub fn default_math_chain(ch: char) -> &'static [FaceId] {
    let italicized = ch.is_ascii_alphabetic()
        || ('α'..='ω').contains(&ch)
        || ch == 'ϵ'
        || ch == 'ϑ'
        || ch == 'ϖ'
        || ch == 'ϱ'
        || ch == 'ς'
        || ch == 'φ'
        || ch == 'ϕ';
    if italicized {
        &[FACE_ITALIC, FACE_REGULAR, FACE_SYMBOLS]
    } else {
        &[FACE_REGULAR, FACE_SYMBOLS, FACE_ITALIC]
    }
}

/// Map a character under a math alphabet, returning the (possibly
/// remapped) character and its face-preference chain. Blackboard and
/// calligraphic go through the Unicode math-alphanumeric planes (with the
/// Letterlike exceptions), which the symbol face covers; the styled text
/// faces handle the rest.
#[must_use]
pub fn alphabet_map(font: MathFont, ch: char) -> (char, &'static [FaceId]) {
    match font {
        MathFont::Roman => (ch, &[FACE_REGULAR, FACE_SYMBOLS]),
        MathFont::Bold => (ch, &[FACE_BOLD, FACE_REGULAR, FACE_SYMBOLS]),
        MathFont::Italic => (ch, &[FACE_ITALIC, FACE_REGULAR, FACE_SYMBOLS]),
        MathFont::BoldItalic => (
            ch,
            &[FACE_BOLD_ITALIC, FACE_BOLD, FACE_ITALIC, FACE_REGULAR],
        ),
        MathFont::Typewriter => (ch, &[FACE_TYPEWRITER, FACE_REGULAR]),
        MathFont::SansSerif => (ch, &[FACE_SANS, FACE_REGULAR]),
        MathFont::Blackboard => (
            blackboard_char(ch),
            &[FACE_SYMBOLS, FACE_REGULAR, FACE_ITALIC],
        ),
        MathFont::Calligraphic => (
            calligraphic_char(ch),
            &[FACE_SYMBOLS, FACE_ITALIC, FACE_REGULAR],
        ),
    }
}

/// The double-struck (blackboard) codepoint of a character: the Letterlike
/// exceptions first, then U+1D538-block letters and U+1D7D8-block digits;
/// unmapped characters pass through (and then fail resolution precisely).
#[must_use]
pub fn blackboard_char(ch: char) -> char {
    match ch {
        'C' => 'ℂ',
        'H' => 'ℍ',
        'N' => 'ℕ',
        'P' => 'ℙ',
        'Q' => 'ℚ',
        'R' => 'ℝ',
        'Z' => 'ℤ',
        'A'..='Z' => offset_char(0x1D538, ch, 'A'),
        'a'..='z' => offset_char(0x1D552, ch, 'a'),
        '0'..='9' => offset_char(0x1D7D8, ch, '0'),
        other => other,
    }
}

/// The script (calligraphic) codepoint of a character, with the Letterlike
/// exceptions.
#[must_use]
pub fn calligraphic_char(ch: char) -> char {
    match ch {
        'B' => 'ℬ',
        'E' => 'ℰ',
        'F' => 'ℱ',
        'H' => 'ℋ',
        'I' => 'ℐ',
        'L' => 'ℒ',
        'M' => 'ℳ',
        'R' => 'ℛ',
        'e' => 'ℯ',
        'g' => 'ℊ',
        'o' => 'ℴ',
        'A'..='Z' => offset_char(0x1D49C, ch, 'A'),
        'a'..='z' => offset_char(0x1D4B6, ch, 'a'),
        other => other,
    }
}

fn offset_char(base: u32, ch: char, zero: char) -> char {
    char::from_u32(base + (ch as u32) - (zero as u32)).unwrap_or(ch)
}

/// Spacing fallbacks for combining accent marks: when a face maps neither
/// the combining character nor anything in the chain, the spacing
/// equivalent often exists (CM Unicode carries the spacing accents).
#[must_use]
pub fn accent_spacing_fallback(combining: char) -> Option<char> {
    Some(match combining {
        '\u{0302}' => '\u{02C6}', // circumflex
        '\u{0303}' => '\u{02DC}', // small tilde
        '\u{0304}' => '\u{00AF}', // macron
        '\u{0306}' => '\u{02D8}', // breve
        '\u{0307}' => '\u{02D9}', // dot above
        '\u{0308}' => '\u{00A8}', // diaeresis
        '\u{030A}' => '\u{02DA}', // ring above
        '\u{030C}' => '\u{02C7}', // caron
        '\u{0301}' => '\u{00B4}', // acute
        '\u{0300}' => '`',        // grave
        '\u{20D7}' => '\u{2192}', // vector arrow → right arrow, scaled
        _ => return None,
    })
}

#[cfg(all(test, feature = "bundled-faces"))]
#[allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn bundled_roster_parses_and_resolves_the_basics() {
        let faces = match FaceSet::bundled() {
            Ok(f) => f,
            Err(e) => panic!("bundled faces: {e}"),
        };
        assert_eq!(faces.len(), 7);
        // Letters italicize; digits stay upright.
        let Some((face, gid)) = faces.resolve('x', default_math_chain('x')) else {
            panic!("x must resolve");
        };
        assert_eq!(face, FACE_ITALIC);
        assert_ne!(gid, 0);
        let Some((face, _)) = faces.resolve('7', default_math_chain('7')) else {
            panic!("7 must resolve");
        };
        assert_eq!(face, FACE_REGULAR);
    }

    #[test]
    fn metrics_are_sane_for_x() {
        let faces = match FaceSet::bundled() {
            Ok(f) => f,
            Err(e) => panic!("bundled faces: {e}"),
        };
        let Some((face, gid)) = faces.resolve('x', &[FACE_REGULAR]) else {
            panic!("x in regular");
        };
        let Some(font) = faces.font(face) else {
            panic!("face")
        };
        let m = glyph_metrics(font, gid);
        assert!(m.advance > 0.3 && m.advance < 0.7, "{m:?}");
        // x-height of the bundled CM within 0.13% of σ5 (the ratified
        // validation).
        assert!(
            (m.height - crate::metrics::CM.x_height).abs() < 0.002,
            "measured x-height {} vs σ5 {}",
            m.height,
            crate::metrics::CM.x_height
        );
    }
}
