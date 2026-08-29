//! River-seed demerits — tests for the opt-in river penalty in the
//! Knuth-Plass line breaker.
//!
//! Contract: disabled is byte-identical to the classic path; enabled never
//! increases the number of detected river seeds across a fixed corpus
//! (sometimes decreases it); both paths are deterministic; the PDF surface
//! keeps default output unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    AdvanceMetrics, FontSize, LayoutUnit, LineBreak, PairMetrics, ParagraphItem,
    ParagraphLayoutScratch, break_paragraph_into, paragraph_items_from_text,
};
use franken_markdown::{PdfOptions, parse_markdown, render_pdf_document};

/// Flat deterministic metrics: every glyph 500/1000 em, no pair kerning.
struct FlatMetrics;
impl AdvanceMetrics for FlatMetrics {
    fn advance_1000(&self, _ch: char) -> u32 {
        500
    }
}
impl PairMetrics for FlatMetrics {}

fn breaks_with(items: &[ParagraphItem], width: LayoutUnit, river: bool) -> Vec<LineBreak> {
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_river_penalty(river);
    let mut out = Vec::new();
    break_paragraph_into(items, width, &mut scratch, &mut out);
    out
}

/// Count two-line river seeds in a chosen break sequence, mirroring the
/// breaker's detection: previous line's LAST drawn space x vs every space x
/// of the following line (both natural widths from the shared left margin),
/// aligned within 1% of the measure.
fn seed_count(items: &[ParagraphItem], breaks: &[LineBreak], measure: LayoutUnit) -> usize {
    let item_w = |i: usize| -> i64 {
        items
            .get(i)
            .map(|it| it.width().milli_points() as i64)
            .unwrap_or(0)
    };
    let space_x = |line_start: usize, g: usize| -> Option<i64> {
        let mut x = 0i64;
        for i in line_start..g {
            x += item_w(i);
        }
        Some(x)
    };
    let tolerance = measure.milli_points() as i64 / 100;
    let mut seeds = 0usize;
    for pair in breaks.windows(2) {
        let (prev, cur) = (&pair[0], &pair[1]);
        // Last drawn glue of the previous line.
        let mut x_prev: Option<i64> = None;
        for (offset, item) in items[prev.start..prev.end].iter().enumerate().rev() {
            let g = prev.start + offset;
            if let ParagraphItem::Glue(glue) = item
                && glue.width > LayoutUnit::ZERO
            {
                x_prev = space_x(prev.start, g);
                break;
            }
        }
        let Some(x_prev) = x_prev else { continue };
        for (offset, item) in items[cur.start..cur.end].iter().enumerate() {
            let g = cur.start + offset;
            if let ParagraphItem::Glue(glue) = item
                && glue.width > LayoutUnit::ZERO
                && let Some(x) = space_x(cur.start, g)
                && (x_prev - x).abs() <= tolerance
            {
                seeds += 1;
                break;
            }
        }
    }
    seeds
}

/// Fixed corpus: varied word lengths so line compositions differ and space
/// positions are not all on one grid.
const CORPUS: &[&str] = &[
    "the quick brn fox jmps over a lazy hnd again and again until the measure fills",
    "pack my box with five dozen liquor jugs vexingly quick daft zebras jump my vow",
    "sphinx of black quartz judge my vow while vexing daft zebras in the night sky",
    "typography is the art and technique of arranging type to make text readable",
    "rivers of whitespace flow down justified columns when spaces align badly",
];

#[test]
fn river_off_is_classic_and_deterministic() {
    for (n, text) in CORPUS.iter().enumerate() {
        let items = paragraph_items_from_text(&FlatMetrics, text, FontSize::from_points(11));
        let w = LayoutUnit::from_points(150);
        let a = breaks_with(&items, w, false);
        let b = franken_markdown::layout::break_paragraph(&items, w);
        assert_eq!(a, b, "corpus {n}: river-off must equal classic");
        let c = breaks_with(&items, w, false);
        assert_eq!(a, c, "corpus {n}: deterministic");
    }
}

#[test]
fn river_on_never_increases_seeds() {
    for (n, text) in CORPUS.iter().enumerate() {
        let items = paragraph_items_from_text(&FlatMetrics, text, FontSize::from_points(11));
        for &measure_pts in &[120i32, 140, 170, 200] {
            let w = LayoutUnit::from_points(measure_pts);
            let classic = breaks_with(&items, w, false);
            let river = breaks_with(&items, w, true);
            let s_classic = seed_count(&items, &classic, w);
            let s_river = seed_count(&items, &river, w);
            assert!(
                s_river <= s_classic,
                "corpus {n} measure {measure_pts}: river-on seeds {s_river} \
                 must not exceed classic {s_classic}"
            );
            // Badness sanity: the anti-river choice must not wreck line fits.
            let max_badness = |bs: &[LineBreak]| bs.iter().map(|b| b.badness).max().unwrap_or(0);
            assert!(
                max_badness(&river) <= max_badness(&classic).max(3000),
                "corpus {n} measure {measure_pts}: river-on keeps lines sane"
            );
        }
    }
}

#[test]
fn river_penalty_changes_breaks_somewhere_in_corpus() {
    // The penalty must be live: on at least one corpus/measure combination
    // the chosen path differs from classic. (Deterministic inputs, so this
    // is a stable property, not a flaky one.)
    let mut changed = 0;
    for text in CORPUS {
        let items = paragraph_items_from_text(&FlatMetrics, text, FontSize::from_points(11));
        for &measure_pts in &[120i32, 140, 170, 200] {
            let w = LayoutUnit::from_points(measure_pts);
            if breaks_with(&items, w, false) != breaks_with(&items, w, true) {
                changed += 1;
            }
        }
    }
    assert!(
        changed > 0,
        "river penalty should influence at least one fixture (changed={changed})"
    );
}

#[test]
fn pdf_river_default_off_identical_and_optin_deterministic() {
    let doc = parse_markdown(concat!(
        "# Rivers\n\n",
        "Rivers of whitespace flow down justified columns when inter-word spaces ",
        "align between consecutive lines; the breaker can now see the seed of one ",
        "and prefer a different break. Typography is the art and technique of ",
        "arranging type to make text readable and beautiful when set at a fixed ",
        "measure with hyphenation and justification both enabled.\n"
    ));
    let default_a = render_pdf_document(&doc, &PdfOptions::default()).expect("default a");
    let default_b = render_pdf_document(&doc, &PdfOptions::default()).expect("default b");
    assert_eq!(default_a, default_b, "default deterministic");
    let on_a = render_pdf_document(
        &doc,
        &PdfOptions {
            river_penalty: true,
            ..PdfOptions::default()
        },
    )
    .expect("on a");
    let on_b = render_pdf_document(
        &doc,
        &PdfOptions {
            river_penalty: true,
            ..PdfOptions::default()
        },
    )
    .expect("on b");
    assert_eq!(on_a, on_b, "river-on deterministic");
    assert!(on_a.starts_with(b"%PDF-"), "still a valid PDF");
}
