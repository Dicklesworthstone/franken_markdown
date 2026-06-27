//! Clean-room, dependency-free syntax highlighter for fenced code blocks.
//!
//! A single small generic lexer driven by per-language token rules (keywords,
//! types, comment markers, string delimiters). This deliberately does NOT pull
//! in a Sublime-grammar engine (syntect) or tree-sitter (C) — for the languages
//! that actually appear in technical Markdown, a focused tokenizer gives a
//! beautiful, fast, zero-dependency result. Unknown languages fall back to plain
//! (escaped) text. The output is byte-offset spans into the original code, so
//! the HTML emitter can slice + escape safely.

/// A highlight token class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    /// Unclassified text (rendered without a span).
    Plain,
    /// Language keyword.
    Keyword,
    /// Type / class name.
    Type,
    /// A name immediately followed by `(` (call/definition).
    Func,
    /// String / char literal.
    Str,
    /// Numeric literal.
    Number,
    /// Comment.
    Comment,
    /// Operator (`+ - * / = < > ...`).
    Operator,
    /// Punctuation (`( ) [ ] { } , ; .`).
    Punct,
}

impl Tok {
    /// CSS class for this token, or `None` for plain text (no span).
    #[must_use]
    pub fn css_class(self) -> Option<&'static str> {
        Some(match self {
            Self::Plain => return None,
            Self::Keyword => "tok-kw",
            Self::Type => "tok-ty",
            Self::Func => "tok-fn",
            Self::Str => "tok-st",
            Self::Number => "tok-nu",
            Self::Comment => "tok-cm",
            Self::Operator => "tok-op",
            Self::Punct => "tok-pn",
        })
    }
}

/// A classified byte range `[start, end)` into the highlighted source.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    /// Token class.
    pub kind: Tok,
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

struct Rules {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    strings: &'static [char],
}

/// True when a focused lexer exists for `lang`.
#[must_use]
pub fn is_supported(lang: &str) -> bool {
    rules(lang).is_some()
}

/// Highlight `code` for language `lang`, returning classified byte spans that
/// exactly tile `code`. Unknown languages yield a single `Plain` span.
#[must_use]
pub fn highlight(lang: &str, code: &str) -> Vec<Span> {
    match rules(lang) {
        Some(r) => lex(code, &r),
        None => vec![Span {
            kind: Tok::Plain,
            start: 0,
            end: code.len(),
        }],
    }
}

fn lex(code: &str, r: &Rules) -> Vec<Span> {
    let mut spans = Vec::new();
    let len = code.len();
    let mut pos = 0;
    while pos < len {
        let rest = &code[pos..];
        let Some(c) = rest.chars().next() else { break };
        let clen = c.len_utf8();

        // Whitespace run.
        if c.is_whitespace() {
            let start = pos;
            while pos < len {
                match code[pos..].chars().next() {
                    Some(w) if w.is_whitespace() => pos += w.len_utf8(),
                    _ => break,
                }
            }
            spans.push(Span {
                kind: Tok::Plain,
                start,
                end: pos,
            });
            continue;
        }

        // Line comment.
        if let Some(lc) = r.line_comments.iter().find(|lc| rest.starts_with(**lc)) {
            let _ = lc;
            let start = pos;
            let end = rest.find('\n').map_or(len, |x| pos + x);
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // Block comment.
        if let Some((open, close)) = r.block_comment
            && rest.starts_with(open)
        {
            let start = pos;
            let after = pos + open.len();
            let end = code[after..]
                .find(close)
                .map_or(len, |x| after + x + close.len());
            spans.push(Span {
                kind: Tok::Comment,
                start,
                end,
            });
            pos = end;
            continue;
        }

        // String / char literal (with backslash escapes).
        if r.strings.contains(&c) {
            let quote = c;
            let start = pos;
            let mut p = pos + clen;
            while p < len {
                let Some(ch) = code[p..].chars().next() else {
                    break;
                };
                let l2 = ch.len_utf8();
                if ch == '\\' {
                    let nx = code[p + l2..].chars().next().map_or(0, char::len_utf8);
                    p += l2 + nx;
                    continue;
                }
                p += l2;
                if ch == quote {
                    break;
                }
            }
            spans.push(Span {
                kind: Tok::Str,
                start,
                end: p,
            });
            pos = p;
            continue;
        }

        // Number literal.
        if c.is_ascii_digit() {
            let start = pos;
            let mut p = pos;
            while p < len {
                let Some(ch) = code[p..].chars().next() else {
                    break;
                };
                if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
                    p += ch.len_utf8();
                } else {
                    break;
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

        // Identifier / keyword / type / function.
        if c == '_' || c.is_alphabetic() {
            let start = pos;
            let mut p = pos;
            while p < len {
                let Some(ch) = code[p..].chars().next() else {
                    break;
                };
                if ch == '_' || ch.is_alphanumeric() {
                    p += ch.len_utf8();
                } else {
                    break;
                }
            }
            let word = &code[start..p];
            let kind = if r.keywords.contains(&word) {
                Tok::Keyword
            } else if r.types.contains(&word)
                || (!r.types.is_empty()
                    && word
                        .chars()
                        .next()
                        .is_some_and(|c0| c0.is_ascii_uppercase()))
            {
                Tok::Type
            } else if code[p..].trim_start().starts_with('(') {
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

        // Operator vs punctuation vs other.
        let kind = if "+-*/%=<>!&|^~?:@".contains(c) {
            Tok::Operator
        } else if "()[]{},;.".contains(c) {
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
    spans
}

fn rules(lang: &str) -> Option<Rules> {
    let l = lang.trim().to_ascii_lowercase();
    let r = match l.as_str() {
        "rust" | "rs" => Rules {
            keywords: RUST_KW,
            types: RUST_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"'],
        },
        "python" | "py" => Rules {
            keywords: PY_KW,
            types: PY_TY,
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
        },
        "javascript" | "js" | "jsx" | "mjs" | "cjs" | "typescript" | "ts" | "tsx" => Rules {
            keywords: JS_KW,
            types: JS_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\'', '`'],
        },
        "json" | "jsonc" => Rules {
            keywords: &["true", "false", "null"],
            types: &[],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"'],
        },
        "bash" | "sh" | "shell" | "zsh" | "console" => Rules {
            keywords: SH_KW,
            types: &[],
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
        },
        "go" | "golang" => Rules {
            keywords: GO_KW,
            types: GO_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '`'],
        },
        "c" | "h" | "cpp" | "c++" | "cc" | "hpp" => Rules {
            keywords: C_KW,
            types: C_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
        },
        "toml" | "ini" => Rules {
            keywords: &["true", "false"],
            types: &[],
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
        },
        "yaml" | "yml" => Rules {
            keywords: &["true", "false", "null", "yes", "no"],
            types: &[],
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
        },
        "sql" => Rules {
            keywords: SQL_KW,
            types: SQL_TY,
            line_comments: &["--"],
            block_comment: Some(("/*", "*/")),
            strings: &['\'', '"'],
        },
        _ => return None,
    };
    Some(r)
}

const RUST_KW: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "union",
    "unsafe", "use", "where", "while",
];
const RUST_TY: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8", "u16",
    "u32", "u64", "u128", "usize", "String", "Vec", "Option", "Result", "Box", "Rc", "Arc",
];
const PY_KW: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "False", "try", "while",
    "with", "yield", "self",
];
const PY_TY: &[&str] = &[
    "int", "float", "str", "bool", "list", "dict", "set", "tuple", "bytes",
];
const JS_KW: &[&str] = &[
    "as",
    "async",
    "await",
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
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "of",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "false",
    "try",
    "type",
    "typeof",
    "var",
    "void",
    "while",
    "yield",
];
const JS_TY: &[&str] = &[
    "string", "number", "boolean", "object", "any", "unknown", "never", "void",
];
const SH_KW: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in",
    "local", "read", "return", "select", "then", "until", "while", "echo", "set", "unset",
    "source",
];
const GO_KW: &[&str] = &[
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
    "nil",
    "true",
    "false",
];
const GO_TY: &[&str] = &[
    "bool", "byte", "rune", "string", "int", "int8", "int16", "int32", "int64", "uint", "uint8",
    "uint16", "uint32", "uint64", "float32", "float64", "error",
];
const C_KW: &[&str] = &[
    "auto",
    "break",
    "case",
    "const",
    "continue",
    "default",
    "do",
    "else",
    "enum",
    "extern",
    "for",
    "goto",
    "if",
    "inline",
    "register",
    "return",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "volatile",
    "while",
    "class",
    "namespace",
    "template",
    "public",
    "private",
    "protected",
    "virtual",
    "new",
    "delete",
    "nullptr",
    "true",
    "false",
];
const C_TY: &[&str] = &[
    "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void",
    "size_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t",
];
const SQL_KW: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "INSERT",
    "INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE",
    "CREATE",
    "TABLE",
    "DROP",
    "ALTER",
    "JOIN",
    "LEFT",
    "RIGHT",
    "INNER",
    "OUTER",
    "ON",
    "GROUP",
    "BY",
    "ORDER",
    "HAVING",
    "LIMIT",
    "AS",
    "AND",
    "OR",
    "NOT",
    "NULL",
    "IN",
    "IS",
    "DISTINCT",
    "UNION",
    "PRIMARY",
    "KEY",
    "FOREIGN",
    "REFERENCES",
    "INDEX",
    "DEFAULT",
    "select",
    "from",
    "where",
    "insert",
    "into",
    "values",
    "update",
    "set",
    "delete",
    "create",
    "table",
    "join",
    "on",
];
const SQL_TY: &[&str] = &[
    "INT",
    "INTEGER",
    "BIGINT",
    "TEXT",
    "VARCHAR",
    "CHAR",
    "BOOLEAN",
    "DATE",
    "TIMESTAMP",
    "REAL",
    "FLOAT",
    "DECIMAL",
    "SERIAL",
    "BLOB",
];
