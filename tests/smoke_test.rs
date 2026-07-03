//! End-to-end smoke test for the HTML render path. Tests may use `unwrap` for
//! brevity, so opt out of the crate-wide restriction lints here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use franken_markdown::{HtmlOptions, Theme, render_html};

fn render(md: &str) -> String {
    render_html(md, &HtmlOptions::default()).unwrap()
}

fn main_inner(html: &str) -> &str {
    let start_marker = "<main class=\"fmd\">\n";
    let end_marker = "</main>\n</body>";
    let start = html.find(start_marker).unwrap() + start_marker.len();
    let end = html.find(end_marker).unwrap();
    &html[start..end]
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
    assert!(
        html.contains(
            "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n<table>"
        )
    );
    assert!(html.contains("<th>A</th>"));
    assert!(html.contains("<td>1</td>"));
    assert!(html.contains("<hr>"));
}

#[test]
fn default_html_includes_responsive_table_and_print_css() {
    let html = render("| A | B |\n|---|---|\n| one | two |");

    assert!(
        html.contains(
            "<div class=\"table-wrap\" role=\"region\" aria-label=\"Markdown table\" tabindex=\"0\">\n<table>"
        )
    );
    assert!(html.contains(".table-wrap {\n  margin: 0 0 1.4em;\n  overflow-x: auto;"));
    assert!(html.contains(".table-wrap:focus-visible {"));
    assert!(html.contains("outline-offset: 3px;"));
    assert!(html.contains("break-inside: avoid;"));
    assert!(html.contains("@media print {"));
    assert!(html.contains(".table-wrap {\n    overflow: visible;"));
    assert!(html.contains("letter-spacing: 0;"));
    assert!(!html.contains("letter-spacing: -"));
}

#[test]
fn approved_html_preview_snapshot_matches() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/html");
    let source = fs::read_to_string(root.join("preview.md")).unwrap();
    let expected = fs::read_to_string(root.join("preview.article.html")).unwrap();
    let actual = render(&source);

    assert_eq!(expected.trim_end(), main_inner(&actual).trim_end());
}

#[test]
fn html_escaping_is_safe() {
    let html = render("A <script>alert(1)</script> & a < b");
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&amp;"));
}

#[test]
fn heading_ids_are_unique_non_empty_and_collision_safe() {
    let html = render("# Alpha\n\n# Alpha\n\n# Alpha 2\n\n# Alpha\n\n# !!!\n\n# ???");

    assert!(html.contains("<h1 id=\"alpha\">Alpha</h1>"));
    assert!(html.contains("<h1 id=\"alpha-2\">Alpha</h1>"));
    assert!(html.contains("<h1 id=\"alpha-2-2\">Alpha 2</h1>"));
    assert!(html.contains("<h1 id=\"alpha-3\">Alpha</h1>"));
    assert!(html.contains("<h1 id=\"section\">!!!</h1>"));
    assert!(html.contains("<h1 id=\"section-2\">???</h1>"));
    assert!(!html.contains("id=\"\""));
}

#[test]
fn heading_plain_text_projection_preserves_raw_html_source() {
    let html = render("# Title <i>raw</i>");

    assert!(html.contains("<title>Title &lt;i&gt;raw&lt;/i&gt;</title>"));
    assert!(html.contains("<h1 id=\"title-irawi\">Title &lt;i&gt;raw&lt;/i&gt;</h1>"));
}

#[test]
fn unsafe_markdown_url_schemes_are_neutralized() {
    let html = render(
        "[bad](javascript:alert(1)) \
         [obfuscated](<java\tscript:alert(2)>) \
         ![image alt](data:image/svg+xml;base64,PHN2Zy8+) \
         [web](https://example.com?q=1) \
         [mail](mailto:hello@example.com) \
         [phone](tel:+15551234567) \
         [anchor](#section) \
         [relative](docs/page.md) \
         [network](//example.com/image.png) \
         ![remote](https://example.com/image.png)",
    );

    assert!(!html.contains("javascript:"));
    assert!(!html.contains("java\tscript:"));
    assert!(!html.contains("data:image"));
    assert!(!html.contains("<a href=\"javascript"));
    assert!(!html.contains("<img src=\"data:"));
    assert!(html.contains("bad"));
    assert!(html.contains("obfuscated"));
    assert!(html.contains("image alt"));
    assert!(html.contains("<a href=\"https://example.com?q=1\">web</a>"));
    assert!(html.contains("<a href=\"mailto:hello@example.com\">mail</a>"));
    assert!(html.contains("<a href=\"tel:+15551234567\">phone</a>"));
    assert!(html.contains("<a href=\"#section\">anchor</a>"));
    assert!(html.contains("<a href=\"docs/page.md\">relative</a>"));
    assert!(html.contains("<a href=\"//example.com/image.png\">network</a>"));
    assert!(html.contains("<img src=\"https://example.com/image.png\" alt=\"remote\">"));
}

#[test]
fn empty_image_destinations_render_alt_text_without_empty_src() {
    let html = render("![fallback alt]()");

    assert!(!html.contains("<img"));
    assert!(!html.contains("src=\"\""));
    assert!(html.contains("fallback alt"));
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
fn default_html_embeds_deterministic_subset_font_faces() {
    let md = "\
# Font Proof

Body **bold** *italic* ***both*** and `mono`.

```rust
fn main() { println!(\"hello\"); }
```
";
    let first = render(md);
    let second = render(md);

    assert_eq!(first, second, "embedded font CSS must be deterministic");
    assert!(first.contains("@font-face {"));
    assert!(first.contains("font-family: \"FMD Body\";"));
    assert!(first.contains("font-family: \"FMD Mono\";"));
    assert!(first.contains("font-style: italic;"));
    assert!(first.contains("font-weight: 700;"));
    assert!(first.contains("data:font/ttf;base64,"));
    assert!(first.contains("--fmd-font-body: \"FMD Body\","));
    assert!(first.contains("--fmd-font-mono: \"FMD Mono\","));
    assert!(
        first.matches("data:font/ttf;base64,").count() >= 5,
        "expected regular/bold/italic/bold-italic body faces plus mono"
    );
    assert!(
        first.len() < 500_000,
        "subset fonts should keep small documents comfortably below 500KB"
    );
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
    assert!(!html.contains("@font-face"));
}

#[test]
fn pdf_path_returns_valid_mvp_pdf_bytes() {
    use franken_markdown::{PdfOptions, render_pdf};
    let pdf = render_pdf("# Hi", &PdfOptions::default()).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.7\n"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    assert!(
        pdf.windows(b"/Type /Catalog".len())
            .any(|w| w == b"/Type /Catalog")
    );
    assert!(pdf.len() > 500);
}
