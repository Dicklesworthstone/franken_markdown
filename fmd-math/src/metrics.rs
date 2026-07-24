//! The math-metrics table: TeX's Appendix-G parameter family, synthesized
//! for the bundled faces.
//!
//! **Provenance.** The values are the published TFM fontdimen families of
//! Computer Modern at 10 pt — cmsy10's σ₅…σ₂₂ and cmex10's ξ₈…ξ₁₃ (The
//! TeXbook, Appendix G; the parameters Appendix G consumes by name) —
//! expressed in ems of the design size. The G0-3 spike ratified this
//! *synthesis method* (franken_manim `docs/g0/G0-3-fmd-math-ratification.md`,
//! Verdict 1): compile the published family in as em constants and
//! **validate** against geometry decoded from the bundled faces by
//! fmd-font. The bundled CM Unicode faces measure within 0.13 % of the
//! published x-height and match the axis height exactly — they *are* the
//! Computer Modern the TFM family describes. The validation tests promoted
//! from the spike live in this crate's suite; per-face recalibration only
//! ever happens through the same measure-and-validate seam.
//!
//! **Scaling rule.** Every parameter is stored in ems of the text size and
//! is multiplied by the current style's size factor at use — the analogue
//! of TeX reading fontdimens from the current size's symbol font.

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
    /// σ₁₈ — sup_drop: superscript baseline drop from the base's top.
    pub sup_drop: f64,
    /// σ₁₉ — sub_drop: subscript baseline drop from the base's bottom.
    pub sub_drop: f64,
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
    /// TeX's `delimiterfactor`/1000: a `\left…\right` delimiter must cover
    /// at least this fraction of twice the content's axis-distance.
    pub delimiter_factor: f64,
    /// TeX's `delimitershortfall` in ems (5 pt at 10 pt): the delimiter
    /// may fall short of full coverage by at most this much.
    pub delimiter_shortfall: f64,
    /// TeX's `nulldelimiterspace` in ems (1.2 pt at 10 pt): the width a
    /// null delimiter (`\left.`) occupies.
    pub null_delimiter_space: f64,
    /// `\baselineskip` in ems (12 pt at 10 pt): minimum baseline-to-
    /// baseline distance when stacking `\\`-separated lines.
    pub baseline_skip: f64,
    /// `\lineskip` in ems (1 pt at 10 pt): the gap used when boxes would
    /// otherwise touch.
    pub line_skip: f64,
    /// The uniform-scaling ceiling of the delimiter mechanism (ADR-0005):
    /// beyond `1.25×` natural, drawn-path construction takes over (the
    /// drawn constructions land with the extensions bead; until then the
    /// engine keeps scaling uniformly and the seam is documented).
    pub delimiter_scale_ceiling: f64,
    /// The interword space of text islands, in ems, used when the face
    /// does not map a space glyph.
    pub fallback_space: f64,
}

/// The Computer Modern family at 10 pt, in ems (values as published for
/// cmsy10/cmex10; see the module docs for provenance and validation).
pub const CM: MathConstants = MathConstants {
    x_height: 0.430_555,
    quad: 1.000_003,
    num1: 0.676_508,
    num2: 0.393_732,
    num3: 0.443_731,
    denom1: 0.685_951,
    denom2: 0.344_841,
    sup1: 0.412_892,
    sup2: 0.362_892,
    sup3: 0.288_889,
    sub1: 0.150_000,
    sub2: 0.247_217,
    sup_drop: 0.386_108,
    sub_drop: 0.050_000,
    delim1: 2.389_999,
    delim2: 1.010_000,
    axis_height: 0.250_000,
    rule_thickness: 0.039_999,
    big_op_spacing1: 0.111_112,
    big_op_spacing2: 0.166_667,
    big_op_spacing3: 0.200_000,
    big_op_spacing4: 0.600_000,
    big_op_spacing5: 0.100_000,
    display_op_scale: 1.4,
    delimiter_factor: 0.901,
    delimiter_shortfall: 0.5,
    null_delimiter_space: 0.12,
    baseline_skip: 1.2,
    line_skip: 0.1,
    delimiter_scale_ceiling: 1.25,
    fallback_space: 1.0 / 3.0,
};

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
    }
}
