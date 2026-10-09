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

fn glyph_x(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
        .x
}

/// franken_manim fm-5wq.56: the text face carries every ligature glyph, so
/// text mode typesets the curly quotes and dashes instead of refusing them.
#[test]
fn text_ligature_glyphs_typeset_from_the_bundled_text_face() {
    let e = engine();
    for (source, ch) in [
        ("can't", '\u{2019}'),
        ("`a", '\u{2018}'),
        ("``a''", '\u{201C}'),
        ("``a''", '\u{201D}'),
        ("1--2", '\u{2013}'),
        ("a---b", '\u{2014}'),
    ] {
        let layout = e.typeset_text(source).expect(source);
        assert!(
            layout.glyphs.iter().any(|glyph| glyph.ch == ch),
            "{source}: no {ch:?} glyph"
        );
        assert!(
            !layout
                .glyphs
                .iter()
                .any(|glyph| glyph.ch == '\'' || glyph.ch == '`')
        );
    }
}

#[test]
fn radical_degree_follows_plain_tex_root() {
    // plain.tex's \r@@t: \mkern5mu \raise.6\dimen@ \copy\rootbox \mkern-10mu
    // \box0, with \dimen@ = \ht0 - \dp0 of the radical box. In display
    // style one mu is 1/18 em.
    let e = engine();
    let plain = e.typeset(r"\sqrt{x}", Style::Display).unwrap();
    let rooted = e.typeset(r"\sqrt[3]{x}", Style::Display).unwrap();
    let raise = 0.6 * (plain.height - plain.depth);
    assert!(
        (glyph_y(&rooted, '3') - raise).abs() < EPS,
        "degree at {} for .6(ht - dp) = {raise}",
        glyph_y(&rooted, '3')
    );
    let degree = e.typeset("3", Style::ScriptScript).unwrap().width;
    let shift = 5.0 / 18.0 + degree - 10.0 / 18.0;
    assert!(
        (glyph_x(&rooted, 'x') - glyph_x(&plain, 'x') - shift).abs() < EPS,
        "radicand moved {} for 5mu + w - 10mu = {shift}",
        glyph_x(&rooted, 'x') - glyph_x(&plain, 'x')
    );
}

#[test]
fn a_radical_box_encloses_its_sign() {
    // TeX packs the raised sign with the overbar (§737), so the box is as
    // deep as the sign's descent below the radicand, not just the radicand.
    let e = engine();
    let l = e.typeset(r"\sqrt{x}", Style::Display).unwrap();
    let contours = fmd_math::paths::resolve_paths(&e, &l).unwrap();
    let ink_bottom = contours
        .iter()
        .flat_map(|contour| {
            std::iter::once(contour.start).chain(contour.segments.iter().map(|segment| {
                match *segment {
                    fmd_math::PathSeg::Line { to } | fmd_math::PathSeg::Quad { to, .. } => to,
                }
            }))
        })
        .map(|(_, y)| y)
        .fold(f64::INFINITY, f64::min);
    assert!(ink_bottom < 0.0, "the sign descends below the baseline");
    assert!(
        -l.depth <= ink_bottom + 1e-9,
        "depth {} leaves ink at {ink_bottom} outside the box",
        l.depth
    );
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
fn display_integrals_take_the_cmex_display_size() {
    // cmex10's display integral is twice the text one (2.222 em vs 1.111 em);
    // the `\sum` class keeps its 1.4.
    let e = engine();
    for op in [r"\int", r"\oint", r"\iint"] {
        let display = e.typeset(op, Style::Display).unwrap();
        let text = e.typeset(op, Style::Text).unwrap();
        let ratio = display.glyphs[0].size / text.glyphs[0].size;
        assert!(
            (ratio - CM.display_integral_scale).abs() < EPS,
            "{op}: {ratio}"
        );
    }
    assert!((CM.display_integral_scale - 2.0).abs() < EPS);
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

    // `center` graduated from the tier-2 vocabulary: inside mathematics it
    // is now the *precise* text-mode-only refusal rather than a pending
    // construct, and the ratchet no longer counts it.
    let err = e
        .typeset(r"\begin{center} x \end{center}", Style::Display)
        .unwrap_err();
    assert_eq!(err.unsupported_construct(), None);
    assert!(
        err.to_string()
            .contains("text-mode line-alignment environment"),
        "{err}"
    );

    // The fm-j5t symbol tranche: every graduated symbol resolves to a real
    // bundled-face glyph in one string — no silent drops, no substitutions.
    let syms = e
        .typeset(
            r"a \nmid b \circlearrowleft \circlearrowright \i \j",
            Style::Display,
        )
        .unwrap();
    assert_eq!(syms.glyphs.len(), 7, "a, nmid, b, two arrows, dotless i, j");
    let op = e.typeset(r"\oiint_S f", Style::Display).unwrap();
    assert!(
        op.glyphs.iter().any(|g| g.ch == '∯'),
        "the surface integral operator renders"
    );

    // The dot accents (fm-j5t): \dddot is a row of three dot marks over
    // the base, \ddddot four — amsmath's own construction.
    let dots3 = e.typeset(r"\dddot x", Style::Display).unwrap();
    assert_eq!(dots3.glyphs.len(), 4, "x and three dots");
    let dots4 = e.typeset(r"\ddddot x", Style::Display).unwrap();
    assert_eq!(dots4.glyphs.len(), 5, "x and four dots");
    let x_top = dots3.glyphs.iter().find(|g| g.ch == 'x').unwrap().y;
    assert!(
        dots3
            .glyphs
            .iter()
            .filter(|g| g.ch != 'x')
            .all(|g| g.y > x_top),
        "dots sit above the base"
    );

    // The labeled extensible arrows (fm-j5t): a drawn band stretched to
    // its script-style label, label riding above as a limit; \xmapsto
    // adds the origin bar (a second closed contour).
    let arrow = e.typeset(r"\xrightarrow{f} x", Style::Display).unwrap();
    assert_eq!(arrow.paths.len(), 1, "one drawn arrow band");
    let f = arrow.glyphs.iter().find(|g| g.ch == 'f').unwrap();
    assert!((f.size - 0.7).abs() < 1e-9, "script-size label");
    assert!(f.y > 0.2, "label rides above the axis band");
    let mapsto = e.typeset(r"\xmapsto{f}", Style::Display).unwrap();
    assert_eq!(
        mapsto.paths[0].contours.len(),
        2,
        "arrow plus the origin bar"
    );
    let below = e.typeset(r"\xrightarrow[g]{f} x", Style::Display).unwrap();
    let g = below.glyphs.iter().find(|gl| gl.ch == 'g').unwrap();
    assert!(g.y < 0.0, "below-label rides under the band");

    // \doublespacing (fm-j5t): the declaration stretches subsequent
    // stacked baselines by setspace's 1.667.
    let single = e.typeset_text("a \\\\ b").unwrap();
    let double = e.typeset_text("\\doublespacing a \\\\ b").unwrap();
    let gap = |l: &fmd_math::Layout| {
        let a = l.glyphs.iter().find(|g| g.ch == 'a').unwrap().y;
        let b = l.glyphs.iter().find(|g| g.ch == 'b').unwrap().y;
        a - b
    };
    assert!(
        (gap(&double) / gap(&single) - 1.667).abs() < 1e-6,
        "stretched skip: {} vs {}",
        gap(&double),
        gap(&single)
    );

    // A construct still outside the tier keeps the named-error contract:
    // precise, tier-tagged, never silent.
    let err = e.typeset(r"\dx", Style::Display).unwrap_err();
    assert_eq!(err.unsupported_construct(), Some(r"\dx"));
}

#[test]
fn a_line_break_unskips_the_space_before_it() {
    // LaTeX's `\\` begins with `\unskip`, so a space before the break does
    // not widen the line it ends: every glyph of a centered block sits
    // exactly where it would without that space. (A space after `\\` is
    // already swallowed, as LaTeX's star/option look-ahead swallows it.)
    let e = engine();
    let positions = |source: &str| -> Vec<(char, f64, f64)> {
        e.typeset_text(source)
            .unwrap()
            .glyphs
            .iter()
            .map(|g| (g.ch, g.x, g.y))
            .collect()
    };
    let bare = positions(r"\begin{center}ab\\cd\end{center}");
    for spaced in [
        r"\begin{center}ab \\cd\end{center}",
        r"\begin{center}ab \\ cd\end{center}",
        r"\begin{flushright}ab \\cd\end{flushright}",
    ] {
        let expected = if spaced.contains("flushright") {
            positions(r"\begin{flushright}ab\\cd\end{flushright}")
        } else {
            bare.clone()
        };
        let got = positions(spaced);
        assert_eq!(got.len(), expected.len(), "{spaced}");
        for (g, want) in got.iter().zip(&expected) {
            assert_eq!(g.0, want.0, "{spaced}");
            assert!(
                (g.1 - want.1).abs() < EPS && (g.2 - want.2).abs() < EPS,
                "{spaced}: {g:?} vs {want:?}"
            );
        }
    }
    // Prose line breaks outside an environment unskip too.
    let prose = e.typeset_text(r"ab \\cd").unwrap();
    let plain = e.typeset_text(r"ab\\cd").unwrap();
    let xs = |l: &Layout| l.glyphs.iter().map(|g| g.x).collect::<Vec<_>>();
    assert_eq!(xs(&prose), xs(&plain));
}

/// Rule 17 (tex.web §755): a math character with no subscript is followed
/// by a kern of its italic correction, so `f(` clears the f's overhang. A
/// subscripted character keeps δ out of its width, and a character inside a
/// word of a text face (TeX's math_text_char) gets none.
#[test]
fn italic_correction_kerns_follow_lone_math_characters() {
    use fmd_math::faces::glyph_metrics;
    let e = engine();
    let metrics = |l: &Layout, i: usize| {
        let g = &l.glyphs[i];
        glyph_metrics(e.faces().font(g.face).expect("glyph face"), g.gid)
    };

    let italic = e.typeset("f(", Style::Text).unwrap();
    let m = metrics(&italic, 0);
    assert!(
        m.italic > 0.0,
        "the math-italic f overhangs its advance: {m:?}"
    );
    let gap = italic.glyphs[1].x - italic.glyphs[0].x;
    assert!(
        (gap - (m.advance + m.italic)).abs() < EPS,
        "f( advances by the f's width plus its italic correction: {gap} vs {m:?}"
    );

    let subscripted = e.typeset("f_i", Style::Text).unwrap();
    let gap = subscripted.glyphs[1].x - subscripted.glyphs[0].x;
    assert!(
        (gap - metrics(&subscripted, 0).advance).abs() < EPS,
        "a subscript attaches at the nucleus width, without δ: {gap}"
    );

    // A group holding one Ord character is that character (tex.web §1186):
    // `{f}(` kerns like `f(`, and `{x}^2` places its script like `x^2`.
    let grouped = e.typeset("{f}(", Style::Text).unwrap();
    let gap = grouped.glyphs[1].x - grouped.glyphs[0].x;
    assert!(
        (gap - (m.advance + m.italic)).abs() < EPS,
        "{{f}}( is f(: {gap} vs {m:?}"
    );
    let xs = |l: &Layout| l.glyphs.iter().map(|g| (g.x, g.y)).collect::<Vec<_>>();
    assert_eq!(
        xs(&e.typeset("{x}^2", Style::Text).unwrap()),
        xs(&e.typeset("x^2", Style::Text).unwrap())
    );
    // ...and an accent over `{v}` takes the math-character skew of `\vec v`.
    assert_eq!(
        xs(&e.typeset(r"\vec{v}", Style::Text).unwrap()),
        xs(&e.typeset(r"\vec v", Style::Text).unwrap())
    );

    // `\mathrm{f}(` is an upright f before an upright `(` of the same family:
    // TeX's math_text_char in a font with interword space, so δ = 0.
    let word = e.typeset(r"\mathrm{f}(", Style::Text).unwrap();
    let upright = metrics(&word, 0);
    assert!(
        upright.italic > 0.0,
        "the upright f overhangs too: {upright:?}"
    );
    let gap = word.glyphs[1].x - word.glyphs[0].x;
    assert!(
        (gap - upright.advance).abs() < EPS,
        "inside a word of the text face the italic correction is zero: {gap} vs {upright:?}"
    );
    // Within `\mathrm{ff}` the first f is such a word character: only the
    // font's kern separates the two.
    let ff = e.typeset(r"\mathrm{ff}", Style::Text).unwrap();
    let (first, second) = (&ff.glyphs[0], &ff.glyphs[1]);
    let font = e.faces().font(first.face).expect("glyph face");
    let kern = fmd_math::faces::kern_em(font, first.gid, second.gid);
    let gap = second.x - first.x;
    assert!(
        (gap - (upright.advance + kern)).abs() < EPS,
        "\\mathrm{{ff}}: {gap} vs {upright:?} + kern {kern}"
    );

    // A `\left…\right` inner is a sub-mlist, not a clean box: its lone f
    // keeps δ before the closing delimiter.
    let fenced = e.typeset(r"\left( f \right)", Style::Text).unwrap();
    let gap = fenced.glyphs[2].x - fenced.glyphs[1].x;
    assert!(
        (gap - (m.advance + m.italic)).abs() < EPS,
        "\\left( f \\right): {gap} vs {m:?}"
    );
}

#[test]
fn ellipses_are_three_thin_spaced_punctuation_dots() {
    // Plain TeX: `\cdots` is `\mathinner{\cdotp\cdotp\cdotp}` and `\ldots`
    // the same with `\ldotp`. Punct-punct glue is a thin space (3 mu) in
    // display and text styles and nothing in script styles.
    let e = engine();
    for (command, dot) in [(r"\cdots", '⋅'), (r"\ldots", '.')] {
        let xs = |layout: &Layout| -> Vec<f64> {
            layout
                .glyphs
                .iter()
                .filter(|glyph| glyph.ch == dot)
                .map(|glyph| glyph.x)
                .collect()
        };
        let display = e.typeset(command, Style::Display).unwrap();
        let script = e
            .typeset(&format!("x_{{{command}}}"), Style::Display)
            .unwrap();
        let (d, s) = (xs(&display), xs(&script));
        assert_eq!(display.glyphs.len(), 3, "{command}: {display:?}");
        assert_eq!(s.len(), 3, "{command}: {script:?}");
        let advance = (s[1] - s[0]) / glyph_size(&script, dot);
        assert!(
            ((s[2] - s[1]) - (s[1] - s[0])).abs() < EPS,
            "{command}: {s:?}"
        );
        for pair in d.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (gap - (advance + 1.0 / 6.0)).abs() < EPS,
                "{command}: gap {gap} vs advance {advance} + 3 mu"
            );
        }
    }
}

#[test]
fn tbinom_and_dbinom_force_their_styles() {
    // amsmath: \tbinom is \genfrac(){0pt}{1} (text style), \dbinom is
    // \genfrac(){0pt}{0} (display style), whatever the surroundings.
    let e = engine();
    let height = |source: &str, style: Style| {
        let layout = e
            .typeset(source, style)
            .unwrap_or_else(|err| panic!("`{source}`: {err}"));
        layout.height + layout.depth
    };
    let display = height(r"\binom{N}{4}", Style::Display);
    let text = height(r"\binom{N}{4}", Style::Text);
    assert!(text < display, "text {text} vs display {display}");
    assert!((height(r"\tbinom{N}{4}", Style::Display) - text).abs() < EPS);
    assert!((height(r"\dbinom{N}{4}", Style::Text) - display).abs() < EPS);
}

#[test]
fn math_lines_open_up_by_jot_and_text_lines_do_not() {
    // The Tex surface's top-level `\\` rows are align* rows: amsmath's
    // \openup\jot puts their baselines \baselineskip + \jot = 15 pt apart at
    // 10 pt. A TexText paragraph keeps plain \baselineskip, 12 pt.
    let e = engine();
    let math = e.typeset(r"a \\ b", Style::Display).unwrap();
    let rows = glyph_y(&math, 'a') - glyph_y(&math, 'b');
    assert!(
        (rows - (CM.baseline_skip + CM.jot)).abs() < EPS,
        "math rows {rows}"
    );
    let text = e.typeset_text(r"a\\b").unwrap();
    let lines = glyph_y(&text, 'a') - glyph_y(&text, 'b');
    assert!((lines - CM.baseline_skip).abs() < EPS, "text lines {lines}");
}
