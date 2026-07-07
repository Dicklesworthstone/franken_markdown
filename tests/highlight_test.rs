//! Tests for the clean-room syntax highlighter. Tests may use `unwrap`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use franken_markdown::highlight::{Span, Tok, highlight, highlight_into, is_supported};
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

fn assert_highlight_into_matches_highlight(lang: &str, code: &str) {
    let expected = highlight(lang, code);
    let mut actual = Vec::new();
    highlight_into(lang, code, &mut actual);

    assert_eq!(
        actual.len(),
        expected.len(),
        "span count drift for language {lang:?}"
    );
    for (idx, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(
            (actual.kind, actual.start, actual.end),
            (expected.kind, expected.start, expected.end),
            "span {idx} drift for language {lang:?}"
        );
    }
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

/// True when highlighting `code` for `lang` yields at least one span of `kind`
/// whose exact tokenized substring equals `text`.
fn has_span(lang: &str, code: &str, kind: Tok, text: &str) -> bool {
    highlight(lang, code)
        .iter()
        .any(|s| s.kind == kind && &code[s.start..s.end] == text)
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
        "mermaid",
        "mmd",
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
fn language_metadata_accepts_mixed_case_language_keys() {
    assert!(is_supported("RuSt,no_run"));
    assert!(is_supported("PyThOn linenums"));
    assert!(is_supported("C++,17"));
    assert!(is_supported("JSONC:metadata"));
    assert!(!is_supported("RuSty,no_run"));

    let rust = "fn main() { let answer = 42; }\n";
    assert_eq!(
        serialized_tokens("rust", rust),
        serialized_tokens("RuSt,no_run", rust)
    );

    let python = "def main():\n    return True\n";
    assert_eq!(
        serialized_tokens("python", python),
        serialized_tokens("PyThOn linenums", python)
    );
}

#[test]
fn language_metadata_accepts_mixed_case_language_prefixes() {
    assert!(is_supported("LANGUAGE-RuSt,no_run"));
    assert!(is_supported("LaNgUaGe-PyThOn linenums"));
    assert!(is_supported("LANGUAGE-C++,17"));
    assert!(!is_supported("LANGUAGE-RuSty,no_run"));

    let rust = "fn main() { let answer = 42; }\n";
    assert_eq!(
        serialized_tokens("rust", rust),
        serialized_tokens("LANGUAGE-RuSt,no_run", rust)
    );

    let cpp = "#include <vector>\nint main() { return 0; }\n";
    assert_eq!(
        serialized_tokens("c++", cpp),
        serialized_tokens("LANGUAGE-C++,17", cpp)
    );
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
fn markdown_block_markers_match_parser_start_rules() {
    for code in [
        "    # indented code, not a heading\n",
        "\t- indented code, not a list\n",
        " \t> indented code, not a quote\n",
    ] {
        assert_spans_tile("markdown", code);
        assert!(
            highlight("markdown", code)
                .iter()
                .all(|span| span.kind == Tok::Plain),
            "four-column Markdown code must stay plain: {code:?}"
        );
    }

    for code in ["#notheading\n", "####### too many hashes\n"] {
        assert_spans_tile("markdown", code);
        assert!(
            !highlight("markdown", code)
                .iter()
                .any(|span| span.kind == Tok::Keyword),
            "invalid ATX heading marker must not be highlighted: {code:?}"
        );
    }

    for code in ["\u{00a0}# not heading\n", "\u{2003}> not quote\n"] {
        assert_spans_tile("markdown", code);
        assert!(
            highlight("markdown", code)
                .iter()
                .all(|span| span.kind == Tok::Plain),
            "only spaces and tabs count as Markdown indentation: {code:?}"
        );
    }

    let overlong_ordered = "1234567890. too many digits\n";
    assert_spans_tile("markdown", overlong_ordered);
    assert!(
        !has_span("markdown", overlong_ordered, Tok::Operator, "1234567890."),
        "ordered list markers must be capped at nine digits"
    );

    assert!(has_span("markdown", "   # valid\n", Tok::Keyword, "#"));
    assert!(has_span("markdown", "   > valid\n", Tok::Operator, ">"));
    assert!(has_span(
        "markdown",
        "   123456789. valid\n",
        Tok::Operator,
        "123456789."
    ));

    let after_indented_comment = "    <!-- note -->\n# valid after comment\n";
    assert_spans_tile("markdown", after_indented_comment);
    assert!(has_span(
        "markdown",
        after_indented_comment,
        Tok::Keyword,
        "#"
    ));
}

#[test]
fn unknown_language_is_a_single_plain_span() {
    let spans = highlight("nope", "anything here");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, Tok::Plain);
}

#[test]
fn highlight_into_matches_highlight_for_supported_and_unknown_languages() {
    for (lang, code) in [
        ("rust", "fn main() { let s = \"hi\"; }\n"),
        ("html", "<section class=\"hero\">Hi</section>\n"),
        ("css", ".hero { color: #0a3069; display: flex; }\n"),
        ("markdown", "# Title\n\n- item with `code`\n"),
        ("mermaid", "flowchart TD\nA --> B\n"),
        ("sql", "SELECT id FROM users WHERE age > 18;\n"),
        ("not-a-language", "plain <text> & symbols\n"),
    ] {
        assert_highlight_into_matches_highlight(lang, code);
    }
}

#[test]
fn highlight_into_clears_stale_spans() {
    let stale = Span {
        kind: Tok::Comment,
        start: 99,
        end: 123,
    };
    let mut spans = vec![stale; 3];

    highlight_into("rust", "", &mut spans);
    assert!(
        spans.is_empty(),
        "supported empty input should remove all stale spans"
    );

    spans.push(stale);
    highlight_into("not-a-language", "plain", &mut spans);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, Tok::Plain);
    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, "plain".len());
}

#[test]
fn highlight_into_preserves_reusable_capacity() {
    let mut spans = Vec::with_capacity(64);
    let capacity = spans.capacity();

    highlight_into("rust", "fn main() { let s = \"hi\"; }\n", &mut spans);
    assert!(!spans.is_empty());
    assert_eq!(
        spans.capacity(),
        capacity,
        "small supported highlight should reuse caller capacity"
    );

    highlight_into("not-a-language", "plain", &mut spans);
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans.capacity(),
        capacity,
        "unknown-language fallback should reuse caller capacity"
    );
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
fn mermaid_render_uses_shared_highlighter() {
    let html = render_html(
        "```mermaid\nflowchart TD\n    A[Markdown] --> B[AST]\n    B -.-> C[PDF]\n```\n",
        &HtmlOptions::default(),
    )
    .unwrap();

    assert!(html.contains("class=\"language-mermaid\""));
    assert!(html.contains("<span class=\"tok-kw\">flowchart</span>"));
    assert!(html.contains("<span class=\"tok-ty\">TD</span>"));
    assert!(html.contains("<span class=\"tok-op\">--&gt;</span>"));
    assert!(html.contains("<span class=\"tok-op\">-.-&gt;</span>"));
}

#[test]
fn unknown_language_code_block_falls_back_to_plain_escaped() {
    let html = render_html("```nope\n1 < 2 && 3\n```\n", &HtmlOptions::default()).unwrap();
    assert!(html.contains("1 &lt; 2 &amp;&amp; 3"));
    assert!(!html.contains("<span class=\"tok-"));
}

// ---------------------------------------------------------------------------
// Token -> CSS class mapping (every variant, incl. the Comment arm).
// ---------------------------------------------------------------------------

#[test]
fn tok_css_classes_cover_every_variant() {
    assert_eq!(Tok::Plain.css_class(), None);
    assert_eq!(Tok::Keyword.css_class(), Some("tok-kw"));
    assert_eq!(Tok::Type.css_class(), Some("tok-ty"));
    assert_eq!(Tok::Func.css_class(), Some("tok-fn"));
    assert_eq!(Tok::Str.css_class(), Some("tok-st"));
    assert_eq!(Tok::Number.css_class(), Some("tok-nu"));
    assert_eq!(Tok::Comment.css_class(), Some("tok-cm"));
    assert_eq!(Tok::Operator.css_class(), Some("tok-op"));
    assert_eq!(Tok::Punct.css_class(), Some("tok-pn"));
}

// ---------------------------------------------------------------------------
// Generic lexer branches (exercised through Rust, which has a full rule set).
// ---------------------------------------------------------------------------

#[test]
fn generic_block_comments_terminated_and_unterminated() {
    let closed = "let a = 1; /* a block */ let b = 2;";
    assert_spans_tile("rust", closed);
    assert!(has_span("rust", closed, Tok::Comment, "/* a block */"));

    // No closing delimiter: the block comment runs to end-of-input.
    let open = "x /* never closed";
    assert_spans_tile("rust", open);
    assert!(has_span("rust", open, Tok::Comment, "/* never closed"));
}

#[test]
fn generic_unterminated_string_runs_to_eof() {
    let code = "let s = \"no closing quote";
    assert_spans_tile("rust", code);
    assert!(has_span("rust", code, Tok::Str, "\"no closing quote"));
}

#[test]
fn known_cosmetic_limitations_never_drop_bytes() {
    // Documented, intentional cosmetic limitations (see the highlight module
    // docs): a JS regex literal is tinted as division and a stray quote inside
    // it can open a spurious string span. This is byte-preserving — the spans
    // still tile the exact input, so no source character is ever added, dropped,
    // or reordered. That invariant is what actually matters for correctness.
    for src in [
        "const re = /ab'c/g;\nconst n = 1;\n", // regex containing a quote
        "let x = a / b / c;\n",                // real division stays fine
        "x = \"oops\nnext line\n",             // unterminated string
    ] {
        assert_spans_tile("js", src);
    }
}

#[test]
fn generic_string_backslash_escape_is_consumed() {
    // The escaped quote must not terminate the string literal.
    let code = "let s = \"a\\\"b\";";
    assert_spans_tile("rust", code);
    assert!(has_span("rust", code, Tok::Str, "\"a\\\"b\""));
}

#[test]
fn generic_numeric_literal_variants() {
    for (code, num) in [
        ("let a = 0xFF;", "0xFF"),
        ("let b = 3.14;", "3.14"),
        ("let c = 1_000_000;", "1_000_000"),
        ("let d = 0b1010;", "0b1010"),
        ("let e = 0o17;", "0o17"),
        ("let f = 2.5e10;", "2.5e10"),
    ] {
        assert_spans_tile("rust", code);
        assert!(has_span("rust", code, Tok::Number, num), "number {num}");
    }

    // Number that runs to end-of-input (loop exits via the length guard).
    let eof = "99";
    assert_spans_tile("rust", eof);
    assert!(has_span("rust", eof, Tok::Number, "99"));
}

#[test]
fn generic_identifier_underscore_start_and_eof() {
    // Leading underscore, interior underscores + digits, runs to EOF, no '('
    // afterwards -> a single Plain identifier span.
    let code = "_private_value1";
    let spans = highlight("rust", code);
    assert_spans_tile("rust", code);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].kind, Tok::Plain);
    assert_eq!(&code[spans[0].start..spans[0].end], "_private_value1");
}

#[test]
fn generic_type_detection_known_and_uppercase_heuristic() {
    let code = "let v: Vec<i32> = MyStruct::new();";
    assert_spans_tile("rust", code);
    assert!(has_span("rust", code, Tok::Type, "Vec")); // in the type table
    assert!(has_span("rust", code, Tok::Type, "i32")); // in the type table
    assert!(has_span("rust", code, Tok::Type, "MyStruct")); // uppercase heuristic
    assert!(has_span("rust", code, Tok::Func, "new")); // followed by '('
}

#[test]
fn generic_empty_type_table_skips_uppercase_heuristic() {
    // Bash has no type table: an uppercase word must stay Plain, not become Type.
    let code = "echo $HOME";
    assert_spans_tile("bash", code);
    let kinds: Vec<Tok> = highlight("bash", code).iter().map(|s| s.kind).collect();
    assert!(
        !kinds.contains(&Tok::Type),
        "an empty type table must not classify uppercase words as types"
    );
    assert!(has_span("bash", code, Tok::Plain, "HOME"));
    // '$' is neither operator nor punctuation in the generic table -> Plain.
    assert!(has_span("bash", code, Tok::Plain, "$"));
}

#[test]
fn generic_operator_punctuation_and_stray_plain_dispatch() {
    let code = "let total = a + b * 2; # $";
    assert_spans_tile("rust", code);
    assert!(has_span("rust", code, Tok::Operator, "=")); // operator class
    assert!(has_span("rust", code, Tok::Operator, "+"));
    assert!(has_span("rust", code, Tok::Operator, "*"));
    assert!(has_span("rust", code, Tok::Punct, ";")); // punctuation class
    // '#' and '$' fall through both tables -> Plain.
    assert!(has_span("rust", code, Tok::Plain, "#"));
    assert!(has_span("rust", code, Tok::Plain, "$"));
}

// ---------------------------------------------------------------------------
// HTML lexer branches.
// ---------------------------------------------------------------------------

#[test]
fn html_self_closing_and_quote_variants() {
    let code = "<img src='logo.png' alt=\"x\"/>";
    assert_spans_tile("html", code);
    assert!(has_span("html", code, Tok::Operator, "/>")); // self-closing
    assert!(has_span("html", code, Tok::Str, "'logo.png'")); // single-quoted attr
    assert!(has_span("html", code, Tok::Str, "\"x\"")); // double-quoted attr
    assert!(has_span("html", code, Tok::Keyword, "img")); // tag name
    assert!(has_span("html", code, Tok::Type, "src")); // attribute name
    assert!(has_span("html", code, Tok::Type, "alt"));
}

#[test]
fn html_unterminated_tag_runs_attribute_loop_to_eof() {
    let code = "<div class=\"main\"";
    assert_spans_tile("html", code);
    assert!(has_span("html", code, Tok::Keyword, "div"));
    assert!(has_span("html", code, Tok::Type, "class"));
    assert!(has_span("html", code, Tok::Str, "\"main\""));
}

#[test]
fn html_unusual_character_inside_tag_is_punctuation() {
    let code = "<button @click=\"go\">x</button>";
    assert_spans_tile("html", code);
    assert!(has_span("html", code, Tok::Punct, "@")); // not a name/quote/op char
    assert!(has_span("html", code, Tok::Keyword, "button"));
    assert!(has_span("html", code, Tok::Type, "click"));
}

#[test]
fn html_truncated_brackets_at_eof_are_not_tags() {
    // A lone '<' at end-of-input is not a tag opener.
    let lt = "text <";
    assert_spans_tile("html", lt);
    assert!(has_span("html", lt, Tok::Operator, "<"));

    // '<' + '/' at end-of-input is also not a (closing) tag.
    let slash = "text </";
    assert_spans_tile("html", slash);
    assert!(has_span("html", slash, Tok::Operator, "<"));
}

#[test]
fn html_unterminated_comment_runs_to_eof() {
    let code = "<!-- no end";
    assert_spans_tile("html", code);
    assert!(has_span("html", code, Tok::Comment, "<!-- no end"));
}

// ---------------------------------------------------------------------------
// CSS lexer branches.
// ---------------------------------------------------------------------------

#[test]
fn css_combinators_and_numeric_units() {
    let code = "a > b + c ~ d { width: 1.5em; line-height: 100%; z-index: 5; }";
    assert_spans_tile("css", code);
    assert!(has_span("css", code, Tok::Operator, ">"));
    assert!(has_span("css", code, Tok::Operator, "+"));
    assert!(has_span("css", code, Tok::Operator, "~"));
    assert!(has_span("css", code, Tok::Number, "1.5em")); // is_css_number_char: '.', alpha
    assert!(has_span("css", code, Tok::Number, "100%")); // is_css_number_char: '%'
    assert!(has_span("css", code, Tok::Number, "5"));
}

#[test]
fn css_string_quotes_escapes_and_unterminated() {
    let single = ".x { content: 'hi'; }";
    assert_spans_tile("css", single);
    assert!(has_span("css", single, Tok::Str, "'hi'")); // single-quoted

    // Backslash escape inside a double-quoted value must not end the string.
    let esc = ".x { content: \"a\\\"b\"; }";
    assert_spans_tile("css", esc);
    assert!(has_span("css", esc, Tok::Str, "\"a\\\"b\""));

    let unterm = ".x { content: \"oops";
    assert_spans_tile("css", unterm);
    assert!(has_span("css", unterm, Tok::Str, "\"oops"));
}

#[test]
fn css_id_selector_that_is_not_a_hex_color() {
    // `#main` is not a 3/4/6/8-digit hex color, so '#' falls through to Plain
    // and `main` is lexed as a normal identifier.
    let code = "#main { color: red; }";
    assert_spans_tile("css", code);
    assert!(has_span("css", code, Tok::Plain, "#"));
    assert!(has_span("css", code, Tok::Type, "color")); // property before ':'
}

#[test]
fn css_identifier_classification_paths() {
    // Property name immediately before ':' -> Type.
    let prop = "x { color: red; }";
    assert!(has_span("css", prop, Tok::Type, "color"));

    // Identifier directly after '.'/'#' selector punctuation -> Type.
    let sel = ".hero { }";
    assert!(has_span("css", sel, Tok::Type, "hero"));

    // Bare identifier at the very start, not a property and not a selector -> Plain
    // (it merges with the following whitespace into one Plain run).
    let bare = "div { }";
    assert_spans_tile("css", bare);
    let bare_spans = highlight("css", bare);
    assert!(
        bare_spans
            .iter()
            .any(|s| s.kind == Tok::Plain && bare[s.start..s.end].starts_with("div")),
        "bare identifier must be Plain"
    );
    assert!(
        !bare_spans
            .iter()
            .any(|s| s.kind == Tok::Type && bare[s.start..s.end].contains("div")),
        "bare identifier must not be a Type"
    );

    // Identifiers followed only by whitespace to EOF: next_non_space_is finds no ':'
    // and there is no selector prefix, so the whole input collapses to one Plain run.
    let trailing = "a b";
    assert_spans_tile("css", trailing);
    let trailing_spans = highlight("css", trailing);
    assert_eq!(trailing_spans.len(), 1);
    assert_eq!(trailing_spans[0].kind, Tok::Plain);
    assert_eq!(
        &trailing[trailing_spans[0].start..trailing_spans[0].end],
        "a b"
    );
}

// ---------------------------------------------------------------------------
// Markdown lexer branches.
// ---------------------------------------------------------------------------

#[test]
fn markdown_html_comment_block() {
    let code = "<!-- note -->\nplain text";
    assert_spans_tile("markdown", code);
    assert!(has_span("markdown", code, Tok::Comment, "<!-- note -->"));
}

#[test]
fn markdown_leading_whitespace_and_plain_to_eof() {
    // Leading indentation is consumed at line start; the following bullet marker
    // keeps it as its own Plain run, and the item text then runs to end-of-input.
    let code = "   - item runs to eof";
    assert_spans_tile("markdown", code);
    assert!(has_span("markdown", code, Tok::Plain, "   ")); // leading indentation
    assert!(has_span("markdown", code, Tok::Operator, "-")); // bullet marker
    assert!(has_span("markdown", code, Tok::Plain, " item runs to eof")); // plain to EOF
}

#[test]
fn markdown_adjacent_plain_spans_merge() {
    // Text, newline, indentation and trailing text are all Plain and contiguous;
    // push_span must coalesce them into a single span.
    let code = "x\n  y";
    assert_spans_tile("markdown", code);
    let spans = highlight("markdown", code);
    assert_eq!(spans.len(), 1, "adjacent Plain spans must merge");
    assert_eq!(spans[0].kind, Tok::Plain);
    assert_eq!(&code[spans[0].start..spans[0].end], "x\n  y");
}

#[test]
fn markdown_emphasis_marker_runs() {
    let code = "this is **bold** and _em_ and ~~strike~~ text";
    assert_spans_tile("markdown", code);
    assert!(has_span("markdown", code, Tok::Operator, "**"));
    assert!(has_span("markdown", code, Tok::Operator, "_"));
    assert!(has_span("markdown", code, Tok::Operator, "~~"));
}

#[test]
fn markdown_digits_and_ordered_list_markers() {
    // A line starting with digits that is not a marker -> Number token.
    let plain_num = "42 reasons\n";
    assert_spans_tile("markdown", plain_num);
    assert!(has_span("markdown", plain_num, Tok::Number, "42"));

    // Valid single- and multi-digit ordered markers, both '.' and ')'.
    let single = "1. first\n";
    assert_spans_tile("markdown", single);
    assert!(has_span("markdown", single, Tok::Operator, "1."));

    let multi = "12) twelfth\n";
    assert_spans_tile("markdown", multi);
    assert!(has_span("markdown", multi, Tok::Operator, "12)"));

    // Ordered marker that ends the input (no trailing char after the dot).
    let eof_marker = "x\n3.";
    assert_spans_tile("markdown", eof_marker);
    assert!(has_span("markdown", eof_marker, Tok::Operator, "3."));

    // Digits running to EOF are not a list marker.
    let all_digits = "x\n789";
    assert_spans_tile("markdown", all_digits);
    assert!(has_span("markdown", all_digits, Tok::Number, "789"));

    // A digit immediately followed by a non-marker char is not a list marker.
    let typo = "12abc\n";
    assert_spans_tile("markdown", typo);
    assert!(has_span("markdown", typo, Tok::Number, "12"));

    // A bare dash at end-of-input is still a (bullet) list marker.
    let dash_eof = "x\n-";
    assert_spans_tile("markdown", dash_eof);
    assert!(has_span("markdown", dash_eof, Tok::Operator, "-"));
}

#[test]
fn markdown_inline_code_and_triple_backtick_fence() {
    let inline = "use `code` here";
    assert_spans_tile("markdown", inline);
    assert!(has_span("markdown", inline, Tok::Str, "`code`"));

    // A run of three or more backticks (a fence) is emitted as-is.
    let fence = "```rust\nlet x = 1;\n```";
    assert_spans_tile("markdown", fence);
    assert!(has_span("markdown", fence, Tok::Str, "```"));
}

// ---------------------------------------------------------------------------
// Per-language rule tables (keywords / types / comments / strings / numbers).
// ---------------------------------------------------------------------------

#[test]
fn python_tokens_classified() {
    let code = "def add(a, b):\n    return a + b  # sum\nx = 'hi'";
    assert_spans_tile("python", code);
    assert!(has_span("python", code, Tok::Keyword, "def"));
    assert!(has_span("python", code, Tok::Keyword, "return"));
    assert!(has_span("python", code, Tok::Func, "add"));
    assert!(has_span("python", code, Tok::Comment, "# sum"));
    assert!(has_span("python", code, Tok::Str, "'hi'"));

    let typed = "n: int = 5";
    assert!(has_span("python", typed, Tok::Type, "int"));
    assert!(has_span("python", typed, Tok::Number, "5"));
}

#[test]
fn javascript_and_typescript_tokens_classified() {
    let js = "const f = (x) => `tmpl ${x}`; // c\n/* b */";
    assert_spans_tile("js", js);
    assert!(has_span("js", js, Tok::Keyword, "const"));
    assert!(has_span("js", js, Tok::Str, "`tmpl ${x}`")); // backtick template string
    assert!(has_span("js", js, Tok::Comment, "// c"));
    assert!(has_span("js", js, Tok::Comment, "/* b */"));

    let ts = "let n: number = 42;";
    assert_spans_tile("ts", ts);
    assert!(has_span("ts", ts, Tok::Keyword, "let"));
    assert!(has_span("ts", ts, Tok::Type, "number"));
    assert!(has_span("ts", ts, Tok::Number, "42"));
}

#[test]
fn json_tokens_classified() {
    let code = "{\n  \"key\": \"value\",\n  \"num\": 42,\n  \"ok\": true,\n  \"x\": null\n}";
    assert_spans_tile("json", code);
    assert!(has_span("json", code, Tok::Str, "\"key\""));
    assert!(has_span("json", code, Tok::Str, "\"value\""));
    assert!(has_span("json", code, Tok::Number, "42"));
    assert!(has_span("json", code, Tok::Keyword, "true"));
    assert!(has_span("json", code, Tok::Keyword, "null"));
}

#[test]
fn bash_tokens_classified() {
    let code = "for f in *.txt; do\n  echo \"$f\" # log\ndone";
    assert_spans_tile("bash", code);
    assert!(has_span("bash", code, Tok::Keyword, "for"));
    assert!(has_span("bash", code, Tok::Keyword, "in"));
    assert!(has_span("bash", code, Tok::Keyword, "do"));
    assert!(has_span("bash", code, Tok::Keyword, "echo"));
    assert!(has_span("bash", code, Tok::Keyword, "done"));
    assert!(has_span("bash", code, Tok::Comment, "# log"));
    assert!(has_span("bash", code, Tok::Str, "\"$f\""));
}

#[test]
fn go_tokens_classified() {
    let code = "func main() {\n\tvar n int = 3\n\ts := `raw`\n}";
    assert_spans_tile("go", code);
    assert!(has_span("go", code, Tok::Keyword, "func"));
    assert!(has_span("go", code, Tok::Keyword, "var"));
    assert!(has_span("go", code, Tok::Type, "int"));
    assert!(has_span("go", code, Tok::Str, "`raw`")); // backtick raw string
    assert!(has_span("go", code, Tok::Func, "main"));
}

#[test]
fn c_and_cpp_tokens_classified() {
    let code = "int main(void) {\n    char *s = \"hi\";\n    return 0; // ok\n}";
    assert_spans_tile("c", code);
    assert!(has_span("c", code, Tok::Type, "int"));
    assert!(has_span("c", code, Tok::Type, "char"));
    assert!(has_span("c", code, Tok::Keyword, "return"));
    assert!(has_span("c", code, Tok::Str, "\"hi\""));
    assert!(has_span("c", code, Tok::Comment, "// ok"));

    let cpp = "template<class T> class Foo { public: Foo(); };";
    assert_spans_tile("cpp", cpp);
    assert!(has_span("cpp", cpp, Tok::Keyword, "template"));
    assert!(has_span("cpp", cpp, Tok::Keyword, "class"));
    assert!(has_span("cpp", cpp, Tok::Keyword, "public"));
    assert!(has_span("cpp", cpp, Tok::Type, "Foo")); // uppercase heuristic
}

#[test]
fn toml_and_ini_tokens_classified() {
    let code = "[section]\nname = \"value\"\nenabled = true\nport = 8080 # c";
    assert_spans_tile("toml", code);
    assert!(has_span("toml", code, Tok::Str, "\"value\""));
    assert!(has_span("toml", code, Tok::Keyword, "true"));
    assert!(has_span("toml", code, Tok::Number, "8080"));
    assert!(has_span("toml", code, Tok::Comment, "# c"));
}

#[test]
fn yaml_tokens_classified() {
    let code = "name: value\nenabled: true\nactive: no\nitems:\n  - 1 # c";
    assert_spans_tile("yaml", code);
    assert!(has_span("yaml", code, Tok::Keyword, "true"));
    assert!(has_span("yaml", code, Tok::Keyword, "no"));
    assert!(has_span("yaml", code, Tok::Comment, "# c"));
    assert!(has_span("yaml", code, Tok::Number, "1"));
}

#[test]
fn sql_tokens_classified() {
    let code = "SELECT id, name FROM users WHERE age > 18; -- note\n/* block */";
    assert_spans_tile("sql", code);
    assert!(has_span("sql", code, Tok::Keyword, "SELECT"));
    assert!(has_span("sql", code, Tok::Keyword, "FROM"));
    assert!(has_span("sql", code, Tok::Keyword, "WHERE"));
    assert!(has_span("sql", code, Tok::Number, "18"));
    assert!(has_span("sql", code, Tok::Comment, "-- note")); // '--' line comment
    assert!(has_span("sql", code, Tok::Comment, "/* block */"));

    let typed = "CREATE TABLE t (id INTEGER, label VARCHAR);";
    assert_spans_tile("sql", typed);
    assert!(has_span("sql", typed, Tok::Type, "INTEGER"));
    assert!(has_span("sql", typed, Tok::Type, "VARCHAR"));
}

#[test]
fn mermaid_tokens_classified_without_treating_labels_as_types() {
    let code = "%% pipeline\nflowchart TD\n    A[Markdown] --> B[AST]\n    B -.-> C[PDF]\n    classDef hot fill:#fee2e2,stroke:#dc2626\n    click A \"https://example.test\"\n";
    assert_spans_tile("mermaid", code);
    assert_spans_tile("mmd", code);
    assert_eq!(tok_of("mermaid", code, "flowchart"), Some(Tok::Keyword));
    assert_eq!(tok_of("mermaid", code, "TD"), Some(Tok::Type));
    assert_eq!(tok_of("mermaid", code, "-->"), Some(Tok::Operator));
    assert_eq!(tok_of("mermaid", code, "-.->"), Some(Tok::Operator));
    assert_eq!(tok_of("mermaid", code, "%% pipeline"), Some(Tok::Comment));
    assert_eq!(
        tok_of("mermaid", code, "\"https://example.test\""),
        Some(Tok::Str)
    );
    assert_eq!(tok_of("mermaid", code, "classDef"), Some(Tok::Keyword));
    assert_ne!(
        tok_of("mermaid", code, "Markdown"),
        Some(Tok::Type),
        "Mermaid node labels must not inherit the generic lexer capitalization heuristic"
    );
}

// ---------------------------------------------------------------------------
// Regression tests for the 2026-06-30 highlighter classification fixes.
// ---------------------------------------------------------------------------

/// Token class of the first span whose exact text is `needle`.
fn tok_of(lang: &str, code: &str, needle: &str) -> Option<Tok> {
    highlight(lang, code)
        .into_iter()
        .find_map(|s| (code.get(s.start..s.end) == Some(needle)).then_some(s.kind))
}

#[test]
fn ini_semicolon_comments_are_comments_not_code() {
    assert_eq!(tok_of("ini", "k = v ; note", "; note"), Some(Tok::Comment));
    assert_spans_tile("ini", "k = v ; note");
}

#[test]
fn capitalized_calls_are_functions_and_all_caps_are_not_types() {
    // A capitalized identifier immediately before `(` is a call, not a type.
    assert_eq!(tok_of("go", "fmt.Println(x)", "Println"), Some(Tok::Func));
    // ALL_CAPS constants are not mislabeled as types.
    assert_ne!(tok_of("python", "MAX = 10", "MAX"), Some(Tok::Type));
    // A genuine Capitalized type name (not a call, not all-caps) still types.
    assert_eq!(
        tok_of("rust", "let x: MyType = y;", "MyType"),
        Some(Tok::Type)
    );
}

#[test]
fn sql_keywords_match_case_insensitively() {
    let q = "select a from t where x and y";
    assert_eq!(tok_of("sql", q, "select"), Some(Tok::Keyword));
    assert_eq!(tok_of("sql", q, "and"), Some(Tok::Keyword));
    assert_eq!(
        tok_of("sql", "SELECT a FROM t", "SELECT"),
        Some(Tok::Keyword)
    );
    assert_spans_tile("sql", q);
}

#[test]
fn c_preprocessor_directives_are_keywords() {
    assert_eq!(
        tok_of("c", "#include <stdio.h>", "#include"),
        Some(Tok::Keyword)
    );
    assert_eq!(tok_of("c", "#define X 1", "#define"), Some(Tok::Keyword));
    assert_spans_tile("c", "#include <stdio.h>\nint main(){ return 0; }");
}
