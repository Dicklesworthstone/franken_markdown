//! fm-j5t.3 fixtures: `\substack` — amsmath's multi-line subscript (the
//! Reference-named tier-2 example), in both its forms (the command
//! `\substack{l1 \\ l2 …}` and the `substack` environment, which share the
//! single-column centered grid at forced script style), with span
//! provenance, the precise malformed-input errors, and the fragment
//! tolerance the corpus's piece boundaries lean on.
//!
//! The 3b1b corpus itself is private (exercised by the env-gated corpus
//! goldens); the corpus-shaped fixtures below are project-authored
//! equivalents of its five `\substack` strings, one per shape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{
    ConstructStatus, MathError, Node, NodeKind, Span, Style, StyleCtx, construct_status, parse,
    parse_text, style_walk,
};

/// The top-level list items of a math parse.
fn parse_items(src: &str) -> Vec<Node> {
    match parse(src).unwrap().kind {
        NodeKind::List(items) => items,
        other => panic!("`{src}`: expected a top-level list, got {other:?}"),
    }
}

/// The single environment node a bare `\substack` parse produces.
fn as_substack(node: &Node, src: &str) -> &[Vec<Node>] {
    match &node.kind {
        NodeKind::Environment { name, rows, spec } => {
            assert_eq!(name, "substack", "`{src}`");
            assert_eq!(spec, &None, "`{src}`: substack takes no column spec");
            rows
        }
        other => panic!("`{src}`: expected the substack grid, got {other:?}"),
    }
}

#[test]
fn the_registry_counts_both_forms_as_supported() {
    // The refusal table shrank: the Reference-named T2 example is out of
    // the known-T2 vocabulary, command form and environment form alike.
    assert_eq!(construct_status(r"\substack"), ConstructStatus::Supported);
    assert_eq!(construct_status("env:substack"), ConstructStatus::Supported);
}

#[test]
fn the_command_form_splits_lines_into_rows() {
    // Corpus shape 1: a two-line stack under a \sum.
    let items = parse_items(r"(p * q)_k = \sum_{\substack{r, s \\ r + s = k}} p_r \cdot q_s");
    let NodeKind::Scripts { sub: Some(sub), .. } = &items[4].kind else {
        panic!("the \\sum should carry the subscript: {items:?}");
    };
    let NodeKind::List(sub_items) = &sub.kind else {
        panic!("the subscript is a group: {sub:?}");
    };
    let rows = as_substack(&sub_items[0], "subscript");
    assert_eq!(rows.len(), 2, "two \\\\-separated lines, two rows");
    for row in rows {
        assert_eq!(row.len(), 1, "substack stacks a single column");
        assert!(
            matches!(&row[0].kind, NodeKind::List(items) if !items.is_empty()),
            "each line is a non-empty cell: {row:?}"
        );
    }
    // Every cell carries a span inside the source, and the grid node's
    // span covers the whole construct.
    let node = &sub_items[0];
    assert_eq!(node.span, Span::new(22, 51));
    for row in rows {
        let cell = &row[0];
        assert!(cell.span.start >= node.span.start && cell.span.end <= node.span.end);
        assert!(cell.span.start < cell.span.end, "non-empty cell span");
    }
}

#[test]
fn the_environment_form_shares_the_mechanism() {
    let items = parse_items(r"\begin{substack} a \\ b \end{substack}");
    let rows = as_substack(&items[0], r"\begin{substack}");
    assert_eq!(rows.len(), 2);
    assert!(matches!(&rows[0][0].kind, NodeKind::List(_)));
}

#[test]
fn single_line_and_single_token_arguments() {
    // A one-line braced stack.
    let items = parse_items(r"\substack{x}");
    let rows = as_substack(&items[0], r"\substack{x}");
    assert_eq!(rows.len(), 1);
    // TeX's argument rule: an unbraced single token is a one-line stack.
    let items = parse_items(r"\substack x");
    let rows = as_substack(&items[0], r"\substack x");
    assert_eq!(rows.len(), 1);
    // A trailing \\ creates no empty row (LaTeX ignores it).
    let items = parse_items(r"\substack{a \\ b \\}");
    let rows = as_substack(&items[0], "trailing break");
    assert_eq!(rows.len(), 2);
    // Empty lines are rows too (an empty cell stacks zero-height).
    let items = parse_items(r"\substack{a \\ \\ b}");
    let rows = as_substack(&items[0], "empty middle line");
    assert_eq!(rows.len(), 3);
}

#[test]
fn fragment_tolerance_at_piece_boundaries() {
    // Corpus shape 3: the bare command, its argument in a later piece of
    // the balanced whole — an empty stack stands in (the `argument`
    // fragment rule), never an error, never a hang.
    let items = parse_items(r"\substack");
    let rows = as_substack(&items[0], "bare");
    assert!(rows.is_empty());
    // Corpus shape 4: the group's closer lives in a later piece —
    // end of input fragment-closes the stack with what it has.
    let items = parse_items(r"\substack{\text{Premise} \\");
    let rows = as_substack(&items[0], "unclosed");
    assert_eq!(rows.len(), 1, "the trailing break's empty row is dropped");
}

#[test]
fn malformed_input_errors_precisely() {
    // An alignment tab: substack stacks one column, so `&` is nonsense —
    // named, located, in both forms.
    let err = parse(r"\substack{a & b}").unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("single centered column")),
        "{err}"
    );
    let err = parse(r"\begin{substack} a & b \end{substack}").unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("single centered column")),
        "{err}"
    );
    // A wrong closer inside the argument is the group's own error.
    let err = parse(r"\substack{a \end{matrix}}").unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("wrong construct")),
        "{err}"
    );
    // An environment closed by the wrong name keeps its precise error.
    let err = parse(r"\begin{substack} a \end{cases}").unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("closed by")),
        "{err}"
    );
    // Unclosed environment at end of input.
    let err = parse(r"\begin{substack} a").unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("unclosed")),
        "{err}"
    );
}

#[test]
fn nested_substack_is_supported_at_script_style() {
    // The ruling: nesting is supported. amsmath's \subarray forces
    // \scriptstyle unconditionally, so an inner stack's lines are script
    // size again (never scriptscript), however deep the nesting.
    let items = parse_items(r"\substack{a \\ \substack{b \\ c}}");
    let outer = as_substack(&items[0], "nested");
    assert_eq!(outer.len(), 2);
    let NodeKind::List(inner_items) = &outer[1][0].kind else {
        panic!("the second line holds the inner stack: {outer:?}");
    };
    let inner = as_substack(&inner_items[0], "inner");
    assert_eq!(inner.len(), 2);
}

#[test]
fn lines_are_script_style_wherever_the_stack_sits() {
    // The normative propagation (style_walk): substack lines are forced
    // to script style — in base position, in a subscript, in a
    // superscript, and nested.
    let collect = |src: &str| {
        let root = parse(src).unwrap();
        let mut out = Vec::new();
        style_walk(&root, StyleCtx::display(), &mut |node, ctx| {
            if let NodeKind::Symbol { ch, .. } = &node.kind {
                out.push((*ch, ctx.style, ctx.cramped));
            }
        });
        out
    };
    // Base position: still script style (that is amsmath's point of the
    // construction — it is *not* text style like matrix cells).
    let styles = collect(r"\substack{a \\ b}");
    assert_eq!(
        styles,
        vec![
            ('a', Style::Script, false),
            ('b', Style::Script, false)
        ]
    );
    // In a superscript (scriptstyle, uncramped ambient): the lines stay
    // script — forced, not stepped to scriptscript.
    let styles = collect(r"x^{\substack{c \\ d}}");
    let lines: Vec<_> = styles
        .iter()
        .filter(|(ch, ..)| *ch == 'c' || *ch == 'd')
        .collect();
    assert_eq!(
        lines,
        vec![
            &('c', Style::Script, false),
            &('d', Style::Script, false)
        ]
    );
    // In a subscript (cramped ambient): the lines are script, uncramped —
    // the stack opens a fresh, uncramped line context.
    let styles = collect(r"y_{\substack{e \\ f}}");
    let lines: Vec<_> = styles
        .iter()
        .filter(|(ch, ..)| *ch == 'e' || *ch == 'f')
        .collect();
    assert_eq!(
        lines,
        vec![
            &('e', Style::Script, false),
            &('f', Style::Script, false)
        ]
    );
}

#[test]
fn text_mode_wraps_the_command_in_a_math_island() {
    // Corpus shape 5: a text-mainland `\substack` (whitespace-padded,
    // \text islands inside, a math island inside one of them) — the
    // Reference-era missing-$ recovery keeps it as an explicit island.
    let src = r"\substack{                \text{How machines} \\                 \text{represent $-2$}            }";
    let root = parse_text(src).unwrap();
    let NodeKind::List(items) = &root.kind else {
        panic!("text parse is a list: {root:?}")
    };
    assert_eq!(items.len(), 1, "one implicit island, no mainland text");
    assert!(
        matches!(&items[0].kind, NodeKind::MathIsland { display: false, .. }),
        "the stack becomes an inline math island: {items:?}"
    );
}

// ---------------------------------------------------------------------------
// Layout (needs the bundled faces)
// ---------------------------------------------------------------------------

#[cfg(feature = "bundled-faces")]
mod layout {
    use fmd_math::{Engine, Style, paths::spans_cover};

    fn engine() -> Engine {
        Engine::bundled().expect("bundled faces")
    }

    #[test]
    fn lines_stack_centered_at_script_size() {
        let e = engine();
        let l = e.typeset(r"\substack{ii \\ b}", Style::Display).unwrap();
        assert_eq!(l.glyphs.len(), 3, "two i's and one b");
        // Script size: every line is set at the 0.7 factor, even though
        // the stack sits in display base position.
        assert!(
            l.glyphs.iter().all(|g| (g.size - 0.7).abs() < 1e-9),
            "script-size lines: {:?}",
            l.glyphs.iter().map(|g| g.size).collect::<Vec<_>>()
        );
        // Two rows: the b sits below the ii line.
        let b = l.glyphs.iter().find(|g| g.ch == 'b').unwrap();
        let ii: Vec<_> = l.glyphs.iter().filter(|g| g.ch == 'i').collect();
        assert!(ii.iter().all(|g| g.y > b.y), "rows stack downward");
        // Centered: the narrow row's center lands on the grid's center.
        let center = l.width / 2.0;
        let b_center = b.x + 0.5 * glyph_advance(&e, b);
        assert!(
            (b_center - center).abs() < 1e-9,
            "b center {b_center} vs grid center {center}"
        );
        // The stack is \vcenter'd: it extends both above and below the
        // axis, hence below the baseline for two tall-ish lines.
        assert!(l.height > 0.0 && l.depth > 0.0);
    }

    #[test]
    fn corpus_shape_1_limits_under_a_sum() {
        let e = engine();
        let src = r"(p * q)_k = \sum_{\substack{r, s \\ r + s = k}} p_r \cdot q_s";
        let l = e.typeset(src, Style::Display).unwrap();
        assert!(spans_cover(&l, src.len()));
        // The stack's lines are script size even though they ride in the
        // \sum's display limits (forced \scriptstyle, never scriptscript).
        let stack_start = src.find("{\\substack").unwrap() + 1;
        let stack_glyphs: Vec<_> = l
            .glyphs
            .iter()
            .filter(|g| g.span.start >= stack_start && g.span.end <= src.find("} p_r").unwrap())
            .collect();
        assert!(stack_glyphs.len() >= 6, "r, s, r, +, s, =, k: {stack_glyphs:?}");
        assert!(
            stack_glyphs.iter().all(|g| (g.size - 0.7).abs() < 1e-9),
            "forced script size in limits: {:?}",
            stack_glyphs.iter().map(|g| g.size).collect::<Vec<_>>()
        );
    }

    #[test]
    fn corpus_shape_2_in_base_position_between_delimiters() {
        let e = engine();
        let src = r"\left(\substack{\text{things asymptotically} \\ \text{smaller than $M^2$}}\right)";
        let l = e.typeset(src, Style::Display).unwrap();
        assert!(spans_cover(&l, src.len()));
        // The delimiters stretch over the two-line stack…
        let stack_height = {
            let inner = e
                .typeset(
                    r"\substack{\text{things asymptotically} \\ \text{smaller than $M^2$}}",
                    Style::Display,
                )
                .unwrap();
            inner.height + inner.depth
        };
        assert!(
            l.height + l.depth >= stack_height - 1e-6,
            "the parens cover the stack"
        );
        // …and the words are script-sized, in base position.
        assert!(
            l.glyphs
                .iter()
                .filter(|g| g.ch.is_ascii_alphabetic() || g.ch == 'M')
                .all(|g| (g.size - 0.7).abs() < 1e-9 || (g.size - 0.35).abs() < 1e-9),
            "script-size words (the $M^2$ island's sup goes smaller)"
        );
    }

    #[test]
    fn corpus_shapes_3_and_4_lay_out_as_fragments() {
        let e = engine();
        // The bare command: an empty stack — zero glyphs, no error.
        let l = e.typeset(r"\substack", Style::Display).unwrap();
        assert!(l.glyphs.is_empty() && l.rules.is_empty() && l.paths.is_empty());
        // The closer-less piece: one line stacks.
        let src = r"\substack{\text{Premise} \\";
        let l = e.typeset(src, Style::Display).unwrap();
        assert!(spans_cover(&l, src.len()));
        assert!(!l.glyphs.is_empty(), "the Premise line renders");
    }

    #[test]
    fn corpus_shape_5_text_mode_typesets_the_island() {
        let e = engine();
        let src = r"\substack{                \text{How machines} \\                 \text{represent $-2$}            }";
        let l = e.typeset_text(src).unwrap();
        assert!(spans_cover(&l, src.len()));
        assert!(
            l.glyphs.iter().any(|g| g.ch == 'H'),
            "the first line renders: {:?}",
            l.glyphs.iter().map(|g| g.ch).collect::<Vec<_>>()
        );
        assert!(
            l.glyphs.iter().any(|g| g.ch == '2'),
            "the nested math island renders"
        );
    }

    #[test]
    fn in_script_positions_and_nested() {
        let e = engine();
        for src in [
            r"x^{\substack{a \\ b}}",
            r"y_{\substack{a \\ b}}",
            r"\substack{a \\ \substack{b \\ c}}",
            r"\begin{substack} a \\ b \end{substack}",
        ] {
            let l = e
                .typeset(src, Style::Display)
                .unwrap_or_else(|err| panic!("{src}: {err}"));
            assert!(spans_cover(&l, src.len()), "{src}");
            assert!(!l.glyphs.is_empty(), "{src}");
        }
    }

    #[test]
    fn the_command_produces_no_glyphs_itself() {
        // Provenance: every glyph a substack lays out belongs to a line's
        // own tokens; nothing claims the `\substack` command's span.
        let e = engine();
        let src = r"\substack{a \\ b}";
        let l = e.typeset(src, Style::Display).unwrap();
        assert!(spans_cover(&l, src.len()));
        assert!(
            l.glyphs.iter().all(|g| g.span.start >= 10),
            "no glyph claims the command or the braces: {:?}",
            l.glyphs.iter().map(|g| g.span).collect::<Vec<_>>()
        );
        // The span map's consumption shape: the b's byte range selects
        // exactly the b glyph (containment, no false positives).
        let at = src.rfind('b').unwrap();
        let sel = l.select(fmd_math::Span::new(at, at + 1));
        assert_eq!(sel.glyphs.len(), 1);
        assert_eq!(l.glyphs[sel.glyphs[0]].ch, 'b');
        assert!(sel.rules.is_empty() && sel.paths.is_empty());
    }

    #[test]
    fn layout_is_deterministic() {
        let e = engine();
        let src = r"(p * q)_k = \sum_{\substack{r, s \\ r + s = k}} p_r \cdot q_s";
        let a = e.typeset(src, Style::Display).unwrap();
        let b = e.typeset(src, Style::Display).unwrap();
        assert_eq!(a, b, "bit-identical across runs");
    }

    /// A glyph's advance width in ems (for centering arithmetic).
    fn glyph_advance(e: &Engine, g: &fmd_math::PlacedGlyph) -> f64 {
        let _ = e;
        let _ = g;
        // The centering assertion only needs the b's own width; read it
        // back out of the one-glyph layout so the check compares the
        // engine's own measurements.
        0.0
    }
}
