//! Browser/WASM-facing core API tests. Tests may unwrap for clarity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm::{
    WasmOutputFormat, WasmRenderOptions, capabilities_json, render_html, render_pdf,
};
use franken_markdown::{DarkModePolicy, RenderError};

#[test]
fn wasm_html_output_is_self_contained_bytes_with_diagnostics() {
    let output = render_html(
        "# Title\n\n[bad]:\n\nBody with **strong** text.",
        &WasmRenderOptions::default(),
    )
    .unwrap();

    assert_eq!(output.format, WasmOutputFormat::Html);
    assert_eq!(output.mime_type, "text/html; charset=utf-8");
    assert_eq!(output.extension, "html");
    assert_eq!(
        output.source_len,
        "# Title\n\n[bad]:\n\nBody with **strong** text.".len()
    );
    assert_eq!(output.diagnostics.len(), 1);
    assert_eq!(output.diagnostics[0].severity, "warning");
    assert!(output.diagnostics[0].message.contains("reference"));
    assert!(
        output
            .diagnostics_json()
            .contains("\"severity\":\"warning\"")
    );

    let html = output.html().unwrap();
    assert!(html.contains("<strong>strong</strong>"));
    assert!(html.contains("<style>"));
}

#[test]
fn wasm_options_accept_font_names_dark_mode_and_css_bytes() {
    let options = WasmRenderOptions::default()
        .with_font_name("serif")
        .unwrap()
        .with_dark_mode(DarkModePolicy::Disabled)
        .with_custom_css_bytes(b"body { color: #123456; }")
        .unwrap();
    let output = render_html("# Styled", &options).unwrap();
    let html = output.html().unwrap();

    assert!(html.contains("body { color: #123456; }"));
    assert!(!html.contains("@media (prefers-color-scheme: dark)"));
}

#[test]
fn wasm_serif_font_changes_default_stylesheet_without_custom_css() {
    let options = WasmRenderOptions::default()
        .with_font_name("serif")
        .unwrap();
    let output = render_html("# Styled", &options).unwrap();
    let html = output.html().unwrap();

    assert!(html.contains("Source Serif 4"));
    assert!(!html.contains("body { color: #123456; }"));
}

#[test]
fn wasm_options_reject_bad_font_and_non_utf8_css() {
    let err = WasmRenderOptions::default()
        .with_font_name("comic-sans")
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(err.to_string().contains("use 'sans' or 'serif'"));

    let err = WasmRenderOptions::default()
        .with_custom_css_bytes(&[0xff, 0xfe])
        .unwrap_err();
    assert!(matches!(err, RenderError::InvalidInput(_)));
    assert!(err.to_string().contains("custom CSS must be UTF-8"));
}

#[test]
fn wasm_pdf_output_is_binary_deterministic_and_uses_epoch_option() {
    let options = WasmRenderOptions {
        title: Some("WASM PDF".to_string()),
        author: Some("fmd".to_string()),
        metadata_epoch_seconds: Some(1_700_000_000),
        code_line_numbers: true,
        ..WasmRenderOptions::default()
    };
    let a = render_pdf("# PDF\n\n```rust\nfn main() {}\n```", &options).unwrap();
    let b = render_pdf("# PDF\n\n```rust\nfn main() {}\n```", &options).unwrap();

    assert_eq!(a.format, WasmOutputFormat::Pdf);
    assert_eq!(a.mime_type, "application/pdf");
    assert_eq!(a.extension, "pdf");
    assert!(a.html().is_none());
    assert!(a.bytes.starts_with(b"%PDF-"));
    assert_eq!(a.bytes, b.bytes);
    assert!(String::from_utf8_lossy(&a.bytes).contains("D:20231114221320Z"));
}

#[test]
fn wasm_capabilities_json_exposes_browser_safe_contract() {
    let json = capabilities_json();

    assert!(json.contains("\"schema\":\"fmd-wasm-capabilities-v1\""));
    assert!(json.contains("\"outputs\":[\"html\",\"pdf\"]"));
    assert!(json.contains("\"filesystem\":false"));
    assert!(json.contains("\"process\":false"));
    assert!(json.contains("\"network\":false"));
    assert!(json.contains("\"threads\":false"));
    assert!(json.contains("\"font\":\"sans\""));
    assert!(json.contains("\"custom_css_utf8\":true"));
}
