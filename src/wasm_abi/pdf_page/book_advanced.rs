//! Additive book PDF options. Existing book signatures and default bytes stay
//! unchanged; the host supplies all source, fonts and image payloads explicitly.

use wasm_bindgen::prelude::*;

use super::{
    FmdRenderResult, configured_book_assets, configured_book_inputs, configured_pdf_options,
    render_error_to_js, running_content, wasm,
};

/// Render a book with explicit typography, optimal pagination and running bands.
///
/// The first 29 arguments match `renderBookPdfConfiguredPage`. Running slots,
/// rules and images use the single-document ABI contract; `typography` is the
/// same comma-separated native token list. Chapter-relative links, citations,
/// image resolution and includes still use the ordinary book renderer.
///
/// # Errors
/// Rejects malformed presentation settings before source parsing or per-asset
/// copies, then propagates source, asset, geometry and native rendering errors.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(
    feature = "wasm-book",
    wasm_bindgen(js_name = renderBookPdfConfiguredAdvanced)
)]
pub fn render_book_pdf_configured_advanced(
    paths: Vec<String>,
    sources: Vec<String>,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    author: Option<String>,
    metadata_epoch_seconds: Option<f64>,
    allow_raw_html: bool,
    code_line_numbers: bool,
    image_destinations: Vec<String>,
    image_bytes_flat: Vec<u8>,
    image_bytes_lengths: Vec<u32>,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
    font_weights: Vec<u32>,
    base_font_size: Option<f64>,
    heading_scale: Option<f64>,
    table_font_size: Option<f64>,
    page_numbers: bool,
    font_scale: Option<f64>,
    lang: Option<String>,
    toc: bool,
    toc_depth: Option<u32>,
    fit_to_pages: Option<u32>,
    microtype_protrusion: bool,
    page_geometry: Vec<f64>,
    running_slots: Vec<String>,
    header_rule: bool,
    footer_rule: bool,
    skip_first_page: bool,
    running_image_destinations: Vec<String>,
    running_image_positions: Vec<u32>,
    running_image_heights: Vec<u32>,
    typography: Option<String>,
) -> Result<FmdRenderResult, JsValue> {
    let running = book_presentation(
        typography.as_deref(),
        running_slots,
        header_rule,
        footer_rule,
        skip_first_page,
        running_image_destinations,
        running_image_positions,
        running_image_heights,
    )
    .map_err(|error| JsValue::from_str(&error))?;
    let inputs = configured_book_inputs(paths, sources).map_err(JsValue::from_str)?;
    configured_book_assets(
        &image_destinations,
        &image_bytes_flat,
        &image_bytes_lengths,
        [
            &body_regular,
            &body_bold,
            &body_italic,
            &body_bold_italic,
            &mono_regular,
        ],
    )
    .map_err(JsValue::from_str)?;
    let mut options = configured_pdf_options(
        font,
        dark_mode,
        title,
        author,
        metadata_epoch_seconds,
        allow_raw_html,
        code_line_numbers,
        image_destinations,
        image_bytes_flat,
        image_bytes_lengths,
        body_regular,
        body_bold,
        body_italic,
        body_bold_italic,
        mono_regular,
        font_weights,
        base_font_size,
        heading_scale,
        table_font_size,
        page_numbers,
        font_scale,
        lang,
        toc,
        toc_depth,
        fit_to_pages,
        microtype_protrusion,
        page_geometry,
    )?
    .with_running(running);
    // Admission already checked the bounded token list. Apply it after the
    // legacy settings so expansion-only can override conservative protrusion,
    // exactly as on the single-document path.
    options
        .apply_typography_tokens(typography.as_deref())
        .map_err(|error| JsValue::from_str(&error))?;
    let mut renderer =
        crate::book::BookRenderer::from_sources(&inputs, &[]).map_err(render_error_to_js)?;
    *renderer.options_mut() = options;
    let bytes = renderer.render_pdf().map_err(render_error_to_js)?;
    Ok(super::super::artifact_result(
        "book-pdf",
        "application/pdf",
        "pdf",
        bytes,
        renderer.source_length(),
        "[]".to_string(),
    ))
}

// Native-testable admission before any JsValue error construction or rendering.
#[allow(clippy::too_many_arguments)]
fn book_presentation(
    typography: Option<&str>,
    slots: Vec<String>,
    header_rule: bool,
    footer_rule: bool,
    skip_first_page: bool,
    destinations: Vec<String>,
    positions: Vec<u32>,
    heights: Vec<u32>,
) -> Result<crate::PdfRunningContent, String> {
    if typography.is_some_and(|text| text.len() > 256) {
        return Err("book typography is limited to 256 UTF-8 bytes".into());
    }
    wasm::WasmRenderOptions::default().apply_typography_tokens(typography)?;
    let mut running = running_content(slots, header_rule, footer_rule, skip_first_page)?;
    let [header, footer] = wasm::running_images_from_abi(destinations, positions, heights)?;
    running.header.image = header;
    running.footer.image = footer;
    Ok(running)
}

#[cfg(test)]
#[path = "book_advanced_tests.rs"]
mod tests;
