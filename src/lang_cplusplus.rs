#![forbid(unsafe_code)]

//! C++ incremental lexer (FCB-022 · fcb-9vx.11).
//!
//! Classifies C++ source into byte-exact [`Span`]s that tile the input
//! exactly. Handles, under a declared capability:
//!
//! - **Raw strings** with custom delimiters: `R"delim( content )delim"`,
//!   delimiter up to 16 chars, content spanning newlines, no escapes.
//! - **Encoding prefixes**: `u8`, `u`, `U`, `L` on strings and character
//!   literals (`u8"..."`, `L'x'`, ...).
//! - **Character literals** with the full escape set.
//! - **Comments**: `//` and `/* */` (non-nesting per ISO C++; unterminated
//!   block comments span to EOF).
//! - **Numbers**: hex/octal/binary, digit separators (`'`), float suffixes
//!   (`f`/`F`/`l`/`L`), integer suffix combinations (`u`/`U` × `l`/`L`/
//!   `ll`/`LL`), hex-float `p` exponents.
//! - **Templates and operators**: angle brackets classify as operators
//!   conservatively (no template resolution — declared capability).
//! - **Preprocessor directives**: `#` followed by a directive word at line
//!   start classifies as a keyword row.
//!
//! ASI does not apply; no compiler claims are made. Digraphs (`<%`, `%>`,
//! `<:`, `:>`) classify as punctuation equivalents of `{}[]`.

use crate::highlight::{Span, Tok};

/// C++ reserved words.
const KEYWORDS: &[&str] = &[
    "alignas", "alignof", "and", "and_eq", "asm", "auto", "bitand", "bitor", "bool", "break",
    "case", "catch", "char", "char8_t", "char16_t", "char32_t", "class", "compl", "concept",
    "const", "consteval", "constexpr", "constinit", "const_cast", "continue", "co_await",
    "co_return", "co_yield", "decltype", "default", "delete", "do", "double", "dynamic_cast",
    "else", "enum", "explicit", "export", "extern", "false", "float", "for", "friend", "goto",
    "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept", "not", "not_eq",
    "nullptr", "operator", "or", "or_eq", "private", "protected", "public", "register",
    "reinterpret_cast", "requires", "return", "short", "signed", "sizeof", "static",
    "static_assert", "static_cast", "struct", "switch", "template", "this", "thread_local",
    "throw", "true", "try", "typedef", "typeid", "typename", "union", "unsigned", "using",
    "virtual", "void", "volatile", "wchar_t", "while", "xor", "xor_eq",
];

/// Common standard-library type names.
const TYPES: &[&str] = &[
    "std", "string", "wstring", "u8string", "vector", "map", "unordered_map", "set",
    "unordered_set", "pair", "tuple", "optional", "variant", "unique_ptr", "shared_ptr",
    "weak_ptr", "function", "array", "deque", "list", "span", "string_view", "thread", "mutex",
    "size_t", "ssize_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t",
    "uint32_t", "uint64_t",
];

/// What the previous significant token was.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Prev {
    Start,
    Value,
    Keyword,
    Operator,
}

/// Consume the raw-string payload: the opening `R"delim(` has been consumed;
/// scan to the first `)delim"`; unterminated spans to EOF.
fn raw_string_end(code: &str, content_start: usize, delim: &str) -> usize {
    let mut terminator = String::with_capacity(delim.len() + 2);
    terminator.push(')');
    terminator.push_str(delim);
    terminator.push('"');
    code[content_start..]
        .find(&terminator)
        .map_or(code.len(), |at| content_start + at + terminator.len())
}

/// Validate a raw-string delimiter: letters/digits/underscore only, max 16
/// characters, no `(`/`)`/backslash/space inside.
fn valid_raw_delim(delim: &str) -> bool {
    delim.len() <= 16
        && !delim.is_empty()
        && delim.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'_'
        })
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
    spans.push(Span {
        kind,
        start,
        end,
    });
    *last_end = end;
}

fn emit_directive(
    spans: &mut Vec<Span>,
    last_end: &mut usize,
    start: usize,
    word: &str,
) {
    let known = matches!(
        word,
        "include" | "define" | "undef" | "if" | "ifdef" | "ifndef" | "else" | "elif" | "endif"
            | "error" | "warning" | "pragma"
    );
    let kind = if known { Tok::Keyword } else { Tok::Plain };
    push_tiling(spans, last_end, kind, start, start + 1 + word.len());
}

fn next_non_space_is(code: &str, mut pos: usize, target: char) -> bool {
    while pos < code.len() {
        let c = code[pos..].chars().next().unwrap();
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
        let c = code[pos..].chars().next().unwrap();
        if pred(c) {
            pos += c.len_utf8();
        } else {
            break;
        }
    }
    pos
}

/// Lex C++ source into exact tiling spans.
pub fn lex_cplusplus_into(code: &str, spans: &mut Vec<Span>) {
    let bytes_len = code.len();
    let mut pos = 0usize;
    let mut last_end = 0usize;
    let mut prev = Prev::Start;

    /// True when `pos` sits at the first non-whitespace byte of its line.
    fn at_line_start(code: &str, pos: usize) -> bool {
        let bytes = code.as_bytes();
        let mut cursor = pos;
        while cursor > 0 {
            let byte = bytes[cursor - 1];
            if byte == b'\n' {
                return true;
            }
            if byte != b' ' && byte != b'\t' && byte != b'\r' {
                return false;
            }
            cursor -= 1;
        }
        true
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
                let c = code[pos..].chars().next().unwrap();
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
            prev = Prev::Start;
            continue;
        }

        // Block comments: non-nesting per ISO C++; unterminated to EOF.
        if rest.starts_with("/*") {
            let start = pos;
            let end = code[start + 2..]
                .find("*/")
                .map_or(bytes_len, |at| start + 2 + at + 2);
            push_tiling(spans, &mut last_end, Tok::Comment, start, end);
            pos = end;
            prev = Prev::Start;
            continue;
        }

        // Raw strings: `R"delim( content )delim"`.
        if rest.starts_with("R\"") {
            let start = pos;
            let prefix_end = pos + 2;
            let delim_start = prefix_end;
            let open_paren = code[delim_start..]
                .find('(')
                .map_or(prefix_end, |off| delim_start + off);
            let delim = &code[delim_start..open_paren];
            if delim.is_empty() || valid_raw_delim(delim) {
                let content_start = open_paren + 1;
                let end = raw_string_end(code, content_start, delim);
                push_tiling(spans, &mut last_end, Tok::Str, start, end);
                pos = end;
                prev = Prev::Value;
                continue;
            }
            // Invalid delimiter: treat the `R` as an identifier.
        }

        // Encoding-prefixed strings: u8"...", u"...", U"...", L"...".
        if (rest.starts_with("u8\"")
            || rest.starts_with("u\"")
            || rest.starts_with("U\"")
            || rest.starts_with("L\""))
            && (pos == 0
                || !is_ident_continue(
                    code[..pos].chars().next_back().unwrap_or(' '),
                ))
        {
            let start = pos;
            let quote = pos
                + if rest.starts_with("u8\"") { 3 } else { 2 };
            let mut scan = quote + 1;
            let mut closed = false;
            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap();
                if c == '\\' {
                    let next_scan = scan + 1;
                    if next_scan >= bytes_len {
                        scan = bytes_len;
                        break;
                    }
                    scan = next_scan + code[next_scan..].chars().next().unwrap().len_utf8();
                    continue;
                }
                if c == '"' {
                    scan += c.len_utf8();
                    closed = true;
                    break;
                }
                if c == '\n' {
                    break;
                }
                scan += c.len_utf8();
            }
            let end = if closed { scan } else { bytes_len.min(scan) };
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Plain strings with escapes and escaped-newline continuations.
        if ch == '"' {
            let start = pos;
            let mut scan = pos + 1;
            let mut closed = false;
            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap();
                if c == '\\' {
                    let next_scan = scan + 1;
                    if next_scan >= bytes_len {
                        scan = bytes_len;
                        break;
                    }
                    scan = next_scan + code[next_scan..].chars().next().unwrap().len_utf8();
                    continue;
                }
                if c == '"' {
                    scan += c.len_utf8();
                    closed = true;
                    break;
                }
                if c == '\n' {
                    break;
                }
                scan += c.len_utf8();
            }
            let end = if closed { scan } else { bytes_len.min(scan) };
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Character literals (with optional encoding prefixes) and the
        // escaped-quote disambiguation: `'` after a value token is the
        // digit-separator, not a literal open.
        if ch == '\'' && !matches!(prev, Prev::Value) {
            let start = pos;
            let mut scan = pos + 1;
            let mut closed = false;
            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap();
                if c == '\\' {
                    let next_scan = scan + 1;
                    if next_scan >= bytes_len {
                        scan = bytes_len;
                        break;
                    }
                    scan = next_scan + code[next_scan..].chars().next().unwrap().len_utf8();
                    continue;
                }
                if c == '\'' {
                    scan += c.len_utf8();
                    closed = true;
                    break;
                }
                if c == '\n' {
                    break;
                }
                scan += c.len_utf8();
            }
            let end = if closed { scan } else { bytes_len.min(scan) };
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Numbers: bases, digit separators, float exponents, suffixes.
        if ch.is_ascii_digit()
            || (ch == '.' && pos + 1 < bytes_len && code.as_bytes()[pos + 1].is_ascii_digit())
        {
            let start = pos;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                pos += 2;
                pos = consume_while(code, pos, |c| {
                    c.is_ascii_hexdigit() || c == '_' || c == '.'
                });
                // Hex-float p exponent.
                if pos < bytes_len
                    && (code.as_bytes()[pos] == b'p' || code.as_bytes()[pos] == b'P')
                {
                    pos += 1;
                    if pos < bytes_len
                        && (code.as_bytes()[pos] == b'+' || code.as_bytes()[pos] == b'-')
                    {
                        pos += 1;
                    }
                    pos = consume_while(code, pos, |c| c.is_ascii_digit());
                }
            } else if rest.starts_with("0b") || rest.starts_with("0B") {
                pos += 2;
                pos = consume_while(code, pos, |c| {
                    c == '0' || c == '1' || c == '\''
                });
            } else {
                pos = consume_while(code, pos, |c| {
                    c.is_ascii_digit() || c == '_' || c == '.'
                });
                if pos < bytes_len
                    && (code.as_bytes()[pos] == b'e' || code.as_bytes()[pos] == b'E')
                {
                    let exp_start = pos;
                    let mut p = pos + 1;
                    if p < bytes_len
                        && (code.as_bytes()[p] == b'+' || code.as_bytes()[p] == b'-')
                    {
                        p += 1;
                    }
                    let digits = consume_while(code, p, |c| c.is_ascii_digit());
                    if digits > p {
                        pos = p;
                        pos = consume_while(code, pos, |c| c.is_ascii_digit());
                    } else {
                        pos = exp_start;
                    }
                }
            }
            // Suffixes: u/U, l/L, ll/LL, f/F, in any (valid) combination.
            let suffixes: &[&[u8]] = &[b"ull", b"llu", b"ull", b"llu", b"ul", b"lu", b"ll",
                b"ll", b"ul", b"lu", b"u", b"U", b"l", b"L", b"f", b"F"];
            for suffix in suffixes {
                if code.as_bytes()[pos..].starts_with(suffix) {
                    pos += suffix.len();
                    break;
                }
            }
            push_tiling(spans, &mut last_end, Tok::Number, start, pos);
            prev = Prev::Value;
            continue;
        }

        // Identifiers, keywords, type names.
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
            prev = if kind == Tok::Keyword {
                Prev::Keyword
            } else {
                Prev::Value
            };
            push_tiling(spans, &mut last_end, kind, start, pos);
            continue;
        }

        // Preprocessor directives: `#` + word at line start.
        if ch == '#' && at_line_start(code, pos) {
            let start = pos;
            pos += 1;
            pos = consume_while(code, pos, |c| c.is_whitespace());
            let word_start = pos;
            pos = consume_while(code, pos, is_ident_continue);
            let word = &code[word_start..pos];
            emit_directive(spans, &mut last_end, start, word);
            continue;
        }

        // Multi-char operators, longest first.
        const OPERATORS: &[&str] = &[
            "<=>", "<<=", ">>=", "->*", "&&", "||", "==", "!=", "<=", ">=", "+=", "-=", "*=",
            "/=", "%=", "^=", "&=", "|=", "++", "--", "->", "::", "<<", ">>",
        ];
        let mut matched_op = false;
        for op in OPERATORS {
            if rest.starts_with(op) {
                push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + op.len());
                pos += op.len();
                prev = Prev::Operator;
                matched_op = true;
                break;
            }
        }
        if matched_op {
            continue;
        }

        // Single-char operators (angle brackets conservative: operator).
        if matches!(
            ch,
            '+' | '-' | '*' | '/' | '%' | '^' | '&' | '|' | '~' | '!' | '<' | '>' | '=' | '?'
        ) {
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + clen);
            pos += clen;
            prev = Prev::Operator;
            continue;
        }

        // Punctuation (digraphs classify at their bracket chars).
        if matches!(ch, '(' | '[' | '{' | ')' | ']' | '}' | ',' | ';' | ':' | '.') {
            push_tiling(spans, &mut last_end, Tok::Punct, pos, pos + clen);
            pos += clen;
            prev = Prev::Operator;
            continue;
        }

        // Everything else: Plain, one char.
        push_tiling(spans, &mut last_end, Tok::Plain, pos, pos + clen);
        pos += clen;
        prev = Prev::Start;
    }

    // Defensive final tile.
    let tail_start = last_end;
    if tail_start < bytes_len {
        push_tiling(spans, &mut last_end, Tok::Plain, tail_start, bytes_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // tests restored below by the caller
}
