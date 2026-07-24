//! CJK (Chinese / Japanese / Korean) line-breaking behaviour.
//!
//! CJK text is written without spaces, so a purely whitespace-driven breaker
//! finds *no* break opportunity inside a run of ideographs: the whole run
//! becomes one unbreakable box that runs past the right margin. These tests pin
//! the UAX #14-guided behaviour that replaces that:
//!
//! * a break is allowed *between* adjacent ideographs / kana / Hangul,
//! * never *before* closing punctuation (`）】、。，！？；：」』`) or a
//!   non-starter (small kana, `々`, `ー`),
//! * never *after* an opening bracket (`（【「『`),
//! * a script boundary (CJK ↔ Latin) is a break opportunity,
//! * pure-ASCII text keeps exactly the breaks it had before (regression guard).
//!
//! The PDF cases deliberately assert on *laid-out line geometry* read back out
//! of the page content stream, not on a string splitter: a splitter unit test
//! can pass while the real layout path still overflows the measure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::fonts::{FontStyle, load_body};
use franken_markdown::layout::{
    AdvanceMetrics, FORCED_BREAK_PENALTY, FontSize, INF_PENALTY, LayoutUnit, PairMetrics,
    ParagraphItem, break_paragraph, paragraph_items_from_text,
};
use franken_markdown::theme::FontFamily;
use franken_markdown::{PdfOptions, Theme, render_pdf};

/// Deterministic metrics oracle: CJK is full-width (1 em), Latin is half-width,
/// the space is a quarter em. Every expected width in this file is therefore
/// hand-computable, and the CJK/Latin width ratio matches real typography.
struct CjkMetrics;

fn is_wide(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x20000..=0x3FFFD)
}

impl AdvanceMetrics for CjkMetrics {
    fn advance_1000(&self, ch: char) -> u32 {
        match ch {
            ' ' => 250,
            _ if is_wide(ch) => 1000,
            _ => 500,
        }
    }
}

impl PairMetrics for CjkMetrics {}

const SIZE: FontSize = FontSize::from_points(10);

/// Character offsets (into the paragraph's plain text) at which the built item
/// stream allows a line break, i.e. every non-prohibitive penalty and every
/// glue. The trailing forced paragraph break is excluded.
fn break_offsets(items: &[ParagraphItem]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut chars = 0usize;
    for (idx, item) in items.iter().enumerate() {
        match item {
            ParagraphItem::Box(b) => chars += b.text.chars().count(),
            // Interword glue stands for one space character; the zero-width
            // CJK break glue stands for nothing at all.
            ParagraphItem::Glue(g) => {
                offsets.push(chars);
                if g.width != LayoutUnit::ZERO {
                    chars += 1;
                }
            }
            ParagraphItem::Penalty(p) => {
                let last = idx + 1 == items.len();
                if !last && p.penalty < INF_PENALTY && p.penalty != FORCED_BREAK_PENALTY {
                    offsets.push(chars);
                }
            }
        }
    }
    offsets.dedup();
    offsets
}

fn line_widths(text: &str, measure_points: i32) -> Vec<LayoutUnit> {
    let items = paragraph_items_from_text(&CjkMetrics, text, SIZE);
    let measure = LayoutUnit::from_points(measure_points);
    break_paragraph(&items, measure)
        .into_iter()
        .map(|line| line.natural_width)
        .collect()
}

fn han(n: usize) -> String {
    // A deterministic run of common ideographs with no spaces and no
    // punctuation, so every break is a pure ID x ID decision.
    const HAN: &str = "中文排版测试字符串换行处理";
    HAN.chars().cycle().take(n).collect()
}

// ---------------------------------------------------------------------------
// Item-stream level: break opportunities inside an unspaced run.
// ---------------------------------------------------------------------------

#[test]
fn cjk_run_offers_break_opportunities_between_ideographs() {
    let text = han(200);
    let items = paragraph_items_from_text(&CjkMetrics, &text, SIZE);
    let offsets = break_offsets(&items);
    assert!(
        offsets.len() > 150,
        "an unspaced 200-ideograph run must offer a break between adjacent \
         characters, got {} opportunities",
        offsets.len()
    );
    assert_eq!(offsets.first().copied(), Some(1), "offsets: {offsets:?}");
}

#[test]
fn long_cjk_paragraph_wraps_inside_the_measure() {
    let text = han(200);
    let measure = LayoutUnit::from_points(200);
    let widths = line_widths(&text, 200);
    assert!(
        widths.len() >= 10,
        "200 full-width ideographs in a 200 pt measure need >= 10 lines, got {}",
        widths.len()
    );
    for (i, w) in widths.iter().enumerate() {
        assert!(
            *w <= measure,
            "line {i} is {} mpt wide, past the {} mpt measure",
            w.milli_points(),
            measure.milli_points()
        );
    }
    // The measure fits exactly 20 full-width characters; a correct breaker
    // uses essentially all of it instead of leaving a ragged half-empty line.
    let filled = widths[..widths.len() - 1]
        .iter()
        .all(|w| w.milli_points() >= measure.milli_points() * 9 / 10);
    assert!(filled, "CJK lines should fill the measure: {widths:?}");
}

#[test]
fn no_break_before_cjk_closing_punctuation() {
    // 。 ， ！ ？ ； ： ） 】 」 』 、 all forbid a break *before* them.
    for closer in [
        '。', '，', '！', '？', '；', '：', '）', '】', '」', '』', '、',
    ] {
        let text = format!("中文{closer}中文");
        let items = paragraph_items_from_text(&CjkMetrics, &text, SIZE);
        let offsets = break_offsets(&items);
        assert!(
            !offsets.contains(&2),
            "a line must not start with {closer:?}: offsets {offsets:?}"
        );
        assert!(
            offsets.contains(&3),
            "a break after {closer:?} is still allowed: offsets {offsets:?}"
        );
    }
}

#[test]
fn no_break_after_cjk_opening_bracket() {
    for opener in ['（', '【', '「', '『', '〔', '《'] {
        let text = format!("中文{opener}中文");
        let items = paragraph_items_from_text(&CjkMetrics, &text, SIZE);
        let offsets = break_offsets(&items);
        assert!(
            !offsets.contains(&3),
            "an opening {opener:?} must not be orphaned at a line end: {offsets:?}"
        );
        assert!(
            offsets.contains(&2),
            "a break before {opener:?} is still allowed: {offsets:?}"
        );
    }
}

#[test]
fn no_break_before_japanese_non_starters() {
    // Small kana and the prolonged sound mark cling to the preceding glyph.
    for ns in ['ゃ', 'ゅ', 'ょ', 'っ', 'ー', '々', 'ゞ'] {
        let text = format!("日本{ns}語");
        let items = paragraph_items_from_text(&CjkMetrics, &text, SIZE);
        let offsets = break_offsets(&items);
        assert!(
            !offsets.contains(&2),
            "{ns:?} must not start a line: {offsets:?}"
        );
    }
}

#[test]
fn hangul_syllables_break_between_but_not_inside_jamo_clusters() {
    let text = "한국어줄바꿈";
    let items = paragraph_items_from_text(&CjkMetrics, text, SIZE);
    let offsets = break_offsets(&items);
    assert!(
        offsets.contains(&1) && offsets.contains(&3),
        "Hangul syllables may break between syllables: {offsets:?}"
    );

    // Conjoining jamo compose one syllable: no break inside the cluster.
    let jamo = "\u{1100}\u{1161}\u{11A8}\u{1102}\u{1161}"; // 각 + 나
    let items = paragraph_items_from_text(&CjkMetrics, jamo, SIZE);
    let offsets = break_offsets(&items);
    assert!(
        !offsets.contains(&1) && !offsets.contains(&2),
        "a Hangul jamo cluster is unbreakable: {offsets:?}"
    );
    assert!(
        offsets.contains(&3),
        "a break between two jamo clusters is allowed: {offsets:?}"
    );
}

#[test]
fn mixed_cjk_and_latin_breaks_at_the_script_boundary() {
    let text = "中文abc中文";
    let items = paragraph_items_from_text(&CjkMetrics, text, SIZE);
    let offsets = break_offsets(&items);
    assert!(
        offsets.contains(&2),
        "a break is allowed between an ideograph and Latin: {offsets:?}"
    );
    assert!(
        offsets.contains(&5),
        "a break is allowed between Latin and an ideograph: {offsets:?}"
    );
    for inside in [3, 4] {
        assert!(
            !offsets.contains(&inside),
            "the Latin word 'abc' must stay unbroken: {offsets:?}"
        );
    }
}

#[test]
fn mixed_cjk_latin_paragraph_wraps_inside_the_measure() {
    let text =
        "franken_markdown 渲染引擎支持中文日本語와 한국어 mixed content 混合排版测试".repeat(4);
    let measure = LayoutUnit::from_points(180);
    for (i, w) in line_widths(&text, 180).into_iter().enumerate() {
        assert!(
            w <= measure,
            "mixed line {i} is {} mpt wide, past the {} mpt measure",
            w.milli_points(),
            measure.milli_points()
        );
    }
}

// ---------------------------------------------------------------------------
// Regression guard: pure ASCII behaviour must not move at all.
// ---------------------------------------------------------------------------

#[test]
fn ascii_paragraph_gains_no_new_break_opportunities() {
    let text = "the quick brown fox jumps over the lazy dog, and the \
                supercalifragilisticexpialidocious hyphenationless identifier stays whole";
    let items = paragraph_items_from_text(&CjkMetrics, text, SIZE);
    let offsets = break_offsets(&items);
    let spaces: Vec<usize> = text
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        offsets, spaces,
        "ASCII text must break at spaces and nowhere else"
    );
}

#[test]
fn ascii_paragraph_lines_are_unchanged() {
    // Exact widths, computed from the oracle: every Latin glyph is 500 and the
    // space 250 at 10 pt, so "the quick" = 9 glyphs -> 8*5000 + 2500 mpt.
    // Recorded from the renderer before CJK support existed.
    let widths = line_widths("the quick brown fox jumps over the lazy dog", 50);
    assert_eq!(
        widths.iter().map(|w| w.milli_points()).collect::<Vec<_>>(),
        vec![42_500, 42_500, 47_500, 37_500, 15_000]
    );
}

// ---------------------------------------------------------------------------
// PDF path: real laid-out geometry, read back from the page content stream.
// ---------------------------------------------------------------------------

/// One `BT ... ET` text-showing run recovered from a page content stream.
#[derive(Debug, Clone, Copy)]
struct PdfTextRun {
    size: f32,
    x: f32,
    y: f32,
    glyphs: usize,
    /// Sum of the TJ array's kern adjustments, in 1/1000 em.
    kern_1000: f32,
}

impl PdfTextRun {
    fn width(self, advance_1000: u32) -> f32 {
        (self.glyphs as f32 * advance_1000 as f32 - self.kern_1000) * self.size / 1000.0
    }
}

/// Parse every uncompressed `BT ... Tf ... Tm [ ... ] TJ ... ET` run.
///
/// Test pages here are deliberately small, so the writer leaves the content
/// stream uncompressed (`PAGE_STREAM_COMPRESSION_MIN`).
fn pdf_text_runs(pdf: &[u8]) -> Vec<PdfTextRun> {
    let text = String::from_utf8_lossy(pdf);
    let mut runs = Vec::new();
    for chunk in text.split("BT ").skip(1) {
        let Some(body) = chunk.split(" ET").next() else {
            continue;
        };
        let Some((head, array)) = body.split_once('[') else {
            continue;
        };
        let Some(array) = array.split(']').next() else {
            continue;
        };
        let nums: Vec<&str> = head.split_whitespace().collect();
        let Some(tf) = nums.iter().position(|t| *t == "Tf") else {
            continue;
        };
        let Some(tm) = nums.iter().position(|t| *t == "Tm") else {
            continue;
        };
        let parse = |s: &str| s.parse::<f32>().unwrap_or(0.0);
        let size = parse(nums[tf - 1]);
        let x = parse(nums[tm - 2]);
        let y = parse(nums[tm - 1]);

        let mut glyphs = 0usize;
        let mut kern_1000 = 0.0f32;
        let mut rest = array;
        while let Some(open) = rest.find('<') {
            let (before, tail) = rest.split_at(open);
            for token in before.split_whitespace() {
                kern_1000 += parse(token);
            }
            let Some(end) = tail.find('>') else { break };
            glyphs += (end - 1) / 4;
            rest = &tail[end + 1..];
        }
        for token in rest.split_whitespace() {
            kern_1000 += parse(token);
        }
        runs.push(PdfTextRun {
            size,
            x,
            y,
            glyphs,
            kern_1000,
        });
    }
    runs
}

/// The advance the PDF actually draws for one CJK character. The bundled OFL
/// faces carry no CJK glyphs, so every ideograph resolves to `.notdef` and the
/// renderer both measures and paints that glyph's advance — which is exactly
/// what the test must compare the line geometry against.
fn cjk_advance_1000() -> u32 {
    load_body(FontFamily::Sans, FontStyle::Regular)
        .unwrap()
        .advance_1000('中')
}

fn narrow_page_options(width_pt: f32, margin_pt: f32) -> PdfOptions {
    let mut theme = Theme::default();
    theme.page.size.width_pt = width_pt;
    theme.page.size.height_pt = 900.0;
    theme.page.margins = franken_markdown::PageMargins {
        top_pt: 36.0,
        right_pt: margin_pt,
        bottom_pt: 36.0,
        left_pt: margin_pt,
    };
    PdfOptions {
        theme,
        ..PdfOptions::default()
    }
}

fn render(md: &str, opts: &PdfOptions) -> Vec<u8> {
    render_pdf(md, opts).unwrap()
}

#[test]
fn pdf_cjk_paragraph_stays_inside_the_right_margin() {
    // 60 pt of measure is narrower than the 14-character emergency chunk the
    // generic long-token breaker used to be the *only* break source for CJK, so
    // this is exactly the case that used to run off the page.
    let opts = narrow_page_options(180.0, 60.0);
    let right = opts.theme.page.size.width_pt - opts.theme.page.margins.right_pt;
    let pdf = render(&format!("{}\n", han(120)), &opts);
    let advance = cjk_advance_1000();
    let runs = pdf_text_runs(&pdf);
    assert!(runs.len() > 5, "expected many wrapped lines, got {runs:?}");
    for run in &runs {
        let end = run.x + run.width(advance);
        assert!(
            end <= right + 0.05,
            "line at y={} ends at {end:.2} pt, past the {right:.2} pt right margin \
             ({} glyphs at {} pt)",
            run.y,
            run.glyphs,
            run.size
        );
    }
}

#[test]
fn pdf_cjk_paragraph_fills_the_measure() {
    let opts = narrow_page_options(180.0, 60.0);
    let content = opts.theme.page.size.width_pt - 2.0 * opts.theme.page.margins.left_pt;
    let pdf = render(&format!("{}\n", han(120)), &opts);
    let advance = cjk_advance_1000();
    let runs = pdf_text_runs(&pdf);
    let per_char = runs[0].size * advance as f32 / 1000.0;
    for run in &runs[..runs.len() - 1] {
        let slack = content - run.width(advance);
        assert!(
            slack < per_char,
            "line at y={} wastes {slack:.2} pt of a {content:.2} pt measure — \
             another ideograph ({per_char:.2} pt) would have fit",
            run.y
        );
    }
}

#[test]
fn pdf_cjk_line_never_starts_with_closing_punctuation() {
    // The measure fits exactly 10 ideographs. A breaker that ignores UAX #14
    // would put the first 10 characters on line one and orphan the closing
    // 。 at the head of line two; the correct break moves one character down.
    let advance = cjk_advance_1000();
    let opts = narrow_page_options(200.0, (200.0 - 10.0 * 11.0 * advance as f32 / 1000.0) / 2.0);
    let text: String = format!("{}。{}", han(10), han(20));
    let pdf = render(&format!("{text}\n"), &opts);
    let runs = pdf_text_runs(&pdf);
    assert!(runs.len() >= 3, "expected several lines, got {runs:?}");
    assert_eq!(
        runs[0].glyphs, 9,
        "the ideograph before 。 must move down with it, so line one holds 9 \
         characters + the closer, not 10: {runs:?}"
    );
}

#[test]
fn pdf_cjk_line_never_ends_with_an_opening_bracket() {
    let advance = cjk_advance_1000();
    let opts = narrow_page_options(200.0, (200.0 - 10.0 * 11.0 * advance as f32 / 1000.0) / 2.0);
    let text: String = format!("{}「{}", han(9), han(20));
    let pdf = render(&format!("{text}\n"), &opts);
    let runs = pdf_text_runs(&pdf);
    assert!(runs.len() >= 3, "expected several lines, got {runs:?}");
    assert_eq!(
        runs[0].glyphs, 9,
        "「 must not be stranded at the end of line one: {runs:?}"
    );
}

/// Per baseline (`y` descending): first `x`, total glyph count, and the x the
/// last run on that line starts at — a compact fingerprint of Latin wrapping.
fn ascii_line_shape(runs: &[PdfTextRun]) -> Vec<(f32, usize, f32)> {
    let mut lines: Vec<(f32, f32, usize, f32)> = Vec::new();
    for run in runs {
        match lines.last_mut() {
            Some(line) if (line.0 - run.y).abs() < 0.01 => {
                line.2 += run.glyphs;
                line.3 = run.x;
            }
            _ => lines.push((run.y, run.x, run.glyphs, run.x)),
        }
    }
    lines
        .into_iter()
        .map(|(_, x, glyphs, last)| (x, glyphs, last))
        .collect()
}

#[test]
fn pdf_ascii_paragraph_geometry_is_unchanged() {
    // Recorded from the pre-CJK renderer: Latin wrapping must not move.
    let opts = narrow_page_options(300.0, 60.0);
    let md = "the quick brown fox jumps over the lazy dog while the \
              hyphenation engine keeps working exactly as it did before\n";
    let runs = pdf_text_runs(&render(md, &opts));
    assert_eq!(
        ascii_line_shape(&runs),
        vec![
            (60.0, 34, 223.85),
            (60.0, 34, 223.32),
            (60.0, 36, 224.49),
            (60.0, 6, 60.0),
        ],
        "Latin wrapping, justification, and hyphenation must not move"
    );
}
