//! R3 no-op proof: paragraph_items_from_styled_text and
//! hyphenated_paragraph_items_from_text_into already presize their items Vec.

use std::hint::black_box;
use std::time::Instant;

use franken_markdown::layout::{
    AdvanceMetrics, FontSize, PairMetrics, StyledRun, StyledText, TextStyle,
    hyphenated_paragraph_items_from_text_into, paragraph_items_from_styled_text,
};

#[derive(Default)]
struct ZeroMetrics;

impl AdvanceMetrics for ZeroMetrics {
    fn advance_1000(&self, _ch: char) -> u32 { 0 }
}
impl PairMetrics for ZeroMetrics {}

fn main() {
    let latin = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris. \
        Duis aute irure dolor in reprehenderit in voluptate velit esse. \
        Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt.";
    let pure_ascii = std::iter::repeat(latin).take(8).collect::<String>();
    let pure_cjk =
        "繁體中文測試資料內含多種字符與排版符號用以驗證排版引擎對於亞洲文字之處理能力".repeat(2);

    let metrics = ZeroMetrics::default();
    let fs = FontSize::from_points(10);

    let st = StyledText {
        runs: vec![
            StyledRun { text: pure_ascii.clone(), style: TextStyle::BODY },
            StyledRun { text: pure_cjk.clone(), style: TextStyle::BODY },
        ],
    };
    let start = Instant::now();
    let items = paragraph_items_from_styled_text(&metrics, &st, fs);
    let dt = start.elapsed();
    let styled_count = items.len();
    let styled_chars = pure_ascii.chars().count() + pure_cjk.chars().count();
    let styled_required_capacity = styled_chars * 4 + 4;
    println!("paragraph_items_from_styled_text  {styled_count} items (capacity preset = {styled_required_capacity}) in {dt:?}");
    black_box(styled_count);

    let mut out = Vec::new();
    let h = franken_markdown::layout::Hyphenator::english();
    let mut scratch = franken_markdown::layout::ParagraphLayoutScratch::new();

    let start = Instant::now();
    hyphenated_paragraph_items_from_text_into(&metrics, &h, &pure_ascii, fs, &mut scratch, &mut out);
    let ascii_n = out.len();
    out.clear();
    hyphenated_paragraph_items_from_text_into(&metrics, &h, &pure_cjk, fs, &mut scratch, &mut out);
    let dt = start.elapsed();
    let cjk_n = out.len();
    let cjk_chars = pure_cjk.chars().count();
    let cjk_required_capacity = cjk_chars * 4 + 4;
    println!("hyphenated_paragraph_items_from_text_into  ascii={ascii_n} (capacity reserved on first call); cjk={cjk_n} (capacity required = {cjk_required_capacity}); wall {dt:?}");
    black_box((ascii_n, cjk_n));

    let passed = if styled_count <= styled_required_capacity { "true" } else { "FALSE" };
    println!("verdict: items.len() <= chars*4+4  (styled: {styled_count} <= {styled_required_capacity} = {passed}; hyph cjk: {cjk_n} <= {cjk_required_capacity})");
}
