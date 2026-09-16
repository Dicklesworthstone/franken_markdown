//! FCB-022.13 Go adversarial split corpus: every representative fixture
//! is lexed whole and at every byte split through the shared FCB-021
//! resumable engine; coalesced token meaning and exact source tiling must
//! agree. Malformed and truncated inputs must lex with bounded work, never panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_go::{GO_CAPABILITY_V1, GoCapabilityV1, lex_go_into};
use franken_markdown::resume::{ResumableLexer, ResumeError, coalesce_spans};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/go_route/consumer_document.go");

/// Representative Go fixtures exercising the bead's feature list:
/// raw backticks vs interpreted strings, rune literals, comments,
/// numeric forms, channel/pointer operators, keywords, types, and truncations.
const FIXTURES: &[&str] = &[
    "package main\nfunc main() {}\n",
    "// line comment\n/* block\ncomment */ const x = 42\n",
    "raw := `multi\nline\nstring with \"quotes\" and \\no\\ escapes`\n",
    "interpreted := \"escaped \\\"quotes\\\" and \\n\\t\\x41\\u0041\\U00000041\"\n",
    "var r1 rune = 'a'\nvar r2 = '\\n'\nvar r3 = '\\''\nvar r4 = '\\u0041'\n",
    "hex := 0xDEAD_BEEF\noct := 0o755\nbin := 0b1010_1010\n",
    "flt := 1_000.5e-3\nimg := 3i + 2.5i\n",
    "ch := make(chan int, 10)\ngo func() { ch <- 1 }()\nval := <-ch\n",
    "type Reader interface {\n\tRead(p []byte) (n int, err error)\n}\n",
    "type Config struct {\n\tHost string `json:\"host\"`\n\tPort int    `json:\"port\"`\n}\n",
    "truncRaw := `never closed",
    "truncInterp := \"unterminated string",
    "truncComment := /* open comment",
    "q := 'trunc",
    CONSUMER_DOCUMENT,
];

fn coalesced(spans: &[Span]) -> Vec<(Tok, usize, usize)> {
    coalesce_spans(spans)
        .iter()
        .map(|span| (span.kind, span.start, span.end))
        .collect()
}

fn assert_tiling(spans: &[Span], total_len: usize) {
    let mut next = 0usize;
    for span in spans {
        assert_eq!(span.start, next, "gap/overlap at {}", span.start);
        assert!(span.end > span.start, "empty span at {}", span.start);
        next = span.end;
    }
    assert_eq!(next, total_len, "spans must tile entire input");
}

#[test]
fn every_split_matches_whole_run_on_all_fixtures() {
    for &fixture in FIXTURES {
        let whole = highlight("go", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("go").expect("go route exists");
            lexer
                .feed(&fixture.as_bytes()[..split])
                .expect("first feed");
            lexer
                .feed(&fixture.as_bytes()[split..])
                .expect("second feed");
            lexer.finish().expect("finish after full input");
            let split_coalesced = coalesced(lexer.spans());
            assert_eq!(
                split_coalesced, whole_coalesced,
                "fixture {fixture:?} diverges at split {split}"
            );
        }
    }
}

#[test]
fn three_way_splits_match_whole_run() {
    for &fixture in FIXTURES {
        let whole = highlight("go", fixture);
        let whole_coalesced = coalesced(&whole);
        for first in 0..fixture.len() {
            for second in first..=fixture.len() {
                let mut lexer = ResumableLexer::new("go").expect("go route exists");
                lexer.feed(&fixture.as_bytes()[..first]).expect("feed 1");
                lexer
                    .feed(&fixture.as_bytes()[first..second])
                    .expect("feed 2");
                lexer.feed(&fixture.as_bytes()[second..]).expect("feed 3");
                lexer.finish().expect("finish");
                assert_eq!(
                    coalesced(lexer.spans()),
                    whole_coalesced,
                    "fixture {fixture:?} diverges at splits {first}/{second}"
                );
            }
        }
    }
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let routed = highlight("go", fixture);
        let mut direct = Vec::new();
        lex_go_into(fixture, &mut direct);
        assert_eq!(
            coalesced(&routed),
            coalesced(&direct),
            "route and direct scanner disagree for {fixture:?}"
        );
    }
}

#[test]
fn malformed_inputs_lex_with_bounded_output() {
    for hostile in [
        "`\"`\"`",
        "\"\\",
        "'\\",
        "/*/*/",
        "0x_G",
        "0b_2",
        "999_.",
        "ch <- <- <-",
        "```",
        "'''",
    ] {
        let mut spans = Vec::new();
        lex_go_into(hostile, &mut spans);
        // Bounded: every emitted span stays inside the input.
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        // The resumable path must not panic on any split of hostile input.
        for split in 0..=hostile.len() {
            let mut lexer = ResumableLexer::new("go").expect("route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn go_capability_row_is_versioned() {
    let row: GoCapabilityV1 = GO_CAPABILITY_V1;
    assert_eq!(row.version, 1);
    const {
        assert!(GO_CAPABILITY_V1.incremental);
    };
    const {
        assert!(GO_CAPABILITY_V1.strings_and_runes);
    };
    const {
        assert!(GO_CAPABILITY_V1.numbers_and_operators);
    };
}

#[test]
fn malformed_bounds_and_error_handling() {
    let mut lexer = ResumableLexer::with_limits("go", 32).expect("valid route");
    let ok_feed = lexer.feed(b"package main\n");
    assert!(ok_feed.is_ok());

    // Exceeding the pending cap triggers SuffixTooLong
    let long_chunk = "`".to_string() + &"a".repeat(50);
    let err = lexer.feed(long_chunk.as_bytes()).unwrap_err();
    assert_eq!(err.code(), "SUFFIX_TOO_LONG");
    match err {
        ResumeError::SuffixTooLong { held, cap } => {
            assert!(held > 32);
            assert_eq!(cap, 32);
        }
        _ => panic!("expected SuffixTooLong"),
    }

    // Finishing seals the lexer
    let mut normal_lexer = ResumableLexer::new("go").expect("valid route");
    normal_lexer.feed(b"func f() int { return 1 }\n").unwrap();
    normal_lexer.finish().unwrap();
    assert!(normal_lexer.is_finished());

    // Feed after finish is refused
    let finish_err = normal_lexer.feed(b"extra").unwrap_err();
    assert_eq!(finish_err, ResumeError::AlreadyFinished);
    assert_eq!(finish_err.code(), "ALREADY_FINISHED");

    // Double finish is also refused
    assert_eq!(
        normal_lexer.finish().unwrap_err(),
        ResumeError::AlreadyFinished
    );
}

#[test]
fn negative_control_tiling_oracle_detects_gap() {
    let gapped = vec![
        Span {
            kind: Tok::Plain,
            start: 0,
            end: 1,
        },
        Span {
            kind: Tok::Plain,
            start: 2,
            end: 3,
        },
    ];
    let detected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_tiling(&gapped, 3);
    }))
    .is_err();
    assert!(detected, "tiling oracle must catch a gap");
}
