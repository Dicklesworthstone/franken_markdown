//! Focused tests for the resumable lexical engine (FCB-021.A).
//!
//! The package oracle: run each representative fixture whole and at every
//! relevant byte split, then require identical coalesced token meaning and
//! exact source tiling — including splits inside UTF-8 characters,
//! delimiters, strings, comments and identifiers.

#![forbid(unsafe_code)]

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::resume::{FeedReport, ResumableLexer, ResumeError};

/// Coalesce adjacent same-kind spans: the chunked stream may split a token
/// across feeds, but the coalesced meaning must match the whole-input run.
fn coalesce(spans: &[Span]) -> Vec<(Tok, usize, usize)> {
    let mut runs: Vec<(Tok, usize, usize)> = Vec::new();
    for span in spans {
        match runs.last_mut() {
            Some((kind, start, end)) if *kind == span.kind && *end == span.start => {
                *end = span.end;
            }
            _ => runs.push((span.kind, span.start, span.end)),
        }
    }
    runs
}

fn assert_tiling(spans: &[Span], total: usize) {
    let mut next = 0usize;
    for span in spans {
        assert_eq!(span.start, next, "gap/overlap at {}", span.start);
        assert!(span.end > span.start, "empty span at {}", span.start);
        next = span.end;
    }
    assert_eq!(next, total, "spans must tile the complete source");
}

fn chunked_spans(lang: &str, source: &str, split: usize) -> Vec<Span> {
    let mut lexer = ResumableLexer::new(lang).expect("supported language");
    let (a, b) = source.as_bytes().split_at(split.min(source.len()));
    if !a.is_empty() {
        lexer.feed(a).expect("chunk a feeds");
    }
    if !b.is_empty() {
        lexer.feed(b).expect("chunk b feeds");
    }
    lexer.finish().expect("finish flushes the suffix");
    lexer.spans().to_vec()
}

const FIXTURE: &str = "fn main() {\n    let s = \"hi\"; // trail\n    /* block */ let n = 42;\n}";

#[test]
fn whole_and_every_single_split_coalesce_identically() {
    let whole = highlight("rust", FIXTURE);
    assert_tiling(&whole, FIXTURE.len());
    let whole_runs = coalesce(&whole);

    for split in 0..=FIXTURE.len() {
        // Skip byte positions that would split a UTF-8 character; this
        // fixture is ASCII, so every position is a character boundary.
        let chunked = chunked_spans("rust", FIXTURE, split);
        assert_tiling(&chunked, FIXTURE.len());
        assert_eq!(
            coalesce(&chunked),
            whole_runs,
            "split at {split} must coalesce to the whole-input classification"
        );
    }
}

#[test]
fn multi_byte_characters_survive_any_split() {
    // "ä" is two bytes, "日" is three: splits inside them must be held, not
    // refused, and the final classification must match the whole input.
    let source = "let s = \"ä日\"; // ünïcödé\n";
    let whole = highlight("rust", source);
    let whole_runs = coalesce(&whole);
    for split in 0..source.len() {
        let chunked = chunked_spans("rust", source, split);
        assert_tiling(&chunked, source.len());
        assert_eq!(coalesce(&chunked), whole_runs, "split at {split}");
    }
}
#[test]
fn unresolved_suffix_is_held_then_released() {
    let mut lexer = ResumableLexer::new("rust").expect("supported");
    let report = lexer.feed(b"fn").expect("feed prefix");
    assert_eq!(report.spans_emitted, 0, "an unterminated token is held");
    assert!(report.unresolved);
    assert_eq!(report.pending_bytes, 2);

    let report = lexer.feed(b" main() {}").expect("feed rest");
    assert!(report.spans_emitted >= 1, "the held keyword is released");

    lexer.finish().expect("finish flushes the suffix");
    assert_tiling(lexer.spans(), 12); // "fn main() {}" is 12 bytes
}

#[test]
fn unsupported_language_refused_upfront() {
    assert_eq!(
        ResumableLexer::new("definitely-not-a-language").unwrap_err(),
        ResumeError::UnsupportedLanguage
    );
}

#[test]
fn malformed_utf8_refused_with_offset() {
    let mut lexer = ResumableLexer::new("rust").expect("supported");
    lexer.feed(b"fn ").expect("valid prefix");
    let error = lexer.feed(&[0xFF]).expect_err("0xFF is never valid UTF-8");
    assert_eq!(error.code(), "INVALID_UTF8");
}

#[test]
fn truncated_utf8_head_is_held_not_refused() {
    let mut lexer = ResumableLexer::new("rust").expect("supported");
    // First byte of a two-byte character: truncated, not malformed.
    lexer.feed(b"// \xC3").expect("truncated head is held");
    lexer.feed(b"\xA9 rest").expect("the completing byte arrives");
    lexer.finish().expect("finish succeeds");
    let source_len = 10; // "// " + 2-byte char + " rest"
    assert_tiling(lexer.spans(), source_len);
}

#[test]
fn suffix_cap_refuses_unbounded_unterminated_comment() {
    let mut lexer = ResumableLexer::with_limits("rust", 64).expect("supported");
    let big = format!("/* {}", "x".repeat(200));
    let error = lexer.feed(big.as_bytes()).expect_err("cap is exceeded");
    assert_eq!(error.code(), "SUFFIX_TOO_LONG");
}

#[test]
fn already_finished_refuses_further_feeds() {
    let mut lexer = ResumableLexer::new("rust").expect("supported");
    lexer.feed(b"fn").expect("first feed");
    let _ = lexer.finish().expect("finish succeeds");
    assert_eq!(
        lexer.feed(b"more").unwrap_err(),
        ResumeError::AlreadyFinished
    );
}

#[test]
fn feed_report_fields_are_truthful() {
    let mut lexer = ResumableLexer::new("rust").expect("supported");
    let FeedReport {
        spans_emitted,
        pending_bytes,
        unresolved,
    } = lexer.feed(b"let x = 1;\n").expect("feed");
    assert!(spans_emitted >= 1);
    assert_eq!(pending_bytes, lexer.pending_bytes());
    // `unresolved` means the held suffix is nonempty, by definition.
    assert_eq!(unresolved, pending_bytes > 0);
}
