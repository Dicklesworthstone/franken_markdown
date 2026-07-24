//! The atom engine: TeX's eight atom classes, contextual Bin→Ord
//! degradation, and the inter-atom spacing table.
//!
//! Everything here is TeX's *published* mathematics (The TeXbook, chapter 17
//! and appendix G): the spacing table is transcribed entry-for-entry from
//! page 170, and the two degradation rules are appendix G's rules 5 and 6.
//! These tables are the normative contract the spacing fixtures lock.

use crate::node::{Node, NodeKind, StackKind};
use crate::style::Style;

/// TeX's eight atom classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtomClass {
    /// Ordinary: letters, digits, most symbols.
    Ord,
    /// Large operator: `\sum`, `\int`, `\lim`, …
    Op,
    /// Binary operation: `+`, `−`, `\times`, …
    Bin,
    /// Relation: `=`, `<`, `\le`, arrows, …
    Rel,
    /// Opening delimiter: `(`, `[`, `\langle`, …
    Open,
    /// Closing delimiter: `)`, `]`, `\rangle`, …
    Close,
    /// Punctuation: `,`, `;`, …
    Punct,
    /// Inner: fractions, `\left…\right` groups, `\ldots`-class dots.
    Inner,
}

/// The eight classes in table order (the order of the spacing table's rows
/// and columns).
pub const ATOM_CLASSES: [AtomClass; 8] = [
    AtomClass::Ord,
    AtomClass::Op,
    AtomClass::Bin,
    AtomClass::Rel,
    AtomClass::Open,
    AtomClass::Close,
    AtomClass::Punct,
    AtomClass::Inner,
];

impl AtomClass {
    /// Row/column index in the spacing table.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Ord => 0,
            Self::Op => 1,
            Self::Bin => 2,
            Self::Rel => 3,
            Self::Open => 4,
            Self::Close => 5,
            Self::Punct => 6,
            Self::Inner => 7,
        }
    }
}

/// The amount of glue between two adjacent atoms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spacing {
    /// No space.
    None,
    /// Thin space: 3 mu.
    Thin,
    /// Medium space: 4 mu.
    Med,
    /// Thick space: 5 mu.
    Thick,
}

impl Spacing {
    /// The glue amount in mu (1 mu = 1/18 em at the current size).
    #[must_use]
    pub const fn mu(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Thin => 3,
            Self::Med => 4,
            Self::Thick => 5,
        }
    }
}

/// One entry of the spacing table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairSpacing {
    /// Space inserted in every style.
    Always(Spacing),
    /// Space inserted only in display and text styles (the TeXbook's
    /// parenthesized entries): suppressed in script and scriptscript.
    DisplayTextOnly(Spacing),
    /// The pair cannot occur after Bin→Ord degradation (the TeXbook's `*`
    /// entries).
    Impossible,
}

use PairSpacing::{Always as A, DisplayTextOnly as P, Impossible as X};
use Spacing::{Med, None as N0, Thick, Thin};

/// The inter-atom spacing table, exactly as published (The TeXbook,
/// p. 170). Rows are the left atom's class, columns the right atom's class,
/// both in [`ATOM_CLASSES`] order.
pub const SPACING_TABLE: [[PairSpacing; 8]; 8] = [
    // right:  Ord       Op        Bin      Rel       Open      Close     Punct     Inner
    /* Ord   */
    [
        A(N0),
        A(Thin),
        P(Med),
        P(Thick),
        A(N0),
        A(N0),
        A(N0),
        P(Thin),
    ],
    /* Op    */
    [A(Thin), A(Thin), X, P(Thick), A(N0), A(N0), A(N0), P(Thin)],
    /* Bin   */
    [P(Med), P(Med), X, X, P(Med), X, X, P(Med)],
    /* Rel   */
    [
        P(Thick),
        P(Thick),
        X,
        A(N0),
        P(Thick),
        A(N0),
        A(N0),
        P(Thick),
    ],
    /* Open  */
    [A(N0), A(N0), X, A(N0), A(N0), A(N0), A(N0), A(N0)],
    /* Close */
    [
        A(N0),
        A(Thin),
        P(Med),
        P(Thick),
        A(N0),
        A(N0),
        A(N0),
        P(Thin),
    ],
    /* Punct */
    [
        P(Thin),
        P(Thin),
        X,
        P(Thin),
        P(Thin),
        P(Thin),
        P(Thin),
        P(Thin),
    ],
    /* Inner */
    [
        P(Thin),
        A(Thin),
        P(Med),
        P(Thick),
        P(Thin),
        A(N0),
        P(Thin),
        P(Thin),
    ],
];

/// The raw table entry for a pair.
#[must_use]
pub const fn pair_spacing(left: AtomClass, right: AtomClass) -> PairSpacing {
    SPACING_TABLE[left.index()][right.index()]
}

/// The glue between two adjacent atoms in a given style, with the script
/// suppression rule applied. Impossible pairs yield no space (the engine
/// never produces them after degradation; tolerating them here keeps the
/// function total for untrusted callers).
#[must_use]
pub const fn spacing_in_style(left: AtomClass, right: AtomClass, style: Style) -> Spacing {
    match pair_spacing(left, right) {
        PairSpacing::Always(s) => s,
        PairSpacing::DisplayTextOnly(s) => {
            if style.is_script() {
                Spacing::None
            } else {
                s
            }
        }
        PairSpacing::Impossible => Spacing::None,
    }
}

/// The intrinsic atom class of a direct character in math mode, before any
/// command mapping. Follows plain TeX's mathcode assignments.
#[must_use]
pub const fn char_class(ch: char) -> AtomClass {
    match ch {
        '+' | '−' | '-' | '∗' | '*' | '±' | '∓' | '×' | '⋅' | '÷' | '∘' | '∙' | '⊕' | '⊖' | '⊗'
        | '⊘' | '⊙' | '∪' | '∩' | '∨' | '∧' | '∖' | '⋄' | '†' | '‡' | '⊎' | '⊔' | '⊓' | '≀'
        | '⨿' | '⋆' | '◁' | '▷' => AtomClass::Bin,
        '=' | '<' | '>' | ':' | '≤' | '≥' | '≠' | '≡' | '≈' | '∼' | '≃' | '≅' | '≐' | '∝' | '∈'
        | '∋' | '∉' | '⊂' | '⊃' | '⊆' | '⊇' | '≪' | '≫' | '⊨' | '⊢' | '⊣' | '≍' | '∣' | '∥'
        | '→' | '←' | '↔' | '⇒' | '⇐' | '⇔' | '⟶' | '⟵' | '⟷' | '⟹' | '⟸' | '⟺' | '↦' | '⟼'
        | '↑' | '↓' | '↕' | '⇑' | '⇓' | '⇕' | '↗' | '↘' | '↙' | '↖' | '↪' | '↩' | '⇀' | '⇁'
        | '↼' | '↽' | '⇌' => AtomClass::Rel,
        '(' | '[' | '⟨' | '⌈' | '⌊' => AtomClass::Open,
        ')' | ']' | '⟩' | '⌉' | '⌋' | '!' | '?' => AtomClass::Close,
        ',' | ';' => AtomClass::Punct,
        _ => AtomClass::Ord,
    }
}

/// The intrinsic atom class a node contributes to its enclosing list, or
/// `None` for non-atom items (spacing, ties, breaks, alignment tabs, and
/// the style/color markers), which are transparent to both degradation and
/// inter-atom spacing.
#[must_use]
pub fn intrinsic_class(node: &Node) -> Option<AtomClass> {
    match &node.kind {
        NodeKind::Symbol { class, .. } => Some(*class),
        NodeKind::BigOp { .. } | NodeKind::OpName { .. } => Some(AtomClass::Op),
        NodeKind::Scripts { base, .. } => base
            .as_deref()
            .map_or(Some(AtomClass::Ord), intrinsic_class),
        NodeKind::Frac { .. } | NodeKind::LeftRight { .. } => Some(AtomClass::Inner),
        NodeKind::SizedDelim { class, .. } => Some(*class),
        NodeKind::Radical { .. }
        | NodeKind::Accent { .. }
        | NodeKind::List(_)
        | NodeKind::Text { .. }
        | NodeKind::TextRun { .. }
        | NodeKind::TextStyled { .. }
        | NodeKind::MathIsland { .. }
        | NodeKind::Environment { .. } => Some(AtomClass::Ord),
        NodeKind::MathFont { body, .. } | NodeKind::Phantom { body, .. } => intrinsic_class(body),
        NodeKind::Stack { kind, base, .. } => match kind {
            StackKind::Stackrel => Some(AtomClass::Rel),
            StackKind::Overset | StackKind::Underset => intrinsic_class(base),
        },
        NodeKind::Fragment(kind) => match kind {
            crate::node::FragmentKind::UnmatchedClose
            | crate::node::FragmentKind::RedundantMathShift => None,
            crate::node::FragmentKind::StrayRight(_) => Some(AtomClass::Close),
        },
        NodeKind::StyleChange(_)
        | NodeKind::ColorChange(_)
        | NodeKind::Space(_)
        | NodeKind::Tie
        | NodeKind::Linebreak
        | NodeKind::AlignTab => None,
    }
}

/// Classify a horizontal list: every item's *effective* atom class, with
/// TeX's two Bin→Ord degradation rules applied (appendix G, rules 5 and 6):
///
/// 1. a Bin atom that opens the list or follows a Bin, Op, Rel, Open, or
///    Punct atom becomes Ord;
/// 2. a Bin atom directly before a Rel, Close, or Punct atom becomes Ord.
///
/// Non-atom items yield `None` and are transparent: they neither receive a
/// class nor interrupt atom adjacency.
#[must_use]
pub fn classify_list(items: &[Node]) -> Vec<Option<AtomClass>> {
    let mut classes: Vec<Option<AtomClass>> = items.iter().map(intrinsic_class).collect();
    let mut prev_atom: Option<usize> = None;
    for i in 0..classes.len() {
        let Some(current) = classes[i] else { continue };
        if current == AtomClass::Bin {
            let degrade = match prev_atom {
                None => true,
                Some(p) => matches!(
                    classes[p],
                    Some(
                        AtomClass::Bin
                            | AtomClass::Op
                            | AtomClass::Rel
                            | AtomClass::Open
                            | AtomClass::Punct
                    )
                ),
            };
            if degrade {
                classes[i] = Some(AtomClass::Ord);
            }
        } else if matches!(
            current,
            AtomClass::Rel | AtomClass::Close | AtomClass::Punct
        ) {
            if let Some(p) = prev_atom {
                if classes[p] == Some(AtomClass::Bin) {
                    classes[p] = Some(AtomClass::Ord);
                }
            }
        }
        prev_atom = Some(i);
    }
    classes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_table_diagonal_spot_checks() {
        assert_eq!(pair_spacing(AtomClass::Ord, AtomClass::Op), A(Thin));
        assert_eq!(pair_spacing(AtomClass::Bin, AtomClass::Bin), X);
        assert_eq!(pair_spacing(AtomClass::Rel, AtomClass::Rel), A(N0));
        assert_eq!(pair_spacing(AtomClass::Ord, AtomClass::Rel), P(Thick));
    }

    #[test]
    fn script_styles_suppress_parenthesized_entries() {
        assert_eq!(
            spacing_in_style(AtomClass::Ord, AtomClass::Bin, Style::Text),
            Med
        );
        assert_eq!(
            spacing_in_style(AtomClass::Ord, AtomClass::Bin, Style::Script),
            Spacing::None
        );
        assert_eq!(
            spacing_in_style(AtomClass::Ord, AtomClass::Op, Style::ScriptScript),
            Thin
        );
    }

    #[test]
    fn mu_values() {
        assert_eq!(Spacing::Thin.mu(), 3);
        assert_eq!(Spacing::Med.mu(), 4);
        assert_eq!(Spacing::Thick.mu(), 5);
        assert_eq!(Spacing::None.mu(), 0);
    }
}
