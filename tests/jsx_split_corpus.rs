//! FCB-022.8 JSX adversarial split corpus: every representative fixture
//! is lexed whole and at every byte split through the shared FCB-021
//! resumable engine; coalesced token meaning and exact source tiling must
//! agree. Malformed and truncated inputs must lex with bounded work, never panic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_jsx::{JSX_CAPABILITY_V1, JsxCapabilityV1, lex_jsx_into};
use franken_markdown::resume::{ResumableLexer, ResumeError, coalesce_spans};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/jsx_route/consumer_document.jsx");

/// Representative JSX fixtures exercising the bead's feature list:
/// tags/components, attributes, quoted braces as literal strings,
/// fragments, embedded expressions, character entities, comparisons vs tags,
/// comments inside tags/expressions, and truncations.
const FIXTURES: &[&str] = &[
    "const el = <div className=\"test\">Hello World</div>;\n",
    "const el = <><img src=\"icon.png\" /><Button disabled /></>;\n",
    "const el = <div count={x + 1}>{items.map(i => <span key={i}>{i}</span>)}</div>;\n",
    "const el = <div title=\"{literal braces in attribute}\" alt='{more braces}' />;\n",
    "const el = <div>&copy; 2026 &amp; &lt;FrankenCode&gt; &#169;</div>;\n",
    "const cmp = a < b && b > c;\nconst tag = <tag attr=\"val\">content</tag>;\n",
    "const s = <div>{/* JSX comment */}{`nested ${val}` + \"str\"}</div>;\n",
    "const r = /pattern/i;\nconst d = a / b / c;\nconst el = <Comp regex={/foo/} />;\n",
    "truncTag = <div className=\"unterminated",
    "truncExpr = <div>{x + 1",
    "truncString = <div title=\"unterminated string",
    "truncFragment = <>open fragment",
    "truncComment = <div>{/* unclosed comment",
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
        let whole = highlight("jsx", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("jsx").expect("jsx route exists");
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
        let whole = highlight("jsx", fixture);
        let whole_coalesced = coalesced(&whole);
        for first in 0..fixture.len() {
            for second in first..=fixture.len() {
                let mut lexer = ResumableLexer::new("jsx").expect("jsx route exists");
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
fn aliases_route_to_jsx_lexer() {
    let routed = highlight("jsx", "const x = <div />;\n");
    let mut direct = Vec::new();
    lex_jsx_into("const x = <div />;\n", &mut direct);
    assert_eq!(
        coalesced(&routed),
        coalesced(&direct),
        "routed jsx does not match direct jsx lexer"
    );
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let routed = highlight("jsx", fixture);
        let mut direct = Vec::new();
        lex_jsx_into(fixture, &mut direct);
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
        "<div <div <div",
        "</></></>",
        "<{x + 1}>",
        "<div title=\"unclosed",
        "<div attr={ { {",
        "<div>{/* unclosed",
        "&unterminated;",
        "&; &#; &#x;",
        "<>>>><<<<",
        "// unclosed line comment",
        "/* unclosed block comment",
        "\\",
    ] {
        let mut spans = Vec::new();
        lex_jsx_into(hostile, &mut spans);
        // Bounded: every emitted span stays inside the input.
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        // The resumable path must not panic on any split of hostile input.
        for split in 0..=hostile.len() {
            let mut lexer = ResumableLexer::new("jsx").expect("route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn jsx_capability_row_is_versioned() {
    let row: JsxCapabilityV1 = JSX_CAPABILITY_V1;
    assert_eq!(row.version, 1);
    const {
        assert!(JSX_CAPABILITY_V1.incremental);
    };
    const {
        assert!(JSX_CAPABILITY_V1.tag_and_attribute_transitions);
    };
    const {
        assert!(JSX_CAPABILITY_V1.embedded_expressions);
    };
    const {
        assert!(JSX_CAPABILITY_V1.fragments);
    };
    const {
        assert!(JSX_CAPABILITY_V1.entities);
    };
}

#[test]
fn malformed_bounds_and_error_handling() {
    let mut lexer = ResumableLexer::with_limits("jsx", 32).expect("valid route");
    let ok_feed = lexer.feed(b"const x = 1;\n");
    assert!(ok_feed.is_ok());

    // Exceeding the pending cap triggers SuffixTooLong
    let long_chunk = "<div className=\"".to_string() + &"a".repeat(50);
    let err = lexer.feed(long_chunk.as_bytes()).unwrap_err();
    assert_eq!(err.code(), "SUFFIX_TOO_LONG");
    if let ResumeError::SuffixTooLong { held, cap } = err {
        assert!(held > 32);
        assert_eq!(cap, 32);
    } else {
        assert!(false, "expected SuffixTooLong");
    }

    // Finishing seals the lexer
    let mut normal_lexer = ResumableLexer::new("jsx").expect("valid route");
    normal_lexer.feed(b"const el = <div />;\n").unwrap();
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
