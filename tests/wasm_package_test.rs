//! Browser package contract tests.
//!
//! The source package is handwritten and dependency-free JavaScript/TypeScript.
//! The generated `pkg/` glue is produced by `scripts/check-wasm-package.sh`
//! after compiling the feature-gated `wasm-bindgen` adapter.
//!
//! IMPORTANT (bead 3i5.6): the source-string assertions below are a SOURCE-SHAPE
//! LINT only — they prove the hand-written wrapper/types/package metadata keep
//! the expected exports and field names. They do NOT prove the WASM package
//! actually loads or renders. The real "first-class WASM" proof is
//! `scripts/check-wasm-package.sh`, which builds the generated module, loads it
//! in headless node, renders HTML+PDF, and asserts byte-identical native<->WASM
//! parity. Never treat a passing source-shape lint as evidence of working WASM.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

#[cfg(feature = "wasm-bindgen")]
fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

#[cfg(feature = "wasm-bindgen")]
fn tiny_rgb_png() -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

    let rows = [0, 0x24, 0x91, 0xB8];
    let idat = franken_markdown::compress::zlib_compress(&rows);

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1A\n");
    png.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&png_chunk(b"IDAT", &idat));
    png.extend_from_slice(&png_chunk(b"IEND", &[]));
    png
}

#[test]
fn browser_package_sources_export_agent_friendly_api() {
    // SOURCE-SHAPE LINT (not proof of working WASM): asserts the wrapper source
    // keeps the expected export surface. Real proof: scripts/check-wasm-package.sh.
    let js = fs::read_to_string("wasm/franken_markdown.js").unwrap();
    let dts = fs::read_to_string("wasm/franken_markdown.d.ts").unwrap();
    let package = fs::read_to_string("wasm/package.json").unwrap();

    assert!(js.contains("export async function init"));
    assert!(js.contains("export async function capabilities"));
    assert!(js.contains("export async function renderHtml"));
    assert!(js.contains("export async function renderPdf"));
    assert!(js.contains("export async function createRenderer"));
    assert!(js.contains("renderHtmlConfiguredWithFonts"));
    // The wrapper renders PDFs (with any number of images) through the multi
    // entry point; the single-image ABI functions remain in the Rust crate.
    assert!(js.contains("renderPdfConfiguredMulti"));
    assert!(js.contains("fontAssets"));
    assert!(js.contains("fontAssetsOption"));
    assert!(js.contains("fontBytesForSlot"));
    assert!(js.contains("fontAssets contains duplicate slot"));
    assert!(js.contains("body-bold-italic"));
    assert!(js.contains("darkMode"));
    assert!(js.contains("darkModeOption"));
    assert!(js.contains("darkMode must be 'auto' or 'disabled'"));
    assert!(js.contains("pdfImages"));
    assert!(js.contains("Uint8Array"));
    assert!(js.contains("ArrayBuffer.isView"));
    assert!(js.contains("mimeType"));
    assert!(js.contains("sourceLength"));
    assert!(js.contains("diagnostics"));
    assert!(js.contains("bytes"));
    assert!(js.contains("Blob"));
    assert!(js.contains("TextDecoder"));
    assert!(js.contains("initPromise = null"));
    assert!(js.contains("result.free()"));
    assert!(js.contains("function parseJson"));
    assert!(js.contains("Invalid ${label} returned by franken_markdown wasm core"));
    assert!(js.contains("function verbatimOption"));
    assert!(js.contains("metadataEpochSeconds must be a number"));
    assert!(js.contains("Number.isSafeInteger"));
    assert!(js.contains("Number.MAX_SAFE_INTEGER"));

    assert!(dts.contains("export interface FmdRenderOutput"));
    assert!(dts.contains("export type FmdDarkMode"));
    assert!(dts.contains("darkMode?: FmdDarkMode"));
    assert!(dts.contains("export interface FmdPdfImageAsset"));
    assert!(dts.contains("export interface FmdFontAsset"));
    assert!(dts.contains("export type FmdFontAssetSlot"));
    assert!(dts.contains("bytes: Uint8Array"));
    assert!(dts.contains("fontAssets?: FmdFontAsset[]"));
    assert!(dts.contains("pdfImages?: FmdPdfImageAsset[]"));
    assert!(dts.contains("image_assets: \"png_v0_host_supplied_bytes\""));
    assert!(dts.contains("font_assets: \"ttf_v0_host_supplied_bytes\""));
    assert!(dts.contains("mimeType: string"));
    assert!(dts.contains("sourceLength: number"));
    assert!(dts.contains("diagnostics: FmdDiagnostic[]"));
    assert!(dts.contains("renderHtml(markdown: string"));
    assert!(dts.contains("renderPdf(markdown: string"));
    assert!(dts.contains("metadataEpochSeconds?: number"));

    assert!(package.contains("\"type\": \"module\""));
    assert!(package.contains("\"sideEffects\": false"));
    assert!(package.contains("\"franken_markdown.d.ts\""));
    assert!(package.contains("\"demo/index.html\""));
    assert!(package.contains("\"demo/demo.js\""));
    assert!(package.contains("\"pkg/franken_markdown_bg.wasm\""));
}

#[test]
fn browser_demo_sources_use_public_package_api() {
    let html = fs::read_to_string("wasm/demo/index.html").unwrap();
    let js = fs::read_to_string("wasm/demo/demo.js").unwrap();

    assert!(html.contains("id=\"markdown\""));
    assert!(html.contains("id=\"preview\""));
    assert!(html.contains("id=\"download-pdf\""));
    assert!(html.contains("id=\"font\""));
    assert!(html.contains("id=\"dark-mode\""));
    assert!(html.contains("id=\"custom-css\""));
    assert!(html.contains("id=\"allow-html\""));
    assert!(html.contains("id=\"line-numbers\""));
    assert!(html.contains("script type=\"module\" src=\"./demo.js\""));

    assert!(js.contains("import { createRenderer } from \"../franken_markdown.js\""));
    assert!(js.contains("function requiredElement(selector)"));
    assert!(js.contains("franken_markdown demo is missing required element"));
    assert!(js.contains("markdown: requiredElement(\"#markdown\")"));
    assert!(js.contains("renderer.renderHtml(markdown, options)"));
    assert!(js.contains("renderer.renderPdf(markdown, renderOptions())"));
    assert!(js.contains("darkMode: els.darkMode.value"));
    assert!(js.contains("customCss"));
    assert!(js.contains("allowRawHtml"));
    assert!(js.contains("codeLineNumbers"));
    assert!(js.contains("URL.createObjectURL(output.blob())"));
    assert!(js.contains("output.filename(filenameBase())"));
    assert!(js.contains("els.preview.srcdoc = output.text()"));
    assert!(!js.contains("fetch("));
    assert!(!js.contains("XMLHttpRequest"));
}

#[test]
fn rust_side_wasm_core_round_trips_html_and_pdf_bytes() {
    use franken_markdown::wasm::{WasmOutputFormat, WasmRenderOptions, render_html, render_pdf};

    let markdown = "# Package\n\nBody with **strong** text.";
    let html = render_html(markdown, &WasmRenderOptions::default()).unwrap();
    let pdf = render_pdf(markdown, &WasmRenderOptions::default()).unwrap();

    assert_eq!(html.format, WasmOutputFormat::Html);
    assert_eq!(html.mime_type, "text/html; charset=utf-8");
    assert_eq!(html.extension, "html");
    assert_eq!(html.source_len, markdown.len());
    assert!(html.bytes.starts_with(b"<!DOCTYPE html>"));
    assert!(html.diagnostics_json().starts_with('['));

    assert_eq!(pdf.format, WasmOutputFormat::Pdf);
    assert_eq!(pdf.mime_type, "application/pdf");
    assert_eq!(pdf.extension, "pdf");
    assert_eq!(pdf.source_len, markdown.len());
    assert!(pdf.bytes.starts_with(b"%PDF-"));
    assert!(pdf.diagnostics_json().starts_with('['));
}

#[cfg(feature = "wasm-bindgen")]
#[test]
fn wasm_bindgen_adapter_round_trips_package_api_shape() {
    use franken_markdown::wasm_abi::{
        capabilities, render_html_configured, render_html_configured_with_fonts,
        render_pdf_configured, render_pdf_configured_with_assets, render_pdf_configured_with_image,
    };

    let html = render_html_configured(
        "# Package",
        Some("serif".to_string()),
        Some("disabled".to_string()),
        Some("Package".to_string()),
        Some("\nbody { color: #123456; }\n".to_string()),
        false,
    )
    .unwrap();
    let pdf = render_pdf_configured(
        "# Package",
        None,
        Some("auto".to_string()),
        None,
        None,
        Some(1_700_000_000.0),
        false,
        true,
    )
    .unwrap();

    assert_eq!(html.format(), "html");
    assert_eq!(html.mime_type(), "text/html; charset=utf-8");
    assert_eq!(html.extension(), "html");
    assert_eq!(html.source_length(), "# Package".len());
    assert!(html.bytes().starts_with(b"<!DOCTYPE html>"));
    assert!(html.diagnostics_json().starts_with('['));
    let html_text = String::from_utf8(html.bytes()).unwrap();
    assert!(html_text.contains("\nbody { color: #123456; }\n"));

    assert_eq!(pdf.format(), "pdf");
    assert_eq!(pdf.mime_type(), "application/pdf");
    assert_eq!(pdf.extension(), "pdf");
    assert!(pdf.bytes().starts_with(b"%PDF-"));
    assert!(pdf.diagnostics_json().starts_with('['));

    let image_pdf = render_pdf_configured_with_image(
        "![WASM image](diagram.png)",
        None,
        None,
        None,
        None,
        Some(1_700_000_000.0),
        false,
        false,
        "diagram.png".to_string(),
        tiny_rgb_png(),
    )
    .unwrap();
    let image_pdf_bytes = image_pdf.bytes();
    let image_pdf_text = String::from_utf8_lossy(&image_pdf_bytes).into_owned();
    assert!(image_pdf_text.contains("/Subtype /Image"));
    assert!(image_pdf_text.contains("/XObject << /Im1 "));
    assert!(image_pdf_text.contains("/S /Figure"));
    assert!(image_pdf_text.contains("/Alt (WASM image)"));

    let custom_font = franken_markdown::fonts::body_bytes(
        franken_markdown::FontFamily::Serif,
        franken_markdown::fonts::FontStyle::Regular,
    )
    .to_vec();
    let html_with_font = render_html_configured_with_fonts(
        "Custom font",
        None,
        None,
        None,
        None,
        false,
        custom_font.clone(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let html_with_font_text = String::from_utf8(html_with_font.bytes()).unwrap();
    assert!(html_with_font_text.contains("@font-face"));
    assert!(html_with_font_text.contains("font-family: \"FMD Body\""));

    let pdf_with_font = render_pdf_configured_with_assets(
        "Custom font",
        None,
        None,
        None,
        None,
        Some(1_700_000_000.0),
        false,
        false,
        String::new(),
        Vec::new(),
        custom_font,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap();
    let pdf_with_font_bytes = pdf_with_font.bytes();
    let pdf_with_font_text = String::from_utf8_lossy(&pdf_with_font_bytes).into_owned();
    assert!(pdf_with_font_text.contains("/FontFile2"));
    assert!(pdf_with_font_text.contains("/CIDFontType2"));

    let caps = capabilities();
    assert!(caps.contains("\"schema\":\"fmd-wasm-capabilities-v1\""));
    assert!(caps.contains("\"filesystem\":false"));
    assert!(caps.contains("\"image_assets\":\"png_v0_host_supplied_bytes\""));
    assert!(caps.contains("\"font_assets\":\"ttf_v0_host_supplied_bytes\""));
}
