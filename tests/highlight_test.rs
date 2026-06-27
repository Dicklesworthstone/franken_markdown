//! Tests for the clean-room syntax highlighter. Tests may use `unwrap`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::highlight::{Tok, highlight, is_supported};
use franken_markdown::{HtmlOptions, render_html};

#[test]
fn common_languages_are_supported() {
    for l in [
        "rust",
        "rs",
        "python",
        "py",
        "javascript",
        "js",
        "ts",
        "json",
        "bash",
        "sh",
        "go",
        "c",
        "cpp",
        "sql",
        "toml",
        "yaml",
    ] {
        assert!(is_supported(l), "expected a lexer for {l}");
    }
    assert!(!is_supported("brainfuck"));
}

#[test]
fn spans_tile_the_input_exactly_and_contiguously() {
    let code = "fn main() {\n    let x = 1; // hi\n    let s = \"a\\\"b\";\n}\n";
    let spans = highlight("rust", code);
    let mut pos = 0;
    for s in &spans {
        assert_eq!(s.start, pos, "spans must be contiguous");
        assert!(s.end >= s.start);
        pos = s.end;
    }
    assert_eq!(pos, code.len(), "spans must cover every byte");
}

#[test]
fn rust_tokens_are_classified() {
    let spans = highlight("rust", "let x = \"hi\"; // c");
    let kinds: Vec<Tok> = spans.iter().map(|s| s.kind).collect();
    assert!(kinds.contains(&Tok::Keyword)); // let
    assert!(kinds.contains(&Tok::Str)); // "hi"
    assert!(kinds.contains(&Tok::Comment)); // // c
}

#[test]
fn unknown_language_is_a_single_plain_span() {
    let spans = highlight("nope", "anything here");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, Tok::Plain);
}

#[test]
fn html_code_block_gets_token_spans_and_css() {
    let md = "```rust\nfn main() { let s = 1; }\n```\n";
    let html = render_html(md, &HtmlOptions::default()).unwrap();
    assert!(html.contains("<span class=\"tok-kw\">fn</span>"));
    assert!(html.contains(".tok-kw"));
    assert!(html.contains("class=\"language-rust\""));
}

#[test]
fn highlighted_content_is_still_html_escaped() {
    let html = render_html("```js\nlet h = \"<b>&\";\n```\n", &HtmlOptions::default()).unwrap();
    assert!(html.contains("&lt;b&gt;&amp;"));
    assert!(!html.contains("\"<b>&\""));
}

#[test]
fn unknown_language_code_block_falls_back_to_plain_escaped() {
    let html = render_html("```nope\n1 < 2 && 3\n```\n", &HtmlOptions::default()).unwrap();
    assert!(html.contains("1 &lt; 2 &amp;&amp; 3"));
    assert!(!html.contains("<span class=\"tok-"));
}
