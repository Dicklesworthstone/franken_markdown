//! Real-font wrapping and emitted glyph checks for authored prose hints.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{SvgOptions, render_svg_with_diagnostics};
use super::*;
use franken_markdown::{FontAssets, parse_markdown};

fn poster() -> Poster {
    Poster::new(&SvgOptions::default())
}

fn fixture_poster() -> Poster {
    const FONT: &[u8] = include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf");
    poster()
        .with_resources(
            &FontAssets {
                body_regular: Some(FONT.to_vec()),
                body_bold: Some(FONT.to_vec()),
                mono_regular: Some(FONT.to_vec()),
                ..FontAssets::default()
            },
            &[],
        )
        .unwrap()
}

fn wrap(poster: &Poster, text: &str, width: f64) -> Vec<Vec<Word>> {
    poster.wrap(&[Piece::Text(text.into(), RStyle::BODY)], 10.0, width)
}

fn text(line: &[Word]) -> String {
    line.iter().map(|run| run.text.as_str()).collect()
}

fn glyphs(poster: &Poster) -> Vec<(usize, u16, f64, f64)> {
    poster
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph {
                slot, gid, x, y, ..
            } => Some((*slot, *gid, *x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn unused_hints_keep_real_ligatures_and_pair_adjustments_at_exact_fitting_widths() {
    for (hinted, clean, width) in [
        ("f\u{00ad}i", "fi", 5.0),
        ("f\u{200b}i", "fi", 5.0),
        ("a\u{00ad}b", "ab", 9.6),
        ("a\u{200b}b", "ab", 9.6),
        ("f\u{00ad}i a\u{200b}b", "fi ab", 17.6),
    ] {
        let mut a = fixture_poster();
        let mut b = fixture_poster();
        let left = wrap(&a, hinted, width);
        let right = wrap(&b, clean, width);
        assert_eq!(left.len(), right.len(), "{hinted:?}");
        for (index, (a_line, b_line)) in left.iter().zip(&right).enumerate() {
            assert_eq!(text(a_line), text(b_line));
            assert_eq!(a.words_width(a_line, 10.0), b.words_width(b_line, 10.0));
            a.draw_words(a_line, 0.0, 20.0 * index as f64, 10.0);
            b.draw_words(b_line, 0.0, 20.0 * index as f64, 10.0);
        }
        assert_eq!(glyphs(&a), glyphs(&b));
        assert_eq!(a.missing, 0);
    }
}

#[test]
fn selected_soft_hyphens_paint_once_and_zero_width_opportunities_never_paint() {
    let p = poster();
    let measure = Shaper::new(&p).shape("WWWW-", RStyle::BODY, 10.0).width() + 0.01;
    for (source, first) in [
        ("WWWW\u{00ad}WWWW", "WWWW-"),
        ("WWWW\u{200b}WWWW", "WWWW"),
        ("WWWW\u{200b}\u{00ad}\u{200b}WWWW", "WWWW-"),
    ] {
        let lines = wrap(&p, source, measure);
        assert_eq!(
            lines.iter().map(|line| text(line)).collect::<Vec<_>>(),
            [first, "WWWW"]
        );
        assert!(
            lines
                .iter()
                .all(|line| p.words_width(line, 10.0) <= measure)
        );
    }
}

#[test]
fn actual_fractional_fits_survive_integer_planner_refusal_in_single_and_multiword_text() {
    let p = fixture_poster();
    let size = 10.0001;
    let measure = Shaper::new(&p).shape("ab", RStyle::BODY, size).width();
    assert!((measure * 1000.0).fract() > 0.0);
    for (hinted, clean) in [
        ("a\u{200b}b", "ab"),
        ("fi a\u{200b}b fi", "fi ab fi"),
        ("a\u{00ad}b a\u{200b}b", "ab ab"),
    ] {
        let left = p.wrap(&[Piece::Text(hinted.into(), RStyle::BODY)], size, measure);
        let right = p.wrap(&[Piece::Text(clean.into(), RStyle::BODY)], size, measure);
        assert_eq!(
            left.iter().map(|line| text(line)).collect::<Vec<_>>(),
            right.iter().map(|line| text(line)).collect::<Vec<_>>(),
            "{hinted:?}"
        );
        assert!(left.iter().all(|line| p.words_width(line, size) <= measure));
    }
}

#[test]
fn trailing_zero_ink_diagnostics_survive_on_exactly_one_physical_fragment() {
    let mut p = poster();
    let mut runs = vec![Word {
        text: "WWWW\u{200b}WWWW".into(),
        style: RStyle::BODY,
        w: 0.0,
        gap: 0.0,
        formula: None,
        image: None,
        warning: None,
        shaped: None,
    }];
    let authored = discretionary::normalize(&mut runs);
    let mut flow = TextFlow::default();
    {
        let shaper = Shaper::new(&p);
        for run in &mut runs {
            let prepared = shaper.shape(&run.text, run.style, 10.0);
            run.w = prepared.width();
            run.shaped = Some(prepared);
        }
        runs[0].warning = Some(SvgWarning {
            code: "original",
            message: "original".into(),
        });
        // The paragraph limiter appends a diagnostic-only run when real runs
        // already own resource/shaping warnings. Neither warning may disappear.
        runs.push(Word {
            text: String::new(),
            style: RStyle::BODY,
            w: 0.0,
            gap: 0.0,
            formula: None,
            image: None,
            shaped: None,
            warning: Some(SvgWarning {
                code: "svg_paragraph_limit",
                message: "limit".into(),
            }),
        });
        let measure = shaper.shape("WWWW", RStyle::BODY, 10.0).width() + 0.01;
        discretionary::place(
            &p,
            &mut flow,
            &shaper,
            paragraph::Group {
                runs,
                gap: 0.0,
                hints: authored.hints,
            },
            10.0,
            measure,
        );
    }
    flow.new_line();
    assert_eq!(flow.lines.len(), 2);
    for (index, line) in flow.lines.iter().enumerate() {
        p.draw_words(line, 0.0, 20.0 * index as f64, 10.0);
    }
    for code in ["original", "svg_paragraph_limit"] {
        assert_eq!(
            p.warnings
                .iter()
                .filter(|warning| warning.code == code)
                .count(),
            1,
            "{code}"
        );
    }
}

#[test]
fn unused_hints_inside_a_chosen_fragment_retain_its_joined_ligature() {
    let mut p = fixture_poster();
    let lines = wrap(&p, "ab\u{200b}f\u{00ad}i", 9.6);
    assert_eq!(
        lines.iter().map(|line| text(line)).collect::<Vec<_>>(),
        ["ab", "fi"]
    );
    p.draw_words(&lines[1], 0.0, 20.0, 10.0);
    assert_eq!(glyphs(&p), [(0, 6, 0.0, 20.0)]);
}

#[test]
fn leading_trailing_consecutive_and_hint_only_words_add_no_blank_lines_or_spaces() {
    let p = poster();
    for (source, expected) in [
        ("\u{00ad}\u{200b}alpha\u{00ad}\u{200b}", "alpha"),
        ("al\u{00ad}\u{00ad}pha\u{200b}\u{200b}beta", "alphabeta"),
        ("alpha \u{00ad}\u{200b} beta", "alphabeta"),
    ] {
        let lines = wrap(&p, source, 400.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(text(&lines[0]), expected);
    }
    assert!(wrap(&p, "\u{00ad}\u{200b}", 400.0).is_empty());
    let lines = p.wrap(
        &[
            Piece::Text("a".into(), RStyle::BODY),
            Piece::Break,
            Piece::Text("\u{00ad}\u{200b}".into(), RStyle::BODY),
        ],
        10.0,
        400.0,
    );
    assert_eq!(lines.len(), 2);
    assert!(lines[1].is_empty());
}

#[test]
fn a_hint_before_combining_marks_cannot_separate_the_marks_from_their_base() {
    let p = poster();
    for source in ["e\u{00ad}\u{0301}lite", "e\u{200b}\u{00ad}\u{0301}lite"] {
        for measure in [400.0, 15.0, 5.0] {
            let lines = wrap(&p, source, measure);
            let expected = wrap(&p, "e\u{0301}lite", measure);
            assert_eq!(
                lines.iter().map(|line| text(line)).collect::<Vec<_>>(),
                expected.iter().map(|line| text(line)).collect::<Vec<_>>()
            );
            assert!(lines.iter().all(|line| !text(line).starts_with('\u{0301}')));
        }
    }
}

#[test]
fn chosen_hyphen_keeps_the_authored_style_link_and_decoration() {
    let p = poster();
    let authored = RStyle {
        bold: true,
        strike: true,
        link: Some(7),
        ..RStyle::BODY
    };
    let shaper = Shaper::new(&p);
    let measure = shaper.shape("WWWW", RStyle::BODY, 10.0).width()
        + shaper.shape("-", authored, 10.0).width()
        + 0.01;
    let lines = p.wrap(
        &[
            Piece::Text("WWWW".into(), RStyle::BODY),
            Piece::Text("\u{200b}\u{00ad}".into(), authored),
            Piece::Text("WWWW".into(), RStyle::BODY),
        ],
        10.0,
        measure,
    );
    assert_eq!(lines.len(), 2);
    let hyphen = lines[0].last().unwrap();
    assert_eq!(hyphen.text, "-");
    assert_eq!(hyphen.style, authored);
    assert_eq!(lines[1][0].style, RStyle::BODY);
}

#[test]
fn literal_inline_and_fenced_code_keep_both_marker_characters() {
    let p = poster();
    let source = "f\u{00ad}i a\u{200b}b";
    let lines = p.wrap(
        &[Piece::Text(
            source.into(),
            RStyle {
                mono: true,
                ..RStyle::BODY
            },
        )],
        10.0,
        400.0,
    );
    assert_eq!(text(&lines[0]), source);
    assert_eq!(p.code_lines(source, 10.0, 400.0), [source]);
}

#[test]
fn formula_and_image_runs_stay_opaque_and_keep_their_diagnostics() {
    let p = poster();
    let lines = p.wrap(
        &[
            Piece::Math("x^2".into(), false, RStyle::BODY),
            Piece::Text("\u{200b}WWWW ".into(), RStyle::BODY),
            Piece::Image(
                "missing.png".into(),
                "co\u{00ad}operate".into(),
                RStyle::BODY,
            ),
        ],
        10.0,
        30.0,
    );
    assert_eq!(
        lines
            .iter()
            .flatten()
            .filter(|run| run.formula.is_some())
            .count(),
        1
    );
    assert!(
        lines
            .iter()
            .flatten()
            .any(|run| run.warning.is_some() && run.text.contains('\u{00ad}'))
    );
}

#[test]
fn bounded_fallback_keeps_complete_visible_text_and_a_single_limit_warning() {
    let mut p = poster();
    let source = "a\u{200b}".repeat(2100) + "tail";
    let lines = wrap(&p, &source, 60.0);
    assert_eq!(
        lines.iter().map(|line| text(line)).collect::<String>(),
        "a".repeat(2100) + "tail"
    );
    for (index, line) in lines.iter().enumerate() {
        assert!(p.words_width(line, 10.0) <= 60.000_001);
        p.draw_words(line, 0.0, 20.0 * index as f64, 10.0);
    }
    assert_eq!(
        p.warnings
            .iter()
            .filter(|warning| warning.code == "svg_discretionary_limit")
            .count(),
        1
    );
}

#[test]
fn complete_svg_has_no_unused_hint_glyphs_or_missing_glyph_diagnostics() {
    let options = SvgOptions::default();
    for (hinted, visible) in [
        ("co\u{00ad}operate alpha\u{200b}beta", "cooperate alphabeta"),
        ("**al\u{00ad}pha** *be\u{200b}ta*", "**alpha** *beta*"),
        ("> co\u{00ad}operate", "> cooperate"),
        ("- alpha\u{200b}beta", "- alphabeta"),
        (
            "| Word |\n| --- |\n| co\u{00ad}operate |",
            "| Word |\n| --- |\n| cooperate |",
        ),
    ] {
        let a = render_svg_with_diagnostics(&parse_markdown(hinted), &options);
        let b = render_svg_with_diagnostics(&parse_markdown(visible), &options);
        assert_eq!(a, b, "{hinted:?}");
    }
}
