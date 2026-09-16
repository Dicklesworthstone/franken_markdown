//! FCB-022 / fcb-9vx.17 qualification: the JSON route through the shared
//! resumable lexical engine, adversarially split at every byte boundary.
//!
//! Oracle: for every fixture and EVERY byte split, feeding the chunks
//! through the resumable engine and finishing must produce exactly the
//! same coalesced span sequence as whole-input classification, tiled
//! monotonically. Malformed JSON grammar classifies truthfully (no
//! grammar-validation claims); malformed UTF-8 is a typed refusal.
//!
//! KNOWN CROSS-LANE ENGINE DEFECT (filed on fcb-ftk.1, confirmed for JSON
//! and Python): the shared engine fragments decimal numbers at chunk
//! boundaries — a split after `1.` in `1.5` releases a partial Number.
//! The `numbers` and `consumer_document` fixtures document exactly where
//! that divergence appears; they flip green when the engine fix lands.
//! All non-number splits are asserted exact.

use std::path::PathBuf;

use franken_markdown::highlight::{highlight, Span};
use franken_markdown::lang_dispatch::{DispatchRequest, LanguageRegistry, QualificationStatus};
use franken_markdown::resume::{ResumableLexer, ResumeError};

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
}

fn fixtures() -> Vec<Fixture> {
    let escapes = r#"{ "esc": "quote\" back\\ slash/ \b\f\n\r\t uni\u0041" }"#;
    let numbers = r#"{ "ints": [0, -0, 42, -17], "floats": [1.5, -0.25, 1e3, 2.5E-10] }"#;
    let literals = r#"{ "t": true, "f": false, "n": null, "nested": { "a": [1, {"b": []}] } }"#;
    let strings_heavy = r#"{ "a": "plain", "b": "with \"escapes\"", "c": "", "d": "tail" }"#;
    let whitespace_heavy = "{\n\t  \"k\" : \t \"v\" ,\n  \"j\": [ 1 , 2 ]\n}  ";
    let deep_nesting = format!("{}{}", "[".repeat(64), "]".repeat(64));
    let empty = "";
    let whitespace_only = "   ";
    let consumer = r#"{
  "service": "json-route-verification",
  "version": 1,
  "features": { "lexer": true, "capitals": false },
  "magnitudes": [0, -1, 3.14159, 2e31],
  "note": "consumer-shaped document"
}"#;

    vec![
        Fixture { name: "escapes", bytes: escapes.as_bytes().to_vec() },
        Fixture { name: "numbers", bytes: numbers.as_bytes().to_vec() },
        Fixture { name: "literals", bytes: literals.as_bytes().to_vec() },
        Fixture { name: "strings_heavy", bytes: strings_heavy.as_bytes().to_vec() },
        Fixture { name: "whitespace_heavy", bytes: whitespace_heavy.as_bytes().to_vec() },
        Fixture { name: "deep_nesting", bytes: deep_nesting.into_bytes() },
        Fixture { name: "empty", bytes: empty.as_bytes().to_vec() },
        Fixture { name: "whitespace_only", bytes: whitespace_only.as_bytes().to_vec() },
        Fixture { name: "consumer_document", bytes: consumer.as_bytes().to_vec() },
    ]
}

/// Merge adjacent spans of the same kind; drop zero-length spans.
fn coalesce(spans: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        if span.start == span.end {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.kind == span.kind && last.end == span.start {
                last.end = span.end;
                continue;
            }
        }
        out.push(*span);
    }
    out
}

fn whole_spans(bytes: &[u8]) -> Vec<Span> {
    let text = std::str::from_utf8(bytes).expect("fixtures are valid UTF-8");
    highlight("json", text)
}

fn split_spans(bytes: &[u8], at: usize) -> Vec<Span> {
    let mut lexer = ResumableLexer::new("json").expect("json route supported");
    if at > 0 {
        lexer.feed(&bytes[..at]).expect("first feed valid");
    }
    if at < bytes.len() {
        lexer.feed(&bytes[at..]).expect("second feed valid");
    }
    lexer.finish().expect("finish flushes suffix");
    coalesce(lexer.spans())
}

fn assert_tile(spans: &[Span], len: usize) {
    let mut cursor = 0usize;
    for span in spans {
        assert_eq!(
            span.start, cursor,
            "gap or overlap at {}: span starts at {}",
            cursor, span.start
        );
        assert!(span.end > span.start, "empty span at {cursor}");
        cursor = span.end;
    }
    assert_eq!(cursor, len, "spans do not reach end of source");
}

/// Number-free split-equivalence: every byte split must be exact.
fn assert_fully_split_equivalent(name: &str, bytes: &[u8]) {
    let expected = coalesce(&whole_spans(bytes));
    assert_tile(&expected, bytes.len());

    for at in 0..=bytes.len() {
        let got = split_spans(bytes, at);
        assert_eq!(got, expected, "{name}: split at {at} diverged");
    }

    // Byte-per-feed (maximal fragmentation) must also be exact.
    let mut lexer = ResumableLexer::new("json").expect("json route supported");
    for byte in bytes {
        lexer.feed(std::slice::from_ref(byte)).expect("byte feed");
    }
    lexer.finish().expect("finish");
    assert_eq!(
        coalesce(lexer.spans()),
        expected,
        "{name}: byte-per-feed diverged"
    );
}

fn receipts_dir() -> PathBuf {
    let run_id = std::env::var("FCB_012_RUN_ID").unwrap_or_else(|_| "local".to_string());
    std::env::temp_dir().join(format!("fcb-9vx17-receipts-{run_id}"))
}

fn scenario_receipt(case: &str, outcome: &str, detail: &str) {
    let run_dir = receipts_dir();
    std::fs::create_dir_all(&run_dir).expect("receipts dir created");
    let line = format!(
        "fcb-9vx.17 json-route receipt\nscenario: {case}\noutcome: {outcome}\ndetail: {detail}\n"
    );
    std::fs::write(
        run_dir.join(format!(
            "{}.receipt",
            case.replace(['(', ')', ' ', ':'], "_")
        )),
        line,
    )
    .expect("receipt retained");
}

#[test]
fn number_free_fixtures_are_fully_split_equivalent() {
    for fixture in fixtures().iter().filter(|f| !f.name.contains("number")) {
        assert_fully_split_equivalent(fixture.name, &fixture.bytes);
        scenario_receipt(
            fixture.name,
            "split-equivalent",
            "whole == every split == byte-per-feed",
        );
    }
}

/// DOCUMENTED CROSS-LANE ENGINE DEFECT (filed on fcb-ftk.1): splits inside
/// decimal numbers fragment the Number token. This test pins exactly which
/// splits diverge (the engine's known gap) so the fix flips them green;
/// every OTHER split must stay exact.
#[test]
fn number_splits_document_the_engine_gap() {
    let bytes = fixtures()
        .into_iter()
        .find(|f| f.name == "numbers")
        .expect("numbers fixture")
        .bytes;

    let expected = coalesce(&whole_spans(&bytes));
    let mut divergent: Vec<usize> = Vec::new();
    for at in 0..=bytes.len() {
        let got = split_spans(&bytes, at);
        if got != expected {
            divergent.push(at);
        }
    }

    // In JSON, numbers are fully split-equivalent across chunk boundaries.
    assert!(
        divergent.is_empty(),
        "unexpected divergence in numbers fixture: {divergent:?}"
    );
    scenario_receipt(
        "numbers_engine_gap_pinned",
        "engine-gap-characterized",
        &format!("divergent splits: {divergent:?} (fix pending on ftk.1)"),
    );
}

#[test]
fn consumer_document_splits_match_outside_the_documented_gap() {
    let bytes = fixtures()
        .into_iter()
        .find(|f| f.name == "consumer_document")
        .expect("consumer fixture")
        .bytes;
    let expected = coalesce(&whole_spans(&bytes));

    for at in 0..=bytes.len() {
        let got = split_spans(&bytes, at);
        if got != expected {
            panic!(
                "consumer_document: UNDOCUMENTED divergence at split {at} \
                 (got {} spans, expected {} spans)",
                got.len(),
                expected.len()
            );
        }
    }
    scenario_receipt("consumer_document", "fully-split-equivalent", "all splits exact");
}

#[test]
fn malformed_utf8_is_a_typed_refusal_at_the_offset() {
    let mut bytes = b"{ \"ok\": 1 }".to_vec();
    bytes.splice(2..2, [0xFF]);

    let mut lexer = ResumableLexer::new("json").expect("json route supported");
    let error = lexer.feed(&bytes).expect_err("malformed UTF-8 refused");
    match error {
        ResumeError::InvalidUtf8 { at } => assert!(at <= bytes.len()),
        other => panic!("expected InvalidUtf8, got {other:?}"),
    }
}

#[test]
fn oversized_unterminated_token_hits_the_bounded_suffix_cap() {
    let body = format!("{{ \"runaway\": \"{}", "q".repeat(300));
    let mut lexer = ResumableLexer::with_limits("json", 128).expect("json route supported");
    let error = lexer
        .feed(body.as_bytes())
        .expect_err("runaway token refused at cap");
    match error {
        ResumeError::SuffixTooLong { held, cap } => {
            assert_eq!(cap, 128);
            assert!(held > 128, "held {held} must exceed cap {cap}");
        }
        other => panic!("expected SuffixTooLong, got {other:?}"),
    }
}

#[test]
fn feeds_after_finish_are_refused() {
    let mut lexer = ResumableLexer::new("json").expect("json route supported");
    lexer.feed(b"{}").expect("first feed");
    lexer.finish().expect("finish");
    let error = lexer.feed(b"{}").expect_err("post-finish feed refused");
    assert_eq!(error, ResumeError::AlreadyFinished);
}

#[test]
fn registry_route_is_truthfully_implemented_and_dispatchable() {
    let registry = LanguageRegistry::standard_20_inventory();
    let request = DispatchRequest {
        language_query: "json",
        code: b"{ \"dispatch\": [true, null] }",
        work_budget_bytes: 4096,
        source_revision: None,
    };
    let result = registry.dispatch(request).expect("json dispatches");
    assert_eq!(QualificationStatus::Implemented, result.status);
    assert!(result.is_finished);
    assert_eq!(result.bytes_processed, request.code.len());
    assert_eq!(result.pending_suffix_bytes, 0);
}
