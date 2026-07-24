//! Style-propagation fixtures: the context every subtree receives under
//! [`fmd_math::style_walk`], against TeX's published rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{Node, NodeKind, Style, StyleCtx, parse, parse_text, style_walk};

/// Collect `(source_slice, style, cramped)` for every Symbol/TextRun leaf.
fn leaf_styles(src: &str, root: &Node, initial: StyleCtx) -> Vec<(String, Style, bool)> {
    let mut out = Vec::new();
    style_walk(root, initial, &mut |node, ctx| match &node.kind {
        NodeKind::Symbol { ch, .. } => out.push((ch.to_string(), ctx.style, ctx.cramped)),
        NodeKind::TextRun(t) => out.push((t.clone(), ctx.style, ctx.cramped)),
        _ => {
            let _ = src;
        }
    });
    out
}

fn styles_of(src: &str) -> Vec<(String, Style, bool)> {
    let root = parse(src).unwrap();
    leaf_styles(src, &root, StyleCtx::display())
}

fn find(styles: &[(String, Style, bool)], leaf: &str) -> (Style, bool) {
    let hit = styles
        .iter()
        .find(|(s, _, _)| s == leaf)
        .unwrap_or_else(|| panic!("leaf `{leaf}` not found in {styles:?}"));
    (hit.1, hit.2)
}

#[test]
fn scripts_go_d_t_to_s_to_ss_with_cramped_subscripts() {
    let styles = styles_of("a^b_c");
    assert_eq!(find(&styles, "a"), (Style::Display, false));
    assert_eq!(find(&styles, "b"), (Style::Script, false));
    assert_eq!(find(&styles, "c"), (Style::Script, true));

    // A script inside a script goes to scriptscript.
    let styles = styles_of("a^{b^c}");
    assert_eq!(find(&styles, "b"), (Style::Script, false));
    assert_eq!(find(&styles, "c"), (Style::ScriptScript, false));

    // Scriptscript is terminal.
    let styles = styles_of("a^{b^{c^d}}");
    assert_eq!(find(&styles, "d"), (Style::ScriptScript, false));
}

#[test]
fn fraction_interiors_step_down_with_cramped_denominators() {
    let styles = styles_of(r"\frac{n}{d}");
    assert_eq!(find(&styles, "n"), (Style::Text, false));
    assert_eq!(find(&styles, "d"), (Style::Text, true));

    // In text style (via \textstyle), interiors go to script.
    let styles = styles_of(r"\textstyle \frac{n}{d}");
    assert_eq!(find(&styles, "n"), (Style::Script, false));
    assert_eq!(find(&styles, "d"), (Style::Script, true));

    // Nested fractions keep stepping down.
    let styles = styles_of(r"\frac{\frac{a}{b}}{c}");
    assert_eq!(find(&styles, "a"), (Style::Script, false));
    assert_eq!(find(&styles, "b"), (Style::Script, true));
    assert_eq!(find(&styles, "c"), (Style::Text, true));
}

#[test]
fn dfrac_and_tfrac_force_their_style() {
    // \dfrac in a script context still lays out as a display fraction:
    // interiors at text style.
    let styles = styles_of(r"x^{\dfrac{a}{b}}");
    assert_eq!(find(&styles, "a"), (Style::Text, false));
    assert_eq!(find(&styles, "b"), (Style::Text, true));

    // \tfrac at top level lays out as a text fraction: interiors at script.
    let styles = styles_of(r"\tfrac{a}{b}");
    assert_eq!(find(&styles, "a"), (Style::Script, false));
    assert_eq!(find(&styles, "b"), (Style::Script, true));
}

#[test]
fn over_interiors_follow_the_ambient_style() {
    let styles = styles_of(r"{a \over b}");
    assert_eq!(find(&styles, "a"), (Style::Text, false));
    assert_eq!(find(&styles, "b"), (Style::Text, true));
}

#[test]
fn radicands_and_accent_bases_are_cramped() {
    let styles = styles_of(r"\sqrt{x}");
    assert_eq!(find(&styles, "x"), (Style::Display, true));

    // The radical index is scriptscript, unconditionally.
    let styles = styles_of(r"\sqrt[i]{x}");
    assert_eq!(find(&styles, "i"), (Style::ScriptScript, false));

    let styles = styles_of(r"\hat{y}");
    assert_eq!(find(&styles, "y"), (Style::Display, true));

    // Cramping propagates into scripts: superscripts inside a radicand
    // stay cramped.
    let styles = styles_of(r"\sqrt{x^k}");
    assert_eq!(find(&styles, "k"), (Style::Script, true));
}

#[test]
fn style_markers_restyle_the_rest_of_their_list() {
    let styles = styles_of(r"a \scriptstyle b c");
    assert_eq!(find(&styles, "a"), (Style::Display, false));
    assert_eq!(find(&styles, "b"), (Style::Script, false));
    assert_eq!(find(&styles, "c"), (Style::Script, false));

    // The marker's reach ends with its group.
    let styles = styles_of(r"{\scriptstyle b} c");
    assert_eq!(find(&styles, "b"), (Style::Script, false));
    assert_eq!(find(&styles, "c"), (Style::Display, false));

    // \displaystyle restores display inside a script.
    let styles = styles_of(r"x^{\displaystyle y}");
    assert_eq!(find(&styles, "y"), (Style::Display, false));
}

#[test]
fn stack_annotations_are_scripts() {
    let styles = styles_of(r"a \stackrel{t}{=} b");
    assert_eq!(find(&styles, "t"), (Style::Script, false));
    assert_eq!(find(&styles, "="), (Style::Display, false));

    let styles = styles_of(r"\underset{u}{X}");
    assert_eq!(find(&styles, "u"), (Style::Script, true));
    assert_eq!(find(&styles, "X"), (Style::Display, false));
}

#[test]
fn math_islands_in_text_enter_at_text_style() {
    let root = parse_text("word $x^2$").unwrap();
    let styles = leaf_styles("word $x^2$", &root, StyleCtx::new(Style::Text));
    assert_eq!(find(&styles, "x"), (Style::Text, false));
    assert_eq!(find(&styles, "2"), (Style::Script, false));
}

#[test]
fn matrix_class_cells_are_text_style_align_cells_keep_ambient() {
    let styles = styles_of(r"\begin{matrix} m \end{matrix}");
    assert_eq!(find(&styles, "m"), (Style::Text, false));

    let styles = styles_of(r"\begin{align*} q &= r \end{align*}");
    assert_eq!(find(&styles, "q"), (Style::Display, false));
}

#[test]
fn big_operator_scripts_are_scripts_regardless_of_limits() {
    let styles = styles_of(r"\sum_{n=1}^{N} a_n");
    assert_eq!(find(&styles, "N"), (Style::Script, false));
    assert_eq!(find(&styles, "1"), (Style::Script, true));
}

#[test]
fn size_factors_follow_the_style() {
    assert_eq!(StyleCtx::display().size_factor(), 1.0);
    assert_eq!(StyleCtx::new(Style::Script).size_factor(), 0.7);
    assert_eq!(StyleCtx::new(Style::ScriptScript).size_factor(), 0.5);
}
