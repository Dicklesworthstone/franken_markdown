//! Size declarations (`\tiny`, `\footnotesize`, `\small`, `\large`,
//! `\Large`, `\huge`): the fm-j5t.2 acceptance suite.
//!
//! Semantics (LaTeX2e, `classes.dtx` §6.1 / `size10.clo`): the commands
//! are *declarations* — they set the current size factor for the rest of
//! the enclosing group, absolutely (not cumulatively), exiting at group
//! end. The factor multiplies the math style's own size factor, so the
//! script styles compose multiplicatively with declarations, exactly as
//! TeX's script/scriptscript sizes track the current size. `\\` does not
//! end a group, so a declaration persists across line breaks; a `$…$`
//! island inside a declaration's scope inherits it (math in a `\small`
//! scope is small).
//!
//! The factor ladder is the LaTeX 10 pt class option (`size10.clo`):
//! tiny 5, scriptsize 7, footnotesize 8, small 9, normalsize 10,
//! large 12, Large 14.4, LARGE 17.28, huge 20.74, Huge 24.88 pt — here
//! relative to the 10 pt base. The corpus exercises six of the ten.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::{Engine, Layout, MathError, Style, parse, parse_text};

fn engine() -> Engine {
    match Engine::bundled() {
        Ok(e) => e,
        Err(e) => panic!("bundled faces: {e}"),
    }
}

fn glyph_size(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
        .size
}

const EPS: f64 = 1e-12;

/// Every glyph's size factor for `src` in display math style.
fn sizes(src: &str) -> Layout {
    engine().typeset(src, Style::Display).unwrap()
}

#[test]
fn factor_table_is_the_latex_10pt_ladder() {
    // The six corpus commands at their `size10.clo` factors (the full
    // ladder: scriptsize 0.7, normalsize 1.0, LARGE 1.728, Huge 2.488).
    for (cmd, factor) in [
        (r"\tiny", 0.5),
        (r"\footnotesize", 0.8),
        (r"\small", 0.9),
        (r"\large", 1.2),
        (r"\Large", 1.44),
        (r"\huge", 2.074),
    ] {
        let l = sizes(&format!(r"a {cmd} b"));
        assert!(
            (glyph_size(&l, 'a') - 1.0).abs() < EPS,
            "{cmd}: base {}",
            glyph_size(&l, 'a')
        );
        assert!(
            (glyph_size(&l, 'b') - factor).abs() < EPS,
            "{cmd}: declared {}",
            glyph_size(&l, 'b')
        );
    }
}

#[test]
fn declarations_are_absolute_not_cumulative() {
    // A second declaration replaces the first; it does not multiply it.
    let l = sizes(r"\small \Large a");
    assert!((glyph_size(&l, 'a') - 1.44).abs() < EPS);
    let l = sizes(r"\Large \tiny a");
    assert!((glyph_size(&l, 'a') - 0.5).abs() < EPS);
}

#[test]
fn declaration_exits_at_group_end() {
    let l = sizes(r"{\small a} b");
    assert!((glyph_size(&l, 'a') - 0.9).abs() < EPS);
    assert!((glyph_size(&l, 'b') - 1.0).abs() < EPS);
}

#[test]
fn nested_declarations_restore_outward() {
    let l = sizes(r"{\small a {\Large b} c}");
    assert!((glyph_size(&l, 'a') - 0.9).abs() < EPS);
    assert!((glyph_size(&l, 'b') - 1.44).abs() < EPS);
    assert!((glyph_size(&l, 'c') - 0.9).abs() < EPS);
}

#[test]
fn declaration_at_group_end_is_harmless() {
    // Nothing follows the declaration: it marks an empty remainder,
    // produces no glyphs, and the layout is just the leading atom.
    let l = sizes(r"a {\small}");
    assert_eq!(l.glyphs.len(), 1, "{l:?}");
    assert!((glyph_size(&l, 'a') - 1.0).abs() < EPS);
}

#[test]
fn scope_inside_fraction() {
    // A declaration inside the numerator is confined to it (display
    // fraction: numerator and denominator both at text style here).
    let l = sizes(r"\frac{\small a}{b}");
    assert!((glyph_size(&l, 'a') - 0.9).abs() < EPS);
    assert!((glyph_size(&l, 'b') - 1.0).abs() < EPS);
    // A declaration around the fraction flows through numerator and
    // denominator (both inherit the group's factor), then exits.
    let l = sizes(r"{\small \frac{a}{b}} c");
    assert!((glyph_size(&l, 'a') - 0.9).abs() < EPS);
    assert!((glyph_size(&l, 'b') - 0.9).abs() < EPS);
    assert!((glyph_size(&l, 'c') - 1.0).abs() < EPS);
}

#[test]
fn scripts_compose_multiplicatively() {
    // The script style's 0.7 composes with the declaration's 0.9.
    let l = sizes(r"x^{\small y}");
    assert!((glyph_size(&l, 'y') - 0.7 * 0.9).abs() < EPS);
    let l = sizes(r"{\small x^y}");
    assert!((glyph_size(&l, 'y') - 0.7 * 0.9).abs() < EPS);
    // Second-order scripts: scriptscript's 0.5 with the declaration.
    let l = sizes(r"x^{y^{\small z}}");
    assert!((glyph_size(&l, 'z') - 0.5 * 0.9).abs() < EPS);
}

#[test]
fn declaration_persists_across_linebreak() {
    // `\\` does not end the group: the second line keeps the factor.
    let l = sizes(r"{\small a \\ b}");
    assert!((glyph_size(&l, 'a') - 0.9).abs() < EPS);
    assert!((glyph_size(&l, 'b') - 0.9).abs() < EPS);
}

#[test]
fn declarations_produce_no_glyphs_and_keep_provenance() {
    // The marker itself renders nothing; every glyph carries a nonempty
    // in-source span.
    let src = r"x \small y \huge z";
    let l = sizes(src);
    assert_eq!(l.glyphs.len(), 3, "{l:?}");
    for g in &l.glyphs {
        assert!(
            g.span.end <= src.len() && g.span.start < g.span.end,
            "{g:?}"
        );
    }
}

#[test]
fn text_mode_declarations_apply_to_the_mainland() {
    let e = engine();
    let l = e.typeset_text(r"\small Calvin:").unwrap();
    assert!(!l.glyphs.is_empty());
    for g in &l.glyphs {
        assert!((g.size - 0.9).abs() < EPS, "{g:?}");
    }
    let l = e.typeset_text(r"\tiny Move disk 0, Move disk 0").unwrap();
    for g in &l.glyphs {
        assert!((g.size - 0.5).abs() < EPS, "{g:?}");
    }
    // A declaration after some text leaves the prefix at the base size.
    let l = e.typeset_text(r"AB \huge C").unwrap();
    assert!((glyph_size(&l, 'A') - 1.0).abs() < EPS);
    assert!((glyph_size(&l, 'C') - 2.074).abs() < EPS);
}

#[test]
fn math_island_inherits_the_declaration() {
    // The corpus idiom: `\small` in the text mainland, then a `$…$`
    // island — the island's mathematics is set at the declared size.
    let l = engine()
        .typeset_text(r"\text{\small $\frac{1}{24}$}")
        .unwrap();
    // Inline island at text style; the fraction's numerator drops to
    // script style: 0.9 × 0.7.
    assert!(
        (glyph_size(&l, '1') - 0.9 * 0.7).abs() < EPS,
        "{}",
        glyph_size(&l, '1')
    );
}

#[test]
fn corpus_strings_parse_and_lay_out() {
    // The real 3b1b corpus strings containing size declarations
    // (tex_corpus.jsonl; `mode` as recorded).
    let e = engine();
    let math = [
        // optics_puzzles/slowing_waves.py
        r"\text{Index of refraction } = {\small \text{Speed in a vacuum} \over \text{Speed in medium}} = 1.00",
        // tau_poem.py
        r"\small",
        // eoc/chapter10.py
        r"\text{\small $\frac{1}{24}$}",
    ];
    for src in math {
        parse(src).unwrap_or_else(|err| panic!("`{src}` failed to parse: {err}"));
        e.typeset(src, Style::Display)
            .unwrap_or_else(|err| panic!("`{src}` failed to lay out: {err}"));
    }
    let text = [
        // diffyq/part1/wordy_scenes.py
        r"\Large Language of differential equations",
        // alt_calc.py
        r"\Large",
        // brachistochrone/wordplay.py
        r"\large Warm-up challenge: Confirm this for yourself",
        // quaternions.py
        "\\huge ...Every morning in the early part of the above-cited\n            month, on my coming down to breakfast, your (then)\n            little brother William Edwin, and yourself, used to\n            ask me,",
        // eola/chapter7.py
        r"\small Calvin:",
        // hanoi.py
        r"\tiny Move disk 0, Move disk 0",
        // sphere_area.py: declaration + linebreaks + math islands
        "\\small\n            Question \\#1: What is the circumference of\\\\\n            one of these rings (in terms of $R$ and $\\theta$)?\\\\",
    ];
    for src in text {
        parse_text(src).unwrap_or_else(|err| panic!("`{src}` failed to parse: {err}"));
        e.typeset_text(src)
            .unwrap_or_else(|err| panic!("`{src}` failed to lay out: {err}"));
    }
}

#[test]
fn corpus_sphere_area_string_sizes() {
    // The sphere_area string end-to-end: `\small` at the head of the
    // mainland, a `\\` line break, then `$…$` islands — the islands sit
    // on the second line and must still see the declaration.
    let src = "\\small\n            Question \\#1: What is the circumference of\\\\\n            one of these rings (in terms of $R$ and $\\theta$)?\\\\";
    let l = engine().typeset_text(src).unwrap();
    assert!(
        (glyph_size(&l, 'R') - 0.9).abs() < EPS,
        "{}",
        glyph_size(&l, 'R')
    );
    assert!(
        (glyph_size(&l, 'θ') - 0.9).abs() < EPS,
        "{}",
        glyph_size(&l, 'θ')
    );
}

#[test]
fn declarations_are_not_arguments() {
    // Like `\displaystyle`, a declaration cannot fill an argument slot.
    let err = parse(r"\frac\small{a}{b}").unwrap_err();
    let MathError::Malformed { what, .. } = &err else {
        panic!("expected Malformed, got {err:?}");
    };
    assert!(
        what.contains("cannot be used in argument position"),
        "{what}"
    );
}

#[test]
fn chaos_remains_precise() {
    // Arbitrary token soup around the new commands still fails with a
    // precise error (or parses), never a hang or a panic.
    for src in [
        r"\small\small\small",
        r"{\small",
        r"\small}",
        r"\huge{\tiny{\Large}}",
        r"\sqrt\small{x}",
        r"\small\limits x",
    ] {
        let _ = parse(src);
    }
}
