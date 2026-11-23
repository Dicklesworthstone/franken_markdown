//! Navigation-only boundaries must never become font-shaping boundaries.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::{Ink, SvgOptions};
use super::*;
use franken_markdown::{FontAssets, Inline};

const FIXTURE: &[u8] = include_bytes!("../../fmd-font/fonts/test-shaping/FmdShaping.ttf");

fn poster() -> Poster {
    Poster::new(&SvgOptions::default())
        .with_resources(
            &FontAssets {
                body_regular: Some(FIXTURE.to_vec()),
                body_bold: Some(FIXTURE.to_vec()),
                body_italic: Some(FIXTURE.to_vec()),
                body_bold_italic: Some(FIXTURE.to_vec()),
                mono_regular: Some(FIXTURE.to_vec()),
                ..FontAssets::default()
            },
            &[],
        )
        .unwrap()
}

fn linked(poster: &Poster, parts: &[&str]) -> Vec<Piece> {
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let id = poster.navigation.borrow_mut().intern(
                &format!("https://example.com/{index}"),
                None,
                &[Inline::Text((*part).to_owned())],
            );
            Piece::Text(
                (*part).to_owned(),
                RStyle {
                    link: Some(id),
                    ..RStyle::BODY
                },
            )
        })
        .collect()
}

fn glyphs(poster: &Poster) -> Vec<(usize, u16, f64, f64, Ink)> {
    poster
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyph {
                slot,
                gid,
                x,
                y,
                ink,
                ..
            } => Some((*slot, *gid, *x, *y, *ink)),
            _ => None,
        })
        .collect()
}

fn attr<'a>(element: &'a str, name: &str) -> &'a str {
    element
        .split_once(&format!("{name}=\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}

fn regions(poster: Poster) -> Vec<(String, f64, f64, f64)> {
    let (bytes, _, warnings) = poster.emit(400.0);
    assert!(warnings.is_empty(), "{warnings:?}");
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("<a "))
        .map(|line| {
            let rect = line.split_once("<rect ").unwrap().1;
            let x: f64 = attr(rect, "x").parse().unwrap();
            let width: f64 = attr(rect, "width").parse().unwrap();
            (
                attr(line, "href").to_owned(),
                x,
                x + width,
                attr(rect, "y").parse().unwrap(),
            )
        })
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
}

fn assert_partition(parts: &[&str], expected: &[(u16, f64, f64)], edges: &[f64]) {
    let mut annotated = poster();
    let mut plain = poster();
    let lines = annotated.wrap(&linked(&annotated, parts), 10.0, 100.0);
    let control = plain.wrap(&[Piece::Text(parts.concat(), RStyle::BODY)], 10.0, 100.0);
    assert_eq!(lines.len(), 1);
    assert_eq!(control.len(), 1);
    close(
        annotated.words_width(&lines[0], 10.0),
        edges[edges.len() - 1] - edges[0],
    );
    annotated.draw_words(&lines[0], 17.0, 30.0, 10.0);
    plain.draw_words(&control[0], 17.0, 30.0, 10.0);
    let actual = glyphs(&annotated);
    assert_eq!(actual, glyphs(&plain), "navigation changed retained glyphs");
    assert_eq!(actual.len(), expected.len());
    for (actual, &(gid, x, y)) in actual.iter().zip(expected) {
        assert_eq!((actual.0, actual.1), (0, gid));
        close(actual.2, x);
        close(actual.3, y);
    }
    assert_eq!(annotated.missing, 0);
    let actual = regions(annotated);
    assert_eq!(actual.len(), parts.len());
    for (index, region) in actual.iter().enumerate() {
        assert_eq!(region.0, format!("https://example.com/{index}"));
        // Serialized rectangles round outwards on the established 0.01pt grid.
        assert!((region.1 - edges[index]).abs() <= 0.011);
        assert!((region.2 - edges[index + 1]).abs() <= 0.011);
        assert!(region.2 > region.1);
    }
}

#[test]
fn link_only_boundaries_preserve_saved_harfbuzz_pair_positions_and_targets() {
    // The independent font fixture specifies ab advances of 460 + 500 units.
    assert_partition(
        &["a", "b"],
        &[(2, 17.0, 30.0), (3, 21.6, 30.0)],
        &[17.0, 21.6, 26.6],
    );
}

#[test]
fn link_only_boundaries_keep_one_ligature_and_partition_its_source_targets() {
    // fi is one 500-unit glyph, even when each source letter owns a link.
    assert_partition(&["f", "i"], &[(6, 17.0, 30.0)], &[17.0, 19.5, 22.0]);
}

#[test]
fn link_only_boundaries_keep_mark_attachment_and_the_mark_target_reachable() {
    // Saved HarfBuzz: mark advance=0, positioned at (-350,+500) from a's pen.
    assert_partition(
        &["a", "\u{0301}", "b"],
        &[(2, 17.0, 30.0), (7, 18.5, 25.0), (3, 22.0, 30.0)],
        &[17.0, 19.5, 22.0, 27.0],
    );
}

#[test]
fn emergency_fragments_keep_every_link_without_splitting_ligatures_or_marks() {
    let parts = ["a", "b", "f", "i", "a", "\u{0301}", "b"];
    let mut annotated = poster();
    let mut plain = poster();
    let lines = annotated.wrap(&linked(&annotated, &parts), 10.0, 5.0);
    let control = plain.wrap(&[Piece::Text(parts.concat(), RStyle::BODY)], 10.0, 5.0);
    let text: Vec<String> = lines
        .iter()
        .map(|line| line.iter().map(|run| run.text.as_str()).collect())
        .collect();
    assert_eq!(text, ["a", "b", "fi", "a\u{0301}", "b"]);
    assert_eq!(lines.len(), control.len());
    for (index, (actual, expected)) in lines.iter().zip(&control).enumerate() {
        let baseline = 30.0 + 30.0 * index as f64;
        close(
            annotated.words_width(actual, 10.0),
            plain.words_width(expected, 10.0),
        );
        annotated.draw_words(actual, 17.0, baseline, 10.0);
        plain.draw_words(expected, 17.0, baseline, 10.0);
    }
    assert_eq!(glyphs(&annotated), glyphs(&plain));
    let actual = regions(annotated);
    assert_eq!(actual.len(), parts.len());
    for (index, region) in actual.iter().enumerate() {
        assert_eq!(region.0, format!("https://example.com/{index}"));
        assert!(region.1 >= 17.0 && region.2 <= 22.01 && region.2 > region.1);
    }
    assert!(actual[0].3 < actual[1].3 && actual[1].3 < actual[2].3);
    assert_eq!(actual[2].3, actual[3].3);
    assert!(actual[3].3 < actual[4].3);
    assert_eq!(actual[4].3, actual[5].3);
    assert!(actual[5].3 < actual[6].3);
}

#[test]
fn discretionary_fragments_clip_navigation_after_source_hint_normalization() {
    let mut p = poster();
    let parts = ["a", "b\u{200b}", "f\u{00ad}", "i"];
    let lines = p.wrap(&linked(&p, &parts), 10.0, 9.6);
    let text: Vec<String> = lines
        .iter()
        .map(|line| line.iter().map(|run| run.text.as_str()).collect())
        .collect();
    assert_eq!(text, ["ab", "fi"]);
    for (index, line) in lines.iter().enumerate() {
        p.draw_words(line, 17.0, 30.0 + 30.0 * index as f64, 10.0);
    }
    let actual_glyphs = glyphs(&p);
    assert_eq!(actual_glyphs.len(), 3);
    assert_eq!(actual_glyphs[2].1, 6);
    let actual = regions(p);
    assert_eq!(actual.len(), parts.len());
    for (index, region) in actual.iter().enumerate() {
        assert_eq!(region.0, format!("https://example.com/{index}"));
    }
    assert_eq!(actual[0].3, actual[1].3);
    assert_eq!(actual[2].3, actual[3].3);
    assert!(actual[1].3 < actual[2].3);
}

#[test]
fn selected_hyphen_keeps_its_own_source_link_after_joint_shaping() {
    let mut p = Poster::new(&SvgOptions::default());
    let width = Shaper::new(&p).shape("WWWW-", RStyle::BODY, 10.0).width() + 0.01;
    let parts = ["WWWW", "\u{00ad}", "WWWW"];
    let lines = p.wrap(&linked(&p, &parts), 10.0, width);
    let text: Vec<String> = lines
        .iter()
        .map(|line| line.iter().map(|run| run.text.as_str()).collect())
        .collect();
    assert_eq!(text, ["WWWW-", "WWWW"]);
    for (index, line) in lines.iter().enumerate() {
        p.draw_words(line, 17.0, 30.0 + 30.0 * index as f64, 10.0);
    }
    let actual = regions(p);
    assert_eq!(actual.len(), 3);
    for (index, region) in actual.iter().enumerate() {
        assert_eq!(region.0, format!("https://example.com/{index}"));
        assert!(region.2 > region.1);
    }
    assert!((actual[0].2 - actual[1].1).abs() <= 0.011);
    assert_eq!(actual[0].3, actual[1].3);
    assert!(actual[1].3 < actual[2].3);
}

#[test]
fn real_paint_and_font_boundaries_still_prevent_cross_style_ligatures() {
    for style in [
        RStyle {
            bold: true,
            ..RStyle::BODY
        },
        RStyle {
            italic: true,
            ..RStyle::BODY
        },
        RStyle {
            mono: true,
            ..RStyle::BODY
        },
        RStyle {
            strike: true,
            ..RStyle::BODY
        },
        RStyle {
            ink: Ink::Accent,
            ..RStyle::BODY
        },
    ] {
        let mut p = poster();
        let mut pieces = linked(&p, &["f", "i"]);
        if let Piece::Text(_, second) = &mut pieces[1] {
            *second = RStyle {
                link: second.link,
                ..style
            };
        }
        let lines = p.wrap(&pieces, 10.0, 100.0);
        close(p.words_width(&lines[0], 10.0), 10.0);
        p.draw_words(&lines[0], 17.0, 30.0, 10.0);
        assert_eq!(
            glyphs(&p).iter().map(|glyph| glyph.1).collect::<Vec<_>>(),
            [4, 5]
        );
        assert_eq!(regions(p).len(), 2);
    }
}
