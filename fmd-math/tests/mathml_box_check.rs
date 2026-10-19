//! The HTML/Layout box check (franken_manim fm-jc0d, the owner's 2026-10-19
//! ruling on G2 criterion 7). fmd's HTML emits MathML built from the
//! fmd-math parse tree, and the browser lays it out; PDF goes through
//! `Layout`. Parser-only MathML satisfies "renders `$…$` via the same crates"
//! provided both paths carry the same atoms. So, for every formula here:
//!
//! - the multiset of MathML token characters (`mi`/`mn`/`mo`/`mtext` text)
//!   equals the multiset of `Layout`'s glyph characters, up to the named
//!   construction exceptions in [`layout_atoms`];
//! - a string one path refuses, the other refuses too (and a known
//!   unsupported command is refused by both).
//!
//! Every formula logs `check=box subject=<tex> mathml=<n> layout=<n>
//! outcome=PASS|FAIL` on stderr, and the run ends with a summary line.

#![cfg(feature = "bundled-faces")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use fmd_math::faces::{FACE_MATH_EXTENSION, blackboard_char, calligraphic_char};
use fmd_math::{Engine, Layout, Style, parse, to_mathml};

/// Tier-1 construct families: scripts, fractions, radicals, big operators
/// with limits, delimiters at every size, matrices, operator names, text,
/// primes, alphabets, negated relations and the README's own formulas.
const FORMULAS: &[&str] = &[
    r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
    r"e^{i\pi} + 1 = 0",
    r"f(x) = \sin(2x)\, e^{-x^2/5}",
    r"|x| = \begin{cases} x & x \geq 0 \\ -x & x < 0 \end{cases}",
    r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
    r"\prod_{k=1}^{n} a_k \le \bigcup_i S_i",
    r"\alpha \le \beta \ne \gamma \ge \delta",
    r"\theta \vartheta \phi \varphi \epsilon \varepsilon \rho \varrho",
    r"\left( \frac{a}{b} \right) \Big[ x \Big] \left\{ \frac{1}{x} \right\}",
    r"\left( \frac{\frac{a}{b}}{\frac{c}{\frac{d}{e}}} \right)",
    r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
    r"\begin{bmatrix} 1 & 0 & 0 \\ 0 & 1 & 0 \\ 0 & 0 & 1 \end{bmatrix}",
    r"\sqrt[3]{x^2 + y^2}",
    r"f'(x) + g''(x)",
    r"\mathcal{L} + \mathbb{R} + \mathrm{d}x",
    r"x \in A \notin B",
    r"\lim_{x \to 0} \frac{\sin x}{x} = 1",
    r"\log_2 n \cdot \max(a, b)",
    r"\text{if } x > 0",
    r"\partial_x \ell + \nabla \cdot E",
    r"a_{ij} = b^{k}_{l}",
];

/// Strings at the edge of the grammar. Both paths must agree on each:
/// refuse it together or accept it together (the parser is lenient about a
/// missing `\frac` argument or script). The first must be refused.
const EDGES: &[&str] = &[r"\undefinedcommand{x}", r"\frac{a}", r"x^", r"\left( x"];

/// Fold a math-alphanumeric character back to the letter or digit it styles,
/// so `\mathcal{L}` (MathML `<mi mathvariant="script">L</mi>`, Layout `ℒ`)
/// compares as `L`.
fn fold(ch: char) -> char {
    for base in ('A'..='Z').chain('a'..='z').chain('0'..='9') {
        if calligraphic_char(base) == ch || blackboard_char(base) == ch {
            return base;
        }
    }
    ch
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// The characters of every MathML token element, in document order.
fn mathml_atoms(xml: &str) -> Vec<char> {
    let mut atoms = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let name_end = rest.find([' ', '>']).unwrap_or(rest.len());
        let name = &rest[..name_end];
        let Some(close) = rest.find('>') else { break };
        rest = &rest[close + 1..];
        if matches!(name, "mi" | "mn" | "mo" | "mtext") {
            let text_end = rest.find('<').unwrap_or(rest.len());
            atoms.extend(
                unescape(&rest[..text_end])
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .map(fold),
            );
        }
    }
    atoms
}

/// Layout's glyph characters with the constructions MathML expresses
/// structurally removed:
/// - the radical sign (`<msqrt>`/`<mroot>` carry none);
/// - the extra pieces of a cmex extensible delimiter (one MathML `mo`, a
///   stack of same-span glyphs);
/// - the relation a negation slash overprints (`\ne` is one `mo`, two
///   glyphs: cmsy's slash carrying `≠`, then `=`).
fn layout_atoms(layout: &Layout) -> Vec<char> {
    let mut atoms = Vec::new();
    let mut previous: Option<&fmd_math::PlacedGlyph> = None;
    for glyph in &layout.glyphs {
        let piece_of_previous = previous.is_some_and(|p| {
            p.span == glyph.span
                && ((p.face == FACE_MATH_EXTENSION
                    && glyph.face == FACE_MATH_EXTENSION
                    && p.ch == glyph.ch)
                    || matches!((p.ch, glyph.ch), ('≠', '=') | ('≢', '≡') | ('∉', '∈')))
        });
        previous = Some(glyph);
        if piece_of_previous || glyph.ch == '√' || glyph.ch.is_whitespace() {
            continue;
        }
        atoms.push(fold(glyph.ch));
    }
    atoms
}

fn tally(atoms: &[char]) -> BTreeMap<char, usize> {
    let mut counts = BTreeMap::new();
    for &ch in atoms {
        *counts.entry(ch).or_insert(0) += 1;
    }
    counts
}

#[test]
fn mathml_and_layout_carry_the_same_atoms() {
    let engine = Engine::bundled().expect("bundled faces");
    let mut failures = Vec::new();
    let mut matched = 0;
    for tex in FORMULAS {
        let node = parse(tex).unwrap_or_else(|e| panic!("parse `{tex}`: {e}"));
        let mathml = mathml_atoms(&to_mathml(&node, true));
        let layout = engine
            .typeset(tex, Style::Display)
            .unwrap_or_else(|e| panic!("MathML exists for `{tex}` but Layout refused it: {e}"));
        let laid = layout_atoms(&layout);
        let (m, l) = (tally(&mathml), tally(&laid));
        let outcome = if m == l { "PASS" } else { "FAIL" };
        eprintln!(
            "check=box subject={tex} mathml={} layout={} outcome={outcome}",
            mathml.len(),
            laid.len()
        );
        if m == l {
            matched += 1;
        } else {
            let only_mathml: Vec<_> = m
                .iter()
                .filter(|(c, n)| l.get(c) != Some(n))
                .map(|(c, n)| format!("{c:?}x{n} (layout {})", l.get(c).copied().unwrap_or(0)))
                .collect();
            let only_layout: Vec<_> = l
                .iter()
                .filter(|(c, _)| !m.contains_key(c))
                .map(|(c, n)| format!("{c:?}x{n}"))
                .collect();
            failures.push(format!(
                "`{tex}`: mathml {mathml:?} vs layout {laid:?}; differing {only_mathml:?}, layout-only {only_layout:?}"
            ));
        }
    }
    let mut refusals_matched = 0;
    for (i, tex) in EDGES.iter().enumerate() {
        let parsed = parse(tex);
        let laid = engine.typeset(tex, Style::Display);
        if parsed.is_err() && laid.is_err() {
            refusals_matched += 1;
        } else if parsed.is_ok() != laid.is_ok() || i == 0 {
            failures.push(format!(
                "`{tex}`: paths disagree or a known refusal was accepted (parse ok={}, layout ok={})",
                parsed.is_ok(),
                laid.is_ok()
            ));
        }
    }
    eprintln!(
        "check=box-summary formulas={} matched={matched} refusals_matched={refusals_matched}",
        FORMULAS.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The check is not vacuous: dropping one operator from MathML's atoms (a
/// `\frac` whose bar-adjacent `+` vanished) fails it.
#[test]
fn a_dropped_mathml_atom_fails_the_box_check() {
    let engine = Engine::bundled().expect("bundled faces");
    let tex = r"\frac{a + b}{c}";
    let node = parse(tex).unwrap();
    let mut mathml = mathml_atoms(&to_mathml(&node, true));
    let laid = layout_atoms(&engine.typeset(tex, Style::Display).unwrap());
    assert_eq!(tally(&mathml), tally(&laid));
    let plus = mathml.iter().position(|&c| c == '+').expect("the + atom");
    mathml.remove(plus);
    assert_ne!(tally(&mathml), tally(&laid));
}
