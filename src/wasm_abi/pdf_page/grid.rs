//! Additive fixed-body-grid ABI. Every earlier PDF export retains its exact
//! signature; only explicitly requested grids select these entry points.

use wasm_bindgen::prelude::*;

use super::book_advanced::book_presentation;
use super::{
    FmdRenderResult, configured_book_assets, configured_book_inputs, configured_pdf_options,
    render_error_to_js, render_result, running_content, wasm,
};

/// Render one document with the complete paper/running-image/typography
/// contract plus a final baseline spacing in points. Zero disables the grid.
///
/// # Errors
/// Rejects invalid or fitting-incompatible grids before parsing or copying
/// individual assets, then preserves the native render error.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfiguredGrid)]
pub fn render_pdf_configured_grid(
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
    baseline_grid_pt: f64,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let grid = admit_grid(baseline_grid_pt, fit_to_pages).map_err(render_error_to_js)?;
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
    wasm::render_pdf(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Render a book with all advanced PDF options and a final body-grid spacing.
/// The first 37 arguments match `renderBookPdfConfiguredAdvanced`; the legacy
/// export's arity and default byte output are unchanged.
///
/// # Errors
/// Rejects invalid or fitting-incompatible grids before source normalization
/// and per-asset copying, then preserves the native render error.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "wasm-book", wasm_bindgen(js_name = renderBookPdfConfiguredGrid))]
pub fn render_book_pdf_configured_grid(
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
) -> Result<FmdRenderResult, JsValue> {
    render_book_pdf_configured_profiled(
        paths,
        sources,
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
        running_slots,
        header_rule,
        footer_rule,
        skip_first_page,
        running_image_destinations,
        running_image_positions,
        running_image_heights,
        typography,
        baseline_grid_pt,
        None,
    )
}

/// Render a book with the complete PDF contract and an explicit authoring dialect.
/// Omitting the dialect preserves the historical extension-enabled default.
///
/// # Errors
/// Rejects unknown dialects before source parsing or per-asset copying.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "wasm-book", wasm_bindgen(js_name = renderBookPdfConfiguredProfiled))]
pub fn render_book_pdf_configured_profiled(
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
    options.profile = profile;
    options.theme.page.baseline_grid = grid;
    // Admission already checked the bounded token list. Apply it after the
    // legacy settings so expansion-only can override conservative protrusion,
    // exactly as on the single-document path.
    options
        .apply_typography_tokens(typography.as_deref())
        .map_err(|error| JsValue::from_str(&error))?;
    let mut renderer = crate::book::BookRenderer::from_sources_with_profile(&inputs, &[], profile)
        .map_err(render_error_to_js)?;
    *renderer.options_mut() = options;
    let report = renderer.render_pdf_reported().map_err(render_error_to_js)?;
    Ok(super::super::book_pdf_artifact_result(
        report,
        renderer.source_length(),
    ))
}

// Native-testable admission, before any JsValue construction or large copies.
pub(super) fn admit_grid(
    points: f64,
    fit_to_pages: Option<u32>,
) -> crate::Result<Option<crate::BaselineGrid>> {
    let grid = if points == 0.0 {
        None
    } else {
        Some(crate::BaselineGrid::from_points(points)?)
    };
    if grid.is_some() && fit_to_pages.is_some_and(|pages| pages > 0) {
        return Err(crate::RenderError::InvalidInput(
            "pdf_baseline_grid_incompatible: baseline grids cannot be combined with fit_to_pages; remove the page-fit target or disable the grid".into(),
        ));
    }
    Ok(grid)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::admit_grid;

    #[test]
    fn grid_admission_retains_precision_and_rejects_fitting() {
        let grid = admit_grid(13.1254, None).expect("valid fractional grid");
        assert_eq!(grid.expect("enabled").spacing_milli_points(), 13125);
        assert!(admit_grid(0.0, Some(2)).expect("disabled grid").is_none());
        for value in [f64::NAN, f64::INFINITY, -1.0, 0.5, 144.001] {
            let error = admit_grid(value, None).expect_err("invalid grid");
            assert_eq!(error.code(), "invalid_input");
            assert!(error.to_string().contains("pdf_baseline_grid:"));
        }
        let error = admit_grid(14.0, Some(2)).expect_err("grid must not be fitted");
        assert_eq!(error.code(), "invalid_input");
        assert!(
            error
                .to_string()
                .contains("pdf_baseline_grid_incompatible:")
        );
    }
    #[test]
    fn single_grid_binding_matches_native_pdf() {
        let source = "# Grid\n\nA body paragraph on a measured baseline grid.\n";
        let mut options = crate::PdfOptions::default();
        options.theme.page.baseline_grid =
            Some(crate::BaselineGrid::from_points(13.125).expect("grid"));
        let expected = crate::render_pdf(source, &options).expect("native PDF");
        let actual = super::render_pdf_configured_grid(
            source,
            None,
            None,
            None,
            None,
            None,
            false,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            false,
            None,
            None,
            false,
            None,
            None,
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            13.125,
        )
        .expect("grid ABI");
        assert_eq!(actual.bytes, expected);
    }

    #[test]
    fn book_grid_binding_matches_native_pdf() {
        let source = "# Grid\n\nA body paragraph on a measured baseline grid.\n";
        let mut options = crate::PdfOptions::default();
        options.theme.page.baseline_grid =
            Some(crate::BaselineGrid::from_points(13.125).expect("grid"));
        let book = crate::book::BookRenderer::new(&[crate::book::BookInput {
            path: "chapter.md".to_string(),
            source: source.to_string(),
        }])
        .expect("book");
        let expected = crate::book::render_book_pdf(book.book(), &options).expect("native PDF");
        let actual = super::render_book_pdf_configured_grid(
            vec!["chapter.md".to_string()],
            vec![source.to_string()],
            None,
            None,
            None,
            None,
            None,
            false,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
            false,
            None,
            None,
            false,
            None,
            None,
            false,
            Vec::new(),
            Vec::new(),
            false,
            false,
            false,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            13.125,
        )
        .expect("grid ABI");
        assert_eq!(actual.bytes, expected);
    }
    #[test]
    fn profiled_book_pdf_matches_native_and_retains_paper_grid_and_typography() {
        let source = "# Dialect\n\nTerm\n: Definition with **emphasis**.\n";
        let inputs = [crate::BookInput {
            path: "one.md".into(),
            source: source.into(),
        }];
        let mut ordinary = None;
        for profile in [None, Some("gfm-plus"), Some("commonmark-gfm")] {
            let rendered = super::render_book_pdf_configured_profiled(
                vec!["one.md".into()],
                vec![source.into()],
                None,
                None,
                Some("Book".into()),
                Some("Author".into()),
                Some(0.0),
                false,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some(12.0),
                Some(1.3),
                Some(10.0),
                true,
                Some(1.0),
                Some("en".into()),
                false,
                Some(2),
                None,
                false,
                vec![612.0, 792.0, 36.0, 36.0, 36.0, 36.0],
                Vec::new(),
                false,
                false,
                false,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Some("footnote-backlinks".into()),
                13.125,
                profile.map(str::to_string),
            )
            .expect("profiled ABI");
            let mut book = crate::book::BookRenderer::from_sources(&inputs, &[]).expect("book");
            let options = book.options_mut();
            options.title = Some("Book".into());
            options.author = Some("Author".into());
            options.metadata_epoch_seconds = Some(0);
            options.base_font_size = Some(12.0);
            options.heading_scale = Some(1.3);
            options.table_font_size = Some(10.0);
            options.page_numbers = true;
            options.font_scale = Some(1.0);
            options.lang = Some("en".into());
            options.toc = false;
            options.toc_depth = Some(2);
            options.theme.page =
                crate::PageStyle::from_browser_geometry(&[612.0, 792.0, 36.0, 36.0, 36.0, 36.0])
                    .expect("paper")
                    .expect("explicit paper");
            options.theme.page.baseline_grid =
                Some(crate::BaselineGrid::from_points(13.125).expect("grid"));
            options
                .apply_typography_tokens(Some("footnote-backlinks"))
                .expect("typography");
            options.profile = crate::wasm_abi::parse_authoring_profile(profile).expect("profile");
            assert_eq!(rendered.bytes(), book.render_pdf().expect("native PDF"));
            match profile {
                None => ordinary = Some(rendered.bytes()),
                Some("gfm-plus") => assert_eq!(Some(rendered.bytes()), ordinary),
                _ => assert_ne!(Some(rendered.bytes()), ordinary),
            }
        }
    }
}
