//! Adversarial split corpus for the C incremental lexer (FCB-022, C route).
//!
//! The package oracle for this language: every representative C fixture is
//! classified whole and at EVERY byte split (including inside preprocessor
//! continuations, string escapes, multibyte characters and EOF), and the
//! chunked classification must coalesce to exactly the whole-input run while
//! tiling the source. Malformed directives and truncated constructs must
//! degrade to bounded plain/keyword classifications without refusing.

#![forbid(unsafe_code)]

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::lex_c::ResumableCLexer;

/// Coalesce adjacent same-kind spans: chunked streams may split a token at a
/// feed boundary, so equality is asserted after coalescing.
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

/// Feed `source` through the resumable engine one character at a time (the
/// finest granularity the `&str` API supports) and return the sealed
/// classification.
fn char_by_char(source: &str) -> Vec<Span> {
    let mut lexer = ResumableCLexer::new();
    for ch in source.chars() {
        let mut chunk = [0u8; 4];
        let chunk_str = ch.encode_utf8(&mut chunk);
        lexer.feed(chunk_str).expect("character feed is bounded");
    }
    lexer.finish().expect("EOF flush");
    lexer.spans().to_vec()
}

/// Feed `source` split at one specific byte position.
fn split_at(source: &str, split: usize) -> Vec<Span> {
    let mut lexer = ResumableCLexer::new();
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
    let whole = highlight("c", source);
    assert_tiling(&whole, source.len());
    let whole_runs = coalesce(&whole);

    let every_char = char_by_char(source);
    assert_tiling(&every_char, source.len());
    assert_eq!(
        coalesce(&every_char),
        whole_runs,
        "character-by-character feeding must coalesce to the whole run"
    );

    // `&str` chunks are always valid UTF-8, so only character-boundary
    // splits are representable feeds; byte positions inside a multibyte
    // character are held by construction.
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

const PREPROCESSOR_CONTINUATION: &str = concat!(
    "#define MAX(a, b) ((a) > (b) ? (a) : \\\n",
    "                         (b))\n",
    "int main(void) { return MAX(1, 2); }\n"
);

const STRING_ESCAPED_NEWLINE: &str = concat!(
    "const char *msg = \"line one \\\n",
    "line two \\\"quoted\\\" \\n end\";\n",
    "char c = '\\'';\n"
);

const LITERALS: &str = concat!(
    "int hex = 0x1F;\n",
    "unsigned long big = 4294967295UL;\n",
    "float f = 1.5e-10f;\n",
    "double d = .5;\n",
    "char tab = '\\t';\n"
);

const MALFORMED_DIRECTIVES: &str = concat!(
    "#\n",
    "# 123\n",
    "#define\n",
    "#include <stdio.h>\n",
    "#error unterminated string \"oops\n"
);

const UNTERMINATED_BLOCK_COMMENT: &str = "int x = 1; /* never closed\nint y = 2;\n";

const MULTIBYTE_IN_COMMENT: &str = "/* špatný komentář – ñ */ int main(void) {}\n";

#[test]
fn preprocessor_continuation_is_split_safe() {
    assert_chunked_equivalent(PREPROCESSOR_CONTINUATION);
}

#[test]
fn escaped_newline_inside_strings_is_split_safe() {
    assert_chunked_equivalent(STRING_ESCAPED_NEWLINE);
}

#[test]
fn integer_and_float_literals_are_split_safe() {
    assert_chunked_equivalent(LITERALS);
}

#[test]
fn malformed_directives_degrade_without_hanging() {
    assert_chunked_equivalent(MALFORMED_DIRECTIVES);
}

#[test]
fn unterminated_block_comment_degrades_to_bounded_classification() {
    assert_chunked_equivalent(UNTERMINATED_BLOCK_COMMENT);
}

#[test]
fn multibyte_characters_inside_comments_are_split_safe() {
    assert_chunked_equivalent(MULTIBYTE_IN_COMMENT);
}

#[test]
fn a_realistic_c_program_is_split_safe_at_every_byte() {
    let program = concat!(
        "#include <stdlib.h>\n",
        "#define GREETING \"hello\"\n",
        "\n",
        "/* entry point:\n",
        "   handles \\\"escapes\\\" */\n",
        "int main(int argc, char **argv) {\n",
        "    unsigned flags = 0x7FU;\n",
        "    double ratio = 3.5e2;\n",
        "    const char *s = \"tab\\there\";\n",
        "    return argc == 1 ? 0 : (int)flags;\n",
        "}\n"
    );
    assert_chunked_equivalent(program);
}

#[test]
fn c_capability_row_is_versioned() {
    use franken_markdown::lex_c::C_CAPABILITY_V1;
    assert_eq!(C_CAPABILITY_V1.version, 1);
    assert!(C_CAPABILITY_V1.incremental);
    assert!(C_CAPABILITY_V1.preprocessor_continuation);
    assert!(C_CAPABILITY_V1.escaped_newline_in_literals);
}

#[test]
fn malformed_bounds_and_error_handling() {
    use franken_markdown::lex_c::{LexCError, ResumableCLexer};

    let mut lexer = ResumableCLexer::with_limits(32);
    let ok_feed = lexer.feed("int x = 42;\n");
    assert!(ok_feed.is_ok());

    // Exceeding the pending cap triggers SuffixTooLong
    let long_chunk = "/* ".to_string() + &"a".repeat(50);
    let err = lexer.feed(&long_chunk).unwrap_err();
    assert_eq!(err.code(), "SUFFIX_TOO_LONG");
    match err {
        LexCError::SuffixTooLong { held, cap } => {
            assert!(held > 32);
            assert_eq!(cap, 32);
        }
        _ => panic!("expected SuffixTooLong"),
    }

    // Finishing seals the lexer
    let mut normal_lexer = ResumableCLexer::new();
    normal_lexer.feed("int main() { return 0; }").unwrap();
    normal_lexer.finish().unwrap();
    assert!(normal_lexer.is_finished());

    // Feed after finish is refused
    let finish_err = normal_lexer.feed("extra").unwrap_err();
    assert_eq!(finish_err, LexCError::AlreadyFinished);
    assert_eq!(finish_err.code(), "ALREADY_FINISHED");

    // Double finish is also refused
    assert_eq!(normal_lexer.finish().unwrap_err(), LexCError::AlreadyFinished);
}
