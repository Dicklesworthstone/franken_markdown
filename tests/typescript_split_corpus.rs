//! FCB-022.7 TypeScript adversarial split corpus: every representative fixture
//! is lexed whole and at every byte split through the shared FCB-021
//! resumable engine; coalesced token meaning and exact source tiling must
//! agree. Malformed and truncated inputs must lex with bounded work, never panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_typescript::{
    TYPESCRIPT_CAPABILITY_V1, TypeScriptCapabilityV1, lex_typescript_into,
};
use franken_markdown::resume::{ResumableLexer, ResumeError, coalesce_spans};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/typescript_route/consumer_document.ts");

/// Representative TypeScript fixtures exercising the bead's feature list:
/// type contexts, generics vs comparisons, template literal types, type assertions
/// (as/satisfies), type predicates (is/asserts), access modifiers, enums,
/// and composed JavaScript constructs with truncations.
const FIXTURES: &[&str] = &[
    "type Id = string | number;\n",
    "interface Box<T> { val: T; }\n",
    "function max<T>(a: T, b: T): boolean { return a < b && b > 0; }\n",
    "type Event = `on${string}`;\n",
    "const x = 42 as const;\nconst c = {} satisfies Record<string, unknown>;\n",
    "function isNum(x: unknown): x is number { return typeof x === \"number\"; }\n",
    "class A { public readonly x: string; private y: number; protected run(): void {} }\n",
    "enum E { A = 1, B = 2 }\nnamespace N { export const v = 10; }\n",
    "const r = /pattern[a-z/]/i;\nconst d = a / b / c;\n",
    "const s = `outer: ${x + 1} and ${`nested ${y}`}`;\n",
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
        let whole = highlight("typescript", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("typescript").expect("typescript route exists");
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
        let whole = highlight("typescript", fixture);
        let whole_coalesced = coalesced(&whole);
        for first in 0..fixture.len() {
            for second in first..=fixture.len() {
                let mut lexer =
                    ResumableLexer::new("typescript").expect("typescript route exists");
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
fn aliases_route_to_typescript_lexer() {
    for alias in &["typescript", "ts"] {
        let routed = highlight(alias, "type Answer = number;\n");
        let mut direct = Vec::new();
        lex_typescript_into("type Answer = number;\n", &mut direct);
        assert_eq!(
            coalesced(&routed),
            coalesced(&direct),
            "alias {alias} does not match direct typescript lexer"
        );
    }
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let routed = highlight("typescript", fixture);
        let mut direct = Vec::new();
        lex_typescript_into(fixture, &mut direct);
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
        "type <<>> = ;",
        "interface { { {",
        "\\",
    ] {
        let mut spans = Vec::new();
        lex_typescript_into(hostile, &mut spans);
        // Bounded: every emitted span stays inside the input.
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        // The resumable path must not panic on any split of hostile input.
        for split in 0..=hostile.len() {
            let mut lexer = ResumableLexer::new("typescript").expect("route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn typescript_capability_row_is_versioned() {
    let row: TypeScriptCapabilityV1 = TYPESCRIPT_CAPABILITY_V1;
    assert_eq!(row.version, 1);
    const {
        assert!(TYPESCRIPT_CAPABILITY_V1.incremental);
    };
    const {
        assert!(TYPESCRIPT_CAPABILITY_V1.type_keywords);
    };
    const {
        assert!(TYPESCRIPT_CAPABILITY_V1.generics_vs_comparisons);
    };
    const {
        assert!(TYPESCRIPT_CAPABILITY_V1.template_literal_types);
    };
    const {
        assert!(TYPESCRIPT_CAPABILITY_V1.satisfies_and_as_casts);
    };
}

#[test]
fn malformed_bounds_and_error_handling() {
    let mut lexer = ResumableLexer::with_limits("typescript", 32).expect("valid route");
    let ok_feed = lexer.feed(b"type X = 1;\n");
    assert!(ok_feed.is_ok());

    // Exceeding the pending cap triggers SuffixTooLong
    let long_chunk = "`".to_string() + &"a".repeat(50);
    let err = lexer.feed(long_chunk.as_bytes()).unwrap_err();
    assert_eq!(err.code(), "SUFFIX_TOO_LONG");
    if let ResumeError::SuffixTooLong { held, cap } = err {
        assert!(held > 32);
        assert_eq!(cap, 32);
    } else {
        assert!(false, "expected SuffixTooLong");
    }

    // Finishing seals the lexer
    let mut normal_lexer = ResumableLexer::new("typescript").expect("valid route");
    normal_lexer.feed(b"interface Config { id: string; }\n").unwrap();
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
