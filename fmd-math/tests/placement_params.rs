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
    // cmex10's display summation (char 0x58) and its text one (0x50) are two
    // designed glyphs, not one scaled; their TFM boxes are 1.6 and 1.0 em.
    let ds = display.height + display.depth;
    let ts = text.height + text.depth;
    assert!(
        (ds - 1.6).abs() < 1e-4 && (ts - 1.0).abs() < 1e-4,
        "{ds} {ts}"
    );
    assert_eq!(display.glyphs[0].face, fmd_math::faces::FACE_MATH_EXTENSION);
    assert_ne!(display.glyphs[0].gid, text.glyphs[0].gid);
    // Axis-centered (tex.web §749): the box's center at σ22.
    for layout in [&display, &text] {
        let center = (layout.height - layout.depth) / 2.0;
        assert!((center - CM.axis_height).abs() < 1e-9, "{center}");
    }
}

#[test]
fn display_integrals_take_the_cmex_display_size() {
    // cmex10's display integral is twice the text one (2.222 em vs 1.111 em);
    // the `\sum` class keeps its 1.4.
    let e = engine();
    for op in [r"\int", r"\oint", r"\iint"] {
        let display = e.typeset(op, Style::Display).unwrap();
        // amsmath builds `\iint` from two `\int`s; cmex has no single
        // character, so it keeps the fallback face's glyph at the display
        // ratio (the glyph assertion below is for `\int` and `\oint`).
        let text = e.typeset(op, Style::Text).unwrap();
        // The designed display glyph (cmex 0x5A, 0x49) against the text one
        // (0x52, 0x48): ink 2.222 em against 1.111 em.
        let ratio = (display.height + display.depth) / (text.height + text.depth);
        assert!(
            (ratio - CM.display_integral_scale).abs() < 2e-3,
            "{op}: {ratio}"
        );
        assert!(
            op == r"\iint"
                || display
                    .glyphs
                    .iter()
                    .all(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION
                        && (g.size - 1.0).abs() < EPS),
            "{op}: cmex's own display glyph, unscaled"
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

/// Rule 18a reads sup_drop and sub_drop from the symbol font of the size the
/// scripts are set at (tex.web §756): cmsy7's 0.353171 × 7 pt and
/// 0.071428 × 7 pt, not cmsy10's at the base's size. Each value is real
/// TeX's `\showbox` under `article` + `amsmath` (franken_manim fm-4ptw), in
/// ems of 10 pt: where the drop binds, the scripts sit exactly there.
#[test]
fn script_drops_are_read_at_the_script_size() {
    let e = engine();
    // Display integral with limits: the superscript 11.13903 pt up and δ =
    // 4.44444 pt right of the subscript, which is 9.11122 pt down.
    let int = e.typeset(r"\int_0^1 x", Style::Display).unwrap();
    let (sup, sub) = (glyph_y(&int, '1'), glyph_y(&int, '0'));
    assert!((sup - 1.113_903).abs() < 1e-4, "\\int superscript at {sup}");
    assert!((sub + 0.911_122).abs() < 1e-4, "\\int subscript at {sub}");
    let delta = glyph_x(&int, '1') - glyph_x(&int, '0');
    assert!((delta - 0.444_444).abs() < 1e-4, "\\int δ {delta}");
    // A group nucleus: 7.5 pt tall, so its superscript sits 7.5 - 2.47220 =
    // 5.0278 pt up; 2.5 pt deep, so its subscript sits 2.5 + 0.5 = 3.0 pt down.
    let sup = glyph_y(&e.typeset(r"{(a)}^2", Style::Display).unwrap(), '2');
    assert!((sup - 0.502_78).abs() < 1e-4, "{{(a)}}^2 at {sup}");
    let sub = glyph_y(&e.typeset(r"{(a)}_2", Style::Text).unwrap(), '2');
    assert!((sub + 0.3).abs() < 1e-4, "{{(a)}}_2 at {sub}");
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
    // A display fraction needs more than the 1.0 em small paren, so TeX
    // takes a cmex10 successor: each paren is cmex's own glyph (no drawn
    // path, no scaling), and the construct still covers the rule-19 target.
    let parens: Vec<_> = l
        .glyphs
        .iter()
        .filter(|g| g.ch == '(' || g.ch == ')')
        .collect();
    assert_eq!(parens.len(), 2, "one cmex glyph per paren");
    assert!(
        parens
            .iter()
            .all(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION && (g.size - 1.0).abs() < EPS),
        "cmex successors, unscaled"
    );
    assert!(l.paths.is_empty());
    assert!(l.height + l.depth >= target - 1e-6);

    // The small variant is the roman face's own paren: an inline
    // \left(x\right) needs nothing larger.
    let small = e.typeset(r"\left( x \right)", Style::Text).unwrap();
    assert!(
        small
            .glyphs
            .iter()
            .any(|g| g.ch == '(' && g.face == fmd_math::faces::FACE_REGULAR),
        "natural glyph kept"
    );
    assert!(small.paths.is_empty());
}

/// The vertical ink extent of the glyphs `keep` selects, ems.
fn glyph_ink_extent(
    e: &Engine,
    l: &Layout,
    keep: impl Fn(&fmd_math::PlacedGlyph) -> bool,
) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for g in l.glyphs.iter().filter(|g| keep(g)) {
        let font = e.faces().font(g.face).expect("placed face");
        let bbox = font.glyph_bbox(g.gid).expect("inked glyph");
        let s = g.size / f64::from(font.units_per_em);
        lo = lo.min(g.y + f64::from(bbox[1]) * s);
        hi = hi.max(g.y + f64::from(bbox[3]) * s);
    }
    (lo, hi)
}

#[test]
fn delimiters_take_tex_variant_sizes() {
    // Real TeX (LaTeX with amsmath, 10 pt) sets each of these delimiters at
    // a cmex10 variant or extensible size, which here also bounds the box:
    // height = axis + total / 2, depth = total / 2 - axis. The totals in ems
    // are the TeX boxes' height plus depth over 10 pt. The axis-centred
    // delimiter tops every box, so the height pins its size; the depth
    // need only cover it, since content can reach lower here than in TeX
    // (the face's script k in \binom dips below the baseline, where
    // cmmi7's k has no depth).
    use Style::{Display, Text};
    let e = engine();
    for (tex, style, total) in [
        (
            r"\left[\begin{array}{c}\quad \\\end{array}\right]",
            Display,
            1.2,
        ),
        (
            r"\left[\begin{array}{c}\quad \\\quad \\\end{array}\right]",
            Display,
            2.4,
        ),
        (
            r"\left[\begin{array}{c}\quad \\\quad \\\quad \\\end{array}\right]",
            Display,
            3.6,
        ),
        (
            r"\left[\begin{array}{c}\quad \\\quad \\\quad \\\quad \\\quad \\\end{array}\right]",
            Display,
            6.0,
        ),
        (r"\begin{pmatrix}1\\2\\3\end{pmatrix}", Display, 3.6),
        (
            r"\left\{\begin{matrix}a\\b\\c\\d\end{matrix}\right.",
            Display,
            4.8,
        ),
        (
            r"\left\lfloor\begin{matrix}a\\b\\c\end{matrix}\right\rfloor",
            Display,
            3.6,
        ),
        (r"\binom{n}{k}", Text, 1.2),
        (r"\binom{n}{k}", Display, 2.4),
        (r"\Big|", Text, 1.8),
    ] {
        let l = e.typeset(tex, style).unwrap();
        let half = l.height - CM.axis_height;
        assert!(
            (half - total / 2.0).abs() < 1e-6 && l.depth + CM.axis_height > total / 2.0 - 1e-6,
            "{tex} in {style:?}: {} + {} against TeX's {total} em",
            l.height,
            l.depth
        );
    }
    // A fraction in parentheses: TeX's 1.8 em variant, cmex's \Big paren
    // (its ink is the TFM box to Type 1 rounding). The box's depth is the
    // fraction's.
    let l = e
        .typeset(r"\left(\frac{a}{x}\right)", Style::Display)
        .unwrap();
    for paren in ['(', ')'] {
        let (lo, hi) = glyph_ink_extent(&e, &l, |g| g.ch == paren);
        assert!((hi - lo - 1.8).abs() < 2e-3, "{paren}: {lo}..{hi}");
        assert!(((hi + lo) / 2.0 - CM.axis_height).abs() < 2e-3);
    }
    // Angles have no extensible recipe: past five rows TeX's angle stays at
    // its largest variant, 3.0 em, while the matrix grows.
    let l = e
        .typeset(
            r"\left\langle\begin{matrix}a\\b\\c\\d\\e\end{matrix}\right\rangle",
            Style::Display,
        )
        .unwrap();
    let (lo, hi) = glyph_ink_extent(&e, &l, |g| g.ch == '⟨');
    assert!((hi - lo - 3.0).abs() < 2e-3, "angle: {lo}..{hi}");
    assert!(l.height + l.depth > 5.0);
}

#[test]
fn delimiters_take_tex_box_widths() {
    // Real TeX's \wd of each box (LaTeX with amsmath, 10 pt, in ems). The
    // content is digits and array columns, whose widths fmd-math already
    // shares with TeX, so each total is the content plus the delimiters'
    // cmex10 widths: `\big`'s 0.4167 em bracket, the extensible bracket's
    // 0.6667, the 2.4 em paren's 0.7361 and so on.
    use Style::{Display, Text};
    let e = engine();
    for (tex, style, width) in [
        (
            r"\left[\begin{array}{cc}3&1\\0&2\end{array}\right]",
            Display,
            4.055563,
        ),
        (
            r"\left[\begin{array}{c}1\end{array}\right]",
            Display,
            2.333339,
        ),
        (
            r"\left[\begin{array}{c}1\\2\\3\end{array}\right]",
            Display,
            2.833339,
        ),
        (r"\begin{pmatrix}1&2\\3&4\end{pmatrix}", Display, 3.472232),
        (
            r"\left\{\begin{array}{c}1\\2\\3\\4\end{array}\right.",
            Display,
            2.508891,
        ),
        (
            r"\left|\begin{array}{c}1\\2\end{array}\right|",
            Display,
            2.166667,
        ),
        (
            r"\left\langle\begin{array}{c}1\\2\end{array}\right\rangle",
            Display,
            3.000005,
        ),
        (
            r"\left\lfloor\begin{array}{c}1\\2\end{array}\right\rfloor",
            Display,
            2.666673,
        ),
        (r"\Big(1\Big)", Text, 1.694447),
    ] {
        let l = e.typeset(tex, style).unwrap();
        assert!(
            (l.width - width).abs() < 1e-4,
            "{tex} in {style:?}: {} against TeX's {width} em",
            l.width
        );
    }
    // The ink faces the content as cmex's does: a 2.4 em bracket's ink ends
    // 0.0148 em (bracketleftbigg's right bearing) before its box does.
    let l = e
        .typeset(
            r"\left[\begin{array}{cc}3&1\\0&2\end{array}\right]",
            Style::Display,
        )
        .unwrap();
    let bracket = l.glyphs.iter().find(|g| g.ch == '[').expect("cmex bracket");
    let font = e.faces().font(bracket.face).expect("placed face");
    let bbox = font.glyph_bbox(bracket.gid).expect("inked glyph");
    let scale = bracket.size / f64::from(font.units_per_em);
    let (x0, x1) = (
        bracket.x + f64::from(bbox[0]) * scale,
        bracket.x + f64::from(bbox[2]) * scale,
    );
    assert!((0.527781 - x1 - 0.014781).abs() < 1e-6, "ink {x0}..{x1}");
}

#[test]
fn big_operators_take_tex_box_widths() {
    // Real TeX's \wd (LaTeX with amsmath, 10 pt, in ems) of each operator
    // alone: cmex10's TFM width plus, for the integrals, the italic
    // correction TeX's clean_box keeps (display \int: 0.5556 + 0.4444).
    use Style::{Display, Text};
    let e = engine();
    for (tex, style, width) in [
        (r"\int", Display, 1.000002),
        (r"\int", Text, 0.666667),
        (r"\oint", Display, 1.000002),
        (r"\sum", Display, 1.444447),
        (r"\sum", Text, 1.055559),
        (r"\prod", Display, 1.277780),
        (r"\bigcup", Display, 1.111115),
        (r"\bigoplus", Display, 1.511116),
    ] {
        let l = e.typeset(tex, style).unwrap();
        assert!(
            (l.width - width).abs() < 1e-4,
            "{tex} in {style:?}: {} against TeX's {width} em",
            l.width
        );
    }
    // What follows a lone operator starts where TeX's does: the box, then
    // the Op-Ord thin space (\int x is 1.0 + 0.1667 + x in TeX).
    let l = e.typeset(r"\int x", Style::Display).unwrap();
    let x = l.glyphs.iter().find(|g| g.ch != '∫').unwrap();
    assert!((x.x - (1.000002 + 1.0 / 6.0)).abs() < 1e-4, "x at {}", x.x);
    // Scripts sit where rule 13 puts them on a nolimits operator: the
    // subscript at the TFM width, the superscript the italic correction
    // further right (display \int: 0.5556 and 1.0).
    let l = e.typeset(r"\int_0^1", Style::Display).unwrap();
    let at = |ch: char| l.glyphs.iter().find(|g| g.ch == ch).unwrap().x;
    assert!((at('0') - 0.555557).abs() < 1e-4, "sub at {}", at('0'));
    assert!((at('1') - 1.000003).abs() < 1e-4, "sup at {}", at('1'));
}

#[test]
fn null_delimiter_occupies_nulldelimiterspace() {
    let e = engine();
    let with_null = e.typeset(r"\left. x \right.", Style::Display).unwrap();
    let bare = e.typeset(r"x", Style::Display).unwrap();
    assert!((with_null.width - (bare.width + 2.0 * CM.null_delimiter_space)).abs() < 1e-9);
}

#[test]
fn fractions_without_delimiters_carry_null_delimiters() {
    // Rule 15e: TeX wraps every generalized fraction in its delimiters, and
    // \frac's are null: \nulldelimiterspace each side of the bar.
    let e = engine();
    let frac = e.typeset(r"\frac{a}{b}", Style::Display).unwrap();
    let bar = &frac.rules[0];
    assert!((bar.x - CM.null_delimiter_space).abs() < EPS, "{bar:?}");
    assert!((frac.width - (bar.width + 2.0 * CM.null_delimiter_space)).abs() < EPS);
    // An empty `\over` is the bare null fraction: TeX's 2.4 pt box at the
    // num1 and denom1 shifts.
    let over = e.typeset(r"\over", Style::Display).unwrap();
    assert!((over.width - 2.0 * CM.null_delimiter_space).abs() < EPS);
    assert!((over.height - CM.num1).abs() < EPS && (over.depth - CM.denom1).abs() < EPS);
}

#[test]
fn vdots_and_ddots_are_latex_period_constructions() {
    // LaTeX: \vdots stacks three text periods on 4 pt baselines under a 6 pt
    // kern; \ddots raises them 7, 4 and 1 pt between 1-2-2-1 mu kerns, the
    // highest under a 7 pt kern. Points and \hbox{.} keep their size in
    // scripts.
    let e = engine();
    let period = e.typeset(r"\text{.}", Style::Display).unwrap();
    let dots = |layout: &Layout| -> Vec<(f64, f64, f64)> {
        layout
            .glyphs
            .iter()
            .map(|g| {
                assert_eq!(g.ch, '.', "{layout:?}");
                (g.x, g.y, g.size)
            })
            .collect()
    };
    let vdots = e.typeset(r"\vdots", Style::Display).unwrap();
    let v = dots(&vdots);
    assert_eq!(v.len(), 3);
    // Top to bottom, as the vbox emits them.
    for (dot, y) in v.iter().zip([0.8, 0.4, 0.0]) {
        assert!(dot.0.abs() < EPS && (dot.1 - y).abs() < EPS && (dot.2 - 1.0).abs() < EPS);
    }
    assert!((vdots.width - period.width).abs() < EPS);
    assert!((vdots.height - (1.4 + period.height)).abs() < EPS);
    let scripted = e.typeset(r"x_{\vdots}", Style::Display).unwrap();
    assert!(
        scripted
            .glyphs
            .iter()
            .filter(|g| g.ch == '.')
            .all(|g| (g.size - 1.0).abs() < EPS),
        "{scripted:?}"
    );

    let ddots = e.typeset(r"\ddots", Style::Display).unwrap();
    let d = dots(&ddots);
    let mu = 1.0 / 18.0;
    assert_eq!(d.len(), 3);
    for (i, (dot, y)) in d.iter().zip([0.7, 0.4, 0.1]).enumerate() {
        let x = mu + f64::from(u8::try_from(i).unwrap()) * (period.width + 2.0 * mu);
        assert!((dot.0 - x).abs() < EPS && (dot.1 - y).abs() < EPS, "{d:?}");
    }
    assert!((ddots.width - (3.0 * period.width + 6.0 * mu)).abs() < EPS);
    assert!((ddots.height - (1.4 + period.height)).abs() < EPS);
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
    // The script's own `+` (cmr7's, the cut TeX sets scripts in).
    let plus_script_w = e.typeset("+", Style::Script).unwrap().width;
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
    let e = engine();
    // TeX's own boxes: each placed glyph's TFM character.
    let tfm = |l: &Layout, i: usize| {
        let g = &l.glyphs[i];
        e.faces().tfm(g.face, g.gid).expect("a TeX font's glyph")
    };
    let metrics = |l: &Layout, i: usize| {
        let (font, code) = tfm(l, i);
        font.char(code).expect("its TFM character")
    };

    let italic = e.typeset("f(", Style::Text).unwrap();
    let m = metrics(&italic, 0);
    assert!(
        (m.italic - 0.107_64).abs() < 1e-5,
        "cmmi10's f has its TFM italic correction: {m:?}"
    );
    let gap = italic.glyphs[1].x - italic.glyphs[0].x;
    assert!(
        (gap - (m.width + m.italic)).abs() < EPS,
        "f( advances by the f's width plus its italic correction: {gap} vs {m:?}"
    );

    let subscripted = e.typeset("f_i", Style::Text).unwrap();
    let gap = subscripted.glyphs[1].x - subscripted.glyphs[0].x;
    assert!(
        (gap - metrics(&subscripted, 0).width).abs() < EPS,
        "a subscript attaches at the nucleus width, without δ: {gap}"
    );

    // A group holding one Ord character is that character (tex.web §1186):
    // `{f}(` kerns like `f(`, and `{x}^2` places its script like `x^2`.
    let grouped = e.typeset("{f}(", Style::Text).unwrap();
    let gap = grouped.glyphs[1].x - grouped.glyphs[0].x;
    assert!(
        (gap - (m.width + m.italic)).abs() < EPS,
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
        (gap - upright.width).abs() < EPS,
        "inside a word of cmr10 the italic correction is zero: {gap} vs {upright:?}"
    );
    // Within `\mathrm{ff}` the first f is such a word character, and cmr10's
    // program pairs f with f only as a ligature, which the Reference's
    // preamble disables: no kern, no δ.
    let ff = e.typeset(r"\mathrm{ff}", Style::Text).unwrap();
    let ((font, first), (_, second)) = (tfm(&ff, 0), tfm(&ff, 1));
    assert_eq!(font.kern(first, second), None);
    let gap = ff.glyphs[1].x - ff.glyphs[0].x;
    assert!(
        (gap - upright.width).abs() < EPS,
        "\\mathrm{{ff}}: {gap} vs {upright:?}"
    );

    // A `\left…\right` inner is a sub-mlist, not a clean box: its lone f
    // keeps δ before the closing delimiter.
    let fenced = e.typeset(r"\left( f \right)", Style::Text).unwrap();
    let gap = fenced.glyphs[2].x - fenced.glyphs[1].x;
    assert!(
        (gap - (m.width + m.italic)).abs() < EPS,
        "\\left( f \\right): {gap} vs {m:?}"
    );
}

/// TeX's TFM boxes, not the ink: a character's box is its TFM height,
/// depth and width (floored at zero), cmmi's lig/kern program kerns an Ord
/// character against the next character of its font whatever that atom's
/// class, cmmi keeps δ between letters (its interword space is zero), and
/// an accent's skew is the base's kern with the skewchar. Each value below
/// is TeX's own box for the string under the Reference preamble (real
/// TeX's `\wd`, `\ht` and `\dp`, in ems).
#[test]
fn character_boxes_and_kerns_are_tex_metrics() {
    let e = engine();
    for (source, (w, h, d)) in [
        ("-", (0.777_78, 0.583_33, 0.083_33)),
        (r"\cdot", (0.277_78, 0.444_44, 0.0)),
        (r"\rightarrow", (1.0, 0.366_88, 0.0)),
        ("y", (0.526_16, 0.430_55, 0.194_44)),
        ("4", (0.5, 0.644_44, 0.0)),
        ("df", (0.951_05, 0.694_44, 0.194_44)),
        (r"\theta", (0.497_22, 0.694_44, 0.0)),
        // Clean boxes keep a lone character's italic correction in their
        // width (tex.web §721), and an accent's box is its width plus its
        // italic correction (§709): `\showbox` in display style.
        (r"\frac{f}{2}", (0.837_225, 1.370_952, 0.685_951)),
        ("x^f", (1.089_935, 0.899_002, 0.0)),
        (r"\sqrt{f}", (1.597_227, 0.983_821, 0.256_187)),
        (r"\tilde{f}", (0.597_226, 0.931_749, 0.194_444)),
        (r"\vec{v}", (0.520_601, 0.714_444, 0.0)),
    ] {
        let l = e.typeset(source, Style::Display).unwrap();
        assert!(
            (l.width - w).abs() < 1e-4 && (l.height - h).abs() < 1e-4 && (l.depth - d).abs() < 1e-4,
            "{source}: ({}, {}, {}) vs TeX ({w}, {h}, {d})",
            l.width,
            l.height,
            l.depth
        );
    }
    // `V,`: cmmi10 kerns V against its comma by -1/6 em.
    let gap = |source: &str| {
        let l = e.typeset(source, Style::Text).unwrap();
        l.glyphs[1].x - l.glyphs[0].x
    };
    let v = e.typeset("V", Style::Text).unwrap();
    let (font, code) = e.faces().tfm(v.glyphs[0].face, v.glyphs[0].gid).unwrap();
    let ch = font.char(code).unwrap();
    assert!((gap("V,") - (ch.width + ch.italic - 0.166_67)).abs() < 1e-4);
    // Two letters keep the first one's δ.
    let (font, code) = {
        let y = e.typeset("y", Style::Text).unwrap();
        e.faces().tfm(y.glyphs[0].face, y.glyphs[0].gid).unwrap()
    };
    let y = font.char(code).unwrap();
    assert!((gap("yx") - (y.width + y.italic)).abs() < EPS);
    // `\hat{A}` skews by cmmi10's A–skewchar kern (5/36 em) over the
    // accent's centered position.
    let skewed = e.typeset(r"\hat{A}", Style::Text).unwrap();
    let centered = e.typeset(r"\hat{\mathrm{A}}", Style::Text).unwrap();
    let accent_x = |l: &Layout| l.glyphs.iter().find(|g| g.ch != 'A').unwrap().x;
    let a_width = |l: &Layout| l.width;
    let shift = (accent_x(&skewed) - a_width(&skewed) / 2.0)
        - (accent_x(&centered) - a_width(&centered) / 2.0);
    assert!((shift - 0.138_89).abs() < 1e-3, "skew {shift}");
}

#[test]
fn ellipses_are_three_thin_spaced_punctuation_dots() {
    // Plain TeX: `\cdots` is `\mathinner{\cdotp\cdotp\cdotp}` and `\ldots`
    // the same with `\ldotp`. Punct-punct glue is a thin space (3 mu) in
    // display and text styles and nothing in script styles.
    let e = engine();
    for (command, dot, single) in [
        (r"\cdots", '⋅', r"\cdot"),
        (r"\ldots", '.', "."),
        (r"\hdots", '.', "."),
    ] {
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
        // Each size's own dot: scripts set the 7-point cut, so the script
        // advance is measured at script style, not scaled from 10 points.
        let advance = e.typeset(single, Style::Display).unwrap().width;
        let script_advance = e.typeset(single, Style::Script).unwrap().width;
        for pair in s.windows(2) {
            assert!(
                (pair[1] - pair[0] - script_advance).abs() < EPS,
                "{command}: script dots {s:?} must abut (advance {script_advance})"
            );
        }
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

#[test]
fn tex_rows_follow_align_star() {
    // The Reference sets Tex in align*: `&` cells pair right/left around the
    // alignment point, a right-hand cell opens with amsmath's `{}`, and a row
    // without `&` sits in the right-aligned first column. TexText lines stay
    // flush left.
    let e = engine();
    let x = |layout: &Layout, ch: char| -> f64 {
        layout
            .glyphs
            .iter()
            .find(|g| g.ch == ch)
            .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
            .x
    };
    // `1 &= 2` is as wide as `1 = 2`: the empty Ord keeps `=` a relation.
    let aligned = e.typeset(r"1 &= 2", Style::Display).unwrap();
    let plain = e.typeset(r"1 = 2", Style::Display).unwrap();
    assert!((aligned.width - plain.width).abs() < EPS, "{aligned:?}");
    assert!((x(&aligned, '=') - x(&plain, '=')).abs() < EPS);
    // Rows align at `&`.
    let rows = e.typeset(r"1 + 1 &= 2 \\ 3 &= 4", Style::Display).unwrap();
    let eqs: Vec<f64> = rows
        .glyphs
        .iter()
        .filter(|g| g.ch == '=')
        .map(|g| g.x)
        .collect();
    assert_eq!(eqs.len(), 2);
    assert!((eqs[0] - eqs[1]).abs() < EPS, "{eqs:?}");
    // A row without `&` is right-aligned: `1` ends where `222` does.
    let right = e.typeset(r"1 \\ 222", Style::Display).unwrap();
    let one = e.typeset("1", Style::Display).unwrap();
    assert!((x(&right, '1') + one.width - right.width).abs() < EPS);
    let text = e.typeset_text(r"1\\222").unwrap();
    assert!(x(&text, '1').abs() < EPS);
}

#[test]
fn script_boxes_carry_scriptspace() {
    // §756: each sub- and superscript box is widened by \scriptspace
    // (0.5 pt), in every style; limits are not scripts and get none.
    let e = engine();
    let two = e.typeset("2", Style::Script).unwrap().width;
    for src in ["{}^2", "{}_2"] {
        let w = e.typeset(src, Style::Display).unwrap().width;
        assert!(
            (w - (two + CM.script_space)).abs() < EPS,
            "{src}: {w} vs {two}"
        );
    }
    let both = e.typeset("{}^2_2", Style::Display).unwrap().width;
    assert!((both - (two + CM.script_space)).abs() < EPS, "{both}");
    let limits = e.typeset(r"\sum_2", Style::Display).unwrap().width;
    let sum = e.typeset(r"\sum", Style::Display).unwrap().width;
    assert!((limits - sum).abs() < EPS, "limits {limits} vs {sum}");
}

#[test]
fn array_rows_carry_latex_struts() {
    // LaTeX's array (amsmath's matrices, cases): every row carries
    // \@arstrut, 0.7 and 0.3 \baselineskip times \arraystretch, and rows
    // stack depth to height. Measured in LaTeX: a two-row matrix of
    // letters is 1.45 em high and 0.95 em deep, a three-row one 3.6 em in
    // all; cases rows are 1.44 em apart (\arraystretch 1.2).
    let e = engine();
    let two = e
        .typeset(r"\begin{matrix} a \\ b \end{matrix}", Style::Display)
        .unwrap();
    assert!(
        (two.height - 1.45).abs() < EPS && (two.depth - 0.95).abs() < EPS,
        "{two:?}"
    );
    let three = e
        .typeset(r"\begin{matrix} a \\ b \\ c \end{matrix}", Style::Display)
        .unwrap();
    assert!((three.height + three.depth - 3.6).abs() < EPS, "{three:?}");
    let cases = e
        .typeset(r"\begin{cases} a & b \\ c & d \end{cases}", Style::Display)
        .unwrap();
    let pitch = glyph_y(&cases, 'a') - glyph_y(&cases, 'c');
    assert!((pitch - 1.44).abs() < EPS, "cases rows {pitch}");
}

#[test]
fn accents_take_tex_accent_geometry() {
    // Rule 12 with plain TeX's accent characters (cmr10's, cmmi10's vector):
    // the accent ink sits where TeX's does, whichever face supplies it.
    let e = engine();
    let one = e.typeset("1", Style::Display).unwrap();
    let accent = |layout: &Layout, base: char| {
        layout
            .glyphs
            .iter()
            .find(|g| g.ch != base)
            .cloned()
            .unwrap_or_else(|| panic!("an accent glyph in {layout:?}"))
    };
    // `\vec` draws a 0.44 em arrow above the letter, topping out at cmmi10's
    // 0.714 em, not a full-size arrow through it.
    let vec = e.typeset(r"\vec{v}", Style::Display).unwrap();
    let arrow = accent(&vec, 'v');
    assert!(arrow.size > 0.4 && arrow.size < 0.6, "{arrow:?}");
    assert!((vec.height - 0.714).abs() < 0.02, "{vec:?}");
    // `\hat` is CM's own circumflex (cmr10's), topping out 0.694 em above
    // its box, which rises by h - min(h, x-height): barely over an x-height
    // letter, by 0.25 em over `A`. (CM Unicode's circumflex is 0.013 em
    // taller than cmr10's.)
    let hat = e.typeset(r"\hat{x}", Style::Display).unwrap();
    assert_eq!(accent(&hat, 'x').face, one.glyphs[0].face);
    for (base, accented) in [("x", r"\hat{x}"), ("A", r"\hat{A}")] {
        let h = e.typeset(base, Style::Display).unwrap().height;
        let laid = e.typeset(accented, Style::Display).unwrap().height;
        let tex = h - h.min(CM.x_height) + 0.694;
        assert!((laid - tex).abs() < 0.02, "{accented}: {laid} vs {tex}");
    }
}

#[test]
fn primes_are_cmsy_prime_characters() {
    // Plain TeX's \prime is cmsy's prime: 0.275 em wide and 0.559 em high in
    // text style, cmsy7's 0.329 x 0.559 (x 0.7) as a superscript. `'` is
    // `^{\prime}`, so `x''` superscripts two of them.
    let e = engine();
    let text = e.typeset(r"\prime", Style::Display).unwrap();
    assert!((text.width - 0.275).abs() < EPS, "{text:?}");
    assert!((text.height - 0.559).abs() < EPS && text.depth.abs() < EPS);
    let script_prime = 0.7 * 0.329;
    for (src, n) in [("{}'", 1.0), ("{}''", 2.0)] {
        let l = e.typeset(src, Style::Display).unwrap();
        let w = n * script_prime + CM.script_space;
        assert!((l.width - w).abs() < EPS, "{src}: {} vs {w}", l.width);
        let h = CM.sup1 + 0.7 * 0.559;
        assert!((l.height - h).abs() < EPS, "{src}: {} vs {h}", l.height);
    }
    let quote = e.typeset("f'(x)", Style::Display).unwrap();
    let explicit = e.typeset(r"f^\prime(x)", Style::Display).unwrap();
    assert_eq!(quote.glyphs.len(), explicit.glyphs.len());
    for (a, b) in quote.glyphs.iter().zip(&explicit.glyphs) {
        assert!((a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && a.size == b.size);
    }
}

#[test]
fn braces_are_latex_brace_fills() {
    // LaTeX's \overbrace and \underbrace: the base in display style,
    // centered on a fill of four 0.45 em cmex pieces (never narrower than
    // 1.8 em), 3 pt from it, with 3 pt more past the fill's 0.12 em row.
    // Measured in LaTeX: \underbrace{\frac{1}{6^2}} is 1.8 em wide and
    // 0.72 em deeper than the fraction.
    let e = engine();
    let x = |layout: &Layout, ch: char| {
        layout
            .glyphs
            .iter()
            .find(|g| g.ch == ch)
            .unwrap_or_else(|| panic!("glyph {ch}"))
            .x
    };
    let frac = e.typeset(r"\frac{1}{6^2}", Style::Display).unwrap();
    let under = e
        .typeset(r"\underbrace{\frac{1}{6^2}}", Style::Display)
        .unwrap();
    assert!((under.width - 1.8).abs() < EPS, "{}", under.width);
    assert!((under.height - frac.height).abs() < EPS);
    assert!((under.depth - (frac.depth + 0.72)).abs() < EPS);
    let centered = (1.8 - frac.width) / 2.0;
    assert!((x(&under, '1') - x(&frac, '1') - centered).abs() < EPS);
    let sum = e.typeset("a+b+c+d", Style::Display).unwrap();
    let over = e.typeset(r"\overbrace{a+b+c+d}", Style::Display).unwrap();
    assert!((over.width - sum.width).abs() < EPS);
    assert!((over.height - (sum.height + 0.72)).abs() < EPS);
    assert!((over.depth - sum.depth).abs() < EPS);
    // \displaystyle resets the size: a braced base in a superscript is
    // full size.
    let scripted = e.typeset(r"x^{\overbrace{a}}", Style::Display).unwrap();
    assert!((glyph_size(&scripted, 'a') - 1.0).abs() < EPS);
}

#[test]
fn a_trailing_row_break_opens_no_empty_math_row() {
    // align* ends its last row at \crcr: `= x + \\` is one row, like `= x +`.
    // A TexText paragraph's trailing \\ still sets an empty line.
    let e = engine();
    let one = e.typeset(r"= x +", Style::Display).unwrap();
    let trailing = e.typeset(r"= x + \\", Style::Display).unwrap();
    assert!((trailing.height - one.height).abs() < EPS);
    assert!((trailing.depth - one.depth).abs() < EPS);
    assert!((trailing.width - one.width).abs() < EPS);
    let paragraph = e.typeset_text(r"a\\").unwrap();
    let line = e.typeset_text("a").unwrap();
    assert!(paragraph.depth > line.depth + CM.baseline_skip / 2.0);
}

#[test]
fn explicit_spaces_keep_inter_atom_spacing() {
    // tex.web §761 keeps the previous atom's type across glue and kerns:
    // `a\,=b` is `a=b` plus 3 mu, and `a\quad\Rightarrow\quad b` is
    // `a\Rightarrow b` plus two quads (plain TeX: 24.577 - 22.911 pt and
    // 45.133 - 25.133 pt at 10 pt).
    let e = engine();
    let width = |s: &str| e.typeset(s, Style::Text).unwrap().width;
    assert!((width(r"a\,=b") - width("a=b") - 3.0 / 18.0).abs() < EPS);
    assert!((width(r"a\quad\Rightarrow\quad b") - width(r"a\Rightarrow b") - 2.0).abs() < EPS);
    // A kern still separates the characters, so no font kern applies
    // across it.
    assert!((width(r"A\!V") - width("A") - width("V") + 3.0 / 18.0).abs() < EPS);
}

/// fm-8zy2: in math mode TeX sets family 0 (digits, `+ = ( ) [ ]`) from
/// cmr10 at display and text size, and from cmr7 in scripts. CM Unicode's
/// Regular stands in for it in text mode only: its `+` and `=` carry 0.57
/// of cmr10's ink and its brackets 1.48.
#[test]
fn family_zero_is_cmr10_in_math_mode_and_cm_unicode_in_text_mode() {
    use fmd_math::faces::{FACE_REGULAR, FACE_ROMAN_7, FACE_ROMAN_10};
    let e = engine();
    let math = e.typeset("[1+2]=3", Style::Display).unwrap();
    assert_eq!(math.glyphs.len(), 7);
    for g in &math.glyphs {
        assert_eq!(g.face, FACE_ROMAN_10, "math-mode {:?}", g.ch);
    }
    let inline = e.typeset("[x]", Style::Text).unwrap();
    assert_eq!(inline.glyphs[0].face, FACE_ROMAN_10);
    let script = e.typeset("x^{[2]}", Style::Display).unwrap();
    for g in script.glyphs.iter().skip(1) {
        assert_eq!(g.face, FACE_ROMAN_7, "script {:?}", g.ch);
    }
    let text = e.typeset_text("[1+2]").unwrap();
    for g in &text.glyphs {
        assert_eq!(g.face, FACE_REGULAR, "text-mode {:?}", g.ch);
    }
}
