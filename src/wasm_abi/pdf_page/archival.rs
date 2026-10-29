//! Single-document PDF/A emission through the ordinary native document writer.
//! No synthetic chapter, PDF byte conversion, browser printing or fallback.

use super::{
    FmdRenderResult, configured_pdf_options, render_error_to_js, render_result, running_content,
};
use crate::wasm::{self, WasmDiagnostic, WasmOutputFormat, WasmRenderOptions, WasmRenderOutput};
use crate::{DiagnosticSeverity, PdfASettings};
use wasm_bindgen::prelude::*;

/// The running-image PDF ABI plus export-local `off` / `2b`, strictness and an
/// optional body baseline grid. Omitting the final argument preserves the
/// original archival binding's default output.
/// Ordinary bindings retain their signatures and behavior. This is emission,
/// not independent PDF/A validation. Metadata uses the supplied epoch, never
/// the clock. Strict mode fails on the native writer's forbidden URI actions;
/// non-strict mode uses its explicit action-dropping policy.
///
/// # Errors
/// Rejects unsupported profiles, strict-with-off, source/resource budgets,
/// malformed settings and native rendering failures. Never substitutes PDF.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfiguredArchival)]
pub fn render_pdf_configured_archival(
    markdown: &str,
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
    profile: &str,
    strict: bool,
    baseline_grid_pt: Option<f64>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let settings = archival_settings(profile, strict).map_err(JsValue::from_str)?;
    let grid = super::grid::admit_grid(baseline_grid_pt.unwrap_or(0.0), fit_to_pages)
        .map_err(render_error_to_js)?;
    admit_inputs(
        markdown,
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
    // Admit the chrome before copying any asset payloads.
    let mut running = running_content(running_slots, header_rule, footer_rule, skip_first_page)
        .map_err(JsValue::from_str)?;
    let [header_image, footer_image] = wasm::running_images_from_abi(
        running_image_destinations,
        running_image_positions,
        running_image_heights,
    )
    .map_err(JsValue::from_str)?;
    running.header.image = header_image;
    running.footer.image = footer_image;
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
    options.theme.page.baseline_grid = grid;
    options
        .apply_typography_tokens(typography.as_deref())
        .map_err(|e| JsValue::from_str(&e))?;
    render_archival(markdown, &options, settings)
        .map(render_result)
        .map_err(render_error_to_js)
}

fn archival_settings(profile: &str, strict: bool) -> Result<PdfASettings, &'static str> {
    match (profile, strict) {
        ("off", false) => Ok(PdfASettings::OFF),
        ("2b", false) => Ok(PdfASettings::a2b()),
        ("2b", true) => Ok(PdfASettings::a2b_strict()),
        ("off", true) => Err("pdfAStrict requires pdfA: 2b"),
        _ => Err("pdfA must be off or 2b"),
    }
}

// Both direct ABI callers and worker callers are admitted before parser work,
// per-resource copies or font decoding. No additional resource authority.
fn admit_inputs(
    source: &str,
    destinations: &[String],
    bytes: &[u8],
    lengths: &[u32],
    fonts: [&[u8]; 5],
) -> Result<(), &'static str> {
    if source.len() > 32 * 1024 * 1024 {
        return Err("archival PDF source exceeds 32 MiB");
    }
    if destinations.len() > 4096 || destinations.len() != lengths.len() {
        return Err("archival PDF needs matching image arrays with at most 4096 entries");
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut text_bytes = 0usize;
    for destination in destinations {
        if destination.trim().is_empty()
            || destination.len() > 8192
            || !seen.insert(destination.as_str())
        {
            return Err(
                "archival PDF image destinations must be nonempty, unique and at most 8 KiB",
            );
        }
        text_bytes += destination.len();
        if text_bytes > 65536 {
            return Err("archival PDF image destinations exceed 64 KiB");
        }
    }
    let mut packed = 0usize;
    for &length in lengths {
        if length == 0 || length as usize > 32 * 1024 * 1024 {
            return Err("archival PDF image payload must contain 1 byte to 32 MiB");
        }
        packed = packed
            .checked_add(length as usize)
            .ok_or("image size overflow")?;
    }
    if packed != bytes.len() {
        return Err("archival PDF packed image lengths do not match the payload");
    }
    let mut total = bytes.len();
    for font in fonts {
        if font.len() > 32 * 1024 * 1024 {
            return Err("archival PDF font exceeds 32 MiB");
        }
        total = total
            .checked_add(font.len())
            .ok_or("resource size overflow")?;
    }
    if total > 128 * 1024 * 1024 {
        return Err("archival PDF image and font resources exceed 128 MiB");
    }
    Ok(())
}

// Same parsing, options and diagnostics as wasm::render_pdf. Only the final
// native writer selection differs. Kept JsValue-free for native regression tests.
fn render_archival(
    source: &str,
    options: &WasmRenderOptions,
    settings: PdfASettings,
) -> crate::Result<WasmRenderOutput> {
    if settings == PdfASettings::OFF {
        return wasm::render_pdf(source, options);
    }
    let parsed = crate::parse_markdown_spanned(source);
    let mut diagnostics: Vec<WasmDiagnostic> = parsed
        .diagnostics
        .iter()
        .map(|item| WasmDiagnostic {
            severity: match item.severity {
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Error => "error",
            },
            start: item.span.start,
            end: item.span.end,
            message: item.message.clone(),
        })
        .collect();
    let doc = parsed.into_document();
    let pdf_options = options.pdf_options();
    for warning in crate::render_warnings(&doc, &pdf_options) {
        diagnostics.push(WasmDiagnostic {
            severity: "warning",
            start: 0,
            end: 0,
            message: warning.message(),
        });
    }
    let bytes = crate::render_pdf_document_pdfa(&doc, &pdf_options, settings)?;
    Ok(WasmRenderOutput {
        format: WasmOutputFormat::Pdf,
        mime_type: WasmOutputFormat::Pdf.mime_type(),
        extension: WasmOutputFormat::Pdf.extension(),
        bytes,
        diagnostics,
        source_len: source.len(),
    })
}

#[cfg(test)]
mod tests;
