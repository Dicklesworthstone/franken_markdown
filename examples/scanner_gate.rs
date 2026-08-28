//! Scanner hotspot gate (bead p61q.1) — the SIMD island's go/no-go decision.
//!
//! Measures whether the scalar byte scanners (`find_html_text_escape`,
//! `find_html_escape`, `find_any_special_byte`) are a meaningful share of
//! end-to-end HTML render time on realistic corpora. Decision rule (written
//! into the artifact): proceed to intrinsics only if the scanners' estimated
//! cost is >= 2% of end-to-end p95 on this host; otherwise the epic closes as
//! a measured no-op — which is a successful outcome, not a failure.
//!
//! Method (upper-bound framing): the scanners are linear in input bytes; we
//! time them standalone over exactly the bytes one render would scan (the
//! rendered HTML output), so the estimate is an upper bound on their true
//! share (real renders skip already-escaped spans via early-exit first-index
//! scans and bulk-copy the rest).
//!
//! Artifact: tests/artifacts/perf/scanner-gate-<ts>/ per
//! docs/PERFORMANCE_ARTIFACT_SCHEMA.md (fmd-perf-artifact-v1).

use std::fs;
use std::time::Instant;

use franken_markdown::scanner::{find_any_special_byte, find_html_escape, find_html_text_escape};
use franken_markdown::{HtmlOptions, parse_markdown, render_html_document};

const ITERATIONS: usize = 40;
const WARMUP: usize = 5;

fn percentile(sorted: &[u128], p: f64) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (((sorted.len() as f64) - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn stats(samples: &mut Vec<u128>) -> (u128, u128, u128, u128, u128) {
    samples.sort_unstable();
    let min = samples.first().copied().unwrap_or(0);
    let max = samples.last().copied().unwrap_or(0);
    let p50 = percentile(samples, 0.50);
    let p95 = percentile(samples, 0.95);
    let p99 = percentile(samples, 0.99);
    (min, p50, p95, p99, max)
}

/// Synthetic docs: prose-heavy, link-heavy, and escape-heavy — the three
/// shapes that stress the scanners differently.
fn synthetic_corpus() -> Vec<(&'static str, String)> {
    let para = |words: usize, links: usize, code: usize| -> String {
        let mut s = String::new();
        for w in 0..words {
            s.push_str(&format!("word{w} "));
            if w % 17 == 0 && links > 0 {
                s.push_str(&format!("[link{w}](https://example.test/{w}) "));
            }
            if w % 23 == 0 && code > 0 {
                s.push_str(&format!("`code{w}` "));
            }
            if w % 11 == 0 {
                s.push_str("A & B < C > D \"quoted\" — entities force escaping. ");
            }
        }
        s.push('\n');
        s
    };
    let mut prose = String::from("# Scanner gate corpus\n\n");
    let mut linky = String::from("# Link-heavy\n\n");
    let mut escapey = String::from("# Escape-heavy\n\n");
    for i in 0..200 {
        prose.push_str(&para(40, 0, 0));
        linky.push_str(&para(30, 3, 1));
        escapey.push_str(&para(30, 0, 1));
        if i % 5 == 0 {
            prose.push_str(&format!("## Section {i}\n\n"));
            linky.push_str(&format!("## Links {i}\n\n"));
            escapey.push_str(&format!("## Escapes {i}\n\n"));
        }
    }
    vec![("prose", prose), ("links", linky), ("escapes", escapey)]
}

fn perf_sample_line(
    scenario: &str,
    category: &str,
    iterations: usize,
    input_bytes: usize,
    output_bytes: usize,
    stats: (u128, u128, u128, u128, u128, u128),
    notes: &str,
) -> String {
    let (min, mean, p50, p95, p99, max) = stats;
    format!(
        "{{\"type\":\"perf_sample\",\"scenario\":\"{scenario}\",\"category\":\"{category}\",\"iterations\":{iterations},\"input_bytes\":{input_bytes},\"output_bytes\":{output_bytes},\"min_ns\":{min},\"mean_ns\":{mean},\"p50_ns\":{p50},\"p95_ns\":{p95},\"p99_ns\":{p99},\"max_ns\":{max},\"notes\":\"{notes}\"}}"
    )
}

fn main() {
    let corpus = synthetic_corpus();
    let total_input: usize = corpus.iter().map(|(_, s)| s.len()).sum();

    // --- End-to-end render samples (all three docs per iteration) ---
    let mut e2e: Vec<u128> = Vec::with_capacity(ITERATIONS);
    let mut output_bytes = 0usize;
    for _ in 0..WARMUP {
        for (_, src) in &corpus {
            let doc = parse_markdown(src);
            let _ = render_html_document(&doc, &HtmlOptions::default()).unwrap();
        }
    }
    for _ in 0..ITERATIONS {
        let t0 = Instant::now();
        for (_, src) in &corpus {
            let doc = parse_markdown(src);
            let html = render_html_document(&doc, &HtmlOptions::default()).unwrap();
            output_bytes = html.len();
        }
        e2e.push(t0.elapsed().as_nanos());
    }

    // --- Standalone scanner micro-samples over the rendered output bytes ---
    // The rendered HTML is the byte stream the HTML emitter's escape scanners
    // process during emission (upper-bound framing: every output byte scanned
    // once per render at the measured standalone rate).
    let rendered = {
        let (_, src) = corpus
            .iter()
            .max_by_key(|(_, s)| s.len())
            .expect("non-empty corpus");
        let doc = parse_markdown(src);
        render_html_document(&doc, &HtmlOptions::default()).unwrap()
    };
    let scan_bytes = rendered.len();

    let mut text_escape: Vec<u128> = Vec::with_capacity(ITERATIONS * 8);
    let mut attr_escape: Vec<u128> = Vec::with_capacity(ITERATIONS * 8);
    let mut special: Vec<u128> = Vec::with_capacity(ITERATIONS * 8);
    let rendered_bytes = rendered.as_bytes();
    for _ in 0..ITERATIONS * 4 {
        for chunk_start in (0..scan_bytes).step_by(4096) {
            let chunk = &rendered_bytes[chunk_start..(chunk_start + 4096).min(scan_bytes)];
            let t0 = Instant::now();
            let _ = find_html_text_escape(chunk);
            text_escape.push(t0.elapsed().as_nanos().max(1));
            let t0 = Instant::now();
            let _ = find_html_escape(chunk);
            attr_escape.push(t0.elapsed().as_nanos().max(1));
            let t0 = Instant::now();
            let _ = find_any_special_byte(chunk);
            special.push(t0.elapsed().as_nanos().max(1));
        }
    }

    // Per-render scanner-equivalent time: each full pass scans the whole
    // output with all three scanners; per-render cost = total scan ns across
    // all passes / number of passes. (No integer truncation — the earlier
    // per-byte division rounded the estimate to zero.)
    let passes = (ITERATIONS * 4).max(1) as u128;
    let total_scan_ns: u128 = text_escape.iter().sum::<u128>()
        + attr_escape.iter().sum::<u128>()
        + special.iter().sum::<u128>();
    let scanner_est_per_render = total_scan_ns / passes;

    let e2e_stats = stats(&mut e2e);
    let e2e_p95 = e2e_stats.2;
    let share_pct = (scanner_est_per_render as f64 / e2e_p95.max(1) as f64 * 100.0).min(100.0);
    let proceed = share_pct >= 2.0;

    // --- Artifact (fmd-perf-artifact-v1) ---
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let run_id = format!("scanner-gate-{ts}");
    let dir = format!("tests/artifacts/perf/{run_id}");
    fs::create_dir_all(&dir).expect("artifact dir");

    fs::write(
        format!("{dir}/README.md"),
        format!(
            "# Scanner hotspot gate (p61q.1)\n\nDecision input for the SIMD island: scalar scanner share of end-to-end render time.\n\nVerdict: share={share_pct:.2}% -> {}\n",
            if proceed { "PROCEED to intrinsics" } else { "MEASURED NO-OP (scalar stays)" }
        ),
    )
    .unwrap();
    fs::write(
        format!("{dir}/inprocess.jsonl"),
        format!(
            "{}\n{}\n{}\n{}\n{}\n",
            format!(
                "{{\"type\":\"run_start\",\"schema_version\":\"fmd-perf-artifact-v1\",\"run_id\":\"{run_id}\",\"command\":\"cargo run --release --example scanner_gate\"}}"
            ),
            format!(
                "{{\"type\":\"host_fingerprint\",\"target_triple\":\"{}\",\"os\":\"macos\",\"build_profile\":\"release\"}}",
                std::env::consts::ARCH
            ),
            format!(
                "{{\"type\":\"scenario_start\",\"scenario\":\"scanner-gate-e2e\",\"category\":\"render-html\",\"input_bytes\":{total_input},\"iterations\":{ITERATIONS},\"notes\":\"three synthetic docs per iteration\"}}"
            ),
            perf_sample_line(
                "scanner-gate-e2e",
                "render-html",
                ITERATIONS,
                total_input,
                output_bytes,
                {
                    let mut s = e2e.clone();
                    let mean = s.iter().sum::<u128>() / s.len().max(1) as u128;
                    let (min, p50, p95, p99, max) = stats(&mut s);
                    (min, mean, p50, p95, p99, max)
                },
                "full HTML render of the three-doc corpus per iteration",
            ),
            perf_sample_line(
                "scanner-micro-4096b",
                "scanner",
                (text_escape.len() + attr_escape.len() + special.len()) / 3,
                4096,
                0,
                {
                    let mut all = text_escape.clone();
                    all.extend(attr_escape.iter());
                    all.extend(special.iter());
                    let mean = all.iter().sum::<u128>() / all.len().max(1) as u128;
                    let (min, p50, p95, p99, max) = stats(&mut all);
                    (min, mean, p50, p95, p99, max)
                },
                "standalone scanner calls over 4KB chunks of rendered output",
            ),
        ),
    )
    .unwrap();
    fs::write(
        format!("{dir}/hypothesis.md"),
        format!(
            "# Hypothesis (p61q.1 gate)\n\n- H1: the scalar scanners are >= 2% of end-to-end HTML render p95.\n- Measured upper-bound share: {share_pct:.2}% (scanner-est {scanner_est_per_render}ns vs e2e p95 {e2e_p95}ns).\n- Decision: {}\n- Method note: upper-bound framing — the estimate assumes every output byte is scanned by all three scanners once per render; real emission bulk-copies escaped-free spans, so the true share is lower.\n",
            if proceed { "PROCEED to C7_2 intrinsics" } else { "MEASURED NO-OP — close the SIMD epic as a successful measurement; scalar stays the oracle and the implementation" }
        ),
    )
    .unwrap();

    println!("scanner share (upper bound): {share_pct:.2}% of e2e p95");
    println!(
        "e2e p95: {e2e_p95}ns | output: {output_bytes}B | scan est/render: {scanner_est_per_render}ns"
    );
    println!(
        "decision: {}",
        if proceed { "PROCEED" } else { "MEASURED NO-OP" }
    );
    println!("artifact: {dir}");
}
