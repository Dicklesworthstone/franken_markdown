//! Source-aware one-shot PDF publication. The selected chapters determine
//! reading order; separately authorized include sources are never chapters.

use wasm_bindgen::prelude::*;

use super::book_advanced::book_presentation;
use super::grid::admit_grid;
use super::{
    FmdRenderResult, configured_book_assets, configured_book_inputs, configured_pdf_options,
    render_error_to_js,
};
use crate::book::BookInput;

/// Render a book from explicit chapters and optional include-only sources.
///
/// The first 39 arguments match `renderBookPdfConfiguredProfiled`; the final
/// vectors supply relative paths and UTF-8 text for transclusion only. Source
/// order and roles are retained. `expand_includes = false` treats directives
/// literally and requires empty include vectors. Source dialect selection
/// precedes both include protection and parsing.
///
/// The whole capture shares the existing 4096-source / 64 MiB input budget,
/// and the native source-bundle engine owns expansion, rebasing, cycle, depth,
/// resolver-copy and expanded-output limits. Output and diagnostics come from
/// the same chapter-aware PDF emission as the retained book API.
///
/// # Errors
/// Rejects mismatched source arrays, invalid source roles, budgets, profiles,
/// paths, includes, assets, typography, and rendering errors. No host file or
/// network resource is opened.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "wasm-book", wasm_bindgen(js_name = renderBookPdfConfiguredSources))]
pub fn render_book_pdf_configured_sources(
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
    baseline_grid_pt: f64,
    authoring_profile: Option<String>,
    include_paths: Vec<String>,
    include_sources: Vec<String>,
    expand_includes: bool,
) -> Result<FmdRenderResult, JsValue> {
    let profile = super::super::parse_authoring_profile(authoring_profile.as_deref())
        .map_err(JsValue::from_str)?;
    let grid = admit_grid(baseline_grid_pt, fit_to_pages).map_err(render_error_to_js)?;
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
    let (chapters, resources) = source_bundle(
        paths,
        sources,
        include_paths,
        include_sources,
        expand_includes,
    )
    .map_err(JsValue::from_str)?;
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
    options.profile = profile;
    options.theme.page.baseline_grid = grid;
    // Admission already checked the bounded token list. Apply it after the
    // legacy settings so expansion-only can override conservative protrusion,
    // exactly as on the single-document path.
    options
        .apply_typography_tokens(typography.as_deref())
        .map_err(|error| JsValue::from_str(&error))?;
    let mut renderer = if expand_includes {
        crate::book::BookRenderer::from_sources_with_profile(&chapters, &resources, profile)
    } else {
        crate::book::BookRenderer::new_with_profile(&chapters, profile)
    }
    .map_err(render_error_to_js)?;
    *renderer.options_mut() = options;
    let report = renderer.render_pdf_reported().map_err(render_error_to_js)?;
    Ok(super::super::book_pdf_artifact_result(
        report,
        renderer.source_length(),
    ))
}

// Reject the complete capture before copying per-asset payloads or parsing.
// Returning ordinary Rust errors also permits native negative-case coverage;
// wasm-bindgen cannot construct JsValue errors outside a WASM runtime.
fn source_bundle(
    paths: Vec<String>,
    sources: Vec<String>,
    include_paths: Vec<String>,
    include_sources: Vec<String>,
    expand_includes: bool,
) -> Result<(Vec<BookInput>, Vec<BookInput>), &'static str> {
    admit_sources(
        &paths,
        &sources,
        &include_paths,
        &include_sources,
        expand_includes,
        64 * 1024 * 1024,
    )?;
    let chapters = configured_book_inputs(paths, sources)?;
    let resources = if include_paths.is_empty() {
        Vec::new()
    } else {
        configured_book_inputs(include_paths, include_sources)?
    };
    Ok((chapters, resources))
}

fn admit_sources(
    paths: &[String],
    sources: &[String],
    include_paths: &[String],
    include_sources: &[String],
    expand_includes: bool,
    byte_limit: usize,
) -> Result<(), &'static str> {
    if paths.len() != sources.len() || paths.is_empty() || paths.len() > 4096 {
        return Err("book needs 1..=4096 matching chapter paths and sources");
    }
    if include_paths.len() != include_sources.len() {
        return Err("book include paths and sources must have equal lengths");
    }
    if include_paths.len() > 4096 - paths.len() {
        return Err("book chapters and include sources exceed the 4096-source limit");
    }
    if !expand_includes && !include_paths.is_empty() {
        return Err("book include-only sources require expandIncludes");
    }
    let mut total = 0usize;
    for text in paths
        .iter()
        .chain(sources)
        .chain(include_paths)
        .chain(include_sources)
    {
        total = total
            .checked_add(text.len())
            .ok_or("book source size overflow")?;
        if total > byte_limit {
            return Err("book source text and paths exceed the 64 MiB limit");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "book_sources_tests.rs"]
mod tests;
