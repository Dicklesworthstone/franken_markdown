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

#[derive(Clone, Copy)]
struct Rules {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    strings: &'static [char],
    /// Case-insensitive keyword/type matching (e.g. SQL, where `select`,
    /// `SELECT`, and `Select` are all the keyword).
    case_insensitive: bool,
    /// A `#` at the start of a line begins a preprocessor directive whose first
    /// word is a keyword (C/C++ `#include`, `#define`, ...).
    hash_directives: bool,
}

#[derive(Clone, Copy)]
enum Lexer {
    Generic(Rules),
    Html,
    Css,
    Markdown,
}

/// True when a focused lexer exists for `lang`.
#[must_use]
pub fn is_supported(lang: &str) -> bool {
    lexer(lang).is_some()
}

/// Highlight `code` for language `lang`, returning classified byte spans that
/// exactly tile `code`. Unknown languages yield a single `Plain` span.
#[must_use]
pub fn highlight(lang: &str, code: &str) -> Vec<Span> {
    match lexer(lang) {
        Some(Lexer::Generic(r)) => lex_generic(code, &r),
        Some(Lexer::Html) => lex_html(code),
        Some(Lexer::Css) => lex_css(code),
        Some(Lexer::Markdown) => lex_markdown(code),
        None => vec![Span {
            kind: Tok::Plain,
            start: 0,
            end: code.len(),
        }],
    }
}

fn lex_generic(code: &str, r: &Rules) -> Vec<Span> {
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

        // C/C++ preprocessor directive: `#word` at the start of a line (only
        // whitespace before it). Colours `#include`, `#define`, ... as keywords.
        if r.hash_directives && c == '#' {
            let line_prefix = code[..pos].rsplit('\n').next().unwrap_or("");
            if line_prefix.trim().is_empty() {
                let start = pos;
                let mut p = pos + clen;
                while p < len
                    && code[p..]
                        .chars()
                        .next()
                        .is_some_and(|ch| ch == ' ' || ch == '\t')
                {
                    p += 1;
                }
                while p < len {
                    match code[p..].chars().next() {
                        Some(ch) if ch.is_ascii_alphabetic() => p += ch.len_utf8(),
                        _ => break,
                    }
                }
                spans.push(Span {
                    kind: Tok::Keyword,
                    start,
                    end: p,
                });
                pos = p;
                continue;
            }
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
            let is_keyword = if r.case_insensitive {
                r.keywords.iter().any(|k| k.eq_ignore_ascii_case(word))
            } else {
                r.keywords.contains(&word)
            };
            let is_type = if r.case_insensitive {
                r.types.iter().any(|k| k.eq_ignore_ascii_case(word))
            } else {
                r.types.contains(&word)
            };
            let next_is_paren = code[p..].trim_start().starts_with('(');
            // ALL_CAPS identifiers are conventionally constants, not types, so the
            // "Capitalized => Type" heuristic must skip them.
            let mut letters = word.chars().filter(|c| c.is_alphabetic());
            let all_caps = word.chars().any(|c| c.is_ascii_uppercase())
                && letters.all(|c| c.is_ascii_uppercase());
            let first_upper = word.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            let kind = if is_keyword {
                Tok::Keyword
            } else if is_type {
                Tok::Type
            } else if next_is_paren {
                // A call/definition wins over the capitalization heuristic, so
                // `Println(` is a function, not a type.
                Tok::Func
            } else if !r.types.is_empty() && first_upper && !all_caps {
                Tok::Type
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

fn lex_html(code: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let len = code.len();
    let mut pos = 0usize;

    while pos < len {
        let rest = &code[pos..];

        if rest.starts_with("<!--") {
            let end = find_after(code, pos + 4, "-->").unwrap_or(len);
            push_span(&mut spans, Tok::Comment, pos, end);
            pos = end;
            continue;
        }

        if rest.starts_with('<') {
            if !looks_like_html_tag(rest) {
                push_span(&mut spans, Tok::Operator, pos, pos + 1);
                pos += 1;
                continue;
            }

            push_span(&mut spans, Tok::Operator, pos, pos + 1);
            pos += 1;

            if code[pos..].starts_with('/') {
                push_span(&mut spans, Tok::Operator, pos, pos + 1);
                pos += 1;
            }

            while pos < len {
                let Some(ch) = code[pos..].chars().next() else {
                    break;
                };
                let clen = ch.len_utf8();

                if ch.is_whitespace() {
                    let start = pos;
                    pos = consume_while(code, pos, char::is_whitespace);
                    push_span(&mut spans, Tok::Plain, start, pos);
                } else if code[pos..].starts_with("/>") {
                    push_span(&mut spans, Tok::Operator, pos, pos + 2);
                    pos += 2;
                    break;
                } else if ch == '>' {
                    push_span(&mut spans, Tok::Operator, pos, pos + clen);
                    pos += clen;
                    break;
                } else if ch == '"' || ch == '\'' {
                    let end = consume_quoted(code, pos, ch);
                    push_span(&mut spans, Tok::Str, pos, end);
                    pos = end;
                } else if ch == '=' {
                    push_span(&mut spans, Tok::Operator, pos, pos + clen);
                    pos += clen;
                } else if is_html_name_char(ch) {
                    let start = pos;
                    pos = consume_while(code, pos, is_html_name_char);
                    let kind = if previous_non_space_is_tag_open(code, start) {
                        Tok::Keyword
                    } else {
                        Tok::Type
                    };
                    push_span(&mut spans, kind, start, pos);
                } else {
                    push_span(&mut spans, Tok::Punct, pos, pos + clen);
                    pos += clen;
                }
            }
            continue;
        }

        let next_tag = rest.find('<').map_or(len, |off| pos + off);
        push_span(&mut spans, Tok::Plain, pos, next_tag);
        pos = next_tag;
    }

    spans
}

fn lex_css(code: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let len = code.len();
    let mut pos = 0usize;

    while pos < len {
        let rest = &code[pos..];
        let Some(ch) = rest.chars().next() else {
            break;
        };
        let clen = ch.len_utf8();

        if ch.is_whitespace() {
            let start = pos;
            pos = consume_while(code, pos, char::is_whitespace);
            push_span(&mut spans, Tok::Plain, start, pos);
        } else if rest.starts_with("/*") {
            let end = find_after(code, pos + 2, "*/").unwrap_or(len);
            push_span(&mut spans, Tok::Comment, pos, end);
            pos = end;
        } else if ch == '"' || ch == '\'' {
            let end = consume_quoted(code, pos, ch);
            push_span(&mut spans, Tok::Str, pos, end);
            pos = end;
        } else if ch == '@' {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, is_css_ident_char);
            push_span(&mut spans, Tok::Keyword, start, pos);
        } else if ch == '#' && starts_hex_color(code, pos) {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, |c| c.is_ascii_hexdigit());
            push_span(&mut spans, Tok::Number, start, pos);
        } else if ch.is_ascii_digit() {
            let start = pos;
            pos = consume_while(code, pos, is_css_number_char);
            push_span(&mut spans, Tok::Number, start, pos);
        } else if is_css_ident_start(ch) {
            let start = pos;
            pos = consume_while(code, pos, is_css_ident_char);
            let word = &code[start..pos];
            let kind = if CSS_KW.contains(&word) {
                Tok::Keyword
            } else if next_non_space_is(code, pos, ':')
                || previous_non_space_is_selector(code, start)
            {
                Tok::Type
            } else {
                Tok::Plain
            };
            push_span(&mut spans, kind, start, pos);
        } else {
            let kind = if ":;{}(),[]".contains(ch) {
                Tok::Punct
            } else if ".>+~*=|^$!".contains(ch) {
                Tok::Operator
            } else {
                Tok::Plain
            };
            push_span(&mut spans, kind, pos, pos + clen);
            pos += clen;
        }
    }

    spans
}

fn lex_markdown(code: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let len = code.len();
    let mut pos = 0usize;
    let mut line_start = true;

    while pos < len {
        let rest = &code[pos..];

        if rest.starts_with("<!--") {
            let end = find_after(code, pos + 4, "-->").unwrap_or(len);
            push_span(&mut spans, Tok::Comment, pos, end);
            line_start = end > 0 && code.as_bytes().get(end - 1).is_some_and(|b| *b == b'\n');
            pos = end;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        let clen = ch.len_utf8();

        if ch == '\n' {
            push_span(&mut spans, Tok::Plain, pos, pos + clen);
            pos += clen;
            line_start = true;
        } else if line_start && ch.is_whitespace() {
            let start = pos;
            pos = consume_while(code, pos, |c| c != '\n' && c.is_whitespace());
            push_span(&mut spans, Tok::Plain, start, pos);
        } else if line_start && ch == '#' {
            let start = pos;
            pos = consume_while(code, pos, |c| c == '#');
            push_span(&mut spans, Tok::Keyword, start, pos);
            line_start = false;
        } else if line_start && ch == '>' {
            push_span(&mut spans, Tok::Operator, pos, pos + clen);
            pos += clen;
            line_start = false;
        } else if line_start && is_list_marker(rest) {
            let end = list_marker_end(code, pos);
            push_span(&mut spans, Tok::Operator, pos, end);
            pos = end;
            line_start = false;
        } else if ch == '`' {
            let end = consume_markdown_code(code, pos);
            push_span(&mut spans, Tok::Str, pos, end);
            line_start = false;
            pos = end;
        } else if ch.is_ascii_digit() {
            let start = pos;
            pos = consume_while(code, pos, |c| c.is_ascii_digit());
            push_span(&mut spans, Tok::Number, start, pos);
            line_start = false;
        } else if "*_~".contains(ch) {
            let start = pos;
            pos = consume_while(code, pos, |c| c == ch);
            push_span(&mut spans, Tok::Operator, start, pos);
            line_start = false;
        } else if "[]()!|:".contains(ch) {
            push_span(&mut spans, Tok::Punct, pos, pos + clen);
            pos += clen;
            line_start = false;
        } else {
            let start = pos;
            pos = consume_markdown_plain(code, pos);
            push_span(&mut spans, Tok::Plain, start, pos);
            line_start = false;
        }
    }

    spans
}

fn push_span(spans: &mut Vec<Span>, kind: Tok, start: usize, end: usize) {
    if start >= end {
        return;
    }
    if let Some(prev) = spans.last_mut()
        && prev.kind == kind
        && prev.end == start
    {
        prev.end = end;
        return;
    }
    spans.push(Span { kind, start, end });
}

fn consume_while(code: &str, mut pos: usize, pred: impl Fn(char) -> bool) -> usize {
    while pos < code.len() {
        let Some(ch) = code[pos..].chars().next() else {
            break;
        };
        if pred(ch) {
            pos += ch.len_utf8();
        } else {
            break;
        }
    }
    pos
}

fn find_after(code: &str, from: usize, needle: &str) -> Option<usize> {
    code.get(from..)
        .and_then(|rest| rest.find(needle).map(|off| from + off + needle.len()))
}

fn consume_quoted(code: &str, pos: usize, quote: char) -> usize {
    let mut p = pos + quote.len_utf8();
    while p < code.len() {
        let Some(ch) = code[p..].chars().next() else {
            break;
        };
        let len = ch.len_utf8();
        if ch == '\\' {
            let escaped = code[p + len..].chars().next().map_or(0, char::len_utf8);
            p += len + escaped;
            continue;
        }
        p += len;
        if ch == quote {
            break;
        }
    }
    p
}

fn previous_non_space_is_tag_open(code: &str, start: usize) -> bool {
    let mut prev = None;
    for ch in code[..start].chars().rev() {
        if ch.is_whitespace() {
            continue;
        }
        prev = Some(ch);
        break;
    }
    matches!(prev, Some('<' | '/'))
}

fn looks_like_html_tag(rest: &str) -> bool {
    let mut chars = rest.chars();
    if chars.next() != Some('<') {
        return false;
    }

    let Some(mut ch) = chars.next() else {
        return false;
    };
    if ch == '/' {
        let Some(next) = chars.next() else {
            return false;
        };
        ch = next;
    }

    ch.is_ascii_alphabetic() || matches!(ch, '!' | '?')
}

fn is_html_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.' | '!' | '?')
}

fn is_css_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || matches!(ch, '_' | '-')
}

fn is_css_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

fn is_css_number_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '%' | '-')
}

fn starts_hex_color(code: &str, pos: usize) -> bool {
    let mut count = 0usize;
    for ch in code[pos + 1..].chars() {
        if ch.is_ascii_hexdigit() {
            count += 1;
        } else {
            break;
        }
    }
    matches!(count, 3 | 4 | 6 | 8)
}

fn next_non_space_is(code: &str, pos: usize, expected: char) -> bool {
    for ch in code[pos..].chars() {
        if ch.is_whitespace() {
            continue;
        }
        return ch == expected;
    }
    false
}

fn previous_non_space_is_selector(code: &str, start: usize) -> bool {
    for ch in code[..start].chars().rev() {
        if ch.is_whitespace() {
            continue;
        }
        return matches!(ch, '.' | '#');
    }
    false
}

fn is_list_marker(rest: &str) -> bool {
    let mut chars = rest.char_indices();
    let Some((_, first)) = chars.next() else {
        return false;
    };

    if matches!(first, '-' | '*' | '+') {
        return chars
            .next()
            .is_none_or(|(_, ch)| ch == '\n' || ch.is_whitespace());
    }

    if !first.is_ascii_digit() {
        return false;
    }

    let mut digits = 1usize;
    for (idx, ch) in chars {
        if ch.is_ascii_digit() {
            digits += 1;
            continue;
        }
        if digits > 0 && matches!(ch, '.' | ')') {
            let after = idx + ch.len_utf8();
            return rest[after..]
                .chars()
                .next()
                .is_none_or(|next| next == '\n' || next.is_whitespace());
        }
        return false;
    }
    false
}

fn list_marker_end(code: &str, pos: usize) -> usize {
    let rest = &code[pos..];
    if rest
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, '-' | '*' | '+'))
    {
        return pos + 1;
    }
    let mut p = pos;
    p = consume_while(code, p, |c| c.is_ascii_digit());
    if let Some(ch) = code[p..].chars().next()
        && matches!(ch, '.' | ')')
    {
        p += ch.len_utf8();
    }
    p
}

fn consume_markdown_code(code: &str, pos: usize) -> usize {
    let run_end = consume_while(code, pos, |c| c == '`');
    let tick_count = run_end - pos;
    if tick_count >= 3 {
        return run_end;
    }
    let needle = &code[pos..run_end];
    code.get(run_end..)
        .and_then(|rest| rest.find(needle).map(|off| run_end + off + tick_count))
        .unwrap_or(run_end)
}

fn consume_markdown_plain(code: &str, mut pos: usize) -> usize {
    while pos < code.len() {
        let Some(ch) = code[pos..].chars().next() else {
            break;
        };
        if ch == '\n' || "`*_~[]()!|:".contains(ch) {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn language_key(lang: &str) -> String {
    let lower = lang.trim().to_ascii_lowercase();
    let without_prefix = lower.strip_prefix("language-").unwrap_or(&lower);
    let end = without_prefix
        .char_indices()
        .find_map(|(idx, ch)| {
            (!(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '#' | '-' | '_'))).then_some(idx)
        })
        .unwrap_or(without_prefix.len());
    without_prefix[..end].to_string()
}

fn lexer(lang: &str) -> Option<Lexer> {
    let l = language_key(lang);
    match l.as_str() {
        "html" | "htm" | "xhtml" | "xml" | "svg" => Some(Lexer::Html),
        "css" | "scss" | "sass" => Some(Lexer::Css),
        "markdown" | "md" | "mdown" | "mkd" => Some(Lexer::Markdown),
        "rust" | "rs" => Some(Lexer::Generic(Rules {
            keywords: RUST_KW,
            types: RUST_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"'],
            case_insensitive: false,
            hash_directives: false,
        })),
        "python" | "py" => Some(Lexer::Generic(Rules {
            keywords: PY_KW,
            types: PY_TY,
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
            case_insensitive: false,
            hash_directives: false,
        })),
        "javascript" | "js" | "jsx" | "mjs" | "cjs" | "typescript" | "ts" | "tsx" => {
            Some(Lexer::Generic(Rules {
                keywords: JS_KW,
                types: JS_TY,
                line_comments: &["//"],
                block_comment: Some(("/*", "*/")),
                strings: &['"', '\'', '`'],
                case_insensitive: false,
                hash_directives: false,
            }))
        }
        "json" | "jsonc" => Some(Lexer::Generic(Rules {
            keywords: &["true", "false", "null"],
            types: &[],
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"'],
            case_insensitive: false,
            hash_directives: false,
        })),
        "bash" | "sh" | "shell" | "zsh" | "console" => Some(Lexer::Generic(Rules {
            keywords: SH_KW,
            types: &[],
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
            case_insensitive: false,
            hash_directives: false,
        })),
        "go" | "golang" => Some(Lexer::Generic(Rules {
            keywords: GO_KW,
            types: GO_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '`'],
            case_insensitive: false,
            hash_directives: false,
        })),
        "c" | "h" | "cpp" | "c++" | "cc" | "hpp" => Some(Lexer::Generic(Rules {
            keywords: C_KW,
            types: C_TY,
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            strings: &['"', '\''],
            case_insensitive: false,
            hash_directives: true,
        })),
        "toml" => Some(Lexer::Generic(Rules {
            keywords: &["true", "false"],
            types: &[],
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
            case_insensitive: false,
            hash_directives: false,
        })),
        "ini" | "cfg" | "conf" => Some(Lexer::Generic(Rules {
            keywords: &["true", "false"],
            types: &[],
            // INI comments are conventionally `;` (and often `#`).
            line_comments: &[";", "#"],
            block_comment: None,
            strings: &['"', '\''],
            case_insensitive: false,
            hash_directives: false,
        })),
        "yaml" | "yml" => Some(Lexer::Generic(Rules {
            keywords: &["true", "false", "null", "yes", "no"],
            types: &[],
            line_comments: &["#"],
            block_comment: None,
            strings: &['"', '\''],
            case_insensitive: false,
            hash_directives: false,
        })),
        "sql" => Some(Lexer::Generic(Rules {
            keywords: SQL_KW,
            types: SQL_TY,
            line_comments: &["--"],
            block_comment: Some(("/*", "*/")),
            strings: &['\'', '"'],
            case_insensitive: true,
            hash_directives: false,
        })),
        _ => None,
    }
}

const CSS_KW: &[&str] = &[
    "auto",
    "block",
    "border-box",
    "center",
    "currentColor",
    "flex",
    "grid",
    "inherit",
    "initial",
    "inline",
    "inline-block",
    "none",
    "revert",
    "solid",
    "transparent",
    "unset",
];

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
