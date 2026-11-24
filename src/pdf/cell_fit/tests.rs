#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::PdfOptions;
use crate::pdf::{
    F_BODY, F_BOLD, F_IMAGE, F_MATH, Tok, build_cell_line_owned, note_tags, wrap_cell_styled,
    wrap_cell_styled_unfitted,
};
use crate::theme::FontFamily;

fn faces() -> Faces {
    let mut options = PdfOptions::default();
    options.theme.font = FontFamily::Serif;
    Faces::load(&options).unwrap()
}

fn token(source: &str) -> Tok {
    Tok {
        text: source.to_owned(),
        slot: F_BODY,
        space: false,
        hard_break: false,
        literal: false,
        list_label: false,
        note_spans: note_tags::Spans::default(),
        link: None,
        strike: false,
    }
}

fn source(lines: &[CellWrapLine]) -> String {
    lines
        .iter()
        .flat_map(|line| &line.runs)
        .map(|run| run.text.as_str())
        .collect()
}

#[test]
fn ordinary_long_serif_words_fit_their_complete_shaped_width() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    let scalar = text_width_cached("o", 10.0, F_BODY, &faces, &widths);
    let shaped = text_width_cached("oooo", 10.0, F_BODY, &faces, &widths);
    assert!(
        shaped > 4.0 * scalar + 0.05,
        "fixture has positive oo kerning"
    );
    let measure = (4.0 * scalar + shaped) * 0.5;
    let original = "o".repeat(37);
    let tokens = [token(&original)];
    let first_pass = wrap_cell_styled_unfitted(&tokens, measure, 10.0, &faces, &widths);
    assert!(first_pass.iter().all(|line| line.width <= measure));
    assert_eq!(
        first_pass.len(),
        13,
        "three shaped glyphs fit on each full row"
    );
    let actual = wrap_cell_styled(&tokens, measure, 10.0, &faces, &widths);
    assert_eq!(source(&actual), original);
    assert!(actual.iter().all(|line| line.width <= measure + 0.01));
    assert!(
        actual
            .iter()
            .flat_map(|line| &line.runs)
            .all(|run| run.slot == F_BODY)
    );
}

#[test]
fn joined_tokens_keep_their_links_labels_and_note_byte_ranges() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    let pair = text_width_cached("oo", 10.0, F_BODY, &faces, &widths);
    let joined = text_width_cached("oooo", 10.0, F_BODY, &faces, &widths);
    assert!(joined > pair * 2.0 + 0.05);
    let measure = (pair * 2.0 + joined) * 0.5;
    let tokens = [token("oo"), token("oo")].map(|mut tok| {
        tok.link = Some(7);
        tok.strike = true;
        tok.list_label = true;
        tok.note_spans = note_tags::Spans::whole(&tok.text, note_tags::Mark::Label(9));
        tok
    });
    let approximate = wrap_cell_styled_unfitted(&tokens, measure, 10.0, &faces, &widths);
    assert_eq!(approximate.len(), 1, "the token estimate fits one row");
    assert!(
        approximate[0].width > measure + 0.01,
        "merged run does not fit"
    );
    let actual = wrap_cell_styled(&tokens, measure, 10.0, &faces, &widths);
    assert_eq!(source(&actual), "oooo");
    assert!(actual.len() > 1);
    for line in &actual {
        assert!(line.width <= measure + 0.01);
        for run in &line.runs {
            assert_eq!(run.link, Some(7));
            assert!(run.strike && run.list_label);
            for byte in 0..run.text.len() {
                assert_eq!(run.note_spans.at(byte), Some(note_tags::Mark::Label(9)));
            }
        }
    }
}

#[test]
fn repair_preserves_atomic_combining_clusters_and_inline_objects() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    let mut bold = token("o\u{0301}o\u{0301}");
    bold.slot = F_BOLD;
    let mut image = token("8x8\u{1f}asset\u{1f}An image");
    image.slot = F_IMAGE;
    image.link = Some(11);
    let mut formula = token("x");
    formula.slot = F_MATH;
    let tokens = [token("oooo"), image, bold, formula];
    let expected: String = tokens.iter().map(|tok| tok.text.as_str()).collect();
    let row = build_cell_line_owned(tokens, 10.0, &faces, &widths);
    let actual = repair(vec![row], 7.0, 10.0, &faces, &widths);
    assert_eq!(source(&actual), expected);
    let runs: Vec<_> = actual.iter().flat_map(|line| &line.runs).collect();
    let images: Vec<_> = runs.iter().filter(|run| run.slot == F_IMAGE).collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].text, "8x8\u{1f}asset\u{1f}An image");
    assert_eq!(images[0].link, Some(11));
    assert_eq!(runs.iter().filter(|run| run.slot == F_MATH).count(), 1);
    for run in runs.iter().filter(|run| run.slot == F_BOLD) {
        assert_eq!(run.text, "o\u{0301}");
    }
    assert!(actual.iter().all(|line| line.width <= 7.01));
}

#[test]
fn an_indivisible_wide_cluster_keeps_all_source_and_makes_progress() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    let original = "o\u{0301}o";
    let row = build_cell_line_owned([token(original)], 10.0, &faces, &widths);
    let actual = repair(vec![row], 0.1, 10.0, &faces, &widths);
    assert_eq!(actual.len(), 2);
    assert_eq!(source(&actual), original);
    assert_eq!(actual[0].runs[0].text, "o\u{0301}");
    assert_eq!(actual[1].runs[0].text, "o");
}

#[test]
fn a_terminal_discretionary_hyphen_stays_with_the_preceding_cluster() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    let prefix = text_width_cached("oooo", 10.0, F_BODY, &faces, &widths);
    let hyphen = text_width_cached("-", 10.0, F_BODY, &faces, &widths);
    let measure = prefix + hyphen * 0.5;
    let row = build_cell_line_owned([token("oooo"), token("-")], 10.0, &faces, &widths);
    assert!(row.width > measure);
    let actual = repair(vec![row], measure, 10.0, &faces, &widths);
    assert_eq!(source(&actual), "oooo-");
    assert!(actual.iter().all(|line| line.width <= measure));
    assert_eq!(source(&actual[actual.len() - 1..]), "o-");
}

#[test]
fn fitting_rows_and_unbounded_measurement_keep_their_allocations() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    for measure in [100.0, f32::MAX] {
        let row = build_cell_line_owned([token("ordinary office")], 10.0, &faces, &widths);
        let before = row.runs[0].text.as_ptr();
        let original_width = row.width;
        let actual = repair(vec![row], measure, 10.0, &faces, &widths);
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].runs[0].text.as_ptr(), before);
        assert_eq!(actual[0].width.to_bits(), original_width.to_bits());
    }
}

#[test]
fn overlong_literal_tokens_keep_whitespace_styles_and_notes_on_cluster_boundaries() {
    let faces = faces();
    let widths = RefCell::new(WidthCache::default());
    let mut tok = token("aa  xx\u{0301}xxxxxxxxxxxxxxxxxxxxx");
    tok.slot = super::super::F_MONO;
    tok.literal = true;
    tok.link = Some(9);
    tok.strike = true;
    tok.note_spans = note_tags::Spans::whole(&tok.text, note_tags::Mark::Label(4));
    let measure = text_width_cached("aa  xx", 10.0, tok.slot, &faces, &widths) + 0.01;
    let original = tok.text.clone();
    let lines = wrap_cell_styled(&[tok], measure, 10.0, &faces, &widths);
    assert_eq!(source(&lines), original);
    assert_eq!(lines[0].runs[0].text, "aa  ");
    assert!(lines.iter().all(|line| line.width <= measure + 0.01));
    for run in lines.iter().flat_map(|line| &line.runs) {
        assert_ne!(run.text.chars().next(), Some('\u{0301}'));
        assert_eq!(run.link, Some(9));
        assert!(run.strike);
        assert!(
            (0..run.text.len())
                .all(|byte| run.note_spans.at(byte) == Some(note_tags::Mark::Label(4)))
        );
    }
}
