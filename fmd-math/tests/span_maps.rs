//! Span-map fixtures (§11.3): provenance is exact everywhere, and the
//! query surface implements the substring-map consumption pattern the
//! Reference needed a render-twice-and-align hack for.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::spanmap::find_occurrences;
use fmd_math::{Engine, NodeKind, Span, Style, parse_text};

fn engine() -> Engine {
    match Engine::bundled() {
        Ok(e) => e,
        Err(e) => panic!("bundled faces: {e}"),
    }
}

#[test]
fn every_primitive_span_is_nonempty_and_in_source() {
    let e = engine();
    let srcs = [
        r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt{\pi}}{2}",
        r"f''(x) + g'_n",
        r"\sum_{k=0}^{n} \binom{n}{k} x^k",
        r"{a+b \over c} \cdot \left( d - e \right)",
        r"\minus 1 + \mathds{R}",
        r"\overline{AB} + \underline{x}",
    ];
    for src in srcs {
        let layout = e.typeset(src, Style::Display).unwrap();
        for g in &layout.glyphs {
            assert!(
                g.span.end <= src.len() && g.span.start < g.span.end,
                "`{src}`: {g:?}"
            );
        }
        for r in &layout.rules {
            assert!(
                r.span.end <= src.len() && r.span.start < r.span.end,
                "`{src}`: {r:?}"
            );
        }
    }
    let src = r"the value $x^2$ grows \textbf{fast}";
    let layout = e.typeset_text(src).unwrap();
    for g in &layout.glyphs {
        assert!(
            g.span.end <= src.len() && g.span.start < g.span.end,
            "{g:?}"
        );
    }
}

#[test]
fn text_runs_carry_per_character_spans() {
    // Escapes decode two bytes to one char; the spans must track exactly.
    let src = r"a\%b";
    let root = parse_text(src).unwrap();
    let NodeKind::List(items) = &root.kind else {
        panic!("list")
    };
    let NodeKind::TextRun { text, char_spans } = &items[0].kind else {
        panic!("text run, got {:?}", items[0].kind);
    };
    assert_eq!(text, "a%b");
    assert_eq!(
        char_spans,
        &vec![Span::new(0, 1), Span::new(1, 3), Span::new(3, 4)]
    );
}

#[test]
fn primes_carry_their_own_token_spans() {
    let e = engine();
    let src = r"f''";
    let layout = e.typeset(src, Style::Display).unwrap();
    let mut prime_spans: Vec<Span> = layout
        .glyphs
        .iter()
        .filter(|g| g.ch == '′')
        .map(|g| g.span)
        .collect();
    prime_spans.sort_by_key(|s| s.start);
    assert_eq!(prime_spans, vec![Span::new(1, 2), Span::new(2, 3)]);
}

#[test]
fn default_pack_expansions_map_to_the_expansion_site() {
    let e = engine();
    let src = r"\minus 1";
    let layout = e.typeset(src, Style::Display).unwrap();
    let minus = layout
        .glyphs
        .iter()
        .find(|g| g.ch == '−')
        .expect("minus glyph");
    assert_eq!(minus.span, Span::new(0, 6), "the \\minus command's span");
}

#[test]
fn fraction_rules_belong_to_the_fraction() {
    let e = engine();
    let src = r"\frac{a}{b}";
    let layout = e.typeset(src, Style::Display).unwrap();
    let bar = &layout.rules[0];
    assert_eq!(bar.span, Span::new(0, src.len()));
}

#[test]
fn substring_selection_matches_by_source_identity() {
    let e = engine();
    let src = r"x^2 + y^2";
    let layout = e.typeset(src, Style::Display).unwrap();
    // t2c("y^2"): one occurrence, selecting exactly the y and its 2.
    let occ = find_occurrences(src, "y^2");
    assert_eq!(occ, vec![Span::new(6, 9)]);
    let sel = layout.select(occ[0]);
    assert_eq!(sel.glyphs.len(), 2);
    let chars: Vec<char> = sel.glyphs.iter().map(|&i| layout.glyphs[i].ch).collect();
    assert!(chars.contains(&'y') && chars.contains(&'2'));
    // t2c("x"): only the base x, not the y.
    let occ = find_occurrences(src, "x");
    let sel = layout.select(occ[0]);
    assert_eq!(sel.glyphs.len(), 1);
    assert_eq!(layout.glyphs[sel.glyphs[0]].ch, 'x');
}

#[test]
fn command_substrings_do_not_false_positive() {
    // The i inside \pi is not the letter i: containment semantics reject it.
    let e = engine();
    let src = r"\pi + i";
    let layout = e.typeset(src, Style::Display).unwrap();
    let occs = find_occurrences(src, "i");
    // Occurrences exist inside the command name and as the lone letter.
    assert_eq!(occs.len(), 2);
    let inside_pi = layout.select(occs[0]);
    assert!(inside_pi.is_empty(), "no primitive from inside \\pi");
    let lone = layout.select(occs[1]);
    assert_eq!(lone.glyphs.len(), 1);
    assert_eq!(layout.glyphs[lone.glyphs[0]].ch, 'i');
}

#[test]
fn transform_matching_by_source_identity_across_formulas() {
    // The TransformMatchingTex seam: parts correspond when the same source
    // substring selects nonempty primitive sets on both sides.
    let e = engine();
    let (a_src, b_src) = (r"a + b", r"b - a");
    let a = e.typeset(a_src, Style::Display).unwrap();
    let b = e.typeset(b_src, Style::Display).unwrap();
    for key in ["a", "b"] {
        let sa = a.select(find_occurrences(a_src, key)[0]);
        let sb = b.select(find_occurrences(b_src, key)[0]);
        assert_eq!(sa.glyphs.len(), 1, "`{key}` in `{a_src}`");
        assert_eq!(sb.glyphs.len(), 1, "`{key}` in `{b_src}`");
        assert_eq!(
            a.glyphs[sa.glyphs[0]].ch, b.glyphs[sb.glyphs[0]].ch,
            "matched parts render the same character"
        );
    }
    // A key present on one side only: empty on the other, so the consumer
    // fades it, exactly as TransformMatchingTex specifies.
    assert!(find_occurrences(b_src, "+").is_empty());
}

#[test]
fn touching_selection_serves_the_inspector() {
    let e = engine();
    let src = r"\frac{a}{b}";
    let layout = e.typeset(src, Style::Display).unwrap();
    // A byte inside the \frac command touches the whole construct: the bar
    // (whole-span) and nothing else exactly; the interior letters' spans
    // don't cover byte 2.
    let sel = layout.select_touching(Span::new(2, 3));
    assert_eq!(sel.rules.len(), 1);
    assert!(sel.glyphs.is_empty());
    // A byte at the numerator letter touches that glyph and the bar.
    // rfind: the first 'a' in the source is the one inside "\frac".
    let a_pos = src.rfind('a').unwrap();
    let sel = layout.select_touching(Span::new(a_pos, a_pos + 1));
    assert_eq!(sel.glyphs.len(), 1);
    assert_eq!(sel.rules.len(), 1);
}
