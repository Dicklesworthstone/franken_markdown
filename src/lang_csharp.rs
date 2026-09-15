//! Classifies C# source into byte-exact [`Span`]s that tile the input.
//!
//! Implements the FCB-022.A C# slice: `//` and `/* */` comments, regular,
//! verbatim (`@"`), interpolated (`$"`), and raw (`"""`) strings with
//! brace-aware interpolation, character literals, numeric literals (hex,
//! binary, decimals, underscores, `f/d/m/u/l` suffixes), preprocessor
//! directive lines (keyword-classified `#word`), the C# keyword and BCL
//! type tables, and `@`-escaped identifiers. Unterminated constructs
//! extend to end of input as the provisional trailing span so a following
//! chunk reclassifies them truthfully (chunk boundaries are not EOF).

use crate::highlight::{Span, Tok};

/// Lex C# source into exact tiling spans.
pub fn lex_csharp_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0;
    let bytes = code.as_bytes();

    while pos < len {
        let rest = &code[pos..];
        let c = first_char(code, pos);
        let clen = c.len_utf8();

        // 1. Whitespace run.
        if c.is_whitespace() {
            let start = pos;
            while pos < len {
                let b = bytes[pos];
                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                    pos += 1;
                } else if b >= 0x80 && first_char(code, pos).is_whitespace() {
                    pos += first_char(code, pos).len_utf8();
                } else {
                    break;
                }
            }
            spans.push(Span {
                kind: Tok::Plain,
                start,
                end: pos,
            });
            continue;
        }

        // 2. Line comment.
        if rest.starts_with("//") {
            let start = pos;
            let end = rest.find('\n').map_or(len, |offset| pos + offset);
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // 3. Block comment (C# block comments do not nest).
        if rest.starts_with("/*") {
            let start = pos;
            let mut p = pos + 2;
            while p + 1 < len && !(bytes[p] == b'*' && bytes[p + 1] == b'/') {
                p += 1;
            }
            let end = (p + 2).min(len);
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // 4. Preprocessor directive: `#` as the first non-whitespace token
        //    of a line classifies the directive word as a keyword and the
        //    remainder of the line as plain. A `#` elsewhere is operator
        //    punctuation (`#nullable`-style directives are line-bound).
        if c == '#' {
            let at_line_start = {
                let mut p = pos;
                while p > 0 && (bytes[p - 1] == b' ' || bytes[p - 1] == b'\t') {
                    p -= 1;
                }
                p == 0 || bytes[p - 1] == b'\n'
            };
            let end = rest.find('\n').map_or(len, |offset| pos + offset);
            if at_line_start {
                // Directive line: keyword-classified `#word`, remainder plain.
                let mut word_end = start + 1;
                while word_end < end && word_end < len {
                    let b = bytes[word_end];
                    if b == b'_' || b.is_ascii_alphanumeric() {
                        word_end += 1;
                    } else {
                        break;
                    }
                }
                spans.push(Span {
                    kind: Tok::Keyword,
                    start,
                    end: word_end.max(start + 1),
                });
                if word_end < end {
                    spans.push(Span {
                        kind: Tok::Plain,
                        start: word_end,
                        end,
                    });
                }
                pos = end;
                continue;
            }
            // Mid-line `#` is a single operator character; the rest of the
            // line lexes normally.
            spans.push(Span {
                kind: Tok::Operator,
                start: pos,
                end: pos + 1,
            });
            pos += 1;
            continue;
        }

        // 5. Verbatim / interpolated / raw / regular strings. Prefixes:
        //    @ (verbatim), $ (interpolated), combinations, and C# 11
        //    raw triple-quoted strings.
        if c == '@' || c == '$' || c == '"' {
            if let Some(end) = scan_prefixed_string_end(code, pos) {
                spans.push(Span {
                    kind: Tok::Str,
                    start: pos,
                    end,
                });
                pos = end;
                continue;
            }
        }

        // 6. Character literal: 'c', '\n', '\u0041'. Unterminated falls
        //    through to the operator branch.
        if c == '\'' {
            let start = pos;
            let mut p = pos + 1;
            let mut closed = false;
            if p < len && bytes[p] == b'\\' {
                p += 1;
                let next = code[p..].chars().next();
                p += next.map_or(0, char::len_utf8);
                // Unicode escape forms \u0041 / \U00010041.
                while p < len && bytes[p].is_ascii_hexdigit() && p <= start + 7 {
                    p += 1;
                }
            } else if p < len {
                p += first_char(code, p).len_utf8();
            }
            if p < len && bytes[p] == b'\'' {
                p += 1;
                closed = true;
            }
            if closed {
                spans.push(Span {
                    kind: Tok::Str,
                    start,
                    end: p,
                });
                pos = p;
                continue;
            }
            // Fall through: lone quote is operator punctuation.
        }

        // 7. Numbers: hex, binary, decimals, underscores, suffixes.
        if c.is_ascii_digit() {
            let start = pos;
            let mut p = pos;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                p += 2;
                while p < len && (bytes[p].is_ascii_hexdigit() || bytes[p] == b'_') {
                    p += 1;
                }
            } else if rest.starts_with("0b") || rest.starts_with("0B") {
                p += 2;
                while p < len && (bytes[p] == b'0' || bytes[p] == b'1' || bytes[p] == b'_') {
                    p += 1;
                }
            } else {
                while p < len {
                    let b = bytes[p];
                    if b == b'.' {
                        if bytes.get(p + 1).is_some_and(|next| next.is_ascii_digit()) {
                            p += 1;
                        } else {
                            break;
                        }
                    } else if b == b'e' || b == b'E' {
                        p += 1;
                        if p < len && (bytes[p] == b'+' || bytes[p] == b'-') {
                            p += 1;
                        }
                    } else if b.is_ascii_digit() || b == b'_' {
                        p += 1;
                    } else {
                        break;
                    }
                }
                while p < len
                    && matches!(
                        bytes[p],
                        b'f' | b'F' | b'd' | b'D' | b'm' | b'M' | b'u' | b'U' | b'l' | b'L'
                    )
                {
                    // Numeric suffix run; stop at anything word-like beyond
                    // a single suffix cluster (e.g. `1ul`).
                    let mut suffix_len = 0;
                    let mut q = p;
                    while q < len
                        && matches!(
                            bytes[q],
                            b'f' | b'F' | b'd' | b'D' | b'm' | b'M' | b'u' | b'U' | b'l' | b'L'
                        )
                    {
                        suffix_len += 1;
                        q += 1;
                    }
                    if suffix_len > 2 {
                        break;
                    }
                    p = q;
                }
            }
            spans.push(Span {
                kind: Tok::Number,
                start,
                end: p,
            });
            pos = p;
            continue;
        }

        // 8. Identifier / keyword / type / call. `@` prefix escapes even
        //    keyword spellings (`@class` is a plain identifier).
        if c == '@'
            && pos + 1 < len
            && (bytes[pos + 1] == b'_' || first_char(code, pos + 1).is_alphabetic())
        {
            let start = pos;
            let mut p = pos + 1;
            while p < len {
                let b = bytes[p];
                if b == b'_' || b.is_ascii_alphanumeric() {
                    p += 1;
                } else if b >= 0x80 {
                    let ch = first_char(code, p);
                    if ch == '_' || ch.is_alphanumeric() {
                        p += ch.len_utf8();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            spans.push(Span {
                kind: Tok::Plain,
                start,
                end: p,
            });
            pos = p;
            continue;
        }
        if c == '_' || c.is_alphabetic() {
            let start = pos;
            let mut p = pos;
            while p < len {
                let b = bytes[p];
                if b == b'_' || b.is_ascii_alphanumeric() {
                    p += 1;
                } else if b >= 0x80 {
                    let ch = first_char(code, p);
                    if ch == '_' || ch.is_alphanumeric() {
                        p += ch.len_utf8();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let word = &code[start..p];
            let is_capitalized = word
                .chars()
                .next()
                .map(|first| first.is_uppercase())
                .unwrap_or(false)
                && !word.chars().skip(1).all(|ch| ch.is_uppercase());
            let kind = if CS_KW.contains(word) {
                Tok::Keyword
            } else if CS_TY.contains(word) {
                Tok::Type
            } else if is_capitalized {
                Tok::Type
            } else if next_non_whitespace_byte(code, p) == Some(b'(') {
                Tok::Func
            } else {
                Tok::Plain
            };
            spans.push(Span {
                kind,
                start,
                end: p,
            });
            pos = p;
            continue;
        }

        // 9. Operators and punctuation.
        let kind = if is_operator_char(c) {
            Tok::Operator
        } else if is_punct_char(c) {
            Tok::Punct
        } else {
            Tok::Plain
        };
        spans.push(Span {
            kind,
            start: pos,
            end: pos + clen,
        });
        pos += clen;
    }
}

/// Scan a C# string starting at `start` (the `@`, `$`, `"` byte) and return
/// its exclusive end. Handles verbatim `@"..."` (doubled-quote escapes,
/// spans newlines), interpolated `$"..."` (brace-aware: same-kind quotes
/// inside `{...}` do not close), combinations (`@$`, `$@`), raw
/// `"""..."""` (C# 11), and regular `"..."` (backslash escapes).
fn scan_prefixed_string_end(code: &str, start: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let len = code.len();
    let mut p = start;
    let mut verbatim = false;
    let mut interpolated = false;
    let mut raw = false;
    while p < len && matches!(bytes[p], b'@' | b'$') {
        match bytes[p] {
            b'@' => verbatim = true,
            b'$' => interpolated = true,
            _ => unreachable!(),
        }
        p += 1;
    }
    // Raw string: three or more opening quotes; closes at a quote run of
    // at least the opening length.
    let mut quote_run = 0;
    let mut q = p;
    while q < len && bytes[q] == b'"' {
        quote_run += 1;
        q += 1;
    }
    if quote_run >= 3 {
        raw = true;
        let mut closing = 0;
        let mut scan = p + quote_run;
        while scan < len {
            if bytes[scan] == b'"' {
                closing += 1;
                if closing >= quote_run {
                    return Some(scan + 1);
                }
            } else {
                closing = 0;
            }
            scan += 1;
        }
        return Some(len);
    }
    if quote_run == 0 {
        return None; // prefix without opening quote: not a string
    }
    let mut closing_quote = bytes[p];
    p += 1; // consume the opening quote

    let mut brace_depth = 0usize;
    let mut interp_quote: Option<u8> = None;
    while p < len {
        let ch = first_char(code, p);
        if interpolated {
            if let Some(iq) = interp_quote {
                if verbatim {
                    // Verbatim interpolation keeps doubled-quote escapes.
                    if ch as u8 == iq && p + 1 < len && bytes[p + 1] == iq {
                        p += 2;
                        continue;
                    }
                } else if ch == '\\' {
                    let next = code[p + 1..].chars().next().map_or(0, char::len_utf8);
                    if next == 0 {
                        return Some(len);
                    }
                    p += 1 + next;
                    continue;
                }
                if ch as u8 == iq {
                    interp_quote = None;
                }
                p += ch.len_utf8();
                continue;
            }
            if ch == '{' {
                if p + 1 < len && bytes[p + 1] == b'{' {
                    p += 2;
                    continue;
                }
                brace_depth += 1;
                p += ch.len_utf8();
                continue;
            }
            if ch == '}' && brace_depth > 0 {
                brace_depth -= 1;
                p += ch.len_utf8();
                continue;
            }
        }
        if verbatim {
            if ch == '"' {
                if p + 1 < len && bytes[p + 1] == b'"' && brace_depth > 0 {
                    // Doubled quote inside interpolation of a verbatim
                    // string stays literal.
                    p += 2;
                    continue;
                }
                if p + 1 < len && bytes[p + 1] == b'"' {
                    p += 2;
                    continue;
                }
                return Some(p + 1);
            }
            p += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            let next = code[p + 1..].chars().next().map_or(0, char::len_utf8);
            if next == 0 {
                return Some(len);
            }
            p += 1 + next;
            continue;
        }
        if ch == '\n' && !interpolated && brace_depth == 0 {
            // Regular strings do not span newlines: broken string.
            return Some(p);
        }
        if interpolated {
            if ch == '{' {
                brace_depth += 1;
                p += ch.len_utf8();
                continue;
            }
            if ch == '}' && brace_depth > 0 {
                brace_depth -= 1;
                p += ch.len_utf8();
                continue;
            }
            if (ch == '"' || ch == '\'') && brace_depth > 0 {
                interp_quote = Some(ch as u8);
                p += ch.len_utf8();
                continue;
            }
        }
        if ch as u8 == closing_quote {
            return Some(p + 1);
        }
        p += ch.len_utf8();
    }
    Some(len)
}

fn first_char(code: &str, pos: usize) -> char {
    code[pos..].chars().next().unwrap_or('\0')
}

fn next_non_whitespace_byte(code: &str, pos: usize) -> Option<u8> {
    let bytes = code.as_bytes();
    let mut p = pos;
    while p < bytes.len() {
        let b = bytes[p];
        if b == b' ' || b == b'\t' || b == b'\r' {
            p += 1;
        } else {
            return Some(b);
        }
    }
    None
}

fn is_operator_char(c: char) -> bool {
    matches!(
        c,
        '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '?' | ':'
    )
}

fn is_punct_char(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '.' | '@' | '#')
}

/// C# keyword membership over a sorted static table (binary search;
/// const-buildable without heap allocation).
struct KwTable {
    words: &'static [&'static str],
}

impl KwTable {
    fn contains(&self, word: &str) -> bool {
        self.words.binary_search(&word).is_ok()
    }
}

const CS_KW_SORTED: &[&str] = &[
    "abstract", "as", "async", "await", "base", "bool", "break", "byte", "case", "catch", "char",
    "checked", "class", "const", "continue", "decimal", "default", "delegate", "do", "double",
    "dynamic", "else", "enum", "event", "explicit", "extern", "false", "finally", "fixed",
    "float", "for", "foreach", "get", "goto", "if", "implicit", "in", "init", "int", "interface",
    "internal", "is", "lock", "long", "namespace", "new", "null", "object", "operator", "out",
    "override", "params", "partial", "private", "protected", "public", "readonly", "ref",
    "return", "sbyte", "sealed", "set", "short", "sizeof", "stackalloc", "static", "string",
    "struct", "switch", "this", "throw", "true", "try", "typeof", "uint", "ulong", "unchecked",
    "unsafe", "ushort", "using", "var", "virtual", "void", "volatile", "when", "where", "while",
    "yield",
];

const CS_TY_SORTED: &[&str] = &[
    "ArgumentException", "Boolean", "Byte", "Char", "Console", "Decimal", "Dictionary", "Double",
    "Exception", "Int16", "Int32", "Int64", "List", "Object", "Single", "String", "Task",
];

static CS_KW: KwTable = KwTable {
    words: CS_KW_SORTED,
};
static CS_TY: KwTable = KwTable {
    words: CS_TY_SORTED,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(code: &str) -> Vec<(Tok, String)> {
        let mut spans = Vec::new();
        lex_csharp_into(code, &mut spans);
        spans
            .iter()
            .map(|span| (span.kind, code[span.start..span.end].to_string()))
            .collect()
    }

    #[test]
    fn keywords_types_and_calls_are_classified() {
        let spans = kinds("public class Widget {\n    var count = Read();\n}");
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Keyword && text == "public"));
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Keyword && text == "class"));
        assert!(
            spans.iter().any(|(kind, text)| *kind == Tok::Type && text == "Widget"),
            "spans: {spans:?}"
        );
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Keyword && text == "var"));
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Func && text == "Read"));
    }

    #[test]
    fn at_escaped_identifiers_are_plain_not_keywords() {
        let owned = kinds("@class @event");
        assert!(owned.contains(&(Tok::Plain, "@class".to_string())));
        assert!(owned.contains(&(Tok::Plain, "@event".to_string())));
        assert!(owned.iter().all(|(kind, _)| *kind == Tok::Plain));
    }

    #[test]
    fn line_and_block_comments_are_classified() {
        let spans = kinds("// line\n/* block\nspan */ x");
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Comment && text == "// line"));
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Comment && text.contains("block")));
    }

    #[test]
    fn verbatim_strings_span_newlines_and_double_quotes() {
        let spans = kinds("var s = @\"line1\nline2 with \"\"quote\"\" end\";");
        assert!(spans.iter().any(|(kind, text)| {
            *kind == Tok::Str && text.contains("line2") && text.contains("\"\"quote\"\"")
        }));
    }

    #[test]
    fn interpolated_braces_protect_same_kind_quotes() {
        let spans = kinds("var m = $\"x {args[\"inner\"]} y\";");
        // The inner "inner" quote lives inside braces; the string closes at
        // the final quote.
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Str && text.contains("inner")));
    }

    #[test]
    fn raw_triple_quoted_strings_close_at_run() {
        let spans = kinds("var r = \"\"\"raw \"partial\" text\"\"\";");
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Str && text.contains("raw \"partial\" text")));
    }

    #[test]
    fn preprocessor_directives_are_line_bound_keywords() {
        let spans = kinds("    #nullable enable\nx = a # b;");
        assert!(spans.iter().any(|(kind, text)| {
            *kind == Tok::Keyword && text == "#nullable"
        }));
        // Mid-line hash is operator punctuation, not a directive.
        assert!(
            spans
                .iter()
                .any(|(kind, text)| *kind == Tok::Operator && text == "#"),
            "spans: {spans:?}"
        );
    }

    #[test]
    fn numbers_cover_hex_binary_float_and_suffixes() {
        let spans = kinds("0xFF_ul 0b1010 1.5m 3e-2f");
        let numbers: Vec<&str> = spans
            .iter()
            .filter(|(kind, _)| *kind == Tok::Number)
            .map(|(_, text)| text.as_str())
            .collect();
        assert!(numbers.contains(&"0xFF_ul"), "numbers: {numbers:?}");
        assert!(numbers.contains(&"0b1010"));
        assert!(numbers.contains(&"1.5m"));
        assert!(numbers.contains(&"3e-2f"));
    }

    #[test]
    fn regular_strings_stop_at_newline() {
        let spans = kinds("var a = \"broken\nvar b = 2;");
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Str && text == "\"broken"));
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Number && text == "2"));
    }

    #[test]
    fn character_literals_and_lone_quotes() {
        let spans = kinds("var c = 'x';\nvar q = ';");
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Str && text == "'x'"));
    }

    #[test]
    fn error_codes_are_stable() {
        let spans = kinds("var s = \"unterminated");
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Str && text.contains("unterminated")));
    }
}
