//! Measure items-per-byte ratio for paragraph item builders.
//!
//! Runs paragraph_items_from_styled_text and hyphenated_paragraph_items_from_text_into
//! over the fmd_layout_perf corpus inputs + showcase, counts items, and reports
//! the items-per-byte ratio so we can pick a capacity formula.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::type_complexity,
    clippy::cast_precision_loss
)]

use franken_markdown::FontFamily;
use franken_markdown::fonts::{self, FontStyle};
use franken_markdown::layout::{
    FontSize, Hyphenator, ParagraphItem, ParagraphLayoutScratch, StyledText,
    hyphenated_paragraph_items_from_text_into, paragraph_items_from_styled_text,
};
use std::env;
use std::fs;
use std::path::Path;

fn fnv64a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn generated_words(count: usize) -> String {
    const WORDS: &[&str] = &[
        "typography",
        "rendering",
        "deterministic",
        "paragraph",
        "hyphenation",
        "microtype",
        "baseline",
        "ligature",
        "kerning",
        "markdown",
        "performance",
        "document",
    ];
    let mut out = String::new();
    for i in 0..count {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(WORDS[i % WORDS.len()]);
    }
    out
}

fn repeat_paragraph(paragraph: &str, repeats: usize) -> String {
    let mut out = String::new();
    for i in 0..repeats {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(paragraph);
        out.push(' ');
        out.push_str(&generated_words(18));
    }
    out
}

fn balanced_prose(repeats: usize) -> String {
    let paragraph = "A polished document needs calm line lengths, consistent rhythm, real kerning, and breakpoints that avoid distracting rivers of whitespace. The layout engine should prefer balanced rows while preserving deterministic output for native and browser callers.";
    repeat_paragraph(paragraph, repeats)
}

fn corpus_inputs() -> Vec<(&'static str, String)> {
    let mut inputs = vec![
        ("paragraph-1000", generated_words(1_000)),
        ("balanced-prose-28", balanced_prose(28)),
        ("balanced-prose-200", balanced_prose(200)),
        ("balanced-prose-1000", balanced_prose(1000)),
    ];
    if let Ok(readme) = fs::read_to_string("README.md") {
        inputs.push(("readme", readme));
    }
    if let Ok(showcase) = fs::read_to_string("examples/showcase.md") {
        inputs.push(("showcase", showcase));
    }
    if let Ok(notes) = fs::read_to_string("CHANGELOG.md") {
        inputs.push(("changelog", notes));
    }
    inputs
}

fn main() {
    let font = fonts::load_body(FontFamily::Sans, FontStyle::Regular).expect("font load");
    let size = FontSize::from_points(11);
    let hyphenator = Hyphenator::english();

    let inputs = corpus_inputs();
    println!(
        "input\tbytes\twords\titems_styled\titems_hyphen\tratio_styled\titems_per_word_hyphen"
    );
    let mut all_items_per_byte = Vec::new();
    let mut all_items_per_word_hyphen = Vec::new();
    let mut max_overshoot_ratio = 0.0_f64;
    for (name, text) in &inputs {
        let bytes = text.len();
        let words = text.split_whitespace().count();
        let styled = StyledText::plain(text);
        let items_styled = paragraph_items_from_styled_text(&font, &styled, size);
        let mut items_hyphen: Vec<ParagraphItem> = Vec::new();
        let mut scratch = ParagraphLayoutScratch::new();
        hyphenated_paragraph_items_from_text_into(
            &font,
            &hyphenator,
            text,
            size,
            &mut scratch,
            &mut items_hyphen,
        );
        let is = items_styled.len();
        let ih = items_hyphen.len();
        let ratio_styled = is as f64 / bytes as f64;
        let items_per_word = ih as f64 / words as f64;
        println!(
            "{}\t{}\t{}\t{}\t{}\t{:.6}\t{:.4}",
            name, bytes, words, is, ih, ratio_styled, items_per_word
        );
        let combined_ratio = (is + ih) as f64 / bytes as f64;
        all_items_per_byte.push(combined_ratio);
        all_items_per_word_hyphen.push(items_per_word);
        if combined_ratio > max_overshoot_ratio {
            max_overshoot_ratio = combined_ratio;
        }
    }

    let avg = all_items_per_byte.iter().sum::<f64>() / all_items_per_byte.len() as f64;
    let max = all_items_per_byte
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let min = all_items_per_byte
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    println!("\n--- items-per-byte (styled+hyphen, average per input) ---");
    println!("min/avg/max: {:.6} / {:.6} / {:.6}", min, avg, max);
    let avg_iw =
        all_items_per_word_hyphen.iter().sum::<f64>() / all_items_per_word_hyphen.len() as f64;
    println!("hyphen items-per-word avg: {:.4}", avg_iw);

    // Capacity formulas — pick the one with smallest worst-case overshoot.
    println!("\n--- heuristic capacity formulas (input-bytes → capacity) ---");
    let formulas: Vec<(&str, Box<dyn Fn(usize, usize) -> usize>)> = vec![
        ("bytes/6", Box::new(|b, _| b / 6)),
        ("bytes/5", Box::new(|b, _| b / 5)),
        ("bytes/4", Box::new(|b, _| b / 4)),
        ("bytes/3 + 4", Box::new(|b, _| b / 3 + 4)),
        ("bytes/2 + 4", Box::new(|b, _| b / 2 + 4)),
        ("bytes + 4", Box::new(|b, _| b + 4)),
        ("words*3 + 1", Box::new(|_, w| w * 3 + 1)),
        ("words*4 + 1", Box::new(|_, w| w * 4 + 1)),
        ("words*6 + 1", Box::new(|_, w| w * 6 + 1)),
    ];
    for (label, f) in &formulas {
        let mut worst = 0_f64;
        for (name, text) in &inputs {
            let bytes = text.len();
            let cap = f(bytes, text.split_whitespace().count());
            let styled = StyledText::plain(text);
            let items_styled = paragraph_items_from_styled_text(&font, &styled, size);
            let mut items_hyphen: Vec<ParagraphItem> = Vec::new();
            let mut scratch = ParagraphLayoutScratch::new();
            hyphenated_paragraph_items_from_text_into(
                &font,
                &hyphenator,
                text,
                size,
                &mut scratch,
                &mut items_hyphen,
            );
            let actual = items_styled.len() + items_hyphen.len();
            let overshoot = (cap as f64 - actual as f64) / actual.max(1) as f64;
            if overshoot > worst {
                worst = overshoot;
            }
            println!(
                "{}  input={} bytes={} capacity={} actual={} overshoot_pct={:.2}",
                label,
                name,
                bytes,
                cap,
                actual,
                overshoot * 100.0
            );
        }
        println!(
            "{}  worst_overshoot_pct = {:.2} (target: cap >= actual; positive = slack)",
            label,
            worst * 100.0
        );
        println!();
    }

    // Env dump
    let cwd = env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    println!("cwd={}", cwd.display());
    if let Ok(readme) = fs::read("README.md") {
        println!("fnv64a-README={:016x}", fnv64a(&readme));
    }
}
