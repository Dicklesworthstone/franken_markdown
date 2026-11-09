#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{RStyle, SvgOptions};
use super::*;
use franken_markdown::{FontAssetSlot, FontAssets, fonts::FontStyle, theme::SystemAppearance};

fn painted(code: &str, language: Option<&str>, width: f64) -> Poster {
    let mut poster = Poster::new(&SvgOptions::default());
    poster.fenced_code_panel(code, language, 0.0, width);
    poster
}

fn without_glyph_colors(ops: &[Op]) -> Vec<Op> {
    ops.iter()
        .cloned()
        .map(|mut op| {
            if let Op::Glyph { ink, .. } = &mut op {
                *ink = Ink::Fg;
            }
            op
        })
        .collect()
}

#[test]
fn multiline_tokens_keep_their_colors_through_tabs_and_emergency_wrapping() {
    let source = "let café = \"line one\n\tline two\";\n/* alpha\n\tbeta */\nlet n = 42;";
    let colored = painted(source, Some("rust"), 60.0);
    let plain = painted(source, None, 60.0);
    assert_eq!(without_glyph_colors(&colored.ops), plain.ops);
    assert_eq!(colored.y, plain.y);
    assert_eq!(colored.warnings, plain.warnings);
    let style = RStyle {
        mono: true,
        ..RStyle::BODY
    };
    let expected: Vec<_> = [
        (Tok::Keyword, "let"),
        (Tok::Plain, "café"),
        (Tok::Operator, "="),
        (Tok::Str, "\"line oneline two\""),
        (Tok::Punct, ";"),
        (Tok::Comment, "/* alphabeta */"),
        (Tok::Keyword, "let"),
        (Tok::Plain, "n"),
        (Tok::Operator, "="),
        (Tok::Number, "42"),
        (Tok::Punct, ";"),
    ]
    .into_iter()
    .flat_map(|(token, text)| {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .map(move |ch| (token, ch))
    })
    .map(|(token, ch)| {
        let (slot, gid) = colored.resolve(ch, style);
        (slot, gid, Ink::Syntax(token))
    })
    .collect();
    let actual: Vec<_> = colored
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph { slot, gid, ink, .. } => Some((*slot, *gid, *ink)),
            _ => None,
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn composed_and_positioned_clusters_keep_the_exact_plain_code_geometry() {
    let custom = FontAssets::default()
        .with_slot(
            FontAssetSlot::MonoRegular,
            franken_markdown::fonts::body_bytes(
                franken_markdown::FontFamily::Serif,
                FontStyle::Regular,
            )
            .to_vec(),
        )
        .unwrap();
    let source = "let café = \"e\u{0301}\tfi\";\r\n// a\u{0301}\u{0307} fj\r\n\r\n  let x = 12;\r";
    for fonts in [FontAssets::default(), custom] {
        for width in [40.0, 270.0] {
            let mut colored = Poster::new(&SvgOptions::default())
                .with_resources(&fonts, &[])
                .unwrap();
            let mut plain = Poster::new(&SvgOptions::default())
                .with_resources(&fonts, &[])
                .unwrap();
            colored.fenced_code_panel(source, Some("rust"), 0.0, width);
            plain.code_panel(source, 0.0, width);
            assert_eq!(without_glyph_colors(&colored.ops), plain.ops);
            assert_eq!(colored.y, plain.y);
            assert_eq!(colored.missing, plain.missing);
            assert_eq!(colored.warnings, plain.warnings);
            assert!(colored.ops.iter().any(|op| matches!(
                op,
                Op::Glyph {
                    ink: Ink::Syntax(Tok::Str),
                    ..
                }
            )));
            let rows = colored.code_words(source, f64::from(colored.scale.code), width);
            assert_eq!(
                rows.iter().map(|row| row.text.as_str()).collect::<String>(),
                "let café = \"e\u{0301}  fi\";// a\u{0301}\u{0307} fj  let x = 12;\r"
            );
        }
    }
}

#[test]
fn every_palette_class_uses_the_shared_effective_appearance() {
    for appearance in [
        SystemAppearance::Light,
        SystemAppearance::Dark,
        SystemAppearance::HighContrastLight,
        SystemAppearance::HighContrastDark,
    ] {
        let mut theme = Theme {
            appearance,
            ..Theme::default()
        };
        theme.colors.fg = "#123456".into();
        theme.dark_colors.fg = "#fedcba".into();
        let colors = theme.effective_colors(false, false);
        let palette = Palette::new(&theme);
        for token in TOKENS {
            assert_eq!(palette.color(token), theme.code_color(colors, token));
        }
        assert_eq!(palette.color(Tok::Plain), colors.fg);
        assert_eq!(palette.color(Tok::Punct), colors.fg);
        assert_eq!(palette.color(Tok::Operator), palette.color(Tok::Number));
        if matches!(
            appearance,
            SystemAppearance::HighContrastLight | SystemAppearance::HighContrastDark
        ) {
            assert!(
                TOKENS
                    .into_iter()
                    .all(|token| palette.color(token) == colors.fg)
            );
        }
    }
}

#[test]
fn source_projection_checks_complete_utf8_spans_and_physical_line_boundaries() {
    let source = "é\tx\r\ny\n\n";
    let spans = [
        Span {
            kind: Tok::Str,
            start: 0,
            end: 4,
        },
        Span {
            kind: Tok::Comment,
            start: 4,
            end: source.len(),
        },
    ];
    let plan = Plan::from_source(source, &spans).unwrap();
    assert_eq!(plan.bytes, "é   xy".len());
    assert_eq!(
        plan.spans,
        [
            Span {
                kind: Tok::Str,
                start: 0,
                end: "é   x".len()
            },
            Span {
                kind: Tok::Comment,
                start: "é   x".len(),
                end: "é   xy".len()
            }
        ]
    );
    for invalid_spans in [
        vec![
            Span {
                kind: Tok::Plain,
                start: 0,
                end: 1,
            },
            Span {
                kind: Tok::Plain,
                start: 1,
                end: source.len(),
            },
        ],
        vec![Span {
            kind: Tok::Plain,
            start: 0,
            end: source.len() - 1,
        }],
        vec![Span {
            kind: Tok::Plain,
            start: 1,
            end: source.len(),
        }],
        vec![Span {
            kind: Tok::Plain,
            start: 0,
            end: source.len() + 1,
        }],
        vec![Span {
            kind: Tok::Plain,
            start: 0,
            end: 0,
        }],
    ] {
        assert_eq!(
            Plan::from_source(source, &invalid_spans)
                .err()
                .unwrap()
                .code,
            "svg_code_highlight_invalid"
        );
    }
}

#[test]
fn per_fence_and_cumulative_limits_refuse_before_partial_coloring() {
    let mut state = Highlighting::new(&Theme::default());
    for (source, language) in [
        ("x".repeat(MAX_SOURCE_BYTES + 1), "rust".to_owned()),
        ("let x = 1;".to_owned(), "R".repeat(MAX_LANGUAGE_BYTES + 1)),
    ] {
        let remaining = state.remaining_input;
        assert_eq!(
            state.prepare(&source, Some(&language)).err().unwrap().code,
            "svg_code_highlight_limit"
        );
        assert_eq!(state.remaining_input, remaining);
    }
    let source = "let answer = 42;";
    let mut poster = Poster::new(&SvgOptions::default());
    poster.code.remaining_input = source.len() + "rust".len();
    poster.fenced_code_panel(source, Some("rust"), 0.0, 140.0);
    let start = poster.ops.len();
    poster.fenced_code_panel(source, Some("rust"), 0.0, 140.0);
    assert!(poster.ops[..start].iter().any(|op| matches!(
        op,
        Op::Glyph {
            ink: Ink::Syntax(Tok::Keyword),
            ..
        }
    )));
    assert!(poster.ops[start..].iter().all(|op| !matches!(
        op,
        Op::Glyph {
            ink: Ink::Syntax(_),
            ..
        }
    )));
    assert_eq!(poster.warnings.len(), 1);
    assert_eq!(poster.warnings[0].code, "svg_code_highlight_limit");
    let mut plain = painted(source, None, 140.0);
    plain.code_panel(source, 0.0, 140.0);
    assert_eq!(without_glyph_colors(&poster.ops), plain.ops);
    assert_eq!(poster.y, plain.y);
    let mut state = Highlighting::new(&Theme::default());
    state.remaining_spans = 1;
    assert_eq!(
        state.prepare(source, Some("rust")).err().unwrap().code,
        "svg_code_highlight_limit"
    );
    assert_eq!(
        state.remaining_input,
        TOTAL_INPUT_BYTES - source.len() - "rust".len()
    );
}

#[test]
fn unknown_languages_empty_code_and_plain_fallbacks_keep_their_output() {
    for source in ["", "\n\n", "let x = 42;\n\t// comment\n"] {
        let ordinary = painted(source, None, 140.0);
        let unknown = painted(source, Some("unknown-language"), 140.0);
        assert_eq!(ordinary.ops, unknown.ops);
        assert_eq!(ordinary.y, unknown.y);
        assert_eq!(ordinary.warnings, unknown.warnings);
    }
    let mut state = Highlighting::new(&Theme::default());
    state.remaining_input = 0;
    assert!(
        state
            .prepare("source", Some("unknown-language"))
            .unwrap()
            .is_none()
    );
    assert!(state.prepare("", Some("rust")).unwrap().is_none());
}
