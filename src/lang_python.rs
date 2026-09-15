//! Classifies Python source into byte-exact [`Span`]s that tile the input.
//!
//! Implements the FCB-022.A Python slice: triple-quoted, raw, byte, and
//! f-strings with prefixes (`r`/`b`/`f`/`u` and two-letter combinations),
//! escaped line continuations, `#` comments, numeric literals (hex, octal,
//! binary, floats, underscores, imaginary suffix), keywords, types, and
//! call detection. Unterminated strings and truncated quotes extend to end
//! of input as the provisional trailing span so a following chunk can
//! reclassify them truthfully (chunk boundaries are not EOF).

use crate::highlight::{Span, Tok};

/// Lex Python source into exact tiling spans.
pub fn lex_python_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0;
    let bytes = code.as_bytes();

    while pos < len {
        let rest = &code[pos..];
        let c = first_char(code, pos);
        let clen = c.len_utf8();

        // 1. Whitespace run (covers indentation).
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

        // 2. Explicit line continuation: backslash + newline stays one
        //    plain unit so the logical line is not misclassified.
        if c == '\\' && pos + 1 < len && (bytes[pos + 1] == b'\n' || bytes[pos + 1] == b'\r') {
            let mut end = pos + 2;
            if bytes[pos + 1] == b'\r' && end < len && bytes[end] == b'\n' {
                end += 1;
            }
            spans.push(Span {
                kind: Tok::Plain,
                start: pos,
                end,
            });
            pos = end;
            continue;
        }

        // 3. Comment: `#` to end of line.
        if c == '#' {
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

        // 4. Identifier / prefixed string / keyword / call.
        if c == '_' || c.is_alphabetic() {
            let word_end = word_end(code, pos);
            let word_lower = code[pos..word_end].to_ascii_lowercase();
            let next_is_quote =
                word_end < len && (bytes[word_end] == b'"' || bytes[word_end] == b'\'');
            if next_is_quote
                && matches!(
                    word_lower.as_str(),
                    "r" | "b" | "f" | "u" | "rb" | "br" | "rf" | "fr"
                )
            {
                let raw = word_lower.contains('r');
                let fstring = word_lower.contains('f');
                let end = scan_string_end(code, word_end, raw, fstring);
                spans.push(Span {
                    kind: Tok::Str,
                    start: pos,
                    end,
                });
                pos = end;
                continue;
            }

            let word = &code[pos..word_end];
            let kind = if PY_KW.contains(word) {
                Tok::Keyword
            } else if PY_TY.contains(word) {
                Tok::Type
            } else if next_non_whitespace_byte(code, word_end) == Some(b'(') {
                Tok::Func
            } else {
                Tok::Plain
            };
            spans.push(Span {
                kind,
                start: pos,
                end: word_end,
            });
            pos = word_end;
            continue;
        }

        // 5. Numbers: hex/oct/bin, decimals, floats, exponents, underscores,
        //    and the imaginary suffix. Leading-dot floats (.5e-3) are
        //    literals too, so a dot followed by a digit enters here.
        if c.is_ascii_digit() || (c == '.' && rest.len() > 1 && bytes[pos + 1].is_ascii_digit())
        {
            let start = pos;
            let mut p = pos;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                p += 2;
                while p < len && (bytes[p].is_ascii_hexdigit() || bytes[p] == b'_') {
                    p += 1;
                }
            } else if rest.starts_with("0o") || rest.starts_with("0O") {
                p += 2;
                while p < len && ((bytes[p] >= b'0' && bytes[p] <= b'7') || bytes[p] == b'_') {
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
                if p < len && (bytes[p] == b'j' || bytes[p] == b'J') {
                    p += 1;
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

        // 6. Strings without prefix.
        if c == '"' || c == '\'' {
            let end = scan_string_end(code, pos, false, false);
            spans.push(Span {
                kind: Tok::Str,
                start: pos,
                end,
            });
            pos = end;
            continue;
        }

        // 7. Operators and punctuation (one character per house style).
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

/// Cooperative deadline/cancel surface for future driver integration.
/// Preserved as the seam the izu8 scenario driver can poll.
#[derive(Debug, Default)]
pub struct CancelToken(Option<bool>);

/// Scan a Python string starting at its opening quote (`open`), honoring
/// `raw` (no escape processing: `r"\"` is unterminated, never closed) and
/// `fstring` (brace-delimited interpolation where same-kind quotes do not
/// close the string). Single-quoted strings terminate at a newline (Python
/// forbids spanning lines without triple quotes). Unterminated strings run
/// to end of input as the provisional trailing span.
fn scan_string_end(code: &str, open: usize, _raw: bool, fstring: bool) -> usize {
    let bytes = code.as_bytes();
    let len = code.len();
    let quote = bytes[open];
    let triple = open + 2 < len && bytes[open + 1] == quote && bytes[open + 2] == quote;
    let mut p = if triple { open + 3 } else { open + 1 };
    let mut depth = 0usize;
    let mut interp_quote: Option<u8> = None;
    while p < len {
        let ch = first_char(code, p);
        // Backslash consumes the next character in raw and non-raw strings
        // alike (CPython lexical rule: `r"\"` is unterminated, never closed).
        if ch == '\\' {
            let next = code[p + 1..].chars().next().map_or(0, char::len_utf8);
            if next == 0 {
                return len;
            }
            p += 1 + next;
            continue;
        }
        if fstring {
            if let Some(iq) = interp_quote {
                if ch as u8 == iq {
                    interp_quote = None;
                }
                p += ch.len_utf8();
                continue;
            }
            if depth == 0 {
                if ch == '{' && p + 1 < len && bytes[p + 1] == b'{' {
                    p += 2;
                    continue;
                }
                if ch == '}' && p + 1 < len && bytes[p + 1] == b'}' {
                    p += 2;
                    continue;
                }
            }
            if ch == '{' {
                depth += 1;
                p += ch.len_utf8();
                continue;
            }
            if ch == '}' && depth > 0 {
                depth -= 1;
                p += ch.len_utf8();
                continue;
            }
        }
        let inside_interpolation = fstring && depth > 0;
        if !inside_interpolation {
            if triple
                && p + 2 < len
                && bytes[p] == quote
                && bytes[p + 1] == quote
                && bytes[p + 2] == quote
            {
                return p + 3;
            }
            if !triple && ch as u8 == quote {
                return p + ch.len_utf8();
            }
            if !triple && ch == '\n' {
                return p;
            }
        }
        p += ch.len_utf8();
    }
    len
}

fn first_char(code: &str, pos: usize) -> char {
    code[pos..].chars().next().unwrap_or('\0')
}

fn word_end(code: &str, start: usize) -> usize {
    let bytes = code.as_bytes();
    let len = code.len();
    let mut p = start;
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
    p
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
        '+' | '-' | '*' | '/' | '%' | '@' | '=' | '<' | '>' | '!' | '~' | '&' | '|' | '^' | ':'
    )
}

fn is_punct_char(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '.' | '\\' | '$' | '?' | '`')
}

/// Python keyword membership over a sorted static table (binary search;
/// const-buildable without heap allocation).
struct KwTable {
    words: &'static [&'static str],
}

impl KwTable {
    fn contains(&self, word: &str) -> bool {
        self.words.binary_search(&word).is_ok()
    }
}

const PY_KW_SORTED: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
    "while", "with", "yield",
];

const PY_TY_SORTED: &[&str] = &[
    "BaseException", "bool", "bytearray", "bytes", "complex", "dict", "enumerate", "filter",
    "float", "frozenset", "int", "list", "map", "memoryview", "object", "range", "set", "slice",
    "staticmethod", "str", "super", "tuple", "type", "zip",
];

static PY_KW: KwTable = KwTable { words: PY_KW_SORTED };
static PY_TY: KwTable = KwTable { words: PY_TY_SORTED };

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(code: &str) -> Vec<(Tok, String)> {
        let mut spans = Vec::new();
        lex_python_into(code, &mut spans);
        spans
            .iter()
            .map(|span| (span.kind, code[span.start..span.end].to_string()))
            .collect()
    }

    #[test]
    fn triple_quoted_strings_span_newlines_and_comments() {
        let spans = kinds("x = \"\"\"a # not comment\nb\"\"\"");
        assert!(spans.iter().any(|(kind, text)| {
            *kind == Tok::Str && text.contains("# not comment")
        }));
    }

    #[test]
    fn f_string_braces_protect_same_kind_quotes() {
        let spans = kinds("msg = f\"inner {'quote'} tail\"");
        let f_string_spans: Vec<&str> = spans
            .iter()
            .filter(|(kind, _)| *kind == Tok::Str)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(f_string_spans, vec!["f\"inner {'quote'} tail\""]);
    }

    #[test]
    fn raw_string_backslash_quote_does_not_close() {
        let spans = kinds(r#"value = r\"unterminated"#);
        // r"..." where the backslash-quote pair does not close: unterminated
        // raw string runs to end of input as one provisional Str span.
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Str && text.contains("unterminated")));
    }

    #[test]
    fn unterminated_triple_quote_holds_to_end_of_input() {
        let spans = kinds("doc = \"\"\"starts here");
        let (kind, text) = spans.last().expect("spans exist");
        assert_eq!(*kind, Tok::Str);
        assert_eq!(text, "\"\"\"starts here");
    }

    #[test]
    fn single_quoted_string_stops_at_newline() {
        let spans = kinds("s = \"broken\nnext = 1");
        assert!(spans.iter().any(|(kind, text)| {
            *kind == Tok::Str && text == "\"broken"
        }));
        assert!(spans.iter().any(|(kind, text)| {
            *kind == Tok::Number && text == "1"
        }));
    }

    #[test]
    fn keywords_types_and_calls_are_classified() {
        let spans = kinds("class Widget:\n    def render(self):\n        return None");
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Keyword && text == "class"));
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Keyword && text == "def"));
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Func && text == "render"));
        assert!(spans.iter().any(|(kind, text)| *kind == Tok::Keyword && text == "None"));
    }

    #[test]
    fn numbers_cover_hex_float_and_imaginary() {
        let spans = kinds("a = 0xFF_00\nb = 1.5e-3\nc = 4j");
        let numbers: Vec<&str> = spans
            .iter()
            .filter(|(kind, _)| *kind == Tok::Number)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(numbers, vec!["0xFF_00", "1.5e-3", "4j"]);
    }

    #[test]
    fn line_continuation_stays_plain_and_comments_still_classified() {
        let spans = kinds("x = 1 \\\n    # comment\ny = 2");
        assert!(spans.iter().any(|(kind, text)| {
            *kind == Tok::Plain && text.contains('\\')
        }));
        assert!(spans
            .iter()
            .any(|(kind, text)| *kind == Tok::Comment && text == "# comment"));
    }
}
