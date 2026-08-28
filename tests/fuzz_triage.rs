//! Crash-triage pipeline for coverage-guided fuzzing (m7fs.2).
//!
//! The production engine is not supposed to panic. This file therefore drills
//! the *pipeline* with an injected oracle: a large buffer containing the
//! marker `!PANIC!` "crashes" (the oracle panics), and delta-debugging must
//! shrink it to exactly that marker. The minimized bytes are the same as
//! `tests/fixtures/fuzz-regressions/drill.bin`, which is how a real nightly
//! crash would be promoted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

const MARKER: &[u8] = b"!PANIC!";

fn log_check(id: &str, subject: &str, ok: bool) {
    eprintln!(
        "check id={id} subject={subject} outcome={}",
        if ok { "PASS" } else { "FAIL" }
    );
    assert!(ok, "{id}: {subject}");
}

fn contains_marker(data: &[u8]) -> bool {
    data.windows(MARKER.len()).any(|w| w == MARKER)
}

/// Injected panic used by the drill (one shot). Minimizing uses
/// [`contains_marker`] so libtest is not flooded with panic-hook lines.
fn drill_panic(data: &[u8]) {
    if contains_marker(data) {
        panic!("injected fuzz drill");
    }
}

/// Greedy chunk-deletion minimizer (Zeller-style ddmin, n=2,4,8,…).
fn ddmin(data: Vec<u8>, crashes: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    assert!(crashes(&data), "ddmin requires a crashing starting input");
    let mut cur = data;
    let mut changed = true;
    while changed && cur.len() > 1 {
        changed = false;
        let mut chunk = cur.len() / 2;
        if chunk == 0 {
            chunk = 1;
        }
        while chunk > 0 {
            let mut i = 0;
            while i + chunk <= cur.len() {
                let mut cand = Vec::with_capacity(cur.len() - chunk);
                cand.extend_from_slice(&cur[..i]);
                cand.extend_from_slice(&cur[i + chunk..]);
                if !cand.is_empty() && crashes(&cand) {
                    cur = cand;
                    changed = true;
                } else {
                    i += chunk;
                }
            }
            chunk /= 2;
        }
    }
    cur
}

fn bloated_crash() -> Vec<u8> {
    let mut v = Vec::from(&b"PADDING_HEAD_"[..]);
    v.extend_from_slice(&[0xAA; 64]);
    v.extend_from_slice(MARKER);
    v.extend_from_slice(&[0xBB; 64]);
    v.extend_from_slice(b"_PADDING_TAIL");
    v
}

#[test]
fn drill_oracle_accepts_innocent_bytes() {
    log_check(
        "m7fs.2.oracle.clean",
        "innocent bytes do not crash",
        catch_unwind(AssertUnwindSafe(|| drill_panic(b"hello zlib seed"))).is_ok(),
    );
}

#[test]
#[should_panic(expected = "injected fuzz drill")]
fn drill_oracle_panics_on_the_marker() {
    eprintln!("check id=m7fs.2.oracle.marker subject=marker panics outcome=PASS");
    drill_panic(MARKER);
}

#[test]
fn ddmin_shrinks_injected_panic_to_the_marker() {
    let start = bloated_crash();
    log_check(
        "m7fs.2.ddmin.start",
        &format!("start_len={}", start.len()),
        start.len() > MARKER.len(),
    );
    let min = ddmin(start, contains_marker);
    log_check("m7fs.2.ddmin.bytes", &format!("min={min:?}"), min == MARKER);
}

#[test]
fn promoted_drill_fixture_matches_minimizer_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("tests/fixtures/fuzz-regressions/drill.bin");
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    log_check(
        "m7fs.2.promote.fixture",
        "drill.bin is the minimized marker",
        bytes == MARKER,
    );
    let min = ddmin(bloated_crash(), contains_marker);
    log_check(
        "m7fs.2.promote.roundtrip",
        "minimizer output equals the promoted fixture",
        min == bytes,
    );
}

#[test]
fn engine_regression_bins_do_not_panic() {
    use franken_markdown::text::Font;
    use franken_markdown::{
        HtmlOptions, PdfOptions, parse_markdown, render_html_document, render_pdf_document,
        zlib_decompress,
    };

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("tests/fixtures/fuzz-regressions");
    let mut n = 0usize;
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bin") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        if name == "drill.bin" {
            // Synthetic minimizer fixture, not an engine crash.
            continue;
        }
        n += 1;
        let data = fs::read(&path).unwrap();
        let survived = catch_unwind(AssertUnwindSafe(|| {
            let src = String::from_utf8_lossy(&data);
            let doc = parse_markdown(&src);
            let _ = render_html_document(&doc, &HtmlOptions::default());
            let _ = render_pdf_document(&doc, &PdfOptions::default());
            let _ = zlib_decompress(&data, 64 * 1024);
            if let Ok(font) = Font::parse(data.clone()) {
                let keep: Vec<char> = data.iter().take(8).map(|&b| char::from(b)).collect();
                let _ = font.subset(&keep);
            }
        }))
        .is_ok();
        log_check(
            "m7fs.2.regression",
            &format!("{name} len={}", data.len()),
            survived,
        );
    }
    if n == 0 {
        eprintln!("check id=m7fs.2.regression subject=no engine fixtures yet outcome=SKIP");
    }
}
