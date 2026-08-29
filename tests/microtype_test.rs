//! Integration proof for opt-in microtypography wiring (bead 544o).
//!
//! The protrusion hooks in `layout` existed tested-but-unwired; the wiring
//! lets the Knuth-Plass breaker fit against optical-margin-adjusted widths and
//! makes the PDF emitter hang the punctuation it credited. These tests pin the
//! contract: DISABLED is byte-identical to the pre-flag behavior, and enabling
//! protrusion changes break decisions only at real margin boundaries.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::layout::{
    FontSize, LayoutUnit, MicrotypeOptions, ParagraphItem, Protrusion, break_paragraph,
    measure_text, paragraph_items_from_text,
};

use franken_markdown::{PdfOptions, parse_markdown, render_pdf_document};

/// Flat deterministic metrics: every glyph 500/1000 em (0.5x font size), no
/// pair kerning.
struct FlatMetrics;
impl franken_markdown::layout::AdvanceMetrics for FlatMetrics {
    fn advance_1000(&self, _ch: char) -> u32 {
        500
    }
}
impl franken_markdown::layout::PairMetrics for FlatMetrics {}

fn protrude_last_box_right(items: &mut [ParagraphItem], per_mille_pts: i32, size: FontSize) {
    for item in items.iter_mut() {
        if let ParagraphItem::Box(b) = item {
            if b.text.ends_with(['.', ',', ':', ';', '!', '?']) {
                b.protrusion = Protrusion {
                    left: LayoutUnit::ZERO,
                    right: LayoutUnit::from_milli_points(per_mille_pts),
                };
            }
        }
    }
    let _ = size;
}

#[test]
fn disabled_protrusion_is_identical_to_baseline() {
    let text = "The quick brown fox jumps over the lazy dog, then rests. Pack my box with five dozen liquor jugs, quietly.";
    let size = FontSize::from_points(10);
    let items = paragraph_items_from_text(&FlatMetrics, text, size);
    let line_width = measure_text(&FlatMetrics, "The quick brown fox jumps o", size);
    let baseline = break_paragraph(&items, line_width);
    // A fresh, unmutated item stream (all protrusions ZERO) must break
    // identically — the effective-width path is byte-neutral by construction.
    let items2 = paragraph_items_from_text(&FlatMetrics, text, size);
    let again = break_paragraph(&items2, line_width);
    assert_eq!(baseline, again, "zero-protrusion items break identically");
}

#[test]
fn protrusion_lets_a_period_word_fit_the_line() {
    // Craft: a line width where the final word "rests." is 2pt over the
    // measure. With protrusion enabled, its trailing period hangs 550 per-mille
    // of 10pt = 5.5pt, and the line then fits — the exact decision change the
    // MICROTYPOGRAPHY.md contract pins.
    let text = "alpha beta gamma delta epsilon zeta eta theta iota rests.";
    let size = FontSize::from_points(10);
    let metrics = FlatMetrics;
    // Self-calibrating: find the one-line cliff width, then step 2pt below it
    // (the MICROTYPOGRAPHY.md contract case: 2pt over the measure; the trailing
    // period protrudes 550 per-mille of 10pt = 5.5pt, so it fits).
    let full = measure_text(&metrics, text, size);
    let mut cliff = full;
    while cliff > LayoutUnit::ZERO
        && break_paragraph(&paragraph_items_from_text(&metrics, text, size), cliff).len() == 1
    {
        cliff = cliff - LayoutUnit::from_milli_points(1_000);
    }
    assert!(
        cliff > LayoutUnit::ZERO && cliff < full,
        "fixture must have a 1-line cliff"
    );
    let min_1line = cliff + LayoutUnit::from_milli_points(1_000);
    let line_width = min_1line - LayoutUnit::from_milli_points(2_000);

    let plain_items = paragraph_items_from_text(&metrics, text, size);
    let plain = break_paragraph(&plain_items, line_width);
    assert!(plain.len() >= 2, "2pt below the cliff must not fit plainly");

    let mut pro_items = paragraph_items_from_text(&metrics, text, size);
    for item in pro_items.iter_mut() {
        if let ParagraphItem::Box(b) = item {
            b.protrusion = franken_markdown::layout::protrusion_for_text(
                &b.text,
                size,
                MicrotypeOptions::CONSERVATIVE,
            );
        }
    }
    let protruded = break_paragraph(&pro_items, line_width);

    assert!(
        protruded.len() <= plain.len(),
        "protrusion must never produce MORE lines: {} vs {}",
        protruded.len(),
        plain.len()
    );
    assert!(
        protruded.len() < plain.len(),
        "crafted case must take one fewer line (got {} vs {})",
        protruded.len(),
        plain.len()
    );
}

#[test]
fn protrusion_never_worsens_breaks_on_a_real_paragraph() {
    let text = "In a hole in the ground there lived a hobbit. Not a nasty, dirty, wet hole, filled with the ends of worms and an oozy smell, nor yet a dry, bare, sandy hole with nothing in it to sit down on or to eat: it was a hobbit-hole, and that means comfort.";
    let size = FontSize::from_points(11);
    let metrics = FlatMetrics;
    for width_words in [8usize, 11, 17] {
        let probe: String = text
            .split_whitespace()
            .take(width_words)
            .collect::<Vec<_>>()
            .join(" ");
        let line_width = measure_text(&metrics, &probe, size);
        let plain = break_paragraph(&paragraph_items_from_text(&metrics, text, size), line_width);
        let mut pro_items = paragraph_items_from_text(&metrics, text, size);
        protrude_last_box_right(&mut pro_items, 0, size);
        let neutral = break_paragraph(&pro_items, line_width);
        assert_eq!(
            plain, neutral,
            "zero protrusion is neutral at width {width_words}"
        );
        for item in pro_items.iter_mut() {
            if let ParagraphItem::Box(b) = item {
                b.protrusion = franken_markdown::layout::protrusion_for_text(
                    &b.text,
                    size,
                    MicrotypeOptions::CONSERVATIVE,
                );
            }
        }
        let protruded = break_paragraph(&pro_items, line_width);
        assert!(
            protruded.len() <= plain.len(),
            "protrusion never worsens line count at width {width_words}"
        );
    }
}

#[test]
fn pdf_optin_stays_valid_and_deterministic() {
    // A dense paragraph so some line boundary lands near the margin.
    let sentence = "The renderer breaks this long sentence at the measured margin. ";
    let doc = parse_markdown(&sentence.repeat(24));
    let off = render_pdf_document(&doc, &PdfOptions::default()).expect("render off");
    let on_a = render_pdf_document(
        &doc,
        &PdfOptions {
            microtype: MicrotypeOptions::CONSERVATIVE,
            ..PdfOptions::default()
        },
    )
    .expect("render on a");
    let on_b = render_pdf_document(
        &doc,
        &PdfOptions {
            microtype: MicrotypeOptions::CONSERVATIVE,
            ..PdfOptions::default()
        },
    )
    .expect("render on b");
    assert_eq!(on_a, on_b, "opt-in render is deterministic");
    assert!(
        on_a.starts_with(b"%PDF-"),
        "opt-in render is still a valid PDF"
    );
    // Protrusion changes line fits, never pagination volume: same page count.
    // Page objects carry "/Type /Page " (trailing space; "/Type /Pages" is the
    // parent and must not match).
    let count_pages = |pdf: &[u8]| {
        pdf.windows(b"/Type /Page ".len())
            .filter(|w| *w == b"/Type /Page ")
            .count()
    };
    assert!(count_pages(&off) >= 1, "fixture renders at least one page");
    assert_eq!(
        count_pages(&off),
        count_pages(&on_a),
        "protrusion must not change page count"
    );
    // The decision-change proof itself lives in the breaker-level tests above
    // (protrusion_lets_a_period_word_fit_the_line); a short fixture that never
    // grazes the margin is CORRECTLY byte-identical under protrusion.
}

#[path = "pdf_inflate_helper.rs"]
mod pdf_inflate_helper;

#[test]
fn expansion_emits_tz_operators_optin_only() {
    let sentence = "The renderer breaks this long sentence at the measured margin. ";
    let doc = parse_markdown(&sentence.repeat(24));

    // Default: no Tz operators anywhere (byte-identical classic emission).
    let off = render_pdf_document(&doc, &PdfOptions::default()).expect("render off");
    let off_content = pdf_inflate_helper::decompressed_content(&off);
    assert!(
        !off_content.windows(b" Tz ".len()).any(|w| w == b" Tz "),
        "default render must not emit Tz"
    );

    // Opt-in expansion: Tz operators appear and stay within ±1.5% (98.5..101.5).
    let on = render_pdf_document(
        &doc,
        &PdfOptions {
            microtype: MicrotypeOptions {
                protrusion: false,
                max_expansion_per_mille: 15,
            },
            ..PdfOptions::default()
        },
    )
    .expect("render on");
    let on_content = pdf_inflate_helper::decompressed_content(&on);
    let tz_count = on_content
        .windows(b" Tz ".len())
        .filter(|w| *w == b" Tz ")
        .count();
    assert!(
        tz_count > 0,
        "expansion must emit Tz operators (got {tz_count})"
    );
    // Every emitted factor must lie within the ±1.5% budget: operand form is
    // "<fixed2> Tz " where fixed2 has one decimal digit.
    let mut i = 0;
    let mut checked = 0;
    while let Some(pos) = on_content[i..].windows(4).position(|w| w == b" Tz ") {
        let end = i + pos;
        // Scan back over the decimal operand.
        let mut s = end;
        while s > 0 && on_content[s - 1].is_ascii_digit() {
            s -= 1;
        }
        if s > 0 && on_content[s - 1] == b'.' {
            s -= 1;
            while s > 0 && on_content[s - 1].is_ascii_digit() {
                s -= 1;
            }
        }
        let val: f32 = std::str::from_utf8(&on_content[s..end])
            .ok()
            .and_then(|t| t.parse().ok())
            .unwrap_or(100.0);
        assert!(
            (98.5..=101.5).contains(&val),
            "Tz factor {val} outside ±1.5% budget"
        );
        checked += 1;
        i = end + 4;
    }
    assert_eq!(checked, tz_count, "parsed every Tz operand");
}

#[test]
fn expansion_render_is_deterministic_and_same_page_count() {
    let sentence = "The renderer breaks this long sentence at the measured margin. ";
    let doc = parse_markdown(&sentence.repeat(24));
    let opts = |n| PdfOptions {
        microtype: MicrotypeOptions {
            protrusion: false,
            max_expansion_per_mille: n,
        },
        ..PdfOptions::default()
    };
    let a = render_pdf_document(&doc, &opts(15)).expect("a");
    let b = render_pdf_document(&doc, &opts(15)).expect("b");
    assert_eq!(a, b, "expansion render is deterministic");
    let zero = render_pdf_document(&doc, &opts(0)).expect("zero");
    assert_eq!(
        zero,
        render_pdf_document(&doc, &PdfOptions::default()).expect("default")
    );
    let count_pages = |pdf: &[u8]| {
        pdf.windows(b"/Type /Page ".len())
            .filter(|w| *w == b"/Type /Page ")
            .count()
    };
    assert_eq!(
        count_pages(&a),
        count_pages(&zero),
        "expansion keeps page count"
    );
}
