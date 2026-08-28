//! Four grammar-generator invariants (bead 2c72.2).
//!
//! (a) parse + HTML render never panics
//! (b) HTML text-extract re-parse converges
//! (c) spanned parse: spans in-bounds and non-decreasing
//! (d) double-render byte identity (HTML always; PDF on a subset)
//!
//! Counts: 10_000 parse/span docs (cheap); 2_000 HTML; 200 PDF. Override with
//! `FMD_PROPTEST_N`. Failures print `seed=` plus a source prefix.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use franken_markdown::{
    ADVERSARIES, GenOptions, HtmlOptions, PdfOptions, SourceSpan, adversarial, generate,
    parse_markdown, parse_markdown_spanned, render_html_document, render_pdf_document,
};

const PARSE_N: u64 = 10_000;
const HTML_N: u64 = 2_000;
const PDF_N: u64 = 200;
const ROUND_N: u64 = 2_000;
const SEEDS_PATH: &str = "tests/fixtures/proptest/seeds.md";

fn log_check(id: &str, subject: &str, outcome: &str) {
    eprintln!("check={id} subject={subject} outcome={outcome}");
}

fn assert_ok(id: &str, subject: &str, ok: bool, detail: &str) {
    if ok {
        log_check(id, subject, "PASS");
    } else {
        log_check(id, subject, "FAIL");
        panic!("{id} failed for `{subject}`: {detail}");
    }
}

fn n_from_env(default: u64) -> u64 {
    std::env::var("FMD_PROPTEST_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

fn opts() -> GenOptions {
    GenOptions {
        max_bytes: 2048,
        max_depth: 3,
        max_blocks: 12,
        verbose: false,
    }
}

fn pdf_opts() -> PdfOptions {
    PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    }
}

fn fail_blob(seed: &str, src: &str) -> String {
    let prefix: String = src.chars().take(240).collect();
    format!("seed={seed} bytes={} prefix={prefix:?}", src.len())
}

fn span_ok(span: SourceSpan, src: &str) -> bool {
    span.start <= span.end && span.end <= src.len() && src.get(span.start..span.end).is_some()
}

/// Visible text inside `<main class="fmd">`.
fn article_text(html: &str) -> String {
    const OPEN: &str = "<main class=\"fmd\">";
    const CLOSE: &str = "</main>";
    let start = html.find(OPEN).map(|i| i + OPEN.len()).unwrap_or(0);
    let rest = html.get(start..).unwrap_or("");
    let end = rest.find(CLOSE).unwrap_or(rest.len());
    html_text(rest.get(..end).unwrap_or(""))
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "table"
            | "thead"
            | "tbody"
            | "blockquote"
            | "pre"
            | "hr"
            | "section"
            | "article"
            | "main"
            | "br"
            | "figure"
            | "figcaption"
            | "dl"
            | "dt"
            | "dd"
    )
}

fn push_block_break(out: &mut String) {
    if out.is_empty() {
        return;
    }
    while out.ends_with(' ') {
        out.pop();
    }
    if out.ends_with("\n\n") {
        return;
    }
    if out.ends_with('\n') {
        out.push('\n');
    } else {
        out.push_str("\n\n");
    }
}

fn push_cell_space(out: &mut String) {
    if out.is_empty() || out.ends_with(' ') || out.ends_with('\n') {
        return;
    }
    out.push(' ');
}

fn tag_name(tag: &str) -> String {
    tag.trim()
        .trim_start_matches('/')
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Visible text of `html`. Block tags become paragraph breaks so extract →
/// parse → render → extract converges; pretty-print whitespace is ignored.
fn html_text(html: &str) -> String {
    let mut out = String::new();
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for n in chars.by_ref() {
                if n == '>' {
                    break;
                }
                tag.push(n);
            }
            let name = tag_name(&tag);
            if is_block_tag(&name) {
                push_block_break(&mut out);
            } else if name == "td" || name == "th" {
                push_cell_space(&mut out);
            }
            continue;
        }
        if c == '&' {
            let mut ent = String::new();
            while let Some(&n) = chars.peek() {
                if n == ';' || ent.len() > 8 {
                    break;
                }
                ent.push(n);
                chars.next();
            }
            if chars.peek() == Some(&';') {
                chars.next();
            }
            let decoded = match ent.as_str() {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "nbsp" => ' ',
                _ => {
                    out.push('&');
                    out.push_str(&ent);
                    continue;
                }
            };
            if decoded.is_whitespace() {
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
            } else {
                out.push(decoded);
            }
            continue;
        }
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                out.push(' ');
            }
            continue;
        }
        out.push(c);
    }
    out.trim().to_owned()
}

fn check_no_panic(src: &str, seed: &str) {
    let owned = src.to_string();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let doc = parse_markdown(&owned);
        render_html_document(&doc, &HtmlOptions::default()).expect("html render");
    }));
    assert_ok("no-panic", seed, result.is_ok(), &fail_blob(seed, src));
}

fn check_spans(src: &str, seed: &str) {
    let spanned = parse_markdown_spanned(src);
    assert_ok(
        "span-len",
        seed,
        spanned.source_len == src.len(),
        &format!("source_len={} input={}", spanned.source_len, src.len()),
    );
    let mut prev_start = 0usize;
    for (i, block) in spanned.blocks.iter().enumerate() {
        let ok = span_ok(block.span, src) && block.span.start >= prev_start;
        assert_ok(
            "span-mono",
            &format!("{seed}#{i}"),
            ok,
            &format!("span={:?} prev_start={prev_start}", block.span),
        );
        prev_start = block.span.start;
    }
    for (i, diag) in spanned.diagnostics.iter().enumerate() {
        assert_ok(
            "span-diag",
            &format!("{seed}#d{i}"),
            span_ok(diag.span, src),
            &format!("{:?}", diag.span),
        );
    }
}

fn check_html_determinism(src: &str, seed: &str) {
    let doc = parse_markdown(src);
    let a = render_html_document(&doc, &HtmlOptions::default()).expect("html a");
    let b = render_html_document(&doc, &HtmlOptions::default()).expect("html b");
    assert_ok("html-det", seed, a == b, &fail_blob(seed, src));
}

fn check_pdf_determinism(src: &str, seed: &str) {
    let doc = parse_markdown(src);
    let opts = pdf_opts();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let a = render_pdf_document(&doc, &opts).expect("pdf a");
        let b = render_pdf_document(&doc, &opts).expect("pdf b");
        (a, b)
    }));
    match result {
        Ok((a, b)) => assert_ok("pdf-det", seed, a == b, &fail_blob(seed, src)),
        Err(_) => assert_ok("pdf-det", seed, false, &fail_blob(seed, src)),
    }
}

fn first_text_diff(a: &str, b: &str) -> String {
    let mut ia = a.char_indices();
    let mut ib = b.char_indices();
    loop {
        match (ia.next(), ib.next()) {
            (Some((_, ca)), Some((_, cb))) if ca == cb => continue,
            (Some((i, ca)), Some((_, cb))) => {
                let lo = i.saturating_sub(40);
                return format!(
                    "text_len {} vs {} at byte {i} {ca:?} vs {cb:?} left={:?} right={:?}",
                    a.len(),
                    b.len(),
                    a.get(lo..(i + 40).min(a.len())).unwrap_or(""),
                    b.get(lo..(i + 40).min(b.len())).unwrap_or("")
                );
            }
            (None, None) => return format!("texts equal len={}", a.len()),
            (Some((i, _)), None) => {
                return format!(
                    "text2 shorter at {i} extra_left={:?}",
                    &a[i..a.len().min(i + 40)]
                );
            }
            (None, Some((i, _))) => {
                return format!(
                    "text1 shorter at {i} extra_right={:?}",
                    &b[i..b.len().min(i + 40)]
                );
            }
        }
    }
}

fn first_block_diff(a: &franken_markdown::Document, b: &franken_markdown::Document) -> String {
    let n = a.blocks.len().max(b.blocks.len());
    for i in 0..n {
        if a.blocks.get(i) != b.blocks.get(i) {
            return format!(
                "block[{i}/{} vs {}] left={:?} right={:?}",
                a.blocks.len(),
                b.blocks.len(),
                a.blocks.get(i),
                b.blocks.get(i)
            );
        }
    }
    "blocks equal".to_owned()
}

fn check_round_trip_converge(src: &str, seed: &str) {
    const ROUNDS: usize = 4;
    let html_opts = HtmlOptions::default();
    let text =
        article_text(&render_html_document(&parse_markdown(src), &html_opts).expect("html0"));
    let mut prev = parse_markdown(&text);
    let mut last_text = text;
    for round in 1..=ROUNDS {
        let html = render_html_document(&prev, &html_opts).expect("html round");
        let next_text = article_text(&html);
        let next = parse_markdown(&next_text);
        if next == prev {
            assert_ok(
                "round-trip",
                seed,
                true,
                &format!("{} fixpoint_round={round}", fail_blob(seed, src)),
            );
            return;
        }
        last_text = next_text;
        prev = next;
    }
    let again_html = render_html_document(&prev, &html_opts).expect("html tail");
    let again_text = article_text(&again_html);
    let again = parse_markdown(&again_text);
    let detail = format!(
        "{} no fixpoint in {ROUNDS} rounds {} {}",
        fail_blob(seed, src),
        first_text_diff(&last_text, &again_text),
        first_block_diff(&prev, &again)
    );
    assert_ok("round-trip", seed, prev == again, &detail);
}

fn load_committed_seeds() -> Vec<(String, String)> {
    let raw = std::fs::read_to_string(SEEDS_PATH).unwrap_or_default();
    let mut out = Vec::new();
    let mut name = String::from("anon");
    let mut buf = String::new();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("=== ") {
            if let Some(label) = rest.strip_suffix(" ===") {
                if (!buf.is_empty() || (out.is_empty() && !name.is_empty()))
                    && !(name == "anon" && buf.is_empty())
                {
                    out.push((name.clone(), buf.clone()));
                }
                name = label.trim().to_owned();
                buf.clear();
                continue;
            }
        }
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(line);
    }
    out.push((name, buf));
    out
}

#[test]
fn committed_regression_seeds() {
    let seeds = load_committed_seeds();
    assert_ok(
        "corpus-nonempty",
        SEEDS_PATH,
        seeds.len() >= 5,
        &format!("n={}", seeds.len()),
    );
    for (name, src) in &seeds {
        check_no_panic(src, name);
        check_spans(src, name);
        check_html_determinism(src, name);
        check_round_trip_converge(src, name);
    }
}

#[test]
fn parse_span_invariants_over_generated_corpus() {
    let n = n_from_env(PARSE_N);
    let opts = opts();
    eprintln!("md_proptest phase=parse_span n={n}");
    for seed in 0..n {
        let src = generate(seed, &opts);
        let label = format!("g{seed}");
        let result = catch_unwind(AssertUnwindSafe(|| {
            check_spans(&src, &label);
            let _ = parse_markdown(&src);
        }));
        assert_ok(
            "parse-span-unwind",
            &label,
            result.is_ok(),
            &fail_blob(&label, &src),
        );
    }
    for kind in ADVERSARIES {
        let src = adversarial(*kind, 2048);
        let label = format!("adv-{}", kind.name());
        check_spans(&src, &label);
        check_no_panic(&src, &label);
    }
    log_check("parse-span-summary", &format!("n={n}"), "PASS");
}

#[test]
fn html_no_panic_and_determinism() {
    let n = n_from_env(HTML_N).min(HTML_N);
    let opts = opts();
    eprintln!("md_proptest phase=html n={n}");
    for seed in 0..n {
        let src = generate(seed, &opts);
        let label = format!("h{seed}");
        check_no_panic(&src, &label);
        check_html_determinism(&src, &label);
    }
    log_check("html-summary", &format!("n={n}"), "PASS");
}

#[test]
fn html_round_trip_converges() {
    let n = n_from_env(ROUND_N).min(ROUND_N);
    let opts = opts();
    eprintln!("md_proptest phase=round_trip n={n}");
    for seed in 0..n {
        let src = generate(seed, &opts);
        let label = format!("r{seed}");
        let result = catch_unwind(AssertUnwindSafe(|| check_round_trip_converge(&src, &label)));
        assert_ok(
            "round-trip-unwind",
            &label,
            result.is_ok(),
            &fail_blob(&label, &src),
        );
    }
    log_check("round-trip-summary", &format!("n={n}"), "PASS");
}

#[test]
fn pdf_determinism_subset() {
    let n = n_from_env(PDF_N).min(PDF_N);
    let opts = opts();
    eprintln!("md_proptest phase=pdf n={n}");
    for seed in 0..n {
        let src = generate(seed, &opts);
        let label = format!("p{seed}");
        check_pdf_determinism(&src, &label);
    }
    log_check("pdf-summary", &format!("n={n}"), "PASS");
}
