//! FCB-022.A C# adversarial split corpus: every representative fixture is
//! lexed whole and at every byte split through the shared FCB-021 resumable
//! engine; coalesced token meaning and exact source tiling must agree.
//! Malformed and truncated inputs must lex with bounded work and truthful
//! provisional tails, never panic.

use franken_markdown::highlight::highlight;
use franken_markdown::highlight::Tok;
use franken_markdown::lang_csharp::lex_csharp_into;
use franken_markdown::resume::{coalesce_spans, ResumableLexer};
use franken_markdown::highlight::Span;

/// Representative C# fixtures exercising comments, preprocessor directives,
/// verbatim/interpolated/raw strings, character literals, numeric suffixes,
/// keywords, types, calls, and truncated constructs.
const FIXTURES: &[&str] = &[
    "var x = 1;\n",
    "public class Widget { var count = Read(); }\n",
    "// line comment\n/* block\ncomment */ var y = 2;\n",
    "var s = \"line1\nline2\";\n",
    "var v = @\"line1\nline2 with \"\"quote\"\" end\";\n",
    "var m = $\"x {args[\"inner\"]} y\";\n",
    "var r = \"\"\"raw \"partial\" text\"\"\";\n",
    "var c = 'x';\nvar esc = '\\n';\n",
    "var n = 0xFF_ul + 0b1010 + 1.5m + 3e-2f;\n",
    "#nullable enable\nvar z = default;\n",
    "namespace App { class Program { static void Main() { } } }\n",
    "var broken = \"trunc",
];

fn coalesced(spans: &[Span]) -> Vec<(Tok, usize, usize)> {
    coalesce_spans(spans)
        .iter()
        .map(|span| (span.kind, span.start, span.end))
        .collect()
}

#[test]
fn every_split_matches_whole_run_on_all_fixtures() {
    for fixture in FIXTURES {
        let whole = highlight("csharp", fixture);
        let whole_coalesced = coalesced(&whole);

        for split in 0..=fixture.len() {
            let mut lexer = ResumableLexer::new("csharp").expect("csharp route exists");
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
fn scanner_direct_matches_highlight_route() {
    for fixture in FIXTURES {
        let routed = highlight("csharp", fixture);
        let mut direct = Vec::new();
        lex_csharp_into(fixture, &mut direct);
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
        "$\"{a",
        "@\"",
        "/*",
        "'",
        "@",
        "#",
        "0x",
    ] {
        let mut spans = Vec::new();
        lex_csharp_into(hostile, &mut spans);
        for span in &spans {
            assert!(span.start <= span.end);
            assert!(span.end <= hostile.len());
        }
        for split in 0..hostile.len() {
            let mut lexer = ResumableLexer::new("csharp").expect("route");
            let _ = lexer.feed(&hostile.as_bytes()[..split]);
            let _ = lexer.feed(&hostile.as_bytes()[split..]);
            let _ = lexer.finish();
        }
    }
}

#[test]
fn truncated_interpolation_holds_until_closed() {
    // A feed boundary inside the interpolation braces must not release a
    // wrong span; the resumed lex reclassifies identically.
    let mut lexer = ResumableLexer::new("csharp").expect("route");
    lexer.feed(b"var m = $\"x {args[").expect("feed prefix");
    lexer.feed(b"\"inner\"]} y\";").expect("feed rest");
    lexer.finish().expect("finish");
    let whole = highlight("csharp", "var m = $\"x {args[\"inner\"]} y\";");
    assert_eq!(coalesced(lexer.spans()), coalesced(&whole));
}
