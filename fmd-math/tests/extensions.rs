//! The fm-kg9 acceptance suite: extensible delimiters across the whole
//! mechanism range (natural → uniform scale → drawn mainline, sizes far
//! past any glyph inventory), the stretchy constructions, environment
//! layout against the published rules, and macros/packs end-to-end
//! through the engine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::metrics::CM;
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
        // Past the small variant TeX builds the delimiter from cmex: a
        // successor glyph, the extensible recipe's pieces, or (angles, which
        // have no recipe) the largest successor. The drawn mainline remains
        // only for faces without cmex.
        assert!(
            layout
                .glyphs
                .iter()
                .any(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION)
                || !layout.paths.is_empty(),
            "{l}…{r}: a six-deep fraction must take a cmex construction"
        );
    }
}

#[test]
fn the_big_family_hits_its_fixed_targets() {
    let e = engine();
    // \big… \Bigg: TeX lands on cmex's 1.2 / 1.8 / 2.4 / 3.0 em variants,
    // axis-centered (measured under the Reference's preamble: \big( is
    // 0.85 em high and 0.35 em deep).
    let sizes = [
        (r"\big(", 1.2),
        (r"\Big(", 1.8),
        (r"\bigg(", 2.4),
        (r"\Bigg(", 3.0),
        (r"\big[", 1.2),
        (r"\Big\{", 1.8),
    ];
    for (src, want) in sizes {
        let l = e.typeset(src, Style::Display).unwrap();
        let total = l.height + l.depth;
        assert!(
            (total - want).abs() < 1e-6,
            "{src}: total {total}, TeX {want}"
        );
        assert!(
            (l.height - (want / 2.0 + CM.axis_height)).abs() < 1e-6,
            "{src}: height {}",
            l.height
        );
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
    // Provenance survives the mechanism switch: whatever builds the large
    // paren (cmex's successor or extensible pieces, or a drawn path)
    // carries the `(` token's span (isolate/t2c keep working).
    let e = engine();
    let src = r"\left( \frac{\frac{1}{2}}{3} \right)";
    let l = e.typeset(src, Style::Display).unwrap();
    assert!(fmd_math::paths::spans_cover(&l, src.len()));
    let open = src.find('(').unwrap();
    let claims = |span: fmd_math::Span| span.start <= open && open < span.end;
    assert!(
        l.paths.iter().any(|p| claims(p.span))
            || l.glyphs.iter().any(|g| g.ch == '(' && claims(g.span)),
        "nothing claims the ( token"
    );
}

// ---------------------------------------------------------------------------
// Stretchy constructions
// ---------------------------------------------------------------------------

#[test]
fn stretchy_constructions_span_their_bases() {
    let e = engine();
    // The braces are TeX's fill: cmex's four pieces and two rules, ending
    // exactly at the layout's width (franken_manim fm-u5sb).
    for src in [r"\overbrace{a+b+c}", r"\underbrace{a+b+c}"] {
        let l = e
            .typeset(src, Style::Display)
            .unwrap_or_else(|err| panic!("{src}: {err}"));
        assert!(l.paths.is_empty(), "{src}: no drawn band");
        let pieces: Vec<_> = l
            .glyphs
            .iter()
            .filter(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION)
            .collect();
        assert_eq!((pieces.len(), l.rules.len()), (4, 2), "{src}");
        let piece = fmd_math::tfm::CMEX10.char(0x7A).expect("braceld").width;
        let end = pieces.iter().map(|g| g.x).fold(0.0_f64, f64::max) + piece;
        assert!(
            (end - l.width).abs() < 1e-9,
            "{src}: fill ends at {end} of {}",
            l.width
        );
    }
    for src in [
        r"\widehat{x+y}",
        r"\widetilde{abc}",
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
    // Left alignment: both rows' value cells start at the same x. A row's
    // start is its leftmost glyph that is not the brace (`x` in the first
    // row, `−` in the second).
    let mut row_starts: std::collections::BTreeMap<i64, f64> = std::collections::BTreeMap::new();
    for g in l.glyphs.iter().filter(|g| g.ch != '{') {
        #[allow(clippy::cast_possible_truncation)]
        let row = (g.y * 1000.0).round() as i64;
        let start = row_starts.entry(row).or_insert(f64::INFINITY);
        *start = start.min(g.x);
    }
    let starts: Vec<f64> = row_starts.values().copied().collect();
    assert!(starts.len() >= 2, "two rows: {starts:?}");
    assert!(
        starts.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-9),
        "value cells left-align: {starts:?}"
    );
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

/// amssymb's symbols that a bundled face carries (franken_manim
/// fm-tex-constructs-t2-1wfq): every one typesets as one glyph.
const AMSSYMB: &[&str] = &[
    "Bbbk",
    "Bumpeq",
    "Cap",
    "Cup",
    "Finv",
    "Game",
    "Lleftarrow",
    "Rrightarrow",
    "Subset",
    "Supset",
    "Vdash",
    "Vvdash",
    "approxeq",
    "backsim",
    "backsimeq",
    "barwedge",
    "because",
    "beth",
    "between",
    "bigstar",
    "blacksquare",
    "blacktriangle",
    "blacktriangledown",
    "blacktriangleleft",
    "blacktriangleright",
    "boxdot",
    "boxminus",
    "boxplus",
    "boxtimes",
    "bumpeq",
    "centerdot",
    "circeq",
    "circledast",
    "circledcirc",
    "circleddash",
    "complement",
    "curlyeqprec",
    "curlyeqsucc",
    "curlyvee",
    "curlywedge",
    "daleth",
    "divideontimes",
    "doteqdot",
    "dotplus",
    "downdownarrows",
    "downharpoonleft",
    "downharpoonright",
    "eqcirc",
    "eqsim",
    "fallingdotseq",
    "geqq",
    "geqslant",
    "ggg",
    "gimel",
    "gneqq",
    "gnsim",
    "gtrdot",
    "gtreqless",
    "gtrless",
    "gtrsim",
    "gvertneqq",
    "hslash",
    "intercal",
    "leftarrowtail",
    "leftleftarrows",
    "leftrightharpoons",
    "leftrightsquigarrow",
    "leftthreetimes",
    "leqq",
    "leqslant",
    "lessdot",
    "lesseqgtr",
    "lessgtr",
    "lesssim",
    "lhd",
    "lll",
    "lneqq",
    "lnsim",
    "looparrowleft",
    "looparrowright",
    "ltimes",
    "lvertneqq",
    "measuredangle",
    "mho",
    "multimap",
    "nLeftarrow",
    "nLeftrightarrow",
    "nRightarrow",
    "nVDash",
    "nVdash",
    "ncong",
    "nexists",
    "ngeq",
    "ngtr",
    "nleftarrow",
    "nleftrightarrow",
    "nleq",
    "nless",
    "nparallel",
    "nprec",
    "npreceq",
    "nrightarrow",
    "nshortmid",
    "nshortparallel",
    "nsim",
    "nsubseteq",
    "nsucc",
    "nsucceq",
    "nsupseteq",
    "ntriangleleft",
    "ntrianglelefteq",
    "ntriangleright",
    "ntrianglerighteq",
    "nvDash",
    "nvdash",
    "pitchfork",
    "preccurlyeq",
    "precnsim",
    "precsim",
    "rhd",
    "rightarrowtail",
    "rightrightarrows",
    "rightsquigarrow",
    "rightthreetimes",
    "risingdotseq",
    "rtimes",
    "shortmid",
    "shortparallel",
    "sphericalangle",
    "sqsubset",
    "sqsupset",
    "square",
    "subsetneq",
    "succcurlyeq",
    "succnsim",
    "succsim",
    "supsetneq",
    "therefore",
    "thickapprox",
    "thicksim",
    "triangledown",
    "trianglelefteq",
    "triangleq",
    "trianglerighteq",
    "twoheadleftarrow",
    "twoheadrightarrow",
    "unlhd",
    "unrhd",
    "upharpoonleft",
    "upharpoonright",
    "upuparrows",
    "vDash",
    "varpropto",
    "varsubsetneq",
    "varsupsetneq",
    "vartriangle",
    "vartriangleleft",
    "vartriangleright",
    "veebar",
    "Box",
    "Doteq",
    "doublecap",
    "doublecup",
    "gggtr",
    "leadsto",
    "llless",
    // Untiered until the AMS faces were bundled (fm-0byq).
    "lozenge",
    "blacklozenge",
    "leftrightarrows",
    "rightleftarrows",
    "Lsh",
    "Rsh",
    "gtrapprox",
    "lessapprox",
    "eqslantless",
    "eqslantgtr",
    "backprime",
    "lesseqqgtr",
    "gtreqqless",
    "doublebarwedge",
    "smallsmile",
    "smallfrown",
    "subseteqq",
    "supseteqq",
    "circledS",
    "lneq",
    "gneq",
    "precneqq",
    "succneqq",
    "precnapprox",
    "succnapprox",
    "lnapprox",
    "gnapprox",
    "diagup",
    "diagdown",
    "subsetneqq",
    "supsetneqq",
    "varsubsetneqq",
    "varsupsetneqq",
    "eth",
    "succapprox",
    "precapprox",
    "curvearrowleft",
    "curvearrowright",
    "digamma",
    "varkappa",
    "backepsilon",
];

#[test]
fn amssymb_symbols_typeset_from_the_bundled_math_faces() {
    use fmd_math::faces::{FACE_AMS_A_10, FACE_AMS_B_10};
    let e = engine();
    for name in AMSSYMB {
        let source = format!(r"\{name}");
        let l = e
            .typeset(&source, Style::Text)
            .unwrap_or_else(|err| panic!("{source}: {err}"));
        assert_eq!(l.glyphs.len(), 1, "{source}: {l:?}");
        // Every amssymb name sets its authored msam or msbm glyph.
        assert!(
            [FACE_AMS_A_10, FACE_AMS_B_10].contains(&l.glyphs[0].face),
            "{source}: {l:?}"
        );
    }
    // The family and code amssymb.sty's \DeclareMathSymbol gives each name,
    // sampled across both faces. hslash/hbar (7D/7E) and nshortmid/nmid
    // (2E/2D) share Unicode characters, so only the code tells them apart;
    // \thickapprox and \thicksim are msbm's thick forms, not cmsy's.
    for (source, face, code) in [
        // fm-0byq's acceptance pair: amsfonts' \checkmark (\mathhexbox
        // \symAMSa58) and a blackboard capital through msbm's ℝ.
        (r"\checkmark", FACE_AMS_A_10, 0x58),
        (r"\mathbb{R}", FACE_AMS_B_10, 0x52),
        (r"\leqslant", FACE_AMS_A_10, 0x36),
        (r"\square", FACE_AMS_A_10, 0x03),
        (r"\therefore", FACE_AMS_A_10, 0x29),
        (r"\doublecap", FACE_AMS_A_10, 0x65),
        (r"\nleq", FACE_AMS_B_10, 0x02),
        (r"\Bbbk", FACE_AMS_B_10, 0x7C),
        (r"\thickapprox", FACE_AMS_B_10, 0x74),
        (r"\thicksim", FACE_AMS_B_10, 0x73),
        (r"\hslash", FACE_AMS_B_10, 0x7D),
        (r"\hbar", FACE_AMS_B_10, 0x7E),
        (r"\nshortmid", FACE_AMS_B_10, 0x2E),
        (r"\nmid", FACE_AMS_B_10, 0x2D),
    ] {
        let glyph = e.typeset(source, Style::Text).unwrap().glyphs[0].clone();
        assert_eq!(glyph.face, face, "{source}");
        assert_eq!(Some(glyph.gid), e.faces().tex_glyph(face, code), "{source}");
    }
    // amssymb's classes: a relation takes thick spaces, a binary operator
    // medium ones, and \square is an ordinary symbol.
    let width = |s: &str| e.typeset(s, Style::Text).unwrap().width;
    let thick = 5.0 / 18.0;
    let medium = 4.0 / 18.0;
    for (between, gap) in [
        (r"\leqslant", thick),
        (r"\therefore", thick),
        (r"\boxplus", medium),
        (r"\ltimes", medium),
        (r"\square", 0.0),
        (r"\hslash", 0.0),
        (r"\nshortmid", thick),
    ] {
        let sum = width("a") + width(between) + width("b");
        let laid = width(&format!(r"a{between} b"));
        // A mu is cmsy10's quad (1.000003 em) over 18.
        assert!(
            (laid - sum - 2.0 * gap).abs() < 1e-5,
            "{between}: {laid} against {sum} + 2 × {gap}"
        );
    }
}

#[test]
fn mathscr_is_a_script_distinct_from_mathcal() {
    use fmd_math::faces::{FACE_MATH_SYMBOLS, FACE_SYMBOLS};
    let e = engine();
    let scr = e.typeset(r"\mathscr{L}", Style::Text).unwrap();
    let cal = e.typeset(r"\mathcal{L}", Style::Text).unwrap();
    assert_eq!(scr.glyphs[0].ch, 'ℒ');
    assert_eq!(scr.glyphs[0].face, FACE_SYMBOLS, "Noto Sans Math's script");
    assert_eq!(cal.glyphs[0].face, FACE_MATH_SYMBOLS, "cmsy's calligraphic");
    let lower = e.typeset(r"\mathscr{abc}", Style::Text).unwrap();
    assert!(lower.glyphs.iter().all(|g| g.face == FACE_SYMBOLS));
}

#[test]
fn mathfrak_sets_euler_fraktur_at_its_optical_sizes() {
    use fmd_math::faces::{FACE_FRAKTUR_5, FACE_FRAKTUR_7, FACE_FRAKTUR_10, FACE_ROMAN_10};
    let e = engine();
    let l = e.typeset(r"\mathfrak{gR1}", Style::Text).unwrap();
    let chars: Vec<char> = l.glyphs.iter().map(|g| g.ch).collect();
    // Semantic codepoints: the U+1D504 block, the Letterlike ℜ, and the
    // digit as itself (Unicode has no Fraktur digits).
    assert_eq!(chars, ['𝔤', 'ℜ', '1']);
    assert!(l.glyphs.iter().all(|g| g.face == FACE_FRAKTUR_10), "{l:?}");
    // Scripts take eufm7 and eufm5, as ueuf.fd selects them.
    let s = e
        .typeset(r"x_{\mathfrak{a}_{\mathfrak{b}}}", Style::Text)
        .unwrap();
    let face = |ch| s.glyphs.iter().find(|g| g.ch == ch).unwrap().face;
    assert_eq!(face('𝔞'), FACE_FRAKTUR_7);
    assert_eq!(face('𝔟'), FACE_FRAKTUR_5);
    // A fixed-family character keeps its face under the alphabet: TeX
    // switches only letters and digits.
    let plus = e.typeset(r"\mathfrak{a+b}", Style::Text).unwrap();
    assert_eq!(plus.glyphs[1].ch, '+');
    assert_eq!(plus.glyphs[1].face, FACE_ROMAN_10);
    let mathml = fmd_math::to_mathml(&fmd_math::parse(r"\mathfrak{g}").unwrap(), false);
    assert!(mathml.contains(r#"mathvariant="fraktur""#), "{mathml}");
}

#[test]
fn gather_rows_are_centred_display_formulas() {
    let e = engine();
    let gather = e
        .typeset(
            r"\begin{gather*} a+b+c \\ \frac{x}{y} \end{gather*}",
            Style::Text,
        )
        .unwrap();
    let align = e
        .typeset(
            r"\begin{align*} a+b+c \\ \frac{x}{y} \end{align*}",
            Style::Text,
        )
        .unwrap();
    // Display-style rows: the fraction's numerator is full size.
    let x = gather.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    assert!((x.size - 1.0).abs() < 1e-12);
    // Centred in the widest row; rows a \jot apart, as align* stacks them.
    let a = gather.glyphs.iter().find(|g| g.ch == 'a').unwrap();
    let bar = &gather.rules[0];
    assert!(a.x.abs() < 1e-12, "the widest row starts the block");
    let row_width = gather.width;
    assert!(((bar.x + bar.width / 2.0) - row_width / 2.0).abs() < 1e-9);
    assert!((gather.height + gather.depth - (align.height + align.depth)).abs() < 1e-9);
    for env in ["gathered", "equation*"] {
        let src = format!(r"\begin{{{env}}} \frac{{x}}{{y}} \end{{{env}}}");
        let l = e.typeset(&src, Style::Text).unwrap();
        let x = l.glyphs.iter().find(|g| g.ch == 'x').unwrap();
        assert!((x.size - 1.0).abs() < 1e-12, "{env}");
    }
}

/// Glyph x positions (ems) relative to the first glyph, row by row.
fn relative_xs(l: &fmd_math::Layout) -> Vec<f64> {
    let mut glyphs: Vec<_> = l.glyphs.iter().collect();
    glyphs.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let first = glyphs[0].x;
    glyphs.iter().map(|g| g.x - first).collect()
}

#[test]
fn display_align_spreads_its_pairs_across_the_line() {
    // TeX under the Reference template (`\centering` and `align*`, which
    // the Tex surface wraps every string in): glyph x in pt, rows top to
    // bottom. Each gap between pairs, and each margin, is the line's
    // leftover over three. Microtype does not move them.
    let tex: [f64; 16] = [
        104.431, 112.4946, 123.0501, 215.965, 223.0702, 233.6258, 86.5179, 94.4553, 104.4553,
        112.4946, 123.0501, 214.5679, 223.0702, 233.6258, 241.0539, 251.0539,
    ];
    let mut rows = [tex[..6].to_vec(), tex[6..].to_vec()];
    for row in &mut rows {
        row.sort_by(f64::total_cmp);
    }
    let first = rows[0][0];
    let want: Vec<f64> = rows.concat().iter().map(|x| (x - first) / 10.0).collect();
    let e = engine();
    let source = r"a&=b & c&=d \\ x+y&=1 & u&=v+w";
    for l in [
        e.typeset(source, Style::Text).unwrap(),
        e.typeset_text(&format!(r"\begin{{align*}} {source} \end{{align*}}"))
            .unwrap(),
    ] {
        let got = relative_xs(&l);
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() < 1e-3, "{got:?} against TeX's {want:?}");
        }
    }
    // `aligned` is no display: \minalignsep (10 pt) between its pairs. TeX's
    // glyph x, pt: 139.6118 147.6754 158.2309 172.5226 179.6278 190.1833.
    let aligned = e
        .typeset(r"\begin{aligned} a&=b & c&=d \end{aligned}", Style::Text)
        .unwrap();
    let want = [0.0, 8.0636, 18.6191, 32.9108, 40.016, 50.5715];
    let got = relative_xs(&aligned);
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w / 10.0).abs() < 1e-3,
            "{got:?} against TeX's {want:?} pt"
        );
    }
}

#[test]
fn constructs_outside_the_corpus_refuse_by_name() {
    use fmd_math::{ConstructStatus, construct_status};
    let e = engine();
    for (source, construct) in [
        (r"x = y \tag{1}", r"\tag"),
        (r"\sideset{}{'}\sum_n", r"\sideset"),
        (r"\cancelto{0}{x}", r"\cancelto"),
        (r"\begin{equation} x \end{equation}", "env:equation"),
        (r"\begin{gather} x \end{gather}", "env:gather"),
        (r"\begin{align} x \end{align}", "env:align"),
        (r"\begin{multline} x \end{multline}", "env:multline"),
        (r"\begin{multline*} x \end{multline*}", "env:multline*"),
    ] {
        assert_eq!(
            construct_status(construct),
            ConstructStatus::UnsupportedT2,
            "{construct}"
        );
        for err in [
            e.typeset(source, Style::Text).unwrap_err(),
            e.typeset_text(source).unwrap_err(),
        ] {
            let message = err.to_string();
            assert!(
                message.contains(construct) && message.contains("fm-tex-numbered-displays-bxl9"),
                "{source}: {message}"
            );
        }
    }
    // \dx stays the corpus program's standing refusal.
    let dx = e.typeset(r"\dx", Style::Text).unwrap_err().to_string();
    assert!(dx.contains("fm-j5t"), "{dx}");
}
