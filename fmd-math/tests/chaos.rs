//! The chaos suite: arbitrary token streams must error *cleanly* — never
//! panic, never hang, never garble (franken_manim §16.5's fuzz doctrine;
//! the in-crate deterministic half of it — a coverage-guided cargo-fuzz
//! harness rides the consuming repo's fuzz program).
//!
//! Determinism: a fixed-seed LCG, so every run exercises the identical
//! inputs and a failure is a one-command repro.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fmd_math::{parse, parse_text};

/// A fixed-parameter LCG (Numerical Recipes constants); good enough to mix
/// a pool, and fully deterministic.
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

/// Fragments deliberately biased toward the grammar's structure: brace
/// churn, script storms, half-open constructs, tier-2 and unknown commands,
/// comments, unicode, and raw specials.
const POOL: &[&str] = &[
    "{",
    "}",
    "^",
    "_",
    "&",
    "~",
    "$",
    "'",
    "\\",
    " ",
    "\n",
    "%",
    "[",
    "]",
    "(",
    ")",
    "a",
    "x",
    "0",
    "9",
    "+",
    "-",
    "=",
    "!",
    "?",
    ",",
    ";",
    ".",
    "|",
    "<",
    ">",
    "α",
    "→",
    "ö",
    "…",
    r"\frac",
    r"\sqrt",
    r"\left",
    r"\right",
    r"\over",
    r"\choose",
    r"\begin",
    r"\end",
    r"\begin{matrix}",
    r"\end{matrix}",
    r"\begin{array}",
    r"\text",
    r"\textbf",
    r"\hat",
    r"\sum",
    r"\int",
    r"\limits",
    r"\nolimits",
    r"\displaystyle",
    r"\mathbb",
    r"\color",
    r"\big",
    r"\bigl",
    r"\Bigg",
    r"\substack",
    r"\notarealcommand",
    r"\,",
    r"\;",
    r"\!",
    r"\\",
    r"\{",
    r"\}",
    r"\%",
    r"\$",
    r"\ ",
    r"\'",
    r"\stackrel",
    r"\overbrace",
    r"\phantom",
    r"\operatorname",
    r"\sqrt[",
    r"\underbrace",
    r"\lim",
    r"\mathds",
];

#[test]
fn random_token_soup_never_panics() {
    let mut rng = Lcg(0x5EED_F00D_CAFE_D00D);
    for _ in 0..20_000 {
        let len = (rng.next() >> 40) as usize % 40;
        let mut s = String::new();
        for _ in 0..len {
            s.push_str(rng.pick(POOL));
        }
        // The only contract: a Result comes back. Panics or hangs fail the
        // suite.
        let _ = parse(&s);
        let _ = parse_text(&s);
    }
}

#[test]
fn random_bytes_as_chars_never_panic() {
    let mut rng = Lcg(0xBAD_5EED);
    for _ in 0..5_000 {
        let len = (rng.next() >> 40) as usize % 60;
        let s: String = (0..len)
            .map(|_| {
                let v = (rng.next() >> 32) as u32 % 0x2FFF;
                char::from_u32(v).unwrap_or('?')
            })
            .collect();
        let _ = parse(&s);
        let _ = parse_text(&s);
    }
}

#[test]
fn adversarial_shapes_error_cleanly() {
    let deep_braces = "{".repeat(10_000);
    let deep_left = r"\left(".repeat(5_000);
    let deep_env = r"\begin{matrix}".repeat(3_000);
    let script_storm = "^".repeat(5_000);
    let prime_storm = format!("x{}", "'".repeat(100_000));
    let sqrt_chain = r"\sqrt".repeat(5_000);
    let frac_chain = r"\frac1".repeat(5_000);
    let cases: Vec<String> = vec![
        deep_braces,
        deep_left,
        deep_env,
        script_storm,
        prime_storm,
        sqrt_chain,
        frac_chain,
        "\\".to_owned(),
        "$".to_owned(),
        "%".repeat(1_000),
        "\u{0000}\u{FFFF}\u{10FFFF}".to_owned(),
        format!(r"\begin{{{}}}", "n".repeat(10_000)),
        format!(r"\color{{{}", "r".repeat(10_000)),
    ];
    for s in &cases {
        // Must return (quickly) with a Result; panics/hangs fail.
        let _ = parse(s);
        let _ = parse_text(s);
    }
}

#[test]
fn prime_saturation_does_not_overflow() {
    // 300 primes: the counter saturates rather than wrapping or panicking.
    let s = format!("x{}", "'".repeat(300));
    assert!(parse(&s).is_ok());
}
