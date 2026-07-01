//! `wasm-bindgen` adapter for the browser package.
//!
//! This module is intentionally feature-gated behind `wasm-bindgen`. The core
//! `crate::wasm` API remains dependency-free and is the source of truth; this
//! file only maps that API into a JavaScript-callable shape without hand-written
//! unsafe pointer exports.

use wasm_bindgen::prelude::*;

use crate::wasm::{self, WasmRenderOptions};
use crate::{DarkModePolicy, FontAssetSlot, Theme};

/// Render output object exposed to JavaScript.
#[wasm_bindgen]
pub struct FmdRenderResult {
    bytes: Vec<u8>,
    diagnostics_json: String,
    extension: String,
    format: String,
    mime_type: String,
    source_len: usize,
}

#[wasm_bindgen]
impl FmdRenderResult {
    /// Stable output format: `html` or `pdf`.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn format(&self) -> String {
        self.format.clone()
    }

    /// Browser MIME type for Blob construction.
    #[wasm_bindgen(getter, js_name = mimeType)]
    #[must_use]
    pub fn mime_type(&self) -> String {
        self.mime_type.clone()
    }

    /// Default file extension without a leading dot.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn extension(&self) -> String {
        self.extension.clone()
    }

    /// Source size in bytes.
    #[wasm_bindgen(getter, js_name = sourceLength)]
    #[must_use]
    pub fn source_length(&self) -> usize {
        self.source_len
    }

    /// Rendered output bytes. HTML is UTF-8; PDF is binary.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// Recoverable parser diagnostics as stable JSON.
    #[wasm_bindgen(js_name = diagnosticsJson)]
    #[must_use]
    pub fn diagnostics_json(&self) -> String {
        self.diagnostics_json.clone()
    }
}

/// Dependency-free capability contract as JSON.
#[wasm_bindgen(js_name = capabilities)]
#[must_use]
pub fn capabilities() -> String {
    wasm::capabilities_json()
}

/// Render Markdown to self-contained HTML using default browser-safe options.
///
/// # Errors
/// Returns a JavaScript error when rendering fails.
#[wasm_bindgen(js_name = renderHtml)]
pub fn render_html(markdown: &str) -> std::result::Result<FmdRenderResult, JsValue> {
    render_html_configured(markdown, None, None, None, None, false)
}

/// Render Markdown to PDF using default browser-safe options.
///
/// # Errors
/// Returns a JavaScript error when rendering fails.
#[wasm_bindgen(js_name = renderPdf)]
pub fn render_pdf(markdown: &str) -> std::result::Result<FmdRenderResult, JsValue> {
    render_pdf_configured(markdown, None, None, None, None, None, false, false)
}

/// Render Markdown to self-contained HTML with browser package options.
///
/// # Errors
/// Returns a JavaScript error when options are invalid or rendering fails.
#[wasm_bindgen(js_name = renderHtmlConfigured)]
pub fn render_html_configured(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    custom_css: Option<String>,
    allow_raw_html: bool,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.custom_css = nonempty_verbatim(custom_css);
    options.allow_raw_html = allow_raw_html;
    wasm::render_html(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Render Markdown to self-contained HTML with browser package options and
/// caller-supplied font bytes.
///
/// Empty byte arrays mean "use bundled fallback" for that slot.
///
/// # Errors
/// Returns a JavaScript error when options are invalid or rendering fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderHtmlConfiguredWithFonts)]
pub fn render_html_configured_with_fonts(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    custom_css: Option<String>,
    allow_raw_html: bool,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.custom_css = nonempty_verbatim(custom_css);
    options.allow_raw_html = allow_raw_html;
    apply_font_assets(
        &mut options,
        body_regular,
        body_bold,
        body_italic,
        body_bold_italic,
        mono_regular,
    )?;
    wasm::render_html(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Render Markdown to PDF with browser package options.
///
/// # Errors
/// Returns a JavaScript error when options are invalid or rendering fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfigured)]
pub fn render_pdf_configured(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    author: Option<String>,
    metadata_epoch_seconds: Option<f64>,
    allow_raw_html: bool,
    code_line_numbers: bool,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let options = pdf_options_configured(
        font,
        dark_mode,
        title,
        author,
        metadata_epoch_seconds,
        allow_raw_html,
        code_line_numbers,
    )?;
    wasm::render_pdf(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Render Markdown to PDF with one browser-supplied image asset.
///
/// This dependency-free adapter is intentionally narrow: callers pass bytes
/// they already own (for example from a file picker or fetch handled outside
/// the core). The renderer never touches the browser filesystem or network.
///
/// # Errors
/// Returns a JavaScript error when options are invalid or rendering fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfiguredWithImage)]
pub fn render_pdf_configured_with_image(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    author: Option<String>,
    metadata_epoch_seconds: Option<f64>,
    allow_raw_html: bool,
    code_line_numbers: bool,
    image_destination: String,
    image_bytes: Vec<u8>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    render_pdf_configured_with_assets(
        markdown,
        font,
        dark_mode,
        title,
        author,
        metadata_epoch_seconds,
        allow_raw_html,
        code_line_numbers,
        image_destination,
        image_bytes,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
}

/// Render Markdown to PDF with browser package options, one optional image
/// asset, and caller-supplied font bytes.
///
/// Empty image destination/bytes means "no image asset"; empty font byte arrays
/// mean "use bundled fallback" for that slot.
///
/// # Errors
/// Returns a JavaScript error when options are invalid or rendering fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfiguredWithAssets)]
pub fn render_pdf_configured_with_assets(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    author: Option<String>,
    metadata_epoch_seconds: Option<f64>,
    allow_raw_html: bool,
    code_line_numbers: bool,
    image_destination: String,
    image_bytes: Vec<u8>,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = pdf_options_configured(
        font,
        dark_mode,
        title,
        author,
        metadata_epoch_seconds,
        allow_raw_html,
        code_line_numbers,
    )?;
    if !image_destination.trim().is_empty() || !image_bytes.is_empty() {
        options = options
            .with_pdf_image_asset(image_destination, image_bytes)
            .map_err(render_error_to_js)?;
    }
    apply_font_assets(
        &mut options,
        body_regular,
        body_bold,
        body_italic,
        body_bold_italic,
        mono_regular,
    )?;
    wasm::render_pdf(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

fn pdf_options_configured(
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    author: Option<String>,
    metadata_epoch_seconds: Option<f64>,
    allow_raw_html: bool,
    code_line_numbers: bool,
) -> std::result::Result<WasmRenderOptions, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.author = nonempty_verbatim(author);
    options.metadata_epoch_seconds = parse_epoch(metadata_epoch_seconds)?;
    options.allow_raw_html = allow_raw_html;
    options.code_line_numbers = code_line_numbers;
    Ok(options)
}

fn options_with_font_and_dark_mode(
    font: Option<String>,
    dark_mode: Option<String>,
) -> std::result::Result<WasmRenderOptions, JsValue> {
    let mut options = options_with_font(font)?;
    if let Some(policy) = empty_to_none(dark_mode) {
        options = options.with_dark_mode(parse_dark_mode(&policy)?);
    }
    Ok(options)
}

fn options_with_font(font: Option<String>) -> std::result::Result<WasmRenderOptions, JsValue> {
    match empty_to_none(font) {
        Some(name) => WasmRenderOptions {
            theme: Theme::default(),
            ..WasmRenderOptions::default()
        }
        .with_font_name(&name)
        .map_err(render_error_to_js),
        None => Ok(WasmRenderOptions::default()),
    }
}

fn parse_dark_mode(value: &str) -> std::result::Result<DarkModePolicy, JsValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "system" => Ok(DarkModePolicy::Auto),
        "disabled" | "disable" | "off" | "light" => Ok(DarkModePolicy::Disabled),
        _ => Err(JsValue::from_str("darkMode must be 'auto' or 'disabled'")),
    }
}

fn apply_font_assets(
    options: &mut WasmRenderOptions,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
) -> std::result::Result<(), JsValue> {
    set_font_asset(options, FontAssetSlot::BodyRegular, body_regular)?;
    set_font_asset(options, FontAssetSlot::BodyBold, body_bold)?;
    set_font_asset(options, FontAssetSlot::BodyItalic, body_italic)?;
    set_font_asset(options, FontAssetSlot::BodyBoldItalic, body_bold_italic)?;
    set_font_asset(options, FontAssetSlot::MonoRegular, mono_regular)
}

fn set_font_asset(
    options: &mut WasmRenderOptions,
    slot: FontAssetSlot,
    bytes: Vec<u8>,
) -> std::result::Result<(), JsValue> {
    if bytes.is_empty() {
        return Ok(());
    }
    options
        .font_assets
        .set_slot(slot, bytes)
        .map_err(render_error_to_js)
}

fn empty_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Map an absent or empty JS string to `None`, but preserve any non-empty value
/// VERBATIM — including surrounding whitespace. Titles, authors, and custom CSS
/// must reach the renderer byte-for-byte identical to the native CLI, which
/// passes them through untouched; trimming (as `empty_to_none` does for
/// enum-like values) would break native↔WASM output parity for padded metadata
/// such as `"  Draft  "`.
fn nonempty_verbatim(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.is_empty())
}

fn parse_epoch(value: Option<f64>) -> std::result::Result<Option<u64>, JsValue> {
    const JS_MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    match value {
        Some(epoch) if epoch.is_finite() && epoch >= 0.0 && epoch.fract() == 0.0 => {
            if epoch > JS_MAX_SAFE_INTEGER {
                Err(JsValue::from_str(
                    "metadataEpochSeconds must be <= Number.MAX_SAFE_INTEGER",
                ))
            } else {
                Ok(Some(epoch as u64))
            }
        }
        Some(_) => Err(JsValue::from_str(
            "metadataEpochSeconds must be a finite non-negative integer",
        )),
        None => Ok(None),
    }
}

fn render_result(output: wasm::WasmRenderOutput) -> FmdRenderResult {
    FmdRenderResult {
        diagnostics_json: output.diagnostics_json(),
        extension: output.extension.to_string(),
        format: output.format.as_str().to_string(),
        mime_type: output.mime_type.to_string(),
        source_len: output.source_len,
        bytes: output.bytes,
    }
}

fn render_error_to_js(err: crate::RenderError) -> JsValue {
    JsValue::from_str(&err.to_string())
}
