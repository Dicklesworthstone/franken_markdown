//! FCB-022.A Python adversarial split corpus: every representative fixture
//! is lexed whole and at every byte split through the shared FCB-021
//! resumable engine; coalesced token meaning and exact source tiling must
//! agree. Malformed and truncated inputs must lex with bounded work and
//! truthful provisional tails, never panic.

use franken_markdown::highlight::highlight;
use franken_markdown::resume::{coalesce_spans, ResumableLexer};
use franken_markdown::lang_python::lex_python_into;
use franken_markdown::highlight::Span;

/// Representative Python fixtures exercising the bead's feature list:
/// triple/raw/byte/f strings, prefixes, escapes, line continuations,
/// comments, indentation, numbers, keywords, calls, truncated quotes.
const FIXTURES: &[&str] = &[
    "x = 1\n",
    "def render(self):\n    return f\"frame {self.n:03}\"\n",
    "doc = \"\"\"a # not comment\nb'''\n\"\"\"\n",
    "raw = r\"backslash \\\" stays\"\npath = rb'C:\\dir'\n",
    "value = 0xFF_00 + 1.5e-3 + 4j\n",
    "if x is None:\n\tpass\nelif y:\n    z: int = 3\n",
    "@decorator\ndef fn(a, b=2):\n    return a := b\n",
    "s = 'broken\nnext = \"ok\"'\n",
    "t = \"\"\"unterminated\n",
    "q = \"trunc",
    "partial = f\"{'nest'} more",
    "width = \\\n    42\n# comment\n",
];

fn coalesced(spans: &[Span]) -> Vec<(franken_markdown::highlight::Tok, usize, usize)> {
    coalesce_spans(spans)
        .iter()
        .map(|span| (span.kind, span.start, span.end))
        .collect()
}

#[test]
fn every_split_matches_whole_run_on_all_fixtures() {
    for fixture in FIXTURES {
        let whole = highlight("python", fixture);
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("python").expect("python route exists");
            lexer.feed(&fixture.as_bytes()[..split]).expect("first feed");
            lexer.feed(&fixture.as_bytes()[split..]).expect("second feed");
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
    for fixture in FIXTURES {
        let whole = highlight("python", fixture);
        let whole_coalesced = coalesced(&whole);
        for first in 0..fixture.len() {
            for second in first..=fixture.len() {
                let mut lexer = ResumableLexer::new("python").expect("python route exists");
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
    for fixture in FIXTURES {
        let routed = highlight("python", fixture);
        let mut direct = Vec::new();
        lex_python_into(fixture, &mut direct);
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
        "\"\"\"\"\"",
        "\"\"\"\\",
        "f\"{'a'",
        "x = ",
        ")))",
        "@@@",
        "'''''''''",
        "b",
    ] {
        let mut spans = Vec::new();
        lex_python_into(hostile, &mut spans);
        // Bounded: every emitted span stays inside the input.
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        // The resumable path must not panic on any split of hostile input.
        for split in 0..hostile.len() {
            let mut lexer = ResumableLexer::new("python").expect("route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn truncated_quote_is_held_not_wrongly_classified() {
    // A chunk ending in `"` could become a triple quote; the provisional
    // tail must extend to end of input so the next chunk reclassifies.
    let mut lexer = ResumableLexer::new("python").expect("route");
    lexer.feed(b"doc = \"").expect("feed prefix");
    let held = lexer.spans();
    // Nothing after the `=` region may be finalized yet.
    for span in held {
        assert!(span.end <= 6, "premature span {span:?}");
    }
    lexer.feed(b"\"\"text\n").expect("feed rest");
    lexer.finish().expect("finish");
    let whole = highlight("python", "doc = \"\"\"text\n");
    assert_eq!(coalesced(lexer.spans()), coalesced(&whole));
}
