//! Adversarial split corpus for the YAML incremental lexer (FCB-022, YAML
//! route / fcb-9vx.19). Whole and chunked runs coalesce identically at
//! every character boundary with exact source tiling.

#![forbid(unsafe_code)]

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::lex_yaml::{LexYamlError, ResumableYamlLexer, YAML_CAPABILITY_V1};

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

fn char_by_char(source: &str) -> Vec<Span> {
    let mut lexer = ResumableYamlLexer::new();
    for ch in source.chars() {
        let mut buf = [0u8; 4];
        lexer.feed(ch.encode_utf8(&mut buf)).expect("char feed");
    }
    lexer.finish().expect("EOF flush");
    lexer.spans().to_vec()
}

fn split_at(source: &str, split: usize) -> Vec<Span> {
    let mut lexer = ResumableYamlLexer::new();
    let (a, b) = source.split_at(split.min(source.len()));
    if !a.is_empty() {
        lexer.feed(a).expect("first chunk feeds");
    }
    if !b.is_empty() {
        lexer.feed(b).expect("second chunk feeds");
    }
    lexer.finish().expect("EOF flush");
    lexer.spans().to_vec()
}

fn assert_chunked_equivalent(source: &str) {
    let whole = highlight("yaml", source);
    assert_tiling(&whole, source.len());
    let whole_runs = coalesce(&whole);

    let every_char = char_by_char(source);
    assert_tiling(&every_char, source.len());
    assert_eq!(
        coalesce(&every_char),
        whole_runs,
        "character-by-character feeding must coalesce to the whole run"
    );

    for split in 0..=source.len() {
        if !source.is_char_boundary(split) {
            continue;
        }
        let chunked = split_at(source, split);
        assert_tiling(&chunked, source.len());
        assert_eq!(
            coalesce(&chunked),
            whole_runs,
            "split at {split} must coalesce to the whole run"
        );
    }
}

const KEY_VALUE_TYPES: &str = concat!(
    "name: my-app\n",
    "version: 1.0\n",
    "enabled: true\n",
    "debug: false\n",
    "count: 42\n"
);

const NESTED_AND_LISTS: &str = concat!(
    "server:\n",
    "  host: localhost\n",
    "  ports:\n",
    "    - 80\n",
    "    - 443\n",
    "  tls:\n",
    "    enabled: yes\n"
);

const QUOTED_AND_MULTILINE: &str = concat!(
    "double: \"quoted \\\"inner\\\"\"\n",
    "single: 'literal \\\\ no escape'\n",
    "block: |\n",
    "  line one\n",
    "  line two\n",
    "folded: >-\n",
    "  folded text\n"
);

#[test]
fn key_value_types_are_split_safe() {
    assert_chunked_equivalent(KEY_VALUE_TYPES);
}

#[test]
fn nested_structures_and_lists_are_split_safe() {
    assert_chunked_equivalent(NESTED_AND_LISTS);
}

#[test]
fn quoted_and_block_scalars_are_split_safe() {
    assert_chunked_equivalent(QUOTED_AND_MULTILINE);
}

#[test]
fn malformed_bounds_and_error_handling() {
    let mut lexer = ResumableYamlLexer::with_limits(32);
    let ok_feed = lexer.feed("key: value\n");
    assert!(ok_feed.is_ok());

    // Exceeding the pending cap triggers SuffixTooLong
    let long_chunk = "# ".to_string() + &"a".repeat(50);
    let err = lexer.feed(&long_chunk).unwrap_err();
    assert_eq!(err.code(), "SUFFIX_TOO_LONG");
    match err {
        LexYamlError::SuffixTooLong { held, cap } => {
            assert!(held > 32);
            assert_eq!(cap, 32);
        }
        _ => panic!("expected SuffixTooLong"),
    }

    // Finishing seals the lexer
    let mut normal_lexer = ResumableYamlLexer::new();
    normal_lexer.feed("name: test\n").unwrap();
    normal_lexer.finish().unwrap();
    assert!(normal_lexer.is_finished());

    // Feed after finish is refused
    let finish_err = normal_lexer.feed("extra: 1\n").unwrap_err();
    assert_eq!(finish_err, LexYamlError::AlreadyFinished);
    assert_eq!(finish_err.code(), "ALREADY_FINISHED");

    // Double finish is also refused
    assert_eq!(normal_lexer.finish().unwrap_err(), LexYamlError::AlreadyFinished);
}

#[test]
fn yaml_capability_row_is_versioned() {
    assert_eq!(YAML_CAPABILITY_V1.version, 1);
    assert!(YAML_CAPABILITY_V1.incremental);
    assert!(YAML_CAPABILITY_V1.basic_and_literal_strings);
    assert!(YAML_CAPABILITY_V1.colon_and_dash_cuts);
}
