//! Additive paper-geometry ABI. The existing multi-image ABI delegates here
//! with an empty geometry vector, preserving its signature and default output.

use wasm_bindgen::prelude::*;

use super::{
    FmdRenderResult, apply_font_assets, apply_font_weights, empty_to_none,
    heading_depth, optional_positive_usize, pdf_options_configured, positive_f32,
    render_error_to_js, render_result, split_nonempty_image_assets,
};
use crate::wasm;
use crate::{PageMargins, PageSize, PageStyle};

/// Render using the ordinary PDF engine with explicit paper and margins.
///
/// `page_geometry` is empty for the existing theme defaults, or exactly six
/// point values: width, height, top, right, bottom, left. Values are checked
/// before parsing or copying per-asset payloads, including after f32 conversion.
/// No output scaling, MediaBox patching, browser printing, or global state.
///
/// # Errors
/// Returns an error for invalid geometry, assets, options, or rendering.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfiguredPage)]
pub fn render_pdf_configured_page(
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
) -> std::result::Result<FmdRenderResult, JsValue> {
    let options = configured_pdf_options(
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
    )?;
    wasm::render_pdf(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Compile selected chapters with the complete PDF configuration contract.
/// Unlike the narrow legacy book ABI, this route retains host images and fonts,
/// typography, metadata, navigation and optional paper geometry. Includes use
/// the bounded source-bundle resolver, and chapter links/citations are bound by
/// the ordinary native book renderer. No source is fetched from the host.
///
/// # Errors
/// Rejects source/asset budgets, invalid options, include resolution errors,
/// and book or PDF rendering failures.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderBookPdfConfiguredPage)]
pub fn render_book_pdf_configured_page(
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
) -> std::result::Result<FmdRenderResult, JsValue> {
    let inputs = configured_book_inputs(paths, sources).map_err(JsValue::from_str)?;
    configured_book_assets(
        &image_destinations, &image_bytes_flat, &image_bytes_lengths,
        [&body_regular, &body_bold, &body_italic, &body_bold_italic, &mono_regular],
    ).map_err(JsValue::from_str)?;
    let options = configured_pdf_options(
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
    )?;
    let mut renderer = crate::book::BookRenderer::from_sources(&inputs, &[])
        .map_err(render_error_to_js)?;
    *renderer.options_mut() = options;
    let bytes = renderer.render_pdf().map_err(render_error_to_js)?;
    Ok(super::artifact_result(
        "book-pdf",
        "application/pdf",
        "pdf",
        bytes,
        renderer.source_length(),
        "[]".to_string(),
    ))
}

// Admission before parsing, include expansion, or per-asset payload copies.
// Keep failure paths independent of JsValue for native regression coverage.
fn configured_book_inputs(
    paths: Vec<String>,
    sources: Vec<String>,
) -> Result<Vec<crate::book::BookInput>, &'static str> {
    if paths.len() != sources.len() || paths.is_empty() || paths.len() > 4096 {
        return Err("book needs 1..=4096 matching paths and sources");
    }
    let mut total = 0usize;
    for (path, source) in paths.iter().zip(&sources) {
        for len in [path.len(), source.len()] {
            total = total.checked_add(len).ok_or("book source size overflow")?;
            if total > 64 * 1024 * 1024 {
                return Err("book source text and paths exceed the 64 MiB limit");
            }
        }
    }
    Ok(paths.into_iter().zip(sources)
        .map(|(path, source)| crate::book::BookInput { path, source }).collect())
}

// Bound the direct ABI as well as the ergonomic JavaScript wrapper. Validate
// before split_nonempty_image_assets clones any image or parses font payloads.
fn configured_book_assets(
    destinations: &[String],
    bytes: &[u8],
    lengths: &[u32],
    fonts: [&[u8]; 5],
) -> Result<(), &'static str> {
    const ASSET_LIMIT: usize = 32 * 1024 * 1024;
    const TOTAL_LIMIT: usize = 128 * 1024 * 1024;
    if destinations.len() > 4096 || destinations.len() != lengths.len()
        || bytes.len() > TOTAL_LIMIT
        || lengths.iter().any(|&len| len == 0 || len as usize > ASSET_LIMIT)
    {
        return Err("book images exceed count, payload, or parallel-array limits");
    }
    let mut destination_bytes = 0usize;
    for destination in destinations {
        if destination.trim().is_empty() || destination.len() > 8192 {
            return Err("book image destinations need 1..=8192 UTF-8 bytes");
        }
        destination_bytes += destination.len();
        if destination_bytes > 65536 {
            return Err("book image destinations exceed 64 KiB");
        }
    }
    let mut font_bytes = 0usize;
    for font in fonts {
        if font.len() > ASSET_LIMIT {
            return Err("book fonts exceed the 32 MiB per-face limit");
        }
        font_bytes += font.len();
        if font_bytes > TOTAL_LIMIT {
            return Err("book fonts exceed the 128 MiB aggregate limit");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn configured_pdf_options(
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
) -> std::result::Result<wasm::WasmRenderOptions, JsValue> {
    let page = PageStyle::from_browser_geometry(&page_geometry).map_err(JsValue::from_str)?;
    let mut options = pdf_options_configured(
        font,
        dark_mode,
        title,
        author,
        metadata_epoch_seconds,
        allow_raw_html,
        code_line_numbers,
        base_font_size,
        heading_scale,
        table_font_size,
    )?;
    if let Some(page) = page {
        options.theme.page = page;
    }
    options.page_numbers = page_numbers;
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    options.lang = empty_to_none(lang);
    options.toc = toc;
    options.toc_depth = heading_depth(toc_depth)?;
    options.fit_to_pages = optional_positive_usize(fit_to_pages, "fitToPages")?;
    options.microtype = if microtype_protrusion {
        crate::layout::MicrotypeOptions::CONSERVATIVE
    } else {
        crate::layout::MicrotypeOptions::DISABLED
    };
    for (destination, bytes) in
        split_nonempty_image_assets(&image_destinations, &image_bytes_flat, &image_bytes_lengths)
            .map_err(JsValue::from_str)?
    {
        options = options
            .with_pdf_image_asset(destination.to_string(), bytes.to_vec())
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
    apply_font_weights(&mut options, &font_weights)?;
    Ok(options)
}

impl PageStyle {
    /// One admission policy for single-document and retained-book WASM exports.
    /// Empty geometry preserves the caller's existing theme. This constructor
    /// is crate-private and available only with the browser adapter feature.
    pub(crate) fn from_browser_geometry(values: &[f64]) -> Result<Option<Self>, &'static str> {
        page_style(values)
    }
}

/// Browser admission policy, deliberately narrower than arbitrary native theme
/// construction: 2..200 inch paper and at least a 1 inch content box. Validate
/// without JsValue so native tests also exercise rejection paths.
fn page_style(values: &[f64]) -> Result<Option<PageStyle>, &'static str> {
    if values.is_empty() {
        return Ok(None);
    }
    if values.len() != 6 {
        return Err("page geometry must contain width, height, top, right, bottom, left");
    }
    if values.iter().any(|v| !v.is_finite() || *v < 0.0 || *v > 14_400.0)
        || !(144.0..=14_400.0).contains(&values[0])
        || !(144.0..=14_400.0).contains(&values[1])
    {
        return Err("page dimensions must be 144..14400 points; margins must be finite and nonnegative");
    }
    let [width, height, top, right, bottom, left]: [f32; 6] =
        std::array::from_fn(|i| values[i] as f32);
    // Check BOTH host precision and exactly the arithmetic used by the native
    // theme. Rounding must not turn an invalid content rectangle into admission.
    if values[0] - values[5] - values[3] < 72.0
        || values[1] - values[2] - values[4] < 72.0
        || width - left - right < 72.0
        || height - top - bottom < 72.0
    {
        return Err("page margins must leave at least 72 points of content width and height");
    }
    let size = if width == PageSize::LETTER.width_pt && height == PageSize::LETTER.height_pt {
        PageSize::LETTER
    } else {
        PageSize { name: "custom", width_pt: width, height_pt: height }
    };
    Ok(Some(PageStyle {
        size,
        margins: PageMargins { top_pt: top, right_pt: right, bottom_pt: bottom, left_pt: left },
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::wasm::WasmRenderOptions;


    #[test]
    fn configured_books_bound_raw_asset_admission() {
        assert!(configured_book_assets(&[], &[], &[], [&[]; 5]).is_ok());
        assert!(configured_book_assets(&["a.svg".into()], &[1], &[1], [&[]; 5]).is_ok());
        assert!(configured_book_assets(&["a.svg".into()], &[], &[], [&[]; 5]).is_err());
        assert!(configured_book_assets(&["a.svg".into()], &[], &[0], [&[]; 5]).is_err());
        assert!(configured_book_assets(&["a.svg".into()], &[], &[33 * 1024 * 1024], [&[]; 5]).is_err());
        assert!(configured_book_assets(&[" ".into()], &[1], &[1], [&[]; 5]).is_err());
        assert!(configured_book_assets(&["x".repeat(8193)], &[1], &[1], [&[]; 5]).is_err());
        assert!(configured_book_assets(&vec!["x".repeat(8192); 9], &[1; 9], &[1; 9], [&[]; 5]).is_err());
    }

    #[test]
    fn configured_books_bound_sources_and_preserve_reading_order() {
        assert!(configured_book_inputs(vec![], vec![]).is_err());
        assert!(configured_book_inputs(vec!["one.md".into()], vec![]).is_err());
        assert!(configured_book_inputs(vec!["x".into(); 4097], vec![String::new(); 4097]).is_err());
        let inputs = configured_book_inputs(
            vec!["z.md".into(), "a.md".into()], vec!["# Z".into(), "# A".into()],
        ).unwrap();
        assert_eq!(inputs[0].path, "z.md");
        assert_eq!(inputs[1].source, "# A");
    }

    #[test]
    fn configured_book_abi_matches_native_pipeline_with_images_and_typography() {
        let paths = vec!["guide/one.md".into(), "two.md".into()];
        let sources = vec![
            "# One\n\n[Next](../two.md#two)\n\n![Chart](chart.svg)\n".into(),
            "# Two\n\nA second chapter.\n".into(),
        ];
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#.to_vec();
        let geometry = vec![720.0, 540.0, 36.0, 30.0, 24.0, 18.0];
        let actual = render_book_pdf_configured_page(
            paths.clone(), sources.clone(), Some("serif".into()), None,
            Some("  Manual  ".into()), Some("Author".into()), Some(0.0), false, true,
            vec!["guide/chart.svg".into()], svg.clone(), vec![svg.len() as u32],
            vec![], vec![], vec![], vec![], vec![], vec![],
            Some(12.0), Some(1.3), Some(9.0), true, Some(1.125), Some("de".into()),
            true, Some(2), None, true, geometry.clone(),
        ).unwrap();
        let mut renderer = crate::book::BookRenderer::from_sources(
            &configured_book_inputs(paths, sources).unwrap(), &[],
        ).unwrap();
        let options = renderer.options_mut();
        options.theme = options.theme.clone().with_font(crate::FontFamily::Serif);
        options.theme.page = page_style(&geometry).unwrap().unwrap();
        options.title = Some("  Manual  ".into());
        options.author = Some("Author".into());
        options.metadata_epoch_seconds = Some(0);
        options.code_line_numbers = true;
        options.base_font_size = Some(12.0);
        options.heading_scale = Some(1.3);
        options.table_font_size = Some(9.0);
        options.page_numbers = true;
        options.font_scale = Some(1.125);
        options.lang = Some("de".into());
        options.toc = true;
        options.toc_depth = Some(2);
        options.microtype = crate::layout::MicrotypeOptions::CONSERVATIVE;
        options.pdf_image_assets.push(crate::PdfImageAsset {
            destination: "guide/chart.svg".into(), bytes: svg,
        });
        assert_eq!(actual.bytes(), renderer.render_pdf().unwrap());
        assert!(String::from_utf8_lossy(&actual.bytes()).contains("/MediaBox [0 0 720 540]"));
    }

    #[test]
    fn absence_preserves_the_theme_and_explicit_default_matches_it() {
        assert_eq!(page_style(&[]).unwrap(), None);
        assert_eq!(page_style(&[612.0, 792.0, 72.0, 72.0, 72.0, 72.0]).unwrap(),
            Some(PageStyle::default()));
    }

    #[test]
    fn independent_margins_and_landscape_reach_the_native_theme() {
        let page = page_style(&[792.0, 612.0, 18.0, 24.0, 30.0, 36.0]).unwrap().unwrap();
        let mut options = WasmRenderOptions::default();
        options.theme.page = page;
        assert_eq!(options.pdf_options().theme.page, page);
        assert_eq!(page.margins, PageMargins {
            top_pt: 18.0, right_pt: 24.0, bottom_pt: 30.0, left_pt: 36.0,
        });
    }

    #[test]
    fn rejects_bad_lengths_nonfinite_values_and_dimension_limits() {
        for n in [1, 2, 3, 4, 5, 7, 100] {
            assert!(page_style(&vec![612.0; n]).is_err());
        }
        for i in 0..6 {
            for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 14_401.0] {
                let mut values = [612.0, 792.0, 72.0, 72.0, 72.0, 72.0];
                values[i] = bad;
                assert!(page_style(&values).is_err());
            }
        }
        for i in 0..2 {
            let mut values = [612.0, 792.0, 0.0, 0.0, 0.0, 0.0];
            values[i] = 143.999999;
            assert!(page_style(&values).is_err());
        }
    }

    #[test]
    fn refuses_collapsed_content_including_host_precision_rounding() {
        for values in [
            [612.0, 792.0, 72.0, 300.0, 72.0, 300.0],
            [612.0, 792.0, 400.0, 72.0, 400.0, 72.0],
            [612.0, 792.0, 72.0, 270.000001, 72.0, 270.0],
            [612.0, 792.0, 360.000001, 72.0, 360.0, 72.0],
        ] {
            assert!(page_style(&values).is_err());
        }
        assert!(page_style(&[612.0, 792.0, 360.0, 270.0, 360.0, 270.0]).is_ok());
        assert!(page_style(&[144.0, 144.0, 0.0, 0.0, 0.0, 0.0]).is_ok());
    }

    #[test]
    fn configured_paper_changes_actual_pdf_media_box_and_is_deterministic() {
        let mut options = WasmRenderOptions::default();
        options.theme.page = page_style(&[720.0, 540.0, 36.0, 36.0, 36.0, 36.0])
            .unwrap().unwrap();
        options.metadata_epoch_seconds = Some(0);
        let first = wasm::render_pdf("# Paper\n\nMeasured content.", &options).unwrap();
        let second = wasm::render_pdf("# Paper\n\nMeasured content.", &options).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert!(String::from_utf8_lossy(&first.bytes).contains("/MediaBox [0 0 720 540]"));
    }

    #[test]
    fn old_multi_abi_and_empty_geometry_produce_identical_pdf_bytes() {
        let old = super::super::render_pdf_configured_multi(
            "# Unchanged", None, None, None, None, Some(0.0), false, false,
            vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![],
            None, None, None, false, None, None, false, None, None, false,
        ).unwrap();
        let new = render_pdf_configured_page(
            "# Unchanged", None, None, None, None, Some(0.0), false, false,
            vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![],
            None, None, None, false, None, None, false, None, None, false, vec![],
        ).unwrap();
        assert_eq!(old.bytes(), new.bytes());
    }
}
