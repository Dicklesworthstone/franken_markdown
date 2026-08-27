//! MathML node-walk backend: TeX → MathML goldens, well-formedness, chaos.
//!
//! Every assertion logs `check=<id> subject=<tex> outcome=PASS|FAIL` on stderr.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{mathml_well_formed, parse, parse_text, to_mathml, to_mathml_element};

fn log_check(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log_check(id, subject, "PASS");
    } else {
        log_check(id, subject, "FAIL");
        panic!("{id} failed for `{subject}`: {detail}");
    }
}

fn render_display(tex: &str) -> String {
    let node = parse(tex).unwrap_or_else(|e| panic!("parse `{tex}`: {e}"));
    to_mathml(&node, true)
}

fn assert_golden(id: &str, tex: &str, expected: &str) {
    let got = render_display(tex);
    if got != expected {
        log_check(id, tex, "FAIL");
        panic!("{id} golden mismatch for `{tex}`\n expected: {expected}\n      got: {got}");
    }
    if let Err(e) = mathml_well_formed(&got) {
        log_check(id, tex, "FAIL");
        panic!("{id} well-formedness: {e}\n xml: {got}");
    }
    log_check(id, tex, "PASS");
}

fn assert_contains(id: &str, tex: &str, needles: &[&str]) {
    let got = render_display(tex);
    if let Err(e) = mathml_well_formed(&got) {
        log_check(id, tex, "FAIL");
        panic!("{id} well-formedness: {e}\n xml: {got}");
    }
    for n in needles {
        if !got.contains(n) {
            log_check(id, tex, "FAIL");
            panic!("{id} missing `{n}` in `{tex}`\n xml: {got}");
        }
    }
    log_check(id, tex, "PASS");
}

const NS: &str = r#"<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">"#;

#[test]
fn identifier_and_number_and_operator() {
    assert_golden("sym-mi", "x", &format!("{NS}<mi>x</mi></math>"));
    assert_golden("sym-mn", "7", &format!("{NS}<mn>7</mn></math>"));
    assert_golden("sym-mo", "+", &format!("{NS}<mo>+</mo></math>"));
}

#[test]
fn fraction_sqrt_scripts() {
    assert_golden(
        "frac",
        r"\frac{a}{b}",
        &format!("{NS}<mfrac><mrow><mi>a</mi></mrow><mrow><mi>b</mi></mrow></mfrac></math>"),
    );
    assert_golden(
        "sqrt",
        r"\sqrt{x}",
        &format!("{NS}<msqrt><mrow><mi>x</mi></mrow></msqrt></math>"),
    );
    assert_contains(
        "mroot",
        r"\sqrt[3]{x}",
        &["<mroot>", "</mroot>", "<mn>3</mn>"],
    );
    assert_contains(
        "msubsup",
        r"x_i^2",
        &["<msubsup>", "<mi>x</mi>", "<mi>i</mi>", "<mn>2</mn>"],
    );
    assert_contains("msup", r"x^2", &["<msup>", "<mi>x</mi>", "<mn>2</mn>"]);
    assert_contains("msub", r"x_i", &["<msub>", "<mi>x</mi>", "<mi>i</mi>"]);
}

#[test]
fn binom_and_left_right_and_accents() {
    assert_contains(
        "binom",
        r"\binom{n}{k}",
        &[
            r#"<mo fence="true" stretchy="true">(</mo>"#,
            r#"<mfrac linethickness="0">"#,
            r#"<mo fence="true" stretchy="true">)</mo>"#,
        ],
    );
    assert_contains(
        "leftright",
        r"\left( \frac{a}{b} \right)",
        &[
            r#"<mo fence="true" stretchy="true">(</mo>"#,
            "<mfrac>",
            r#"<mo fence="true" stretchy="true">)</mo>"#,
        ],
    );
    assert_contains("hat", r"\hat x", &["<mover>", "\u{02C6}", "</mover>"]);
    assert_contains("underline", r"\underline{x}", &["<munder>", "</munder>"]);
}

#[test]
fn environments_become_mtables() {
    assert_contains(
        "matrix",
        r"\begin{matrix} a & b \\ c & d \end{matrix}",
        &[
            "<mtable>",
            "<mtr>",
            "<mtd>",
            "<mi>a</mi>",
            "<mi>d</mi>",
            "</mtable>",
        ],
    );
    assert_contains(
        "pmatrix",
        r"\begin{pmatrix} 0 & 1 \\ 1 & 0 \end{pmatrix}",
        &[
            r#"<mo fence="true" stretchy="true">(</mo>"#,
            "<mtable>",
            r#"<mo fence="true" stretchy="true">)</mo>"#,
        ],
    );
    assert_contains(
        "cases",
        r"\begin{cases} x & x > 0 \\ -x & x \le 0 \end{cases}",
        &[
            r#"<mo fence="true" stretchy="true">{</mo>"#,
            "<mtable",
            "<mtd>",
        ],
    );
    assert_contains(
        "aligned",
        r"\begin{aligned} p &= q \end{aligned}",
        &[r#"columnalign="right left""#, "<mtable"],
    );
}

#[test]
fn opname_limits_phantom_space_font() {
    assert_contains(
        "sin",
        r"\sin x",
        &[r#"<mi mathvariant="normal">sin</mi>"#, "<mi>x</mi>"],
    );
    assert_contains("sum-limits", r"\sum_{n=1}^N", &["<munderover>", "<mo"]);
    assert_contains("phantom", r"\phantom{x}", &["<mphantom>", "<mi>x</mi>"]);
    assert_contains(
        "thinspace",
        r"a\,b",
        &[r#"<mspace width="0.167em">"#, "</mspace>"],
    );
    assert_contains(
        "mathbb",
        r"\mathbb{R}",
        &[r#"<mstyle mathvariant="double-struck">"#],
    );
}

#[test]
fn display_vs_inline_attr() {
    let node = parse("x").unwrap();
    let block = to_mathml(&node, true);
    let inline = to_mathml(&node, false);
    assert_ok(
        "display-block",
        "x",
        block.contains(r#"display="block""#),
        &block,
    );
    assert_ok(
        "display-inline",
        "x",
        inline.contains(r#"display="inline""#),
        &inline,
    );
}

#[test]
fn element_path_skips_math_wrapper() {
    let node = parse("x").unwrap();
    let inner = to_mathml_element(&node);
    assert_ok(
        "element-no-math",
        "x",
        !inner.contains("<math") && inner.contains("<mrow>") && inner.contains("<mi>x</mi>"),
        &inner,
    );
}

#[test]
fn well_formedness_rejects_self_close_and_mismatch() {
    let cases = [
        ("self-close", "<math><mspace width=\"1em\"/></math>"),
        ("mismatch", "<math><mi>x</mo></math>"),
        ("bare-amp", "<math><mi>a&b</mi></math>"),
        ("unclosed", "<math><mi>x</mi>"),
    ];
    for (id, xml) in cases {
        let err = mathml_well_formed(xml);
        assert_ok(id, xml, err.is_err(), "expected rejection");
    }
    assert_ok(
        "good-empty-mrow",
        "<mrow></mrow>",
        mathml_well_formed("<mrow></mrow>").is_ok(),
        "empty mrow should pass",
    );
}

#[test]
fn goldens_are_deterministic() {
    let tex = r"\frac{1+\sqrt{x}}{x_i^2}";
    let a = render_display(tex);
    let b = render_display(tex);
    assert_ok("determinism", tex, a == b, "two renders differed");
}

#[test]
fn text_mode_math_island() {
    let node = parse_text("hello $x^2$").unwrap();
    let xml = to_mathml(&node, false);
    if let Err(e) = mathml_well_formed(&xml) {
        log_check("text-island", "hello $x^2$", "FAIL");
        panic!("{e}\n{xml}");
    }
    assert_ok(
        "text-island",
        "hello $x^2$",
        xml.contains("<mtext>") && xml.contains("<msup>"),
        &xml,
    );
}

#[test]
fn xml_escapes_lt_gt_amp() {
    assert_contains("lt", "<", &["&lt;"]);
    assert_contains("gt", ">", &["&gt;"]);
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn pick<'a, T>(&mut self, pool: &'a [T]) -> &'a T {
        let idx = (self.next() >> 33) as usize % pool.len();
        &pool[idx]
    }
}

const POOL: &[&str] = &[
    "x",
    "y",
    "0",
    "1",
    "+",
    "-",
    "=",
    "(",
    ")",
    "{",
    "}",
    "^",
    "_",
    r"\frac",
    r"\sqrt",
    r"\left",
    r"\right",
    r"\sum",
    r"\hat",
    r"\begin{matrix}",
    r"\end{matrix}",
    "a",
    "b",
    r"\,",
    r"\sin",
    r"\mathbb",
    r"\phantom",
    "&",
    r"\\",
];

#[test]
fn chaos_parse_then_mathml_never_panics_and_stays_well_formed() {
    let mut rng = Lcg(0x4A7B_0000_F00D);
    let mut parsed = 0u32;
    let mut well = 0u32;
    for i in 0..2_000u32 {
        let len = (rng.next() >> 40) as usize % 24;
        let mut s = String::new();
        for _ in 0..len {
            s.push_str(rng.pick(POOL));
        }
        if let Ok(node) = parse(&s) {
            parsed += 1;
            let xml = to_mathml(&node, true);
            match mathml_well_formed(&xml) {
                Ok(()) => well += 1,
                Err(e) => {
                    log_check("chaos-wf", &s, "FAIL");
                    panic!("iter {i} well-formedness failed: {e}\n tex: {s}\n xml: {xml}");
                }
            }
        }
        if let Ok(node) = parse_text(&s) {
            let xml = to_mathml(&node, false);
            if let Err(e) = mathml_well_formed(&xml) {
                log_check("chaos-text-wf", &s, "FAIL");
                panic!("iter {i} text well-formedness failed: {e}\n tex: {s}\n xml: {xml}");
            }
        }
    }
    log_check(
        "chaos-summary",
        &format!("parsed={parsed} well_formed={well}"),
        "PASS",
    );
    assert_ok(
        "chaos-did-parse",
        "corpus",
        parsed > 0,
        "chaos loop parsed zero inputs",
    );
}
