//! Focused tests for the resumable lexical engine (FCB-021.A).
//!
//! The package oracle: run each representative fixture whole and at every
//! relevant byte split, then require identical coalesced token meaning and
//! exact source tiling — including splits inside UTF-8 characters,
//! delimiters, strings, comments and identifiers.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::resume::{
    coalesce_spans, highlight_chunked, verify_whole_block_coalesced_equivalence, CheckpointError,
    CommentState, FeedReport, LexerCheckpoint, ResumableLexer, ResumeError, StringState,
    CHECKPOINT_MAGIC, CHECKPOINT_VERSION, MAX_CHECKPOINT_BYTES, MAX_COMMENT_DEPTH,
};

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
    lexer.finish().expect("finish succeeds");
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

#[test]
fn checkpoint_roundtrip_and_serialization_invariants() {
    let cp = LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".to_string(),
        source_revision: 42,
        byte_offset: 128,
        comment_state: CommentState::Block { depth: 3 },
        string_state: StringState::RawString { hashes: 2 },
        interpolation_depth: 1,
        is_eof: false,
        unresolved_suffix: b"/* test".to_vec(),
    };

    let encoded = cp.to_bytes();
    assert!(encoded.len() <= MAX_CHECKPOINT_BYTES);
    assert_eq!(&encoded[..4], &CHECKPOINT_MAGIC);

    let decoded = LexerCheckpoint::from_bytes(&encoded).expect("valid checkpoint decodes");
    assert_eq!(decoded, cp);
    assert_eq!(decoded.version, CHECKPOINT_VERSION);
    assert_eq!(decoded.lang, "rust");
    assert_eq!(decoded.source_revision, 42);
    assert_eq!(decoded.byte_offset, 128);
    assert_eq!(decoded.comment_state, CommentState::Block { depth: 3 });
    assert_eq!(decoded.string_state, StringState::RawString { hashes: 2 });
    assert_eq!(decoded.interpolation_depth, 1);
    assert!(!decoded.is_eof);
    assert_eq!(decoded.unresolved_suffix, b"/* test");
}

#[test]
fn checkpoint_invalid_magic_refused() {
    let cp = LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".to_string(),
        source_revision: 1,
        byte_offset: 0,
        comment_state: CommentState::None,
        string_state: StringState::None,
        interpolation_depth: 0,
        is_eof: false,
        unresolved_suffix: Vec::new(),
    };
    let mut bytes = cp.to_bytes();
    bytes[0] = b'X'; // Corrupt magic
    let err = LexerCheckpoint::from_bytes(&bytes).expect_err("bad magic refused");
    assert_eq!(err, CheckpointError::InvalidMagic);
    assert_eq!(err.code(), "INVALID_MAGIC");
}

#[test]
fn checkpoint_unsupported_version_refused() {
    let cp = LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".to_string(),
        source_revision: 1,
        byte_offset: 0,
        comment_state: CommentState::None,
        string_state: StringState::None,
        interpolation_depth: 0,
        is_eof: false,
        unresolved_suffix: Vec::new(),
    };
    let mut bytes = cp.to_bytes();
    bytes[4..8].copy_from_slice(&99u32.to_le_bytes()); // Version 99
    let err = LexerCheckpoint::from_bytes(&bytes).expect_err("unsupported version refused");
    assert_eq!(
        err,
        CheckpointError::UnsupportedVersion {
            found: 99,
            expected: CHECKPOINT_VERSION
        }
    );
    assert_eq!(err.code(), "UNSUPPORTED_VERSION");
}

#[test]
fn checkpoint_payload_size_limits_refused() {
    let huge = vec![0u8; MAX_CHECKPOINT_BYTES + 10];
    let err = LexerCheckpoint::from_bytes(&huge).expect_err("huge payload refused");
    assert!(matches!(err, CheckpointError::PayloadTooLarge { .. }));
    assert_eq!(err.code(), "PAYLOAD_TOO_LARGE");
}

#[test]
fn checkpoint_nesting_depth_limits_refused() {
    let cp = LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".to_string(),
        source_revision: 1,
        byte_offset: 0,
        comment_state: CommentState::Block { depth: 100 }, // exceeds MAX_COMMENT_DEPTH (64)
        string_state: StringState::None,
        interpolation_depth: 0,
        is_eof: false,
        unresolved_suffix: Vec::new(),
    };
    let bytes = cp.to_bytes();
    let err = LexerCheckpoint::from_bytes(&bytes).expect_err("excessive depth refused");
    assert_eq!(
        err,
        CheckpointError::DepthLimitExceeded {
            depth: 100,
            max: MAX_COMMENT_DEPTH,
        }
    );
    assert_eq!(err.code(), "DEPTH_LIMIT_EXCEEDED");
}

#[test]
fn checkpoint_source_correspondence_mismatch_refused() {
    let cp = LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".to_string(),
        source_revision: 10,
        byte_offset: 200,
        comment_state: CommentState::None,
        string_state: StringState::None,
        interpolation_depth: 0,
        is_eof: false,
        unresolved_suffix: Vec::new(),
    };

    // Expected revision mismatch
    let err = ResumableLexer::from_checkpoint(&cp, 11, 200)
        .expect_err("revision mismatch refused");
    assert_eq!(
        err,
        CheckpointError::SourceCorrespondenceMismatch {
            expected_revision: 11,
            found_revision: 10,
        }
    );
    assert_eq!(err.code(), "SOURCE_CORRESPONDENCE_MISMATCH");

    // Expected offset mismatch
    let err = ResumableLexer::from_checkpoint(&cp, 10, 250)
        .expect_err("offset mismatch refused");
    assert_eq!(
        err,
        CheckpointError::OffsetMismatch {
            expected_offset: 250,
            found_offset: 200,
        }
    );
    assert_eq!(err.code(), "OFFSET_MISMATCH");
}

#[test]
fn checkpoint_trailing_bytes_refused() {
    let cp = LexerCheckpoint {
        version: CHECKPOINT_VERSION,
        lang: "rust".to_string(),
        source_revision: 1,
        byte_offset: 0,
        comment_state: CommentState::None,
        string_state: StringState::None,
        interpolation_depth: 0,
        is_eof: false,
        unresolved_suffix: Vec::new(),
    };
    let mut bytes = cp.to_bytes();
    bytes.push(0xFF); // Trailing garbage byte
    let err = LexerCheckpoint::from_bytes(&bytes).expect_err("trailing bytes refused");
    assert_eq!(err, CheckpointError::TrailingBytes);
    assert_eq!(err.code(), "TRAILING_BYTES");
}

#[test]
fn resume_from_checkpoint_tiling_and_coalesced_equivalence() {
    let code = r#"
fn calculate_metrics(count: usize) -> f64 {
    let factor = 3.14159;
    let mut total = 0.0;
    for i in 0..count {
        total += i as f64 * factor;
    }
    total
}
"#;

    let whole_spans = highlight("rust", code);
    assert_tiling(&whole_spans, code.len());
    let whole_coalesced = coalesce_spans(&whole_spans);

    // Stage 1: Lex the first half
    let split_pos = code.find("for i in 0..count").unwrap();
    let (part1, part2) = code.split_at(split_pos);

    let mut lexer1 = ResumableLexer::new("rust").expect("supported");
    lexer1.feed(part1.as_bytes()).expect("feed part1");

    // Take checkpoint at the boundary
    let cp = lexer1.checkpoint(100);
    assert_eq!(cp.source_revision, 100);
    let stage1_spans = lexer1.spans().to_vec();

    // Stage 2: Resume from checkpoint with matching expected revision and offset
    let mut lexer2 = ResumableLexer::from_checkpoint(&cp, 100, cp.byte_offset)
        .expect("resumes from checkpoint");
    lexer2.feed(part2.as_bytes()).expect("feed part2");
    lexer2.finish().expect("finish stage 2");

    let stage2_spans = lexer2.spans().to_vec();

    // Combine stage 1 and stage 2 spans
    let mut combined = stage1_spans;
    combined.extend(stage2_spans);

    // Assert exact tiling and coalesced equivalence against whole-block output
    assert_tiling(&combined, code.len());
    let combined_coalesced = coalesce_spans(&combined);
    assert_eq!(
        combined_coalesced, whole_coalesced,
        "spans resumed from checkpoint must coalesce identically to whole-block classification"
    );
}

#[test]
fn resume_from_checkpoint_comment_and_string_states() {
    let code = "/* multi-line\n   comment\n   block */\nlet msg = \"hello\nworld\";\n";
    let whole_spans = highlight("rust", code);
    assert_tiling(&whole_spans, code.len());
    let whole_coalesced = coalesce_spans(&whole_spans);

    // Split inside the comment
    let split1 = code.find("comment").unwrap();
    let (p1, rest) = code.split_at(split1);
    let mut lex1 = ResumableLexer::new("rust").unwrap();
    lex1.feed(p1.as_bytes()).unwrap();

    let cp1 = lex1.checkpoint(1);
    assert_eq!(cp1.comment_state, CommentState::Block { depth: 1 });

    let mut lex2 = ResumableLexer::from_checkpoint(&cp1, 1, cp1.byte_offset).unwrap();
    lex2.feed(rest.as_bytes()).unwrap();
    lex2.finish().unwrap();

    let mut combined = lex1.spans().to_vec();
    combined.extend(lex2.spans().to_vec());
    assert_tiling(&combined, code.len());
    assert_eq!(coalesce_spans(&combined), whole_coalesced);
}

#[test]
fn whole_block_compatibility_across_languages() {
    let test_cases = [
        ("rust", "fn solve(x: i32) -> bool { x > 0 && true }"),
        ("python", "def compute(items):\n    return [x * 2 for x in items if x > 0]"),
        (
            "javascript",
            "function render(tree) {\n    const node = tree.root;\n    return node ? node.id : null;\n}",
        ),
        (
            "typescript",
            "interface Config {\n    timeout: number;\n    retries?: number;\n}",
        ),
        (
            "c",
            "#include <stdio.h>\nint main() { printf(\"hello\\n\"); return 0; }",
        ),
        (
            "cpp",
            "#include <iostream>\nint main() { std::cout << 42 << std::endl; }",
        ),
        (
            "go",
            "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"hi\") }",
        ),
        (
            "powershell",
            "Get-ChildItem -Path . | Where-Object { $_.Length -gt 100 }",
        ),
        (
            "json",
            "{\"name\": \"fcb\", \"version\": 1, \"features\": [\"audit\", \"view\"]}",
        ),
        (
            "sql",
            "SELECT id, title, score FROM submissions WHERE score > 10 ORDER BY score DESC;",
        ),
        ("yaml", "name: fcb\nsteps:\n  - name: test\n    run: cargo test"),
        (
            "toml",
            "[package]\nname = \"franken_code_browser\"\nversion = \"0.1.0\"",
        ),
        ("bash", "#!/bin/bash\nset -euo pipefail\necho \"running $1\""),
    ];

    for (lang, code) in test_cases {
        for chunk_size in [1, 2, 3, 7, 16, 64] {
            let chunked = highlight_chunked(lang, code, chunk_size).expect("highlight_chunked succeeds");
            assert_tiling(&chunked, code.len());
            assert!(
                verify_whole_block_coalesced_equivalence(lang, code, chunk_size)
                    .expect("verification succeeds")
            );
        }
    }
}

#[test]
fn adversarial_delimiter_and_codepoint_splits() {
    let code = r#####"
    let r = r#"raw string with inner "#;
    let s = "escaped \"quotes\" and \n newlines";
    /* nested-looking /* comment */ text */
    // unicode comments: 🦀 Rust, 日 Japan, ü Umlaut
    let x = 'c';
    "#####;

    let bytes = code.as_bytes();
    for split in 1..bytes.len() {
        let (a, b) = bytes.split_at(split);
        let mut lex1 = ResumableLexer::new("rust").unwrap();
        lex1.feed(a).unwrap();
        let cp = lex1.checkpoint(1);
        let mut lex2 = ResumableLexer::from_checkpoint(&cp, 1, cp.byte_offset).unwrap();
        lex2.feed(b).unwrap();
        lex2.finish().unwrap();

        let mut combined = lex1.spans().to_vec();
        combined.extend(lex2.spans().to_vec());
        assert_tiling(&combined, bytes.len());

        let whole = highlight("rust", code);
        assert_eq!(
            coalesce_spans(&combined),
            coalesce_spans(&whole),
            "split at {split} failed coalesced equivalence"
        );
    }
}
