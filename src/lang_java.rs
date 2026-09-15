#![forbid(unsafe_code)]

//! Java incremental lexer (FCB-022 · fcb-9vx.14).
//!
//! Classifies Java source into byte-exact [`Span`]s that tile the input.
//! Implements:
//! - Line comments (`//`) and non-nesting block comments (`/* */`, `/** */`).
//! - Text blocks (`""" ... """`) with multiline spans and escape handling.
//! - Standard strings (`"..."`) with escapes (`\n`, `\"`, `\\`, unicode `\uXXXX`).
//! - Character literals (`'c'`, `'\''`, `'\\'`, `'\n'`).
//! - Annotations (`@Override`, `@SuppressWarnings`, etc.).
//! - Numbers: hex (`0xCAFE_BABE`), binary (`0b1010`), octal, floats, doubles,
//!   scientific notation, and suffixes (`L`, `l`, `F`, `f`, `D`, `d`).
//! - Keywords, contextual keywords (`record`, `sealed`, `var`, `yield`),
//!   standard library types, capitalized type conventions, and method calls.
//! - Exact source byte tiling across whole inputs and arbitrary splits.

use crate::highlight::{Span, Tok};

/// Java keywords, contextual keywords, and literal values.
pub const JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "void",
    "volatile",
    "while",
    // Contextual keywords (Java 10+)
    "record",
    "sealed",
    "non-sealed",
    "permits",
    "yield",
    "var",
    // Literals highlighted as keywords
    "true",
    "false",
    "null",
];

/// Common standard library types and primitive wrapper types.
pub const JAVA_TYPES: &[&str] = &[
    "boolean",
    "byte",
    "char",
    "short",
    "int",
    "long",
    "float",
    "double",
    "void",
    "Boolean",
    "Byte",
    "Character",
    "Short",
    "Integer",
    "Long",
    "Float",
    "Double",
    "String",
    "Object",
    "Class",
    "System",
    "Math",
    "Number",
    "CharSequence",
    "List",
    "ArrayList",
    "LinkedList",
    "Map",
    "HashMap",
    "TreeMap",
    "Set",
    "HashSet",
    "TreeSet",
    "Collection",
    "Collections",
    "Arrays",
    "Optional",
    "Stream",
    "Iterator",
    "Iterable",
    "Comparable",
    "Comparator",
    "Runnable",
    "Callable",
    "Future",
    "CompletableFuture",
    "Thread",
    "ThreadGroup",
    "Throwable",
    "Exception",
    "RuntimeException",
    "Error",
    "StringBuilder",
    "StringBuffer",
    "Scanner",
    "File",
    "Path",
    "Paths",
    "Files",
    "InputStream",
    "OutputStream",
    "Reader",
    "Writer",
    "PrintStream",
    "PrintWriter",
];

/// Multi-character operators in Java, longest first.
const MULTI_CHAR_OPS: &[&str] = &[
    ">>>=", ">>>", ">>=", "<<=", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "<<", ">>", "+=",
    "-=", "*=", "/=", "%=", "&=", "|=", "^=", "->", "::", "...",
];

/// Declared capability for the Java lexical route (FCB-022.14).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JavaCapabilityV1 {
    pub text_blocks: bool,
    pub unicode_escapes: bool,
    pub annotations: bool,
    pub method_references: bool,
    pub pattern_matching_switch: bool,
    pub records: bool,
}

impl JavaCapabilityV1 {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            text_blocks: true,
            unicode_escapes: true,
            annotations: true,
            method_references: true,
            pattern_matching_switch: true,
            records: true,
        }
    }
}

/// Lex Java source into exact tiling spans.
pub fn lex_java_into(code: &str, spans: &mut Vec<Span>) {
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
                if b.is_ascii_whitespace() || b == 0x0b {
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

        // 2. Line comment: // ...
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

        // 3. Block comment: /* ... */ (non-nesting in Java).
        if rest.starts_with("/*") {
            let start = pos;
            let mut p = pos + 2;
            while p < len {
                if p + 1 < len && bytes[p] == b'*' && bytes[p + 1] == b'/' {
                    p += 2;
                    break;
                }
                p += 1;
            }
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end: p,
            });
            pos = p;
            continue;
        }

        // 4. Text blocks: """ ... """ (Java 15+).
        if rest.starts_with("\"\"\"") {
            let start = pos;
            let mut p = pos + 3;

            while p < len {
                if bytes[p] == b'\\' {
                    p += 1;
                    if p < len {
                        p += first_char_at(code, p).len_utf8();
                    }
                    continue;
                }
                if p + 2 < len && bytes[p] == b'"' && bytes[p + 1] == b'"' && bytes[p + 2] == b'"' {
                    p += 3;

                    break;
                }
                p += 1;
            }
            let end = p.min(len);
            spans.push(Span {
                kind: Tok::Str,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // 5. Standard strings: "..."
        if c == '"' {
            let start = pos;
            let mut p = pos + 1;

            while p < len {
                let b = bytes[p];
                if b == b'\\' {
                    p += 1;
                    if p < len {
                        p += first_char_at(code, p).len_utf8();
                    }
                    continue;
                }
                if b == b'"' {
                    p += 1;

                    break;
                }
                if b == b'\n' || b == b'\r' {
                    // Java standard strings cannot span unescaped newlines.
                    break;
                }
                p += 1;
            }
            let end = p;
            spans.push(Span {
                kind: Tok::Str,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // 6. Character literals: 'c', '\'', etc.
        if c == '\'' {
            let start = pos;
            let mut p = pos + 1;

            while p < len {
                let b = bytes[p];
                if b == b'\\' {
                    p += 1;
                    if p < len {
                        p += first_char_at(code, p).len_utf8();
                    }
                    continue;
                }
                if b == b'\'' {
                    p += 1;

                    break;
                }
                if b == b'\n' || b == b'\r' {
                    break;
                }
                p += 1;
            }
            let end = p;
            spans.push(Span {
                kind: Tok::Str,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // 7. Numbers: hex, binary, octal, float, decimal integers.
        if c.is_ascii_digit() || (c == '.' && pos + 1 < len && bytes[pos + 1].is_ascii_digit()) {
            let start = pos;
            let mut p = pos;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                p += 2;
                while p < len && (bytes[p].is_ascii_hexdigit() || bytes[p] == b'_') {
                    p += 1;
                }
                // Hex float exponent: p/P
                if p < len && (bytes[p] == b'p' || bytes[p] == b'P') {
                    p += 1;
                    if p < len && (bytes[p] == b'+' || bytes[p] == b'-') {
                        p += 1;
                    }
                    while p < len && (bytes[p].is_ascii_digit() || bytes[p] == b'_') {
                        p += 1;
                    }
                }
            } else if rest.starts_with("0b") || rest.starts_with("0B") {
                p += 2;
                while p < len && (bytes[p] == b'0' || bytes[p] == b'1' || bytes[p] == b'_') {
                    p += 1;
                }
            } else {
                let mut seen_dot = c == '.';
                if seen_dot {
                    p += 1;
                }
                while p < len {
                    let b = bytes[p];
                    if b == b'.' && !seen_dot {
                        if p + 1 < len
                            && (bytes[p + 1].is_ascii_digit()
                                || bytes[p + 1] == b'e'
                                || bytes[p + 1] == b'E')
                        {
                            seen_dot = true;
                            p += 1;
                        } else if p + 1 == len || !bytes[p + 1].is_ascii_alphabetic() {
                            // trailing dot in float literal e.g. 1.
                            seen_dot = true;
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
            }
            // Optional suffixes: l/L (long), f/F (float), d/D (double)
            while p < len && matches!(bytes[p], b'l' | b'L' | b'f' | b'F' | b'd' | b'D') {
                p += 1;
            }
            spans.push(Span {
                kind: Tok::Number,
                start,
                end: p,
            });
            pos = p;
            continue;
        }

        // 8. Annotations: @Annotation or @interface
        if c == '@' {
            let start = pos;
            pos += 1;
            // Scan subsequent whitespace or identifier
            let after_at = pos;
            while pos < len
                && (bytes[pos] == b'_' || bytes[pos] == b'$' || bytes[pos].is_ascii_alphanumeric())
            {
                pos += 1;
            }
            if pos > after_at {
                let word = &code[after_at..pos];
                if word == "interface" {
                    // @interface declaration
                    spans.push(Span {
                        kind: Tok::Punct,
                        start,
                        end: after_at,
                    });
                    spans.push(Span {
                        kind: Tok::Keyword,
                        start: after_at,
                        end: pos,
                    });
                } else {
                    // @Annotation
                    spans.push(Span {
                        kind: Tok::Type,
                        start,
                        end: pos,
                    });
                }
                continue;
            } else {
                // Isolated @
                spans.push(Span {
                    kind: Tok::Punct,
                    start,
                    end: pos,
                });
                continue;
            }
        }

        // 9. Identifier / Keyword / Type / Function call.
        if c == '_' || c == '$' || c.is_alphabetic() {
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
            let is_keyword = JAVA_KEYWORDS.contains(&word);
            let kind = if is_keyword {
                Tok::Keyword
            } else if JAVA_TYPES.contains(&word) {
                Tok::Type
            } else if next_byte_after_whitespace(code, pos) == Some(b'(') {
                Tok::Func
            } else if is_capitalized_type(word) {
                Tok::Type
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

        // 10. Multi-character operators.
        let mut matched_op = false;
        for &op in MULTI_CHAR_OPS {
            if rest.starts_with(op) {
                let kind = if op == "::" || op == "..." {
                    Tok::Punct
                } else {
                    Tok::Operator
                };
                spans.push(Span {
                    kind,
                    start: pos,
                    end: pos + op.len(),
                });
                pos += op.len();
                matched_op = true;
                break;
            }
        }
        if matched_op {
            continue;
        }

        // 11. Single-character operators and punctuation.
        let kind = if is_java_punct(c) {
            Tok::Punct
        } else if is_java_op(c) {
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

fn is_java_punct(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | ';' | ',' | '.' | '@')
}

fn is_java_op(c: char) -> bool {
    matches!(
        c,
        '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^' | '!' | '~' | '<' | '>' | '=' | '?' | ':'
    )
}

fn is_capitalized_type(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) if first.is_uppercase() => {
            // Not ALL-CAPS (which are constants)
            word.chars().any(|ch| ch.is_lowercase())
        }
        _ => false,
    }
}

fn first_char_at(code: &str, pos: usize) -> char {
    code[pos..].chars().next().unwrap_or('\0')
}

fn next_byte_after_whitespace(code: &str, mut pos: usize) -> Option<u8> {
    let bytes = code.as_bytes();
    while pos < bytes.len() {
        let b = bytes[pos];
        if b == b' ' || b == b'\t' || b == b'\r' || b == b'\n' {
            pos += 1;
        } else {
            return Some(b);
        }
    }
    None
}
