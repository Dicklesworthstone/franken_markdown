//! Parser metamorphic tests: deterministic pseudo-fuzzing and invariants that
//! should hold across equivalent source forms. These tests intentionally avoid
//! third-party fuzz/differential crates so production and dev builds stay lean.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, parse_markdown, render_html, render_html_document};

fn render(src: &str) -> String {
    render_html(src, &HtmlOptions::default()).unwrap()
}

fn render_parsed(src: &str) -> String {
    let doc = parse_markdown(src);
    render_html_document(&doc, &HtmlOptions::default()).unwrap()
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_usize(&mut self, max: usize) -> usize {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.state >> 32) as usize) % max
    }
}

const BLOCKS: &[&str] = &[
    "# Heading\n",
    "Paragraph with **strong**, *em*, `code`, ~~gone~~, and [link](https://example.com).\n",
    "> quoted\n> block\n",
    "- one\n- [x] done\n  - nested\n",
    "1. first\n2. second\n",
    "| Name | Value |\n|---|---:|\n| alpha | 1 |\n| beta | 2 |\n",
    "```text\nfn main() <unsafe>\n```\n",
    "Text with <script>alert(1)</script> & raw brackets.\n",
    "[ref]: /target \"title\"\nUse [ref] and [missing].\n",
    "---\n",
];

fn generated_document(rng: &mut Lcg, blocks: usize) -> String {
    let mut out = String::new();
    for idx in 0..blocks {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(BLOCKS[rng.next_usize(BLOCKS.len())]);
    }
    out
}

#[test]
fn generated_corpus_renders_deterministically_and_parse_once_matches() {
    let mut rng = Lcg::new(0xf0d0_cafe_5eed_1234);
    for _case in 0..160 {
        let blocks = 1 + rng.next_usize(8);
        let src = generated_document(&mut rng, blocks);
        let direct_a = render(&src);
        let direct_b = render(&src);
        let parsed = render_parsed(&src);

        assert_eq!(direct_a, direct_b);
        assert_eq!(direct_a, parsed);
        assert!(direct_a.starts_with("<!DOCTYPE html>\n"));
        assert!(direct_a.ends_with("</html>\n"));
    }
}

#[test]
fn generated_corpus_escapes_raw_html_by_default() {
    let src = generated_document(&mut Lcg::new(0x515c_a1ab_1e55), 32);
    let html = render(&src);

    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
}

#[test]
fn line_endings_and_final_newline_do_not_change_output() {
    let lf = "# Title\n\nParagraph with **strong** text.\n\n- one\n- two\n";
    let crlf = lf.replace('\n', "\r\n");
    let no_final_newline = lf.trim_end_matches('\n');

    assert_eq!(render(lf), render(&crlf));
    assert_eq!(render(lf), render(no_final_newline));
}

#[test]
fn reference_definition_position_is_not_semantic() {
    let before = "[id]: /dest \"title\"\n\nSee [id].\n";
    let after = "See [id].\n\n[id]: /dest \"title\"\n";

    assert_eq!(render(before), render(after));
}

#[test]
fn reference_label_case_and_whitespace_variants_are_equivalent() {
    let compact = "[Multi Word]: /ok\n\nSee [this][multi word].";
    let spaced = "[  MULTI   WORD  ]: /ok\n\nSee [this][Multi   Word].";

    assert_eq!(render(compact), render(spaced));
}
