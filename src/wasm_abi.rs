//! `wasm-bindgen` adapter for the browser package.
//!
//! This module is intentionally feature-gated behind `wasm-bindgen`. The core
//! `crate::wasm` API remains dependency-free and is the source of truth; this
//! file only maps that API into a JavaScript-callable shape without hand-written
//! unsafe pointer exports.

use wasm_bindgen::prelude::*;

use std::collections::{BTreeMap, BTreeSet};

use crate::wasm::{self, WasmRenderOptions};
use crate::{
    BookInput, DarkModePolicy, FontAssetSlot, SvgOptions, Theme, ZipWriter, book_pdf_document,
    build_book, build_search_index, compute_diff, compute_doc_stats, inject_book_nav,
    parse_markdown, render_epub, render_html_document, render_interactive_html,
    render_pdf_document, render_svg_with_report, rewrite_links_for_site, search_index_json,
};

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
    /// Stable output format (`html`, `pdf`, `svg`, `epub`, `zip`, or `diff-html`).
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

/// Compute the engine's document intelligence, readability, outline, and
/// structural-lint report without rendering a second time in the host.
#[wasm_bindgen(js_name = documentStats)]
#[must_use]
pub fn document_stats(markdown: &str) -> String {
    let document = parse_markdown(markdown);
    compute_doc_stats(markdown, &document).to_json()
}

/// Build the deterministic search index used by static-document experiences.
#[wasm_bindgen(js_name = searchIndex)]
#[must_use]
pub fn search_index(markdown: &str) -> String {
    let document = parse_markdown(markdown);
    search_index_json(&build_search_index(&document))
}

/// Compute a semantic AST diff and return its stable JSON contract.
#[wasm_bindgen(js_name = semanticDiff)]
#[must_use]
pub fn semantic_diff(
    old_markdown: &str,
    new_markdown: &str,
    old_name: Option<String>,
    new_name: Option<String>,
) -> String {
    let old = parse_markdown(old_markdown);
    let new = parse_markdown(new_markdown);
    compute_diff(
        &old,
        &new,
        old_name.as_deref().unwrap_or("Before"),
        new_name.as_deref().unwrap_or("Current"),
    )
    .to_json()
}

/// Render a semantic AST diff as a self-contained visual HTML document.
#[wasm_bindgen(js_name = renderSemanticDiffHtml)]
#[must_use]
pub fn render_semantic_diff_html(
    old_markdown: &str,
    new_markdown: &str,
    old_name: Option<String>,
    new_name: Option<String>,
) -> FmdRenderResult {
    let old = parse_markdown(old_markdown);
    let new = parse_markdown(new_markdown);
    let diff = compute_diff(
        &old,
        &new,
        old_name.as_deref().unwrap_or("Before"),
        new_name.as_deref().unwrap_or("Current"),
    );
    artifact_result(
        "diff-html",
        "text/html; charset=utf-8",
        "html",
        diff.to_html(&Theme::default()).into_bytes(),
        old_markdown.len() + new_markdown.len(),
        "[]".to_string(),
    )
}

/// Run the exact PDF verification pipeline and keep only authoring-time
/// accessibility findings.
#[wasm_bindgen(js_name = accessibilityAudit)]
pub fn accessibility_audit(markdown: &str) -> std::result::Result<String, JsValue> {
    let document = parse_markdown(markdown);
    let report =
        crate::verify::verify_pdf(&document, &crate::PdfOptions::default()).ok_or_else(|| {
            JsValue::from_str("accessibility audit could not materialize the document")
        })?;
    Ok(crate::verify::to_json(&crate::verify::filter_a11y(report)))
}

/// Render a standalone vector poster with glyph outlines embedded as paths.
#[wasm_bindgen(js_name = renderSvgConfigured)]
pub fn render_svg_configured(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    font_scale: Option<f64>,
    max_width_pt: Option<f64>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    let document = parse_markdown(markdown);
    let (bytes, report) = render_svg_with_report(
        &document,
        &SvgOptions {
            theme: options.html_options().theme,
            max_width_pt: finite_f32(max_width_pt).unwrap_or(612.0),
        },
    );
    let diagnostics = format!(
        "[{{\"severity\":\"warning\",\"start\":0,\"end\":0,\"message\":\"SVG omitted {} unmapped glyph(s)\"}}]",
        report.glyphs_missing
    );
    Ok(artifact_result(
        "svg",
        "image/svg+xml",
        "svg",
        bytes,
        markdown.len(),
        if report.glyphs_missing == 0 {
            "[]".to_string()
        } else {
            diagnostics
        },
    ))
}

/// Render an EPUB 3 e-book through the same parser and HTML theme model.
#[wasm_bindgen(js_name = renderEpubConfigured)]
pub fn render_epub_configured(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    lang: Option<String>,
    font_scale: Option<f64>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.lang = empty_to_none(lang);
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    let document = parse_markdown(markdown);
    let bytes = render_epub(&document, &options.html_options()).map_err(render_error_to_js)?;
    Ok(artifact_result(
        "epub",
        "application/epub+zip",
        "epub",
        bytes,
        markdown.len(),
        "[]".to_string(),
    ))
}

/// Render a self-hosting, single-file HTML workspace with its own editor,
/// preview, intelligence panel, and print/PDF path.
#[wasm_bindgen(js_name = renderInteractiveHtmlConfigured)]
pub fn render_interactive_html_configured(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    lang: Option<String>,
    font_scale: Option<f64>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.lang = empty_to_none(lang);
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    let document = parse_markdown(markdown);
    let html = render_interactive_html(&document, markdown, &options.html_options());
    Ok(artifact_result(
        "interactive-html",
        "text/html; charset=utf-8",
        "html",
        html.into_bytes(),
        markdown.len(),
        "[]".to_string(),
    ))
}

/// Compile in-memory Markdown files into a deterministic, zero-JavaScript HTML
/// site ZIP. The host owns file selection; the core owns include expansion,
/// link rewriting, navigation, parsing, rendering, and the search index.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderBookSite)]
pub fn render_book_site(
    paths: Vec<String>,
    sources: Vec<String>,
    title: Option<String>,
    font: Option<String>,
    dark_mode: Option<String>,
    font_scale: Option<f64>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let source_len = sources.iter().map(String::len).sum();
    let book = in_memory_book(paths, sources)?;
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    let html_options = options.html_options();
    let known_pages: BTreeSet<String> = book
        .chapters
        .iter()
        .map(|chapter| chapter.out_name.clone())
        .collect();
    let mut archive = ZipWriter::new();
    for chapter in &book.chapters {
        let mut document = chapter.doc.clone();
        rewrite_links_for_site(&mut document, &known_pages);
        let html = render_html_document(&document, &html_options).map_err(render_error_to_js)?;
        let html = inject_book_nav(&html, &book, &chapter.out_name);
        archive.add_deflated(&chapter.out_name, html.as_bytes());
    }
    let first = &book.chapters[0];
    let first_name = crate::book::escape_attr_pub(&first.out_name);
    let first_text = crate::book::escape_text_pub(&first.title);
    let index = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0; url={first_name}\"><title>{first_text}</title></head><body><p>Open <a href=\"{first_name}\">{first_text}</a>.</p></body></html>\n"
    );
    archive.add_deflated("index.html", index.as_bytes());
    let merged = book_pdf_document(&book);
    let index_json = search_index_json(&build_search_index(&merged));
    archive.add_deflated("search-index.json", index_json.as_bytes());
    let receipt = book_receipt_json(&book);
    archive.add_deflated("frankenmarkdown-receipt.json", receipt.as_bytes());
    Ok(artifact_result(
        "book-site",
        "application/zip",
        "zip",
        archive.finish(),
        source_len,
        "[]".to_string(),
    ))
}

/// Compile in-memory Markdown files into one continuous, bookmarked PDF book.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderBookPdf)]
pub fn render_book_pdf(
    paths: Vec<String>,
    sources: Vec<String>,
    title: Option<String>,
    author: Option<String>,
    font: Option<String>,
    dark_mode: Option<String>,
    font_scale: Option<f64>,
    page_numbers: bool,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let source_len = sources.iter().map(String::len).sum();
    let book = in_memory_book(paths, sources)?;
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.author = nonempty_verbatim(author);
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    options.toc = true;
    options.page_numbers = page_numbers;
    let document = book_pdf_document(&book);
    let bytes =
        render_pdf_document(&document, &options.pdf_options()).map_err(render_error_to_js)?;
    Ok(artifact_result(
        "book-pdf",
        "application/pdf",
        "pdf",
        bytes,
        source_len,
        "[]".to_string(),
    ))
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
    font_weights: Vec<u32>,
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
    apply_font_weights(&mut options, &font_weights)?;
    wasm::render_html(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Render Markdown to self-contained HTML with fonts and any number of host
/// image assets (data-URI inlined). Parallel image arrays match the PDF multi
/// ABI so the JS wrapper can share flattening.
///
/// # Errors
/// Returns a JavaScript error when the image arrays are inconsistent, an option
/// is invalid, or rendering fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderHtmlConfiguredMulti)]
pub fn render_html_configured_multi(
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
    font_weights: Vec<u32>,
    image_destinations: Vec<String>,
    image_bytes_flat: Vec<u8>,
    image_bytes_lengths: Vec<u32>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.custom_css = nonempty_verbatim(custom_css);
    options.allow_raw_html = allow_raw_html;
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
    wasm::render_html(markdown, &options)
        .map(render_result)
        .map_err(render_error_to_js)
}

/// Render Markdown to self-contained HTML with the complete browser option
/// set, including the core-owned uniform typographic scale.
///
/// This additive entry point keeps the original narrow ABI stable while giving
/// the ergonomic JavaScript wrapper one canonical path for fonts, images, and
/// type scale. The scale is interpreted by [`WasmRenderOptions`] and therefore
/// changes the Rust-generated theme rather than patching the returned HTML.
///
/// # Errors
/// Returns a JavaScript error when parallel image arrays are inconsistent, a
/// font asset is invalid, the scale is not positive and finite, or rendering
/// fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderHtmlConfiguredAdvanced)]
pub fn render_html_configured_advanced(
    markdown: &str,
    font: Option<String>,
    dark_mode: Option<String>,
    title: Option<String>,
    custom_css: Option<String>,
    allow_raw_html: bool,
    font_scale: Option<f64>,
    body_regular: Vec<u8>,
    body_bold: Vec<u8>,
    body_italic: Vec<u8>,
    body_bold_italic: Vec<u8>,
    mono_regular: Vec<u8>,
    font_weights: Vec<u32>,
    image_destinations: Vec<String>,
    image_bytes_flat: Vec<u8>,
    image_bytes_lengths: Vec<u32>,
    lang: Option<String>,
    toc: bool,
    toc_depth: Option<u32>,
) -> std::result::Result<FmdRenderResult, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.custom_css = nonempty_verbatim(custom_css);
    options.allow_raw_html = allow_raw_html;
    options.font_scale = positive_f32(font_scale, "fontScale")?;
    options.lang = empty_to_none(lang);
    options.toc = toc;
    options.toc_depth = heading_depth(toc_depth)?;
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
        None,
        None,
        None,
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
        None,
        None,
        None,
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

/// Render Markdown to PDF with browser package options, ANY number of image
/// assets, and caller-supplied font bytes. This is the general form the JS
/// wrapper uses so multi-image documents reach native↔WASM parity (the
/// single-image entry points above remain for a narrower ABI).
///
/// Images arrive as three parallel arrays — a destination per image, all image
/// bytes concatenated, and the byte length of each image — because wasm-bindgen
/// cannot pass a `Vec<Vec<u8>>` directly. Empty font byte arrays mean "use
/// bundled fallback" for that slot.
///
/// # Errors
/// Returns a JavaScript error when the image arrays are inconsistent, an option
/// is invalid, or rendering fails.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = renderPdfConfiguredMulti)]
pub fn render_pdf_configured_multi(
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
) -> std::result::Result<FmdRenderResult, JsValue> {
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
    base_font_size: Option<f64>,
    heading_scale: Option<f64>,
    table_font_size: Option<f64>,
) -> std::result::Result<WasmRenderOptions, JsValue> {
    let mut options = options_with_font_and_dark_mode(font, dark_mode)?;
    options.title = nonempty_verbatim(title);
    options.author = nonempty_verbatim(author);
    options.metadata_epoch_seconds = parse_epoch(metadata_epoch_seconds)?;
    options.allow_raw_html = allow_raw_html;
    options.code_line_numbers = code_line_numbers;
    options.base_font_size = finite_f32(base_font_size);
    options.heading_scale = finite_f32(heading_scale);
    options.table_font_size = finite_f32(table_font_size);
    Ok(options)
}

/// Reject non-finite host floats up front so the deterministic clamps in
/// [`crate::theme::TypeScale::resolve`] never see NaN/inf.
fn finite_f32(value: Option<f64>) -> Option<f32> {
    value.and_then(|v| {
        if !v.is_finite() {
            return None;
        }
        let f = v as f32;
        f.is_finite().then_some(f)
    })
}

fn positive_f32(
    value: Option<f64>,
    option_name: &str,
) -> std::result::Result<Option<f32>, JsValue> {
    let Some(value) = value else { return Ok(None) };
    if !value.is_finite() || value <= 0.0 {
        return Err(JsValue::from_str(&format!(
            "{option_name} must be a positive finite number"
        )));
    }
    let value = value as f32;
    if !value.is_finite() {
        return Err(JsValue::from_str(&format!(
            "{option_name} must fit in a finite 32-bit float"
        )));
    }
    Ok(Some(value))
}

fn heading_depth(value: Option<u32>) -> std::result::Result<Option<u8>, JsValue> {
    let Some(value) = value else { return Ok(None) };
    if !(1..=6).contains(&value) {
        return Err(JsValue::from_str(
            "tocDepth must be an integer from 1 through 6",
        ));
    }
    Ok(Some(value as u8))
}

fn optional_positive_usize(
    value: Option<u32>,
    option_name: &str,
) -> std::result::Result<Option<usize>, JsValue> {
    let Some(value) = value else { return Ok(None) };
    if value == 0 {
        return Err(JsValue::from_str(&format!(
            "{option_name} must be a positive integer"
        )));
    }
    Ok(Some(value as usize))
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

/// Parallel CSS `font-weight` pins for the five slots. Empty means "no pins".
/// Length 5: 0 = unset, 1..=1000 = pin. Any other length is invalid.
fn apply_font_weights(
    options: &mut WasmRenderOptions,
    weights: &[u32],
) -> std::result::Result<(), JsValue> {
    if weights.is_empty() {
        return Ok(());
    }
    if weights.len() != FontAssetSlot::ALL.len() {
        return Err(JsValue::from_str(
            "font_weights must be empty or a 5-element array (body-regular, body-bold, body-italic, body-bold-italic, mono-regular)",
        ));
    }
    for (slot, &weight) in FontAssetSlot::ALL.iter().zip(weights.iter()) {
        if weight == 0 {
            continue;
        }
        let weight = u16::try_from(weight).map_err(|_| {
            JsValue::from_str("font slot weight must be an integer 1..=1000 (0 means unset)")
        })?;
        options
            .font_assets
            .set_slot_weight(*slot, weight)
            .map_err(render_error_to_js)?;
    }
    Ok(())
}

fn split_nonempty_image_assets<'a>(
    image_destinations: &'a [String],
    image_bytes_flat: &'a [u8],
    image_bytes_lengths: &[u32],
) -> std::result::Result<Vec<(&'a str, &'a [u8])>, &'static str> {
    if image_destinations.len() != image_bytes_lengths.len() {
        return Err("image_destinations and image_bytes_lengths must have the same length");
    }

    let mut offset = 0usize;
    let mut assets = Vec::new();
    for (destination, &len) in image_destinations.iter().zip(image_bytes_lengths.iter()) {
        let len = len as usize;
        let end = offset
            .checked_add(len)
            .ok_or("image byte lengths overflow the flattened buffer")?;
        let bytes = image_bytes_flat
            .get(offset..end)
            .ok_or("flattened image bytes are shorter than declared")?;
        offset = end;
        // A fully-empty entry is a "no image" placeholder; skip it.
        if destination.trim().is_empty() && bytes.is_empty() {
            continue;
        }
        assets.push((destination.as_str(), bytes));
    }

    if offset != image_bytes_flat.len() {
        return Err("flattened image bytes are longer than the declared lengths");
    }
    Ok(assets)
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
    parse_epoch_u64(value).map_err(JsValue::from_str)
}

fn parse_epoch_u64(value: Option<f64>) -> std::result::Result<Option<u64>, &'static str> {
    const JS_MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    match value {
        Some(epoch) if epoch.is_finite() && epoch >= 0.0 && epoch.fract() == 0.0 => {
            if epoch > JS_MAX_SAFE_INTEGER {
                Err("metadataEpochSeconds must be <= Number.MAX_SAFE_INTEGER")
            } else {
                Ok(Some(epoch as u64))
            }
        }
        Some(_) => Err("metadataEpochSeconds must be a finite non-negative integer"),
        None => Ok(None),
    }
}

fn artifact_result(
    format: &str,
    mime_type: &str,
    extension: &str,
    bytes: Vec<u8>,
    source_len: usize,
    diagnostics_json: String,
) -> FmdRenderResult {
    FmdRenderResult {
        bytes,
        diagnostics_json,
        extension: extension.to_string(),
        format: format.to_string(),
        mime_type: mime_type.to_string(),
        source_len,
    }
}

fn in_memory_book(
    paths: Vec<String>,
    sources: Vec<String>,
) -> std::result::Result<crate::Book, JsValue> {
    if paths.len() != sources.len() {
        return Err(JsValue::from_str(
            "book paths and sources must have the same length",
        ));
    }
    if paths.is_empty() {
        return Err(JsValue::from_str("book needs at least one Markdown file"));
    }

    let mut ordered = Vec::with_capacity(paths.len());
    let mut files = BTreeMap::new();
    for (path, source) in paths.into_iter().zip(sources) {
        let normalized = normalize_book_path(&path).map_err(JsValue::from_str)?;
        if files.insert(normalized.clone(), source).is_some() {
            return Err(JsValue::from_str(&format!(
                "book contains the same normalized path twice: {normalized}"
            )));
        }
        ordered.push(normalized);
    }

    let mut inputs = Vec::with_capacity(ordered.len());
    for root_path in &ordered {
        let source = files
            .get(root_path)
            .expect("ordered book path must exist in in-memory map");
        let expanded = crate::transclude::expand_includes(source, &|requested, origin| {
            let including = if origin == "<input>" {
                root_path.as_str()
            } else {
                origin
            };
            let resolved = resolve_book_include(including, requested)?;
            Ok(files
                .get(&resolved)
                .map(|contents| (contents.clone(), resolved)))
        })
        .map_err(render_error_to_js)?;
        inputs.push(BookInput {
            path: root_path.clone(),
            source: expanded,
        });
    }
    build_book(&inputs).map_err(render_error_to_js)
}

fn resolve_book_include(origin: &str, requested: &str) -> std::result::Result<String, String> {
    if requested.starts_with('/') || requested.starts_with('\\') {
        return Err("include_escape: absolute paths are outside the selected book".to_string());
    }
    let parent = origin.rsplit_once('/').map_or("", |(parent, _)| parent);
    let joined = if parent.is_empty() {
        requested.to_string()
    } else {
        format!("{parent}/{requested}")
    };
    normalize_book_path(&joined).map_err(|detail| format!("include_escape: {detail}"))
}

fn normalize_book_path(path: &str) -> std::result::Result<String, &'static str> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() {
        return Err("book file path cannot be empty");
    }
    if path.starts_with('/') || path.contains(':') {
        return Err("book paths must be relative to the selected document group");
    }
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err("path leaves the selected document group");
                }
            }
            value => segments.push(value),
        }
    }
    if segments.is_empty() {
        return Err("book file path cannot resolve to the document-group root");
    }
    let normalized = segments.join("/");
    if !normalized.ends_with(".md") && !normalized.ends_with(".markdown") {
        return Err("book files must use .md or .markdown extensions");
    }
    Ok(normalized)
}

fn book_receipt_json(book: &crate::Book) -> String {
    let mut out = format!(
        "{{\"schema\":\"fmd-book-receipt-v1\",\"chapter_count\":{},\"chapters\":[",
        book.chapters.len()
    );
    for (index, chapter) in book.chapters.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"path\":\"");
        push_json_escaped(&mut out, &chapter.path);
        out.push_str("\",\"title\":\"");
        push_json_escaped(&mut out, &chapter.title);
        out.push_str("\",\"output\":\"");
        push_json_escaped(&mut out, &chapter.out_name);
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}

fn push_json_escaped(out: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{1f}' => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
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

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    #![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

    use super::{parse_epoch_u64, split_nonempty_image_assets};

    #[test]
    fn split_nonempty_image_assets_validates_parallel_arrays_without_js_values() {
        let destinations = vec![
            String::new(),
            "a.png".to_string(),
            "   ".to_string(),
            "b.png".to_string(),
        ];
        let flat = b"abcde".to_vec();
        let assets = split_nonempty_image_assets(&destinations, &flat, &[0, 2, 0, 3])
            .expect("valid image assets");

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0], ("a.png", &b"ab"[..]));
        assert_eq!(assets[1], ("b.png", &b"cde"[..]));

        let mismatch = split_nonempty_image_assets(&destinations[..1], &flat, &[0, 2]);
        assert_eq!(
            mismatch.unwrap_err(),
            "image_destinations and image_bytes_lengths must have the same length"
        );

        let short = split_nonempty_image_assets(&destinations[..1], &flat[..1], &[2]);
        assert_eq!(
            short.unwrap_err(),
            "flattened image bytes are shorter than declared"
        );

        let long = split_nonempty_image_assets(&destinations[..1], &flat[..2], &[1]);
        assert_eq!(
            long.unwrap_err(),
            "flattened image bytes are longer than the declared lengths"
        );
    }

    #[test]
    fn parse_epoch_u64_accepts_only_js_safe_non_negative_integer_seconds() {
        assert_eq!(parse_epoch_u64(None).unwrap(), None);
        assert_eq!(parse_epoch_u64(Some(0.0)).unwrap(), Some(0));
        assert_eq!(
            parse_epoch_u64(Some(9_007_199_254_740_991.0)).unwrap(),
            Some(9_007_199_254_740_991)
        );

        assert_eq!(
            parse_epoch_u64(Some(9_007_199_254_740_992.0)).unwrap_err(),
            "metadataEpochSeconds must be <= Number.MAX_SAFE_INTEGER"
        );
        for invalid in [f64::NAN, f64::INFINITY, -1.0, 1.5] {
            assert_eq!(
                parse_epoch_u64(Some(invalid)).unwrap_err(),
                "metadataEpochSeconds must be a finite non-negative integer"
            );
        }
    }
}
