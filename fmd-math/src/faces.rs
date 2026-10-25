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
//! **TeX's fonts where TeX has them.** In math mode, family 0 at display and
//! text size is cmr10 itself ([`FACE_ROMAN_10`]); CM Unicode's Regular,
//! whose `+ = [ ]` differ from cmr10's, stands for it only in text mode.
//! A glyph from a face that stands for one of TeX's math-family fonts (the
//! CM/AMS math faces, and CM Regular as cmr10) takes that font's TFM box and
//! kerns
//! ([`crate::tfm`]): TeX lays out from the TFM, whose heights, depths and
//! italic corrections are the designer's, not the ink's. Every other glyph
//! keeps metrics measured from its outline, with the italic correction
//! synthesized as the ink's right overhang past the advance
//! (`max(0, bbox_xmax − advance)`), which plays its role in script
//! placement.

use crate::mbox::FaceId;
use crate::node::MathFont;
use crate::style::Style;
use crate::tfm::TfmFont;

/// The engine's face roster, in [`FaceId`] order. The set mirrors the
/// bundled sovereign default (§11.1): CM regular/italic/bold/bold-italic,
/// CM Typewriter, a sans face, and the math-symbol coverage face.
pub struct FaceSet {
    fonts: Vec<fmd_font::Font>,
    /// Per face, the TFM file TeX reads for it in math mode, with each
    /// glyph's code there. Only the bundled roster carries any.
    tfm: Vec<Option<FaceTfm>>,
}

/// A face's TFM file and its glyph-to-code map.
struct FaceTfm {
    font: &'static TfmFont,
    /// The TFM code of each glyph id, where the face's glyph is that code's
    /// character.
    codes: Vec<Option<u8>>,
}

/// The characters cmr10 (OT1) carries that CM Regular maps by Unicode,
/// with their codes: upright Greek capitals, the dotless letters, the
/// ASCII letters, digits and punctuation at their own positions, and the
/// OT1 quotes, dashes and Latin extras. Accents are left out: fmd-math sets
/// them from their own TeX accent metrics.
#[cfg(feature = "bundled-faces")]
const OT1_UNICODE: &[(char, u8)] = &[
    ('Γ', 0x00),
    ('Δ', 0x01),
    ('Θ', 0x02),
    ('Λ', 0x03),
    ('Ξ', 0x04),
    ('Π', 0x05),
    ('Σ', 0x06),
    ('Υ', 0x07),
    ('Φ', 0x08),
    ('Ψ', 0x09),
    ('Ω', 0x0A),
    ('ı', 0x10),
    ('ȷ', 0x11),
    ('ß', 0x19),
    ('æ', 0x1A),
    ('œ', 0x1B),
    ('ø', 0x1C),
    ('Æ', 0x1D),
    ('Œ', 0x1E),
    ('Ø', 0x1F),
    ('”', 0x22),
    ('’', 0x27),
    ('¡', 0x3C),
    ('¿', 0x3E),
    ('“', 0x5C),
    ('‘', 0x60),
    ('–', 0x7B),
    ('—', 0x7C),
];

/// The ASCII characters cmr10 sets at their own code.
#[cfg(feature = "bundled-faces")]
fn ot1_ascii(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || "!#$%&()*+,-./:;=?@[]".contains(ch)
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
/// [`FaceId`] of the math-symbol coverage face (Noto Sans Math subset), the
/// last-resort fallback once the CM math faces miss.
pub const FACE_SYMBOLS: FaceId = FaceId(6);
/// [`FaceId`] of CM math italic at 10 pt (TeX's cmmi10, family 1): math
/// letters, lowercase Greek, and the punctuation and relations plain TeX
/// takes from family 1 (`. , / < >`, `\partial`, `\ell`, `\star`, …).
pub const FACE_MATH_ITALIC: FaceId = FaceId(7);
/// [`FaceId`] of CM math symbols at 10 pt (cmsy10, family 2): relations,
/// binary operators, arrows, `{ } | ‖`, primes and calligraphic capitals.
pub const FACE_MATH_SYMBOLS: FaceId = FaceId(8);
/// [`FaceId`] of CM math extension (cmex10, family 3): big operators in
/// their text and display sizes, delimiter sizes and extensible pieces.
pub const FACE_MATH_EXTENSION: FaceId = FaceId(9);
/// [`FaceId`] of cmmi7, the math italic TeX sets first-order scripts in.
pub const FACE_MATH_ITALIC_7: FaceId = FaceId(10);
/// [`FaceId`] of cmmi5 (second-order scripts).
pub const FACE_MATH_ITALIC_5: FaceId = FaceId(11);
/// [`FaceId`] of cmsy7 (first-order scripts).
pub const FACE_MATH_SYMBOLS_7: FaceId = FaceId(12);
/// [`FaceId`] of cmsy5 (second-order scripts).
pub const FACE_MATH_SYMBOLS_5: FaceId = FaceId(13);
/// [`FaceId`] of cmr7, the upright face of first-order scripts.
pub const FACE_ROMAN_7: FaceId = FaceId(14);
/// [`FaceId`] of cmr5 (second-order scripts).
pub const FACE_ROMAN_5: FaceId = FaceId(15);
/// [`FaceId`] of cmr10, TeX's family 0 in math mode at display and text
/// size: digits, `+ = ( ) [ ]`, upright Greek and operator names.
pub const FACE_ROMAN_10: FaceId = FaceId(16);
/// AMS-A at text/display size.
pub const FACE_AMS_A_10: FaceId = FaceId(17);
/// AMS-A's first-order script cut.
pub const FACE_AMS_A_7: FaceId = FaceId(18);
/// AMS-A's second-order script cut.
pub const FACE_AMS_A_5: FaceId = FaceId(19);
/// AMS-B and blackboard capitals at text/display size.
pub const FACE_AMS_B_10: FaceId = FaceId(20);
/// AMS-B's first-order script cut.
pub const FACE_AMS_B_7: FaceId = FaceId(21);
/// AMS-B's second-order script cut.
pub const FACE_AMS_B_5: FaceId = FaceId(22);

/// The private-use page the CM and AMS faces map their TFM code positions
/// into: code `c` is `U+F000 + c` in that face (ADR-0028).
pub const TEX_CODE_PAGE: u32 = 0xF000;

/// The character addressing TFM code position `code` of a CM or AMS face.
#[must_use]
pub const fn tex_code(code: u8) -> char {
    match char::from_u32(TEX_CODE_PAGE + code as u32) {
        Some(ch) => ch,
        None => '\u{F000}',
    }
}

impl FaceSet {
    /// Build a face set from parsed fonts, in [`FaceId`] order: regular,
    /// italic, bold, bold-italic, typewriter, sans, symbols, then the CM
    /// math faces (math italic, symbols, extension, the 7/5-point italic,
    /// symbols and roman cuts, and cmr10), then AMS-A and AMS-B at 10/7/5 pt.
    /// A roster without the CM or AMS math faces
    /// still lays out: resolution falls back across whatever is present.
    /// Its glyphs take metrics measured from their outlines: only
    /// [`FaceSet::bundled`] knows which faces are TeX's fonts.
    #[must_use]
    pub fn from_fonts(fonts: Vec<fmd_font::Font>) -> Self {
        Self {
            fonts,
            tfm: Vec::new(),
        }
    }

    /// The bundled sovereign default: the four CM text faces, CM
    /// Typewriter, IBM Plex Sans, the Noto math-symbol subset, and the CM
    /// CM and AMS math faces TeX sets mathematics in (ADR-0028).
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
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_ITALIC_10.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_SYMBOLS_10.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_EXTENSION_10.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_ITALIC_7.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_ITALIC_5.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_SYMBOLS_7.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_MATH_SYMBOLS_5.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_ROMAN_7.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_ROMAN_5.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::CM_ROMAN_10.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::AMS_A_10.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::AMS_A_7.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::AMS_A_5.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::AMS_B_10.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::AMS_B_7.to_vec())?,
            fmd_font::Font::parse(fmd_font::bundled::AMS_B_5.to_vec())?,
        ];
        let mut faces = Self::from_fonts(fonts);
        faces.tfm = (0..faces.fonts.len()).map(|_| None).collect();
        // CM Regular is CM Unicode, mapped by Unicode; the converted faces
        // carry every TFM code at U+F000 + code (ADR-0028).
        let regular = OT1_UNICODE
            .iter()
            .copied()
            .chain(
                (0x21..0x7F_u8)
                    .filter(|&code| ot1_ascii(char::from(code)))
                    .map(|code| (char::from(code), code)),
            )
            .collect::<Vec<_>>();
        faces.attach_tfm(FACE_REGULAR, &crate::tfm::CMR10, &regular);
        let tex_coded = (0..128_u8)
            .map(|code| (tex_code(code), code))
            .collect::<Vec<_>>();
        for (face, font) in [
            (FACE_MATH_ITALIC, &crate::tfm::CMMI10),
            (FACE_MATH_SYMBOLS, &crate::tfm::CMSY10),
            (FACE_MATH_EXTENSION, &crate::tfm::CMEX10),
            (FACE_MATH_ITALIC_7, &crate::tfm::CMMI7),
            (FACE_MATH_ITALIC_5, &crate::tfm::CMMI5),
            (FACE_MATH_SYMBOLS_7, &crate::tfm::CMSY7),
            (FACE_MATH_SYMBOLS_5, &crate::tfm::CMSY5),
            (FACE_ROMAN_7, &crate::tfm::CMR7),
            (FACE_ROMAN_5, &crate::tfm::CMR5),
            (FACE_ROMAN_10, &crate::tfm::CMR10),
            (FACE_AMS_A_10, &crate::tfm::MSAM10),
            (FACE_AMS_A_7, &crate::tfm::MSAM7),
            (FACE_AMS_A_5, &crate::tfm::MSAM5),
            (FACE_AMS_B_10, &crate::tfm::MSBM10),
            (FACE_AMS_B_7, &crate::tfm::MSBM7),
            (FACE_AMS_B_5, &crate::tfm::MSBM5),
        ] {
            faces.attach_tfm(face, font, &tex_coded);
        }
        Ok(faces)
    }

    /// Record `font` as `face`'s TFM, reading each code's glyph through the
    /// characters that address it.
    #[cfg(feature = "bundled-faces")]
    fn attach_tfm(&mut self, face: FaceId, font: &'static TfmFont, chars: &[(char, u8)]) {
        let Some(sfnt) = self.fonts.get(face.0) else {
            return;
        };
        let mut codes = vec![None; usize::from(sfnt.num_glyphs)];
        for &(ch, code) in chars {
            let gid = sfnt.glyph_index(ch);
            if gid != 0 && font.char(code).is_some() {
                if let Some(slot) = codes.get_mut(usize::from(gid)) {
                    *slot = Some(code);
                }
            }
        }
        if let Some(slot) = self.tfm.get_mut(face.0) {
            *slot = Some(FaceTfm { font, codes });
        }
    }

    /// The TFM file TeX reads for `face` in math mode and the code of glyph
    /// `gid` there; `None` when the face has no TFM or the glyph is not one
    /// of its characters.
    #[must_use]
    pub fn tfm(&self, face: FaceId, gid: u16) -> Option<(&'static TfmFont, u8)> {
        let entry = self.tfm.get(face.0)?.as_ref()?;
        let code = (*entry.codes.get(usize::from(gid))?)?;
        Some((entry.font, code))
    }

    /// The TFM file behind `face`, if it has one.
    #[must_use]
    pub fn tfm_font(&self, face: FaceId) -> Option<&'static TfmFont> {
        self.tfm.get(face.0)?.as_ref().map(|entry| entry.font)
    }

    /// The optically sized cut TeX sets a script in: cmmi7, cmsy7 and cmr7
    /// in script style, the 5-point cuts in scriptscript, for a glyph
    /// resolved from the 10-point math italic, math symbols or roman face.
    /// `None` outside the script styles, for faces without sized cuts (bold,
    /// typewriter, cmex — plain TeX scales cmex10 at every size), or when
    /// the cut lacks the character.
    #[must_use]
    pub fn optical(&self, face: FaceId, ch: char, style: Style) -> Option<(FaceId, u16)> {
        let cut = match (style, face) {
            (Style::Script, FACE_MATH_ITALIC) => FACE_MATH_ITALIC_7,
            (Style::Script, FACE_MATH_SYMBOLS) => FACE_MATH_SYMBOLS_7,
            (Style::Script, FACE_REGULAR) => FACE_ROMAN_7,
            (Style::Script, FACE_AMS_A_10) => FACE_AMS_A_7,
            (Style::Script, FACE_AMS_B_10) => FACE_AMS_B_7,
            (Style::ScriptScript, FACE_MATH_ITALIC) => FACE_MATH_ITALIC_5,
            (Style::ScriptScript, FACE_MATH_SYMBOLS) => FACE_MATH_SYMBOLS_5,
            (Style::ScriptScript, FACE_REGULAR) => FACE_ROMAN_5,
            (Style::ScriptScript, FACE_AMS_A_10) => FACE_AMS_A_5,
            (Style::ScriptScript, FACE_AMS_B_10) => FACE_AMS_B_5,
            _ => return None,
        };
        let gid = self.font(cut)?.glyph_index(ch);
        (gid != 0).then_some((cut, gid))
    }

    /// The face TeX itself sets `ch` in at `style`, for a glyph resolved
    /// from `face`: the optically sized cut in the script styles
    /// ([`FaceSet::optical`]), and cmr10 at display and text size
    /// ([`FaceSet::math_roman`]). `None` when neither applies or the cut
    /// lacks the character.
    #[must_use]
    pub fn tex_face(
        &self,
        face: FaceId,
        ch: char,
        style: Style,
        text_mode: bool,
    ) -> Option<(FaceId, u16)> {
        if let Some(cut) = self.optical(face, ch, style) {
            return Some(cut);
        }
        if !matches!(style, Style::Display | Style::Text) {
            return None;
        }
        self.math_roman(face, ch, text_mode)
    }

    /// cmr10's glyph for `ch`, TeX's family 0 in math mode, where CM Regular
    /// resolved it. CM Unicode's Regular stands in for cmr10 only in text
    /// mode: its `+` and `=` carry 0.57 of cmr10's ink and its brackets 1.48.
    /// `None` in text mode, for other faces, or when cmr10 lacks `ch`.
    #[must_use]
    pub fn math_roman(&self, face: FaceId, ch: char, text_mode: bool) -> Option<(FaceId, u16)> {
        if text_mode || face != FACE_REGULAR {
            return None;
        }
        let gid = self.font(FACE_ROMAN_10)?.glyph_index(ch);
        (gid != 0).then_some((FACE_ROMAN_10, gid))
    }

    /// The glyph at TFM code position `code` of `face` (a CM math face), if
    /// the roster carries that face.
    #[must_use]
    pub fn tex_glyph(&self, face: FaceId, code: u8) -> Option<u16> {
        let gid = self.font(face)?.glyph_index(tex_code(code));
        (gid != 0).then_some(gid)
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

/// TeX's family assignment for a direct character with no alphabet
/// override (plain TeX's `\mathcode`s): Latin letters and lowercase Greek
/// from the math italic (cmmi, family 1); relations, operators, arrows and
/// `{ } |` from the math symbols (cmsy, family 2), which map exactly the
/// characters TeX takes from them; `. , / < >` and `\partial`-class
/// symbols from cmmi; digits, `+ = ( ) [ ]` and uppercase Greek from the
/// roman face (family 0). The CM text faces and the Noto subset follow as
/// fallbacks, so a roster without the CM math faces resolves as before.
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
        &[FACE_MATH_ITALIC, FACE_ITALIC, FACE_REGULAR, FACE_SYMBOLS]
    } else {
        &[
            FACE_MATH_SYMBOLS,
            FACE_MATH_ITALIC,
            FACE_AMS_A_10,
            FACE_AMS_B_10,
            FACE_REGULAR,
            FACE_SYMBOLS,
            FACE_ITALIC,
        ]
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
            &[FACE_AMS_B_10, FACE_SYMBOLS, FACE_REGULAR, FACE_ITALIC],
        ),
        // `\mathcal` is cmsy's calligraphic capitals in TeX; the CM math
        // symbols face maps them at the math-script codepoints.
        MathFont::Calligraphic => (
            calligraphic_char(ch),
            &[FACE_MATH_SYMBOLS, FACE_SYMBOLS, FACE_ITALIC, FACE_REGULAR],
        ),
        // `\mathscr` is mathrsfs's rsfs10 in TeX, a script distinct from
        // cmsy's calligraphic: the same Unicode script codepoints, resolved
        // from Noto Sans Math's script letters before any CM face.
        MathFont::Script => (
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
        assert_eq!(faces.len(), 23);
        // Letters come from the math italic; digits stay upright.
        let Some((face, gid)) = faces.resolve('x', default_math_chain('x')) else {
            panic!("x must resolve");
        };
        assert_eq!(face, FACE_MATH_ITALIC);
        assert_ne!(gid, 0);
        let Some((face, _)) = faces.resolve('7', default_math_chain('7')) else {
            panic!("7 must resolve");
        };
        assert_eq!(face, FACE_REGULAR);
    }

    /// TeX's family for each character class: the glyph source of `≤ ≥ −`
    /// is cmsy, of Greek and `∂` cmmi, of `( + =` and uppercase Greek the
    /// roman face — never the Noto fallback (ADR-0028's acceptance).
    #[test]
    fn math_characters_resolve_to_the_faces_tex_uses() {
        let faces = FaceSet::bundled().expect("bundled faces");
        let source = |ch: char| {
            faces
                .resolve(ch, default_math_chain(ch))
                .map(|(face, _)| face)
                .unwrap_or_else(|| panic!("{ch:?} must resolve"))
        };
        for ch in [
            '≤', '≥', '−', '∞', '→', '∈', '⋅', '×', '{', '|', '∇', '≡', '′',
        ] {
            assert_eq!(source(ch), FACE_MATH_SYMBOLS, "{ch:?}");
        }
        for ch in [
            'α', 'θ', 'ϑ', 'ϕ', 'φ', 'ϵ', 'ε', '∂', 'ℓ', 'x', 'f', '<', ',', '/',
        ] {
            assert_eq!(source(ch), FACE_MATH_ITALIC, "{ch:?}");
        }
        for ch in ['(', '+', '=', '2', 'Γ', 'Ω'] {
            assert_eq!(source(ch), FACE_REGULAR, "{ch:?}");
        }
        let (cal, chain) = alphabet_map(MathFont::Calligraphic, 'L');
        assert_eq!(
            faces.resolve(cal, chain).map(|(f, _)| f),
            Some(FACE_MATH_SYMBOLS)
        );
    }

    /// Scripts take the 7- and 5-point cuts; display/text and faces without
    /// sized cuts keep their glyph.
    #[test]
    fn scripts_take_the_optical_cuts() {
        let faces = FaceSet::bundled().expect("bundled faces");
        assert_eq!(
            faces
                .optical(FACE_MATH_ITALIC, 'x', Style::Script)
                .map(|h| h.0),
            Some(FACE_MATH_ITALIC_7)
        );
        assert_eq!(
            faces
                .optical(FACE_MATH_SYMBOLS, '−', Style::ScriptScript)
                .map(|h| h.0),
            Some(FACE_MATH_SYMBOLS_5)
        );
        assert_eq!(
            faces.optical(FACE_REGULAR, '2', Style::Script).map(|h| h.0),
            Some(FACE_ROMAN_7)
        );
        assert_eq!(faces.optical(FACE_REGULAR, '2', Style::Text), None);
        assert_eq!(faces.optical(FACE_BOLD, 'x', Style::Script), None);
        assert!(
            faces.tex_glyph(FACE_MATH_EXTENSION, 0x5A).is_some(),
            "display integral"
        );
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
