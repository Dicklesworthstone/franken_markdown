//! Multi-objective (Pareto) line breaking — opt-in refinement tests.
//!
//! Contract: disabled is byte-identical to the classic path; enabled is
//! deterministic; the scalar total of the Pareto-chosen path never exceeds
//! the classic scalar total (the front search is a superset of the classic
//! search, and the final pick is min-scalar); the machinery is live (breaks
//! differ on at least one corpus fixture).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    AdvanceMetrics, FontSize, LayoutUnit, LineBreak, PairMetrics, ParagraphItem,
    ParagraphLayoutScratch, break_paragraph, break_paragraph_into, paragraph_items_from_text,
};
use franken_markdown::{PdfOptions, parse_markdown, render_pdf_document};

struct FlatMetrics;
impl AdvanceMetrics for FlatMetrics {
    fn advance_1000(&self, _ch: char) -> u32 {
        500
    }
}
impl PairMetrics for FlatMetrics {}

fn breaks_pareto(
    items: &[franken_markdown::layout::ParagraphItem],
    w: LayoutUnit,
) -> Vec<LineBreak> {
    let mut scratch = ParagraphLayoutScratch::new();
    scratch.set_pareto_breaking(true);
    let mut out = Vec::new();
    break_paragraph_into(items, w, &mut scratch, &mut out);
    out
}

fn scalar_sum(breaks: &[LineBreak]) -> i64 {
    breaks.iter().map(|b| b.demerits).sum()
}

/// Long-word corpus: hyphen penalties, flagged breaks, and fitness shifts all
/// activate, giving the front search real alternatives to keep.
const CORPUS: &[&str] = &[
    "extraordinarily incomprehensible misinterpretations decontaminating reconstructing",
    "internationalization counterrevolutionaries disproportionateness unfathomableness",
    "the thorough oxymoron quizzed the phenomenological zeitgeist examining skepticism",
    "deterministic incremental hyphenation algorithms balance typography and breaks",
];

#[test]
fn pareto_off_is_classic_and_deterministic() {
    for (n, text) in CORPUS.iter().enumerate() {
        let items = paragraph_items_from_text(&FlatMetrics, text, FontSize::from_points(11));
        let w = LayoutUnit::from_points(150);
        let mut scratch = ParagraphLayoutScratch::new();
        let mut out = Vec::new();
        break_paragraph_into(&items, w, &mut scratch, &mut out);
        assert_eq!(out, break_paragraph(&items, w), "corpus {n}: off = classic");
        let mut again = Vec::new();
        break_paragraph_into(&items, w, &mut scratch, &mut again);
        assert_eq!(out, again, "corpus {n}: deterministic");
    }
}

#[test]
fn pareto_on_is_deterministic_and_never_worse_scalar() {
    for (n, text) in CORPUS.iter().enumerate() {
        let items = paragraph_items_from_text(&FlatMetrics, text, FontSize::from_points(11));
        for &measure in &[110i32, 140, 170, 200] {
            let w = LayoutUnit::from_points(measure);
            let classic = break_paragraph(&items, w);
            let pareto = breaks_pareto(&items, w);
            assert!(
                !pareto.is_empty(),
                "corpus {n} measure {measure}: pareto path produced breaks"
            );
            assert!(
                scalar_sum(&pareto) <= scalar_sum(&classic),
                "corpus {n} measure {measure}: pareto scalar {} must not exceed classic {}",
                scalar_sum(&pareto),
                scalar_sum(&classic)
            );
            let pareto_again = breaks_pareto(&items, w);
            assert_eq!(pareto, pareto_again, "corpus {n}: deterministic");
        }
    }
}
/// Hand-built fixtures with REAL hyphenation structure: flagged reward
/// penalties at word ends, so the hyphen dimension actually varies and the
/// front search has alternatives the scalar path prunes away. Brute-forcing
/// word widths × rewards × measures gives a deterministic sweep; the pins are
/// (a) scalar never worse anywhere, (b) breaks differ somewhere.
fn hyphen_items(
    word_widths: &[i64],
    rewards: &[bool],
) -> Vec<franken_markdown::layout::ParagraphItem> {
    use franken_markdown::layout::{Glue, Penalty, Protrusion, TextBox};
    let mut items = Vec::new();
    for (i, &w) in word_widths.iter().enumerate() {
        items.push(ParagraphItem::Box(TextBox {
            text: format!("w{i}"),
            runs: franken_markdown::layout::StyledText::default(),
            width: LayoutUnit::from_milli_points(w as i32),
            protrusion: Protrusion::default(),
        }));
        if rewards.get(i).copied().unwrap_or(false) {
            // Flagged hyphen-point reward after the box.
            items.push(ParagraphItem::Penalty(Penalty {
                penalty: -80,
                flagged: true,
                width: LayoutUnit::from_milli_points(300),
            }));
        }
        if i + 1 < word_widths.len() {
            items.push(ParagraphItem::Glue(Glue {
                width: LayoutUnit::from_milli_points(250),
                stretch: LayoutUnit::from_milli_points(900),
                shrink: LayoutUnit::from_milli_points(600),
            }));
        }
    }
    items.push(ParagraphItem::Penalty(Penalty {
        penalty: franken_markdown::layout::FORCED_BREAK_PENALTY,
        flagged: false,
        width: LayoutUnit::ZERO,
    }));
    items
}

#[test]
fn pareto_never_worse_and_live_on_hyphen_fixtures() {
    let mut checked = 0;
    let mut changed = 0;
    let word_sets: &[&[i64]] = &[
        &[3000i64, 5000, 4000],
        &[4000, 3000, 5000, 3000],
        &[5000, 2000, 6000, 2000, 4000],
        &[2000, 2500, 3000, 2000, 2500, 3000, 2000],
        &[1500, 3000, 2500, 2000, 3500, 2000, 2500, 3000],
    ];
    for words in word_sets {
        for reward_mask in 0..(1 << words.len().min(8)) {
            let rewards: Vec<bool> = (0..words.len())
                .map(|i| reward_mask & (1 << i.min(7)) != 0)
                .collect();
            let items = hyphen_items(words, &rewards);
            for measure in [4000i32, 5000, 6000, 7000, 8000, 9000, 10000] {
                let w = LayoutUnit::from_milli_points(measure);
                let classic = break_paragraph(&items, w);
                let pareto = breaks_pareto(&items, w);
                assert!(!pareto.is_empty());
                assert!(
                    scalar_sum(&pareto) <= scalar_sum(&classic),
                    "pareto scalar {} > classic {} (words={words:?} rewards={rewards:?} m={measure})",
                    scalar_sum(&pareto),
                    scalar_sum(&classic)
                );
                if classic != pareto {
                    changed += 1;
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "sweep ran ({checked} fixtures)");
    assert!(changed > 0, "pareto live on {changed}/{checked} fixtures");
}

#[test]
fn pdf_pareto_default_off_and_optin_deterministic() {
    let doc = parse_markdown(concat!(
        "# Pareto\n\n",
        "Extraordinarily incomprehensible misinterpretations decontaminating ",
        "reconstructing the thorough oxymoron quizzed the phenomenological ",
        "zeitgeist while deterministic incremental hyphenation algorithms ",
        "balance typography against the stubborn arithmetic of line breaks.\n\n",
        "## Second\n\n",
        "A second paragraph exercises the production scratch-reuse path: the ",
        "same workspace laid out the first paragraph, so any stale front state ",
        "would corrupt this one's line breaking. Thorough oxymoron quizzed the ",
        "phenomenological zeitgeist examining skepticism once more.\n"
    ));
    let a = render_pdf_document(&doc, &PdfOptions::default()).expect("a");
    let b = render_pdf_document(&doc, &PdfOptions::default()).expect("b");
    assert_eq!(a, b, "default deterministic");
    let opts = || PdfOptions {
        pareto_line_breaking: true,
        ..PdfOptions::default()
    };
    let c = render_pdf_document(&doc, &opts()).expect("c");
    let d = render_pdf_document(&doc, &opts()).expect("d");
    assert_eq!(c, d, "opt-in deterministic");
    assert!(c.starts_with(b"%PDF-"), "still a valid PDF");
}

#[test]
fn pareto_with_reused_scratch_matches_fresh_scratch_per_paragraph() {
    // Production reuses ONE ParagraphLayoutScratch across every paragraph in
    // a document. A regression here (fronts not cleared per paragraph) made
    // paragraph N+1 read paragraph N's fronts — wrong items, wrong classes,
    // corrupt reconstruction. Fresh-scratch and reused-scratch results must
    // agree for every paragraph, and both must stay scalar-bounded by the
    // classic path run on the same scratch discipline.
    let docs: Vec<Vec<&str>> = vec![
        vec![
            "Extraordinarily incomprehensible misinterpretations decontaminating reconstructing typography.",
            "The thorough oxymoron quizzed the phenomenological zeitgeist examining skepticism again.",
            "Deterministic incremental hyphenation algorithms balance stubborn arithmetic of breaks.",
        ],
        vec!["First paragraph alone with a few words to break across the measure."],
        vec![
            "Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike.",
            "November oscar papa quebec romeo sierra tango uniform victor whiskey xray yankee zulu.",
        ],
    ];
    for paragraphs in &docs {
        let mut reused = ParagraphLayoutScratch::new();
        reused.set_pareto_breaking(true);
        for text in paragraphs {
            let items = paragraph_items_from_text(&FlatMetrics, text, FontSize::from_points(11));
            let w = LayoutUnit::from_points(150);
            let mut out_reused = Vec::new();
            break_paragraph_into(&items, w, &mut reused, &mut out_reused);
            let out_fresh = breaks_pareto(&items, w);
            assert_eq!(
                out_reused, out_fresh,
                "reused scratch diverged from fresh scratch for: {text}"
            );
            let classic = break_paragraph(&items, w);
            assert!(
                scalar_sum(&out_reused) <= scalar_sum(&classic),
                "reused-scratch pareto scalar {} exceeded classic {}",
                scalar_sum(&out_reused),
                scalar_sum(&classic)
            );
        }
    }
}
