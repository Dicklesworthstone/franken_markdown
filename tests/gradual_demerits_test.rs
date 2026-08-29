//! Gradual adjacent demerits (Verna DocEng '25) — opt-in refinement tests.
//!
//! Pin: default (off) is deterministic; opt-in (on) is deterministic and
//! valid; the fitness_milli field carries meaningful signed values.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::AdvanceMetrics;
use franken_markdown::layout::PairMetrics;
use franken_markdown::layout::{FontSize, LayoutUnit, break_paragraph, paragraph_items_from_text};
use franken_markdown::{PdfOptions, parse_markdown, render_pdf_document};

struct FlatMetrics;
impl AdvanceMetrics for FlatMetrics {
    fn advance_1000(&self, _ch: char) -> u32 {
        500
    }
}
impl PairMetrics for FlatMetrics {}

fn arms(breaks: &[franken_markdown::layout::LineBreak]) -> f64 {
    if breaks.len() < 2 {
        return 0.0;
    }
    let diffs: Vec<f64> = breaks
        .windows(2)
        .map(|w| {
            let a = w[0].fitness_milli as f64 / 1000.0;
            let b = w[1].fitness_milli as f64 / 1000.0;
            (a - b).powi(2)
        })
        .collect();
    (diffs.iter().sum::<f64>() / diffs.len() as f64).sqrt()
}

#[test]
fn default_breaks_are_deterministic_with_fitness_milli() {
    let text = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. How vexingly quick daft zebras jump! Sphinx of black quartz judge my vow.";
    let size = FontSize::from_points(11);
    let items = paragraph_items_from_text(&FlatMetrics, text, size);
    let a = break_paragraph(&items, LayoutUnit::from_points(180));
    let b = break_paragraph(&items, LayoutUnit::from_points(180));
    assert_eq!(a, b, "deterministic");
    assert!(
        a.iter()
            .all(|lb| (-1000..=1000).contains(&lb.fitness_milli)),
        "fitness_milli clamped to [-1000, 1000]"
    );
}

#[test]
fn fixture_produces_nonzero_arms() {
    // The paragraph from Verna's paper — known to have spacing inhomogeneity.
    let text = "In olden times when wishing still helped one, there lived a king whose daughters were all beautiful; and the youngest was so beautiful that the sun itself, which has seen so much, was astonished whenever it shone in her face.";
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&FlatMetrics, text, size);
    let breaks = break_paragraph(&items, LayoutUnit::from_points(140));
    assert!(
        breaks.len() >= 4,
        "needs multiple lines, got {}",
        breaks.len()
    );
    let score = arms(&breaks);
    assert!(score > 0.0, "fixture must have variation (ARMS={score})");
}

#[test]
fn pdf_default_off_is_deterministic() {
    let doc = parse_markdown(
        "# Title\n\nA paragraph with enough text to produce multiple lines when justified at the default measure. The quick brown fox jumps over the lazy dog and continues running through the forest. Pack my box with five dozen liquor jugs.\n",
    );
    let a = render_pdf_document(&doc, &PdfOptions::default()).expect("a");
    let b = render_pdf_document(&doc, &PdfOptions::default()).expect("b");
    assert_eq!(a, b);
    assert!(a.starts_with(b"%PDF-"));
}

#[test]
fn pdf_gradual_on_is_deterministic_and_valid() {
    let doc = parse_markdown(
        "# Title\n\nA paragraph with enough text to produce multiple lines when justified at the default measure. The quick brown fox jumps over the lazy dog and continues running through the forest. Pack my box with five dozen liquor jugs.\n",
    );
    let a = render_pdf_document(
        &doc,
        &PdfOptions {
            gradual_demerits: true,
            ..PdfOptions::default()
        },
    )
    .expect("a");
    let b = render_pdf_document(
        &doc,
        &PdfOptions {
            gradual_demerits: true,
            ..PdfOptions::default()
        },
    )
    .expect("b");
    assert_eq!(a, b, "gradual on must be deterministic");
    assert!(a.starts_with(b"%PDF-"), "still valid PDF");
    let pages = a
        .windows(b"/Type /Page ".len())
        .filter(|w| *w == b"/Type /Page ")
        .count();
    assert!(pages >= 1);
}
