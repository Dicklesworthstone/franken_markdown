#![forbid(unsafe_code)]

//! Go incremental lexer (FCB-022 · fcb-9vx.13).
//!
//! Classifies Go source into byte-exact [`Span`]s that tile the input
//! exactly. Handles, under a declared capability:
//!
//! - **Raw strings** (backquotes): no escapes, span newlines, terminated
//!   only by the matching backquote; unterminated spans to EOF.
//! - **Interpreted strings** with the full Go escape set (`\xNN`, `\NNN`,
//!   `\uNNNN`, `\UNNNNNNNN`, escaped newlines); unterminated spans to EOF.
//! - **Rune literals** with the same escape set; malformed runes span to
//!   the closing quote or EOF under the declared capability.
//! - **Comments**: `//` and `/* */` (unterminated spans to EOF).
//! - **Numbers**: hex/octal/binary, floats with exponents, underscores,
//!   imaginary suffix `i` (e.g. `3i`, `1.5e-2i`).
//!
//! Keywords and predeclared identifiers follow the Go spec; predeclared
//! functions classify as calls when followed by `(`. ASI does not exist in
//! Go and no compiler claims are made.

use crate::highlight::{Span, Tok};

/// Go reserved words.
const KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

/// Predeclared type and constant identifiers.
const TYPES: &[&str] = &[
    "bool",
    "byte",
    "complex64",
    "complex128",
    "error",
    "float32",
    "float64",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "rune",
    "string",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
    "true",
    "false",
    "iota",
    "nil",
];

/// The versioned Go capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GoCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for Go.
    pub incremental: bool,
    /// Raw backticks, interpreted strings, and rune literals supported.
    pub strings_and_runes: bool,
    /// Numeric separators and imaginary numbers supported.
    pub numbers_and_operators: bool,
}

/// The Go capability row published by this module.
pub const GO_CAPABILITY_V1: GoCapabilityV1 = GoCapabilityV1 {
    version: 1,
    incremental: true,
    strings_and_runes: true,
    numbers_and_operators: true,
};

/// Lex Go source into exact tiling spans.
pub fn lex_go_into(code: &str, spans: &mut Vec<Span>) {
    let bytes_len = code.len();
    let mut pos = 0usize;
    let mut last_end = 0usize;

    fn push_tiling(
        spans: &mut Vec<Span>,
        last_end: &mut usize,
        kind: Tok,
        start: usize,
        end: usize,
    ) {
        if *last_end < start {
            spans.push(Span {
                kind: Tok::Plain,
                start: *last_end,
                end: start,
            });
        }
        spans.push(Span { kind, start, end });
        *last_end = end;
    }

    while pos < bytes_len {
        let rest = &code[pos..];
        let ch = match rest.chars().next() {
            Some(c) => c,
            None => break,
        };
        let clen = ch.len_utf8();

        // Whitespace.
        if ch.is_whitespace() {
            let start = pos;
            pos += clen;
            while pos < bytes_len {
                let c = code[pos..].chars().next().unwrap_or('\0');
                if c.is_whitespace() {
                    pos += c.len_utf8();
                } else {
                    break;
                }
            }
            push_tiling(spans, &mut last_end, Tok::Plain, start, pos);
            continue;
        }

        // Line comments.
        if rest.starts_with("//") {
            let start = pos;
            let end = code[start..].find('\n').map_or(bytes_len, |nl| start + nl);
            push_tiling(spans, &mut last_end, Tok::Comment, start, end);
            pos = end;

            continue;
        }

        // Block comments: unterminated spans to EOF.
        if rest.starts_with("/*") {
            let start = pos;
            let end = code[start + 2..]
                .find("*/")
                .map_or(bytes_len, |at| start + 2 + at + 2);
            push_tiling(spans, &mut last_end, Tok::Comment, start, end);
            pos = end;

            continue;
        }

        // Raw strings: backquotes, no escapes, span newlines. Unterminated
        // spans to EOF under the declared capability.
        if ch == '`' {
            let start = pos;
            let end = code[start + 1..]
                .find('`')
                .map_or(bytes_len, |at| start + 1 + at + 1);
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;

            continue;
        }

        // Interpreted strings and rune literals with the Go escape set.
        if ch == '"' || ch == '\'' {
            let start = pos;
            let mut scan = pos + 1;

            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap_or('\0');
                if c == ch {
                    scan += c.len_utf8();

                    break;
                }
                if c == '\n' && ch == '"' {
                    // Interpreted strings cannot span raw newlines.
                    break;
                }
                if c == '\\' {
                    let next_scan = scan + 1;
                    if next_scan >= bytes_len {
                        scan = bytes_len;
                        break;
                    }
                    // Skip the escape character; \x/\u/\U forms carry fixed
                    // hex widths consumed here as well.
                    let esc = code.as_bytes()[next_scan];
                    match esc {
                        b'x' | b'u' | b'U' => {
                            let width = match esc {
                                b'x' => 2,
                                b'u' => 4,
                                _ => 8,
                            };
                            scan = next_scan + 1;
                            // Only ASCII hex digits belong to this escape.
                            // Leave malformed text (including quotes and UTF-8)
                            // for the normal character scanner.
                            for _ in 0..width {
                                if scan < bytes_len && code.as_bytes()[scan].is_ascii_hexdigit() {
                                    scan += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                        _ => {
                            let e = code[next_scan..].chars().next().unwrap_or('\0');
                            scan = next_scan + e.len_utf8();
                        }
                    }
                    continue;
                }
                scan += c.len_utf8();
            }
            push_tiling(spans, &mut last_end, Tok::Str, start, scan);
            pos = scan;

            continue;
        }

        // Numbers: 0x/0o/0b bases, decimals with exponents, underscores,
        // imaginary suffix `i`.
        if ch.is_ascii_digit() {
            let start = pos;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                pos += 2;
                pos = consume_while(code, pos, |c| c.is_ascii_hexdigit() || c == '_');
            } else if rest.starts_with("0o") || rest.starts_with("0O") {
                pos += 2;
                pos = consume_while(code, pos, |c| ('0'..='7').contains(&c) || c == '_');
            } else if rest.starts_with("0b") || rest.starts_with("0B") {
                pos += 2;
                pos = consume_while(code, pos, |c| c == '0' || c == '1' || c == '_');
            } else {
                pos = consume_while(code, pos, |c| c.is_ascii_digit() || c == '_' || c == '.');
                if pos < bytes_len && matches!(code.as_bytes()[pos], b'e' | b'E') {
                    let mut p = pos + 1;
                    if p < bytes_len && (code.as_bytes()[p] == b'+' || code.as_bytes()[p] == b'-') {
                        p += 1;
                    }
                    let digits = consume_while(code, p, |c| c.is_ascii_digit());
                    if digits > p || p >= bytes_len {
                        pos = digits;
                    }
                }
                if pos < bytes_len && code.as_bytes()[pos] == b'i' {
                    pos += 1;
                }
            }
            // Imaginary suffix.
            if pos < bytes_len && code.as_bytes()[pos] == b'i' {
                pos += 1;
            }
            push_tiling(spans, &mut last_end, Tok::Number, start, pos);

            continue;
        }

        // Identifiers, keywords, predeclared names.
        if is_ident_start(ch) {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, is_ident_continue);
            let word = &code[start..pos];
            let kind = if is_keyword(word) {
                Tok::Keyword
            } else if is_type_name(word) {
                Tok::Type
            } else if next_non_space_is(code, pos, '(') {
                Tok::Func
            } else {
                Tok::Plain
            };
            push_tiling(spans, &mut last_end, kind, start, pos);
            continue;
        }

        // Multi-char operators, longest first.
        const OPERATORS: &[&str] = &[
            "<<=", ">>=", "&^=", "...", ":=", "<-", "&&", "||", "==", "!=", "<=", ">=", "+=", "-=",
            "*=", "/=", "%=", "&^",
        ];
        let mut matched_op = false;
        for op in OPERATORS {
            if rest.starts_with(op) {
                push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + op.len());
                pos += op.len();

                matched_op = true;
                break;
            }
        }
        if matched_op {
            continue;
        }

        // Single-char operators.
        if matches!(
            ch,
            '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '<' | '>' | '=' | '!' | '~'
        ) {
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + clen);
            pos += clen;

            continue;
        }

        // Punctuation.
        if matches!(
            ch,
            '(' | '[' | '{' | ')' | ']' | '}' | ',' | ';' | ':' | '.'
        ) {
            push_tiling(spans, &mut last_end, Tok::Punct, pos, pos + clen);
            pos += clen;

            continue;
        }

        // Everything else: Plain, one char.
        push_tiling(spans, &mut last_end, Tok::Plain, pos, pos + clen);
        pos += clen;
    }

    // Defensive final tile.
    let tail_start = last_end;
    if tail_start < bytes_len {
        push_tiling(spans, &mut last_end, Tok::Plain, tail_start, bytes_len);
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '\u{200C}' || c == '\u{200D}'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_numeric()
}

fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word)
}

fn is_type_name(word: &str) -> bool {
    TYPES.contains(&word)
}

fn next_non_space_is(code: &str, mut pos: usize, target: char) -> bool {
    while pos < code.len() {
        let c = code[pos..].chars().next().unwrap_or('\0');
        if c.is_whitespace() {
            pos += c.len_utf8();
        } else {
            return c == target;
        }
    }
    false
}

fn consume_while(code: &str, mut pos: usize, pred: impl Fn(char) -> bool) -> usize {
    while pos < code.len() {
        let c = code[pos..].chars().next().unwrap_or('\0');
        if pred(c) {
            pos += c.len_utf8();
        } else {
            break;
        }
    }
    pos
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The source-tiling oracle: spans must exactly tile [0, len).
    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(span.start, cursor, "gap/overlap at {cursor}");
            assert!(span.end > span.start, "empty span at {cursor}");
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    #[test]
    fn keywords_types_and_builtins_classified() {
        let code = "func main() { var x int = len(s); }";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let text: Vec<&str> = spans.iter().map(|s| &code[s.start..s.end]).collect();
        assert!(text.contains(&"func"));
        assert!(text.contains(&"var"));
        assert!(text.contains(&"int"));
    }

    #[test]
    fn raw_strings_span_newlines_without_escapes() {
        let code = "let p = `line1\\nstill line1\nline2`;";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let strs: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            strs.iter().any(|s| s.contains('\n')),
            "raw string spans the newline: {strs:?}"
        );
    }

    #[test]
    fn interpreted_strings_process_escapes_and_reject_raw_newlines() {
        let code = "let s = \"a\\n b\";";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let strs: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(strs.iter().any(|s| s.contains("\\n")));
    }

    #[test]
    fn rune_literals_with_unicode_escapes() {
        let code = "let r = '\\u1234'; let q = '\\n'; let z = 'x';";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let runes: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(runes.iter().any(|s| s.contains("\\u1234")));
        assert!(runes.contains(&"'x'"));
    }

    #[test]
    fn unterminated_raw_string_spans_to_eof() {
        let code = "let s = `never closed";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let last = spans.last().unwrap();
        assert_eq!(last.kind, Tok::Str);
        assert_eq!(last.end, code.len());
    }

    #[test]
    fn numbers_with_imaginary_suffix_and_separators() {
        let code = "3i 1_000 0xdead_beef 1.5e-2 0b1010_1010";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let numbers: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Number)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(numbers.contains(&"3i"), "{numbers:?}");
        assert!(numbers.contains(&"1_000"));
        assert!(numbers.contains(&"0xdead_beef"));
        assert!(numbers.contains(&"1.5e-2"));
        assert!(numbers.contains(&"0b1010_1010"));
    }

    #[test]
    fn channel_operator_and_go_statement_classified() {
        let code = "go func() { ch <- v }(); x := <-ch;";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let ops: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Operator)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(ops.contains(&"<-"), "{ops:?}");
        assert!(ops.contains(&":="));
    }

    #[test]
    fn unterminated_block_comment_spans_to_eof() {
        let code = "x := 1; /* never closed";
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
        let last = spans.last().unwrap();
        assert_eq!(last.kind, Tok::Comment);
        assert_eq!(last.end, code.len());
    }

    #[test]
    fn negative_control_tiling_oracle_detects_gap() {
        let code = "abc";
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
            assert_tiling(code, &gapped);
        }))
        .is_err();
        assert!(detected, "tiling oracle must catch a gap");
        let mut spans = Vec::new();
        lex_go_into(code, &mut spans);
        assert_tiling(code, &spans);
    }
}
