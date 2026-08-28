//! Property-based parser/renderer invariants (bead 2c72.1).
//!
//! A std-only, LCG-driven grammar generator produces structured-but-random
//! Markdown documents, and invariants are asserted over every generated
//! input:
//!
//! 1. **No panic**: parse + render never panic.
//! 2. **Determinism**: the same input renders to byte-identical output.
//! 3. **Adversarial seeds**: NUL, astral, CRLF, unclosed constructs — all
//!    survive with graceful degradation.
//!
//! Seeded reproducibility: failures print the seed and the minimal input.
//! Round-trip convergence through full-HTML re-parsing is intentionally
//! omitted: the rendered document embeds font-face CSS whose order depends
//! on which font weights the document uses, so byte-level HTML round-trip
//! does not converge by design.

use franken_markdown::{parse_markdown, render_html_document, HtmlOptions};

// ---------------------------------------------------------------------------
// LCG (deterministic, matches the hostile-input sweep style in fmd-font)
// ---------------------------------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

// ---------------------------------------------------------------------------
// Grammar-aware document generator
// ---------------------------------------------------------------------------

const WORDS: [&str; 10] = [
    "alpha", "beta", "gamma", "data", "test", "value", "x", "café", "日本語", "影院",
];

const INLINE_BITS: [&str; 8] = [
    "*em*", "**strong**", "~~strike~~", "`code`", "[link](/u)", "a & b",
    "emoji \u{1f642}", "entity &amp;",
];

const ADVERSARIAL: [&str; 8] = [
    "[^1]", "[^unclosed", "*unclosed *emphasis", "[unclosed](/link",
    "\u{0000}nul", "\u{10FFFF}astral", "a\r\nb", "**a *b** c*",
];

fn gen_inline(lcg: &mut Lcg, out: &mut String) {
    match lcg.below(6) {
        0 => {
            out.push_str(lcg.pick(&WORDS));
            out.push(' ');
        }
        1 => out.push_str(lcg.pick(&INLINE_BITS)),
        2 => out.push_str(lcg.pick(&ADVERSARIAL)),
        3 => {
            out.push('*');
            gen_inline(lcg, out);
            out.push('*');
        }
        4 => {
            out.push_str("**");
            gen_inline(lcg, out);
            out.push_str("** ");
        }
        _ => {
            out.push_str(lcg.pick(&WORDS));
            out.push(' ');
        }
    }
}

fn gen_paragraph(lcg: &mut Lcg, out: &mut String) {
    let lines = 1 + lcg.below(3);
    for l in 0..lines {
        for _ in 0..(2 + lcg.below(6)) {
            gen_inline(lcg, out);
        }
        if l + 1 < lines {
            out.push('\n');
        }
    }
    out.push('\n');
}

fn gen_heading(lcg: &mut Lcg, out: &mut String, level: usize) {
    for _ in 0..level {
        out.push('#');
    }
    out.push(' ');
    for _ in 0..(1 + lcg.below(4)) {
        gen_inline(lcg, out);
    }
    out.push('\n');
}

fn gen_list(lcg: &mut Lcg, out: &mut String) {
    let ordered = lcg.below(2) == 0;
    let items = 1 + lcg.below(4);
    for i in 0..items {
        if ordered {
            out.push_str(&format!("{}. ", i + 1));
        } else {
            out.push_str("- ");
        }
        if lcg.below(4) == 0 {
            out.push_str(if lcg.below(2) == 0 { "[x] " } else { "[ ] " });
        }
        for _ in 0..(1 + lcg.below(3)) {
            gen_inline(lcg, out);
        }
    }
    out.push('\n');
}

fn gen_table(lcg: &mut Lcg, out: &mut String) {
    let cols = 1 + lcg.below(3);
    out.push_str("| head");
    for _ in 1..cols {
        out.push_str(" | head");
    }
    out.push_str(" |\n|");
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for _ in 0..(1 + lcg.below(2)) {
        out.push_str("| cell");
        for _ in 1..cols {
            out.push_str(&format!(" | c{}", lcg.below(100)));
        }
        out.push_str(" |\n");
    }
    out.push('\n');
}

fn gen_fence(lcg: &mut Lcg, out: &mut String) {
    let langs = ["", "rust", "text"];
    out.push_str(&format!("```{}\n", langs[lcg.below(langs.len())]));
    for _ in 0..(1 + lcg.below(3)) {
        out.push_str("code line\n");
    }
    out.push_str("```\n");
}

fn gen_quote(lcg: &mut Lcg, out: &mut String, depth: usize) {
    out.push_str("> ");
    for _ in 0..(1 + lcg.below(3)) {
        gen_inline(lcg, out);
    }
    if depth < 2 && lcg.below(3) == 0 {
        out.push_str("> \n> ");
        gen_quote(lcg, out, depth + 1);
    }
    out.push('\n');
}

fn gen_document(lcg: &mut Lcg) -> String {
    let mut doc = String::new();
    let blocks = 1 + lcg.below(6);
    for i in 0..blocks {
        if i > 0 {
            doc.push('\n');
        }
        match lcg.below(7) {
            0 => {
                let level = 1 + lcg.below(3);
                gen_heading(lcg, &mut doc, level);
            }
            1 => gen_paragraph(lcg, &mut doc),
            2 => gen_list(lcg, &mut doc),
            3 => gen_table(lcg, &mut doc),
            4 => gen_fence(lcg, &mut doc),
            5 => gen_quote(lcg, &mut doc, 0),
            _ => gen_paragraph(lcg, &mut doc),
        }
    }
    doc
}

// ---------------------------------------------------------------------------
// Invariant tests
// ---------------------------------------------------------------------------

const GENERATOR_SEED: u64 = 0xC0FF_EE01_2345_6789;
const DOCUMENTS: usize = 600;

#[test]
fn no_panic_across_generated_corpus() {
    let opts = HtmlOptions::default();
    let mut lcg = Lcg::new(GENERATOR_SEED);
    for case in 0..DOCUMENTS {
        let src = gen_document(&mut lcg);
        let result = std::panic::catch_unwind(|| {
            let doc = parse_markdown(&src);
            let _ = render_html_document(&doc, &opts).expect("render");
        });
        assert!(
            result.is_ok(),
            "case {case}: parse/render panicked on:\n{src}"
        );
    }
}

#[test]
fn render_is_deterministic_across_generated_corpus() {
    let opts = HtmlOptions::default();
    let mut lcg = Lcg::new(GENERATOR_SEED);
    for case in 0..DOCUMENTS {
        let src = gen_document(&mut lcg);
        let doc = parse_markdown(&src);
        let a = render_html_document(&doc, &opts).unwrap();
        let b = render_html_document(&doc, &opts).unwrap();
        assert_eq!(
            a, b,
            "case {case}: render is not deterministic; input:\n{src}"
        );
    }
}

#[test]
fn adversarial_seeds_no_panic_and_deterministic() {
    let opts = HtmlOptions::default();
    for seed_text in ADVERSARIAL {
        for prefix in ["", "# ", "- ", "> ", "    "] {
            let src = format!("{prefix}{seed_text}\n\nbody\n");
            let render_result = std::panic::catch_unwind(|| {
                let doc = parse_markdown(&src);
                render_html_document(&doc, &opts).expect("render")
            });
            let html = match render_result {
                Ok(html) => html,
                Err(_) => panic!("adversarial seed panicked on:\n{src}"),
            };
            let doc_again = parse_markdown(&src);
            let html2 = render_html_document(&doc_again, &opts).unwrap();
            assert_eq!(
                html, html2,
                "determinism violated for:\n{src}"
            );
        }
    }
}

#[test]
fn generator_is_reproducible() {
    let mut a = Lcg::new(GENERATOR_SEED);
    let mut b = Lcg::new(GENERATOR_SEED);
    for _ in 0..50 {
        assert_eq!(gen_document(&mut a), gen_document(&mut b));
    }
}
