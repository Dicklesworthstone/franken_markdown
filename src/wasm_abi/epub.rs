//! Additive, asset-capable EPUB ABI over the existing publication renderer.

use wasm_bindgen::prelude::*;

use super::{
    FmdRenderResult, apply_font_assets, apply_font_weights, artifact_result, empty_to_none,
    heading_depth, nonempty_verbatim, options_with_font_and_dark_mode, positive_f32,
    push_json_escaped, render_error_to_js, split_nonempty_image_assets,
};
use crate::wasm::WasmRenderOptions;

const MIB: usize = 1024 * 1024;

/// Render an EPUB with host-owned images, font faces, stylesheet and navigation.
/// The original six-argument `renderEpubConfigured` remains available unchanged.
/// No asset is fetched; the native EPUB writer owns XHTML, resource manifests,
/// font subsetting and ZIP serialization. Raw HTML is always escaped.
///
/// # Errors
/// Rejects invalid options, inconsistent packed arrays, excessive input/assets,
/// invalid fonts, and publication-rendering failures before publishing output.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderEpubConfiguredAdvanced)]
pub fn render_epub_configured_advanced(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    lang: Option<String>,
    font_scale: Option<f64>,
    custom_css: Option<String>,
    toc: bool,
    toc_depth: Option<u32>,
    image_destinations: Vec<String>,
    image_bytes_flat: Vec<u8>,
    image_bytes_lengths: Vec<u32>,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
    font_weights: Vec<u32>,
) -> Result<FmdRenderResult, JsValue> {
    admit_assets(
        markdown.len(), custom_css.as_ref().map_or(0, String::len),
        &image_destinations, &image_bytes_flat, &image_bytes_lengths,
        &[body_regular.len(), body_bold.len(), body_italic.len(), body_bold_italic.len(), mono_regular.len()],
    ).map_err(JsValue::from_str)?;
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.lang = empty_to_none(lang);
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    // An explicitly empty stylesheet is different from an absent stylesheet.
    options.custom_css = custom_css;
    options.toc = toc;
    options.toc_depth = heading_depth(toc_depth)?;
    for (destination, bytes) in
        split_nonempty_image_assets(&image_destinations, &image_bytes_flat, &image_bytes_lengths)
            .map_err(JsValue::from_str)?
    {
        options = options.with_pdf_image_asset(destination.to_owned(), bytes.to_vec())
            .map_err(render_error_to_js)?;
    }
    apply_font_assets(&mut options, body_regular, body_bold, body_italic, body_bold_italic, mono_regular)?;
    apply_font_weights(&mut options, &font_weights)?;
    render_publication(markdown, &options).map_err(render_error_to_js)
}

// Also checked in JavaScript before WASM conversion. Direct binding callers
// still pass this admission before per-asset copies, font parsing or layout.
fn admit_assets(
    source_bytes: usize,
    css_bytes: usize,
    destinations: &[String],
    bytes: &[u8],
    lengths: &[u32],
    font_lengths: &[usize],
) -> Result<(), &'static str> {
    if source_bytes > 32 * MIB || css_bytes > 4 * MIB {
        return Err("EPUB source exceeds 32 MiB or stylesheet exceeds 4 MiB");
    }
    if destinations.len() > 4096 || bytes.len() > 128 * MIB {
        return Err("EPUB image assets exceed 4096 entries or 128 MiB");
    }
    let mut destination_bytes = 0usize;
    let mut unique = std::collections::BTreeMap::new();
    for (destination, payload) in split_nonempty_image_assets(destinations, bytes, lengths)? {
        if destination.trim().is_empty() || destination.len() > 8192 || payload.is_empty() {
            return Err("EPUB image needs a nonempty destination (at most 8192 bytes) and payload");
        }
        destination_bytes = destination_bytes.checked_add(destination.len())
            .ok_or("EPUB image destination bytes overflow")?;
        if destination_bytes > 64 * 1024 || payload.len() > 32 * MIB {
            return Err("EPUB image exceeds 32 MiB or destination text exceeds 64 KiB");
        }
        if unique.insert(destination, ()).is_some() {
            return Err("EPUB image destinations must be unique");
        }
    }
    let mut total = 0usize;
    for &len in font_lengths {
        total = total.checked_add(len).ok_or("EPUB font byte count overflow")?;
        if len > 32 * MIB || total > 128 * MIB {
            return Err("EPUB font exceeds 32 MiB or combined faces exceed 128 MiB");
        }
    }
    Ok(())
}

fn render_publication(markdown: &str, options: &WasmRenderOptions) -> crate::Result<FmdRenderResult> {
    let parsed = crate::parse_markdown_spanned(markdown);
    let mut diagnostics = String::from("[");
    for (index, diagnostic) in parsed.diagnostics.iter().enumerate() {
        if index > 0 { diagnostics.push(','); }
        let severity = match diagnostic.severity {
            crate::DiagnosticSeverity::Warning => "warning",
            crate::DiagnosticSeverity::Error => "error",
        };
        use std::fmt::Write as _;
        let _ = write!(diagnostics,
            "{{\"severity\":\"{severity}\",\"start\":{},\"end\":{},\"message\":\"",
            diagnostic.span.start, diagnostic.span.end);
        push_json_escaped(&mut diagnostics, &diagnostic.message);
        diagnostics.push_str("\"}");
    }
    diagnostics.push(']');
    let mut html_options = options.html_options();
    html_options.allow_raw_html = false;
    let bytes = crate::render_epub(&parsed.into_document(), &html_options)?;
    Ok(artifact_result("epub", "application/epub+zip", "epub", bytes, markdown.len(), diagnostics))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn default_epub_bytes_match_the_original_native_export() {
        let source = "# Title\n\nText with **style** and $x^2$.";
        let options = WasmRenderOptions::default();
        let result = render_publication(source, &options).unwrap();
        assert_eq!(result.bytes(), crate::render_epub(&crate::parse_markdown(source), &options.html_options()).unwrap());
        assert_eq!(result.format(), "epub");
        assert_eq!(result.mime_type(), "application/epub+zip");
        assert_eq!(result.extension(), "epub");
        assert_eq!(result.source_length(), source.len());
        assert_eq!(result.bytes(), render_publication(source, &options).unwrap().bytes());
    }

    #[test]
    fn explicit_fonts_styles_and_images_reach_the_publication_writer() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>";
        let source = "# Title\n\n![plot](plot.svg)\n\n**Bold** and `code`.";
        let options = WasmRenderOptions::serif().with_title("  Book  ").with_lang("fr")
            .with_custom_css_bytes(b".fmd { line-height: 1.8 }").unwrap()
            .with_pdf_image_asset("plot.svg", svg.to_vec()).unwrap()
            .with_font_asset_bytes(crate::FontAssetSlot::BodyRegular,
                crate::fonts::body_bytes(crate::FontFamily::Serif, crate::fonts::FontStyle::Regular).to_vec()).unwrap();
        let result = render_publication(source, &options).unwrap();
        let bytes = result.bytes();
        assert_eq!(bytes, crate::render_epub(&crate::parse_markdown(source), &options.html_options()).unwrap());
        for name in ["OEBPS/assets/", "OEBPS/fonts/", "OEBPS/embedded-fonts.css"] {
            assert!(bytes.windows(name.len()).any(|window| window == name.as_bytes()), "missing {name}");
        }
        assert_ne!(bytes, render_publication(source, &WasmRenderOptions::default()).unwrap().bytes());
    }

    #[test]
    fn admission_rejects_bad_packing_duplicates_and_empty_payloads() {
        let destinations = vec!["a.png".into(), "b.png".into()];
        assert!(admit_assets(0, 0, &destinations, b"ab", &[1, 1], &[]).is_ok());
        for lengths in [vec![1], vec![1, 0], vec![1, 2]] {
            assert!(admit_assets(0, 0, &destinations, b"ab", &lengths, &[]).is_err());
        }
        assert!(admit_assets(0, 0, &["a".into(), "a".into()], b"ab", &[1, 1], &[]).is_err());
        assert!(admit_assets(0, 0, &[" ".into()], b"a", &[1], &[]).is_err());
        assert!(admit_assets(0, 0, &["a".into()], b"", &[0], &[]).is_err());
    }

    #[test]
    fn admission_bounds_counts_and_sizes_without_huge_allocations() {
        assert!(admit_assets(32 * MIB, 4 * MIB, &[], &[], &[], &[32 * MIB; 4]).is_ok());
        assert!(admit_assets(32 * MIB + 1, 0, &[], &[], &[], &[]).is_err());
        assert!(admit_assets(0, 4 * MIB + 1, &[], &[], &[], &[]).is_err());
        assert!(admit_assets(0, 0, &[], &[], &[], &[32 * MIB + 1]).is_err());
        assert!(admit_assets(0, 0, &[], &[], &[], &[32 * MIB; 5]).is_err());
        assert!(admit_assets(0, 0, &vec![String::new(); 4097], &[], &[], &[]).is_err());
    }

    #[test]
    fn empty_stylesheet_remains_distinct_and_raw_html_cannot_bypass_xhtml_policy() {
        let source = "# Safe\n\n<script>alert('x')</script>";
        let defaults = WasmRenderOptions::default();
        let custom = WasmRenderOptions { custom_css: Some(String::new()), allow_raw_html: true, ..defaults.clone() };
        let safe = WasmRenderOptions { allow_raw_html: false, ..custom.clone() };
        assert_eq!(render_publication(source, &custom).unwrap().bytes(), render_publication(source, &safe).unwrap().bytes());
        assert_ne!(render_publication(source, &custom).unwrap().bytes(), render_publication(source, &defaults).unwrap().bytes());
    }
}
