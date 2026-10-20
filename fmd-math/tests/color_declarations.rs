//! Colour declarations (`\color`, `\textcolor`): franken_manim
//! fm-tex-color-silent-i0p2.
//!
//! Semantics (xcolor): `\color{c}` is a declaration. It colours the rest of
//! the enclosing group, nested groups inherit it, and an inner declaration
//! wins inside its own group. `\textcolor{c}{body}` is the group
//! `{\color{c}body}`. Like the size declarations, `\\` does not end the
//! group, and a `$…$` island inside the scope inherits it. Colour never
//! moves anything: every glyph and rule sits exactly where the same source
//! without the colour commands puts it. Primitives outside every
//! declaration carry `None`, the consumer's own colour.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::{Color, Engine, Layout, MathError, Style};

fn engine() -> Engine {
    match Engine::bundled() {
        Ok(e) => e,
        Err(e) => panic!("bundled faces: {e}"),
    }
}

fn math(src: &str) -> Layout {
    engine().typeset(src, Style::Display).unwrap()
}

fn text(src: &str) -> Layout {
    engine().typeset_text(src).unwrap()
}

const RED: Option<Color> = Some(Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
});
const BLUE: Option<Color> = Some(Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
});

/// Each glyph's character and colour, in layout order.
fn colors(layout: &Layout) -> Vec<(char, Option<Color>)> {
    layout.glyphs.iter().map(|g| (g.ch, g.color)).collect()
}

/// Glyph and rule geometry, without colours or spans.
fn geometry(layout: &Layout) -> Vec<(char, f64, f64, f64)> {
    let mut out: Vec<(char, f64, f64, f64)> = layout
        .glyphs
        .iter()
        .map(|g| (g.ch, g.x, g.y, g.size))
        .collect();
    out.extend(layout.rules.iter().map(|r| ('_', r.x, r.y, r.width)));
    out
}

#[test]
fn a_declaration_colours_the_rest_of_its_group() {
    // TeX scoping: everything after `\color` in the same group, so the
    // whole of `x + y`; only `x` when the declaration sits in `{…}`.
    assert_eq!(
        colors(&math(r"\color{red} x + y")),
        vec![('x', RED), ('+', RED), ('y', RED)]
    );
    assert_eq!(
        colors(&math(r"{\color{red} x} + y")),
        vec![('x', RED), ('+', None), ('y', None)]
    );
    assert_eq!(
        colors(&math(r"x + \color{red} y")),
        vec![('x', None), ('+', None), ('y', RED)]
    );
}

#[test]
fn inner_declarations_win_and_end_with_their_group() {
    assert_eq!(
        colors(&math(r"\color{red} a {\color{blue} b} c")),
        vec![('a', RED), ('b', BLUE), ('c', RED)]
    );
    assert_eq!(
        colors(&math(r"a \color{red} b \color{blue} c")),
        vec![('a', None), ('b', RED), ('c', BLUE)]
    );
}

#[test]
fn textcolor_is_a_group_opening_with_the_declaration() {
    let layout = math(r"x + \textcolor{blue}{y} + z");
    assert_eq!(
        colors(&layout),
        vec![
            ('x', None),
            ('+', None),
            ('y', BLUE),
            ('+', None),
            ('z', None)
        ]
    );
    // The span map still names the source character.
    let y = layout.glyphs.iter().find(|g| g.ch == 'y').unwrap();
    assert_eq!(
        &r"x + \textcolor{blue}{y} + z"[y.span.start..y.span.end],
        "y"
    );
}

#[test]
fn fractions_scripts_and_rules_take_the_colour_in_force() {
    let whole = math(r"\color{red}\frac{a}{b}");
    assert!(whole.glyphs.iter().all(|g| g.color == RED), "{whole:?}");
    assert!(whole.rules.iter().all(|r| r.color == RED), "{whole:?}");
    let numerator = math(r"\frac{\color{red}a}{b}");
    assert_eq!(colors(&numerator), vec![('a', RED), ('b', None)]);
    assert!(numerator.rules.iter().all(|r| r.color.is_none()));
    let script = math(r"x^{\color{blue}2}");
    assert_eq!(colors(&script), vec![('x', None), ('2', BLUE)]);
    let both = math(r"\color{red} x^2");
    assert_eq!(colors(&both), vec![('x', RED), ('2', RED)]);
}

#[test]
fn colour_never_moves_anything() {
    // Declarations that add no group: the layout is the colourless one.
    for (plain, colored) in [
        (r"x + y", r"\color{red} x + y"),
        (r"x + y", r"x + \color{blue} y"),
        (
            r"\frac{a}{b} = c^2",
            r"\color{red}\frac{a}{b} = \color[RGB]{0,128,255} c^2",
        ),
        (r"a \\ b", r"\color{red} a \\ b"),
    ] {
        assert_eq!(
            geometry(&math(plain)),
            geometry(&math(colored)),
            "{colored}"
        );
    }
    // `\textcolor{c}{…}` is exactly the group `{\color{c}…}` (a group is an
    // Ord atom, in TeX as here, so it is not compared with the bare body).
    assert_eq!(
        geometry(&math(r"a + \textcolor{blue}{\sum_{n=1}^N} n")),
        geometry(&math(r"a + {\color{blue}\sum_{n=1}^N} n"))
    );
}

#[test]
fn the_declaration_crosses_line_breaks_and_enters_math_islands() {
    assert_eq!(
        colors(&math(r"\color{red} a \\ b")),
        vec![('a', RED), ('b', RED)]
    );
    let layout = text(r"ab \color{blue} $x$ cd");
    for g in &layout.glyphs {
        let want = if "xcd".contains(g.ch) { BLUE } else { None };
        assert_eq!(g.color, want, "{}", g.ch);
    }
}

#[test]
fn text_mode_declarations_and_textcolor() {
    let layout = text(r"ab \textcolor{red}{cd} ef");
    for g in &layout.glyphs {
        let want = if "cd".contains(g.ch) { RED } else { None };
        assert_eq!(g.color, want, "{}", g.ch);
    }
    let layout = text(r"plain {\color{blue} blue} plain");
    let blue = layout.glyphs.iter().filter(|g| g.color == BLUE).count();
    assert_eq!(blue, 4, "{layout:?}");
}

#[test]
fn unsupported_colours_fail_by_name() {
    let err = engine()
        .typeset(r"\color{RED} x", Style::Display)
        .unwrap_err();
    let MathError::Malformed { what, at } = err else {
        panic!("{err:?}");
    };
    assert!(
        what.contains("unsupported colour `RED` in \\color"),
        "{what}"
    );
    assert_eq!(at, 7);
    // CSS names and bare hex parse (MathML carries them to the browser
    // verbatim) but have no xcolor value to lay out; bad models and broken
    // arguments fail at parse.
    for src in [
        r"\color{#00ff00} x",
        r"\color{steelblue} x",
        // A declaration in a script position colours nothing, yet still refuses.
        r"x^\color{steelblue}",
        r"\color[cmyk]{0,1,1,0} x",
        r"\textcolor{red}",
        r"\color[RGB {1,2,3} x",
    ] {
        assert!(engine().typeset(src, Style::Display).is_err(), "{src}");
    }
    assert!(fmd_math::parse(r"\color{steelblue} x").is_ok());
    assert!(fmd_math::parse(r"\color[cmyk]{0,1,1,0} x").is_err());
    // `\textcolor{c}x` takes the next token as its body, as TeX does.
    assert_eq!(
        colors(&math(r"\textcolor{red} x y")),
        vec![('x', RED), ('y', None)]
    );
}
