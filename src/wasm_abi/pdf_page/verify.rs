//! Configured PDF layout verification without serializing a PDF or changing
//! the existing default-only accessibility audit.

use super::{configured_pdf_options, render_error_to_js, running_content};
use crate::verify::{VerifyFinding, VerifyReport};
use crate::wasm::{self, WasmRenderOptions};
use wasm_bindgen::prelude::*;

const REPORT_LIMIT: usize = 8 * 1024 * 1024;

/// Verify the same configured layout used by single-document PDF publication.
///
/// The first 36 arguments match `renderPdfConfiguredProfiled`; the final two
/// select the body baseline grid and Markdown authoring profile. PDF/A options
/// are intentionally absent: this is a layout and authoring audit, not PDF/A or
/// PDF/UA conformance certification. No PDF bytes are emitted.
///
/// Returns the complete native verification JSON, including page text runs,
/// anchors, render warnings, parser findings and a digest of the report. Page
/// fitting uses the publication solver and reports the resulting page count;
/// a requested fit target is not a separate hard-cap admission policy here.
/// The report contains body text runs; running bands are admitted by the same
/// layout checks as publication, but are not included among those text runs.
///
/// # Errors
/// Rejects invalid options and source/resource budgets before parsing, retains
/// typed strict-pagination failures, and refuses reports larger than 8 MiB of
/// UTF-8 JSON. All images and fonts come from the supplied byte arrays.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = verifyPdfConfigured)]
pub fn verify_pdf_configured(
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
    baseline_grid_pt: Option<f64>,
    authoring_profile: Option<String>,
) -> Result<String, JsValue> {
    let profile = super::super::parse_authoring_profile(authoring_profile.as_deref())
        .map_err(JsValue::from_str)?;
    let grid = super::grid::admit_grid(baseline_grid_pt.unwrap_or(0.0), fit_to_pages)
        .map_err(render_error_to_js)?;
    super::archival::admit_inputs(
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
    .map_err(|message| {
        JsValue::from_str(&message.replacen("archival PDF", "PDF verification", 1))
    })?;
    // Admit running content before the shared option builder copies resources.
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
    options.profile = profile;
    options.theme.page.baseline_grid = grid;
    options
        .apply_typography_tokens(typography.as_deref())
        .map_err(|error| JsValue::from_str(&error))?;
    verify_source(markdown, &options)
        .and_then(|report| report_json(&report, REPORT_LIMIT))
        .map_err(render_error_to_js)
}

// JsValue-free so native regression tests exercise the actual parsing and
// typed verification failures. Parser degradation must not become a clean
// report merely because the bounded parser still produced a document.
fn verify_source(source: &str, options: &WasmRenderOptions) -> crate::Result<VerifyReport> {
    let parsed = crate::parse_markdown_spanned_with_profile(source, options.profile);
    let findings = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| VerifyFinding {
            code: diagnostic.code,
            detail: format!(
                "{} (source bytes {}..{})",
                diagnostic.message, diagnostic.span.start, diagnostic.span.end,
            ),
        })
        .collect();
    let report = crate::verify::verify_pdf_result(&parsed.into_document(), &options.pdf_options())?;
    Ok(crate::verify::with_extra_findings(report, findings))
}

fn report_json(report: &VerifyReport, max_bytes: usize) -> crate::Result<String> {
    let json = crate::verify::to_json(report);
    if json.len() > max_bytes {
        return Err(crate::RenderError::InvalidInput(format!(
            "PDF verification report exceeds {max_bytes} bytes"
        )));
    }
    Ok(json)
}

#[cfg(test)]
mod tests;
