//! Native coverage for the `wasm-bindgen` ABI adapter (`src/wasm_abi.rs`), bead
//! grn.2.10. This module is feature-gated, so the whole file compiles to nothing
//! unless the `wasm-bindgen` feature is on; run it (and measure it) with
//! `--features wasm-bindgen`.
//!
//! The adapter's success paths build an `FmdRenderResult` without ever crossing
//! the JS boundary, so they run on the native host; the few error paths return a
//! `JsValue` built from a plain string, which `wasm-bindgen` also supports on
//! native. Real Markdown, the real bundled fonts, and real PNG bytes drive every
//! exported function — no mocks.
#![cfg(feature = "wasm-bindgen")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::wasm_abi::{
    capabilities, render_html, render_html_configured, render_html_configured_with_fonts,
    render_pdf, render_pdf_configured, render_pdf_configured_with_assets,
    render_pdf_configured_with_image,
};
use franken_markdown::{FontFamily, fonts, fonts::FontStyle};

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn tiny_rgb_png() -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let idat = franken_markdown::compress::zlib_compress(&[0, 0x20, 0x70, 0xc0]);
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

#[test]
fn capabilities_returns_the_browser_contract_json() {
    let json = capabilities();
    assert!(json.contains("\"schema\":\"fmd-wasm-capabilities-v1\""));
    assert!(json.contains("\"outputs\":[\"html\",\"pdf\"]"));
}

#[test]
fn render_html_default_produces_self_contained_html_result() {
    let out = render_html("# Hi\n\nbody **strong**").expect("native html render");
    assert_eq!(out.format(), "html");
    assert_eq!(out.mime_type(), "text/html; charset=utf-8");
    assert_eq!(out.extension(), "html");
    let bytes = out.bytes();
    let html = String::from_utf8(bytes.clone()).unwrap();
    assert!(html.contains("<main"));
    assert!(html.contains("<strong>strong</strong>"));
    assert_eq!(out.source_length(), "# Hi\n\nbody **strong**".len());
    // Diagnostics JSON is always a JSON array (empty here).
    assert!(out.diagnostics_json().starts_with('['));
}

#[test]
fn render_pdf_default_produces_pdf_bytes() {
    let out = render_pdf("# PDF\n\nbody").expect("native pdf render");
    assert_eq!(out.format(), "pdf");
    assert_eq!(out.mime_type(), "application/pdf");
    assert_eq!(out.extension(), "pdf");
    assert!(out.bytes().starts_with(b"%PDF-"));
}

#[test]
fn render_html_configured_applies_font_dark_mode_title_and_css() {
    // serif font + disabled dark mode + custom css (replaces stylesheet).
    let out = render_html_configured(
        "# Styled",
        Some("serif".to_string()),
        Some("disabled".to_string()),
        Some("My Title".to_string()),
        Some("body{color:#123456}".to_string()),
        false,
    )
    .expect("configured html");
    let html = String::from_utf8(out.bytes()).unwrap();
    assert!(html.contains("body{color:#123456}"));
    assert!(!html.contains("@media (prefers-color-scheme: dark)"));

    // Empty optionals are treated as "unset" (trims to None) — still renders.
    let out2 = render_html_configured(
        "# Plain",
        Some("  ".to_string()),
        None,
        Some(String::new()),
        None,
        false,
    )
    .expect("blank options render");
    assert!(String::from_utf8(out2.bytes()).unwrap().contains("<main"));
}

#[test]
fn render_html_configured_with_fonts_accepts_real_font_bytes() {
    let serif_regular = fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec();
    let out = render_html_configured_with_fonts(
        "Body text",
        None,
        None,
        None,
        None,
        false,
        serif_regular,
        Vec::new(), // empty slots fall back to bundled fonts
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("custom font html");
    let html = String::from_utf8(out.bytes()).unwrap();
    assert!(html.contains("@font-face"));
}

#[test]
fn render_pdf_configured_is_deterministic_with_pinned_epoch() {
    let mk = || {
        render_pdf_configured(
            "# Doc\n\n```rust\nfn main() {}\n```",
            None,
            None,
            Some("Title".to_string()),
            Some("Author".to_string()),
            Some(1_700_000_000.0),
            false,
            true,
        )
        .expect("configured pdf")
        .bytes()
    };
    let a = mk();
    let b = mk();
    assert!(a.starts_with(b"%PDF-"));
    assert_eq!(a, b, "pinned epoch must yield byte-identical PDFs");
}

#[test]
fn render_pdf_configured_with_image_embeds_supplied_png() {
    let out = render_pdf_configured_with_image(
        "![Chart](chart.png)",
        None,
        None,
        None,
        None,
        Some(1_700_000_000.0),
        false,
        false,
        "chart.png".to_string(),
        tiny_rgb_png(),
    )
    .expect("pdf with image");
    let bytes = out.bytes();
    let pdf = String::from_utf8_lossy(&bytes);
    assert!(pdf.contains("/Subtype /Image"));
    assert!(pdf.contains("/Alt (Chart)"));
}

#[test]
fn render_pdf_configured_with_assets_takes_image_and_fonts() {
    let serif = fonts::body_bytes(FontFamily::Serif, FontStyle::Regular).to_vec();
    let out = render_pdf_configured_with_assets(
        "![D](d.png)\n\nbody",
        Some("sans".to_string()),
        Some("auto".to_string()),
        Some("T".to_string()),
        Some("A".to_string()),
        None,
        false,
        false,
        "d.png".to_string(),
        tiny_rgb_png(),
        serif,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("pdf with assets");
    assert!(out.bytes().starts_with(b"%PDF-"));
    assert!(out.format() == "pdf");
}

// NOTE: the adapter's ERROR paths (unknown font/dark-mode, bad epoch, malformed
// font/image bytes) all funnel through `render_error_to_js`, which builds a
// `JsValue::from_str`. On a non-wasm32 host that intrinsic panics with "function
// not implemented on non-wasm32 targets" in a non-unwinding context (SIGABRT, not
// catchable), so the Err paths cannot be exercised natively. They are covered by
// the real wasm build's headless render/parity tests (scripts/check-wasm-package.sh)
// instead; that boundary is exactly what this adapter exists to bridge.
