//! Tests for the clean-room syntax highlighter. Tests may use `unwrap`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use franken_markdown::highlight::{Tok, highlight, is_supported};
use franken_markdown::{HtmlOptions, render_html};

fn assert_spans_tile(lang: &str, code: &str) {
    let spans = highlight(lang, code);
    let mut pos = 0;
    for s in &spans {
        assert_eq!(s.start, pos, "{lang} spans must be contiguous");
        assert!(s.end >= s.start);
        pos = s.end;
    }
    assert_eq!(pos, code.len(), "{lang} spans must cover every byte");
}

fn escaped_token_text(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

fn serialized_tokens(lang: &str, code: &str) -> String {
    let mut out = String::new();
    for span in highlight(lang, code) {
        let text = code.get(span.start..span.end).unwrap();
        out.push_str(&format!("{:?}\t{}\n", span.kind, escaped_token_text(text)));
    }
    out
}

fn kind_code(kind: Tok) -> u64 {
    match kind {
        Tok::Plain => 1,
        Tok::Keyword => 2,
        Tok::Type => 3,
        Tok::Func => 4,
        Tok::Str => 5,
        Tok::Number => 6,
        Tok::Comment => 7,
        Tok::Operator => 8,
        Tok::Punct => 9,
    }
}

fn highlight_stress_digest(rounds: usize) -> (usize, usize, u64) {
    let samples = [
        ("rust", include_str!("fixtures/highlight/rust.code")),
        ("html", include_str!("fixtures/highlight/html.code")),
        ("css", include_str!("fixtures/highlight/css.code")),
        ("markdown", include_str!("fixtures/highlight/markdown.code")),
        (
            "not-a-language",
            include_str!("fixtures/highlight/unknown.code"),
        ),
    ];
    let mut bytes = 0usize;
    let mut spans = 0usize;
    let mut digest = 0xcbf2_9ce4_8422_2325u64;

    for _ in 0..rounds {
        for (lang, source) in samples {
            bytes += source.len();
            for span in highlight(lang, source) {
                spans += 1;
                digest ^= kind_code(span.kind);
                digest = digest.wrapping_mul(0x100_0000_01b3);
                digest ^= span.start as u64;
                digest = digest.wrapping_mul(0x100_0000_01b3);
                digest ^= span.end as u64;
                digest = digest.wrapping_mul(0x100_0000_01b3);
            }
        }
    }

    (bytes, spans, digest)
}

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
        "html",
        "xhtml",
        "xml",
        "css",
        "scss",
        "sass",
        "markdown",
        "md",
    ] {
        assert!(is_supported(l), "expected a lexer for {l}");
    }
    assert!(!is_supported("brainfuck"));
}

#[test]
fn language_metadata_still_selects_the_supported_lexer() {
    assert!(is_supported("rust,no_run"));
    assert!(is_supported("language-rust,ignore"));
    assert!(is_supported("python linenums"));
    assert!(is_supported("c++,17"));
    assert!(!is_supported("rusty,no_run"));

    let html = render_html(
        "```rust,no_run\nfn main() {}\n```\n",
        &HtmlOptions::default(),
    )
    .unwrap();
    assert!(html.contains("class=\"language-rust,no_run\""));
    assert!(html.contains("<span class=\"tok-kw\">fn</span>"));
    assert!(html.contains("<span class=\"tok-fn\">main</span>"));
}

#[test]
fn approved_highlight_token_fixtures_match() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/highlight");
    for (name, lang) in [
        ("rust", "rust"),
        ("html", "html"),
        ("css", "css"),
        ("markdown", "markdown"),
        ("unknown", "not-a-language"),
    ] {
        let code = fs::read_to_string(root.join(format!("{name}.code"))).unwrap();
        let expected = fs::read_to_string(root.join(format!("{name}.tokens"))).unwrap();
        let actual = serialized_tokens(lang, &code);
        assert_eq!(
            expected, actual,
            "highlight token fixture drifted for {name}"
        );
    }
}

#[test]
fn large_mixed_language_highlight_stress_is_deterministic() {
    let first = highlight_stress_digest(512);
    let second = highlight_stress_digest(512);

    assert_eq!(first, second, "highlight stress digest must be stable");
    assert!(
        first.0 > 100_000,
        "stress proof should exercise a non-trivial byte volume"
    );
    assert!(
        first.1 > 20_000,
        "stress proof should exercise many token spans"
    );
}

#[test]
fn spans_tile_the_input_exactly_and_contiguously() {
    let code = "fn main() {\n    let x = 1; // hi\n    let s = \"a\\\"b\";\n}\n";
    assert_spans_tile("rust", code);
    assert_spans_tile(
        "html",
        "<!-- snowman: ☃ -->\n<section class=\"hero\">Hi &amp; bye</section>\n",
    );
    assert_spans_tile("css", ".hero { color: #0a3069; content: \"☃\"; }\n");
    assert_spans_tile(
        "markdown",
        "# Title\n\n> quote\n\n- item with `code` and [link](url)\n",
    );
    assert_spans_tile("markdown", "inline #anchor and a > b stay plain\n");
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
fn html_css_and_markdown_tokens_are_classified() {
    let html = highlight("html", "<!-- c --><section class=\"hero\">Hi</section>");
    let html_kinds: Vec<Tok> = html.iter().map(|s| s.kind).collect();
    assert!(html_kinds.contains(&Tok::Comment));
    assert!(html_kinds.contains(&Tok::Keyword)); // section
    assert!(html_kinds.contains(&Tok::Type)); // class
    assert!(html_kinds.contains(&Tok::Str)); // "hero"

    let css = highlight(
        "css",
        "@media screen { .hero { color: #0a3069; display: flex; } } /* c */",
    );
    let css_kinds: Vec<Tok> = css.iter().map(|s| s.kind).collect();
    assert!(css_kinds.contains(&Tok::Keyword)); // @media / flex
    assert!(css_kinds.contains(&Tok::Type)); // selector / property
    assert!(css_kinds.contains(&Tok::Number)); // #0a3069
    assert!(css_kinds.contains(&Tok::Comment));

    let md = highlight(
        "markdown",
        "# Title\n\n> quote\n\n- item with `code` and [link](url)",
    );
    let md_kinds: Vec<Tok> = md.iter().map(|s| s.kind).collect();
    assert!(md_kinds.contains(&Tok::Keyword)); // heading marker
    assert!(md_kinds.contains(&Tok::Operator)); // blockquote/list/code markers
    assert!(md_kinds.contains(&Tok::Str)); // code span
    assert!(md_kinds.contains(&Tok::Punct)); // link punctuation
}

#[test]
fn html_comparison_text_is_not_treated_as_a_fake_tag() {
    let code = "a < b && c > d\n";
    assert_spans_tile("html", code);

    let kinds: Vec<Tok> = highlight("html", code).iter().map(|s| s.kind).collect();
    assert!(
        !kinds.contains(&Tok::Keyword),
        "plain comparison text must not produce fake tag-name tokens"
    );
    assert!(
        !kinds.contains(&Tok::Type),
        "plain comparison text must not produce fake attribute tokens"
    );
}

#[test]
fn markdown_ordered_list_marker_requires_padding() {
    let typo = highlight("markdown", "1.foo stays text\n");
    assert!(
        !typo.iter().any(|s| s.kind == Tok::Operator),
        "ordered marker highlighting must not treat `1.foo` as a list marker"
    );

    let list = highlight("markdown", "1. item\n");
    assert!(
        list.iter().any(|s| s.kind == Tok::Operator),
        "valid ordered list markers should still be highlighted"
    );
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
fn html_css_and_markdown_render_escaped_token_spans() {
    let html = render_html(
        "```html\n<section class=\"hero\">&</section>\n```\n\n\
         ```css\n.hero { color: #0a3069; }\n```\n\n\
         ```markdown\n# Title with `code`\n```\n",
        &HtmlOptions::default(),
    )
    .unwrap();

    assert!(html.contains("class=\"language-html\""));
    assert!(
        html.contains("<span class=\"tok-op\">&lt;</span><span class=\"tok-kw\">section</span>")
    );
    assert!(html.contains("<span class=\"tok-ty\">class</span>"));
    assert!(html.contains("<span class=\"tok-st\">\"hero\"</span>"));
    assert!(!html.contains("<section class=\"hero\">&</section>"));

    assert!(html.contains("class=\"language-css\""));
    assert!(html.contains("<span class=\"tok-ty\">hero</span>"));
    assert!(html.contains("<span class=\"tok-nu\">#0a3069</span>"));

    assert!(html.contains("class=\"language-markdown\""));
    assert!(html.contains(
        "<span class=\"tok-kw\">#</span> Title with <span class=\"tok-st\">`code`</span>"
    ));
}

#[test]
fn unknown_language_code_block_falls_back_to_plain_escaped() {
    let html = render_html("```nope\n1 < 2 && 3\n```\n", &HtmlOptions::default()).unwrap();
    assert!(html.contains("1 &lt; 2 &amp;&amp; 3"));
    assert!(!html.contains("<span class=\"tok-"));
}
