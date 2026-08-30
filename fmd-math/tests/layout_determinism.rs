//! Determinism: same string + faces ⇒ identical path bytes, run over run
//! (the cross-platform half rides CI's matrix — the arithmetic is pure
//! f64 add/mul with no transcendentals, so platform identity follows from
//! run identity plus IEEE 754).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::paths::{canonical_dump, layout_dump, resolve_paths};
use fmd_math::{Engine, Style};

#[test]
fn repeated_typesetting_is_byte_identical() {
    let srcs = [
        r"\frac{1}{1+\frac{1}{x}}",
        r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
        r"\int_0^1 \sqrt{1 - x^2} \, dx",
        r"\left( a + b \right)^n",
        r"e^{i\pi} + 1 = 0",
        r"x_i^2 \cdot y'_j",
    ];
    for src in srcs {
        let mut dumps: Vec<String> = Vec::new();
        for _ in 0..3 {
            // A fresh engine each run: byte identity must not depend on any
            // warm state.
            let engine = match Engine::bundled() {
                Ok(e) => e,
                Err(e) => panic!("bundled faces: {e}"),
            };
            let layout = engine
                .typeset(src, Style::Display)
                .unwrap_or_else(|e| panic!("`{src}`: {e}"));
            let contours = resolve_paths(&engine, &layout).unwrap();
            dumps.push(format!(
                "{}{}",
                layout_dump(&layout),
                canonical_dump(&contours)
            ));
        }
        assert_eq!(dumps[0], dumps[1], "`{src}` drifted between runs");
        assert_eq!(dumps[1], dumps[2], "`{src}` drifted between runs");
    }
}

#[test]
fn text_mode_typesetting_is_deterministic() {
    let src = r"the area $\pi r^2$ of a \textbf{circle}";
    let a = {
        let engine = Engine::bundled().unwrap();
        layout_dump(&engine.typeset_text(src).unwrap())
    };
    let b = {
        let engine = Engine::bundled().unwrap();
        layout_dump(&engine.typeset_text(src).unwrap())
    };
    assert_eq!(a, b);
}

#[test]
fn warm_engine_memo_matches_cold_engine_byte_for_byte() {
    // The per-(face, gid) metrics memo must be invisible in the output: a
    // reused engine (warm memo from earlier formulas) typesets byte-identically
    // to a fresh engine computing every glyph metric from the font tables.
    let warmups = [
        r"\alpha\beta\gamma + 1234567890",
        r"\sqrt[3]{\frac{x^2 y'}{z_{ij}}} \ne \int_{-\infty}^{\infty}",
    ];
    let srcs = [
        r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
        r"\int_0^1 \sqrt{1 - x^2} \, dx + 42 x_i^2 \cdot y'_j",
        r"e^{i\pi} + 1 = 0 \qquad \left( a + b \right)^n",
        r"\textbf{x} \in \mathbb{R}^{7 \times 7}",
    ];
    let warm = Engine::bundled().unwrap();
    for w in warmups {
        // Populate the memo with glyphs the measured sources also use.
        std::hint::black_box(warm.typeset(w, Style::Display).unwrap());
    }
    for src in srcs {
        let cold = {
            let engine = Engine::bundled().unwrap();
            let layout = engine.typeset(src, Style::Display).unwrap();
            let contours = resolve_paths(&engine, &layout).unwrap();
            format!("{}{}", layout_dump(&layout), canonical_dump(&contours))
        };
        let hot = {
            let layout = warm.typeset(src, Style::Display).unwrap();
            let contours = resolve_paths(&warm, &layout).unwrap();
            format!("{}{}", layout_dump(&layout), canonical_dump(&contours))
        };
        assert_eq!(cold, hot, "`{src}` differs between cold and warm engine");
    }
}
