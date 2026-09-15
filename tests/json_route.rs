//! FCB-022 / fcb-9vx.17 qualification: the JSON route through the shared
//! resumable lexical engine, adversarially split at every byte boundary.
//!
//! Oracle: for every representative fixture and EVERY byte split, feeding
//! the chunks through [`ResumableLexer`] and finishing must produce exactly
//! the same coalesced span sequence as whole-input classification, and the
//! sequence must tile the source monotonically. Malformed input is
//! classified truthfully (no grammar-validation claims); malformed UTF-8 is
//! a typed refusal; an oversized unterminated token hits the bounded
//! suffix cap.
//!
//! Consumer fixture: `tests/fixtures/json_route/consumer_document.json` —
//! a realistic config-shaped document exercised whole and split.

#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use franken_markdown::highlight::{Span, highlight};
use franken_markdown::resume::{ResumableLexer, ResumeError};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/json_route/consumer_document.json");

const RUN_ID_ENV: &str = "FCB_012_RUN_ID";

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
}

fn fixtures() -> Vec<Fixture> {
    let escapes = r#"{ "esc": "quote\" back\\ slash/ \b\f\n\r\t uni\u0041 pair\uD83D\uDE00" }"#;
    let numbers = r#"{ "ints": [0, -0, 42, -17], "floats": [1.5, -0.25, 1e3, 2.5E-10], "big": 12345678901234567890 }"#;
    let literals = r#"{ "t": true, "f": false, "n": null, "nested": { "a": [1, {"b": []}] } }"#;
    let unicode_raw = "{ \"emoji\": \"\u{1F600}\u{00E9}\", \"cjk\": \"\u{4E2D}\u{6587}\" }";
    let whitespace_padded = "   {  \"k\" : \t \"v\" ,\n  \"j\": [ 1 , 2 ]  }  ";
    let malformed_truncated_string = r#"{ "unterminated": "runs off the end"#;
    let malformed_lone_escape = r#"{ "bad-escape": "\q" }"#;
    let malformed_lone_surrogate = r#"{ "lone": "\uD800" }"#;
    let malformed_control_char = "{ \"ctrl\": \"line\nbreak\" }";
    let deep_nesting = format!("{}{}", "[".repeat(64), "]".repeat(64));
    let empty_inputs = "";
    let whitespace_only = "      ";
    let huge_string = format!("{{ \"huge\": \"{}\" }}", "y".repeat(4096));

    vec![
        Fixture {
            name: "escapes_and_unicode_escapes",
            bytes: escapes.as_bytes().to_vec(),
        },
        Fixture {
            name: "numbers",
            bytes: numbers.as_bytes().to_vec(),
        },
        Fixture {
            name: "literals_and_nesting",
            bytes: literals.as_bytes().to_vec(),
        },
        Fixture {
            name: "raw_unicode",
            bytes: unicode_raw.as_bytes().to_vec(),
        },
        Fixture {
            name: "whitespace_padded",
            bytes: whitespace_padded.as_bytes().to_vec(),
        },
        Fixture {
            name: "malformed_truncated_string",
            bytes: malformed_truncated_string.as_bytes().to_vec(),
        },
        Fixture {
            name: "malformed_lone_escape",
            bytes: malformed_lone_escape.as_bytes().to_vec(),
        },
        Fixture {
            name: "malformed_lone_surrogate",
            bytes: malformed_lone_surrogate.as_bytes().to_vec(),
        },
        Fixture {
            name: "malformed_control_char",
            bytes: malformed_control_char.as_bytes().to_vec(),
        },
        Fixture {
            name: "deep_nesting",
            bytes: deep_nesting.into_bytes(),
        },
        Fixture {
            name: "empty",
            bytes: empty_inputs.as_bytes().to_vec(),
        },
        Fixture {
            name: "whitespace_only",
            bytes: whitespace_only.as_bytes().to_vec(),
        },
        Fixture {
            name: "huge_string",
            bytes: huge_string.into_bytes(),
        },
        Fixture {
            name: "consumer_document",
            bytes: include_bytes!("fixtures/json_route/consumer_document.json").to_vec(),
        },
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

/// Whole-input classification for one fixture.
fn whole_spans(bytes: &[u8]) -> Vec<Span> {
    let text = std::str::from_utf8(bytes).expect("fixtures are valid UTF-8");
    highlight("json", text)
}

/// Classification via the resumable engine with one split at `at`.
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

/// Classification via the resumable engine fed ONE BYTE PER FEED.
fn byte_per_feed_spans(bytes: &[u8]) -> Vec<Span> {
    let mut lexer = ResumableLexer::new("json").expect("json route supported");
    for byte in bytes {
        lexer
            .feed(std::slice::from_ref(byte))
            .expect("byte feed valid");
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

fn assert_split_equivalence(name: &str, bytes: &[u8]) {
    let expected = coalesce(&whole_spans(bytes));
    assert_tile(&expected, bytes.len());

    for at in 0..=bytes.len() {
        let got = split_spans(bytes, at);
        assert_eq!(got, expected, "{name}: split at {at} diverged");
    }

    let per_byte = byte_per_feed_spans(bytes);
    assert_eq!(per_byte, expected, "{name}: byte-per-feed diverged");
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

fn receipts_dir() -> PathBuf {
    let run_id = std::env::var(RUN_ID_ENV).unwrap_or_else(|_| "local".to_string());
    std::env::temp_dir().join(format!("fcb-9vx17-receipts-{run_id}"))
}

#[test]
fn every_fixture_is_split_equivalent_at_every_byte_boundary() {
    let fixtures = fixtures();
    assert!(fixtures.len() >= 14, "representative corpus present");
    for fixture in &fixtures {
        assert_split_equivalence(fixture.name, &fixture.bytes);
        scenario_receipt(
            fixture.name,
            "split-equivalent",
            "whole == every split == byte-per-feed",
        );
    }
}

#[test]
fn consumer_document_qualifies_whole_and_split() {
    let bytes = CONSUMER_DOCUMENT.as_bytes();
    assert_split_equivalence("consumer_document", bytes);
    // The consumer fixture must exercise strings, numbers, literals,
    // nesting, and escapes at minimum.
    let text = std::str::from_utf8(bytes).expect("consumer fixture is UTF-8");
    for marker in ["\"", ":", ",", "[", "]", "{", "}", "\\", "123", "true"] {
        assert!(
            text.contains(marker),
            "consumer fixture missing {marker:?} surface"
        );
    }
    scenario_receipt(
        "consumer_document",
        "qualified",
        "whole + split + surface coverage",
    );
}

#[test]
fn malformed_utf8_is_a_typed_refusal_at_the_offset() {
    let mut bytes = b"{ \"ok\": 1 }".to_vec();
    // Splice an invalid 0xFF byte after the opening quote region.
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
    // An unterminated string longer than the configured cap must be
    // refused with SuffixTooLong instead of growing without bound.
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
    use franken_markdown::lang_dispatch::{DispatchRequest, LanguageRegistry};

    let registry = LanguageRegistry::standard_20_inventory();
    let request = DispatchRequest {
        language_query: "json",
        code: b"{ \"dispatch\": [true, null] }",
        work_budget_bytes: 4096,
        source_revision: None,
    };
    let result = registry.dispatch(request).expect("json dispatches");
    assert_eq!(
        franken_markdown::lang_dispatch::QualificationStatus::Implemented,
        result.status,
        "capability row must remain truthful: JSON is implemented"
    );
    assert!(result.is_finished);
    assert_eq!(result.bytes_processed, request.code.len());
    assert_eq!(result.pending_suffix_bytes, 0);
}

#[test]
fn json_capability_row_is_versioned() {
    use franken_markdown::highlight::JSON_CAPABILITY_V1;
    assert_eq!(JSON_CAPABILITY_V1.version, 1);
    const {
        const {
            assert!(JSON_CAPABILITY_V1.incremental);
        };
    };
    const {
        const {
            assert!(JSON_CAPABILITY_V1.escapes_numbers_literals);
        };
    };
}
