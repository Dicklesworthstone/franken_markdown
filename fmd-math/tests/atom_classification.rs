//! Atom-classification fixtures against TeX's rules: intrinsic classes for
//! the symbol families, and the two contextual Bin→Ord degradation rules
//! (The TeXbook, appendix G, rules 5 and 6).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::atom::{AtomClass, classify_list, intrinsic_class};
use fmd_math::{Node, NodeKind, parse};

fn items(src: &str) -> Vec<Node> {
    match parse(src).unwrap().kind {
        NodeKind::List(items) => items,
        other => panic!("root is not a list: {other:?}"),
    }
}

/// The effective classes of a parsed list, atoms only.
fn classes(src: &str) -> Vec<AtomClass> {
    let items = items(src);
    classify_list(&items).into_iter().flatten().collect()
}

#[test]
fn intrinsic_classes_of_symbol_families() {
    use AtomClass::*;
    let expectations: &[(&str, AtomClass)] = &[
        (r"x", Ord),
        (r"7", Ord),
        (r"\pi", Ord),
        (r"\infty", Ord),
        (r"\sum", Op),
        (r"\int", Op),
        (r"\sin", Op),
        (r"\lim", Op),
        (r"+", Bin),
        (r"\cdot", Bin),
        (r"\times", Bin),
        (r"\pm", Bin),
        (r"\oplus", Bin),
        (r"=", Rel),
        (r"<", Rel),
        (r":", Rel),
        (r"\le", Rel),
        (r"\to", Rel),
        (r"\in", Rel),
        (r"\perp", Rel),
        (r"(", Open),
        (r"[", Open),
        (r"\langle", Open),
        (r"\lceil", Open),
        (r")", Close),
        (r"]", Close),
        (r"\rangle", Close),
        (r"!", Close),
        (r"?", Close),
        (r",", Punct),
        (r";", Punct),
        (r"\colon", Punct),
        (r"\frac{a}{b}", Inner),
        (r"\left( x \right)", Inner),
        (r"\cdots", Inner),
        (r"\ldots", Inner),
        (r"\sqrt{x}", Ord),
        (r"\hat x", Ord),
        (r"\vdots", Ord),
        (r"\text{ok}", Ord),
        (r"\mathbb{R}", Ord),
        (r"\begin{matrix} a \end{matrix}", Ord),
        (r"\stackrel{?}{=}", Rel),
    ];
    for (src, expected) in expectations {
        let items = items(src);
        assert_eq!(items.len(), 1, "`{src}` should parse to one item");
        assert_eq!(
            intrinsic_class(&items[0]),
            Some(*expected),
            "`{src}` intrinsic class"
        );
    }
}

#[test]
fn scripts_take_their_base_class() {
    use AtomClass::*;
    for (src, expected) in [
        (r"x^2", Ord),
        (r"\sum_n", Op),
        (r"+^2", Ord), // leading Bin degrades first… see below
    ] {
        let all = classes(src);
        assert_eq!(all.first().copied(), Some(expected), "`{src}`");
    }
}

#[test]
fn rule5_bin_degrades_at_list_start() {
    // A Bin atom opening the list becomes Ord: "−a" is a unary minus.
    assert_eq!(classes(r"-a")[0], AtomClass::Ord);
    assert_eq!(classes(r"+x")[0], AtomClass::Ord);
}

#[test]
fn rule5_bin_degrades_after_bin_op_rel_open_punct() {
    use AtomClass::*;
    // a + - b : the second Bin degrades (after Bin).
    assert_eq!(classes(r"a+-b"), vec![Ord, Bin, Ord, Ord]);
    // a = - b : after Rel.
    assert_eq!(classes(r"a=-b"), vec![Ord, Rel, Ord, Ord]);
    // ( - a : after Open.
    assert_eq!(classes(r"(-a"), vec![Open, Ord, Ord]);
    // a , - b : after Punct.
    assert_eq!(classes(r"a,-b"), vec![Ord, Punct, Ord, Ord]);
    // \sum - a : after Op.
    assert_eq!(classes(r"\sum -a"), vec![Op, Ord, Ord]);
}

#[test]
fn rule6_bin_degrades_before_rel_close_punct() {
    use AtomClass::*;
    // a + = b : the Bin before a Rel becomes Ord.
    assert_eq!(classes(r"a+=b"), vec![Ord, Ord, Rel, Ord]);
    // a + ) : before Close.
    assert_eq!(classes(r"a+)"), vec![Ord, Ord, Close]);
    // a + , b : before Punct.
    assert_eq!(classes(r"a+,b"), vec![Ord, Ord, Punct, Ord]);
}

#[test]
fn binary_between_ordinaries_stays_binary() {
    use AtomClass::*;
    assert_eq!(classes(r"a+b"), vec![Ord, Bin, Ord]);
    assert_eq!(classes(r"a-b"), vec![Ord, Bin, Ord]);
    assert_eq!(classes(r"a \cdot b"), vec![Ord, Bin, Ord]);
    // Close and Inner on the left keep a following Bin binary.
    assert_eq!(classes(r"(a)+b"), vec![Open, Ord, Close, Bin, Ord]);
    assert_eq!(classes(r"\frac{1}{2}+b"), vec![Inner, Bin, Ord]);
}

#[test]
fn spacing_and_markers_are_transparent_to_degradation() {
    use AtomClass::*;
    // The glue between "=" and "-" must not shield the Bin from rule 5.
    assert_eq!(classes(r"a = \, -b"), vec![Ord, Rel, Ord, Ord]);
    // A style marker is equally transparent.
    assert_eq!(classes(r"a = \displaystyle -b"), vec![Ord, Rel, Ord, Ord]);
}

#[test]
fn non_atoms_yield_none() {
    let items = items(r"a \, b");
    let classified = classify_list(&items);
    assert_eq!(classified.len(), 3);
    assert!(classified[0].is_some());
    assert!(classified[1].is_none(), "spacing is not an atom");
    assert!(classified[2].is_some());
}
