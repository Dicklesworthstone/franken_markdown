//! Isolated scanner share vs end-to-end HTML render time (bead p61q.1).
//!
//! Named scanners: `find_html_text_escape`, `find_html_escape`,
//! `find_any_special_byte`. Gate: proceed to SIMD only if any named scanner's
//! median p95 is at least 2% of parse+HTML p95 on at least one document.
//!
//! Stdout is JSONL. Diagnostics go to stderr.

use franken_markdown::{
    HtmlOptions, PdfOptions, find_any_special_byte, find_html_escape, find_html_text_escape,
    parse_markdown, render_html_document, render_pdf_document, scan_markdown_line,
};
use std::env;
use std::fs;
use std::hint::black_box;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const BEAD_ID: &str = "br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-p61q.1";
const SCHEMA_VERSION: &str = "fmd-perf-artifact-v1";
const THRESHOLD_BP: u128 = 200; // 2.00% in basis points
const GENERATED_LARGE_BYTES: usize = 1_048_576;
const OUTER_DEFAULT: usize = 3;

struct Args {
    html_iters: usize,
    scanner_iters: usize,
    outer: usize,
    out_dir: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = writeln!(io::stderr(), "fmd_scanner_share: {err}");
            ExitCode::from(70)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    let started = Instant::now();
    log_phase(
        "start",
        0,
        &format!(
            "bead={BEAD_ID} outer={} html_iters={} scanner_iters={}",
            args.outer, args.html_iters, args.scanner_iters
        ),
    );

    let prose = read_required(Path::new(
        "tests/fixtures/perf/scanner-share/prose-heavy.md",
    ))?;
    let escape = read_required(Path::new(
        "tests/fixtures/perf/scanner-share/escape-heavy.md",
    ))?;
    let showcase = read_required(Path::new("examples/showcase.md"))?;
    let readme = read_required(Path::new("README.md"))?;
    let generated = generated_from_seed(&prose, GENERATED_LARGE_BYTES);

    if let Some(dir) = &args.out_dir {
        fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let corpus = dir.join("generated-large.md");
        fs::write(&corpus, generated.as_bytes())
            .map_err(|e| format!("write {}: {e}", corpus.display()))?;
        log_phase(
            "dump-generated",
            generated.len(),
            &format!("path={}", corpus.display()),
        );
    }

    let docs: [Doc; 5] = [
        Doc {
            name: "showcase",
            source: &showcase,
            measure_pdf: true,
        },
        Doc {
            name: "readme",
            source: &readme,
            measure_pdf: true,
        },
        Doc {
            name: "prose-heavy",
            source: &prose,
            measure_pdf: true,
        },
        Doc {
            name: "escape-heavy",
            source: &escape,
            measure_pdf: true,
        },
        Doc {
            name: "generated-large",
            source: &generated,
            measure_pdf: false,
        },
    ];

    emit_line(&fingerprint_record()?)?;
    emit_line(&json_object(&[
        ("type", json_str("run_start")),
        ("schema_version", json_str(SCHEMA_VERSION)),
        ("bead_id", json_str(BEAD_ID)),
        ("threshold_bp", json_u128(THRESHOLD_BP)),
        ("outer_runs", json_u128(args.outer as u128)),
        ("html_iters", json_u128(args.html_iters as u128)),
        ("scanner_iters", json_u128(args.scanner_iters as u128)),
        (
            "generated_large_bytes",
            json_u128(GENERATED_LARGE_BYTES as u128),
        ),
        (
            "generated_seed",
            json_str("tests/fixtures/perf/scanner-share/prose-heavy.md"),
        ),
    ]))?;

    let mut hottest: Option<ShareRow> = None;
    for doc in docs {
        log_phase("document", doc.source.len(), doc.name);
        let summary = measure_document(doc, &args)?;
        for row in &summary {
            emit_line(&share_record(row))?;
            if named_scanner(row.scanner)
                && hottest
                    .as_ref()
                    .is_none_or(|cur| row.share_bp > cur.share_bp)
            {
                hottest = Some(row.clone());
            }
        }
    }

    let max_bp = hottest.as_ref().map_or(0, |row| row.share_bp);
    let go = max_bp >= THRESHOLD_BP;
    let verdict = if go { "go" } else { "no-go" };
    emit_line(&json_object(&[
        ("type", json_str("gate_decision")),
        ("bead_id", json_str(BEAD_ID)),
        (
            "rule",
            json_str(
                "proceed to SIMD only if any named scanner median p95 is >= 2% of parse+html p95 on at least one document",
            ),
        ),
        ("threshold_bp", json_u128(THRESHOLD_BP)),
        ("max_share_bp", json_u128(max_bp)),
        ("verdict", json_str(verdict)),
        (
            "hottest_document",
            json_str(hottest.as_ref().map_or("", |row| row.document)),
        ),
        (
            "hottest_scanner",
            json_str(hottest.as_ref().map_or("", |row| row.scanner)),
        ),
        (
            "find_any_special_byte_note",
            json_str(
                "production parse uses scan_markdown_line, not find_any_special_byte; the latter is still the named SIMD-island scalar",
            ),
        ),
    ]))?;
    emit_line(&json_object(&[
        ("type", json_str("run_complete")),
        ("verdict", json_str(verdict)),
        ("max_share_bp", json_u128(max_bp)),
        ("elapsed_ms", json_u128(started.elapsed().as_millis())),
    ]))?;
    log_phase(
        "decision",
        0,
        &format!("verdict={verdict} max_share_bp={max_bp} threshold_bp={THRESHOLD_BP}"),
    );
    Ok(())
}

struct Doc<'a> {
    name: &'static str,
    source: &'a str,
    measure_pdf: bool,
}

#[derive(Clone)]
struct ShareRow {
    document: &'static str,
    scanner: &'static str,
    input_bytes: usize,
    scanner_p50_ns: u128,
    scanner_p95_ns: u128,
    e2e_p50_ns: u128,
    e2e_p95_ns: u128,
    share_bp: u128,
}

fn named_scanner(name: &str) -> bool {
    matches!(
        name,
        "find_html_text_escape" | "find_html_escape" | "find_any_special_byte"
    )
}

fn measure_document(doc: Doc<'_>, args: &Args) -> Result<Vec<ShareRow>, String> {
    let bytes = doc.source.as_bytes();
    let html_opts = HtmlOptions::default();
    let pdf_opts = PdfOptions::default();

    let mut html_p95 = Vec::with_capacity(args.outer);
    let mut html_p50 = Vec::with_capacity(args.outer);
    for _ in 0..args.outer {
        let sample = measure(args.html_iters, || {
            let parsed = parse_markdown(doc.source);
            match render_html_document(&parsed, &html_opts) {
                Ok(html) => black_box(html.len()),
                Err(_) => black_box(0),
            }
        });
        html_p50.push(percentile_ns(&sample, 50));
        html_p95.push(percentile_ns(&sample, 95));
    }
    let e2e_p50 = median_u128(&html_p50);
    let e2e_p95 = median_u128(&html_p95);
    emit_line(&json_object(&[
        ("type", json_str("perf_sample")),
        ("scenario", json_str(doc.name)),
        ("category", json_str("parse+html")),
        ("input_bytes", json_u128(bytes.len() as u128)),
        ("outer_runs", json_u128(args.outer as u128)),
        ("iterations", json_u128(args.html_iters as u128)),
        ("p50_ns", json_u128(e2e_p50)),
        ("p95_ns", json_u128(e2e_p95)),
        ("notes", json_str("median of 3 outer-run p50/p95")),
    ]))?;

    if doc.measure_pdf {
        let mut pdf_p95 = Vec::with_capacity(args.outer);
        for _ in 0..args.outer {
            let sample = measure(args.html_iters.clamp(1, 5), || {
                let parsed = parse_markdown(doc.source);
                match render_pdf_document(&parsed, &pdf_opts) {
                    Ok(pdf) => black_box(pdf.len()),
                    Err(_) => black_box(0),
                }
            });
            pdf_p95.push(percentile_ns(&sample, 95));
        }
        emit_line(&json_object(&[
            ("type", json_str("perf_sample")),
            ("scenario", json_str(doc.name)),
            ("category", json_str("parse+pdf")),
            ("input_bytes", json_u128(bytes.len() as u128)),
            ("p95_ns", json_u128(median_u128(&pdf_p95))),
            (
                "notes",
                json_str("context only; HTML scanners are judged against parse+html"),
            ),
        ]))?;
    }

    type ScannerFn = fn(&[u8]) -> Option<usize>;
    let scanners: [(&str, ScannerFn); 3] = [
        ("find_html_text_escape", find_html_text_escape),
        ("find_html_escape", find_html_escape),
        ("find_any_special_byte", find_any_special_byte),
    ];
    let mut rows = Vec::new();
    for (name, scan) in scanners {
        let mut p50s = Vec::with_capacity(args.outer);
        let mut p95s = Vec::with_capacity(args.outer);
        for _ in 0..args.outer {
            let sample = measure(args.scanner_iters, || {
                black_box(scan(bytes).unwrap_or(usize::MAX))
            });
            p50s.push(percentile_ns(&sample, 50));
            p95s.push(percentile_ns(&sample, 95));
        }
        let scanner_p50 = median_u128(&p50s);
        let scanner_p95 = median_u128(&p95s);
        rows.push(ShareRow {
            document: doc.name,
            scanner: name,
            input_bytes: bytes.len(),
            scanner_p50_ns: scanner_p50,
            scanner_p95_ns: scanner_p95,
            e2e_p50_ns: e2e_p50,
            e2e_p95_ns: e2e_p95,
            share_bp: share_bp(scanner_p95, e2e_p95),
        });
    }

    let lines: Vec<&str> = doc.source.lines().collect();
    let mut line_p95 = Vec::with_capacity(args.outer);
    for _ in 0..args.outer {
        let sample = measure(args.scanner_iters, || {
            let mut acc = 0usize;
            for line in &lines {
                if scan_markdown_line(line).first_special_byte.is_some() {
                    acc = acc.wrapping_add(1);
                }
            }
            black_box(acc)
        });
        line_p95.push(percentile_ns(&sample, 95));
    }
    let line_p95 = median_u128(&line_p95);
    rows.push(ShareRow {
        document: doc.name,
        scanner: "scan_markdown_line",
        input_bytes: bytes.len(),
        scanner_p50_ns: 0,
        scanner_p95_ns: line_p95,
        e2e_p50_ns: e2e_p50,
        e2e_p95_ns: e2e_p95,
        share_bp: share_bp(line_p95, e2e_p95),
    });
    Ok(rows)
}

fn share_record(row: &ShareRow) -> String {
    json_object(&[
        ("type", json_str("share_summary")),
        ("scenario", json_str(row.document)),
        ("scanner", json_str(row.scanner)),
        ("named_gate_scanner", json_bool(named_scanner(row.scanner))),
        ("input_bytes", json_u128(row.input_bytes as u128)),
        ("scanner_p50_ns", json_u128(row.scanner_p50_ns)),
        ("scanner_p95_ns", json_u128(row.scanner_p95_ns)),
        ("e2e_p50_ns", json_u128(row.e2e_p50_ns)),
        ("e2e_p95_ns", json_u128(row.e2e_p95_ns)),
        ("share_bp", json_u128(row.share_bp)),
        ("threshold_bp", json_u128(THRESHOLD_BP)),
        (
            "over_threshold",
            json_bool(named_scanner(row.scanner) && row.share_bp >= THRESHOLD_BP),
        ),
    ])
}

fn share_bp(scanner_ns: u128, e2e_ns: u128) -> u128 {
    scanner_ns
        .saturating_mul(10_000)
        .checked_div(e2e_ns)
        .unwrap_or(0)
}

fn fingerprint_record() -> Result<String, String> {
    let mut features = String::from("{");
    let mut first = true;
    for (name, on) in cpu_features() {
        if !first {
            features.push(',');
        }
        first = false;
        features.push('"');
        features.push_str(name);
        features.push_str("\":");
        features.push_str(if on { "true" } else { "false" });
    }
    features.push('}');
    Ok(json_object(&[
        ("type", json_str("host_fingerprint")),
        ("arch", json_str(env::consts::ARCH)),
        ("os", json_str(env::consts::OS)),
        ("family", json_str(env::consts::FAMILY)),
        ("cpu_features", features),
    ]))
}

fn cpu_features() -> Vec<(&'static str, bool)> {
    let mut out = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        out.push(("sse2", is_x86_feature_detected!("sse2")));
        out.push(("ssse3", is_x86_feature_detected!("ssse3")));
        out.push(("avx2", is_x86_feature_detected!("avx2")));
        out.push(("avx512f", is_x86_feature_detected!("avx512f")));
    }
    #[cfg(target_arch = "aarch64")]
    {
        // NEON is a baseline AArch64 feature on the project's release targets.
        out.push(("neon", true));
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        out.push(("unrecognized_arch", true));
    }
    out
}

fn generated_from_seed(seed: &str, target: usize) -> String {
    let mut out = String::with_capacity(target + seed.len());
    let mut n = 0usize;
    while out.len() < target {
        out.push_str("\n\n## Generated ");
        out.push_str(&n.to_string());
        out.push('\n');
        out.push_str(seed);
        n += 1;
    }
    out.truncate(target);
    out
}

fn read_required(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn parse_args<I>(mut args: I) -> Result<Args, String>
where
    I: Iterator<Item = String>,
{
    let mut html_iters = 8usize;
    let mut scanner_iters = 400usize;
    let mut outer = OUTER_DEFAULT;
    let mut out_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iters" => html_iters = parse_positive(&next_val(&mut args, "--iters")?)?,
            "--scanner-iters" => {
                scanner_iters = parse_positive(&next_val(&mut args, "--scanner-iters")?)?
            }
            "--outer" => outer = parse_positive(&next_val(&mut args, "--outer")?)?,
            "--out-dir" => out_dir = Some(PathBuf::from(next_val(&mut args, "--out-dir")?)),
            "--help" | "-h" => {
                println!(
                    "Usage: fmd_scanner_share [--iters N] [--scanner-iters N] [--outer N] [--out-dir DIR]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(Args {
        html_iters,
        scanner_iters,
        outer,
        out_dir,
    })
}

fn next_val<I: Iterator<Item = String>>(args: &mut I, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive(raw: &str) -> Result<usize, String> {
    let n: usize = raw
        .parse()
        .map_err(|_| format!("expected positive integer, got '{raw}'"))?;
    if n == 0 {
        Err(String::from("counts must be greater than zero"))
    } else {
        Ok(n)
    }
}

fn measure<F>(iterations: usize, mut f: F) -> Vec<Duration>
where
    F: FnMut() -> usize,
{
    let warmup = iterations.min(2);
    for _ in 0..warmup {
        let _ = f();
    }
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let value = f();
        let elapsed = start.elapsed();
        black_box(value);
        durations.push(elapsed);
    }
    durations.sort_unstable();
    durations
}

fn percentile_ns(durations: &[Duration], percentile: usize) -> u128 {
    if durations.is_empty() {
        return 0;
    }
    let p = percentile.min(100);
    let idx = ((durations.len() - 1) * p).div_ceil(100);
    durations[idx].as_nanos()
}

fn median_u128(values: &[u128]) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn emit_line(line: &str) -> Result<(), String> {
    let mut out = io::stdout().lock();
    out.write_all(line.as_bytes())
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
        .map_err(|e| format!("stdout write failed: {e}"))
}

fn log_phase(phase: &str, items: usize, detail: &str) {
    let _ = writeln!(
        io::stderr(),
        "fmd_scanner_share: phase={phase} items={items} {detail}"
    );
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn json_str(value: &str) -> String {
    let mut out = String::from("\"");
    out.push_str(&json_escape(value));
    out.push('"');
    out
}

fn json_u128(value: u128) -> String {
    value.to_string()
}

fn json_bool(value: bool) -> String {
    if value {
        String::from("true")
    } else {
        String::from("false")
    }
}

fn json_object(fields: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(key);
        out.push_str("\":");
        out.push_str(value);
    }
    out.push('}');
    out
}
