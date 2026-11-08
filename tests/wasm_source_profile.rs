//! Browser-safe source rendering retains authoring syntax and diagnostic spans.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm::{WasmRenderOptions, render_html, render_pdf};
use franken_markdown::{HtmlOptions, PdfOptions, Profile};

#[test]
fn browser_safe_html_and_pdf_parse_source_in_the_requested_dialect() {
    let source = "*Term\n: definition*\n\n[Link\n: label](/target)\n\n`Code\n: body`\n";
    let options = WasmRenderOptions {
        profile: Some(Profile::CommonMarkGfm),
        metadata_epoch_seconds: Some(1_700_000_000),
        ..WasmRenderOptions::default()
    };
    let html = render_html(source, &options).unwrap();
    assert!(html.html().unwrap().contains("<em>Term\n: definition</em>"));
    assert!(html.html().unwrap().contains("<code>Code : body</code>"));
    let html_options = HtmlOptions {
        profile: Some(Profile::CommonMarkGfm),
        ..HtmlOptions::default()
    };
    let pdf_options = PdfOptions {
        profile: Some(Profile::CommonMarkGfm),
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };
    assert_eq!(
        html.html().unwrap(),
        franken_markdown::render_html(source, &html_options).unwrap()
    );
    assert_eq!(
        render_pdf(source, &options).unwrap().bytes,
        franken_markdown::render_pdf(source, &pdf_options).unwrap()
    );
}

#[test]
fn browser_diagnostics_match_the_selected_grammar_and_original_source() {
    let source = "\u{feff}> [term]:\r\n> : Definition.\r\n";
    let extended = render_html(source, &WasmRenderOptions::default()).unwrap();
    assert!(extended.diagnostics.is_empty());
    let options = WasmRenderOptions::default().with_profile(Profile::CommonMarkGfm);
    for rendered in [
        render_html(source, &options).unwrap(),
        render_pdf(source, &options).unwrap(),
    ] {
        let finding = rendered
            .diagnostics
            .iter()
            .find(|finding| finding.code == Some("malformed_reference"))
            .unwrap();
        assert_eq!(&source[finding.start..finding.end], "[term]:");
        assert_eq!(rendered.source_len, source.len());
    }
}
