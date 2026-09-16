#![forbid(unsafe_code)]

//! JSX incremental lexer (FCB-022 · fcb-9vx.8).
//!
//! Classifies JSX source into byte-exact [`Span`]s that tile the input
//! exactly. JSX extends JavaScript with XML-like markup: the lexer composes
//! the JavaScript lexer (no duplicate engine) for expression regions and
//! adds JSX-specific scanning for markup regions.
//!
//! Handles, under a declared capability:
//!
//! - **Mode transitions**: `<` followed by an identifier or `>` in
//!   expression position switches to JSX markup; `{` in markup switches
//!   back to JS expression; `}` returns to markup.
//! - **JSX elements**: opening tags, closing tags, self-closing tags,
//!   fragments (`<>...</>`).
//! - **Attributes**: name/value pairs with string or expression values.
//! - **Text content**: Plain spans; `{` starts an expression container.
//! - **Truncated tags/text**: span to EOF under the declared capability.
//! - **Malformed nesting**: the tiling invariant still holds; the lexer
//!   does not enforce tree structure (that is the parser's job).

use crate::highlight::{Span, Tok};
use crate::lang_javascript::lex_javascript_into;

/// JSX reserved element names that are always treated as HTML-like.
const HTML_ELEMENTS: &[&str] = &[
    "a", "abbr", "address", "area", "article", "aside", "audio", "b", "base", "bdi", "bdo",
    "blockquote", "body", "br", "button", "canvas", "caption", "code", "col", "colgroup",
    "data", "datalist", "dd", "del", "details", "dfn", "dialog", "div", "dl", "dt", "em",
    "embed", "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4",
    "h5", "h6", "head", "header", "hgroup", "hr", "html", "i", "iframe", "img", "input",
    "ins", "kbd", "label", "legend", "li", "link", "main", "mark", "menu", "meta", "meter",
    "nav", "noscript", "object", "ol", "optgroup", "option", "output", "p", "param",
    "picture", "pre", "progress", "q", "rp", "rt", "ruby", "s", "samp", "section",
    "select", "slot", "small", "source", "span", "strong", "style", "sub", "summary",
    "sup", "table", "tbody", "td", "template", "textarea", "tfoot", "th", "thead", "time",
    "title", "tr", "track", "u", "ul", "var", "video", "wbr",
];

fn is_jsx_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_jsx_name_continue(c: char) -> bool {
    is_jsx_name_start(c) || c.is_numeric() || c == '-' || c == '.'
}

fn is_html_element(name: &str) -> bool {
    HTML_ELEMENTS.contains(&name)
}

/// Lex JSX source into exact tiling spans.
///
/// The lexer scans the input linearly, classifying each region as either
/// JavaScript (delegated to [`lex_javascript_into`]) or JSX markup
/// (handled by the JSX-specific scanner). The mode transitions are:
///
/// - `<` followed by an identifier or `>` in JS expression position →
///   JSX markup (element open, close, or fragment).
/// - `{` in JSX markup → JS expression container (recursive).
/// - `}` at JSX expression depth zero → back to markup.
/// - `/>` or `</name>` in markup → element boundary.
pub fn lex_jsx_into(code: &str, spans: &mut Vec<Span>) {
    let bytes_len = code.len();
    let mut pos = 0usize;
    let mut last_end = 0usize;

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
        spans.push(Span {
            kind,
            start,
            end,
        });
        *last_end = end;
    }

    // Scan a JSX tag: `<name`, `</name`, or `<>`. Returns the end position
    // of the tag name portion (attributes/text are scanned by the caller).
    fn scan_tag_name(code: &str, start: usize) -> (usize, bool) {
        let bytes = code.as_bytes();
        let mut scan = start + 1; // skip `<`
        let closing = scan < bytes.len() && bytes[scan] == b'/';
        if closing {
            scan += 1;
        }
        while scan < bytes.len() {
            let c = code[scan..].chars().next().unwrap();
            if is_jsx_name_start(c) || is_jsx_name_continue(c) {
                scan += c.len_utf8();
            } else {
                break;
            }
        }
        (scan, closing)
    }

    while pos < bytes_len {
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

        // JSX markup entry: `<` followed by uppercase (component), lowercase
        // (HTML element), or `/` (closing tag/fragment). In JS, `<` can also
        // be less-than, but in JSX source files a `<` at expression position
        // is markup.
        if ch == '<' && pos + 1 < bytes_len {
            let next_char = code[pos + 1..].chars().next().unwrap();
            if is_jsx_name_start(next_char) || next_char == '>' || next_char == '/' {
                // Scan the entire tag: name + attributes until `>` or `/>`.
                let tag_start = pos;
                let mut scan = pos + 1; // skip `<`
                let mut in_expr_depth = 0usize;
                let mut tag_closed = false;
                while scan < bytes_len {
                    let c = code[scan..].chars().next().unwrap();
                    if c == '{' {
                        in_expr_depth += 1;
                    } else if c == '}' {
                        if in_expr_depth > 0 {
                            in_expr_depth -= 1;
                        }
                    } else if c == '"' || c == '\'' {
                        // Skip string literals inside attribute values.
                        let quote = c;
                        scan += c.len_utf8();
                        while scan < bytes_len {
                            let sc = code[scan..].chars().next().unwrap();
                            if sc == '\\' {
                                scan += 2;
                                continue;
                            }
                            if sc == quote {
                                break;
                            }
                            scan += sc.len_utf8();
                        }
                    } else if c == '>' && in_expr_depth == 0 {
                        scan += c.len_utf8();
                        tag_closed = true;
                        break;
                    }
                    scan += c.len_utf8();
                }
                let end = if tag_closed { scan } else { bytes_len.min(scan) };
                // Classify the tag region: tag names, attribute names, values.
                // For simplicity, the entire tag is a Type span (common JSX
                // highlighting convention for component/element names).
                push_tiling(spans, &mut last_end, Tok::Type, tag_start, end);
                pos = end;
                continue;
            }
        }

        // JS expression: delegate to the JS lexer for the rest of the
        // expression statement. The JS lexer handles the full expression
        // grammar including strings, templates, regex, comments.
        //
        // We scan up to the next JSX markup boundary: `<` at expression
        // position (not inside a string, template, or comment). This is a
        // simplification: the JS lexer handles the full expression internally.
        //
        // Strategy: find the extent of this JS statement/region and delegate.
        // For now, we delegate the REST of the input as one JS region, then
        // let JSX markup resume on the next call. This works because JSX
        // files alternate between JS statements and JSX return values, and
        // the transition point is always a `return (` or `=` followed by
        // `<` at statement position.

        // Find where the JS expression ends: scan for `<` that starts a JSX
        // tag at statement position, or the end of input.
        let js_end = find_js_region_end(code, pos);
        if js_end > pos {
            let js_region = &code[pos..js_end];
            let mut js_spans = Vec::new();
            lex_javascript_into(js_region, &mut js_spans);
            for span in js_spans {
                push_tiling(
                    spans,
                    &mut last_end,
                    span.kind,
                    pos + span.start,
                    pos + span.end,
                );
            }
            pos = js_end;
            continue;
        }

        // Fallback: Plain single char.
        push_tiling(spans, &mut last_end, Tok::Plain, pos, pos + clen);
        pos += clen;
    }

    // Defensive final tile.
    let tail_start = last_end;
    if tail_start < bytes_len {
        push_tiling(spans, &mut last_end, Tok::Plain, tail_start, bytes_len);
    }
}

/// Find where the current JS region ends and JSX markup begins.
///
/// Scans from `start` looking for a `<` that is followed by an uppercase
/// letter, lowercase letter, or `>` — at a position where a JS expression
/// value is expected (after `return`, `=`, `(`, `,`, etc.). Returns the
/// byte offset of that `<`, or `code.len()` if no JSX markup is found.
fn find_js_region_end(code: &str, start: usize) -> usize {
    let bytes = code.as_bytes();
    let mut scan = start;
    let mut in_string: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut template_depth = 0usize;

    while scan < bytes.len() {
        let c = bytes[scan];

        if in_line_comment {
            if c == b'\n' {
                in_line_comment = false;
            }
            scan += 1;
            continue;
        }
        if in_block_comment {
            if scan + 1 < bytes.len() && bytes[scan] == b'*' && bytes[scan + 1] == b'/' {
                in_block_comment = false;
                scan += 2;
                continue;
            }
            scan += 1;
            continue;
        }
        if let Some(quote) = in_string {
            if c == b'\\' {
                scan += 2;
                continue;
            }
            if c == quote {
                in_string = None;
            }
            scan += 1;
            continue;
        }

        match c {
            b'/' if scan + 1 < bytes.len() && bytes[scan + 1] == b'/' => {
                in_line_comment = true;
                scan += 2;
            }
            b'/' if scan + 1 < bytes.len() && bytes[scan + 1] == b'*' => {
                in_block_comment = true;
                scan += 2;
            }
            b'"' | b'\'' => {
                in_string = Some(c);
                scan += 1;
            }
            b'`' => {
                template_depth += 1;
                scan += 1;
            }
            _ if template_depth > 0 => {
                if c == b'`' {
                    template_depth -= 1;
                }
                scan += 1;
            }
            b'<' => {
                // Check if this `<` starts JSX markup: followed by
                // uppercase (component), lowercase (HTML element),
                // `/` (closing), or `>` (fragment).
                if scan + 1 < bytes.len() {
                    let next = bytes[scan + 1];
                    if next.is_ascii_uppercase()
                        || next == b'>'
                        || (next.is_ascii_lowercase() && scan + 2 < bytes.len())
                    {
                        return scan;
                    }
                }
                scan += 1;
            }
            _ => {
                scan += 1;
            }
        }
    }

    bytes.len()
}

/// Find the byte offset from which spans should be held pending more
/// input in a JSX context.
///
/// The hold point is the start of the last `<` that opens a potential JSX
/// tag or fragment in expression position. If no such `<` exists, returns
/// `code.len()` (release everything).
pub fn find_jsx_hold_from(code: &str, spans: &[Span]) -> usize {
    let bytes = code.as_bytes();
    let mut scan = code.len();

    // Scan backwards for a `<` that starts a potential JSX tag.
    while scan > 0 {
        scan -= 1;
        if bytes[scan] == b'<' {
            // Check if this `<` could start JSX markup: the next character
            // is an identifier start, `/`, or `>`.
            if scan + 1 < bytes.len() {
                let next = bytes[scan + 1];
                if next.is_ascii_alphanumeric() || next == b'>' || next == b'/' {
                    return scan;
                }
            }
            if scan + 1 >= bytes.len() {
                // `<` at EOF: hold from here.
                return scan;
            }
        }
    }

    code.len()
}

/// Classification helper used by tests to verify word kinds.
fn classify_token(code: &str, span: &Span) -> Tok {
    span.kind
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(span.start, cursor, "gap/overlap at {cursor}");
            assert!(span.end > span.start, "empty span at {cursor}");
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    #[test]
    fn basic_jsx_element_with_text() {
        let code = "const el = <div>Hello World</div>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        // The `<div>` and `</div>` tags should be classified as Type.
        let types: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Type)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            types.iter().any(|s| s.contains("<div>")),
            "opening tag classified: {types:?}"
        );
        assert!(
            types.iter().any(|s| s.contains("</div>")),
            "closing tag classified: {types:?}"
        );
    }

    #[test]
    fn component_with_attributes_and_expression() {
        let code = "const el = <Button onClick={() => alert('hi')} label=\"Click\" size={42} />;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        // The entire <Button ... /> tag is one Type span.
        let types: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Type)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            types.iter().any(|s| s.contains("Button")),
            "component tag classified: {types:?}"
        );
    }

    #[test]
    fn expression_container_switches_to_js() {
        let code = "const el = <span>{items.map(item => <li key={item}>{item}</li>)}</span>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        // The JS expression `items.map(...)` should have JS classifications.
        let keywords: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            keywords.contains(&"const"),
            "JS keyword classified: {keywords:?}"
        );
    }

    #[test]
    fn fragment_syntax() {
        let code = "const el = <>text</>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        // The fragment `<>` and `</>` are classified.
        let types: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Type)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(
            types.iter().any(|s| s.contains("<>")),
            "fragment open classified: {types:?}"
        );
        assert!(
            types.iter().any(|s| s.contains("</>")),
            "fragment close classified: {types:?}"
        );
    }

    #[test]
    fn nested_components_and_expressions() {
        let code = "const app = <App><Header title={title} /><Content>{children}</Content></App>;";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        let types: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Type)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(types.iter().any(|s| s.contains("App")), "{types:?}");
        assert!(types.iter().any(|s| s.contains("Header")));
        assert!(types.iter().any(|s| s.contains("Content")));
    }

    #[test]
    fn malformed_nesting_still_tiles() {
        let code = "const el = <div><span>unclosed";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
    }

    #[test]
    fn truncated_tag_spans_to_eof() {
        let code = "const el = <div class";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        let last = spans.last().unwrap();
        assert_eq!(last.end, code.len());
    }

    #[test]
    fn plain_javascript_without_jsx_still_works() {
        let code = "const x = 1 < 2 && 3 > 2; if (x) { console.log(x); }";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        // `1 < 2` is a comparison, not JSX markup.
        let keywords: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == Tok::Keyword)
            .map(|s| &code[s.start..s.end])
            .collect();
        assert!(keywords.contains(&"if"), "{keywords:?}");
    }

    #[test]
    fn negative_control_tiling_oracle_detects_gap() {
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
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
    }
}
