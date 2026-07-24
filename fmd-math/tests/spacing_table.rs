//! The inter-atom spacing table, locked entry-for-entry against the
//! published table (The TeXbook, p. 170) — every atom-class pair, every
//! style.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::Style;
use fmd_math::atom::{
    ATOM_CLASSES, AtomClass, PairSpacing, Spacing, pair_spacing, spacing_in_style,
};

/// The published table, transcribed independently of the implementation:
/// `0`/`1`/`2`/`3` = none/thin/med/thick, lowercase = every style,
/// uppercase-parenthesized encoded as negative = display/text only,
/// `*` = impossible. Rows and columns in Ord Op Bin Rel Open Close Punct
/// Inner order.
const EXPECTED: [[i8; 8]; 8] = [
    // Ord   Op  Bin  Rel Open Close Punct Inner
    [0, 1, -2, -3, 0, 0, 0, -1],     // Ord
    [1, 1, 9, -3, 0, 0, 0, -1],      // Op    (9 = impossible)
    [-2, -2, 9, 9, -2, 9, 9, -2],    // Bin
    [-3, -3, 9, 0, -3, 0, 0, -3],    // Rel
    [0, 0, 9, 0, 0, 0, 0, 0],        // Open
    [0, 1, -2, -3, 0, 0, 0, -1],     // Close
    [-1, -1, 9, -1, -1, -1, -1, -1], // Punct
    [-1, 1, -2, -3, -1, 0, -1, -1],  // Inner
];

fn decode(code: i8) -> PairSpacing {
    match code {
        0 => PairSpacing::Always(Spacing::None),
        1 => PairSpacing::Always(Spacing::Thin),
        -1 => PairSpacing::DisplayTextOnly(Spacing::Thin),
        -2 => PairSpacing::DisplayTextOnly(Spacing::Med),
        -3 => PairSpacing::DisplayTextOnly(Spacing::Thick),
        9 => PairSpacing::Impossible,
        other => panic!("bad code {other}"),
    }
}

#[test]
fn every_pair_matches_the_published_table() {
    for (i, left) in ATOM_CLASSES.into_iter().enumerate() {
        for (j, right) in ATOM_CLASSES.into_iter().enumerate() {
            assert_eq!(
                pair_spacing(left, right),
                decode(EXPECTED[i][j]),
                "pair ({left:?}, {right:?})"
            );
        }
    }
}

#[test]
fn every_pair_in_every_style() {
    for (i, left) in ATOM_CLASSES.into_iter().enumerate() {
        for (j, right) in ATOM_CLASSES.into_iter().enumerate() {
            for style in [
                Style::Display,
                Style::Text,
                Style::Script,
                Style::ScriptScript,
            ] {
                let got = spacing_in_style(left, right, style);
                let want = match decode(EXPECTED[i][j]) {
                    PairSpacing::Always(s) => s,
                    PairSpacing::DisplayTextOnly(s) => {
                        if style.is_script() {
                            Spacing::None
                        } else {
                            s
                        }
                    }
                    PairSpacing::Impossible => Spacing::None,
                };
                assert_eq!(got, want, "({left:?}, {right:?}) in {style:?}");
            }
        }
    }
}

#[test]
fn impossible_pairs_are_exactly_the_published_stars() {
    let stars: Vec<(AtomClass, AtomClass)> = ATOM_CLASSES
        .into_iter()
        .enumerate()
        .flat_map(|(i, l)| {
            ATOM_CLASSES
                .into_iter()
                .enumerate()
                .filter_map(move |(j, r)| (EXPECTED[i][j] == 9).then_some((l, r)))
        })
        .collect();
    // TeX's eight impossible pairs: Op–Bin, Bin–{Bin, Rel, Close, Punct},
    // and {Rel, Open, Punct}–Bin — a Bin can never be adjacent to any of
    // them after degradation.
    assert_eq!(stars.len(), 8);
    for (l, r) in &stars {
        assert!(
            *l == AtomClass::Bin || *r == AtomClass::Bin,
            "every impossible pair involves Bin: ({l:?}, {r:?})"
        );
        assert_eq!(pair_spacing(*l, *r), PairSpacing::Impossible);
    }
}

#[test]
fn mu_conversion() {
    // 1 mu = 1/18 em at the current size; the spacing amounts are 3/4/5 mu.
    assert_eq!(Spacing::Thin.mu(), 3);
    assert_eq!(Spacing::Med.mu(), 4);
    assert_eq!(Spacing::Thick.mu(), 5);
}
