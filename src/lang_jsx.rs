#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::if_same_then_else)]

//! JSX incremental lexer (FCB-022 · fcb-9vx.8).
//!
//! Reuses the qualified upstream JavaScript lexical transitions per the
//! language composition contract. Classifies JSX source into byte-exact
//! [`Span`]s that tile the input exactly. Handles, under declared capability:
//!
//! - **Markup transitions**: `<tag>` and `<>` element openings from expression contexts.
//! - **Tags & components**: HTML tags (`<div>`) and capitalized components (`<Component>`).
//! - **Attributes & quotes**: `attr="value"`, `attr='value'`, with braces inside quotes kept literal.
//! - **Expression containers**: `{ expr }` re-entering full JavaScript lexing.
//! - **Fragments**: `<> ... </>` fragment shorthand.
//! - **Entities**: character entities (`&amp;`, `&lt;`, `&copy;`, `&#169;`) inside children.
//! - **Generics/comparisons vs tags**: `<` after values remains comparison operator.
//! - **Malformed nesting**: unterminated tags/expressions safely tile to EOF with no panic.

use crate::highlight::{Span, Tok, is_capitalized_not_all_caps};
use crate::lang_javascript::{Prev, scan_javascript_token_at};

/// The versioned JSX capability row (FCB-022 capability publication).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsxCapabilityV1 {
    /// Capability row format version.
    pub version: u32,
    /// Incremental (chunk-safe) classification is supported for JSX.
    pub incremental: bool,
    /// Tag and attribute markup transitions supported.
    pub tag_and_attribute_transitions: bool,
    /// Embedded JavaScript expression containers (`{ ... }`) supported.
    pub embedded_expressions: bool,
    /// Fragment shorthand (`<> ... </>`) supported.
    pub fragments: bool,
    /// Character entities inside children supported.
    pub entities: bool,
}

/// The JSX capability row published by this module.
pub const JSX_CAPABILITY_V1: JsxCapabilityV1 = JsxCapabilityV1 {
    version: 1,
    incremental: true,
    tag_and_attribute_transitions: true,
    embedded_expressions: true,
    fragments: true,
    entities: true,
};

/// Contexts in the JSX hierarchical lexer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JsxContext {
    /// Inside a tag: `<tag attr="val" ... >`
    Tag { start: usize, has_tag_name: bool },
    /// Inside JSX children content (between open and close tags)
    Children { start: usize },
    /// Inside an embedded JavaScript expression `{ expr }`
    Expr { start: usize, braces: usize },
}

impl JsxContext {
    fn jsx_start(&self) -> Option<usize> {
        match *self {
            JsxContext::Tag { start, .. } | JsxContext::Children { start } => Some(start),
            JsxContext::Expr { .. } => None,
        }
    }
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
    spans.push(Span { kind, start, end });
    *last_end = end;
}

/// Returns true if `<` at `pos` opens a JSX tag or fragment rather than a comparison.
fn is_jsx_start(code: &str, pos: usize, prev: Prev) -> bool {
    // Only in expression contexts: not after values
    if matches!(prev, Prev::Value | Prev::ValueKeyword) {
        return false;
    }
    let rest = &code[pos..];
    if !rest.starts_with('<') {
        return false;
    }
    let after_lt = &code[pos + 1..];
    // Fragment opening: `<>`
    if after_lt.starts_with('>') {
        return true;
    }
    // Tag name start: ASCII alphabetic or '_'
    let first = match after_lt.chars().next() {
        Some(c) => c,
        None => return false,
    };
    first.is_ascii_alphabetic() || first == '_'
}

/// Lex JSX source into exact tiling spans.
pub fn lex_jsx_into(code: &str, spans: &mut Vec<Span>) {
    lex_jsx_composed_into(code, spans, &[], &[]);
}

/// Lex JSX source with composed extra keywords and types (for TSX composition).
/// Returns the byte offset of the outermost unclosed JSX construct, if any.
pub fn lex_jsx_composed_into(
    code: &str,
    spans: &mut Vec<Span>,
    extra_keywords: &[&str],
    extra_types: &[&str],
) -> Option<usize> {
    let bytes_len = code.len();
    let mut pos = 0usize;
    let mut last_end = 0usize;
    let mut prev = Prev::Start;

    // Stack of JSX element contexts: Tag, Children, Expr
    let mut mode_stack: Vec<JsxContext> = Vec::new();

    // State for composed JavaScript template interpolations
    let mut js_interp_stack: Vec<usize> = Vec::new();
    let mut js_brace_depth: usize = 0;

    while pos < bytes_len {
        // 1. Inside embedded JS expression `{ expr }`
        if let Some(JsxContext::Expr { braces, .. }) = mode_stack.last_mut() {
            let rest = &code[pos..];
            let ch = rest.chars().next().unwrap();

            if ch == '{' {
                *braces += 1;
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Punct, start, pos);
                prev = Prev::Operator;
                continue;
            }

            if ch == '}' {
                if *braces == 0 {
                    mode_stack.pop();
                    let start = pos;
                    pos += 1;
                    push_tiling(spans, &mut last_end, Tok::Punct, start, pos);
                    prev = Prev::Value;
                    continue;
                }
                *braces -= 1;
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Punct, start, pos);
                prev = Prev::CloseBrace;
                continue;
            }

            // Inside embedded expression, check if `<` starts a nested JSX element
            if ch == '<' && is_jsx_start(code, pos, prev) {
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Operator, start, pos);
                mode_stack.push(JsxContext::Tag {
                    start,
                    has_tag_name: false,
                });
                prev = Prev::Start;
                continue;
            }

            // Single token in embedded expression via shared JS tokenizer
            let (end, kind, new_prev) = scan_javascript_token_at(
                code,
                pos,
                prev,
                &mut js_interp_stack,
                &mut js_brace_depth,
                extra_keywords,
                extra_types,
            );
            push_tiling(spans, &mut last_end, kind, pos, end);
            pos = end;
            prev = new_prev;
            continue;
        }

        // 2. Inside JSX Children mode: text, entities, tags, fragments, or `{ expr }`
        if let Some(JsxContext::Children { .. }) = mode_stack.last() {
            let rest = &code[pos..];
            let ch = rest.chars().next().unwrap();

            // Closing fragment: `</>`
            if rest.starts_with("</>") {
                let start = pos;
                pos += 3;
                push_tiling(spans, &mut last_end, Tok::Operator, start, start + 2);
                push_tiling(spans, &mut last_end, Tok::Operator, start + 2, start + 3);
                mode_stack.pop();
                prev = Prev::Value;
                continue;
            }

            // Closing tag: `</tag_name>`
            if rest.starts_with("</") {
                let start = pos;
                pos += 2;
                push_tiling(spans, &mut last_end, Tok::Operator, start, pos);

                // Optional whitespace inside closing tag
                let ws_start = pos;
                while pos < bytes_len && code.as_bytes()[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos > ws_start {
                    push_tiling(spans, &mut last_end, Tok::Plain, ws_start, pos);
                }

                // Tag name
                let name_start = pos;
                while pos < bytes_len
                    && (code.as_bytes()[pos].is_ascii_alphanumeric()
                        || matches!(code.as_bytes()[pos], b'_' | b'-' | b'.' | b':'))
                {
                    pos += 1;
                }
                if pos > name_start {
                    let name = &code[name_start..pos];
                    let kind = if is_capitalized_not_all_caps(name) {
                        Tok::Type
                    } else {
                        Tok::Keyword
                    };
                    push_tiling(spans, &mut last_end, kind, name_start, pos);
                }

                // Optional whitespace before `>`
                let ws_end_start = pos;
                while pos < bytes_len && code.as_bytes()[pos].is_ascii_whitespace() {
                    pos += 1;
                }
                if pos > ws_end_start {
                    push_tiling(spans, &mut last_end, Tok::Plain, ws_end_start, pos);
                }

                // Closing `>`
                if pos < bytes_len && code.as_bytes()[pos] == b'>' {
                    let gt_start = pos;
                    pos += 1;
                    push_tiling(spans, &mut last_end, Tok::Operator, gt_start, pos);
                    mode_stack.pop();
                    prev = Prev::Value;
                } else {
                    prev = Prev::Operator;
                }
                continue;
            }

            // Nested opening fragment: `<>`
            if rest.starts_with("<>") {
                let start = pos;
                pos += 2;
                push_tiling(spans, &mut last_end, Tok::Operator, start, start + 1);
                push_tiling(spans, &mut last_end, Tok::Operator, start + 1, start + 2);
                mode_stack.push(JsxContext::Children { start });
                prev = Prev::Start;
                continue;
            }

            // Nested opening tag: `<tag`
            if ch == '<' && is_jsx_start(code, pos, Prev::Start) {
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Operator, start, pos);
                mode_stack.push(JsxContext::Tag {
                    start,
                    has_tag_name: false,
                });
                prev = Prev::Start;
                continue;
            }

            // Embedded expression container: `{ expr }`
            if ch == '{' {
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Punct, start, pos);
                mode_stack.push(JsxContext::Expr { start, braces: 0 });
                prev = Prev::Operator;
                continue;
            }

            // Character entity: `&name;`, `&#123;`, `&#x1F;`
            if ch == '&' {
                let start = pos;
                let mut p = pos + 1;
                if p < bytes_len && code.as_bytes()[p] == b'#' {
                    p += 1;
                    if p < bytes_len && (code.as_bytes()[p] == b'x' || code.as_bytes()[p] == b'X') {
                        p += 1;
                        while p < bytes_len && code.as_bytes()[p].is_ascii_hexdigit() {
                            p += 1;
                        }
                    } else {
                        while p < bytes_len && code.as_bytes()[p].is_ascii_digit() {
                            p += 1;
                        }
                    }
                } else {
                    while p < bytes_len
                        && (code.as_bytes()[p].is_ascii_alphanumeric()
                            || code.as_bytes()[p] == b'_')
                    {
                        p += 1;
                    }
                }
                if p < bytes_len && code.as_bytes()[p] == b';' && p > start + 1 {
                    p += 1;
                    push_tiling(spans, &mut last_end, Tok::Keyword, start, p);
                    pos = p;
                    continue;
                }
            }

            // Plain children text until next `<`, `{`, or `&`
            let start = pos;
            while pos < bytes_len {
                let c = code[pos..].chars().next().unwrap();
                if matches!(c, '<' | '{' | '&') {
                    break;
                }
                pos += c.len_utf8();
            }
            if pos > start {
                push_tiling(spans, &mut last_end, Tok::Plain, start, pos);
            } else {
                let c = code[pos..].chars().next().unwrap();
                pos += c.len_utf8();
                push_tiling(spans, &mut last_end, Tok::Plain, start, pos);
            }
            continue;
        }

        // 2. Inside JSX Tag mode: `<tag_name attr="val" ... >`
        if let Some(JsxContext::Tag {
            has_tag_name, ..
        }) = mode_stack.last_mut()
        {
            let rest = &code[pos..];
            let ch = rest.chars().next().unwrap();

            // Whitespace inside tag
            if ch.is_whitespace() {
                let start = pos;
                pos += ch.len_utf8();
                while pos < bytes_len && code[pos..].chars().next().unwrap().is_whitespace() {
                    pos += code[pos..].chars().next().unwrap().len_utf8();
                }
                push_tiling(spans, &mut last_end, Tok::Plain, start, pos);
                continue;
            }

            // Comments inside tag
            if rest.starts_with("/*") {
                let start = pos;
                let end = code[start + 2..]
                    .find("*/")
                    .map_or(bytes_len, |at| start + 2 + at + 2);
                push_tiling(spans, &mut last_end, Tok::Comment, start, end);
                pos = end;
                continue;
            }
            if rest.starts_with("//") {
                let start = pos;
                let end = code[start..].find('\n').map_or(bytes_len, |nl| start + nl);
                push_tiling(spans, &mut last_end, Tok::Comment, start, end);
                pos = end;
                continue;
            }

            // First identifier is tag name
            if !*has_tag_name && (ch.is_ascii_alphabetic() || ch == '_') {
                let start = pos;
                pos += ch.len_utf8();
                while pos < bytes_len {
                    let c = code[pos..].chars().next().unwrap();
                    if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') {
                        pos += c.len_utf8();
                    } else {
                        break;
                    }
                }
                let word = &code[start..pos];
                let kind = if is_capitalized_not_all_caps(word) {
                    Tok::Type
                } else {
                    Tok::Keyword
                };
                *has_tag_name = true;
                push_tiling(spans, &mut last_end, kind, start, pos);
                continue;
            }

            // Self-closing tag end: `/>`
            if rest.starts_with("/>") {
                let start = pos;
                pos += 2;
                push_tiling(spans, &mut last_end, Tok::Operator, start, pos);
                mode_stack.pop();
                prev = Prev::Value;
                continue;
            }

            // Tag opening end: `>`
            if ch == '>' {
                let gt_start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Operator, gt_start, pos);
                let tag_start = match mode_stack.pop() {
                    Some(JsxContext::Tag { start, .. }) => start,
                    _ => gt_start,
                };
                mode_stack.push(JsxContext::Children { start: tag_start });
                prev = Prev::Start;
                continue;
            }

            // Embedded expression inside tag: `{ expr }`
            if ch == '{' {
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Punct, start, pos);
                mode_stack.push(JsxContext::Expr { start, braces: 0 });
                prev = Prev::Operator;
                continue;
            }

            // Quoted attribute value: "..." or '...'
            // Braces inside quoted strings remain literal string content.
            if ch == '"' || ch == '\'' {
                let quote = ch;
                let start = pos;
                pos += 1;
                while pos < bytes_len {
                    let c = code[pos..].chars().next().unwrap();
                    if c == '\\' {
                        pos += 1;
                        if pos < bytes_len {
                            pos += code[pos..].chars().next().unwrap().len_utf8();
                        }
                        continue;
                    }
                    pos += c.len_utf8();
                    if c == quote {
                        break;
                    }
                }
                push_tiling(spans, &mut last_end, Tok::Str, start, pos);
                continue;
            }

            // Equals operator
            if ch == '=' {
                let start = pos;
                pos += 1;
                push_tiling(spans, &mut last_end, Tok::Operator, start, pos);
                continue;
            }

            // Attribute name
            if ch.is_ascii_alphabetic() || ch == '_' {
                let start = pos;
                pos += ch.len_utf8();
                while pos < bytes_len {
                    let c = code[pos..].chars().next().unwrap();
                    if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') {
                        pos += c.len_utf8();
                    } else {
                        break;
                    }
                }
                push_tiling(spans, &mut last_end, Tok::Type, start, pos);
                continue;
            }

            // Fallback inside tag
            let start = pos;
            pos += ch.len_utf8();
            push_tiling(spans, &mut last_end, Tok::Plain, start, pos);
            continue;
        }

        // 4. Top-level JavaScript mode: check if `<` starts a JSX element or fragment
        let ch = code[pos..].chars().next().unwrap();

        // Fragment opening `<>`
        if code[pos..].starts_with("<>") && is_jsx_start(code, pos, prev) {
            let start = pos;
            pos += 2;
            push_tiling(spans, &mut last_end, Tok::Operator, start, start + 1);
            push_tiling(spans, &mut last_end, Tok::Operator, start + 1, start + 2);
            mode_stack.push(JsxContext::Children { start });
            prev = Prev::Start;
            continue;
        }

        // Tag opening `<tag`
        if ch == '<' && is_jsx_start(code, pos, prev) {
            let start = pos;
            pos += 1;
            push_tiling(spans, &mut last_end, Tok::Operator, start, pos);
            mode_stack.push(JsxContext::Tag {
                start,
                has_tag_name: false,
            });
            prev = Prev::Start;
            continue;
        }

        // 5. Standard JavaScript tokens via shared token scanner (O(1) per token)
        let (end, kind, new_prev) = scan_javascript_token_at(
            code,
            pos,
            prev,
            &mut js_interp_stack,
            &mut js_brace_depth,
            extra_keywords,
            extra_types,
        );
        push_tiling(spans, &mut last_end, kind, pos, end);
        pos = end;
        prev = new_prev;
    }

    // Trailing gap closure
    if last_end < bytes_len {
        let start = last_end;
        push_tiling(spans, &mut last_end, Tok::Plain, start, bytes_len);
    }

    mode_stack.iter().find_map(|ctx| ctx.jsx_start())
}

/// Find chunk hold point for streaming JSX input.
///
/// If inside an unclosed JSX element or tag, holds from the opening of that
/// construct across chunk boundaries. When all JSX elements are closed, uses
/// JavaScript hold rules.
pub fn find_jsx_hold_from(text: &str, spans: &[Span]) -> usize {
    let mut temp = Vec::new();
    if let Some(unclosed) = lex_jsx_composed_into(text, &mut temp, &[], &[]) {
        return unclosed;
    }
    if spans.is_empty() {
        return text.len();
    }
    if let Some(last) = spans.last() {
        let slice = &text[last.start..last.end];
        if last.kind == Tok::Operator {
            if slice == "/>" {
                return last.end;
            }
            if slice == ">" {
                let before = text[..last.start].trim_end();
                if let Some(lt) = before.rfind('<') {
                    if before[lt..].starts_with("</") {
                        return last.end;
                    }
                }
            }
        }
    }
    crate::resume::find_javascript_hold_from(text, spans)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::highlight::Tok;

    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(
                span.start, cursor,
                "gap/overlap at {cursor}: span {:?} {}-{}",
                span.kind, span.start, span.end
            );
            assert!(span.end > span.start, "empty span at {cursor}");
            assert!(span.end <= code.len(), "span exceeds source length");
            assert!(code.is_char_boundary(span.start));
            assert!(code.is_char_boundary(span.end));
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    #[test]
    fn simple_jsx_element() {
        let code = "const el = <div className=\"test\">Hello World</div>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);

        let texts: Vec<&str> = spans.iter().map(|s| &code[s.start..s.end]).collect();
        assert!(texts.contains(&"div"));
        assert!(texts.contains(&"\"test\""));
        assert!(texts.contains(&"Hello World"));
    }

    #[test]
    fn self_closing_and_fragment() {
        let code = "const el = <><img src=\"icon.png\" /><Button disabled /></>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);

        let types: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Type)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(types.contains(&"Button"));
    }

    #[test]
    fn embedded_expression_containers() {
        let code = "const el = <div count={x + 1}>{items.map(i => <span>{i}</span>)}</div>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
    }

    #[test]
    fn braces_inside_quoted_attribute_are_string_content() {
        let code = "const el = <div title=\"{literal braces}\" />;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);

        let strs: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Str)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(strs.contains(&"\"{literal braces}\""));
    }

    #[test]
    fn entities_inside_children() {
        let code = "const el = <div>&copy; 2026 &amp; &lt;FrankenCode&gt; &#169;</div>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);

        let kw_entities: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(kw_entities.contains(&"&copy;"));
        assert!(kw_entities.contains(&"&amp;"));
    }

    #[test]
    fn comparison_not_confused_with_jsx_tag() {
        let code = "const cmp = a < b && b > c;\nconst tag = <tag>content</tag>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
    }

    #[test]
    fn capability_row_is_versioned() {
        assert_eq!(JSX_CAPABILITY_V1.version, 1);
        assert!(JSX_CAPABILITY_V1.incremental);
        assert!(JSX_CAPABILITY_V1.tag_and_attribute_transitions);
        assert!(JSX_CAPABILITY_V1.embedded_expressions);
        assert!(JSX_CAPABILITY_V1.fragments);
        assert!(JSX_CAPABILITY_V1.entities);
    }

    #[test]
    fn negative_control_tiling_gap() {
        let code = "<div>test</div>";
        let broken = vec![
            Span {
                kind: Tok::Operator,
                start: 0,
                end: 1,
            },
            // gap from 1..4 omitted!
            Span {
                kind: Tok::Operator,
                start: 4,
                end: 5,
            },
            Span {
                kind: Tok::Plain,
                start: 5,
                end: 9,
            },
            Span {
                kind: Tok::Keyword,
                start: 9,
                end: 15,
            },
        ];
        let result = std::panic::catch_unwind(|| {
            assert_tiling(code, &broken);
        });
        assert!(result.is_err(), "oracle must catch gaps");
    }

    #[test]
    fn nonempty_output_buffers_keep_existing_entries() {
        let sentinel = Span { kind: Tok::Comment, start: 7, end: 11 };
        let mut spans = vec![sentinel];
        lex_jsx_into("<A/>", &mut spans);
        assert_eq!(spans[0].kind, Tok::Comment);
        assert_eq!((spans[0].start, spans[0].end), (7, 11));
        assert_tiling("<A/>", &spans[1..]);
    }

    #[test]
    fn chunk_prefixes_hold_the_outermost_unfinished_element() {
        let code = "const x = <A><B>{value}</B></A>;";
        let opening = code.find('<').unwrap();
        let closing_end = code.rfind('>').unwrap() + 1;
        for end in opening + 1..closing_end {
            let prefix = &code[..end];
            let mut spans = Vec::new();
            lex_jsx_into(prefix, &mut spans);
            assert_tiling(prefix, &spans);
            assert_eq!(find_jsx_hold_from(prefix, &spans), opening, "{prefix}");
        }
        assert_eq!(find_jsx_hold_from(code, &[]), code.len());
    }
}
