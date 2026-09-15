//! Adversarial split corpus for the TOML incremental lexer (FCB-022, TOML
//! route / fcb-9vx.18). The package oracle: whole and chunked runs coalesce
//! identically, at every byte split, with exact source tiling.

#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Span, Tok, highlight};
use franken_markdown::lex_toml::ResumableTomlLexer;

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
    let mut lexer = ResumableTomlLexer::new();
    for ch in source.chars() {
        let mut buf = [0u8; 4];
        lexer.feed(ch.encode_utf8(&mut buf)).expect("char feed");
    }
    lexer.finish().expect("EOF flush");
    lexer.spans().to_vec()
}

fn split_at(source: &str, split: usize) -> Vec<Span> {
    let mut lexer = ResumableTomlLexer::new();
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
    let whole = highlight("toml", source);
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

const TYPED_DOCUMENT: &str = concat!(
    "[package]\n",
    "name = \"my-app\"\n",
    "version = \"1.0.0\"\n",
    "keywords = [\"cli\", \"tool\"]\n",
    "\n",
    "[dependencies]\n",
    "serde = { version = \"1\", features = [\"derive\"] }\n",
    "\n",
    "[[bin]]\n",
    "path = \"src/main.rs\"\n"
);

const LITERAL_AND_MULTILINE: &str = concat!(
    "literal = 'no \\n escapes'\n",
    "multi = \"\"\"\n",
    "line 1\n",
    "line 2\n",
    "\"\"\"\n",
    "literal_ml = '''\n",
    "raw text\n",
    "'''\n"
);

const DOTTED_KEYS_AND_TABLES: &str = concat!(
    "a.b.c = 1\n",
    "deep.nested.key = \"value\"\n",
    "[table.sub]\n",
    "flag = true\n",
    "off = false\n"
);

const NUMBERS_AND_DATES: &str = concat!(
    "int = 42\n",
    "neg = -17\n",
    "hex = 0xdead\n",
    "oct = 0o755\n",
    "bin = 0b1101\n",
    "float = 3.14\n",
    "exp = 5e22\n",
    "date = 1979-05-27\n",
    "datetime = 1979-05-27T07:32:00Z\n"
);

#[test]
fn typed_document_is_split_safe() {
    assert_chunked_equivalent(TYPED_DOCUMENT);
}

#[test]
fn literal_and_multiline_strings_are_split_safe() {
    assert_chunked_equivalent(LITERAL_AND_MULTILINE);
}

#[test]
fn dotted_keys_and_tables_are_split_safe() {
    assert_chunked_equivalent(DOTTED_KEYS_AND_TABLES);
}

#[test]
fn numbers_and_dates_are_split_safe() {
    assert_chunked_equivalent(NUMBERS_AND_DATES);
}

#[test]
fn toml_capability_row_is_versioned() {
    use franken_markdown::lex_toml::TOML_CAPABILITY_V1;
    assert_eq!(TOML_CAPABILITY_V1.version, 1);
    const {
        const {
            assert!(TOML_CAPABILITY_V1.incremental);
        };
    };
    const {
        const {
            assert!(TOML_CAPABILITY_V1.basic_and_literal_strings);
        };
    };
    const {
        const {
            assert!(TOML_CAPABILITY_V1.structured_keys);
        };
    };
}

#[test]
fn malformed_bounds_and_error_handling() {
    use franken_markdown::lex_toml::LexTomlError;

    let mut lexer = ResumableTomlLexer::with_limits(32);
    let ok_feed = lexer.feed("name = \"test\"\n");
    assert!(ok_feed.is_ok());

    // Exceeding the pending cap triggers SuffixTooLong
    let long_chunk = "# ".to_string() + &"a".repeat(50);
    let err = lexer.feed(&long_chunk).unwrap_err();
    assert_eq!(err.code(), "SUFFIX_TOO_LONG");
    match err {
        LexTomlError::SuffixTooLong { held, cap } => {
            assert!(held > 32);
            assert_eq!(cap, 32);
        }
        _ => panic!("expected SuffixTooLong"),
    }

    // Finishing seals the lexer
    let mut normal_lexer = ResumableTomlLexer::new();
    normal_lexer.feed("key = 1\n").unwrap();
    normal_lexer.finish().unwrap();
    assert!(normal_lexer.is_finished());

    // Feed after finish is refused
    let finish_err = normal_lexer.feed("extra = 2\n").unwrap_err();
    assert_eq!(finish_err, LexTomlError::AlreadyFinished);
    assert_eq!(finish_err.code(), "ALREADY_FINISHED");

    // Double finish is also refused
    assert_eq!(
        normal_lexer.finish().unwrap_err(),
        LexTomlError::AlreadyFinished
    );
}
