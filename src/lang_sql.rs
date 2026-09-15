//! Classifies SQL source into byte-exact [`Span`]s that tile the input.
//!
//! Implements the FCB-022.A SQL slice: single-quoted strings with `''`
//! doubling escapes, double-quoted and backtick-quoted identifiers with
//! the same doubling rule, `--` line comments, non-nested `/* */` block
//! comments (unterminated extends to EOF as a provisional trailing span —
//! chunk boundaries are not EOF), `?`/`?N`/`$N`/`:name`/`@name`
//! parameters, decimal/hex numeric literals, a bounded cross-dialect
//! keyword and type core, and call detection.
//!
//! Declared capability limits (the versioned capability row): no nested
//! block comments, no procedural-body grammar, no vendor-specific
//! statement validation — unknown/malformed contexts classify as readable
//! plain source and are never compiler-proven facts.

use crate::highlight::{Span, Tok};

/// Lex SQL source into exact tiling spans.
pub fn lex_sql_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0;
    let bytes = code.as_bytes();

    while pos < len {
        let c = bytes[pos];

        // 1. Whitespace run.
        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
            let start = pos;
            while pos < len {
                let b = bytes[pos];
                if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                    pos += 1;
                } else {
                    break;
                }
            }
            spans.push(Span { kind: Tok::Plain, start, end: pos });
            continue;
        }

        // 2. Line comment: `--` to end of line.
        if c == b'-' && pos + 1 < len && bytes[pos + 1] == b'-' {
            let start = pos;
            let end = bytes[pos..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(len, |offset| pos + offset);
            spans.push(Span { kind: Tok::Comment, start, end });
            pos = end;
            continue;
        }

        // 3. Block comment: `/* ... */` (non-nested — declared capability).
        //    Unterminated extends to EOF as the provisional trailing span.
        if c == b'/' && pos + 1 < len && bytes[pos + 1] == b'*' {
            let start = pos;
            let close = code[pos + 2..].find("*/").map_or(len, |offset| pos + 2 + offset + 2);
            spans.push(Span { kind: Tok::Comment, start, end: close });
            pos = close;
            continue;
        }

        // 4. Single-quoted string with `''` doubling. Unterminated extends
        //    to EOF as the provisional trailing span.
        if c == b'\'' {
            let start = pos;
            let mut p = pos + 1;
            while p < len {
                if bytes[p] == b'\'' {
                    if p + 1 < len && bytes[p + 1] == b'\'' {
                        p += 2; // '' escape: embedded quote, string continues
                        continue;
                    }
                    p += 1; // closing quote
                    break;
                }
                p += 1;
            }
            spans.push(Span { kind: Tok::Str, start, end: p });
            pos = p;
            continue;
        }

        // 5. Double-quoted identifier with `""` doubling (ANSI), or
        //    backtick-quoted identifier (MySQL dialect — declared).
        if c == b'"' || c == b'`' {
            let quote = c;
            let start = pos;
            let mut p = pos + 1;
            while p < len {
                if bytes[p] == quote {
                    if quote == b'"' && p + 1 < len && bytes[p + 1] == b'"' {
                        p += 2;
                        continue;
                    }
                    if quote == b'`' && p + 1 < len && bytes[p + 1] == b'`' {
                        p += 2;
                        continue;
                    }
                    p += 1;
                    break;
                }
                p += 1;
            }
            spans.push(Span { kind: Tok::Str, start, end: p });
            pos = p;
            continue;
        }

        // 6. Parameters: `?`, `?N`, `$N`, `:name`, `@name`.
        if c == b'?' {
            let start = pos;
            let mut p = pos + 1;
            while p < len && bytes[p].is_ascii_digit() {
                p += 1;
            }
            spans.push(Span { kind: Tok::Operator, start, end: p });
            pos = p;
            continue;
        }
        if c == b'$' || c == b':' || c == b'@' {
            let start = pos;
            let mut p = pos + 1;
            while p < len {
                let b = bytes[p];
                if b.is_ascii_alphanumeric() || b == b'_' {
                    p += 1;
                } else {
                    break;
                }
            }
            if p > start + 1 {
                spans.push(Span { kind: Tok::Operator, start, end: p });
                pos = p;
                continue;
            }
            // A lone `$`/`:`/`@` falls through to operator handling.
            spans.push(Span { kind: Tok::Operator, start, end: start + 1 });
            pos = start + 1;
            continue;
        }

        // 7. Numbers: optional hex prefix, then digits/dot/exponent run.
        if c.is_ascii_digit() || (c == b'.' && pos + 1 < len && bytes[pos + 1].is_ascii_digit()) {
            let start = pos;
            if c == b'0' && pos + 1 < len && (bytes[pos + 1] == b'x' || bytes[pos + 1] == b'X') {
                pos += 2;
                while pos < len && bytes[pos].is_ascii_hexdigit() {
                    pos += 1;
                }
            } else {
                while pos < len {
                    let b = bytes[pos];
                    if b.is_ascii_digit() || b == b'.' {
                        pos += 1;
                    } else if b == b'e' || b == b'E' {
                        if pos + 1 < len && (bytes[pos + 1] == b'+' || bytes[pos + 1] == b'-') {
                            if pos + 2 < len {
                                if bytes[pos + 2].is_ascii_digit() {
                                    pos += 2;
                                    while pos < len && bytes[pos].is_ascii_digit() {
                                        pos += 1;
                                    }
                                } else {
                                    break;
                                }
                            } else {
                                pos += 2;
                                break;
                            }
                        } else if pos + 1 < len {
                            if bytes[pos + 1].is_ascii_digit() {
                                pos += 1;
                                while pos < len && bytes[pos].is_ascii_digit() {
                                    pos += 1;
                                }
                            } else {
                                break;
                            }
                        } else {
                            pos += 1;
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            spans.push(Span { kind: Tok::Number, start, end: pos });
            continue;
        }

        // 8. Word: identifier / keyword / type / call.
        if c == b'_' || c.is_ascii_alphabetic() {
            let start = pos;
            while pos < len {
                let b = bytes[pos];
                if b == b'_' || b.is_ascii_alphanumeric() || b >= 0x80 {
                    pos += 1;
                } else {
                    break;
                }
            }
            let word = &code[start..pos];
            let next_is_paren = pos < len && bytes[pos] == b'(';
            let upper = word.to_ascii_uppercase();
            let kind = if next_is_paren {
                Tok::Func
            } else if SQL_KEYWORDS.contains(&upper.as_str()) {
                Tok::Keyword
            } else if SQL_TYPES.contains(&upper.as_str()) {
                Tok::Type
            } else {
                Tok::Plain
            };
            spans.push(Span { kind, start, end: pos });
            continue;
        }

        // 9. Punctuation.
        if c == b'(' || c == b')' || c == b'[' || c == b']' || c == b',' || c == b';' || c == b'.' {
            spans.push(Span { kind: Tok::Punct, start: pos, end: pos + 1 });
            pos += 1;
            continue;
        }

        // 10. Operators.
        spans.push(Span { kind: Tok::Operator, start: pos, end: pos + 1 });
        pos += 1;
    }
}

/// Bounded cross-dialect keyword core (upper-cased at comparison time).
static SQL_KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "AND", "OR", "NOT", "NULL", "TRUE", "FALSE", "INSERT", "INTO",
    "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "VIEW", "INDEX", "DROP", "ALTER",
    "JOIN", "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "CROSS", "ON", "GROUP", "BY", "ORDER",
    "HAVING", "LIMIT", "OFFSET", "UNION", "ALL", "DISTINCT", "AS", "IN", "EXISTS", "BETWEEN",
    "LIKE", "ILIKE", "IS", "CASE", "WHEN", "THEN", "ELSE", "END", "WITH", "RECURSIVE",
    "RETURNING", "PRIMARY", "FOREIGN", "KEY", "REFERENCES", "DEFAULT", "UNIQUE", "CHECK",
    "CONSTRAINT", "CASCADE", "ASC", "DESC", "USING", "NATURAL",
];

/// Bounded cross-dialect type core (upper-cased at comparison time).
static SQL_TYPES: &[&str] = &[
    "INT", "INTEGER", "BIGINT", "SMALLINT", "DECIMAL", "NUMERIC", "REAL", "FLOAT", "DOUBLE",
    "CHAR", "VARCHAR", "TEXT", "BOOLEAN", "BOOL", "DATE", "TIME", "TIMESTAMP", "JSON", "JSONB",
    "UUID", "SERIAL", "BIGSERIAL", "BLOB", "CLOB", "BYTEA",
];

/// The versioned SQL capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqlCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for SQL.
    pub incremental: bool,
    /// Quoted identifiers, doubling escapes, comments and parameters supported.
    pub quotes_comments_params: bool,
}

/// The SQL capability row published by this module.
pub const SQL_CAPABILITY_V1: SqlCapabilityV1 = SqlCapabilityV1 {
    version: 1,
    incremental: true,
    quotes_comments_params: true,
};
