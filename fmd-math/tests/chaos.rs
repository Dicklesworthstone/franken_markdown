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
    r"\small",
    r"\Large",
    r"\large",
    r"\huge",
    r"\tiny",
    r"\footnotesize",
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

/// The resource budgets (franken_manim fm-fmd-math-fuzz-budgets-k8fe): each
/// breach is a named [`MathError::Budget`], returned quickly.
#[cfg(feature = "bundled-faces")]
mod budgets {
    use std::time::{Duration, Instant};

    use fmd_math::{BudgetKind, Budgets, Engine, MathError, Style, parse, parse_text};

    /// The best of three timed runs, so a loaded host's scheduling noise does
    /// not fail a bound the code meets.
    fn best_of_three<T>(mut f: impl FnMut() -> T) -> (T, Duration) {
        let mut best = Duration::MAX;
        let mut value = None;
        for _ in 0..3 {
            let start = Instant::now();
            let result = f();
            best = best.min(start.elapsed());
            value = Some(result);
        }
        (value.unwrap(), best)
    }

    #[test]
    fn a_mebibyte_formula_is_refused_by_the_input_budget_quickly() {
        let engine = Engine::bundled().unwrap();
        let expected = MathError::Budget {
            kind: BudgetKind::InputBytes,
            limit: 65_536,
            at: 65_536,
        };
        let formula = "x+".repeat(1 << 19);
        assert_eq!(formula.len(), 1 << 20);
        for result in [
            best_of_three(|| parse(&formula)),
            best_of_three(|| parse_text(&formula)),
        ] {
            let (err, elapsed) = (result.0.unwrap_err(), result.1);
            assert_eq!(err, expected);
            assert!(elapsed < Duration::from_millis(50), "{elapsed:?}");
        }
        for result in [
            best_of_three(|| engine.typeset(&formula, Style::Display)),
            best_of_three(|| engine.typeset_text(&formula)),
        ] {
            let (err, elapsed) = (result.0.unwrap_err(), result.1);
            assert_eq!(err, expected);
            assert!(elapsed < Duration::from_millis(50), "{elapsed:?}");
        }
        assert_eq!(
            expected.to_string(),
            "resource budget `input-bytes` exceeded at byte 65536: more than 65536 source bytes"
        );
        // The offset stays on a character boundary: 65 536 is inside a
        // three-byte arrow, so the error points at the arrow's first byte.
        let arrows = "→".repeat(30_000);
        assert_eq!(
            parse(&arrows).unwrap_err(),
            MathError::Budget {
                kind: BudgetKind::InputBytes,
                limit: 65_536,
                at: 65_535,
            }
        );
        // Exactly at the budget is inside it.
        assert!(parse(&"x".repeat(65_536)).is_ok());
    }

    #[test]
    fn the_input_budget_is_configurable_per_engine() {
        let small = Engine::bundled().unwrap().with_budgets(Budgets {
            max_input_bytes: 8,
            ..Budgets::DEFAULT
        });
        assert_eq!(small.budgets().max_input_bytes, 8);
        assert!(small.typeset("a+b=c", Style::Text).is_ok());
        assert_eq!(
            small.typeset("a+b+c+d=e", Style::Text).unwrap_err(),
            MathError::Budget {
                kind: BudgetKind::InputBytes,
                limit: 8,
                at: 8,
            }
        );
        // A larger budget admits what the default refuses.
        let large = Engine::bundled().unwrap().with_budgets(Budgets {
            max_input_bytes: 1 << 17,
            ..Budgets::DEFAULT
        });
        let source = "x".repeat(70_000);
        assert!(matches!(
            large.typeset_text(&source).unwrap_err(),
            MathError::Budget {
                kind: BudgetKind::Glyphs,
                limit: 65_536,
                ..
            }
        ));
        assert!(matches!(
            Engine::bundled()
                .unwrap()
                .typeset_text(&source)
                .unwrap_err(),
            MathError::Budget {
                kind: BudgetKind::InputBytes,
                ..
            }
        ));
    }

    #[test]
    fn a_ten_thousand_deep_brace_chain_is_refused_by_the_nesting_budget_quickly() {
        let engine = Engine::bundled().unwrap();
        let chain = format!("{}x{}", "{".repeat(10_000), "}".repeat(10_000));
        let expected = MathError::Budget {
            kind: BudgetKind::Nesting,
            limit: 64,
            at: 64,
        };
        for result in [
            best_of_three(|| parse(&chain)),
            best_of_three(|| parse_text(&chain)),
        ] {
            let (err, elapsed) = (result.0.unwrap_err(), result.1);
            assert_eq!(err, expected);
            assert!(elapsed < Duration::from_millis(50), "{elapsed:?}");
        }
        let (err, elapsed) = best_of_three(|| engine.typeset(&chain, Style::Display));
        assert_eq!(err.unwrap_err(), expected);
        assert!(elapsed < Duration::from_millis(50), "{elapsed:?}");
        assert_eq!(
            expected.to_string(),
            "resource budget `nesting` exceeded at byte 64: more than 64 nesting levels"
        );
    }

    /// Every nesting construct, opened 10 000 times, trips the nesting budget;
    /// at the deepest nesting that parses, layout returns (no stack overflow).
    #[test]
    fn every_nesting_construct_is_bounded_and_lays_out_at_the_bound() {
        let engine = Engine::bundled().unwrap();
        let constructs: &[(&str, &str)] = &[
            ("{", "}"),
            (r"\left(", r"\right)"),
            (r"\begin{matrix}", r"\end{matrix}"),
            (r"\sqrt{", "}"),
            (r"\frac{", "}{x}"),
            ("x^{", "}"),
            (r"\overbrace{", "}"),
            (r"\text{$", "$}"),
        ];
        for (open, close) in constructs {
            let deep = format!("{}x{}", open.repeat(10_000), close.repeat(10_000));
            let err = parse(&deep).unwrap_err();
            assert!(
                matches!(
                    err,
                    MathError::Budget {
                        kind: BudgetKind::Nesting | BudgetKind::InputBytes,
                        ..
                    }
                ),
                "{open}: {err}"
            );
            let deepest = (1..=64)
                .rev()
                .map(|depth| format!("{}x{}", open.repeat(depth), close.repeat(depth)))
                .find(|source| parse(source).is_ok())
                .unwrap_or_else(|| panic!("{open}: no nesting depth parses"));
            // Any Result is fine; returning at all is the property.
            let _ = engine.typeset(&deepest, Style::Display);
        }
    }

    #[test]
    fn output_budgets_refuse_at_the_first_primitive_past_the_limit() {
        let engine = Engine::bundled().unwrap();
        let glyphs = engine.typeset("abcd", Style::Text).unwrap().glyphs.len();
        assert_eq!(glyphs, 4);
        let tight = Engine::bundled().unwrap().with_budgets(Budgets {
            max_glyphs: 3,
            ..Budgets::DEFAULT
        });
        assert_eq!(
            tight.typeset("abcd", Style::Text).unwrap_err(),
            MathError::Budget {
                kind: BudgetKind::Glyphs,
                limit: 3,
                at: 3,
            }
        );
        assert!(tight.typeset("abc", Style::Text).is_ok());

        let source = r"x+\widehat{x+y}";
        let layout = engine.typeset(source, Style::Display).unwrap();
        let segments: usize = layout
            .paths
            .iter()
            .flat_map(|path| &path.contours)
            .map(|contour| contour.segments.len())
            .sum();
        assert!(segments > 0);
        let at_limit = Engine::bundled().unwrap().with_budgets(Budgets {
            max_path_segments: segments,
            ..Budgets::DEFAULT
        });
        assert!(at_limit.typeset(source, Style::Display).is_ok());
        let under = Engine::bundled().unwrap().with_budgets(Budgets {
            max_path_segments: segments - 1,
            ..Budgets::DEFAULT
        });
        assert_eq!(
            under.typeset(source, Style::Display).unwrap_err(),
            MathError::Budget {
                kind: BudgetKind::PathSegments,
                limit: segments - 1,
                at: 2,
            }
        );
    }
}
