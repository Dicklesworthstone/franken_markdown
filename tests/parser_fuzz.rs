//! Deterministic generative fuzz harness for the parser + renderers (bead grn.5.1).
//!
//! No third-party fuzz crate (cargo-fuzz would violate the dependency policy and
//! the determinism bar). Instead a seeded LCG generates thousands of adversarial
//! inputs — Markdown-significant byte soup, raw arbitrary bytes via
//! `from_utf8_lossy`, and escalating structural-nesting stressors — and asserts
//! the engine's robustness invariants on every one:
//!
//!   1. it never PANICS (parse, spanned parse, HTML render, PDF render);
//!   2. it always TERMINATES (recursion/size bounds hold — the test completing,
//!      with deep-nesting stressors, is the proof; a regression would hang or
//!      stack-overflow the process);
//!   3. it always yields a structurally valid AST with BALANCED SPANS: every
//!      top-level block span and diagnostic span satisfies
//!      `start <= end <= source_len` and lands on a valid char boundary.
//!
//! Determinism: a fixed seed list makes the whole corpus reproducible.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use franken_markdown::{
    HtmlOptions, PdfOptions, SourceSpan, parse_markdown, parse_markdown_spanned,
    render_html_document, render_pdf_document,
};

/// Small deterministic PRNG (same family as the metamorphic suite).
struct Lcg {
    state: u64,
}
impl Lcg {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // xorshift the high bits back in for better low-bit quality.
        self.state ^ (self.state >> 27)
    }
    fn below(&mut self, max: usize) -> usize {
        (self.next_u64() % max as u64) as usize
    }
}

/// Bytes the parser treats as structurally significant, over-represented so the
/// fuzzer stresses block/inline machinery rather than mostly emitting prose.
const SIGNIFICANT: &[u8] = b"#*_`>-+[]()|!\\~ \t\n.0123456789abcXYZ<&;:=\"'/{}";

/// Build a random "Markdown-ish" UTF-8 string from significant bytes plus the odd
/// multi-byte scalar (so char-boundary handling is exercised).
fn random_markdownish(rng: &mut Lcg, len: usize) -> String {
    let mut s = String::with_capacity(len * 2);
    for _ in 0..len {
        let roll = rng.below(100);
        if roll < 90 {
            s.push(SIGNIFICANT[rng.below(SIGNIFICANT.len())] as char);
        } else if roll < 96 {
            // a multi-byte scalar (emoji / CJK / combining / bidi control)
            let scalars = [
                'é', '汉', '🦀', '\u{0301}', '\u{202e}', '\u{2066}', '\u{200b}',
            ];
            s.push(scalars[rng.below(scalars.len())]);
        } else {
            // a run of one significant byte to provoke deep delimiter runs
            let b = SIGNIFICANT[rng.below(SIGNIFICANT.len())] as char;
            for _ in 0..rng.below(8) {
                s.push(b);
            }
        }
    }
    s
}

/// Build arbitrary raw bytes, then lossily decode to a real `&str` (the library
/// contract is UTF-8 in; the CLI rejects non-UTF-8, but the engine must survive
/// any string the host hands it).
fn random_bytes_lossy(rng: &mut Lcg, len: usize) -> String {
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
        bytes.push((rng.next_u64() & 0xff) as u8);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn span_ok(span: SourceSpan, src: &str) -> bool {
    span.start <= span.end && span.end <= src.len() && span.slice(src).is_some()
}

/// The full invariant battery for one input. Panics here fail the test loudly with
/// the offending input echoed.
fn assert_robust(src: &str) {
    let owned = src.to_string();
    let result = catch_unwind(AssertUnwindSafe(|| {
        // 1) plain parse never panics
        let _doc = parse_markdown(&owned);

        // 2) spanned parse + balanced-span invariant
        let spanned = parse_markdown_spanned(&owned);
        assert_eq!(
            spanned.source_len,
            owned.len(),
            "source_len must equal input length"
        );
        for block in &spanned.blocks {
            assert!(
                span_ok(block.span, &owned),
                "block span out of range: {:?}",
                block.span
            );
        }
        for diag in &spanned.diagnostics {
            assert!(
                span_ok(diag.span, &owned),
                "diagnostic span out of range: {:?}",
                diag.span
            );
        }

        // 3) HTML render never panics and is non-null
        let doc = parse_markdown(&owned);
        let _html = render_html_document(&doc, &HtmlOptions::default())
            .expect("HTML render must not error on arbitrary input");
    }));
    assert!(
        result.is_ok(),
        "engine panicked on input ({} bytes): {:?}",
        src.len(),
        src.chars().take(120).collect::<String>(),
    );
}

#[test]
fn fuzz_random_markdownish_inputs_never_panic_with_balanced_spans() {
    // Many seeds, varied lengths. Deterministic + reproducible.
    let mut count = 0;
    for seed in 0..600u64 {
        let mut rng = Lcg::new(seed.wrapping_mul(2_654_435_761));
        let len = 1 + rng.below(400);
        assert_robust(&random_markdownish(&mut rng, len));
        count += 1;
    }
    assert!(count >= 600);
}

#[test]
fn fuzz_arbitrary_byte_soup_is_survived() {
    for seed in 0..400u64 {
        let mut rng = Lcg::new(seed ^ 0xDEAD_BEEF);
        let len = 1 + rng.below(512);
        assert_robust(&random_bytes_lossy(&mut rng, len));
    }
}

#[test]
fn fuzz_deep_structural_nesting_stays_within_bounds() {
    // Escalating depths of the constructs most likely to recurse. The parser must
    // terminate and not stack-overflow. 2000 is a deliberate regression guard: a
    // debug build overflowed at exactly this depth on nested `>` BEFORE the
    // MAX_BLOCK_NESTING_DEPTH cap landed, so this asserts the bound holds. (Higher
    // depths only add O(n) degraded-parse time without more signal.)
    let depths = [64usize, 256, 1024, 2000];
    for &d in &depths {
        assert_robust(&">".repeat(d)); // nested blockquote markers
        assert_robust(&"#".repeat(d)); // pathological ATX run
        assert_robust(&("> ".repeat(d) + "x"));
        assert_robust(&("- ".repeat(d) + "item"));
        assert_robust(&"*".repeat(d)); // emphasis delimiter run
        assert_robust(&"`".repeat(d)); // code fence/span delimiter run
        assert_robust(&("[".repeat(d) + &"]".repeat(d))); // bracket nesting
        assert_robust(&("(".repeat(d) + &")".repeat(d)));
        assert_robust(&"|".repeat(d)); // table pipe run
        assert_robust(&("\t".repeat(d) + "indented")); // deep indentation
        assert_robust(&"\n".repeat(d)); // many blank lines
    }
}

#[test]
fn fuzz_pathological_emphasis_runs_stay_linear_and_bounded() {
    // Regression for two emphasis-resolution DoS classes that only surface at
    // large scale (the depth-2000 nesting test above is too small to trigger
    // either):
    //   * a single huge delimiter run (`***…***x***…***`) nests one Strong per
    //     pair. Before the fix this was quadratic (each pair re-cloned the
    //     growing subtree) AND built a tree deep enough to overflow the stack at
    //     render/drop time. Now the wrap is a move (linear) and bounded by
    //     MAX_INLINE_NESTING_DEPTH.
    //   * alternating both-open-and-close runs (`*_*_…`) made every closer walk
    //     back over the opposite delimiter, quadratically. Now a linear back-walk
    //     budget bounds it.
    // The proof is that this test COMPLETES: a regression would hang (quadratic
    // on 10^5-scale input) or abort the process (stack overflow).
    let star = "*".repeat(60_000);
    assert_robust(&format!("{star}x{star}"));
    let triple = "***".repeat(30_000);
    assert_robust(&format!("{triple}x{triple}"));
    let under = "_".repeat(80_000);
    assert_robust(&format!("{under}word{under}"));
    let alt_open = "*_".repeat(80_000);
    let alt_close = "_*".repeat(80_000);
    assert_robust(&format!("{alt_open}x{alt_close}"));
}

#[test]
fn fuzz_pathological_bracket_runs_stay_linear() {
    // Regression for the O(n^2) bracket/link scan: a line of deeply nested
    // brackets (`[[[…x…]]]`) once called an O(n) forward scan (and a full-span
    // String collect) from every `[`. With one-pass bracket-pair precomputation
    // and the CommonMark 999-char label cap this is linear. Completing is proof:
    // a regression would hang on 10^5-scale bracket input.
    let open = "[".repeat(80_000);
    let close = "]".repeat(80_000);
    assert_robust(&format!("{open}x{close}"));
    // Also the reference-style variant that used to collect each span.
    assert_robust(&format!("{open}x{close}(u)"));
}

#[test]
fn fuzz_pdf_render_survives_a_sample_of_adversarial_inputs() {
    // PDF rendering is heavier, so sample rather than running the full corpus.
    for seed in 0..40u64 {
        let mut rng = Lcg::new(seed ^ 0x00C0_FFEE);
        let len = 1 + rng.below(300);
        let src = random_markdownish(&mut rng, len);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let doc = parse_markdown(&src);
            render_pdf_document(&doc, &PdfOptions::default())
                .expect("PDF render must not error on arbitrary input")
        }));
        assert!(result.is_ok(), "PDF render panicked on seed {seed}");
        let pdf = result.unwrap();
        assert!(pdf.starts_with(b"%PDF-"), "PDF output must be well-formed");
    }
    // A few structural stressors through the PDF path too.
    for s in ["> ".repeat(2000), "- ".repeat(2000) + "x", "`".repeat(2000)] {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let doc = parse_markdown(&s);
            render_pdf_document(&doc, &PdfOptions::default())
        }));
        assert!(
            result.is_ok(),
            "PDF render panicked on a structural stressor"
        );
    }
}
