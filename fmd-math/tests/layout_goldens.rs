//! Layout goldens: the G0-3 spike construct set plus the corpus head,
//! bit-locked as canonical dumps (structural placement + resolved
//! quadratic path bytes). Bless with `FMD_MATH_UPDATE_GOLDENS=1`;
//! any other diff is a regression to adjudicate, not noise to re-bless.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::paths::{canonical_dump, layout_dump, resolve_paths};
use fmd_math::{Engine, Style};

/// The golden set: the constructs the G0-3 spike proved, plus the highest-
/// occurrence corpus strings (project-authored equivalents).
const CASES: &[(&str, &str, Style)] = &[
    (
        "nested_display_fraction",
        r"\frac{1}{1+\frac{1}{x}}",
        Style::Display,
    ),
    ("simultaneous_scripts", r"x_i^2 + f'", Style::Display),
    ("sum_limits_display", r"\sum_{n=1}^{N} n", Style::Display),
    ("sum_limits_text", r"\sum_{n=1}^{N} n", Style::Text),
    ("integral", r"\int_0^1 x \, dx", Style::Display),
    ("radical_with_degree", r"\sqrt[3]{x+1}", Style::Display),
    (
        "left_right_fraction",
        r"\left( \frac{a}{x} \right)",
        Style::Display,
    ),
    ("euler", r"e^{i\pi} + 1 = 0", Style::Display),
    ("corpus_head_eq", r"=", Style::Display),
    ("corpus_head_plus", r"+", Style::Display),
    ("corpus_head_ddx", r"\frac{d}{dx}", Style::Display),
    ("over_idiom", r"{a + b \over 2}", Style::Display),
    ("accents", r"\hat x + \vec{v} + \bar z", Style::Display),
    (
        "overline_underline",
        r"\overline{AB} + \underline{x}",
        Style::Display,
    ),
    ("binom", r"\binom{n}{k}", Style::Display),
    ("stackrel", r"a \stackrel{?}{=} b", Style::Display),
    (
        "alphabets",
        r"\mathbb{R} \mathrm{d} \mathbf{v}",
        Style::Display,
    ),
];

fn golden_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

#[test]
fn layout_and_path_goldens() {
    let engine = match Engine::bundled() {
        Ok(e) => e,
        Err(e) => panic!("bundled faces: {e}"),
    };
    let update = std::env::var("FMD_MATH_UPDATE_GOLDENS").is_ok();
    let dir = golden_dir();
    if update {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut failures = Vec::new();
    for (name, src, style) in CASES {
        let layout = engine
            .typeset(src, *style)
            .unwrap_or_else(|e| panic!("{name}: `{src}` failed: {e}"));
        let contours = resolve_paths(&engine, &layout)
            .unwrap_or_else(|e| panic!("{name}: path resolution failed: {e}"));
        let dump = format!("{}{}", layout_dump(&layout), canonical_dump(&contours));
        let path = dir.join(format!("{name}.txt"));
        if update {
            std::fs::write(&path, &dump).unwrap();
            continue;
        }
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{name}: missing golden {}: {e}", path.display()));
        if expected != dump {
            failures.push((*name).to_owned());
        }
    }
    assert!(
        failures.is_empty(),
        "layout goldens drifted (adjudicate, don't re-bless blindly): {failures:?}"
    );
}
