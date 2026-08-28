//! Integration proof for WOFF1 HTML font embedding (bead ge1t).
//!
//! The HTML emitter embeds per-document font subsets. WOFF1 wraps those
//! subsets in the renderer's own deterministic DEFLATE; these tests pin the
//! observable contract: smaller bytes than TTF for the same document,
//! byte-determinism across runs, and the right data-URL MIME per format.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlFontFormat, HtmlOptions, render_html};

const DOC: &str = r#"# WOFF1 integration

Body text with **bold**, *italic*, ***bold italic***, and `mono code`.

| Col | Val |
|---|---:|
| a   | 1 |

```rust
fn main() { println!("hi"); }
```
"#;

fn opts(format: HtmlFontFormat) -> HtmlOptions {
    HtmlOptions {
        html_font_format: format,
        ..HtmlOptions::default()
    }
}

#[test]
fn woff1_is_default_and_uses_woff_data_urls() {
    let html = render_html(DOC, &HtmlOptions::default()).expect("render");
    assert!(
        html.contains("data:font/woff;base64,"),
        "default HTML must embed WOFF1 font data URLs"
    );
    assert!(
        html.matches("data:font/woff;base64,").count() >= 5,
        "body regular/bold/italic/bold-italic + mono faces all embed: {}",
        html.matches("data:font/woff;base64,").count()
    );
    assert!(
        !html.contains("data:font/ttf"),
        "default render must not leak raw TTF data URLs"
    );
    assert!(
        html.contains("format(\"woff\")"),
        "font-face src must declare the woff format hint"
    );
}

#[test]
fn woff1_output_is_smaller_than_ttf_for_same_document() {
    let woff = render_html(DOC, &opts(HtmlFontFormat::Woff1)).expect("woff render");
    let ttf = render_html(DOC, &opts(HtmlFontFormat::Ttf)).expect("ttf render");
    assert!(
        woff.len() < ttf.len(),
        "woff1 ({}) must be smaller than ttf ({})",
        woff.len(),
        ttf.len()
    );
    let saving = 1.0 - (woff.len() as f64 / ttf.len() as f64);
    assert!(
        saving > 0.20,
        "expected a meaningful embedded-font saving, got {:.1}%",
        saving * 100.0
    );
}

#[test]
fn woff1_output_is_byte_deterministic() {
    let a = render_html(DOC, &opts(HtmlFontFormat::Woff1)).expect("render a");
    let b = render_html(DOC, &opts(HtmlFontFormat::Woff1)).expect("render b");
    assert_eq!(a, b, "woff1 renders must be byte-identical");
}

#[test]
fn ttf_format_remains_available_as_opt_out() {
    let html = render_html(DOC, &opts(HtmlFontFormat::Ttf)).expect("ttf render");
    assert!(html.contains("data:font/ttf;base64,"));
    assert!(html.contains("format(\"truetype\")"));
}
