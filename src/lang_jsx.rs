#![forbid(unsafe_code)]

//! JSX incremental lexer (FCB-022 · fcb-9vx.8).
//!
//! JSX markup and JavaScript expressions have distinct lexical modes. Tags
//! are highlighted as types, child text stays plain, and expression containers
//! (including attributes) are delegated to the existing JavaScript lexer.
//! Elements, fragments, nested components, and incomplete input all retain
//! byte-exact, nonempty spans on UTF-8 boundaries.
//!
//! Markup is entered only where a JavaScript value is expected: comparisons
//! such as `left<right` remain JavaScript. Strings, comments, regular expressions,
//! and templates do not accidentally open tags. JavaScript after a completed
//! JSX value retains value context, including division operators.
//!
//! The scanner uses bounded explicit stacks, not recursive parsing. Excessive
//! expression nesting falls back to plain text without dropping input. This is
//! a highlighter, not a JSX validator: mismatched tag names are not diagnosed.
//! JSX embedded inside template substitutions retains the JavaScript lexer's
//! template classification rather than being recursively parsed as markup.

use crate::highlight::Span;

#[path = "lang_jsx_scan.rs"]
mod scan;

/// Append source-relative spans that tile the entire JSX input exactly.
/// Existing entries in `spans` are retained, matching the other lexer APIs.
pub fn lex_jsx_into(code: &str, spans: &mut Vec<Span>) {
    scan::scan(code, Some(spans));
}

/// Return the earliest still-open JSX context that must await more input.
///
/// Retaining its opener preserves tag/text/expression state when a streaming
/// host reparses the pending suffix. Complete JSX trees, angle brackets in
/// literals/comments, and ordinary comparisons do not hold completed markup.
/// This is the JSX-specific hold policy; JavaScript token finalization remains
/// the host's responsibility. The supplied spans are accepted for compatibility
/// with the language hold-point API; structural scanning does not depend on
/// token colors or caller-provided span validity.
pub fn find_jsx_hold_from(code: &str, _spans: &[Span]) -> usize {
    scan::scan(code, None)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::highlight::Tok;
    use crate::lang_javascript::lex_javascript_into;

    fn assert_tiling(code: &str, spans: &[Span]) {
        let mut cursor = 0usize;
        for span in spans {
            assert_eq!(span.start, cursor, "gap/overlap at {cursor}");
            assert!(span.end > span.start, "empty span at {cursor}");
            assert!(span.end <= code.len(), "span exceeds source length");
            assert!(code.is_char_boundary(span.start));
            assert!(code.is_char_boundary(span.end));
            cursor = span.end;
        }
        assert_eq!(cursor, code.len(), "spans do not reach end of input");
    }

    fn tokens(code: &str, kind: Tok) -> Vec<&str> {
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        spans
            .into_iter()
            .filter(|span| span.kind == kind)
            .map(|span| &code[span.start..span.end])
            .collect()
    }

    #[test]
    fn basic_jsx_element_with_text() {
        let code = "const el = <div>Hello World</div>;";
        let types = tokens(code, Tok::Type);
        assert!(types.contains(&"<div>"), "opening tag: {types:?}");
        assert!(types.contains(&"</div>"), "closing tag: {types:?}");
        assert!(tokens(code, Tok::Plain).contains(&"Hello World"));
    }

    #[test]
    fn component_with_attributes_and_expression() {
        let code = "const el = <Button onClick={() => alert('hi')} label=\"Click\" size={42} />;";
        assert!(tokens(code, Tok::Type).iter().any(|token| token.contains("Button")));
        assert!(tokens(code, Tok::Func).contains(&"alert"));
        assert!(tokens(code, Tok::Number).contains(&"42"));
        assert!(tokens(code, Tok::Str).contains(&"'hi'"));
    }

    #[test]
    fn expression_container_switches_to_js() {
        let code = "const el = <span>{items.map(item => <li key={item}>{item}</li>)}</span>;";
        assert!(tokens(code, Tok::Keyword).contains(&"const"));
        assert!(tokens(code, Tok::Func).contains(&"map"));
        let types = tokens(code, Tok::Type);
        assert!(types.contains(&"</li>"));
        assert!(types.contains(&"</span>"));
    }

    #[test]
    fn fragment_syntax() {
        let code = "const el = <>text</>;";
        let types = tokens(code, Tok::Type);
        assert!(types.contains(&"<>"), "fragment opener: {types:?}");
        assert!(types.contains(&"</>"), "fragment closer: {types:?}");
        assert!(tokens(code, Tok::Plain).contains(&"text"));
    }

    #[test]
    fn nested_components_and_expressions() {
        let code = "const app = <App><Header title={title} /><Content>{children}</Content></App>;";
        let types = tokens(code, Tok::Type);
        for name in ["App", "Header", "Content"] {
            assert!(types.iter().any(|token| token.contains(name)), "{name}: {types:?}");
        }
        assert!(types.contains(&"</Content>"));
        assert!(types.contains(&"</App>"));
    }

    #[test]
    fn malformed_nesting_still_tiles() {
        for code in ["const el = <div><span>unclosed", "<A><B></A>", "<A>{value"] {
            let mut spans = Vec::new();
            lex_jsx_into(code, &mut spans);
            assert_tiling(code, &spans);
        }
    }

    #[test]
    fn truncated_tag_spans_to_eof() {
        let code = "const el = <div class";
        let mut spans = Vec::new();
        lex_jsx_into(code, &mut spans);
        assert_tiling(code, &spans);
        assert_eq!(spans.last().unwrap().end, code.len());
        assert_eq!(find_jsx_hold_from(code, &spans), code.find('<').unwrap());
    }

    #[test]
    fn plain_javascript_without_jsx_still_works() {
        for code in [
            "const x = 1 < 2 && 3 > 2; if (x) { console.log(x); }",
            "const x = left<right && right>left;",
            "const x = /<div>/; const y = '<span>'; /* <A> */",
            "const x = `<A>${`nested`}</A>`;",
        ] {
            let mut jsx = Vec::new();
            let mut javascript = Vec::new();
            lex_jsx_into(code, &mut jsx);
            lex_javascript_into(code, &mut javascript);
            assert_tiling(code, &jsx);
            assert_eq!(jsx.len(), javascript.len());
            for (actual, expected) in jsx.iter().zip(&javascript) {
                assert_eq!(actual.kind, expected.kind);
                assert_eq!((actual.start, actual.end), (expected.start, expected.end));
            }
        }
    }

    #[test]
    fn negative_control_tiling_oracle_detects_gap() {
        let code = "abc";
        let gapped = vec![
            Span { kind: Tok::Plain, start: 0, end: 1 },
            Span { kind: Tok::Plain, start: 2, end: 3 },
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
