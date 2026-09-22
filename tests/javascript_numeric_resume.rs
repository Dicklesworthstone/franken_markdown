//! Numeric exponent prefixes must remain replayable across arbitrary feeds.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, highlight};
use franken_markdown::resume::{ResumableLexer, ResumeError, coalesce_spans};

const ROUTES: &[&str] = &[
    "javascript",
    "js",
    "mjs",
    "cjs",
    "typescript",
    "ts",
    "jsx",
    "tsx",
];

fn assert_equivalent(route: &str, source: &str, spans: &[Span]) {
    let mut end = 0;
    for span in spans {
        assert_eq!(span.start, end, "{route}: gap/overlap in {source:?}");
        assert!(span.end > span.start && span.end <= source.len());
        end = span.end;
    }
    assert_eq!(end, source.len(), "{route}: incomplete source tiling");
    assert_eq!(
        coalesce_spans(spans),
        coalesce_spans(&highlight(route, source)),
        "{route}: changed token classification for {source:?}"
    );
}

fn assert_every_split(route: &str, source: &str) {
    for split in 0..=source.len() {
        let mut lexer = ResumableLexer::new(route).unwrap();
        lexer.feed(&source.as_bytes()[..split]).unwrap();
        lexer.feed(&source.as_bytes()[split..]).unwrap();
        lexer.finish().unwrap();
        assert_equivalent(route, source, lexer.spans());
    }
}

#[test]
fn numeric_forms_match_whole_highlighting_at_every_split_on_all_js_routes() {
    let source = "const nums = 0xDEAD_BEEF + 0o755 + 0b1010 + 123n + 1_000.5e-3 + .5;\n\
                  const powers = [1e3, 2E+4, .5e-2, 3.e+2, 4E-5, 0x1e + 2];\n";
    for &route in ROUTES {
        assert_every_split(route, source);
    }
}

#[test]
fn exponent_markers_and_signs_survive_all_three_way_splits_and_byte_feeds() {
    for source in ["1e3", "1e-3", "1E+3", ".5e-2", "1.e+2", "1_000.5e-3"] {
        for &route in ROUTES {
            for first in 0..=source.len() {
                for second in first..=source.len() {
                    let mut lexer = ResumableLexer::new(route).unwrap();
                    lexer.feed(&source.as_bytes()[..first]).unwrap();
                    lexer.feed(&source.as_bytes()[first..second]).unwrap();
                    lexer.feed(&source.as_bytes()[second..]).unwrap();
                    lexer.finish().unwrap();
                    assert_equivalent(route, source, lexer.spans());
                }
            }
            let mut lexer = ResumableLexer::new(route).unwrap();
            let mut emitted = Vec::new();
            for byte in source.as_bytes() {
                lexer.feed(std::slice::from_ref(byte)).unwrap();
                emitted.extend(lexer.take_spans());
            }
            lexer.finish().unwrap();
            emitted.extend(lexer.take_spans());
            assert_equivalent(route, source, &emitted);
        }
    }
}

#[test]
fn checkpoint_restoration_retains_the_mantissa_before_an_exponent_is_complete() {
    const PREFIX: &str = "const value = ";
    for &route in ROUTES {
        for incomplete in ["1_000.5e", "1_000.5e+", "1_000.5e-", ".5E", ".5E+", ".5E-"] {
            let mut lexer = ResumableLexer::new(route).unwrap();
            lexer
                .feed(format!("{PREFIX}{incomplete}").as_bytes())
                .unwrap();
            assert!(
                lexer.emitted_bytes() <= PREFIX.len(),
                "{route} released a partial number: {incomplete}"
            );
            let mut emitted = lexer.take_spans();
            let checkpoint = lexer.try_checkpoint(7).unwrap();
            let mut restored =
                ResumableLexer::from_checkpoint(&checkpoint, 7, checkpoint.byte_offset).unwrap();
            restored.feed(b"3;").unwrap();
            restored.finish().unwrap();
            emitted.extend(restored.take_spans());
            assert_equivalent(route, &format!("{PREFIX}{incomplete}3;"), &emitted);
        }
    }
}

#[test]
fn incomplete_and_malformed_exponents_keep_the_batch_eof_classification() {
    for &route in ROUTES {
        for source in [
            "1e", "1E", "1e+", "1e-", "1E+", "1E-", "1.e", "1e + 3", "1e--2", "1e_2", "1e-xy",
            "0x1e + 2",
        ] {
            assert_every_split(route, source);
        }
    }
}

#[test]
fn an_unfinished_exponent_respects_the_replay_budget_and_refuses_transactionally() {
    let mut lexer = ResumableLexer::with_limits("javascript", 8).unwrap();
    lexer.feed(b"1_000e-").unwrap();
    let checkpoint = lexer.checkpoint(9);
    let emitted = lexer.spans().to_vec();
    assert!(matches!(
        lexer.feed(b"12345;"),
        Err(ResumeError::SuffixTooLong { cap: 8, .. })
    ));
    assert_eq!(lexer.checkpoint(9), checkpoint);
    assert_eq!(lexer.spans(), emitted);
    lexer.feed(b"3").unwrap();
    lexer.finish().unwrap();
    assert_equivalent("javascript", "1_000e-3", lexer.spans());
}
