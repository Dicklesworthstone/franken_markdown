//! Determinism: same string + faces ⇒ identical path bytes, run over run
//! (the cross-platform half rides CI's matrix — the arithmetic is pure
//! f64 add/mul with no transcendentals, so platform identity follows from
//! run identity plus IEEE 754).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::paths::{canonical_dump, layout_dump, resolve_paths};
use fmd_math::{Engine, Style};

#[test]
fn ams_symbols_use_authored_faces_metrics_and_optical_cuts() {
    use fmd_math::FaceId;

    // Independent TFM fix_words / 2^20, from the fonts actually emitted by
    // the pinned Reference DVI: text, script and scriptscript cuts. The IDs
    // also lock the appended roster order without deriving it from the loader.
    let cases = [
        (
            r"\checkmark",
            '✓',
            17,
            0x58,
            [0.8333358764648438, 0.9543695449829102, 1.1527957916259766],
        ),
        (
            r"\minus",
            '−',
            17,
            0x39,
            [0.5000019073486328, 0.5535736083984375, 0.6250123977661133],
        ),
        (
            r"\circlearrowright",
            '↻',
            17,
            0x08,
            [0.7777805328369141, 0.8928613662719727, 1.0833492279052734],
        ),
        (
            r"\circlearrowleft",
            '↺',
            17,
            0x09,
            [0.7777805328369141, 0.8928613662719727, 1.0833492279052734],
        ),
        (
            r"\hbar",
            'ℏ',
            20,
            0x7E,
            [0.5402803421020508, 0.6329412460327148, 0.7916774749755859],
        ),
        (
            r"\nmid",
            '∤',
            20,
            0x2D,
            [
                0.27777862548828125,
                0.33928775787353516,
                0.45833778381347656,
            ],
        ),
        (
            r"\mathbb{Z}",
            'ℤ',
            20,
            0x5A,
            [0.6666688919067383, 0.666661262512207, 0.6666507720947266],
        ),
    ];
    let engine = Engine::bundled().unwrap();
    for (source, semantic, base, code, widths) in cases {
        for (style, cut, scale) in [
            (Style::Display, 0, 1.0),
            (Style::Text, 0, 1.0),
            (Style::Script, 1, 0.7),
            (Style::ScriptScript, 2, 0.5),
        ] {
            let layout = engine.typeset(source, style).unwrap();
            assert_eq!(layout.glyphs.len(), 1, "{source} {style:?}");
            assert!(layout.paths.is_empty() && layout.rules.is_empty());
            let glyph = &layout.glyphs[0];
            let face = FaceId(base + cut);
            assert_eq!(glyph.face, face, "{source} {style:?}");
            assert_eq!(glyph.gid, engine.faces().tex_glyph(face, code).unwrap());
            assert_eq!(
                glyph.ch, semantic,
                "never expose the private-use code as text"
            );
            assert!(
                (layout.width - widths[cut] * scale).abs() < 2e-15,
                "{source} {style:?}: {}",
                layout.width
            );
            assert!(!resolve_paths(&engine, &layout).unwrap().is_empty());
        }
    }
}

#[test]
fn shortminus_does_not_replace_ordinary_subtraction_or_lose_its_identity_in_alphabets() {
    let engine = Engine::bundled().unwrap();
    let ordinary = engine.typeset("-", Style::Display).unwrap();
    let short = engine.typeset(r"\minus", Style::Display).unwrap();
    assert_eq!(ordinary.glyphs[0].face, fmd_math::FaceId(8));
    assert_eq!(short.glyphs[0].face, fmd_math::FaceId(17));
    assert!(short.width < ordinary.width);
    assert_ne!(
        canonical_dump(&resolve_paths(&engine, &short).unwrap()),
        canonical_dump(&resolve_paths(&engine, &ordinary).unwrap())
    );
    for source in [r"\mathbf{\minus}", r"\mathit{\minus}", r"\mathbb{\minus}"] {
        let wrapped = engine.typeset(source, Style::Display).unwrap();
        let start = source.find(r"\minus").unwrap();
        let expected_span = fmd_math::Span::new(start, start + r"\minus".len());
        assert_eq!(wrapped.glyphs[0].span, expected_span);
        assert_eq!(
            wrapped.glyphs,
            short
                .glyphs
                .iter()
                .map(|glyph| {
                    let mut glyph = glyph.clone();
                    glyph.span = expected_span;
                    glyph
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(wrapped.width, short.width);
    }
}

#[test]
fn custom_rosters_keep_unicode_fallback_and_refuse_missing_authored_codes() {
    use fmd_math::{FaceId, MathError, Span, faces::FaceSet};

    let font =
        || fmd_font::Font::parse(fmd_font::bundled::NOTO_SANS_MATH_SYMBOLS.to_vec()).unwrap();
    let source = r"\checkmark";
    let fallback = Engine::new(FaceSet::from_fonts(vec![font()]));
    let layout = fallback.typeset(source, Style::Display).unwrap();
    assert_eq!(layout.glyphs.len(), 1);
    assert_eq!(layout.glyphs[0].face, FaceId(0));
    assert_eq!(layout.glyphs[0].ch, '✓');

    // A roster claiming the stable AMS-A slot but lacking its authored code
    // must report the semantic character and original source span. A Unicode
    // checkmark in another slot cannot disguise the corrupt AMS face.
    let incomplete = Engine::new(FaceSet::from_fonts((0..18).map(|_| font()).collect()));
    assert_eq!(
        incomplete.typeset(source, Style::Display).unwrap_err(),
        MathError::UnmappedChar {
            ch: '✓',
            span: Span::new(0, source.len()),
        }
    );
}

#[test]
fn default_pack_keeps_authored_minus_and_request_local_shadowing() {
    let engine = Engine::bundled().unwrap();
    let pack = fmd_math::MacroSet::pack("fmd-math/pack/default").unwrap();
    for (style, face) in [
        (Style::Display, 17),
        (Style::Text, 17),
        (Style::Script, 18),
        (Style::ScriptScript, 19),
    ] {
        let layout = engine.typeset_with_macros(r"\minus", style, &pack).unwrap();
        assert_eq!(layout.glyphs.len(), 1);
        assert_eq!(layout.glyphs[0].face, fmd_math::FaceId(face));
        assert_eq!(layout.glyphs[0].ch, '−');
        assert_eq!(layout.glyphs[0].span, fmd_math::Span::new(0, 6));
    }
    assert!(
        engine
            .typeset_with_macros(r"\newcommand{\minus}{+}\minus", Style::Text, &pack)
            .is_err()
    );
    let renewed = engine
        .typeset_with_macros(r"\renewcommand{\minus}{+}\minus", Style::Text, &pack)
        .unwrap();
    assert_eq!(renewed.glyphs[0].ch, '+');
    assert_eq!(
        engine
            .typeset_with_macros(r"\minus", Style::Text, &pack)
            .unwrap()
            .glyphs[0]
            .face,
        fmd_math::FaceId(17)
    );
    let empty = fmd_math::MacroSet::new();
    let mut literal = pack.clone();
    literal.define("minus", 0, "-").unwrap();
    assert_ne!(pack.canonical_bytes(), literal.canonical_bytes());
    assert_eq!(
        engine
            .typeset_with_macros(r"\minus", Style::Text, &literal)
            .unwrap()
            .glyphs[0]
            .face,
        fmd_math::FaceId(8)
    );
    assert!(
        engine
            .typeset_with_macros(r"\newcommand{\minus}{+}\minus", Style::Text, &empty)
            .is_ok()
    );
    assert!(
        engine
            .typeset_with_macros(r"\renewcommand{\minus}{+}\minus", Style::Text, &empty)
            .is_err()
    );
}

#[test]
fn repeated_typesetting_is_byte_identical() {
    let srcs = [
        r"\frac{1}{1+\frac{1}{x}}",
        r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
        r"\int_0^1 \sqrt{1 - x^2} \, dx",
        r"\left( a + b \right)^n",
        r"e^{i\pi} + 1 = 0",
        r"x_i^2 \cdot y'_j",
        r"\checkmark \circlearrowleft \circlearrowright \hbar \nmid \mathbb{Z}",
        r"e^{\minus t} + e^{\minus\minus t}",
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

#[test]
fn one_engine_shared_across_threads_matches_a_cold_engine() {
    // Engine is Send + Sync: consumers (franken_manim's TeX engine) share one
    // across parallel layout threads. Concurrent memo fills must stay
    // invisible: every thread's output equals a fresh single-threaded engine.
    fn shareable<T: Send + Sync>() {}
    shareable::<Engine>();
    let srcs = [
        r"\sum_{n=1}^{\infty} \frac{1}{n^2} = \frac{\pi^2}{6}",
        r"\int_0^1 \sqrt{1 - x^2} \, dx + 42 x_i^2 \cdot y'_j",
        r"e^{i\pi} + 1 = 0 \qquad \left( a + b \right)^n",
        r"\alpha\beta\gamma \ne \sqrt[3]{\frac{x^2 y'}{z_{ij}}}",
    ];
    let dump =
        |engine: &Engine, src: &str| layout_dump(&engine.typeset(src, Style::Display).unwrap());
    let cold: Vec<String> = srcs
        .iter()
        .map(|src| dump(&Engine::bundled().unwrap(), src))
        .collect();
    let shared = Engine::bundled().unwrap();
    let per_thread: Vec<Vec<String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|offset| {
                let shared = &shared;
                scope.spawn(move || {
                    (0..srcs.len())
                        .map(|i| {
                            let index = (i + offset) % srcs.len();
                            (index, dump(shared, srcs[index]))
                        })
                        .fold(vec![String::new(); srcs.len()], |mut out, (index, text)| {
                            out[index] = text;
                            out
                        })
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    for outputs in per_thread {
        assert_eq!(outputs, cold);
    }
}
