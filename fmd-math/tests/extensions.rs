//! The fm-kg9 acceptance suite: extensible delimiters across the whole
//! mechanism range (natural → uniform scale → drawn mainline, sizes far
//! past any glyph inventory), the stretchy constructions, environment
//! layout against the published rules, and macros/packs end-to-end
//! through the engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::{Engine, MacroSet, Style};

fn engine() -> Engine {
    Engine::bundled().expect("bundled faces")
}

// ---------------------------------------------------------------------------
// Delimiters: the three-stage mechanism, at many sizes, no size can fail
// ---------------------------------------------------------------------------

#[test]
fn every_delimiter_family_serves_every_size() {
    // Deep nesting drives the rule-19 target up without bound; every
    // family must keep producing a covering construction — the §11.4
    // promise that no requested size can fail.
    let e = engine();
    for (l, r) in [
        (r"(", r")"),
        (r"[", r"]"),
        (r"\{", r"\}"),
        (r"|", r"|"),
        (r"\langle", r"\rangle"),
        (r"\lfloor", r"\rfloor"),
        (r"\lceil", r"\rceil"),
        (r"\|", r"\|"),
    ] {
        let mut body = "x".to_owned();
        for _ in 0..6 {
            body = format!(r"\frac{{{body}}}{{y}}");
        }
        let src = format!(r"\left{l} {body} \right{r}");
        let layout = e
            .typeset(&src, Style::Display)
            .unwrap_or_else(|err| panic!("{src}: {err}"));
        let inner = e.typeset(&body, Style::Display).unwrap();
        assert!(
            layout.height + layout.depth >= inner.height + inner.depth - 1e-6,
            "{l}…{r}: delimited construct shorter than its body"
        );
        assert!(
            !layout.paths.is_empty(),
            "{l}…{r}: a six-deep fraction must engage the drawn mainline"
        );
    }
}

#[test]
fn the_big_family_hits_its_fixed_targets() {
    let e = engine();
    // \big… \Bigg): fixed total sizes 0.85 / 1.15 / 1.45 / 1.75 em.
    let sizes = [
        (r"\big(", 0.85),
        (r"\Big(", 1.15),
        (r"\bigg(", 1.45),
        (r"\Bigg(", 1.75),
    ];
    let mut last_total = 0.0;
    for (src, want) in sizes {
        let l = e.typeset(src, Style::Display).unwrap();
        let total = l.height + l.depth;
        assert!(
            total >= want - 1e-6,
            "{src}: total {total} below its {want} target"
        );
        assert!(total > last_total, "{src}: not monotone over the family");
        last_total = total;
    }
}

#[test]
fn drawn_output_is_deterministic_across_runs() {
    let e = engine();
    let src = r"\left\{ \frac{\frac{a}{b}}{\frac{c}{d}} \right\}";
    let a = e.typeset(src, Style::Display).unwrap();
    let b = e.typeset(src, Style::Display).unwrap();
    assert_eq!(a, b, "drawn constructions must be bit-identical");
    let pa = fmd_math::paths::resolve_paths(&e, &a).unwrap();
    let pb = fmd_math::paths::resolve_paths(&e, &b).unwrap();
    assert_eq!(
        fmd_math::paths::canonical_dump(&pa),
        fmd_math::paths::canonical_dump(&pb)
    );
}

#[test]
fn drawn_delimiters_keep_their_source_spans() {
    // Provenance survives the mechanism switch: the drawn paren's path
    // carries the `(` token's span (isolate/t2c keep working).
    let e = engine();
    let src = r"\left( \frac{\frac{1}{2}}{3} \right)";
    let l = e.typeset(src, Style::Display).unwrap();
    assert!(fmd_math::paths::spans_cover(&l, src.len()));
    let open = src.find('(').unwrap();
    assert!(
        l.paths
            .iter()
            .any(|p| p.span.start <= open && open < p.span.end),
        "no drawn path claims the ( token"
    );
}

// ---------------------------------------------------------------------------
// Stretchy constructions
// ---------------------------------------------------------------------------

#[test]
fn stretchy_constructions_span_their_bases() {
    let e = engine();
    for src in [
        r"\widehat{x+y}",
        r"\widetilde{abc}",
        r"\overbrace{a+b+c}",
        r"\underbrace{a+b+c}",
        r"\overrightarrow{AB}",
        r"\overleftarrow{AB}",
    ] {
        let l = e
            .typeset(src, Style::Display)
            .unwrap_or_else(|err| panic!("{src}: {err}"));
        assert_eq!(l.paths.len(), 1, "{src}: one drawn band");
        // The band's ink spans the base's width (its x-extent reaches
        // most of the layout width).
        let max_x = l.paths[0]
            .contours
            .iter()
            .flat_map(|c| {
                core::iter::once(c.start.0).chain(c.segments.iter().map(|s| match s {
                    fmd_math::PathSeg::Line { to } | fmd_math::PathSeg::Quad { to, .. } => to.0,
                }))
            })
            .fold(0.0_f64, f64::max);
        assert!(
            max_x >= l.width * 0.9,
            "{src}: band ends at {max_x} of {}",
            l.width
        );
    }
    // Over-constructions rise above the base; under-constructions sink.
    let base = e.typeset("a+b+c", Style::Display).unwrap();
    let over = e.typeset(r"\overbrace{a+b+c}", Style::Display).unwrap();
    let under = e.typeset(r"\underbrace{a+b+c}", Style::Display).unwrap();
    assert!(over.height > base.height && (over.depth - base.depth).abs() < 1e-9);
    assert!(under.depth > base.depth && (under.height - base.height).abs() < 1e-9);
}

#[test]
fn overbrace_scripts_center_as_limits() {
    // `\overbrace{x+y}^{n}`: the n sits over the brace (amsmath \mathop
    // semantics), so the width stays the brace's, not widened by a side
    // superscript, and the height grows.
    let e = engine();
    let plain = e.typeset(r"\overbrace{x+y}", Style::Display).unwrap();
    let scripted = e.typeset(r"\overbrace{x+y}^{n}", Style::Display).unwrap();
    assert!(scripted.height > plain.height, "the n stacks above");
    assert!(
        (scripted.width - plain.width).abs() < 0.05,
        "limits attach centered, not at the side: {} vs {}",
        scripted.width,
        plain.width
    );
}

// ---------------------------------------------------------------------------
// Environments
// ---------------------------------------------------------------------------

#[test]
fn matrix_columns_share_widths_and_center() {
    let e = engine();
    let l = e
        .typeset(
            r"\begin{matrix} 1 & 22 \\ 333 & 4 \end{matrix}",
            Style::Display,
        )
        .unwrap();
    assert_eq!(l.glyphs.len(), 7);
    // Row 1: the `1` centers over the wider `333`; column 2's `22` left of
    // column 2's center for `4`… assert centering pairwise: glyphs of one
    // column share a center x.
    let g = |ch: char| l.glyphs.iter().filter(|p| p.ch == ch).collect::<Vec<_>>();
    let one = g('1')[0];
    let threes = g('3');
    let col0_center_row0 = one.x + 0.25; // approx half an advance
    let col0_center_row1 = threes[0].x + (threes[2].x - threes[0].x + 0.5) / 2.0;
    assert!(
        (col0_center_row0 - col0_center_row1).abs() < 0.30,
        "column 0 not centered: {col0_center_row0} vs {col0_center_row1}"
    );
    // Rows sit on distinct baselines, first above the second.
    let y1 = one.y;
    let y3 = threes[0].y;
    assert!(y1 > y3, "row order: {y1} vs {y3}");
}

#[test]
fn the_matrix_family_wraps_the_delimiter_engine() {
    let e = engine();
    let bare = e
        .typeset(
            r"\begin{matrix} a & b \\ c & d \end{matrix}",
            Style::Display,
        )
        .unwrap();
    for (env, l, r) in [
        ("pmatrix", '(', ')'),
        ("bmatrix", '[', ']'),
        ("Bmatrix", '{', '}'),
        ("vmatrix", '|', '|'),
    ] {
        let src = format!(r"\begin{{{env}}} a & b \\ c & d \end{{{env}}}");
        let wrapped = e
            .typeset(&src, Style::Display)
            .unwrap_or_else(|err| panic!("{env}: {err}"));
        // Bars are deliberately thin; anything else adds real width.
        let min_added = if env == "vmatrix" { 0.08 } else { 0.2 };
        assert!(
            wrapped.width > bare.width + min_added,
            "{env}: no delimiter width added ({} vs {})",
            wrapped.width,
            bare.width
        );
        // The delimiters appear as glyphs (small matrix ⇒ within scale
        // ceiling) or drawn paths; either way something non-cell exists.
        let has_delims =
            wrapped.glyphs.iter().any(|g| g.ch == l || g.ch == r) || !wrapped.paths.is_empty();
        assert!(has_delims, "{env}: delimiters missing");
    }
    // Vmatrix draws (‖ has no authored glyph in the bundled faces).
    let v = e
        .typeset(r"\begin{Vmatrix} a \end{Vmatrix}", Style::Display)
        .unwrap();
    assert!(
        !v.paths.is_empty() || v.glyphs.iter().any(|g| g.ch == '‖'),
        "Vmatrix delimiters missing"
    );
}

#[test]
fn smallmatrix_is_smaller_and_tighter() {
    let e = engine();
    let big = e
        .typeset(
            r"\begin{matrix} a & b \\ c & d \end{matrix}",
            Style::Display,
        )
        .unwrap();
    let small = e
        .typeset(
            r"\begin{smallmatrix} a & b \\ c & d \end{smallmatrix}",
            Style::Display,
        )
        .unwrap();
    assert!(small.width < big.width);
    assert!(small.height + small.depth < big.height + big.depth);
}

#[test]
fn cases_left_aligns_behind_one_stretched_brace() {
    let e = engine();
    let src = r"\begin{cases} x & x > 0 \\ -x & x \le 0 \end{cases}";
    let l = e.typeset(src, Style::Display).unwrap();
    // One `{`, no `}`: as a glyph or (stretched past ceiling) a drawn path.
    let brace_glyphs = l.glyphs.iter().filter(|g| g.ch == '{').count();
    assert!(
        brace_glyphs == 1 || l.paths.len() == 1,
        "cases wants exactly one left brace"
    );
    assert_eq!(
        l.glyphs.iter().filter(|g| g.ch == '}').count(),
        0,
        "cases has no right brace"
    );
    // Left alignment: both x rows start at the same x (the value column).
    let xs: Vec<&fmd_math::PlacedGlyph> = l.glyphs.iter().filter(|g| g.ch == 'x').collect();
    let first_col_x = xs.iter().map(|g| g.x).fold(f64::INFINITY, f64::min);
    let row_starts: Vec<f64> = xs
        .iter()
        .filter(|g| (g.x - first_col_x).abs() < 0.6)
        .map(|g| g.x)
        .collect();
    assert!(row_starts.len() >= 2, "two value-column xs");
}

#[test]
fn array_honors_column_specs_and_rules() {
    let e = engine();
    // r, c, l: in each column the short entry aligns per the spec against
    // the wide `333`.
    let l = e
        .typeset(
            r"\begin{array}{rcl} 1 & 2 & 3 \\ 333 & 333 & 333 \end{array}",
            Style::Display,
        )
        .unwrap();
    // 1 + 2 + one 3 in row 0, nine 3s in row 1.
    let digits = l
        .glyphs
        .iter()
        .filter(|g| matches!(g.ch, '1' | '2' | '3'))
        .count();
    assert_eq!(digits, 12);
    // Spec alignment: the r-column's short `1` right-aligns against the
    // wide `333` (its x is past the row-1 column start), the l-column's
    // `3` left-aligns (same x as its `333`).
    let one_x = l.glyphs.iter().find(|g| g.ch == '1').unwrap().x;
    let col0_start = l
        .glyphs
        .iter()
        .filter(|g| g.ch == '3')
        .map(|g| g.x)
        .fold(f64::INFINITY, f64::min);
    assert!(one_x > col0_start + 0.3, "r column right-aligns the 1");
    // The vertical-rule variant adds rules spanning the grid.
    let ruled = e
        .typeset(
            r"\begin{array}{|c|c|} a & b \\ c & d \end{array}",
            Style::Display,
        )
        .unwrap();
    let grid_rules = ruled
        .rules
        .iter()
        .filter(|r| r.height > 0.5) // taller than any fraction bar
        .count();
    assert_eq!(grid_rules, 3, "three vertical rules from |c|c|");
    // An unsupported spec character refuses precisely.
    let err = e
        .typeset(r"\begin{array}{c@{}c} a & b \end{array}", Style::Display)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("unsupported array column-spec character '@'"),
        "{err}"
    );
}

#[test]
fn align_star_alternates_rl_and_lines_up_the_point() {
    let e = engine();
    let src = r"\begin{align*} x + y &= 1 \\ x &= 2 \end{align*}";
    let l = e.typeset(src, Style::Display).unwrap();
    // The alignment point: both rows' `=` glyphs share (nearly) one x —
    // the r,l pair closes up around the point.
    let eqs: Vec<f64> = l
        .glyphs
        .iter()
        .filter(|g| g.ch == '=')
        .map(|g| g.x)
        .collect();
    assert_eq!(eqs.len(), 2);
    assert!(
        (eqs[0] - eqs[1]).abs() < 0.05,
        "alignment points drifted: {eqs:?}"
    );
    // Row baselines are distinct and ordered.
    let ys: Vec<f64> = l
        .glyphs
        .iter()
        .filter(|g| g.ch == '=')
        .map(|g| g.y)
        .collect();
    assert!(ys[0] > ys[1]);
}

#[test]
fn environments_lay_out_inside_larger_formulas() {
    let e = engine();
    // The corpus shape: a matrix as a fraction's numerator, wrapped in
    // \left…\right — every mechanism at once.
    let src = r"v = \frac{\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}}{2}";
    let l = e
        .typeset(src, Style::Display)
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(l.glyphs.len() >= 7);
    assert!(fmd_math::paths::spans_cover(&l, src.len()));
}

// ---------------------------------------------------------------------------
// Macros and packs, end to end
// ---------------------------------------------------------------------------

#[test]
fn pack_and_inline_macros_typeset_end_to_end() {
    let e = engine();
    let pack = MacroSet::pack("fmd-math/pack/default").unwrap();
    // The pack's \minus typesets as a binary minus.
    let l = e
        .typeset_with_macros(r"a \minus b", Style::Display, &pack)
        .unwrap();
    assert!(l.glyphs.iter().any(|g| g.ch == '−'), "\\minus → −");

    // An inline macro over the pack.
    let l = e
        .typeset_with_macros(
            r"\newcommand{\half}[1]{\frac{#1}{2}}\half{x} \minus 1",
            Style::Display,
            &pack,
        )
        .unwrap();
    assert!(l.glyphs.iter().any(|g| g.ch == 'x'));
    assert!(l.rules.len() == 1, "the \\frac bar from the macro body");
}

#[test]
fn macro_spans_flow_into_the_span_map() {
    // The macro-call span is what selection sees: selecting the call's
    // byte range selects the produced material.
    let e = engine();
    let set = MacroSet::new();
    let src = r"\newcommand{\half}[1]{\frac{#1}{2}}\half{x}+1";
    let l = e.typeset_with_macros(src, Style::Display, &set).unwrap();
    assert!(fmd_math::paths::spans_cover(&l, src.len()));
    // The literal argument x carries its own true span.
    let x = l.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    assert_eq!(&src[x.span.start..x.span.end], "x");
    // The 2 produced by the body carries the call site's span.
    let two = l.glyphs.iter().find(|g| g.ch == '2').unwrap();
    assert_eq!(&src[two.span.start..two.span.end], r"\half{x}");
}

#[test]
fn plain_parse_supports_inline_definitions_too() {
    // The frozen parse() surface runs the expansion pass, so corpus strings
    // that define-and-use just work.
    let node = fmd_math::parse(r"\newcommand{\f}{y}\f + \f").unwrap();
    let src = r"\newcommand{\f}{y}\f + \f";
    let _ = (node, src);
    let e = engine();
    let l = e.typeset(src, Style::Display).unwrap();
    assert_eq!(l.glyphs.iter().filter(|g| g.ch == 'y').count(), 2);
}

#[test]
fn texttext_mode_expands_macros_in_islands_and_mainland() {
    let e = engine();
    let mut set = MacroSet::new();
    set.define("brand", 0, "Franken").unwrap();
    let l = e
        .typeset_text_with_macros(r"\brand{} math: $\brand^2$", &set)
        .unwrap();
    // 2 × "Franken" = 14 letters, plus "math:" and the script 2.
    assert!(l.glyphs.iter().filter(|g| g.ch == 'F').count() == 2);
}
