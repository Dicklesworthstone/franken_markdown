//! Resource-capable SVG browser ABI over the same renderer used by native hosts.

use wasm_bindgen::prelude::*;

use super::{
    FmdRenderResult, apply_font_assets, apply_font_weights, artifact_result,
    options_with_font_and_dark_mode, positive_f32, push_json_escaped, render_error_to_js,
};
use crate::wasm::WasmRenderOptions;
use crate::{FontAssetSlot, PdfImageAsset, SvgOptions};

const MIB: usize = 1024 * 1024;

/// Render an SVG poster with explicit image bytes and the shared five font slots.
/// Images arrive as parallel destinations, packed bytes and lengths. Empty font
/// slots select bundled faces. Nothing is fetched from a destination string.
///
/// The result retains parser diagnostics AND recoverable image/math warnings.
/// The original five-argument `renderSvgConfigured` remains supported.
///
/// # Errors
/// Rejects excessive source/assets, inconsistent packed arrays, duplicate image
/// keys, invalid fonts/weights and non-finite or out-of-range geometry. Input
/// admission precedes per-resource copies, font parsing and Markdown parsing.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderSvgConfiguredResources)]
pub fn render_svg_configured_resources(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    font_scale: Option<f64>,
    max_width_pt: Option<f64>,
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
        markdown.len(), &image_destinations, image_bytes_flat.len(), &image_bytes_lengths,
        &[body_regular.len(), body_bold.len(), body_italic.len(), body_bold_italic.len(), mono_regular.len()],
        &font_weights,
    ).map_err(JsValue::from_str)?;
    let width = geometry(font_scale, max_width_pt).map_err(JsValue::from_str)?;
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    apply_font_assets(&mut options, body_regular, body_bold, body_italic, body_bold_italic, mono_regular)?;
    apply_font_weights(&mut options, &font_weights)?;
    // Packing has been validated completely. Keep the SVG core's recoverable
    // invalid-image behavior rather than imposing the PDF decoder's format set.
    let mut offset = 0usize;
    for (destination, length) in image_destinations.iter().zip(&image_bytes_lengths) {
        let end = offset + *length as usize;
        options.pdf_image_assets.push(PdfImageAsset::new(
            destination.trim().to_owned(), image_bytes_flat[offset..end].to_vec(),
        ));
        offset = end;
    }
    render_with_options(markdown, &options, width).map_err(render_error_to_js)
}

fn geometry(scale: Option<f64>, width: Option<f64>) -> Result<f32, &'static str> {
    if let Some(scale) = scale {
        if !scale.is_finite() || !(scale as f32).is_finite() || (scale as f32) <= 0.0 {
            return Err("SVG fontScale must be positive and representable as a finite f32");
        }
    }
    let width = width.unwrap_or(612.0);
    if !width.is_finite() || !(144.0..=14_400.0).contains(&width) {
        return Err("SVG maxWidthPt must be a finite number from 144 through 14400");
    }
    Ok(width as f32)
}

// Length-only input permits boundary tests without allocating huge buffers.
// This check also protects callers using generated bindings without our JS API.
fn admit_assets(
    source_bytes: usize,
    destinations: &[String],
    flat_len: usize,
    lengths: &[u32],
    font_lengths: &[usize],
    weights: &[u32],
) -> Result<(), &'static str> {
    if source_bytes > 32 * MIB {
        return Err("SVG source exceeds 32 MiB");
    }
    if destinations.len() > 4096 || destinations.len() != lengths.len() || flat_len > 128 * MIB {
        return Err("SVG images exceed 4096 entries or 128 MiB, or parallel arrays disagree");
    }
    let mut total = 0usize;
    let mut names = 0usize;
    let mut unique = std::collections::BTreeSet::new();
    for (destination, &length) in destinations.iter().zip(lengths) {
        let name = destination.trim();
        if name.is_empty() || destination.len() > 8192 || name.chars().any(char::is_control) {
            return Err("SVG image needs a nonempty, control-free destination of at most 8192 bytes");
        }
        if !unique.insert(name) {
            return Err("SVG image destinations must be unique after trimming");
        }
        names = names.checked_add(destination.len()).ok_or("SVG image name byte count overflow")?;
        let length = usize::try_from(length).map_err(|_| "SVG image length overflow")?;
        if names > 64 * 1024 || length == 0 || length > 32 * MIB {
            return Err("SVG image must contain 1 byte through 32 MiB; names must total at most 64 KiB");
        }
        total = total.checked_add(length).ok_or("SVG image byte count overflow")?;
    }
    if total != flat_len {
        return Err("SVG flattened image bytes do not match the declared lengths");
    }
    if font_lengths.len() != FontAssetSlot::ALL.len() {
        return Err("SVG fonts require exactly five slots");
    }
    let mut total = 0usize;
    for &length in font_lengths {
        total = total.checked_add(length).ok_or("SVG font byte count overflow")?;
        if length > 32 * MIB || total > 128 * MIB {
            return Err("SVG font exceeds 32 MiB or combined faces exceed 128 MiB");
        }
    }
    if (!weights.is_empty() && weights.len() != FontAssetSlot::ALL.len())
        || weights.iter().any(|&weight| weight > 1000)
    {
        return Err("SVG font weights must be empty or five integers from 0 through 1000");
    }
    Ok(())
}

/// Shared by both ABI entry points. Plain documents keep their output bytes;
/// additional diagnostics do not require a second Markdown parse or render.
pub(super) fn render_with_options(
    markdown: &str,
    options: &WasmRenderOptions,
    width: f32,
) -> crate::Result<FmdRenderResult> {
    if markdown.len() > 32 * MIB {
        return Err(crate::RenderError::InvalidInput("SVG source exceeds 32 MiB".to_owned()));
    }
    // Resolve the theme without html_options() cloning all supplied asset bytes.
    let theme_options = WasmRenderOptions {
        theme: options.theme.clone(), font_scale: options.font_scale,
        ..WasmRenderOptions::default()
    };
    let svg_options = SvgOptions { theme: theme_options.html_options().theme, max_width_pt: width };
    let parsed = crate::parse_markdown_spanned(markdown);
    let mut diagnostics = String::from("[");
    for diagnostic in &parsed.diagnostics {
        let severity = match diagnostic.severity {
            crate::DiagnosticSeverity::Warning => "warning",
            crate::DiagnosticSeverity::Error => "error",
        };
        diagnostic_json(&mut diagnostics, severity, diagnostic.span.start, diagnostic.span.end,
            None, &diagnostic.message);
    }
    let (bytes, report, warnings) = crate::svg::render_svg_with_resources(
        &parsed.into_document(), &svg_options, &options.font_assets, &options.pdf_image_assets,
    )?;
    for warning in warnings {
        // The SVG core has no exact source span for these findings. 0..0 is
        // explicitly document-scoped, never an invented inline source location.
        diagnostic_json(&mut diagnostics, "warning", 0, 0, Some(warning.code), &warning.message);
    }
    if report.glyphs_missing > 0 {
        diagnostic_json(&mut diagnostics, "warning", 0, 0, Some("svg_missing_glyphs"),
            &format!("SVG omitted {} unmapped glyph(s)", report.glyphs_missing));
    }
    diagnostics.push(']');
    Ok(artifact_result("svg", "image/svg+xml", "svg", bytes, markdown.len(), diagnostics))
}

fn diagnostic_json(
    out: &mut String,
    severity: &str,
    start: usize,
    end: usize,
    code: Option<&str>,
    message: &str,
) {
    use std::fmt::Write as _;
    if out.len() > 1 { out.push(','); }
    let _ = write!(out, "{{\"severity\":\"{severity}\",\"start\":{start},\"end\":{end},\"message\":\"");
    push_json_escaped(out, message);
    out.push('"');
    if let Some(code) = code {
        out.push_str(",\"scope\":\"document\",\"code\":\"");
        push_json_escaped(out, code);
        out.push('"');
    }
    out.push('}');
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;
    const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"8\"><rect width=\"12\" height=\"8\"/></svg>";

    #[test]
    fn default_output_is_native_svg_with_the_existing_result_contract() {
        let source = "# Title\n\nText with **style** and $x^2$.";
        let options = WasmRenderOptions::default();
        let result = render_with_options(source, &options, 612.0).unwrap();
        assert_eq!(result.bytes(), crate::render_svg(&crate::parse_markdown(source), &SvgOptions::default()));
        assert_eq!(result.source_length(), source.len());
        assert_eq!(result.format(), "svg");
        assert_eq!(result.mime_type(), "image/svg+xml");
        assert_eq!(result.extension(), "svg");
        assert_eq!(result.diagnostics_json(), "[]");
    }

    #[test]
    fn images_and_custom_fonts_reach_the_same_resource_renderer() {
        let source = "# A\n\n![plot](plot.svg)\n\nA";
        let options = WasmRenderOptions::default()
            .with_pdf_image_asset("plot.svg", SVG.to_vec()).unwrap()
            .with_font_asset_bytes(FontAssetSlot::BodyRegular,
                crate::fonts::body_bytes(crate::FontFamily::Serif, crate::fonts::FontStyle::Regular).to_vec()).unwrap();
        let result = render_with_options(source, &options, 360.0).unwrap();
        let direct = crate::svg::render_svg_with_resources(&crate::parse_markdown(source),
            &SvgOptions { max_width_pt: 360.0, ..SvgOptions::default() },
            &options.font_assets, &options.pdf_image_assets).unwrap();
        assert_eq!(result.bytes(), direct.0);
        assert_eq!(result.diagnostics_json(), "[]");
        assert_ne!(result.bytes(), render_with_options(source, &WasmRenderOptions::default(), 360.0).unwrap().bytes());
    }

    #[test]
    fn recoverable_findings_survive_and_never_acquire_invented_spans() {
        let source = "![missing](missing.png)\n\n$\\fmdUnknownCommand{x}$\n\nA\u{0378}";
        let options = WasmRenderOptions::default();
        let result = render_with_options(source, &options, 612.0).unwrap();
        let diagnostics = result.diagnostics_json();
        for code in ["svg_math_unsupported", "svg_missing_glyphs"] {
            assert!(diagnostics.contains(code), "missing {code}: {diagnostics}");
        }
        assert!(diagnostics.contains("\"scope\":\"document\""));
        assert!(diagnostics.contains("\"start\":0,\"end\":0"));
        let again = render_with_options(source, &options, 612.0).unwrap();
        assert_eq!(result.bytes(), again.bytes());
        assert_eq!(diagnostics, again.diagnostics_json());
    }

    #[test]
    fn parser_diagnostics_are_retained_without_rendering_a_second_time() {
        let source = "# é\n\n```rust\nlet x = 1;";
        let parsed = crate::parse_markdown_spanned(source);
        assert!(!parsed.diagnostics.is_empty());
        let result = render_with_options(source, &WasmRenderOptions::default(), 612.0).unwrap();
        assert_eq!(result.source_length(), source.len());
        for diagnostic in &parsed.diagnostics {
            let mut escaped = String::new();
            push_json_escaped(&mut escaped, &diagnostic.message);
            assert!(result.diagnostics_json().contains(&escaped));
        }
    }

    #[test]
    fn diagnostic_json_escapes_quotes_backslashes_and_controls() {
        let mut json = String::from("[");
        diagnostic_json(&mut json, "warning", 2, 4, None, "a\"b\\c\n\u{0001}");
        diagnostic_json(&mut json, "warning", 0, 0, Some("svg_test"), "second");
        json.push(']');
        assert_eq!(json, "[{\"severity\":\"warning\",\"start\":2,\"end\":4,\"message\":\"a\\\"b\\\\c\\n\\u0001\"},{\"severity\":\"warning\",\"start\":0,\"end\":0,\"message\":\"second\",\"scope\":\"document\",\"code\":\"svg_test\"}]");
    }

    #[test]
    fn admission_checks_full_packing_and_canonical_duplicate_destinations() {
        let names = vec!["a.png".into(), "b.png".into()];
        assert!(admit_assets(0, &names, 2, &[1, 1], &[0; 5], &[]).is_ok());
        for lengths in [vec![1], vec![1, 0], vec![1, 2], vec![u32::MAX, 1]] {
            assert!(admit_assets(0, &names, 2, &lengths, &[0; 5], &[]).is_err());
        }
        for names in [vec!["a".into(), " a ".into()], vec![" ".into(), "b".into()], vec!["a\nq".into(), "b".into()]] {
            assert!(admit_assets(0, &names, 2, &[1, 1], &[0; 5], &[]).is_err());
        }
        assert!(admit_assets(0, &["a".repeat(8193)], 1, &[1], &[0; 5], &[]).is_err());
    }

    #[test]
    fn admission_bounds_large_payloads_without_allocating_them() {
        assert!(admit_assets(32 * MIB, &[], 0, &[], &[32 * MIB, 32 * MIB, 32 * MIB, 32 * MIB, 0], &[]).is_ok());
        assert!(admit_assets(32 * MIB + 1, &[], 0, &[], &[0; 5], &[]).is_err());
        assert!(admit_assets(0, &[], 128 * MIB + 1, &[], &[0; 5], &[]).is_err());
        assert!(admit_assets(0, &[], 0, &[], &[32 * MIB; 5], &[]).is_err());
        assert!(admit_assets(0, &vec![String::new(); 4097], 0, &[], &[0; 5], &[]).is_err());
        assert!(admit_assets(0, &[], 0, &[], &[0; 5], &[0, 0, 0, 0, 1001]).is_err());
        assert!(admit_assets(0, &[], 0, &[], &[0; 5], &[400]).is_err());
        assert!(admit_assets(0, &[], 0, &[], &[0; 5], &[100, 400, 700, 900, 1000]).is_ok());
    }

    #[test]
    fn geometry_rejects_nan_overflow_underflow_and_impossible_widths() {
        assert_eq!(geometry(None, None).unwrap(), 612.0);
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0, f64::MIN_POSITIVE, f64::MAX] {
            assert!(geometry(Some(bad), None).is_err());
        }
        for bad in [f64::NAN, f64::INFINITY, 143.0, 14_401.0] {
            assert!(geometry(None, Some(bad)).is_err());
        }
        assert_eq!(geometry(Some(1.25), Some(144.0)).unwrap(), 144.0);
        assert_eq!(geometry(None, Some(14_400.0)).unwrap(), 14_400.0);
    }
}
