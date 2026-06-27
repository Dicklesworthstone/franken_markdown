//! End-to-end smoke test for the HTML render path. Tests may use `unwrap` for
//! brevity, so opt out of the crate-wide restriction lints here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, Theme, render_html};

fn render(md: &str) -> String {
    render_html(md, &HtmlOptions::default()).unwrap()
}

#[test]
fn renders_core_constructs() {
    let md = "\
# Title

A **bold** and *italic* and `code` paragraph with a [link](https://example.com).

- one
- two
- [x] done

1. first
2. second

> a quote

```rust
fn main() {}
```

| A | B |
|---|---|
| 1 | 2 |

---
";
    let html = render(md);

    // Document scaffolding + self-contained styling.
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<style>"));
    assert!(html.contains("<title>Title</title>"));

    // Blocks.
    assert!(html.contains("<h1 id=\"title\">Title</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<em>italic</em>"));
    assert!(html.contains("<code>code</code>"));
    assert!(html.contains("<a href=\"https://example.com\">link</a>"));
    assert!(html.contains("<ul>"));
    assert!(html.contains("<li>one</li>"));
    assert!(html.contains("type=\"checkbox\" disabled checked"));
    assert!(html.contains("<ol>"));
    assert!(html.contains("<blockquote>"));
    assert!(html.contains("<pre><code class=\"language-rust\">"));
    assert!(html.contains("<table>"));
    assert!(html.contains("<th>A</th>"));
    assert!(html.contains("<td>1</td>"));
    assert!(html.contains("<hr>"));
}

#[test]
fn html_escaping_is_safe() {
    let html = render("A <script>alert(1)</script> & a < b");
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&amp;"));
}

#[test]
fn serif_theme_changes_font_stack() {
    let opts = HtmlOptions {
        theme: Theme::serif(),
        ..HtmlOptions::default()
    };
    let html = render_html("# Hi", &opts).unwrap();
    assert!(html.contains("serif"));
    assert!(html.contains("Source Serif"));
}

#[test]
fn custom_stylesheet_replaces_default() {
    let opts = HtmlOptions {
        custom_css: Some("body{color:red}".to_string()),
        ..HtmlOptions::default()
    };
    let html = render_html("# Hi", &opts).unwrap();
    assert!(html.contains("body{color:red}"));
    assert!(!html.contains("--fmd-accent"));
}

#[test]
fn pdf_path_is_typed_not_yet_implemented() {
    use franken_markdown::{PdfOptions, render_pdf};
    let err = render_pdf("# Hi", &PdfOptions::default()).unwrap_err();
    assert_eq!(err.code(), "not_yet_implemented");
}
