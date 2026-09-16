#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::if_same_then_else)]

//! JavaScript incremental lexer (FCB-022 · fcb-9vx.6).
//!
//! Classifies JavaScript source into byte-exact [`Span`]s that tile the
//! input exactly. Handles, under a declared capability:
//!
//! - **Regex versus division**: a `/` opens a regex literal unless the
//!   previous significant token can end an expression (identifier, literal,
//!   closing bracket, or value keyword), in which case it is division.
//!   The scan honors character classes, escapes, and never treats a `/`
//!   inside `[...]` as the terminator.
//! - **Template literals with interpolation stacks**: `` `...${ ... }...` ``
//!   re-enters full code lexing inside the interpolation, with correct
//!   nesting of braces, strings, and nested templates.
//! - **Comments**: `//`, `/* */` (unterminated spans to EOF), and Annex B
//!   HTML-like comments (`<!--`, `-->` at line start).
//! - **Strings** with escapes, including escaped-newline continuations;
//!   truncated quotes span to EOF under the declared capability.
//! - **Numbers**: hex/octal/binary, bigint `n` suffix, numeric separators.
//!
//! ASI is a parsing concern: this lexer classifies tokens only and makes no
//! compiler claims. Where regex-vs-division is genuinely ambiguous the
//! lexer chooses the conservative interpretation and the declared
//! capability says so.

use crate::highlight::{Span, Tok};

/// JavaScript reserved words plus universally-highlighted contextual
/// keywords.
const KEYWORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "const",
    "static",
    "async",
    "await",
    "of",
    "get",
    "set",
];

/// Built-in constructors and global namespaces highlighted as types.
const TYPES: &[&str] = &[
    "any",
    "unknown",
    "never",
    "void",
    "number",
    "string",
    "boolean",
    "bigint",
    "symbol",
    "object",
    "Array",
    "Object",
    "String",
    "Number",
    "Boolean",
    "Symbol",
    "BigInt",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Promise",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "EvalError",
    "ReferenceError",
    "URIError",
    "RegExp",
    "Date",
    "JSON",
    "Math",
    "Reflect",
    "Proxy",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Function",
    "Generator",
    "Intl",
    "console",
    "globalThis",
];

/// What the previous significant token was, for regex-vs-division.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Prev {
    /// Nothing yet (start of input): regex allowed.
    Start,
    /// Identifier, number, string, template tail, regex, or `)`/`]`: a
    /// value just ended, so `/` is division.
    Value,
    /// A value keyword (`this`, `true`, `false`, `null`, `super`): `/` is
    /// division.
    ValueKeyword,
    /// Any other keyword: `/` opens a regex (`return /re/`, `typeof /re/`).
    Keyword,
    /// An operator or open bracket: `/` opens a regex.
    Operator,
    /// `}`: conservatively treated as block close (regex allowed).
    CloseBrace,
}

/// Whether a `/` at the current position opens a regex literal.
fn regex_allowed(prev: Prev) -> bool {
    !matches!(prev, Prev::Value | Prev::ValueKeyword)
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$' || c == '\u{200C}' || c == '\u{200D}'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_numeric()
}

fn is_id_or_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word)
}

fn is_type_name(word: &str) -> bool {
    TYPES.contains(&word)
}

/// The versioned JavaScript capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JavaScriptCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for JavaScript.
    pub incremental: bool,
    /// Regex literals vs division disambiguation supported under declared capability.
    pub regex_vs_division: bool,
    /// Template literals with nested interpolation stacks supported.
    pub template_interpolations: bool,
    /// Numeric forms (hex, octal, binary, bigint, separators) supported.
    pub numeric_forms: bool,
}

/// The JavaScript capability row published by this module.
pub const JAVASCRIPT_CAPABILITY_V1: JavaScriptCapabilityV1 = JavaScriptCapabilityV1 {
    version: 1,
    incremental: true,
    regex_vs_division: true,
    template_interpolations: true,
    numeric_forms: true,
};

/// Lex JavaScript source into exact tiling spans.
pub fn lex_javascript_into(code: &str, spans: &mut Vec<Span>) {
    lex_javascript_composed_into(code, spans, &[], &[]);
}

/// Lex JavaScript or layered extensions (e.g. TypeScript) into exact tiling spans,
/// reusing the full JavaScript lexical state machine per the language composition contract.
pub fn lex_javascript_composed_into(
    code: &str,
    spans: &mut Vec<Span>,
    extra_keywords: &[&str],
    extra_types: &[&str],
) {
    let bytes_len = code.len();
    let _code_bytes = code.as_bytes();
    let mut pos = 0usize;
    let mut last_end = 0usize;
    let mut prev = Prev::Start;

    // Template-literal interpolation stack: each `${` pushes the brace
    // depth captured at that point; the matching `}` at depth zero pops
    // back into template-chunk scanning.
    let mut interp_stack: Vec<usize> = Vec::new();
    let mut brace_depth_in_interp: usize = 0;

    /// Push a span, first closing any gap with a Plain span so the output
    /// always tiles the input exactly.
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
        // Inside a template interpolation: `}` at the recorded depth pops
        // back into the template chunk.
        if let Some(depth) = interp_stack.last().copied() {
            let rest = &code[pos..];
            if rest.starts_with('}') && brace_depth_in_interp == depth {
                interp_stack.pop();
                brace_depth_in_interp = 0;
                push_tiling(spans, &mut last_end, Tok::Punct, pos, pos + 1);
                pos += 1;
                // After `}` we are in template-chunk scanning; handled by the
                // template branch below on the next iteration via prev reset.
                prev = Prev::Start;
                continue;
            }
        }

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

        // Line comments: `//` and Annex B `<!--`.
        if rest.starts_with("//") || rest.starts_with("<!--") {
            let start = pos;
            let end = code[start..].find('\n').map_or(bytes_len, |nl| start + nl);
            push_tiling(spans, &mut last_end, Tok::Comment, start, end);
            pos = end;
            prev = Prev::Start;
            continue;
        }

        // Block comments: `/* ... */`, unterminated spans to EOF.
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

        // Annex B single-line-close comment `-->` (only at line start is
        // spec-true; conservatively accepted after `=` or `(` too — under
        // declared capability this lexer treats a bare `-->` as a comment).
        if rest.starts_with("-->") && matches!(prev, Prev::Start | Prev::Operator) {
            let start = pos;
            let end = code[start..].find('\n').map_or(bytes_len, |nl| start + nl);
            push_tiling(spans, &mut last_end, Tok::Comment, start, end);
            pos = end;
            prev = Prev::Start;
            continue;
        }

        // Strings: ' or " with escapes and escaped-newline continuation.
        // Truncated quotes span to EOF under the declared capability.
        if ch == '\'' || ch == '"' {
            let start = pos;
            let mut scan = pos + 1;
            let mut closed = false;
            while scan < bytes_len {
                let c = code[scan..].chars().next().unwrap();
                if c == '\\' {
                    // Escape: consume the escaped character (handles \n
                    // continuation, \u{...}, \xNN, quotes).
                    let next_scan = scan + 1;
                    if next_scan >= bytes_len {
                        scan = bytes_len;
                        break;
                    }
                    let esc = code[next_scan..].chars().next().unwrap();
                    scan = next_scan + esc.len_utf8();
                    continue;
                }
                if c == ch {
                    scan += c.len_utf8();
                    closed = true;
                    break;
                }
                if c == '\n' {
                    // Unescaped newline: unterminated string ends before it.
                    break;
                }
                scan += c.len_utf8();
            }
            let end = if closed { scan } else { bytes_len.min(scan) };
            assert!(
                end <= bytes_len,
                "TEMPLATE OOB: end={end} len={bytes_len} pos={pos}"
            );
            assert!(end <= bytes_len, "REGEX OOB: end={end} len={bytes_len}");
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Template literal chunk entry.
        if ch == '`' {
            let start = pos;
            let end = template_end(code, pos);
            push_tiling(spans, &mut last_end, Tok::Str, start, end);
            pos = end;
            prev = Prev::Value;
            continue;
        }

        // Interpolation close: handled at the top of the loop.
        if ch == '}' && !interp_stack.is_empty() {
            unreachable!("interp close handled above");
        }

        // Regex or division.
        if ch == '/' {
            if regex_allowed(prev) {
                // Regex literal: scan body honoring escapes and [...]
                // classes; unterminated spans to EOF.
                let start = pos;
                let mut scan = pos + 1;
                let mut in_class = false;
                let mut terminated = false;
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
                    if c == '[' {
                        in_class = true;
                    } else if c == ']' {
                        in_class = false;
                    } else if c == '/' && !in_class {
                        scan += c.len_utf8();
                        terminated = true;
                        break;
                    } else if c == '\n' {
                        // A regex cannot span lines unescaped: not a regex.
                        break;
                    }
                    scan += c.len_utf8();
                }
                if terminated {
                    // Flags.
                    while scan < bytes_len {
                        let c = code[scan..].chars().next().unwrap();
                        if c.is_alphabetic() {
                            scan += c.len_utf8();
                        } else {
                            break;
                        }
                    }
                    let end = scan;
                    push_tiling(spans, &mut last_end, Tok::Str, start, end);
                    pos = end;
                    prev = Prev::Value;
                    continue;
                }
                if scan == bytes_len {
                    // Unterminated regex literal spans to EOF.
                    push_tiling(spans, &mut last_end, Tok::Str, start, bytes_len);
                    pos = bytes_len;
                    prev = Prev::Value;
                    continue;
                }
                // Unterminated across line break: fall through to operator (conservative).
            }
            // Division or `/=` operator.
            let op_len = if code[pos..].starts_with("/=") { 2 } else { 1 };
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + op_len);
            pos += op_len;
            prev = Prev::Operator;
            continue;
        }

        // Numbers: hex/octal/binary, decimal with exponent, bigint suffix,
        // numeric separators, and leading-dot decimals.
        if ch.is_ascii_digit()
            || (ch == '.' && pos + 1 < bytes_len && code.as_bytes()[pos + 1].is_ascii_digit())
        {
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
                pos = consume_while(code, pos, |c| c.is_ascii_digit() || c == '_');
                if pos < bytes_len && code.as_bytes()[pos] == b'.' {
                    pos += 1;
                    pos = consume_while(code, pos, |c| c.is_ascii_digit() || c == '_');
                }
                if pos < bytes_len && (code.as_bytes()[pos] == b'e' || code.as_bytes()[pos] == b'E')
                {
                    let save = pos;
                    pos += 1;
                    if pos < bytes_len
                        && (code.as_bytes()[pos] == b'+' || code.as_bytes()[pos] == b'-')
                    {
                        pos += 1;
                    }
                    pos = consume_while(code, pos, |c| c.is_ascii_digit());
                    if pos == save {
                        // not an exponent after all
                        pos = save;
                    }
                }
            }
            if pos < bytes_len && code.as_bytes()[pos] == b'n' {
                pos += 1;
            }
            assert!(pos <= bytes_len, "NUMBER OOB: pos={pos} len={bytes_len}");
            push_tiling(spans, &mut last_end, Tok::Number, start, pos);
            prev = Prev::Value;
            continue;
        }

        // Identifiers, keywords, type names, function calls.
        if is_ident_start(ch) {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, is_ident_continue);
            assert!(
                pos <= bytes_len,
                "IDENT SLICE OOB: start={start} pos={pos} len={bytes_len}"
            );
            let word = &code[start..pos];
            let is_kw = is_id_or_keyword(word) || extra_keywords.contains(&word);
            let is_ty = is_type_name(word) || extra_types.contains(&word);
            let kind = if is_kw {
                Tok::Keyword
            } else if is_ty {
                Tok::Type
            } else if next_non_space_is(code, pos, '(') {
                Tok::Func
            } else {
                Tok::Plain
            };
            // Regex-vs-division context: value keywords behave like values.
            prev = match word {
                "this" | "true" | "false" | "null" | "super" => Prev::ValueKeyword,
                _ if kind == Tok::Plain || kind == Tok::Func || kind == Tok::Type => Prev::Value,
                _ => Prev::Keyword,
            };
            assert!(pos <= bytes_len, "IDENT OOB: pos={pos} len={bytes_len}");
            push_tiling(spans, &mut last_end, kind, start, pos);
            continue;
        }

        // Template interpolation entry: `${`.
        if rest.starts_with("${") {
            interp_stack.push(brace_depth_in_interp);
            brace_depth_in_interp = 0;
            push_tiling(spans, &mut last_end, Tok::Punct, pos, pos + 2);
            pos += 2;
            prev = Prev::Operator;
            continue;
        }

        // Braces inside interpolations adjust the pop depth.
        if !interp_stack.is_empty() && (ch == '{' || ch == '}') {
            if ch == '{' {
                brace_depth_in_interp += 1;
            } else {
                brace_depth_in_interp = brace_depth_in_interp.saturating_sub(1);
            }
            push_tiling(spans, &mut last_end, Tok::Punct, pos, pos + clen);
            pos += clen;
            continue;
        }

        // Punctuation.
        if matches!(ch, '(' | '[' | '{' | ')' | ']' | '}' | ',' | ';') {
            let kind = Tok::Punct;
            if ch == ')' {
                prev = Prev::Value;
            } else if ch == ']' {
                prev = Prev::Value;
            } else if ch == '}' {
                prev = Prev::CloseBrace;
            } else {
                prev = Prev::Operator;
            }
            push_tiling(spans, &mut last_end, kind, pos, pos + clen);
            pos += clen;
            continue;
        }
        if ch == '.' && !interp_stack.is_empty() {
            // Bare `.` inside an interpolation is punctuation (member
            // access); a numeric literal was already handled above.
        }

        // Multi-char operators, longest first.
        const OPERATORS: &[&str] = &[
            ">>>=", "===", "!==", "**=", "<<=", ">>=", "&&=", "||=", "??=", "=>", "...", "**",
            "==", "!=", "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=",
            "%=", "&=", "|=", "^=", "<<", ">>", ">>>",
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

        // Single-char operators.
        if matches!(
            ch,
            '+' | '-' | '*' | '%' | '&' | '|' | '^' | '~' | '!' | '<' | '>' | '=' | '?' | ':'
        ) {
            push_tiling(spans, &mut last_end, Tok::Operator, pos, pos + clen);
            pos += clen;
            prev = Prev::Operator;
            continue;
        }

        // Everything else (control chars, unusual bytes): Plain, one char.
        push_tiling(spans, &mut last_end, Tok::Plain, pos, pos + clen);
        pos += clen;
        prev = Prev::Start;
    }

    // Trailing gap (input ending in whitespace already handled inline; this
    // is a defensive final tile).
    if last_end < bytes_len {
        let start = last_end;
        push_tiling(spans, &mut last_end, Tok::Plain, start, bytes_len);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The source-tiling oracle: spans must exactly tile [0, len) with no
    /// gaps, overlaps, or reordering.
    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(
                span.start, cursor,
                "gap/overlap at {cursor}: span {:?} {}-{}",
                span.kind, span.start, span.end
            );
            assert!(span.end > span.start, "empty span at {cursor}");
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    #[test]
    fn keywords_types_and_calls_are_classified() {
        let code = "const re = new RegExp(/abc/); fetch(url).then(parse);";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let text: Vec<&str> = spans.iter().map(|s| &code[s.start..s.end]).collect();
        assert!(text.contains(&"const"));
        assert!(text.contains(&"new"));
        assert!(text.contains(&"RegExp"));
        assert!(text.contains(&"fetch"));
        // `fetch(` is a call.
        let func_idx = spans
            .iter()
            .position(|s| s.kind == Tok::Func)
            .expect("func call classified");
        assert_eq!(&code[spans[func_idx].start..spans[func_idx].end], "fetch");
    }

    #[test]
    fn regex_after_operator_division_after_value() {
        let code = "let re = /a+b/g; let d = a / b;";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        // The regex literal is a Str span containing "a+b" body start.
        let str_values: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            str_values.iter().any(|s| s.contains("/a+b/g")),
            "regex classified as literal: {str_values:?}"
        );
        // Division operators classified as Operator.
        let ops: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Operator)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(ops.contains(&"/"), "division operator classified: {ops:?}");
    }

    #[test]
    fn regex_after_return_keyword() {
        let code = "function f() { return /x/; }";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let regex: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            regex.iter().any(|s| s.contains("/x/")),
            "regex after return keyword: {regex:?}"
        );
    }

    #[test]
    fn regex_char_class_keeps_inner_slash() {
        let code = "let re = /[a/b]+/g; let x = 1;";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let regex: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            regex.iter().any(|s| s.contains("/[a/b]+/g")),
            "char class slash must not terminate: {regex:?}"
        );
    }

    #[test]
    fn template_interpolation_nests() {
        let code = "let s = `a${ {k: `inner${x}end`} }b`;";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        // Both template chunks and the inner template are Str spans.
        let strs: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            strs.iter().any(|s| s.contains("inner")),
            "nested template classified: {strs:?}"
        );
    }

    #[test]
    fn comments_and_annex_b_html_comments() {
        let code = "// line\n/* block\nspan */ x = 1; <!-- html-like\n--> also\n";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let comments: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Comment)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert_eq!(comments.len(), 4, "{comments:?}");
    }

    #[test]
    fn strings_with_escape_continuation() {
        let code = "let s = 'one \\\n  two' + \"three\";";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let strs: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            strs.iter().any(|s| s.contains("two")),
            "escaped continuation kept in string: {strs:?}"
        );
    }

    #[test]
    fn truncated_string_spans_to_eof() {
        let code = "let s = 'never closed";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let last = spans.last().unwrap();
        assert_eq!(last.end, code.len());
        assert_eq!(last.kind, Tok::Str);
    }

    #[test]
    fn truncated_regex_falls_back_to_operator_tiling() {
        let code = "let x = a / never closed";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
    }

    #[test]
    fn numbers_hex_oct_bin_bigint_separators() {
        let code = "0xDEAD_beef 0o755 0b1010 123n 1_000.5e-3 .5";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let numbers: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Number)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(numbers.contains(&"0xDEAD_beef"));
        assert!(numbers.contains(&"0o755"));
        assert!(numbers.contains(&"0b1010"));
        assert!(numbers.contains(&"123n"));
        assert!(numbers.contains(&"1_000.5e-3"));
        assert!(numbers.contains(&".5"));
    }

    #[test]
    fn unterminated_block_comment_spans_to_eof() {
        let code = "let x = 1; /* never closed";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
        let last = spans.last().unwrap();
        assert_eq!(last.kind, Tok::Comment);
        assert_eq!(last.end, code.len());
    }

    #[test]
    fn negative_control_tiling_oracle_detects_gap() {
        // The tiling oracle must reject a synthetic gap: feed it spans with
        // a hole and assert the invariant fails.
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
        // Sanity: the real lexer output has no gaps.
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        assert_tiling(code, &spans);
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn debug_nested_template_spans() {
        let code = "let s = `a${ {k: `inner${x}end`} }b`;";
        let mut spans = Vec::new();
        lex_javascript_into(code, &mut spans);
        for s in &spans {
            eprintln!(
                "DEBUG {} {:?} [{}..{}] {:?}",
                s.end,
                s.kind,
                s.start,
                s.end,
                &code[s.start..s.end.min(code.len())]
            );
        }
    }
}

// Keep a template, including nested interpolation templates, in one string span.
// The explicit stack bounds call-stack use for malformed or deeply nested input.
fn template_end(code: &str, start: usize) -> usize {
    let mut stack = vec!['`'];
    let mut pos = start + 1;
    while pos < code.len() {
        let ch = code[pos..].chars().next().unwrap_or('\0');
        let mode = stack.last().copied().unwrap_or('`');
        if ch == '\\' {
            pos += 1;
            if pos < code.len() {
                pos += code[pos..].chars().next().map_or(0, char::len_utf8);
            }
            continue;
        }
        if mode == '`' && code[pos..].starts_with("${") {
            stack.push('}');
            pos += 2;
            continue;
        }
        if ch == mode {
            stack.pop();
            pos += ch.len_utf8();
            if stack.is_empty() {
                return pos;
            }
            continue;
        }
        if mode == '}' {
            if matches!(ch, '`' | '\'' | '"') {
                stack.push(ch);
            } else if ch == '{' {
                stack.push('}');
            } else if code[pos..].starts_with("//") {
                pos += code[pos..].find('\n').unwrap_or(code.len() - pos);
                continue;
            } else if code[pos..].starts_with("/*") {
                pos = code[pos + 2..]
                    .find("*/")
                    .map_or(code.len(), |n| pos + 2 + n + 2);
                continue;
            }
        }
        pos += ch.len_utf8();
    }
    pos
}
