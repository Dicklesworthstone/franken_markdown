//! Parse fixtures over the tier-1 language surface: every construct family
//! parses, the tricky structures (infix `\over`, script clusters, radical
//! indices, `\left…\right`, environments, text islands both directions)
//! produce the trees they should, and the error doctrine holds (precise,
//! named, tier-tagged failures; never garbage).
//!
//! All strings here are project-authored (the 3b1b corpus is private and
//! exercised separately by the env-gated corpus goldens).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::node::{AccentKind, DelimSize, Limits, PhantomKind, StackKind, TextStyle};
use fmd_math::{MathError, Node, NodeKind, parse, parse_text};

fn root_items(node: &Node) -> &[Node] {
    match &node.kind {
        NodeKind::List(items) => items,
        other => panic!("root is not a list: {other:?}"),
    }
}

fn parse_items(src: &str) -> Vec<Node> {
    let root = parse(src).unwrap_or_else(|e| panic!("`{src}` failed: {e}"));
    root_items(&root).to_vec()
}

#[test]
fn empty_string_is_valid() {
    assert!(root_items(&parse("").unwrap()).is_empty());
    assert!(root_items(&parse_text("").unwrap()).is_empty());
}

#[test]
fn every_t1_command_family_parses() {
    // One representative string per construct family; parse success is the
    // assertion.
    let strings = [
        // groups, scripts, primes
        r"x^2",
        r"x_i",
        r"x_i^2",
        r"x^{n+1}_{k-1}",
        r"f'",
        r"f''(x)",
        r"f'_n",
        r"{a+b}^2",
        r"^2",
        r"_i",
        r"10^{th}",
        // fractions in every flavor
        r"\frac{a}{b}",
        r"\frac12",
        r"\dfrac{x}{y}",
        r"\tfrac{1}{2}",
        r"\binom{n}{k}",
        r"{a \over b}",
        r"a \over b",
        r"{n \choose k}",
        // radicals
        r"\sqrt{2}",
        r"\sqrt2",
        r"\sqrt[3]{x}",
        r"\sqrt[n+1]{x+y}",
        // delimiters
        r"\left( \frac{a}{b} \right)",
        r"\left[ x \right]",
        r"\left\{ y \right\}",
        r"\left\langle v \right\rangle",
        r"\left. x \right|",
        r"\left| x \right.",
        r"\left\lceil x \right\rceil",
        r"\left\lfloor x \right\rfloor",
        r"\big( \Big[ \bigg\{ \Bigg\langle",
        r"\bigl( x \bigr)",
        r"\bigm|",
        // accents
        r"\hat x",
        r"\vec{v}",
        r"\dot y",
        r"\ddot y",
        r"\tilde n",
        r"\bar z",
        r"\overline{AB}",
        r"\underline{x}",
        r"\overbrace{a+b}^{\text{sum}}",
        r"\underbrace{x+y}_{k}",
        r"\overrightarrow{AB}",
        r"\widehat{xyz}",
        // big operators with limits
        r"\sum_{n=1}^{\infty} \frac{1}{n^2}",
        r"\int_0^1 x \, dx",
        r"\prod_k a_k",
        r"\oint_C f",
        r"\iint_D g",
        r"\iiint_V h",
        r"\sum\limits_{n} a_n",
        r"\int\limits_0^1 f",
        r"\bigcup_i A_i",
        r"\bigoplus_k V_k",
        // operator names
        r"\sin x + \cos y",
        r"\tan\theta",
        r"\arctan u",
        r"\log_2 n",
        r"\ln e",
        r"\lim_{x \to 0} \frac{\sin x}{x}",
        r"\limsup_n a_n",
        r"\det A",
        r"\max_i x_i",
        r"\min_j y_j",
        r"\gcd(a,b)",
        r"a \mod b",
        r"\operatorname{argmin}_x f(x)",
        // symbol vocabulary
        r"\alpha\beta\gamma\delta\epsilon\varepsilon\zeta\eta\theta\iota\kappa\lambda",
        r"\mu\nu\xi\pi\rho\sigma\varsigma\tau\upsilon\phi\varphi\chi\psi\omega",
        r"\Gamma\Delta\Theta\Lambda\Xi\Pi\Sigma\Upsilon\Phi\Psi\Omega",
        r"\infty \partial \nabla \hbar \ell \wp \imath \jmath \emptyset \exists \forall \neg",
        r"\cdots \ldots \dots \vdots \ddots \hdots \checkmark \prime",
        r"a \pm b \mp c \cdot d \times e \circ f \oplus g \odot h \ast i",
        r"a \le b \ge c \ne d \equiv e \approx f \sim g \simeq h \cong i \propto j",
        r"a \in B \subset C \supset D \ll E \gg F \perp G \mid H \parallel I",
        r"a \to b \rightarrow c \leftarrow d \Rightarrow e \Leftrightarrow f \mapsto g",
        r"\longrightarrow \longleftarrow \longleftrightarrow \Longrightarrow \iff",
        r"\uparrow \downarrow \updownarrow \Uparrow \Downarrow \Updownarrow",
        r"\nearrow \searrow \swarrow \nwarrow \hookrightarrow \rightleftharpoons",
        r"\langle x \rangle \lceil y \rceil \lfloor z \rfloor",
        r"\minus 1",
        r"\mathds{1}",
        r"\vert \Vert \backslash",
        // text islands and alphabets
        r"\text{if } x > 0",
        r"\text{rate} = \frac{d}{dt}",
        r"\textbf{bold} + 1",
        r"\mathbb{R} \mathcal{L} \mathrm{d} \mathbf{v} \mathsf{T} \mathtt{code} \mathit{x}",
        r"\boldsymbol{\alpha}",
        // spacing, styles, phantoms, stacks, color
        r"a \, b \: c \; d \! e \quad f \qquad g \ h",
        r"\displaystyle \sum_n a_n",
        r"\textstyle \int_0^1",
        r"\scriptstyle x",
        r"\phantom{xx} + \hphantom{y} + \vphantom{z}",
        r"a \stackrel{?}{=} b",
        r"\overset{n}{X}",
        r"\underset{k}{Y}",
        r"\color{red} x + y",
        // escapes and specials
        r"100\%",
        r"\$5",
        r"\#1",
        r"a\&b",
        r"x\_y",
        r"\{a, b\}",
        r"5\,\mathrm{kg}",
        r"a ~ b",
        r"x \\ y",
        r"a & b",
        r"n' + m''",
        // environments
        r"\begin{array}{cc} a & b \\ c & d \end{array}",
        r"\begin{array}{c|c} 1 & 2 \end{array}",
        r"\begin{matrix} a & b \\ c & d \end{matrix}",
        r"\begin{pmatrix} 0 & 1 \\ 1 & 0 \end{pmatrix}",
        r"\begin{bmatrix} x \\ y \end{bmatrix}",
        r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}",
        r"\begin{cases} x & x > 0 \\ -x & x \le 0 \end{cases}",
        r"\begin{align*} a &= b \\ c &= d \end{align*}",
        r"\begin{align} x &= y \end{align}",
        r"\begin{aligned} p &= q \end{aligned}",
        r"\left[ \begin{array}{c} 1 \\ 2 \end{array} \right]",
        // unicode passthrough
        r"α + β",
        r"x → y",
    ];
    for s in strings {
        if let Err(e) = parse(s) {
            panic!("`{s}` failed to parse: {e}");
        }
    }
}

#[test]
fn text_mode_contract_parses() {
    let strings = [
        "Hello, world!",
        "The value of $x^2$ grows.",
        r"\textbf{Important:} read $\frac{a}{b}$ twice",
        r"\emph{emphasis} and \underline{underlined}",
        "escapes: \\$ \\% \\& \\# \\_ \\{ \\}",
        "ties~are~kept",
        r"line \\ break",
        "two $a+b$ islands $c-d$ here",
        "nested {group with $\\pi$} text",
        "apostrophes aren't primes in prose",
        "unicode: naïve café — d’accord",
        r"spacing \, \; \quad \qquad here",
        // Self-contained math commands in the mainland: accepted as
        // implicit one-command math islands (the Reference-era LaTeX
        // missing-$ recovery the corpus leans on).
        r"wait \dots there is more",
        r"a \Rightarrow b in prose",
        r"3 \times 4 grid",
        r"the constant \pi here",
        "",
        "  ",
    ];
    for s in strings {
        if let Err(e) = parse_text(s) {
            panic!("`{s}` failed to parse in text mode: {e}");
        }
    }
}

#[test]
fn over_splits_the_enclosing_group() {
    let items = parse_items(r"{a+b \over c}");
    assert_eq!(items.len(), 1);
    let NodeKind::List(inner) = &items[0].kind else {
        panic!("expected group list");
    };
    assert_eq!(inner.len(), 1);
    let NodeKind::Frac { num, den, spec } = &inner[0].kind else {
        panic!("expected fraction from \\over, got {:?}", inner[0].kind);
    };
    assert!(spec.bar);
    assert!(spec.delims.is_none());
    let NodeKind::List(num_items) = &num.kind else {
        panic!("numerator list");
    };
    let NodeKind::List(den_items) = &den.kind else {
        panic!("denominator list");
    };
    assert_eq!(num_items.len(), 3); // a + b
    assert_eq!(den_items.len(), 1); // c
}

#[test]
fn over_splits_at_top_level_too() {
    let items = parse_items(r"a \over b");
    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0].kind, NodeKind::Frac { .. }));
}

#[test]
fn choose_carries_parens_and_no_bar() {
    let items = parse_items(r"{n \choose k}");
    let NodeKind::List(inner) = &items[0].kind else {
        panic!("group");
    };
    let NodeKind::Frac { spec, .. } = &inner[0].kind else {
        panic!("fraction");
    };
    assert!(!spec.bar);
    assert_eq!(spec.delims, Some(('(', ')')));
}

#[test]
fn double_over_is_ambiguous() {
    let err = parse(r"{a \over b \over c}").unwrap_err();
    assert!(matches!(err, MathError::Malformed { .. }));
    assert!(err.to_string().contains("two \\over-class"));
}

#[test]
fn script_cluster_structure() {
    let items = parse_items(r"x_i^2");
    assert_eq!(items.len(), 1);
    let NodeKind::Scripts {
        base,
        sub,
        sup,
        primes,
    } = &items[0].kind
    else {
        panic!("scripts");
    };
    assert!(base.is_some());
    assert!(sub.is_some());
    assert!(sup.is_some());
    assert_eq!(*primes, 0);
}

#[test]
fn primes_count_and_combine_with_subscripts() {
    let items = parse_items(r"f''_n");
    let NodeKind::Scripts {
        primes, sub, sup, ..
    } = &items[0].kind
    else {
        panic!("scripts");
    };
    assert_eq!(*primes, 2);
    assert!(sub.is_some());
    assert!(sup.is_none());
}

#[test]
fn naked_script_gets_an_empty_base() {
    let items = parse_items(r"^2");
    let NodeKind::Scripts { base, .. } = &items[0].kind else {
        panic!("scripts");
    };
    assert!(base.is_none());
}

#[test]
fn double_superscript_is_malformed() {
    for s in [r"x^a^b", r"x'^a"] {
        let err = parse(s).unwrap_err();
        assert!(
            matches!(&err, MathError::Malformed { what, .. } if what.contains("superscript")),
            "`{s}`: {err}"
        );
    }
    let err = parse(r"x_a_b").unwrap_err();
    assert!(matches!(&err, MathError::Malformed { what, .. } if what.contains("subscript")));
}

#[test]
fn radical_with_index() {
    let items = parse_items(r"\sqrt[3]{x}");
    let NodeKind::Radical { index, .. } = &items[0].kind else {
        panic!("radical");
    };
    assert!(index.is_some());
}

#[test]
fn left_right_nests_and_records_delims() {
    let items = parse_items(r"\left( a \left[ b \right] c \right)");
    assert_eq!(items.len(), 1);
    let NodeKind::LeftRight { left, right, body } = &items[0].kind else {
        panic!("leftright");
    };
    assert_eq!(left.ch, Some('('));
    assert_eq!(right.ch, Some(')'));
    assert!(
        body.iter()
            .any(|n| matches!(&n.kind, NodeKind::LeftRight { .. }))
    );
}

#[test]
fn null_delimiter_is_none() {
    let items = parse_items(r"\left. x \right|");
    let NodeKind::LeftRight { left, right, .. } = &items[0].kind else {
        panic!("leftright");
    };
    assert_eq!(left.ch, None);
    assert_eq!(right.ch, Some('|'));
}

#[test]
fn sized_delims_record_size_and_class() {
    let items = parse_items(r"\bigl( x \Bigr)");
    let NodeKind::SizedDelim { size, class, delim } = &items[0].kind else {
        panic!("sized delim, got {:?}", items[0].kind);
    };
    assert_eq!(*size, DelimSize::Big);
    assert_eq!(*class, fmd_math::atom::AtomClass::Open);
    assert_eq!(delim.ch, Some('('));
    let NodeKind::SizedDelim { size, class, .. } = &items[2].kind else {
        panic!("sized delim");
    };
    assert_eq!(*size, DelimSize::BBig);
    assert_eq!(*class, fmd_math::atom::AtomClass::Close);
}

#[test]
fn limits_designators_set_the_mode() {
    let items = parse_items(r"\sum\limits_{n} x");
    let NodeKind::Scripts { base, .. } = &items[0].kind else {
        panic!("scripts, got {:?}", items[0].kind);
    };
    let NodeKind::BigOp { limits, .. } = &base.as_ref().unwrap().kind else {
        panic!("bigop base");
    };
    assert_eq!(*limits, Limits::Limits);

    let items = parse_items(r"\int\nolimits_0^1 f");
    let NodeKind::Scripts { base, .. } = &items[0].kind else {
        panic!("scripts");
    };
    let NodeKind::BigOp {
        limits, integral, ..
    } = &base.as_ref().unwrap().kind
    else {
        panic!("bigop base");
    };
    assert_eq!(*limits, Limits::NoLimits);
    assert!(*integral);
}

#[test]
fn limits_without_operator_is_malformed() {
    let err = parse(r"x\limits^2").unwrap_err();
    assert!(matches!(&err, MathError::Malformed { what, .. } if what.contains("big operator")));
}

#[test]
fn environment_rows_and_cells() {
    let items = parse_items(r"\begin{array}{cc} a & b \\ c & d \end{array}");
    assert_eq!(items.len(), 1);
    let NodeKind::Environment { name, spec, rows } = &items[0].kind else {
        panic!("environment");
    };
    assert_eq!(name, "array");
    assert_eq!(spec.as_deref(), Some("cc"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].len(), 2);
    assert_eq!(rows[1].len(), 2);
}

#[test]
fn trailing_row_break_is_ignored() {
    let items = parse_items(r"\begin{matrix} a \\ b \\ \end{matrix}");
    let NodeKind::Environment { rows, .. } = &items[0].kind else {
        panic!("environment");
    };
    assert_eq!(rows.len(), 2);
}

#[test]
fn env_name_mismatch_is_malformed() {
    let err = parse(r"\begin{matrix} a \end{pmatrix}").unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("closed by")),
        "{err}"
    );
}

#[test]
fn unknown_environment_is_named() {
    let err = parse(r"\begin{mystery} x \end{mystery}").unwrap_err();
    assert_eq!(err.unsupported_construct(), Some("env:mystery"));
}

#[test]
fn t2_environment_is_tier_tagged() {
    let err = parse_text(r"\begin{flushleft} x \end{flushleft}").unwrap_err();
    assert_eq!(err.unsupported_construct(), Some("env:flushleft"));
    assert!(err.to_string().contains("tier T2"));
}

#[test]
fn text_island_in_math_and_math_island_in_text() {
    let items = parse_items(r"\text{if $x^2$ holds}");
    let NodeKind::Text { body } = &items[0].kind else {
        panic!("text island");
    };
    assert!(
        body.iter()
            .any(|n| matches!(&n.kind, NodeKind::MathIsland { .. }))
    );

    let root = parse_text(r"value $\pi r^2$ done").unwrap();
    let items = root_items(&root);
    assert!(
        items
            .iter()
            .any(|n| matches!(&n.kind, NodeKind::MathIsland { .. }))
    );
    assert!(
        items
            .iter()
            .any(|n| matches!(&n.kind, NodeKind::TextRun(t) if t.contains("value")))
    );
}

#[test]
fn textbf_works_in_both_modes() {
    let items = parse_items(r"\textbf{M}");
    assert!(matches!(
        &items[0].kind,
        NodeKind::TextStyled {
            style: TextStyle::Bold,
            ..
        }
    ));
    let root = parse_text(r"\textbf{M}").unwrap();
    assert!(matches!(
        &root_items(&root)[0].kind,
        NodeKind::TextStyled {
            style: TextStyle::Bold,
            ..
        }
    ));
}

#[test]
fn underline_is_accent_in_math_and_styling_in_text() {
    let items = parse_items(r"\underline{x}");
    assert!(matches!(
        &items[0].kind,
        NodeKind::Accent {
            accent: AccentKind::UnderLine,
            ..
        }
    ));
    let root = parse_text(r"\underline{x}").unwrap();
    assert!(matches!(
        &root_items(&root)[0].kind,
        NodeKind::TextStyled {
            style: TextStyle::Underline,
            ..
        }
    ));
}

#[test]
fn stack_family_structure() {
    let items = parse_items(r"a \stackrel{?}{=} b");
    let stack = items
        .iter()
        .find(|n| matches!(&n.kind, NodeKind::Stack { .. }))
        .expect("stack node");
    let NodeKind::Stack { kind, .. } = &stack.kind else {
        unreachable!()
    };
    assert_eq!(*kind, StackKind::Stackrel);
}

#[test]
fn phantoms_record_their_kind() {
    let items = parse_items(r"\vphantom{X}");
    assert!(matches!(
        &items[0].kind,
        NodeKind::Phantom {
            kind: PhantomKind::Vertical,
            ..
        }
    ));
}

#[test]
fn hyphen_and_star_map_to_math_codepoints() {
    let items = parse_items(r"a-b*c");
    assert!(matches!(&items[1].kind, NodeKind::Symbol { ch: '−', .. }));
    assert!(matches!(&items[3].kind, NodeKind::Symbol { ch: '∗', .. }));
}

#[test]
fn every_node_carries_a_nonempty_span_over_source() {
    let src = r"\frac{a}{b} + \sqrt[3]{x^2} \text{ ok $y$}";
    let root = parse(src).unwrap();
    fn walk(node: &Node, src: &str) {
        assert!(node.span.end <= src.len(), "span past end: {node:?}");
        assert!(node.span.start <= node.span.end, "inverted span: {node:?}");
        match &node.kind {
            NodeKind::List(items)
            | NodeKind::LeftRight { body: items, .. }
            | NodeKind::Text { body: items }
            | NodeKind::TextStyled { body: items, .. }
            | NodeKind::MathIsland { body: items, .. } => {
                for n in items {
                    walk(n, src);
                }
            }
            NodeKind::Scripts { base, sub, sup, .. } => {
                for n in [base, sub, sup].into_iter().flatten() {
                    walk(n, src);
                }
            }
            NodeKind::Frac { num, den, .. } => {
                walk(num, src);
                walk(den, src);
            }
            NodeKind::Radical { index, radicand } => {
                if let Some(ix) = index {
                    walk(ix, src);
                }
                walk(radicand, src);
            }
            NodeKind::Accent { base, .. } => walk(base, src),
            NodeKind::MathFont { body, .. } | NodeKind::Phantom { body, .. } => walk(body, src),
            NodeKind::Stack {
                annotation, base, ..
            } => {
                walk(annotation, src);
                walk(base, src);
            }
            NodeKind::Environment { rows, .. } => {
                for row in rows {
                    for cell in row {
                        walk(cell, src);
                    }
                }
            }
            _ => {}
        }
    }
    walk(&root, src);
}

#[test]
fn structural_malformations_are_precise() {
    // Mid-string structural faults stay strict; only end-of-input closes
    // things (the fragment contract).
    for (s, needle) in [
        (r"\begin{matrix} x", "unclosed \\begin{matrix}"),
        (r"\end{matrix}", "without a matching \\begin"),
        (r"{a $ b}", "closed by the wrong construct"),
        (r"{x \right) y}", "\\right without"),
        (r"\frac{a}^2", "denominator"),
        (r"\sqrt[3{x}", "unclosed '['"),
        (r"\sqrt[3}", "not closed by ']'"),
    ] {
        let err = parse(s).unwrap_err();
        assert!(
            matches!(&err, MathError::Malformed { what, .. } if what.contains(needle)),
            "`{s}` produced: {err}"
        );
    }
}

#[test]
fn fragments_of_a_balanced_whole_parse_with_markers() {
    use fmd_math::node::FragmentKind;
    // The Tex surface's multi-argument idiom makes each literal argument
    // its own string, so pieces may be substrings of a balanced whole. The
    // grammar accepts them at end of input / top level, with explicit
    // markers — never silently.
    let items = parse_items(r"a}");
    assert!(matches!(
        &items[1].kind,
        NodeKind::Fragment(FragmentKind::UnmatchedClose)
    ));

    let items = parse_items(r"{a");
    assert!(matches!(&items[0].kind, NodeKind::List(inner) if inner.len() == 1));

    let items = parse_items(r"\right)");
    let NodeKind::Fragment(FragmentKind::StrayRight(delim)) = &items[0].kind else {
        panic!("stray right, got {:?}", items[0].kind);
    };
    assert_eq!(delim.ch, Some(')'));

    let items = parse_items(r"\left( x");
    let NodeKind::LeftRight { right, .. } = &items[0].kind else {
        panic!("leftright");
    };
    assert_eq!(right.ch, None, "fragment-closed \\left gets a null right");

    // Script pieces: the argument lives in the next piece.
    let items = parse_items(r"a^");
    let NodeKind::Scripts { sup: Some(sup), .. } = &items[0].kind else {
        panic!("scripts");
    };
    assert!(matches!(&sup.kind, NodeKind::List(l) if l.is_empty()));

    let items = parse_items(r"^{-2\pi i");
    let NodeKind::Scripts {
        base: None,
        sup: Some(sup),
        ..
    } = &items[0].kind
    else {
        panic!("naked script fragment");
    };
    assert!(matches!(&sup.kind, NodeKind::List(l) if l.len() == 4));

    // Command pieces with their arguments in the next piece.
    let items = parse_items(r"\dot");
    assert!(matches!(&items[0].kind, NodeKind::Accent { .. }));
    let items = parse_items(r"\sqrt{\,");
    assert!(matches!(&items[0].kind, NodeKind::Radical { .. }));
    let items = parse_items(r"\lim_{x \to");
    assert!(matches!(&items[0].kind, NodeKind::Scripts { .. }));

    // Redundant dollars around an already-math string.
    let items = parse_items(r"$0.1$");
    assert!(matches!(
        &items[0].kind,
        NodeKind::Fragment(FragmentKind::RedundantMathShift)
    ));
    assert_eq!(items.len(), 5);
}

#[test]
fn text_mode_malformations_are_precise() {
    for (s, needle) in [
        ("x ^ 2", "mathematics"),
        ("a & b", "outside an alignment"),
        ("open $x", "unclosed '$'"),
        (r"a \over b", "math-mode command"),
        (r"\displaystyle x", "math-mode command"),
        ("{unclosed", "unclosed '{'"),
    ] {
        let err = parse_text(s).unwrap_err();
        assert!(
            matches!(&err, MathError::Malformed { what, .. } if what.contains(needle)),
            "`{s}` produced: {err}"
        );
    }
}

#[test]
fn t2_commands_fail_named_and_tiered() {
    for (s, construct) in [
        (r"\substack{a \\ b}", r"\substack"),
        (r"a \nmid b", r"\nmid"),
        (r"\xrightarrow{f}", r"\xrightarrow"),
        (r"\dddot x", r"\dddot"),
        (r"\oiint_S", r"\oiint"),
    ] {
        let err = parse(s).unwrap_err();
        assert_eq!(err.unsupported_construct(), Some(construct), "`{s}`");
        assert!(err.to_string().contains("tier T2"), "`{s}`: {err}");
    }
    // Text-mode T2: sizes and the text accents.
    for (s, construct) in [
        (r"\Large text", r"\Large"),
        (r"na\'ive", r"\'"),
        (r"\male", r"\male"),
    ] {
        let err = parse_text(s).unwrap_err();
        assert_eq!(err.unsupported_construct(), Some(construct), "`{s}`");
        assert!(err.to_string().contains("tier T2"), "`{s}`: {err}");
    }
}

#[test]
fn unknown_commands_fail_named_and_untiered() {
    let err = parse(r"\definitelynotacommand x").unwrap_err();
    assert_eq!(err.unsupported_construct(), Some(r"\definitelynotacommand"));
    assert!(err.to_string().contains("untiered"));
}

#[test]
fn comments_are_stripped() {
    let items = parse_items("a % trailing comment with $ and ^\n+ b");
    assert_eq!(items.len(), 3);
}

#[test]
fn deep_nesting_errors_cleanly() {
    let deep = format!("{}x{}", "{".repeat(200), "}".repeat(200));
    let err = parse(&deep).unwrap_err();
    assert!(
        matches!(&err, MathError::Malformed { what, .. } if what.contains("depth limit")),
        "{err}"
    );
}
