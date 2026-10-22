//! The math-metrics table: TeX's Appendix-G parameter family, synthesized
//! for the bundled faces.
//!
//! **Provenance.** The values are the TFM fontdimen families of Computer
//! Modern — cmsy10's σ₅…σ₂₂ and cmex10's ξ₈…ξ₁₃ at text size (The TeXbook,
//! Appendix G; the parameters Appendix G consumes by name) and their
//! script-size counterparts — read exactly from the TFM files generated into
//! [`crate::tfm`], in ems of each font's design size. The G0-3 spike ratified this
//! *synthesis method* (franken_manim `docs/g0/G0-3-fmd-math-ratification.md`,
//! Verdict 1): compile the published family in as em constants and
//! **validate** against geometry decoded from the bundled faces by
//! fmd-font. The bundled CM Unicode faces measure within 0.13 % of the
//! published x-height and match the axis height exactly — they *are* the
//! Computer Modern the TFM family describes. The validation tests promoted
//! from the spike live in this crate's suite; per-face recalibration only
//! ever happens through the same measure-and-validate seam.
//!
//! **Size rule.** TeX reads each Appendix-G parameter from the symbol and
//! extension fonts loaded at the current size (cur_size in tex.web). Under
//! LaTeX's article and amsmath those are cmsy10 and cmex10 in display and
//! text style, cmsy7 and cmex7 in script style, and cmsy5 and cmex7 at 5 pt
//! in scriptscript style. Computer Modern's small sizes are optical designs,
//! so their parameters are not cmsy10's scaled down: cmsy7's sup2 is 0.431
//! of its em against cmsy10's 0.363. [`MathConstants::at`] returns a
//! style's parameters in ems of its own fonts, read from the generated TFM
//! fontdimens ([`crate::tfm`]). The layout multiplies them by the style's
//! size, so each product is the length TeX reads. Rule 18a's baseline drops
//! come from the font at the size the scripts are set at (tex.web §756):
//! [`MathConstants::script_drops`]. (franken_manim fm-giir.)

use crate::style::Style;
use crate::tfm::{CMEX7, CMEX10, CMSY5, CMSY7, CMSY10, TfmFont};

/// The Appendix-G parameter family, in ems of the text-size em.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MathConstants {
    /// σ₅ — x-height.
    pub x_height: f64,
    /// σ₆ — quad (1 em).
    pub quad: f64,
    /// σ₈ — num1: numerator shift, display style.
    pub num1: f64,
    /// σ₉ — num2: numerator shift, non-display, with bar.
    pub num2: f64,
    /// σ₁₀ — num3: numerator shift, non-display, barless.
    pub num3: f64,
    /// σ₁₁ — denom1: denominator shift, display.
    pub denom1: f64,
    /// σ₁₂ — denom2: denominator shift, non-display.
    pub denom2: f64,
    /// σ₁₃ — sup1: superscript shift, display uncramped.
    pub sup1: f64,
    /// σ₁₄ — sup2: superscript shift, non-display uncramped.
    pub sup2: f64,
    /// σ₁₅ — sup3: superscript shift, cramped.
    pub sup3: f64,
    /// σ₁₆ — sub1: subscript shift without a superscript.
    pub sub1: f64,
    /// σ₁₇ — sub2: subscript shift with a superscript.
    pub sub2: f64,
    /// σ₂₀ — delim1: delimiter target for display-style generalized
    /// fractions.
    pub delim1: f64,
    /// σ₂₁ — delim2: delimiter target for non-display generalized
    /// fractions.
    pub delim2: f64,
    /// σ₂₂ — axis height above the baseline.
    pub axis_height: f64,
    /// ξ₈ — default rule thickness.
    pub rule_thickness: f64,
    /// ξ₉ — big_op_spacing1: minimum gap below an upper limit.
    pub big_op_spacing1: f64,
    /// ξ₁₀ — big_op_spacing2: minimum gap above a lower limit.
    pub big_op_spacing2: f64,
    /// ξ₁₁ — big_op_spacing3: upper-limit gap floor including its depth.
    pub big_op_spacing3: f64,
    /// ξ₁₂ — big_op_spacing4: lower-limit gap floor including its height.
    pub big_op_spacing4: f64,
    /// ξ₁₃ — big_op_spacing5: padding above/below limit stacks.
    pub big_op_spacing5: f64,
    /// The display-size scale of `\sum`-class big operators. CM has no
    /// cmex-style size-variant glyphs, so the display variant is the
    /// authored glyph scaled by the cmex10 display/text height ratio
    /// (14 pt-class vs 10 pt-class: 1.4) — a calibration constant the Look
    /// Gallery judges (G0-3 "spike simplifications" work item).
    pub display_op_scale: f64,
    /// The display-size scale of `\int`-class operators. cmex10's display
    /// integral (char 0x5A, 2.222 em tall) is twice its text integral (char
    /// 0x52, 1.111 em), not the 1.4 of `\sum`; the pinned manim Reference
    /// measures `Tex(r"\int")` at 2.0 times `\textstyle\int`.
    pub display_integral_scale: f64,
    /// TeX's `delimiterfactor`/1000: a `\left…\right` delimiter must cover
    /// at least this fraction of twice the content's axis-distance.
    pub delimiter_factor: f64,
    /// TeX's `delimitershortfall` in ems (5 pt at 10 pt): the delimiter
    /// may fall short of full coverage by at most this much.
    pub delimiter_shortfall: f64,
    /// TeX's `nulldelimiterspace` in ems (1.2 pt at 10 pt): the width a
    /// null delimiter (`\left.`, either side of `\frac`) occupies.
    pub null_delimiter_space: f64,
    /// TeX's `scriptspace` in ems (0.5 pt at 10 pt): added to the width of
    /// every sub- and superscript box (§756), whatever the style.
    pub script_space: f64,
    /// `\baselineskip` in ems (12 pt at 10 pt): minimum baseline-to-
    /// baseline distance when stacking `\\`-separated lines.
    pub baseline_skip: f64,
    /// `\lineskip` in ems (1 pt at 10 pt): the gap used when boxes would
    /// otherwise touch.
    pub line_skip: f64,
    /// amsmath's `\jot` in ems (3 pt at 10 pt): `\openup\jot` widens both
    /// `\baselineskip` and `\lineskip` between the rows of the align class,
    /// which includes the Tex surface's own align* lines.
    pub jot: f64,
    /// The uniform-scaling ceiling of the delimiter mechanism (ADR-0005):
    /// beyond `1.25×` natural, drawn-path construction takes over (the
    /// drawn constructions land with the extensions bead; until then the
    /// engine keeps scaling uniformly and the seam is documented).
    pub delimiter_scale_ceiling: f64,
    /// The interword space of text islands, in ems, used when the face
    /// does not map a space glyph.
    pub fallback_space: f64,
}

/// The Computer Modern family at 10 pt, in ems: cmsy10's and cmex10's TFM
/// fontdimens, exact (see the module docs for provenance and validation).
/// This is the text-size table; [`MathConstants::at`] reads the others.
pub const CM: MathConstants = MathConstants {
    x_height: CMSY10.param(5),
    quad: CMSY10.param(6),
    num1: CMSY10.param(8),
    num2: CMSY10.param(9),
    num3: CMSY10.param(10),
    denom1: CMSY10.param(11),
    denom2: CMSY10.param(12),
    sup1: CMSY10.param(13),
    sup2: CMSY10.param(14),
    sup3: CMSY10.param(15),
    sub1: CMSY10.param(16),
    sub2: CMSY10.param(17),
    delim1: CMSY10.param(20),
    delim2: CMSY10.param(21),
    axis_height: CMSY10.param(22),
    rule_thickness: CMEX10.param(8),
    big_op_spacing1: CMEX10.param(9),
    big_op_spacing2: CMEX10.param(10),
    big_op_spacing3: CMEX10.param(11),
    big_op_spacing4: CMEX10.param(12),
    big_op_spacing5: CMEX10.param(13),
    display_op_scale: 1.4,
    display_integral_scale: 2.0,
    delimiter_factor: 0.901,
    delimiter_shortfall: 0.5,
    null_delimiter_space: 0.12,
    script_space: 0.05,
    baseline_skip: 1.2,
    line_skip: 0.1,
    jot: 0.3,
    delimiter_scale_ceiling: 1.25,
    fallback_space: 1.0 / 3.0,
};

/// The symbol (family 2) and extension (family 3) fonts TeX loads at
/// `style`'s size under LaTeX's article and amsmath, as `\fontname` reports
/// them: cmex7 also serves scriptscript size, at 5 pt.
#[must_use]
pub fn size_fonts(style: Style) -> (&'static TfmFont, &'static TfmFont) {
    match style {
        Style::Display | Style::Text => (&CMSY10, &CMEX10),
        Style::Script => (&CMSY7, &CMEX7),
        Style::ScriptScript => (&CMSY5, &CMEX7),
    }
}

impl MathConstants {
    /// The parameters TeX reads in `style`, in ems of that style's own
    /// fonts. The layout multiplies them by the style's size.
    ///
    /// σ₅…σ₂₂ come from [`size_fonts`]' symbol font and ξ₈…ξ₁₃ from its
    /// extension font. Everything else is not a fontdimen (spacing
    /// registers, scales, ceilings) and is kept from `self`.
    #[must_use]
    pub fn at(&self, style: Style) -> Self {
        let (sy, ex) = size_fonts(style);
        Self {
            x_height: sy.param(5),
            quad: sy.param(6),
            num1: sy.param(8),
            num2: sy.param(9),
            num3: sy.param(10),
            denom1: sy.param(11),
            denom2: sy.param(12),
            sup1: sy.param(13),
            sup2: sy.param(14),
            sup3: sy.param(15),
            sub1: sy.param(16),
            sub2: sy.param(17),
            delim1: sy.param(20),
            delim2: sy.param(21),
            axis_height: sy.param(22),
            rule_thickness: ex.param(8),
            big_op_spacing1: ex.param(9),
            big_op_spacing2: ex.param(10),
            big_op_spacing3: ex.param(11),
            big_op_spacing4: ex.param(12),
            big_op_spacing5: ex.param(13),
            ..*self
        }
    }

    /// Rule 18a's `(sup_drop, sub_drop)` for scripts set in style `t`, in
    /// ems of `t`'s symbol font. TeX reads them at the script size: from
    /// the script font below script style, else from the scriptscript
    /// font (tex.web §756), never from the base's own size.
    #[must_use]
    pub fn script_drops(&self, t: Style) -> (f64, f64) {
        let (sy, _) = size_fonts(match t {
            Style::Display | Style::Text | Style::Script => Style::Script,
            Style::ScriptScript => Style::ScriptScript,
        });
        (sy.param(18), sy.param(19))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_family_is_internally_consistent() {
        // Sanity relations that hold for the published CM values; a typo in
        // the table would trip one of these. (Read through black_box so the
        // relations are checked as runtime assertions.)
        let c = std::hint::black_box(CM);
        assert!(c.num1 > c.num2 && c.num2 < c.num3);
        assert!(c.denom1 > c.denom2);
        assert!(c.sup1 > c.sup2 && c.sup2 > c.sup3);
        assert!(c.sub2 > c.sub1);
        assert!(c.delim1 > c.delim2);
        assert!((c.axis_height - 0.25).abs() < 1e-9);
        assert!((c.rule_thickness - 0.04).abs() < 1e-5);
        assert!(c.x_height < 0.5 && c.x_height > 0.4);
        // cmsy7's and cmsy5's drops are the same lengths at 10 pt:
        // 2.4722 pt and 0.5 pt.
        let (sup7, sub7) = c.script_drops(Style::Script);
        let (sup5, sub5) = c.script_drops(Style::ScriptScript);
        assert!((sup7 * 0.7 - sup5 * 0.5).abs() < 1e-5);
        assert!((sub7 * 0.7 - sub5 * 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_text_table_is_cmsy10_and_cmex10_at_the_published_digits() {
        // tftopl's six-digit fontdimens for cmsy10 and cmex10. CM reads the
        // TFM by parameter number, so a misnumbered read trips this.
        for (name, published, exact) in [
            ("x_height", 0.430_555, CM.x_height),
            ("quad", 1.000_003, CM.quad),
            ("num1", 0.676_508, CM.num1),
            ("num2", 0.393_732, CM.num2),
            ("num3", 0.443_731, CM.num3),
            ("denom1", 0.685_951, CM.denom1),
            ("denom2", 0.344_841, CM.denom2),
            ("sup1", 0.412_892, CM.sup1),
            ("sup2", 0.362_892, CM.sup2),
            ("sup3", 0.288_889, CM.sup3),
            ("sub1", 0.150_000, CM.sub1),
            ("sub2", 0.247_217, CM.sub2),
            ("delim1", 2.389_999, CM.delim1),
            ("delim2", 1.010_000, CM.delim2),
            ("axis_height", 0.250_000, CM.axis_height),
            ("rule_thickness", 0.039_999, CM.rule_thickness),
            ("big_op_spacing1", 0.111_112, CM.big_op_spacing1),
            ("big_op_spacing2", 0.166_667, CM.big_op_spacing2),
            ("big_op_spacing3", 0.200_000, CM.big_op_spacing3),
            ("big_op_spacing4", 0.600_000, CM.big_op_spacing4),
            ("big_op_spacing5", 0.100_000, CM.big_op_spacing5),
        ] {
            assert!(
                (published - exact).abs() < 1e-6,
                "{name}: {published} vs {exact}"
            );
        }
        assert_eq!(CM.at(Style::Text), CM);
    }

    #[test]
    fn script_styles_read_the_optical_sizes_own_fontdimens() {
        // TeX's \fontdimen under article + amsmath (fm-giir): σ14 of
        // \scriptfont2 is 3.01779 pt (cmsy7 at 7 pt), its quad 8.19447 pt,
        // and ξ8 of \scriptfont3 and \scriptscriptfont3 is 0.33998 pt and
        // 0.24284 pt (cmex7 at 7 pt and at 5 pt). TeX truncates each scaled
        // fontdimen to a whole sp (tex.web §571) and \the prints five
        // digits, so the exact products agree within two sp.
        let sp2 = 2.0 / 65536.0;
        let pt = |v: f64, size: f64| v * size;
        let s = CM.at(Style::Script);
        let ss = CM.at(Style::ScriptScript);
        for (name, ours, tex) in [
            ("script sup2", pt(s.sup2, 7.0), 3.01779),
            ("script quad", pt(s.quad, 7.0), 8.19447),
            ("script rule", pt(s.rule_thickness, 7.0), 0.33998),
            ("scriptscript rule", pt(ss.rule_thickness, 5.0), 0.24284),
        ] {
            assert!(
                (ours - tex).abs() < sp2,
                "{name}: {ours} pt vs TeX {tex} pt"
            );
        }
        assert!((pt(ss.quad, 5.0) - 7.3612).abs() < 1e-4);
        // Text and display read cmsy10 and cmex10.
        assert_eq!(CM.at(Style::Display), CM.at(Style::Text));
    }
}
