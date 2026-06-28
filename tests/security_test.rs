//! Security posture regressions for malformed but valid-UTF-8 Markdown.
//!
//! The renderer must treat hostile Markdown as data: no panics, no filesystem or
//! network side effects in the core, and safe defaults for HTML/PDF rendering.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use franken_markdown::{
    HtmlOptions, PdfOptions, parse_markdown, parse_markdown_spanned, render_html, render_pdf,
};

fn adversarial_markdown_corpus() -> Vec<String> {
    let mut cases = vec![
        "<script>alert(1)</script>\n\n[bad](javascript:alert(1)) ![x](data:image/svg+xml,boom)"
            .to_string(),
        "[unterminated](https://example.com\n\n![also bad](".to_string(),
        "[ref]: <java\tscript:alert(1)> \"title\"\n\n[ref] [missing][nope]".to_string(),
        "```\nfn main() { let s = \"unterminated; }\n".to_string(),
        "| a | b |\n| --- |\n| too | many | cells | here |".to_string(),
        "- [x lazy continuation\n  - nested\n    1. ordered\n        ```\n        not closed"
            .to_string(),
        "<<< >>> <!-- <b>raw</b> -- <i attr=\"unterminated>".to_string(),
        "\0 control \u{202e} bidi \u{2066} isolate [x](#section)".to_string(),
    ];

    cases.push("*".repeat(1024));
    cases.push("[".repeat(512) + &"]".repeat(512));
    cases.push("> ".repeat(160) + "deep quote text");
    cases.push("`code ".repeat(400) + "tail");
    cases.push("```text\n".to_string() + &"a".repeat(6000) + "\n```");
    cases
}

#[test]
fn malformed_markdown_does_not_panic_across_parse_html_or_pdf() {
    for (idx, md) in adversarial_markdown_corpus().iter().enumerate() {
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _doc = parse_markdown(md);
            let _spanned = parse_markdown_spanned(md);

            let html = render_html(md, &HtmlOptions::default()).expect("HTML render is total");
            assert!(
                html.starts_with("<!DOCTYPE html>"),
                "case {idx}: HTML renderer returned a non-document"
            );
            assert!(
                !html.contains("<script>alert(1)</script>"),
                "case {idx}: default HTML output must escape raw script text"
            );

            let pdf = render_pdf(md, &PdfOptions::default()).expect("PDF render is total");
            assert!(
                pdf.starts_with(b"%PDF-1.7\n") && pdf.ends_with(b"%%EOF\n"),
                "case {idx}: PDF renderer returned invalid envelope"
            );
        }));
        assert!(result.is_ok(), "case {idx} panicked for input: {md:?}");
    }
}

#[test]
fn explicit_raw_html_mode_still_does_not_panic() {
    let md = "<section><b>trusted</b><script>kept by caller choice</script></section>";
    let html_opts = HtmlOptions {
        allow_raw_html: true,
        ..HtmlOptions::default()
    };
    let pdf_opts = PdfOptions {
        allow_raw_html: true,
        ..PdfOptions::default()
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        let html = render_html(md, &html_opts).expect("HTML render is total");
        assert!(html.contains("<section>"));
        let pdf = render_pdf(md, &pdf_opts).expect("PDF render is total");
        assert!(pdf.starts_with(b"%PDF-1.7\n"));
    }));
    assert!(result.is_ok());
}
