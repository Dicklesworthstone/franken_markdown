//! GitHub emoji shortcodes in HTML prose become Unicode emoji, the way
//! GitHub's preview renders them; code, URLs and unknown names stay literal.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, PdfOptions, render_html};

fn body(markdown: &str) -> String {
    let html = render_html(markdown, &HtmlOptions::default()).unwrap();
    let start = html.find("<main class=\"fmd\">").unwrap() + "<main class=\"fmd\">".len();
    html[start..html.rfind("</main>").unwrap()]
        .trim()
        .to_string()
}

#[test]
fn known_shortcodes_become_emoji_in_prose_headings_tables_and_links() {
    assert_eq!(
        body("Ship it :rocket: :+1: :white_check_mark:"),
        "<p>Ship it \u{1F680} \u{1F44D} \u{2705}</p>"
    );
    assert!(body("# :sparkles: New").contains("\u{2728} New</h1>"));
    assert!(body("| a |\n|---|\n| :x: |\n").contains("<td>\u{274C}</td>"));
    assert!(body("[:tada: done](https://e.x)").contains(">\u{1F389} done</a>"));
    // Adjacent shortcodes need no separator.
    assert_eq!(body(":x::warning:"), "<p>\u{274C}\u{26A0}\u{FE0F}</p>");
}

#[test]
fn code_urls_times_and_unknown_names_stay_literal() {
    assert_eq!(
        body("`:rocket:` and :not_a_real_emoji: at 10:30:45"),
        "<p><code>:rocket:</code> and :not_a_real_emoji: at 10:30:45</p>"
    );
    assert!(body("```\n:rocket:\n```").contains(":rocket:"));
    assert!(body("<https://e.x/:rocket:/>").contains("href=\"https://e.x/:rocket:/\""));
    // Uppercase or spaced names are not shortcodes.
    assert_eq!(body(":Rocket: : rocket :"), "<p>:Rocket: : rocket :</p>");
}

#[test]
fn pdf_keeps_shortcode_text_under_the_emoji_policy() {
    // PDF bundles no emoji face (docs/EMOJI_FALLBACK.md): the readable
    // shortcode is kept rather than producing .notdef boxes.
    let doc = franken_markdown::parse_markdown("Ship it :rocket:");
    let layer =
        franken_markdown::pdf::verification_text_layer(&doc, &PdfOptions::default()).unwrap();
    let text: String = layer
        .pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(text, "Ship it :rocket:");
}
