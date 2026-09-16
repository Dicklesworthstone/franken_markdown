//! FCB-022.11 C++ adversarial split corpus: every representative fixture
//! is lexed whole and at every byte split through the shared FCB-021
//! resumable engine; coalesced token meaning and exact source tiling must
//! agree. Malformed and truncated inputs must lex with bounded work, never panic.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lang_cplusplus::{
    CPP_CAPABILITY_V1, CppCapabilityV1, lex_cplusplus_into,
};
use franken_markdown::resume::{ResumableLexer, ResumeError, coalesce_spans};

const CONSUMER_DOCUMENT: &str = include_str!("fixtures/cpp_route/consumer_document.cpp");

/// Representative C++ fixtures exercising the bead's feature list:
/// raw strings with custom delimiters, encoding prefixes (u8/u/U/L),
/// digit separators, hex-float p-exponents, number suffixes,
/// conservative angle brackets, preprocessor directives, comments,
/// and truncations.
const FIXTURES: &[&str] = &[
    "int x = 1;\n",
    "// line comment\n/* block\ncomment */ int y = 2;\n",
    "auto s = R\"foo(hello \"world\")foo\";\n",
    "auto s2 = R\"delim(embedded )\" in content)delim\";\n",
    "auto u8s = u8\"utf8\"; auto ws = L\"wide\"; auto u8c = u8'a'; auto wc = L'w';\n",
    "int n = 1'000'000 + 0b1010'0101 + 0x1.fp3 + 42ULL + 3.14f;\n",
    "#include <vector>\n#define FOO 1\n#ifdef FOO\n#endif\n",
    "std::vector<int> v; bool b = a < b && c > d;\n",
    "auto op = a <=> b; auto p = obj->*ptr;\n",
    "truncStr = \"unterminated string",
    "truncRaw = R\"delim(unterminated raw string",
    "truncComment = /* unterminated comment",
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
        let whole = highlight("cpp", fixture);
        assert_tiling(&whole, fixture.len());
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("cpp").expect("cpp route exists");
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
        let whole = highlight("cpp", fixture);
        let whole_coalesced = coalesced(&whole);
        for first in 0..fixture.len() {
            for second in first..=fixture.len() {
                let mut lexer = ResumableLexer::new("cpp").expect("cpp route exists");
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
fn aliases_route_to_cpp_lexer() {
    let aliases = &["cpp", "c++", "cc", "cxx", "hpp", "hxx", "hh", "inl"];
    let probe = "auto s = R\"foo(bar)foo\";";
    let reference = highlight("cpp", probe);
    for &alias in aliases {
        let spans = highlight(alias, probe);
        assert_eq!(
            coalesced(&spans),
            coalesced(&reference),
            "alias {alias} does not match cpp reference"
        );
    }
}

#[test]
fn scanner_direct_matches_highlight_route() {
    for &fixture in FIXTURES {
        let mut direct = Vec::new();
        lex_cplusplus_into(fixture, &mut direct);
        assert_tiling(&direct, fixture.len());

        let via_highlight = highlight("cpp", fixture);
        assert_eq!(
            coalesced(&direct),
            coalesced(&via_highlight),
            "direct scanner must match highlight router"
        );
    }
}

#[test]
fn cpp_capability_row_is_versioned() {
    let row: CppCapabilityV1 = CPP_CAPABILITY_V1;
    assert_eq!(row.version, 1);
    assert!(row.incremental);
    assert!(row.raw_strings);
    assert!(row.encoding_prefixes);
    assert!(row.numeric_forms);
    assert!(row.conservative_templates);
}

#[test]
fn negative_control_tiling_oracle_detects_gap() {
    let bad = vec![
        Span {
            kind: Tok::Keyword,
            start: 0,
            end: 3,
        },
        Span {
            kind: Tok::Plain,
            start: 4,
            end: 10,
        },
    ];
    let result = std::panic::catch_unwind(|| {
        assert_tiling(&bad, 10);
    });
    assert!(
        result.is_err(),
        "tiling oracle must reject input with gaps"
    );
}

#[test]
fn malformed_bounds_and_error_handling() {
    let mut lexer = ResumableLexer::new("cpp").expect("cpp route exists");
    assert_eq!(lexer.lang(), "cpp");
    assert_eq!(lexer.pending_bytes(), 0);
    assert!(!lexer.is_finished());

    // Feeding after finish must refuse with AlreadyFinished
    lexer.feed(b"int x = 1;").expect("valid feed");
    lexer.finish().expect("valid finish");
    assert!(lexer.is_finished());
    let err = lexer
        .feed(b"int y = 2;")
        .expect_err("feed after finish must fail");
    assert_eq!(err, ResumeError::AlreadyFinished);
    assert_eq!(err.code(), "ALREADY_FINISHED");
}

#[test]
fn malformed_inputs_lex_with_bounded_output() {
    // Extreme adversarial inputs: deep delimiters, unclosed quotes, null bytes,
    // raw strings with weird characters, huge digit runs
    let adversarial: &[&[u8]] = &[
        b"R\"(unclosed\0null\xffinvalid",
        b"1'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0'0",
        b"/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/*/",
        b"u8R\"invalid_delim(hello",
        b"0x1.ffffffffffffffffp+1024ULL",
        b"\"\\x\\u\\U\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"\\\"",
        b"<=>=>>=<<=->*::...",
    ];

    for &bytes in adversarial {
        if let Ok(text) = std::str::from_utf8(bytes) {
            let spans = highlight("cpp", text);
            assert_tiling(&spans, text.len());
            assert!(
                spans.len() <= text.len() + 1,
                "span count must be bounded by byte count"
            );
        }
    }
}
