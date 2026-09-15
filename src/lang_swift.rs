#![forbid(unsafe_code)]

//! Swift incremental lexer (FCB-022 · fcb-9vx.15).
//!
//! Classifies Swift source into byte-exact [`Span`]s that tile the input.
//! Implements:
//! - Line comments (`//`) and nested block comments (`/* /* nested */ */`).
//! - Multiline strings (`""" ... """`), raw strings (`#"..."#`, `##"..."##`),
//!   standard strings (`"..."`), and string interpolation (`\(expr)`).
//! - Backtick-escaped identifiers (`` `default` ``).
//! - Attributes (`@objc`, `@Published`, `@State`, `@escaping`, etc.).
//! - Hash directives (`#if`, `#else`, `#endif`, `#available`, etc.).
//! - Numbers: hex (`0xCAFE`), binary (`0b1010`), octal (`0o755`), floats,
//!   scientific notation (`1.5e-3`), and exponents.
//! - Keywords, contextual keywords (`actor`, `async`, `await`, `some`),
//!   standard library types, and function calls.
//! - Multi-character operators (`...`, `..<`, `->`, `??`, etc.).
//! - Exact source byte tiling across whole inputs and arbitrary splits.

use crate::highlight::{Span, Tok};

/// Swift keywords, contextual keywords, and literal values.
pub const SWIFT_KEYWORDS: &[&str] = &[
    // Declarations
    "associatedtype", "class", "deinit", "enum", "extension", "fileprivate",
    "func", "import", "init", "inout", "internal", "let", "open", "operator",
    "private", "precedencegroup", "protocol", "public", "rethrows", "static",
    "struct", "subscript", "typealias", "var",
    // Statements
    "break", "case", "catch", "continue", "default", "defer", "do", "else",
    "fallthrough", "for", "guard", "if", "in", "repeat", "return", "throw",
    "switch", "where", "while",
    // Expressions & Types
    "as", "Any", "await", "async", "false", "is", "nil", "self", "Self",
    "super", "throws", "true", "try",
    // Contextual keywords & modifiers
    "actor", "convenience", "dynamic", "final", "indirect", "lazy", "macro",
    "mutating", "nonmutating", "nonisolated", "optional", "override", "prefix",
    "postfix", "required", "some", "unowned", "weak", "willSet", "didSet",
    "get", "set", "consuming", "borrowing",
];

/// Common standard library types.
pub const SWIFT_TYPES: &[&str] = &[
    "Int", "Int8", "Int16", "Int32", "Int64",
    "UInt", "UInt8", "UInt16", "UInt32", "UInt64",
    "Float", "Double", "Float80", "Bool", "String", "Character", "Substring",
    "Array", "Dictionary", "Set", "Optional", "Result", "Error", "Void",
    "AnyObject", "Task", "URL", "Data", "Date", "UUID", "View",
    "Sequence", "Collection", "Equatable", "Hashable", "Comparable",
    "Codable", "Encodable", "Decodable", "Identifiable", "Sendable",
    "Actor", "MainActor", "GlobalActor", "CustomStringConvertible",
];

/// Multi-character operators in Swift, longest first.
const MULTI_CHAR_OPS: &[&str] = &[
    "...", "..<", "->", "??", "==", "!=", "<=", ">=", "&&", "||", "++", "--",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>",
];

/// Declared capability for the Swift lexical route (FCB-022.15).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SwiftCapabilityV1 {
    pub multiline_strings: bool,
    pub raw_strings: bool,
    pub nested_comments: bool,
    pub string_interpolation: bool,
    pub backtick_identifiers: bool,
    pub attributes: bool,
}

impl SwiftCapabilityV1 {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            multiline_strings: true,
            raw_strings: true,
            nested_comments: true,
            string_interpolation: true,
            backtick_identifiers: true,
            attributes: true,
        }
    }
}

/// Lex Swift source into exact tiling spans.
pub fn lex_swift_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0;
    let bytes = code.as_bytes();

    while pos < len {
        let rest = &code[pos..];
        let c = first_char_at(code, pos);
        let clen = c.len_utf8();

        // 1. Whitespace run.
        if c.is_whitespace() {
            let start = pos;
            while pos < len {
                let b = bytes[pos];
                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                    pos += 1;
                } else if b >= 0x80 && first_char_at(code, pos).is_whitespace() {
                    pos += first_char_at(code, pos).len_utf8();
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

        // 2. Line comment: `//` to end-of-line.
        if rest.starts_with("//") {
            let start = pos;
            pos += 2;
            while pos < len && bytes[pos] != b'\n' {
                pos += 1;
            }
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end: pos,
            });
            continue;
        }

        // 3. Nested block comment: `/* ... */` with nesting counter.
        if rest.starts_with("/*") {
            let start = pos;
            pos += 2;
            let mut depth = 1usize;
            while pos < len && depth > 0 {
                if pos + 1 < len && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
                    depth += 1;
                    pos += 2;
                } else if pos + 1 < len && bytes[pos] == b'*' && bytes[pos + 1] == b'/' {
                    depth -= 1;
                    pos += 2;
                } else {
                    pos += 1;
                }
            }
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end: pos,
            });
            continue;
        }

        // 4. Raw strings: `#"..."#`, `##"..."##`, `#"""..."""#`
        // Count leading `#` before a quote.
        if c == '#' {
            let hash_start = pos;
            let mut hash_count = 0;
            while pos < len && bytes[pos] == b'#' {
                hash_count += 1;
                pos += 1;
            }
            if pos < len && bytes[pos] == b'"' {
                // Raw string! Check if multiline (`"""`) or single (`"`)
                let is_multiline = pos + 2 < len && bytes[pos + 1] == b'"' && bytes[pos + 2] == b'"';
                if is_multiline {
                    pos += 3;
                    // Look for `"""` followed by `hash_count` `#`s
                    while pos < len {
                        if pos + 3 + hash_count <= len
                            && &bytes[pos..pos + 3] == b"\"\"\""
                            && bytes[pos + 3..pos + 3 + hash_count].iter().all(|&b| b == b'#')
                        {
                            pos += 3 + hash_count;
                            break;
                        }
                        pos += 1;
                    }
                } else {
                    pos += 1;
                    // Look for `"` followed by `hash_count` `#`s
                    while pos < len {
                        if bytes[pos] == b'\\' {
                            // Check for raw escape `\#`
                            pos += 1;
                            if pos < len {
                                pos += 1;
                            }
                            continue;
                        }
                        if pos + 1 + hash_count <= len
                            && bytes[pos] == b'"'
                            && bytes[pos + 1..pos + 1 + hash_count].iter().all(|&b| b == b'#')
                        {
                            pos += 1 + hash_count;
                            break;
                        }
                        pos += 1;
                    }
                }
                spans.push(Span {
                    kind: Tok::Str,
                    start: hash_start,
                    end: pos,
                });
                continue;
            } else {
                // Not a raw string. Reset pos to hash_start
                pos = hash_start;
            }
        }

        // 5. Multiline string: `""" ... """`
        if rest.starts_with("\"\"\"") {
            let start = pos;
            pos += 3;
            while pos < len {
                if pos + 2 < len && &bytes[pos..pos + 3] == b"\"\"\"" {
                    pos += 3;
                    break;
                }
                if bytes[pos] == b'\\' {
                    pos += 1;
                    if pos < len {
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            spans.push(Span {
                kind: Tok::Str,
                start,
                end: pos,
            });
            continue;
        }

        // 6. Standard string: `"..."` with escapes and interpolation.
        if c == '"' {
            let start = pos;
            pos += 1;
            while pos < len {
                let b = bytes[pos];
                if b == b'"' {
                    pos += 1;
                    break;
                }
                if b == b'\\' {
                    pos += 1;
                    if pos < len {
                        let next_b = bytes[pos];
                        if next_b == b'(' {
                            // String interpolation \(expr)
                            // We consume balanced parentheses inside the string
                            pos += 1;
                            let mut paren_depth = 1;
                            while pos < len && paren_depth > 0 {
                                if bytes[pos] == b'(' {
                                    paren_depth += 1;
                                } else if bytes[pos] == b')' {
                                    paren_depth -= 1;
                                } else if bytes[pos] == b'"' {
                                    // Nested string inside interpolation!
                                    pos += 1;
                                    while pos < len && bytes[pos] != b'"' {
                                        if bytes[pos] == b'\\' {
                                            pos += 1;
                                        }
                                        pos += 1;
                                    }
                                }
                                pos += 1;
                            }
                            continue;
                        }
                        pos += 1;
                    }
                } else if b == b'\n' {
                    // Swift single-line strings do not span unescaped newlines
                    break;
                } else {
                    pos += 1;
                }
            }
            spans.push(Span {
                kind: Tok::Str,
                start,
                end: pos,
            });
            continue;
        }

        // 7. Backtick-escaped identifier: `` `keyword` ``
        if c == '`' {
            let start = pos;
            pos += 1;
            while pos < len && bytes[pos] != b'`' && bytes[pos] != b'\n' {
                pos += 1;
            }
            if pos < len && bytes[pos] == b'`' {
                pos += 1;
            }
            spans.push(Span {
                kind: Tok::Plain,
                start,
                end: pos,
            });
            continue;
        }

        // 8. Hash directives: `#if`, `#else`, `#endif`, `#available`, etc.
        if c == '#' {
            let start = pos;
            pos += 1;
            let word_start = pos;
            while pos < len && (bytes[pos] == b'_' || bytes[pos].is_ascii_alphanumeric()) {
                pos += 1;
            }
            if pos > word_start {
                spans.push(Span {
                    kind: Tok::Keyword,
                    start,
                    end: pos,
                });
            } else {
                spans.push(Span {
                    kind: Tok::Punct,
                    start,
                    end: pos,
                });
            }
            continue;
        }

        // 9. Attributes: `@attribute` (e.g. `@objc`, `@Published`, `@State`, `@escaping`)
        if c == '@' {
            let start = pos;
            pos += 1;
            let word_start = pos;
            while pos < len && (bytes[pos] == b'_' || bytes[pos].is_ascii_alphanumeric()) {
                pos += 1;
            }
            if pos > word_start {
                spans.push(Span {
                    kind: Tok::Type,
                    start,
                    end: pos,
                });
            } else {
                spans.push(Span {
                    kind: Tok::Punct,
                    start,
                    end: pos,
                });
            }
            continue;
        }

        // 10. Numbers: hex (0x...), binary (0b...), octal (0o...), decimal/float.
        if c.is_ascii_digit()
            || (c == '.' && pos + 1 < len && bytes[pos + 1].is_ascii_digit())
        {
            let start = pos;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                pos += 2;
                while pos < len && (bytes[pos] == b'_' || bytes[pos].is_ascii_hexdigit()) {
                    pos += 1;
                }
                // Optional hex float exponent `p`/`P`
                if pos < len && (bytes[pos] == b'p' || bytes[pos] == b'P') {
                    pos += 1;
                    if pos < len && (bytes[pos] == b'+' || bytes[pos] == b'-') {
                        pos += 1;
                    }
                    while pos < len && (bytes[pos] == b'_' || bytes[pos].is_ascii_digit()) {
                        pos += 1;
                    }
                }
            } else if rest.starts_with("0b") || rest.starts_with("0B") {
                pos += 2;
                while pos < len && (bytes[pos] == b'_' || bytes[pos] == b'0' || bytes[pos] == b'1') {
                    pos += 1;
                }
            } else if rest.starts_with("0o") || rest.starts_with("0O") {
                pos += 2;
                while pos < len && (bytes[pos] == b'_' || (bytes[pos] >= b'0' && bytes[pos] <= b'7')) {
                    pos += 1;
                }
            } else {
                // Decimal or float
                let mut seen_dot = c == '.';
                if seen_dot {
                    pos += 1;
                }
                while pos < len {
                    let b = bytes[pos];
                    if b == b'.' && !seen_dot {
                        if pos + 1 < len && bytes[pos + 1] == b'.' {
                            // Start of range operator `..` or `...`
                            break;
                        }
                        if pos + 1 < len && (bytes[pos + 1].is_ascii_digit() || bytes[pos + 1] == b'e' || bytes[pos + 1] == b'E') {
                            seen_dot = true;
                            pos += 1;
                        } else if pos + 1 == len || !bytes[pos + 1].is_ascii_alphabetic() {
                            // Trailing dot in float literal or at chunk boundary
                            seen_dot = true;
                            pos += 1;
                        } else {
                            break;
                        }
                    } else if b == b'e' || b == b'E' {
                        pos += 1;
                        if pos < len && (bytes[pos] == b'+' || bytes[pos] == b'-') {
                            pos += 1;
                        }
                    } else if b.is_ascii_digit() || b == b'_' {
                        pos += 1;
                    } else {
                        break;
                    }
                }
            }
            spans.push(Span {
                kind: Tok::Number,
                start,
                end: pos,
            });
            continue;
        }

        // 11. Identifiers, keywords, types, calls, and dollar closure arguments.
        if c == '$' || c == '_' || c.is_alphabetic() {
            let start = pos;
            pos += clen;
            while pos < len {
                let ch = first_char_at(code, pos);
                if ch == '_' || ch == '$' || ch.is_alphanumeric() {
                    pos += ch.len_utf8();
                } else {
                    break;
                }
            }
            let word = &code[start..pos];

            let kind = if SWIFT_KEYWORDS.contains(&word) {
                Tok::Keyword
            } else if SWIFT_TYPES.contains(&word) {
                Tok::Type
            } else if is_capitalized_type(word) {
                Tok::Type
            } else if is_function_call(code, pos) {
                Tok::Func
            } else {
                Tok::Plain
            };

            spans.push(Span {
                kind,
                start,
                end: pos,
            });
            continue;
        }

        // 12. Multi-character operators.
        let mut matched_multi = false;
        for &op in MULTI_CHAR_OPS {
            if rest.starts_with(op) {
                spans.push(Span {
                    kind: Tok::Operator,
                    start: pos,
                    end: pos + op.len(),
                });
                pos += op.len();
                matched_multi = true;
                break;
            }
        }
        if matched_multi {
            continue;
        }

        // 13. Single-character punctuation or operator.
        let kind = if is_swift_punct(c) {
            Tok::Punct
        } else if is_swift_operator(c) {
            Tok::Operator
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

/// Helper: returns the first `char` starting at byte offset `pos`.
#[inline(always)]
fn first_char_at(s: &str, pos: usize) -> char {
    s[pos..].chars().next().unwrap_or('\0')
}

/// Heuristic: capitalized identifier followed by lowercase or numbers is a type name.
fn is_capitalized_type(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => chars.all(|c| c.is_alphanumeric() || c == '_'),
        _ => false,
    }
}

/// Check if the identifier is immediately followed by `(` (possibly with whitespace).
fn is_function_call(code: &str, mut pos: usize) -> bool {
    let bytes = code.as_bytes();
    let len = bytes.len();
    while pos < len && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    pos < len && bytes[pos] == b'('
}

/// Swift punctuation characters.
fn is_swift_punct(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ';' | ',' | '.' | ':' | '?' | '!')
}

/// Swift operator characters.
fn is_swift_operator(c: char) -> bool {
    matches!(
        c,
        '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '&' | '|' | '^' | '~'
    )
}
