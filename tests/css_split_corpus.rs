//! Adversarial split corpus for the CSS incremental lexer (FCB-022, CSS
//! route / fcb-9vx.22). Whole and chunked runs coalesce identically at
//! every character boundary with exact source tiling.

#![forbid(unsafe_code)]

use franken_markdown::highlight::{highlight, Span, Tok};
use franken_markdown::lex_css::ResumableCssLexer;

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
    let mut lexer = ResumableCssLexer::new();
    for ch in source.chars() {
        let mut buf = [0u8; 4];
        lexer.feed(ch.encode_utf8(&mut buf)).expect("char feed");
    }
    lexer.finish().expect("EOF flush");
    lexer.spans().to_vec()
}

fn split_at(source: &str, split: usize) -> Vec<Span> {
    let mut lexer = ResumableCssLexer::new();
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
    let whole = highlight("css", source);
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

const SELECTORS_AND_DECLS: &str = concat!(
    ".container {\n",
    "  display: flex;\n",
    "  color: #333;\n",
    "  margin: 0 auto;\n",
    "}\n",
    "#header { padding: 1rem; }\n"
);

const AT_RULES_AND_NESTING: &str = concat!(
    "@media (max-width: 768px) {\n",
    "  .sidebar { display: none; }\n",
    "  @supports (display: grid) {\n",
    "    .grid { display: grid; }\n",
    "  }\n",
    "}\n",
    "@import url(\"theme.css\");\n"
);

const URLS_AND_STRINGS: &str = concat!(
    ".hero {\n",
    "  background: url(\"bg.png\") no-repeat center;\n",
    "  font-family: \"Inter\", 'Roboto', sans-serif;\n",
    "  content: \"\\201C\";\n",
    "}\n"
);

#[test]
fn selectors_and_declarations_are_split_safe() {
    assert_chunked_equivalent(SELECTORS_AND_DECLS);
}

#[test]
fn at_rules_and_nesting_are_split_safe() {
    assert_chunked_equivalent(AT_RULES_AND_NESTING);
}

#[test]
fn urls_and_strings_are_split_safe() {
    assert_chunked_equivalent(URLS_AND_STRINGS);
}

#[test]
fn css_capability_row_is_versioned() {
    use franken_markdown::lex_css::CSS_CAPABILITY_V1;
    assert_eq!(CSS_CAPABILITY_V1.version, 1);
    assert!(CSS_CAPABILITY_V1.incremental);
    assert!(CSS_CAPABILITY_V1.comments_strings_nested);
}
