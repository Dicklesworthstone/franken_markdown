//! Clean-room, dependency-free syntax highlighter for fenced code blocks.
//!
//! A single small generic lexer driven by per-language token rules (keywords,
//! types, comment markers, string delimiters). This deliberately does NOT pull
//! in a Sublime-grammar engine (syntect) or tree-sitter (C) — for the languages
//! that actually appear in technical Markdown, a focused tokenizer gives a
//! beautiful, fast, zero-dependency result. Unknown languages fall back to plain
//! (escaped) text. The output is byte-offset spans into the original code, so
//! the HTML emitter can slice + escape safely.
//!
//! # Known limitations (cosmetic only — never a content drop)
//!
//! The lexer classifies *colors*; it always tiles the exact input bytes, so a
//! mis-classification only tints a token wrongly and never adds, drops, or
//! reorders characters. Two cases are intentionally not modeled, because a
//! correct fix needs per-language lexer state that a single shared tokenizer
//! cannot carry without disproportionate complexity and regression risk for a
//! purely cosmetic gain:
//!
//! - **An unterminated string literal colors to end-of-block.** A quote with no
//!   matching close is treated as a string running to EOF. Bounding it to the
//!   line would mis-handle languages whose strings legitimately span lines
//!   (Rust `"…"`, Go/JS raw/template literals, Bash, SQL). Real editors color
//!   genuinely-unterminated input the same way.
//! - **JS/TS `/regex/` literals are tinted as division.** Distinguishing a
//!   regex from the division operator requires the previous-significant-token
//!   state a stateless generic lexer does not track, so a `/`-delimited regex
//!   containing a quote can open a spurious string span. Keyword/string/comment
//!   coloring elsewhere on the line is unaffected.

use std::borrow::Cow;

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
    Mermaid,
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
    let mut spans = Vec::new();
    highlight_into(lang, code, &mut spans);
    spans
}

/// Highlight `code` for language `lang`, writing classified byte spans into
/// `spans` after clearing it. Unknown languages yield a single `Plain` span.
pub fn highlight_into(lang: &str, code: &str, spans: &mut Vec<Span>) {
    spans.clear();
    match lexer(lang) {
        Some(Lexer::Generic(r)) => lex_generic_into(code, &r, spans),
        Some(Lexer::Html) => lex_html_into(code, spans),
        Some(Lexer::Css) => lex_css_into(code, spans),
        Some(Lexer::Markdown) => lex_markdown_into(code, spans),
        Some(Lexer::Mermaid) => lex_mermaid_into(code, spans),
        None => spans.push(Span {
            kind: Tok::Plain,
            start: 0,
            end: code.len(),
        }),
    }
}

fn lex_generic_into(code: &str, r: &Rules, spans: &mut Vec<Span>) {
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
        if r.hash_directives && c == '#' && line_prefix_is_blank(code, pos) {
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
        let kind = if is_generic_operator_char(c) {
            Tok::Operator
        } else if is_generic_punct_char(c) {
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

fn lex_html_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0usize;

    while pos < len {
        let rest = &code[pos..];

        if rest.starts_with("<!--") {
            let end = find_after(code, pos + 4, "-->").unwrap_or(len);
            push_span(spans, Tok::Comment, pos, end);
            pos = end;
            continue;
        }

        if rest.starts_with('<') {
            if !looks_like_html_tag(rest) {
                push_span(spans, Tok::Operator, pos, pos + 1);
                pos += 1;
                continue;
            }

            push_span(spans, Tok::Operator, pos, pos + 1);
            pos += 1;

            if code[pos..].starts_with('/') {
                push_span(spans, Tok::Operator, pos, pos + 1);
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
                    push_span(spans, Tok::Plain, start, pos);
                } else if code[pos..].starts_with("/>") {
                    push_span(spans, Tok::Operator, pos, pos + 2);
                    pos += 2;
                    break;
                } else if ch == '>' {
                    push_span(spans, Tok::Operator, pos, pos + clen);
                    pos += clen;
                    break;
                } else if ch == '"' || ch == '\'' {
                    let end = consume_quoted(code, pos, ch);
                    push_span(spans, Tok::Str, pos, end);
                    pos = end;
                } else if ch == '=' {
                    push_span(spans, Tok::Operator, pos, pos + clen);
                    pos += clen;
                } else if is_html_name_char(ch) {
                    let start = pos;
                    pos = consume_while(code, pos, is_html_name_char);
                    let kind = if previous_non_space_is_tag_open(code, start) {
                        Tok::Keyword
                    } else {
                        Tok::Type
                    };
                    push_span(spans, kind, start, pos);
                } else {
                    push_span(spans, Tok::Punct, pos, pos + clen);
                    pos += clen;
                }
            }
            continue;
        }

        let next_tag = rest.find('<').map_or(len, |off| pos + off);
        push_span(spans, Tok::Plain, pos, next_tag);
        pos = next_tag;
    }
}

fn lex_css_into(code: &str, spans: &mut Vec<Span>) {
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
            push_span(spans, Tok::Plain, start, pos);
        } else if rest.starts_with("/*") {
            let end = find_after(code, pos + 2, "*/").unwrap_or(len);
            push_span(spans, Tok::Comment, pos, end);
            pos = end;
        } else if ch == '"' || ch == '\'' {
            let end = consume_quoted(code, pos, ch);
            push_span(spans, Tok::Str, pos, end);
            pos = end;
        } else if ch == '@' {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, is_css_ident_char);
            push_span(spans, Tok::Keyword, start, pos);
        } else if ch == '#' && starts_hex_color(code, pos) {
            let start = pos;
            pos += clen;
            pos = consume_while(code, pos, |c| c.is_ascii_hexdigit());
            push_span(spans, Tok::Number, start, pos);
        } else if ch.is_ascii_digit() {
            let start = pos;
            pos = consume_while(code, pos, is_css_number_char);
            push_span(spans, Tok::Number, start, pos);
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
            push_span(spans, kind, start, pos);
        } else {
            let kind = if is_css_punct_char(ch) {
                Tok::Punct
            } else if is_css_operator_char(ch) {
                Tok::Operator
            } else {
                Tok::Plain
            };
            push_span(spans, kind, pos, pos + clen);
            pos += clen;
        }
    }
}

fn lex_markdown_into(code: &str, spans: &mut Vec<Span>) {
    let len = code.len();
    let mut pos = 0usize;
    let mut line_start = true;
    let mut line_indent_columns = 0usize;

    while pos < len {
        let rest = &code[pos..];

        if rest.starts_with("<!--") {
            let end = find_after(code, pos + 4, "-->").unwrap_or(len);
            push_span(spans, Tok::Comment, pos, end);
            line_start = end > 0 && code.as_bytes().get(end - 1).is_some_and(|b| *b == b'\n');
            if line_start {
                line_indent_columns = 0;
            }
            pos = end;
            continue;
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        let clen = ch.len_utf8();

        if ch == '\n' {
            push_span(spans, Tok::Plain, pos, pos + clen);
            pos += clen;
            line_start = true;
            line_indent_columns = 0;
        } else if line_start && ch.is_whitespace() {
            let start = pos;
            let mut only_markdown_indent = true;
            while pos < len {
                let Some(ws) = code[pos..].chars().next() else {
                    break;
                };
                if ws == '\n' || !ws.is_whitespace() {
                    break;
                }
                if !matches!(ws, ' ' | '\t') {
                    only_markdown_indent = false;
                }
                line_indent_columns = markdown_indent_after(line_indent_columns, ws);
                pos += ws.len_utf8();
            }
            push_span(spans, Tok::Plain, start, pos);
            if !only_markdown_indent {
                line_start = false;
            }
        } else if line_start && line_indent_columns <= 3 && is_markdown_heading_marker(rest) {
            let start = pos;
            pos = consume_while(code, pos, |c| c == '#');
            push_span(spans, Tok::Keyword, start, pos);
            line_start = false;
        } else if line_start && line_indent_columns <= 3 && ch == '>' {
            push_span(spans, Tok::Operator, pos, pos + clen);
            pos += clen;
            line_start = false;
        } else if line_start && line_indent_columns <= 3 && is_list_marker(rest) {
            let end = list_marker_end(code, pos);
            push_span(spans, Tok::Operator, pos, end);
            pos = end;
            line_start = false;
        } else if ch == '`' {
            let end = consume_markdown_code(code, pos);
            push_span(spans, Tok::Str, pos, end);
            line_start = false;
            pos = end;
        } else if ch.is_ascii_digit() {
            let start = pos;
            pos = consume_while(code, pos, |c| c.is_ascii_digit());
            push_span(spans, Tok::Number, start, pos);
            line_start = false;
        } else if is_markdown_emphasis_marker(ch) {
            let start = pos;
            pos = consume_while(code, pos, |c| c == ch);
            push_span(spans, Tok::Operator, start, pos);
            line_start = false;
        } else if is_markdown_punct_char(ch) {
            push_span(spans, Tok::Punct, pos, pos + clen);
            pos += clen;
            line_start = false;
        } else {
            let start = pos;
            pos = consume_markdown_plain(code, pos);
            push_span(spans, Tok::Plain, start, pos);
            line_start = false;
        }
    }
}

fn lex_mermaid_into(code: &str, spans: &mut Vec<Span>) {
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
            push_span(spans, Tok::Plain, start, pos);
        } else if rest.starts_with("%%") {
            let end = rest.find('\n').map_or(len, |off| pos + off);
            push_span(spans, Tok::Comment, pos, end);
            pos = end;
        } else if ch == '"' || ch == '\'' {
            let end = consume_quoted(code, pos, ch);
            push_span(spans, Tok::Str, pos, end);
            pos = end;
        } else if ch.is_ascii_digit() {
            let start = pos;
            pos = consume_while(code, pos, is_mermaid_number_char);
            push_span(spans, Tok::Number, start, pos);
        } else if is_mermaid_ident_start(ch) {
            let start = pos;
            pos = consume_while(code, pos, is_mermaid_ident_char);
            let word = &code[start..pos];
            let kind = if MERMAID_KW.iter().any(|kw| kw.eq_ignore_ascii_case(word)) {
                Tok::Keyword
            } else if MERMAID_TY.iter().any(|ty| ty.eq_ignore_ascii_case(word)) {
                Tok::Type
            } else {
                Tok::Plain
            };
            push_span(spans, kind, start, pos);
        } else {
            let kind = if is_mermaid_operator_char(ch) {
                Tok::Operator
            } else if is_mermaid_punct_char(ch) {
                Tok::Punct
            } else {
                Tok::Plain
            };
            let start = pos;
            pos += clen;
            if kind == Tok::Operator {
                pos = consume_while(code, pos, is_mermaid_operator_char);
            }
            push_span(spans, kind, start, pos);
        }
    }
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

/// True iff everything before `pos` on the current line is whitespace, i.e.
/// `pos` is the first non-blank column of its line. Scans backward only to the
/// previous newline and short-circuits at the first non-whitespace char, so it
/// is O(1) amortized on a line densely packed with the trigger character. It
/// replaces `code[..pos].rsplit('\n').next().trim().is_empty()`, which rescanned
/// the whole line prefix on every call and made a long `#####…` line O(n^2) to
/// highlight in the C/C++ preprocessor-directive path (matches the same
/// semantics: the newline terminates the line, all other whitespace is blank).
fn line_prefix_is_blank(code: &str, pos: usize) -> bool {
    for ch in code[..pos].chars().rev() {
        if ch == '\n' {
            return true; // reached the start of this line
        }
        if ch.is_whitespace() {
            continue; // spaces/tabs (and a stray \r) before pos on this line
        }
        return false; // a non-space character precedes pos on this line
    }
    true // start of input: nothing but (optional) whitespace precedes pos
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

const fn is_generic_operator_char(ch: char) -> bool {
    matches!(
        ch,
        '+' | '-'
            | '*'
            | '/'
            | '%'
            | '='
            | '<'
            | '>'
            | '!'
            | '&'
            | '|'
            | '^'
            | '~'
            | '?'
            | ':'
            | '@'
    )
}

const fn is_generic_punct_char(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | '.')
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

const fn is_css_punct_char(ch: char) -> bool {
    matches!(ch, ':' | ';' | '{' | '}' | '(' | ')' | ',' | '[' | ']')
}

const fn is_css_operator_char(ch: char) -> bool {
    matches!(
        ch,
        '.' | '>' | '+' | '~' | '*' | '=' | '|' | '^' | '$' | '!'
    )
}

const fn is_markdown_emphasis_marker(ch: char) -> bool {
    matches!(ch, '*' | '_' | '~')
}

const fn is_markdown_punct_char(ch: char) -> bool {
    matches!(ch, '[' | ']' | '(' | ')' | '!' | '|' | ':')
}

const fn is_markdown_plain_stop_char(ch: char) -> bool {
    matches!(
        ch,
        '`' | '*' | '_' | '~' | '[' | ']' | '(' | ')' | '!' | '|' | ':'
    )
}

fn is_mermaid_number_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '-')
}

fn is_mermaid_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_mermaid_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

fn is_mermaid_operator_char(ch: char) -> bool {
    matches!(
        ch,
        '-' | '=' | '.' | '>' | '<' | '+' | '*' | '/' | '\\' | '&' | '@' | '~' | '!'
    )
}

const fn is_mermaid_punct_char(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')' | '[' | ']' | '{' | '}' | ':' | ',' | ';' | '|'
    )
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
            if digits > 9 {
                return false;
            }
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

fn markdown_indent_after(col: usize, ch: char) -> usize {
    match ch {
        ' ' => col + 1,
        '\t' => col + 4 - col % 4,
        _ => col,
    }
}

fn is_markdown_heading_marker(rest: &str) -> bool {
    let hashes = rest.bytes().take_while(|&byte| byte == b'#').count();
    if hashes == 0 || hashes > 6 {
        return false;
    }
    rest[hashes..]
        .chars()
        .next()
        .is_none_or(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
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
        if ch == '\n' || is_markdown_plain_stop_char(ch) {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn language_key(lang: &str) -> Cow<'_, str> {
    let trimmed = lang.trim();
    let without_prefix = trimmed
        .get(.."language-".len())
        .filter(|prefix| prefix.eq_ignore_ascii_case("language-"))
        .map_or(trimmed, |_| &trimmed["language-".len()..]);
    let end = without_prefix
        .char_indices()
        .find_map(|(idx, ch)| {
            (!(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '#' | '-' | '_'))).then_some(idx)
        })
        .unwrap_or(without_prefix.len());
    let key = &without_prefix[..end];
    if key.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Owned(key.to_ascii_lowercase())
    } else {
        Cow::Borrowed(key)
    }
}

fn lexer(lang: &str) -> Option<Lexer> {
    let l = language_key(lang);
    match l.as_ref() {
        "html" | "htm" | "xhtml" | "xml" | "svg" => Some(Lexer::Html),
        "css" | "scss" | "sass" => Some(Lexer::Css),
        "markdown" | "md" | "mdown" | "mkd" => Some(Lexer::Markdown),
        "mermaid" | "mmd" => Some(Lexer::Mermaid),
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
        "powershell" | "pwsh" | "ps1" => Some(Lexer::Generic(Rules {
            keywords: PS_KW,
            types: &[],
            line_comments: &["#"],
            block_comment: Some(("<#", "#>")),
            strings: &['"', '\''],
            case_insensitive: true,
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

const MERMAID_KW: &[&str] = &[
    "accdescr",
    "acctitle",
    "activate",
    "actor",
    "alt",
    "as",
    "autonumber",
    "class",
    "classdef",
    "classdiagram",
    "click",
    "critical",
    "deactivate",
    "direction",
    "else",
    "end",
    "erdiagram",
    "flowchart",
    "gantt",
    "gitgraph",
    "graph",
    "journey",
    "loop",
    "mindmap",
    "note",
    "opt",
    "over",
    "par",
    "participant",
    "pie",
    "rect",
    "section",
    "sequencediagram",
    "statediagram",
    "statediagram-v2",
    "style",
    "subgraph",
    "theme",
    "timeline",
    "title",
];

const MERMAID_TY: &[&str] = &[
    "BT", "LR", "RL", "TB", "TD", "bottom", "left", "right", "top",
];

const RUST_KW: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "union",
    "unsafe", "use", "where", "while",
];

const PS_KW: &[&str] = &[
    "begin",
    "break",
    "catch",
    "class",
    "continue",
    "data",
    "default",
    "do",
    "dynamicparam",
    "else",
    "elseif",
    "end",
    "exit",
    "filter",
    "finally",
    "for",
    "foreach",
    "from",
    "function",
    "hidden",
    "if",
    "in",
    "param",
    "process",
    "return",
    "switch",
    "throw",
    "trap",
    "try",
    "until",
    "using",
    "var",
    "while",
    "workflow",
    // Common command aliases that appear in install snippets.
    "curl",
    "iex",
    "irm",
    "iwr",
    "wget",
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod line_prefix_dos_tests {
    use super::{Tok, highlight, line_prefix_is_blank};

    #[test]
    fn line_prefix_blank_detection_matches_trim_semantics() {
        // Start of input, after a newline, and after only spaces/tabs are blank;
        // any non-space character on the line makes it non-blank.
        assert!(line_prefix_is_blank("#x", 0)); // start of input
        assert!(line_prefix_is_blank("a\n#x", 2)); // right after '\n'
        assert!(line_prefix_is_blank("a\n  \t#x", 5)); // only spaces/tabs on line
        assert!(!line_prefix_is_blank("a#x", 1)); // 'a' precedes on the line
        assert!(!line_prefix_is_blank("##", 1)); // a '#' precedes on the line
        assert!(line_prefix_is_blank("x\r\n#", 3)); // CRLF: '\n' terminates the line
    }

    #[test]
    fn c_preprocessor_directive_still_highlights() {
        // The behavior the helper guards is unchanged: a line-initial `#include`
        // (optionally indented) is one Keyword token.
        let spans = highlight("c", "  #include <stdio.h>");
        let kw = spans
            .iter()
            .find(|s| matches!(s.kind, Tok::Keyword))
            .expect("directive keyword");
        assert_eq!(&"  #include <stdio.h>"[kw.start..kw.end], "#include");
    }

    #[test]
    fn powershell_install_snippet_highlights_aliases_strings_and_comments() {
        let code = "irm \"https://example.test/install.ps1\" | iex\n# done";
        let spans = highlight("powershell", code);
        assert!(
            spans
                .iter()
                .any(|s| matches!(s.kind, Tok::Keyword) && &code[s.start..s.end] == "irm")
        );
        assert!(
            spans
                .iter()
                .any(|s| matches!(s.kind, Tok::Keyword) && &code[s.start..s.end] == "iex")
        );
        assert!(spans.iter().any(|s| matches!(s.kind, Tok::Str)
            && &code[s.start..s.end] == "\"https://example.test/install.ps1\""));
        assert!(
            spans
                .iter()
                .any(|s| matches!(s.kind, Tok::Comment) && &code[s.start..s.end] == "# done")
        );
    }

    #[test]
    fn dense_hash_line_stays_linear_and_loses_no_bytes() {
        // The DoS shape: one long line of '#'. Previously each '#' rescanned the
        // whole line prefix (O(n^2)); now each is O(1). Completing quickly is the
        // proof of linearity; span tiling proves every byte is preserved.
        let code = "#".repeat(200_000);
        let spans = highlight("c", &code);
        assert!(!spans.is_empty());
        // Spans must tile [0, len) with no gaps or overlaps (no dropped bytes).
        let mut next = 0usize;
        for s in &spans {
            assert_eq!(s.start, next, "gap/overlap in highlight spans");
            assert!(s.end > s.start);
            next = s.end;
        }
        assert_eq!(next, code.len(), "spans must cover the whole input");
    }
}

#[cfg(test)]
mod char_classifier_tests {
    use super::{
        is_css_operator_char, is_css_punct_char, is_generic_operator_char, is_generic_punct_char,
        is_markdown_emphasis_marker, is_markdown_plain_stop_char, is_markdown_punct_char,
        is_mermaid_operator_char, is_mermaid_punct_char,
    };

    fn assert_matches_literal_set(literal: &str, classifier: impl Fn(char) -> bool) {
        for byte in 0u8..=127 {
            let ch = char::from(byte);
            assert_eq!(
                classifier(ch),
                literal.contains(ch),
                "classifier mismatch for ASCII {byte:#04x} ({ch:?}) in {literal:?}"
            );
        }
        assert!(
            !classifier('λ'),
            "ASCII literal classifier must reject non-ASCII probes"
        );
    }

    #[test]
    fn char_classifiers_match_their_former_literal_sets() {
        assert_matches_literal_set("+-*/%=<>!&|^~?:@", is_generic_operator_char);
        assert_matches_literal_set("()[]{},;.", is_generic_punct_char);
        assert_matches_literal_set(":;{}(),[]", is_css_punct_char);
        assert_matches_literal_set(".>+~*=|^$!", is_css_operator_char);
        assert_matches_literal_set("*_~", is_markdown_emphasis_marker);
        assert_matches_literal_set("[]()!|:", is_markdown_punct_char);
        assert_matches_literal_set("`*_~[]()!|:", is_markdown_plain_stop_char);
        assert_matches_literal_set("-.=><+*/\\&@~!", is_mermaid_operator_char);
        assert_matches_literal_set("()[]{}:,;|", is_mermaid_punct_char);
    }
}
