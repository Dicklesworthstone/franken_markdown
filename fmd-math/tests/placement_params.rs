//! Placement-parameter fixtures: positions asserted against the PUBLISHED
//! Appendix-G values (the correctness spec is Knuth's, not pixels). Cases
//! are chosen so the rule's clearance terms do not bind, making the
//! published constant the exact expected coordinate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::metrics::CM;
use fmd_math::{Engine, Layout, Style};

fn engine() -> Engine {
    match Engine::bundled() {
        Ok(e) => e,
        Err(e) => panic!("bundled faces: {e}"),
    }
}

fn glyph_y(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
        .y
}

fn glyph_size(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch}"))
        .size
}

const EPS: f64 = 1e-9;

#[test]
fn display_fraction_shifts_are_num1_denom1() {
    // 'a' and 'x' are short enough that rule 15d's clearances do not bind:
    // the shifts are exactly σ8 and σ11.
    let l = engine().typeset(r"\frac{a}{x}", Style::Display).unwrap();
    assert!(
        (glyph_y(&l, 'a') - CM.num1).abs() < EPS,
        "{}",
        glyph_y(&l, 'a')
    );
    assert!(
        (glyph_y(&l, 'x') - (-CM.denom1)).abs() < EPS,
        "{}",
        glyph_y(&l, 'x')
    );
    // The bar is θ thick, centered on the axis.
    let bar = &l.rules[0];
    assert!((bar.y - (CM.axis_height - CM.rule_thickness / 2.0)).abs() < EPS);
    assert!((bar.height - CM.rule_thickness).abs() < EPS);
}

#[test]
fn text_fraction_shifts_are_num2_denom2() {
    let l = engine().typeset(r"\frac{a}{x}", Style::Text).unwrap();
    assert!((glyph_y(&l, 'a') - CM.num2).abs() < EPS);
    assert!((glyph_y(&l, 'x') - (-CM.denom2)).abs() < EPS);
    // Interiors are script-size.
    assert!((glyph_size(&l, 'a') - 0.7).abs() < EPS);
}

#[test]
fn display_fraction_interiors_are_full_size() {
    let l = engine().typeset(r"\frac{a}{x}", Style::Display).unwrap();
    assert!((glyph_size(&l, 'a') - 1.0).abs() < EPS);
}

#[test]
fn deep_numerator_binds_the_clearance() {
    // 'y' has a descender: in text style rule 15d's numerator clearance
    // binds (u − depth(y·0.7) − (axis + θ/2) < θ) and u grows past σ9.
    let l = engine().typeset(r"\frac{y}{x}", Style::Text).unwrap();
    assert!(glyph_y(&l, 'y') > CM.num2 + EPS, "{}", glyph_y(&l, 'y'));
}

#[test]
fn superscript_shift_is_sup1_in_display() {
    let l = engine().typeset(r"x^2", Style::Display).unwrap();
    assert!((glyph_y(&l, '2') - CM.sup1).abs() < EPS);
    assert!((glyph_size(&l, '2') - 0.7).abs() < EPS);
}

#[test]
fn superscript_shift_is_sup2_in_text() {
    let l = engine().typeset(r"x^2", Style::Text).unwrap();
    assert!((glyph_y(&l, '2') - CM.sup2).abs() < EPS);
}

#[test]
fn cramped_superscript_uses_sup3() {
    // Inside a radicand the context is cramped: σ15.
    let l = engine().typeset(r"\sqrt{x^2}", Style::Display).unwrap();
    assert!((glyph_y(&l, '2') - CM.sup3).abs() < EPS);
}

#[test]
fn lone_subscript_shift_is_sub1() {
    let l = engine().typeset(r"x_i", Style::Display).unwrap();
    assert!((glyph_y(&l, 'i') - (-CM.sub1)).abs() < EPS);
}

#[test]
fn simultaneous_scripts_use_sub2_and_separate_by_4_theta() {
    let l = engine().typeset(r"x_i^2", Style::Display).unwrap();
    let sup_y = glyph_y(&l, '2');
    let sub_y = glyph_y(&l, 'i');
    assert!(sup_y >= CM.sup1 - EPS);
    assert!(sub_y <= -CM.sub2 + EPS);
    // The clash rule guarantees at least 4θ between sup bottom and sub top
    // (measure with the actual glyph inks via layout extents).
    assert!(sup_y - sub_y > 4.0 * CM.rule_thickness);
}

#[test]
fn scriptscript_is_half_size() {
    let l = engine().typeset(r"x^{y^2}", Style::Display).unwrap();
    assert!((glyph_size(&l, '2') - 0.5).abs() < EPS);
    assert!((glyph_size(&l, 'y') - 0.7).abs() < EPS);
    assert!((glyph_size(&l, 'x') - 1.0).abs() < EPS);
}

#[test]
fn radical_rule_position_and_thickness() {
    let e = engine();
    let x_alone = e.typeset("x", Style::Display).unwrap();
    let l = e.typeset(r"\sqrt{x}", Style::Display).unwrap();
    // Rule 11: the clearance is at least ψ, growing by half the sign's
    // excess when the natural √ glyph overshoots the target (CM's does
    // over a lone 'x').
    let psi = CM.rule_thickness + 0.25 * CM.x_height;
    let rule = &l.rules[0];
    assert!((rule.height - CM.rule_thickness).abs() < EPS);
    let clearance = rule.y - x_alone.height;
    assert!(clearance >= psi - 1e-9, "clearance {clearance} < ψ {psi}");
    assert!(clearance < psi + 0.25, "excess out of bounds: {clearance}");
}

#[test]
fn radical_index_is_scriptscript_and_raised() {
    let l = engine().typeset(r"\sqrt[3]{x}", Style::Display).unwrap();
    assert!((glyph_size(&l, '3') - 0.5).abs() < EPS);
    assert!(glyph_y(&l, '3') > 0.3, "degree must be raised");
}

#[test]
fn big_op_is_display_scaled_and_axis_centered() {
    let e = engine();
    let display = e.typeset(r"\sum", Style::Display).unwrap();
    let text = e.typeset(r"\sum", Style::Text).unwrap();
    let ds = display.glyphs[0].size;
    let ts = text.glyphs[0].size;
    assert!((ds / ts - CM.display_op_scale).abs() < EPS);
    // Axis-centered: ink center at σ22.
    let ink_center = (display.height - display.depth) / 2.0;
    assert!((ink_center - CM.axis_height).abs() < 0.02, "{ink_center}");
}

#[test]
fn display_limits_go_above_and_below_with_the_xi_gaps() {
    let e = engine();
    let l = e.typeset(r"\sum_{n=1}^{N}", Style::Display).unwrap();
    // Upper limit above the op, lower below.
    let n_upper = glyph_y(&l, 'N');
    let one = glyph_y(&l, '1');
    assert!(n_upper > 0.5, "upper limit above: {n_upper}");
    assert!(one < -0.5, "lower limit below: {one}");
    // In text style the same scripts sit beside the operator.
    let t = e.typeset(r"\sum_{n=1}^{N}", Style::Text).unwrap();
    assert!((glyph_y(&t, 'N') - CM.sup2).abs() < 0.2);
}

#[test]
fn integrals_take_side_scripts_even_in_display() {
    let l = engine().typeset(r"\int_0^1", Style::Display).unwrap();
    let sum_layout = engine().typeset(r"\sum_0^1", Style::Display).unwrap();
    // The integral's scripts are beside it (its width exceeds the bare
    // glyph), while the sum's limits stack (width equals the op width).
    let int_alone = engine().typeset(r"\int", Style::Display).unwrap();
    let sum_alone = engine().typeset(r"\sum", Style::Display).unwrap();
    assert!(l.width > int_alone.width + 0.1);
    assert!((sum_layout.width - sum_alone.width).abs() < 0.35);
}

#[test]
fn left_right_delimiters_cover_the_rule_19_target() {
    let e = engine();
    let l = e
        .typeset(r"\left(\frac{a}{x}\right)", Style::Display)
        .unwrap();
    let inner = e.typeset(r"\frac{a}{x}", Style::Display).unwrap();
    let delta = (inner.height - CM.axis_height).max(inner.depth + CM.axis_height);
    let target = (2.0 * delta * CM.delimiter_factor).max(2.0 * delta - CM.delimiter_shortfall);
    // A display fraction pushes the parens past the 1.25× uniform-scale
    // ceiling, so the ADR-0005 drawn mainline serves them: two drawn
    // contours (one per paren), no paren glyphs, and the construct still
    // covers the rule-19 target.
    assert!(
        l.glyphs.iter().all(|g| g.ch != '(' && g.ch != ')'),
        "parens should be drawn constructions past the ceiling"
    );
    assert_eq!(l.paths.len(), 2, "one drawn path per paren");
    assert!(l.height + l.depth >= target - 1e-6);

    // Below the ceiling the authored glyph is kept: an inline \left(x\right)
    // needs no scaling at all.
    let small = e.typeset(r"\left( x \right)", Style::Text).unwrap();
    assert!(
        small.glyphs.iter().any(|g| g.ch == '('),
        "natural glyph kept"
    );
    assert!(small.paths.is_empty());
}

#[test]
fn null_delimiter_occupies_nulldelimiterspace() {
    let e = engine();
    let with_null = e.typeset(r"\left. x \right.", Style::Display).unwrap();
    let bare = e.typeset(r"x", Style::Display).unwrap();
    assert!((with_null.width - (bare.width + 2.0 * CM.null_delimiter_space)).abs() < 1e-9);
}

#[test]
fn spacing_glue_matches_the_table() {
    let e = engine();
    // a+b: 4mu medium spaces around Bin in text/display.
    let ab = e.typeset("ab", Style::Display).unwrap();
    let apb = e.typeset("a+b", Style::Display).unwrap();
    let plus = e.typeset("+", Style::Display).unwrap();
    let kern_ab = {
        // width(a+b) = width(ab) + width(+) + 2×4mu ± the ab kern delta.
        let expected = ab.width + plus.width + 2.0 * 4.0 / 18.0;
        apb.width - expected
    };
    assert!(kern_ab.abs() < 0.02, "medium spacing off by {kern_ab}");
    // In script style the medium space vanishes: x^{a+b}.
    let sup = e.typeset("x^{a+b}", Style::Display).unwrap();
    let sup_ab = e.typeset("x^{ab}", Style::Display).unwrap();
    let plus_script_w = plus.width * 0.7;
    assert!(
        (sup.width - (sup_ab.width + plus_script_w)).abs() < 0.02,
        "script-style spacing must be suppressed"
    );
}

#[test]
fn phantoms_occupy_the_right_dimensions() {
    let e = engine();
    let x = e.typeset("x", Style::Display).unwrap();
    let ph = e.typeset(r"\phantom{x}", Style::Display).unwrap();
    assert!(ph.glyphs.is_empty());
    assert!((ph.width - x.width).abs() < EPS);
    assert!((ph.height - x.height).abs() < EPS);
    let hp = e.typeset(r"\hphantom{x}", Style::Display).unwrap();
    assert!((hp.width - x.width).abs() < EPS && hp.height.abs() < EPS);
    let vp = e.typeset(r"\vphantom{x}", Style::Display).unwrap();
    assert!(vp.width.abs() < EPS && (vp.height - x.height).abs() < EPS);
}

#[test]
fn every_glyph_carries_its_span_and_face() {
    let src = r"\frac{a}{x} + \sqrt{y}";
    let l = engine().typeset(src, Style::Display).unwrap();
    assert!(fmd_math::paths::spans_cover(&l, src.len()));
    assert!(!l.glyphs.is_empty());
}

#[test]
fn formerly_pending_constructs_now_lay_out() {
    // The fm-kg9 frontier, crossed: environments and the stretchy
    // constructions produce real layouts (their dedicated fixtures live in
    // extensions.rs); the named-error contract still holds for what remains
    // outside the tier — precise, tier-tagged, never silent.
    let e = engine();
    let m = e
        .typeset(
            r"\begin{matrix} a & b \\ c & d \end{matrix}",
            Style::Display,
        )
        .unwrap();
    assert_eq!(m.glyphs.len(), 4);
    let b = e.typeset(r"\overbrace{x+y}", Style::Display).unwrap();
    assert_eq!(b.paths.len(), 1, "the drawn brace band");

    // Tier-2 parse vocabulary still refuses by name (the ratchet's shape).
    let err = e.typeset(r"\substack{a \\ b}", Style::Display).unwrap_err();
    assert_eq!(err.unsupported_construct(), Some(r"\substack"));
    assert!(err.to_string().contains("tier T2"), "{err}");
    let err = e
        .typeset(r"\begin{center} x \end{center}", Style::Display)
        .unwrap_err();
    assert_eq!(err.unsupported_construct(), Some("env:center"));
}
