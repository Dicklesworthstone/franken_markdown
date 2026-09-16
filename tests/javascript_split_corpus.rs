//! FCB-022.6 JavaScript adversarial split corpus: every representative fixture
//! is lexed whole and at every byte split through the shared FCB-021
//! resumable engine; coalesced token meaning and exact source tiling must
//! agree. Malformed and truncated inputs must lex with bounded work, never panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_javascript::{
    JAVASCRIPT_CAPABILITY_V1, JavaScriptCapabilityV1, lex_javascript_into,
};
use franken_markdown::resume::{ResumableLexer, ResumeError, coalesce_spans};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/javascript_route/consumer_document.js");

/// Representative JavaScript fixtures exercising the bead's feature list:
/// regex vs division, template literal interpolation stacks, comments,
/// Annex B syntax, numeric forms, keyword/type classifications, and truncations.
const FIXTURES: &[&str] = &[
    "const x = 1;\n",
    "// line comment\n/* block\ncomment */ const y = 2;\n",
    "<!-- annex B open comment\n--> annex B close comment\n",
    "const re = /pattern[a-z/]/i;\n",
    "const div = a / b / c;\n",
    "return /pattern/.test(str);\n",
    "const s = `outer: ${x + 1} and ${`nested ${y}`}`;\n",
    "const str = \"escaped \\\"quotes\\\" and \\n \\t \\u0041\";\n",
    "const cont = \"line 1 \\\nline 2\";\n",
    "const nums = 0xDEAD_BEEF + 0o755 + 0b1010 + 123n + 1_000.5e-3 + .5;\n",
    "async function run(opts = {}) { return await fetch(opts.url ?? '/'); }\n",
    "truncString = \"unterminated string",
    "truncTemplate = `unterminated ${nested",
    "truncRegex = /never closed regex",
    "truncComment = /* unclosed comment",
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
        let whole = highlight("javascript", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("javascript").expect("javascript route exists");
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
        let whole = highlight("javascript", fixture);
        let whole_coalesced = coalesced(&whole);
        for first in 0..fixture.len() {
            for second in first..=fixture.len() {
                let mut lexer =
                    ResumableLexer::new("javascript").expect("javascript route exists");
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
fn aliases_route_to_javascript_lexer() {
    for alias in &["javascript", "js", "mjs", "cjs", "jsx", "typescript", "ts", "tsx"] {
        let routed = highlight(alias, "const x = 42;\n");
        let mut direct = Vec::new();
        lex_javascript_into("const x = 42;\n", &mut direct);
        assert_eq!(
            coalesced(&routed),
            coalesced(&direct),
            "alias {alias} does not match direct javascript lexer"
        );
    }
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let routed = highlight("javascript", fixture);
        let mut direct = Vec::new();
        lex_javascript_into(fixture, &mut direct);
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
        "`${`",
        "${${${${",
        "///",
        "/**/",
        "0x_G",
        "0b_2",
        "123n_",
        "/[/]/",
        "`\"'`",
        "\\",
    ] {
        let mut spans = Vec::new();
        lex_javascript_into(hostile, &mut spans);
        // Bounded: every emitted span stays inside the input.
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        // The resumable path must not panic on any split of hostile input.
        for split in 0..=hostile.len() {
            let mut lexer = ResumableLexer::new("javascript").expect("route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn javascript_capability_row_is_versioned() {
    let row: JavaScriptCapabilityV1 = JAVASCRIPT_CAPABILITY_V1;
    assert_eq!(row.version, 1);
    const {
        assert!(JAVASCRIPT_CAPABILITY_V1.incremental);
    };
    const {
        assert!(JAVASCRIPT_CAPABILITY_V1.regex_vs_division);
    };
    const {
        assert!(JAVASCRIPT_CAPABILITY_V1.template_interpolations);
    };
    const {
        assert!(JAVASCRIPT_CAPABILITY_V1.numeric_forms);
    };
}

#[test]
fn malformed_bounds_and_error_handling() {
    let mut lexer = ResumableLexer::with_limits("javascript", 32).expect("valid route");
    let ok_feed = lexer.feed(b"const x = 1;\n");
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
    let mut normal_lexer = ResumableLexer::new("javascript").expect("valid route");
    normal_lexer.feed(b"function fn() { return 1; }\n").unwrap();
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
