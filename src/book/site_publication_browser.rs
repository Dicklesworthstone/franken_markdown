//! Additive browser entry point for the canonical site publisher. Browser and
//! native hosts supply bytes; no filesystem, network, or ambient font lookup.

use std::collections::BTreeSet;
use wasm_bindgen::prelude::*;

use super::BookRenderer;
use crate::book::BookInput;
use crate::wasm::WasmRenderOptions;
use crate::{DarkModePolicy, FontAssetSlot, PdfImageAsset, RenderError, Result};

/// Publish chapters and optional include-only resources as one offline ZIP.
///
/// Images use book-root keys; fonts use the five ordinary renderer slots.
/// Chapter order is preserved. The output includes the canonical chapter-aware
/// search page/index and `frankenmarkdown-receipt.json`. Source parsing remains
/// safe: this API deliberately has no raw-HTML trust switch.
///
/// # Errors
/// Rejects source, asset, metadata and output budgets, malformed options,
/// missing/cyclic includes, colliding paths and rendering failures.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderBookSitePublication)]
pub fn render_book_site_publication(
    paths: Vec<String>,
    sources: Vec<String>,
    include_paths: Vec<String>,
    include_sources: Vec<String>,
    expand_includes: bool,
    title: Option<String>,
    font: Option<String>,
    dark_mode: Option<String>,
    font_scale: Option<f64>,
    lang: Option<String>,
    custom_css: Option<String>,
    toc: bool,
    toc_depth: Option<u32>,
    image_destinations: Vec<String>,
    image_bytes_flat: Vec<u8>,
    image_lengths: Vec<u32>,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
    font_weights: Vec<u32>,
) -> std::result::Result<Vec<u8>, JsValue> {
    let request = SiteRequest {
        paths, sources, include_paths, include_sources, expand_includes,
        title, font, dark_mode, font_scale, lang, custom_css, toc, toc_depth,
        image_destinations, image_bytes_flat, image_lengths,
        fonts: [body_regular, body_bold, body_italic, body_bold_italic, mono_regular],
        font_weights,
    };
    build_renderer(request)
        .and_then(|renderer| renderer.render_site_publication())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[derive(Default)]
struct SiteRequest {
    paths: Vec<String>,
    sources: Vec<String>,
    include_paths: Vec<String>,
    include_sources: Vec<String>,
    expand_includes: bool,
    title: Option<String>,
    font: Option<String>,
    dark_mode: Option<String>,
    font_scale: Option<f64>,
    lang: Option<String>,
    custom_css: Option<String>,
    toc: bool,
    toc_depth: Option<u32>,
    image_destinations: Vec<String>,
    image_bytes_flat: Vec<u8>,
    image_lengths: Vec<u32>,
    fonts: [Vec<u8>; 5],
    font_weights: Vec<u32>,
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book site: {message}"))
}

fn source_inputs(paths: Vec<String>, sources: Vec<String>, total: &mut usize) -> Result<Vec<BookInput>> {
    if paths.len() != sources.len() {
        return Err(invalid("paths and sources must have equal lengths"));
    }
    for (path, source) in paths.iter().zip(&sources) {
        for len in [path.len(), source.len()] {
            *total = total.checked_add(len).ok_or_else(|| invalid("source size overflow"))?;
            if *total > 64 * 1024 * 1024 {
                return Err(invalid("source text and paths exceed 64 MiB"));
            }
        }
    }
    Ok(paths.into_iter().zip(sources).map(|(path, source)| BookInput { path, source }).collect())
}

fn validate_assets(request: &SiteRequest) -> Result<()> {
    const SINGLE: usize = 32 * 1024 * 1024;
    const TOTAL: usize = 128 * 1024 * 1024;
    if request.image_destinations.len() > 4096
        || request.image_destinations.len() != request.image_lengths.len()
        || request.image_bytes_flat.len() > TOTAL
    {
        return Err(invalid("image arrays differ in length or exceed 4096 assets / 128 MiB"));
    }
    let mut names = BTreeSet::new();
    let mut name_bytes = 0usize;
    let mut payload_bytes = 0usize;
    for (destination, &len) in request.image_destinations.iter().zip(&request.image_lengths) {
        let key = destination.trim();
        if key.is_empty() || destination.len() > 8192 || !names.insert(key) {
            return Err(invalid("image destinations must be unique, nonempty and at most 8192 bytes"));
        }
        name_bytes += destination.len();
        if name_bytes > 65536 {
            return Err(invalid("image destinations exceed 64 KiB"));
        }
        if len == 0 || len as usize > SINGLE {
            return Err(invalid("images must contain 1 byte through 32 MiB each"));
        }
        payload_bytes = payload_bytes.checked_add(len as usize)
            .ok_or_else(|| invalid("image size overflow"))?;
        if payload_bytes > TOTAL {
            return Err(invalid("images exceed 128 MiB"));
        }
    }
    if payload_bytes != request.image_bytes_flat.len() {
        return Err(invalid("flattened image bytes do not match their declared lengths"));
    }
    let mut font_bytes = 0usize;
    for bytes in &request.fonts {
        if bytes.len() > SINGLE {
            return Err(invalid("fonts exceed 32 MiB per face"));
        }
        font_bytes += bytes.len();
        if font_bytes > TOTAL {
            return Err(invalid("fonts exceed 128 MiB"));
        }
    }
    if (!request.font_weights.is_empty() && request.font_weights.len() != 5)
        || request.font_weights.iter().any(|&weight| weight > 1000)
    {
        return Err(invalid("font weights must be empty or five integers in 0..=1000"));
    }
    Ok(())
}

// Keep the full admission/construction path callable without JsValue so native
// tests can exercise failures without invoking browser-only imported functions.
fn build_renderer(request: SiteRequest) -> Result<BookRenderer> {
    if request.paths.is_empty() || request.paths.len() > 4096
        || request.include_paths.len() > 4096 - request.paths.len()
    {
        return Err(invalid("expected 1..=4096 chapters and include sources combined"));
    }
    if !request.expand_includes && (!request.include_paths.is_empty() || !request.include_sources.is_empty()) {
        return Err(invalid("include-only resources require expandIncludes"));
    }
    validate_assets(&request)?;
    let mut metadata_bytes = 0usize;
    for text in [&request.title, &request.font, &request.dark_mode, &request.lang, &request.custom_css]
        .into_iter().flatten()
    {
        metadata_bytes = metadata_bytes.checked_add(text.len()).ok_or_else(|| invalid("metadata size overflow"))?;
        if metadata_bytes > 4 * 1024 * 1024 {
            return Err(invalid("metadata and CSS exceed 4 MiB combined"));
        }
    }
    let mut options = WasmRenderOptions::default();
    if let Some(font) = request.font.as_deref().filter(|text| !text.trim().is_empty()) {
        options = options.with_font_name(font)?;
    }
    if let Some(dark) = request.dark_mode.as_deref() {
        options.theme = options.theme.with_dark_mode(match dark.trim().to_ascii_lowercase().as_str() {
            "" | "auto" | "system" => DarkModePolicy::Auto,
            "disabled" | "disable" | "off" | "light" => DarkModePolicy::Disabled,
            _ => return Err(invalid("darkMode must be auto or disabled")),
        });
    }
    options.font_scale = match request.font_scale {
        Some(scale) if scale.is_finite() && (scale as f32).is_finite() && (scale as f32) > 0.0 => Some(scale as f32),
        Some(_) => return Err(invalid("fontScale must be finite, positive and representable as f32")),
        None => None,
    };
    options.toc_depth = match request.toc_depth {
        Some(depth @ 1..=6) => Some(depth as u8),
        Some(_) => return Err(invalid("tocDepth must be 1..=6")),
        None => None,
    };
    options.title = request.title.filter(|text| !text.is_empty());
    options.lang = request.lang.and_then(|text| {
        let value = text.trim();
        if value.is_empty() { None } else { Some(value.to_string()) }
    });
    // An explicit empty stylesheet means no default CSS, not absence.
    options.custom_css = request.custom_css;
    options.toc = request.toc;
    let mut total = 0usize;
    let chapters = source_inputs(request.paths, request.sources, &mut total)?;
    let resources = source_inputs(request.include_paths, request.include_sources, &mut total)?;
    let mut renderer = if request.expand_includes {
        BookRenderer::from_sources(&chapters, &resources)?
    } else {
        BookRenderer::new(&chapters)?
    };
    let mut offset = 0usize;
    for (destination, len) in request.image_destinations.into_iter().zip(request.image_lengths) {
        let end = offset + len as usize; // Full lengths/aggregate validated above.
        options.pdf_image_assets.push(PdfImageAsset {
            destination: destination.trim().to_string(),
            bytes: request.image_bytes_flat[offset..end].to_vec(),
        });
        offset = end;
    }
    for (index, (slot, bytes)) in FontAssetSlot::ALL.into_iter().zip(request.fonts).enumerate() {
        if !bytes.is_empty() {
            options.font_assets.set_slot(slot, bytes)?;
        }
        if let Some(&weight) = request.font_weights.get(index).filter(|&&value| value != 0) {
            options.font_assets.set_slot_weight(slot, weight as u16)?;
        }
    }
    *renderer.options_mut() = options;
    Ok(renderer)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn request() -> SiteRequest {
        SiteRequest { paths: vec!["guide/start.md".into()], sources: vec!["# Start".into()], ..SiteRequest::default() }
    }

    #[test]
    fn rejects_invalid_wire_arrays_lengths_weights_and_options_before_rendering() {
        let mut value = request();
        value.sources.clear();
        assert!(build_renderer(value).is_err());
        let mut value = request();
        value.font_weights = vec![0, 1001, 0, 0, 0];
        assert!(build_renderer(value).is_err());
        for lengths in [vec![], vec![0], vec![2], vec![33 * 1024 * 1024]] {
            let mut value = request();
            value.image_destinations = vec!["a.svg".into()];
            value.image_bytes_flat = vec![1];
            value.image_lengths = lengths;
            assert!(build_renderer(value).is_err());
        }
        for scale in [f64::NAN, f64::INFINITY, 0.0, -1.0, f64::MAX, f64::MIN_POSITIVE] {
            let mut value = request();
            value.font_scale = Some(scale);
            assert!(build_renderer(value).is_err());
        }
        let mut value = request();
        value.toc_depth = Some(7);
        assert!(build_renderer(value).is_err());
    }

    #[test]
    fn includes_are_expanded_without_promoting_resources_to_chapters() {
        let mut value = request();
        value.sources = vec!["# Start\n\n{{#include snippet.md}}\n".into()];
        value.include_paths = vec!["guide/snippet.md".into()];
        value.include_sources = vec!["Included prose.\n".into()];
        value.expand_includes = true;
        let expected = value.sources[0].len() + value.include_sources[0].len();
        let renderer = build_renderer(value).unwrap();
        assert_eq!(renderer.book().chapters.len(), 1);
        assert_eq!(renderer.source_length(), expected);
        assert!(format!("{:?}", renderer.book().chapters[0].doc).contains("Included prose."));
        let mut value = request();
        value.include_paths = vec!["guide/snippet.md".into()];
        value.include_sources = vec!["Included".into()];
        assert!(build_renderer(value).is_err());
    }

    #[test]
    fn supplied_images_css_language_and_navigation_reach_the_canonical_publisher() {
        let mut value = request();
        value.sources = vec!["# Start\n\n![Chart](figure.svg)".into()];
        value.title = Some("  Site  ".into());
        value.custom_css = Some(String::new());
        value.lang = Some("de".into());
        value.toc = true;
        value.toc_depth = Some(2);
        value.font_scale = Some(1.125);
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#.to_vec();
        value.image_destinations = vec!["guide/figure.svg".into()];
        value.image_lengths = vec![svg.len() as u32];
        value.image_bytes_flat = svg.clone();
        let renderer = build_renderer(value).unwrap();
        assert_eq!(renderer.options().title.as_deref(), Some("  Site  "));
        assert_eq!(renderer.options().custom_css.as_deref(), Some(""));
        assert_eq!(renderer.options().lang.as_deref(), Some("de"));
        assert!(renderer.options().toc);
        assert_eq!(renderer.options().toc_depth, Some(2));
        assert_eq!(renderer.options().font_scale, Some(1.125));
        assert_eq!(renderer.options().pdf_image_assets[0].bytes, svg);
        let bytes = renderer.render_site_publication().unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert_eq!(bytes, renderer.render_site_publication().unwrap());
    }

    #[test]
    fn root_site_publication_retains_missing_include_and_collision_errors() {
        let mut value = request();
        value.expand_includes = true;
        value.sources = vec!["{{#include missing.md}}".into()];
        assert!(build_renderer(value).is_err());
        let mut value = request();
        value.paths = vec!["a/b.md".into(), "a__b.md".into()];
        value.sources = vec!["# One".into(), "# Two".into()];
        assert!(build_renderer(value).is_err());
    }
}
