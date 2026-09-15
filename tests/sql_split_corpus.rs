//! FCB-022 / fcb-9vx.20 qualification: the SQL route through the shared
//! resumable lexical engine, adversarially split at every byte boundary.
//!
//! Oracle: for every fixture and EVERY byte split, feeding the chunks
//! through [`ResumableLexer`] and finishing must produce exactly the same
//! coalesced span sequence as whole-input classification, monotonically
//! tiling the source. Malformed input is classified truthfully (no grammar
//! validation claims); unterminated strings/comments extend to EOF as the
//! provisional trailing span (chunk boundaries are not EOF).
//!
//! KNOWN CROSS-LANE DEFECT (filed on fcb-ftk.1): the shared engine
//! fragments decimal numbers at chunk boundaries (a split after `1.` in
//! `1.5` releases a partial Number). Number fixtures here document the
//! divergence and stay marked until the engine fix lands; all non-number
//! cases must pass.

use std::path::PathBuf;

use franken_markdown::highlight;
use franken_markdown::highlight::Span;
use franken_markdown::resume::{ResumableLexer, ResumeError};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/sql_route/consumer_document.sql");

struct Fixture {
    name: &'static str,
    bytes: Vec<u8>,
}

fn fixtures() -> Vec<Fixture> {
    let strings_doubling =
        b"SELECT 'it''s' FROM \"my\"\"table\" WHERE name = 'plain'".to_vec();
    let comments = b"-- line comment\nSELECT 1 /* block\ncomment */ FROM t -- trailing".to_vec();
    let unterminated_string = b"SELECT 'runs off".to_vec();
    let unterminated_block = b"SELECT 1 /* never closed".to_vec();
    let params = b"SELECT * FROM t WHERE a = ? AND b = $1 AND c = :name AND d = @flag".to_vec();
    let quoted_identifiers = b"SELECT \"col one\", `col two` FROM \"my schema\".\"my table\"".to_vec();
    let numbers_hex =
        b"SELECT 0xFF, 42, 3.14, 1e10, -2.5E-3 FROM t WHERE id = 0x1A".to_vec();
    let keywords_types =
        b"INSERT INTO users (id, name) VALUES (1, 'ada') RETURNING id, name".to_vec();
    let whitespace_only = b"   \n\t  ".to_vec();
    let empty = b"".to_vec();
    let consumer = CONSUMER_DOCUMENT.as_bytes().to_vec();

    vec![
        Fixture { name: "strings_doubling", bytes: strings_doubling },
        Fixture { name: "comments", bytes: comments },
        Fixture { name: "unterminated_string", bytes: unterminated_string },
        Fixture { name: "unterminated_block", bytes: unterminated_block },
        Fixture { name: "params", bytes: params },
        Fixture { name: "quoted_identifiers", bytes: quoted_identifiers },
        Fixture { name: "numbers_hex", bytes: numbers_hex },
        Fixture { name: "keywords_types", bytes: keywords_types },
        Fixture { name: "whitespace_only", bytes: whitespace_only },
        Fixture { name: "empty", bytes: empty },
        Fixture { name: "consumer_document", bytes: consumer },
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
    highlight::highlight("sql", text)
}

fn split_spans(bytes: &[u8], at: usize) -> Vec<Span> {
    let mut lexer = ResumableLexer::new("sql").expect("sql route supported");
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
            "gap or overlap: span starts at {} but cursor is {}",
            span.start, cursor
        );
        assert!(span.end > span.start, "empty span at {cursor}");
        cursor = span.end;
    }
    assert_eq!(cursor, len, "spans do not tile to end of source");
}

fn assert_split_equivalence(name: &str, bytes: &[u8]) {
    let expected = coalesce(&whole_spans(bytes));
    assert_tile(&expected, bytes.len());

    for at in 0..=bytes.len() {
        let got = split_spans(bytes, at);
        if got != expected {
            for (index, pair) in got.iter().zip(expected.iter()).enumerate() {
                if pair.0 != pair.1 {
                    panic!(
                        "{name}: first divergence at span {index}: got {:?} expected {:?} (got {} spans, expected {})",
                        pair.0, pair.1, got.len(), expected.len()
                    );
                }
            }
            panic!(
                "{name}: prefix matches but lengths diverge (got {}, expected {})",
                got.len(),
                expected.len()
            );
        }
    }
}

fn receipts_dir() -> PathBuf {
    let run_id = std::env::var("FCB_012_RUN_ID").unwrap_or_else(|_| "local".to_string());
    std::env::temp_dir().join(format!("fcb-9vx20-receipts-{run_id}"))
}

fn scenario_receipt(case: &str, outcome: &str, detail: &str) {
    let run_dir = receipts_dir();
    std::fs::create_dir_all(&run_dir).expect("receipts dir created");
    let line = format!(
        "fcb-9vx.20 sql-route receipt\nscenario: {case}\noutcome: {outcome}\ndetail: {detail}\n"
    );
    std::fs::write(
        run_dir.join(format!("{}.receipt", case.replace(['(', ')', ' ', ':'], "_"))),
        line,
    )
    .expect("receipt retained");
}

#[test]
fn every_fixture_is_split_equivalent_at_every_byte_boundary() {
    let fixtures = fixtures();
    assert!(fixtures.len() >= 11, "representative corpus present");
    for fixture in &fixtures {
        assert_split_equivalence(fixture.name, &fixture.bytes);
        scenario_receipt(fixture.name, "split-equivalent", "whole == every split");
    }
}

#[test]
fn consumer_document_qualifies_whole_and_split() {
    let bytes = CONSUMER_DOCUMENT.as_bytes();
    assert_split_equivalence("consumer_document", bytes);
    let text = std::str::from_utf8(bytes).expect("consumer fixture is UTF-8");
    for marker in ["SELECT", "--", "/*", "'', ", "?", "$1", ":param"] {
        assert!(
            text.contains(marker),
            "consumer fixture missing {marker:?} surface"
        );
    }
    scenario_receipt("consumer_document", "qualified", "whole + split + surface coverage");
}

#[test]
fn unterminated_string_extends_to_eof_as_provisional_span() {
    let bytes = b"SELECT 'never closed".to_vec();
    let spans = split_spans(&bytes, bytes.len());
    let last = spans.last().expect("spans present");
    assert_eq!(last.end, bytes.len(), "provisional span reaches EOF");
    assert_eq!(
        std::str::from_utf8(&bytes[last.start..last.end]).expect("valid slice"),
        "'never closed"
    );
}

#[test]
fn feeds_after_finish_are_refused() {
    let mut lexer = ResumableLexer::new("sql").expect("sql route supported");
    lexer.feed(b"SELECT 1").expect("first feed");
    lexer.finish().expect("finish");
    let error = lexer.feed(b"SELECT 2").expect_err("post-finish feed refused");
    assert_eq!(error, ResumeError::AlreadyFinished);
}

#[test]
fn registry_route_is_truthfully_implemented_and_dispatchable() {
    use franken_markdown::lang_dispatch::{DispatchRequest, LanguageRegistry};

    let registry = LanguageRegistry::standard_20_inventory();
    let request = DispatchRequest {
        language_query: "sql",
        code: b"SELECT id FROM users WHERE active = TRUE",
        work_budget_bytes: 4096,
        source_revision: None,
    };
    let result = registry.dispatch(request).expect("sql dispatches");
    assert_eq!(
        franken_markdown::lang_dispatch::QualificationStatus::Implemented,
        result.status,
        "capability row must remain truthful: SQL is implemented"
    );
    assert!(result.is_finished);
    assert_eq!(result.bytes_processed, request.code.len());
}

#[test]
fn sql_capability_row_is_versioned() {
    use franken_markdown::lang_sql::SQL_CAPABILITY_V1;
    assert_eq!(SQL_CAPABILITY_V1.version, 1);
    assert!(SQL_CAPABILITY_V1.incremental);
    assert!(SQL_CAPABILITY_V1.quotes_comments_params);
}
