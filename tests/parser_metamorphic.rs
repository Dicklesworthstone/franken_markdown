//! Parser metamorphic tests: deterministic pseudo-fuzzing and invariants that
//! should hold across equivalent source forms. These tests intentionally avoid
//! third-party fuzz/differential crates so production and dev builds stay lean.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    HtmlOptions, PdfOptions, parse_markdown, render_html, render_html_document, render_pdf,
};

fn render(src: &str) -> String {
    render_html(src, &HtmlOptions::default()).unwrap()
}

fn render_pdf_pinned(src: &str) -> Vec<u8> {
    let opts = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    render_pdf(src, &opts).unwrap()
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
fn utf8_bom_does_not_change_normal_rendered_output() {
    let plain = "[id]: /dest \"title\"\n\n# Title\n\nSee [id].\n";
    let bom = format!("\u{feff}{plain}");

    assert_eq!(render(plain), render(&bom));
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

// --- grn.5.2: cross-cutting invariants over generated inputs -----------------

/// Adversarial blocks whose TEXT carries HTML/JS-injection payloads. Rendered with
/// the default (escaping) policy, none of these must survive as live markup.
const INJECTION_BLOCKS: &[&str] = &[
    "A paragraph with <script>alert('xss')</script> and <img src=x onerror=alert(1)>.\n",
    "Ampersand & entity &amp; and a bare < and > in text.\n",
    "Inline `code with <b>tags</b> & amps` stays literal.\n",
    "> quote with <iframe src=evil></iframe>\n",
    "- list <svg/onload=alert(1)> item\n",
    "[link](javascript:alert(1)) and ![img](x\"onerror=alert(1)).\n",
];

fn injection_document(rng: &mut Lcg, blocks: usize) -> String {
    let mut out = String::new();
    for idx in 0..blocks {
        if idx > 0 {
            out.push_str("\n\n");
        }
        out.push_str(INJECTION_BLOCKS[rng.next_usize(INJECTION_BLOCKS.len())]);
    }
    out
}

#[test]
fn generated_html_never_emits_live_injected_markup() {
    let mut rng = Lcg::new(0x1235_c70a_5afe_0001);
    for _ in 0..120 {
        let blocks = 1 + rng.next_usize(6);
        let html = render(&injection_document(&mut rng, blocks));
        // Inspect only rendered USER content (after the trusted fmd <head>/<style>).
        let body = html.split("</head>").nth(1).unwrap_or(&html);
        // Tags fmd NEVER legitimately emits — their presence would mean raw-HTML
        // pass-through of user text (a default-escaping breach). (Markdown images
        // legitimately produce <img>, so it is intentionally NOT in this set.)
        for tag in [
            "<script", "<iframe", "<svg", "<b>", "</b>", "<object", "<embed",
        ] {
            assert!(!body.contains(tag), "raw HTML tag {tag:?} leaked into body");
        }
        // No live attribute-breakout or event handler: fmd escapes `"` to &quot;, so
        // a raw quote-then-handler can never appear, and it never builds a
        // javascript: href.
        for breakout in [
            "\"onerror",
            "\"onload",
            "\"onmouseover",
            "href=\"javascript:",
        ] {
            assert!(
                !body.contains(breakout),
                "attribute breakout {breakout:?} leaked"
            );
        }
        // The escaped forms must be present (chars were kept, just neutralized).
        assert!(body.contains("&lt;") || body.contains("&amp;") || body.contains("&quot;"));
    }
}

/// Count well-formedness / structural invariants on a real PDF's raw bytes.
fn pdf_is_structurally_sound(pdf: &[u8]) -> Result<(), String> {
    if !pdf.starts_with(b"%PDF-") {
        return Err("missing %PDF- header".into());
    }
    let text = String::from_utf8_lossy(pdf);
    if !text.trim_end().ends_with("%%EOF") {
        return Err("missing %%EOF trailer".into());
    }
    if !text.contains("startxref") {
        return Err("missing startxref".into());
    }
    // Every indirect object opens with "<n> 0 obj" and closes with "endobj".
    let opens = text.matches(" 0 obj").count();
    let closes = text.matches("endobj").count();
    if opens != closes {
        return Err(format!(
            "unbalanced objects: {opens} ' 0 obj' vs {closes} 'endobj'"
        ));
    }
    // Tagged-PDF contract: the structure tree root must be present.
    if !text.contains("/StructTreeRoot") {
        return Err("missing /StructTreeRoot (not a tagged PDF)".into());
    }
    Ok(())
}

#[test]
fn generated_pdf_is_structurally_sound_and_deterministic() {
    let mut rng = Lcg::new(0x0dfd_5a1e_de7e_4321);
    for _ in 0..40 {
        let blocks = 1 + rng.next_usize(6);
        let src = generated_document(&mut rng, blocks);
        let a = render_pdf_pinned(&src);
        let b = render_pdf_pinned(&src);
        assert_eq!(a, b, "pinned-epoch PDF must be byte-identical across runs");
        if let Err(why) = pdf_is_structurally_sound(&a) {
            panic!("PDF structural invariant violated: {why}\nsource:\n{src}");
        }
    }
}

#[test]
fn small_tagged_pdf_balances_marked_content_operators() {
    // A small document keeps its content stream uncompressed, so the marked-content
    // operators are visible in the raw bytes. Each BDC/BMC must have a matching EMC.
    let pdf = render_pdf_pinned("# Title\n\nA paragraph.\n\n- item one\n- item two\n\n> a quote\n");
    let text = String::from_utf8_lossy(&pdf);
    let opens = text.matches("BDC").count() + text.matches("BMC").count();
    let closes = text.matches("EMC").count();
    assert!(opens > 0, "a tagged PDF must emit marked content");
    assert_eq!(
        opens, closes,
        "marked content must balance: {opens} open vs {closes} EMC"
    );
}
