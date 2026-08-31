//! Browser/WASM-facing render API over the dependency-free core.
//!
//! This module intentionally does not depend on `wasm-bindgen`, JavaScript
//! glue, filesystem access, process environment, threads, or any native runtime
//! feature. It is the stable Rust-side contract that a future package generator
//! or hand-written host shim can expose to JS/TS without changing parser,
//! theme, HTML, or PDF behavior.

use crate::{
    DarkModePolicy, DiagnosticSeverity, FontAssetSlot, FontAssets, FontFamily, HtmlFontFormat,
    HtmlOptions, PdfImageAsset, PdfOptions, RenderError, Result, Theme, parse_markdown_spanned,
    render_html_document, render_pdf_document, render_warnings,
};

/// Output kind requested by a browser/WASM caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmOutputFormat {
    /// Complete self-contained HTML document bytes.
    Html,
    /// Deterministic PDF bytes.
    Pdf,
}

impl WasmOutputFormat {
    /// MIME type suitable for browser Blob creation.
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Html => "text/html; charset=utf-8",
            Self::Pdf => "application/pdf",
        }
    }

    /// Default file extension without a leading dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }

    /// Stable JSON/config spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
        }
    }
}

/// Render options that are safe to pass from a browser/WASM host.
#[derive(Debug, Clone, Default)]
pub struct WasmRenderOptions {
    /// Shared theme, including font family, dark-mode policy, spacing, colors,
    /// and page contract.
    pub theme: Theme,
    /// Optional document title.
    pub title: Option<String>,
    /// Optional PDF author metadata.
    pub author: Option<String>,
    /// Optional UTC Unix timestamp for deterministic PDF metadata.
    pub metadata_epoch_seconds: Option<u64>,
    /// Optional complete stylesheet replacement for HTML output.
    pub custom_css: Option<String>,
    /// Pass raw HTML through instead of escaping it. Keep false for untrusted
    /// browser/editor input.
    pub allow_raw_html: bool,
    /// Render muted line numbers in fenced code blocks for PDF output.
    pub code_line_numbers: bool,
    /// Render running page numbers in the bottom margin of PDF pages.
    pub page_numbers: bool,
    /// Optional base body size override in points; see [`crate::PdfOptions`].
    pub base_font_size: Option<f32>,
    /// Optional uniform typographic scale multiplier (e.g. 1.125 = 112.5% / Large).
    pub font_scale: Option<f32>,
    /// Optional per-step heading ratio; see [`crate::PdfOptions`].
    pub heading_scale: Option<f32>,
    /// Optional nominal table size override in points; see [`crate::PdfOptions`].
    pub table_font_size: Option<f32>,
    /// Caller-provided image bytes keyed by Markdown image destination.
    ///
    /// Browser hosts pass explicit bytes. The core never fetches URLs and never
    /// reads the filesystem.
    pub pdf_image_assets: Vec<PdfImageAsset>,
    /// Caller-provided font bytes. Missing slots use bundled deterministic
    /// fallback fonts.
    pub font_assets: FontAssets,
    /// Document language tag (e.g. "en", "de", "fr", "es", "nl").
    pub lang: Option<String>,
    /// Markdown authoring profile (e.g. CommonMark/GFM default vs GFM-plus).
    pub profile: Option<crate::Profile>,
    /// Generate a table of contents.
    pub toc: bool,
    /// Maximum heading depth for table of contents (e.g. 1..=6).
    pub toc_depth: Option<u8>,
    /// Optional target page count budget for PDF adaptive page fitting.
    pub fit_to_pages: Option<usize>,
    /// Opt-in microtypography for justified PDF body paragraphs (bead 544o):
    /// optical-margin protrusion. DISABLED by default; default output stays
    /// byte-identical.
    pub microtype: crate::layout::MicrotypeOptions,
}

impl WasmRenderOptions {
    /// Default sans-serif browser/WASM options.
    #[must_use]
    pub fn sans() -> Self {
        Self::default()
    }

    /// Serif browser/WASM options for long-form reading.
    #[must_use]
    pub fn serif() -> Self {
        Self {
            theme: Theme::serif(),
            ..Self::default()
        }
    }

    /// Return a copy with the given authoring profile.
    #[must_use]
    pub fn with_profile(mut self, profile: crate::Profile) -> Self {
        self.profile = Some(profile);
        self
    }

    /// Return a copy with table of contents generation enabled or disabled.
    #[must_use]
    pub fn with_toc(mut self, toc: bool) -> Self {
        self.toc = toc;
        self
    }

    /// Return a copy with a maximum table of contents heading depth.
    #[must_use]
    pub fn with_toc_depth(mut self, depth: u8) -> Self {
        self.toc_depth = Some(depth);
        self
    }

    /// Return a copy with microtypography protrusion enabled or disabled
    /// (PDF path only; justified body paragraphs).
    #[must_use]
    pub fn with_microtype_protrusion(mut self, enabled: bool) -> Self {
        self.microtype = if enabled {
            crate::layout::MicrotypeOptions::CONSERVATIVE
        } else {
            crate::layout::MicrotypeOptions::DISABLED
        };
        self
    }

    /// Return a copy with the document language tag set.
    #[must_use]
    pub fn with_lang(mut self, lang: impl Into<String>) -> Self {
        self.lang = Some(lang.into());
        self
    }

    /// Return a copy with the body font set from the stable config spelling.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] for unknown font names.
    pub fn with_font_name(mut self, font: &str) -> Result<Self> {
        let parsed = FontFamily::parse(font).ok_or_else(|| {
            RenderError::InvalidInput(format!("unknown font '{font}'; use 'sans' or 'serif'"))
        })?;
        self.theme = self.theme.with_font(parsed);
        Ok(self)
    }

    /// Return a copy with dark-mode CSS enabled or disabled.
    #[must_use]
    pub fn with_dark_mode(mut self, dark_mode: DarkModePolicy) -> Self {
        self.theme = self.theme.with_dark_mode(dark_mode);
        self
    }

    /// Return a copy with a custom stylesheet provided as UTF-8 bytes.
    ///
    /// Browser hosts commonly move assets as bytes. Accepting bytes here avoids
    /// imposing a JavaScript string conversion on the host while still keeping
    /// the renderer core dependency-free.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] when the bytes are not valid UTF-8.
    pub fn with_custom_css_bytes(mut self, css: &[u8]) -> Result<Self> {
        let css = std::str::from_utf8(css)
            .map_err(|_| RenderError::InvalidInput("custom CSS must be UTF-8".to_string()))?;
        self.custom_css = Some(css.to_string());
        Ok(self)
    }

    /// Return a copy with one supplied font asset appended/replaced.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] for malformed slot names or font
    /// bytes that the clean-room TrueType reader/subsetter cannot validate.
    pub fn with_font_asset_bytes(
        mut self,
        slot: FontAssetSlot,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Self> {
        self.font_assets.set_slot(slot, bytes)?;
        Ok(self)
    }

    /// Return a copy with one supplied font asset, using stable slot spelling.
    ///
    /// Valid slots are `body-regular`, `body-bold`, `body-italic`,
    /// `body-bold-italic`, and `mono-regular`.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] for unknown slots or malformed font
    /// bytes.
    pub fn with_font_asset_name(self, slot: &str, bytes: impl Into<Vec<u8>>) -> Result<Self> {
        let slot = FontAssetSlot::parse(slot).ok_or_else(|| {
            RenderError::InvalidInput(format!(
                "unknown font asset slot '{slot}'; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular"
            ))
        })?;
        self.with_font_asset_bytes(slot, bytes)
    }

    /// Pin CSS `font-weight` for a host-supplied (or VF-shared) slot.
    ///
    /// Variable `wght` faces instance at this location; static faces ignore the
    /// pin. Valid range is 1..=1000.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] when `weight` is outside 1..=1000.
    pub fn with_font_slot_weight(mut self, slot: FontAssetSlot, weight: u16) -> Result<Self> {
        self.font_assets.set_slot_weight(slot, weight)?;
        Ok(self)
    }

    /// Pin CSS `font-weight` using the stable slot spelling.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] for unknown slots or out-of-range
    /// weights.
    pub fn with_font_slot_weight_name(self, slot: &str, weight: u16) -> Result<Self> {
        let slot = FontAssetSlot::parse(slot).ok_or_else(|| {
            RenderError::InvalidInput(format!(
                "unknown font asset slot '{slot}'; use body-regular, body-bold, body-italic, body-bold-italic, or mono-regular"
            ))
        })?;
        self.with_font_slot_weight(slot, weight)
    }

    /// Return a copy with the given document title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Return a copy with the given PDF author metadata.
    #[must_use]
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Return a copy with an explicit UTC Unix timestamp for PDF CreationDate/ModDate.
    #[must_use]
    pub fn with_metadata_epoch_seconds(mut self, seconds: u64) -> Self {
        self.metadata_epoch_seconds = Some(seconds);
        self
    }

    /// Return a copy with raw HTML passthrough enabled or disabled.
    #[must_use]
    pub fn with_allow_raw_html(mut self, allow: bool) -> Self {
        self.allow_raw_html = allow;
        self
    }

    /// Return a copy with code-block line numbers enabled or disabled for PDF.
    #[must_use]
    pub fn with_code_line_numbers(mut self, line_numbers: bool) -> Self {
        self.code_line_numbers = line_numbers;
        self
    }

    /// Return a copy with one caller-provided PDF image asset attached.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] when the Markdown destination is
    /// blank. Image byte validation happens during PDF rendering so callers get
    /// the same supported-format behavior on native and WASM targets.
    pub fn with_pdf_image_asset(
        mut self,
        destination: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Self> {
        let destination = destination.into();
        if destination.trim().is_empty() {
            return Err(RenderError::InvalidInput(
                "image asset destination must not be blank".to_string(),
            ));
        }
        self.pdf_image_assets
            .push(PdfImageAsset::new(destination, bytes));
        Ok(self)
    }

    /// Return a copy with running page numbers enabled or disabled for PDF output.
    #[must_use]
    pub fn with_page_numbers(mut self, page_numbers: bool) -> Self {
        self.page_numbers = page_numbers;
        self
    }

    /// Return a copy with a base body font size override in points.
    #[must_use]
    pub fn with_base_font_size(mut self, points: f32) -> Self {
        self.base_font_size = Some(points);
        self
    }

    /// Return a copy with a uniform typographic font scale multiplier.
    #[must_use]
    pub fn with_font_scale(mut self, scale: f32) -> Self {
        self.font_scale = Some(scale);
        self
    }

    /// Return a copy with a per-step heading scale ratio override.
    #[must_use]
    pub fn with_heading_scale(mut self, ratio: f32) -> Self {
        self.heading_scale = Some(ratio);
        self
    }

    /// Return a copy with a nominal table font size override in points.
    #[must_use]
    pub fn with_table_font_size(mut self, points: f32) -> Self {
        self.table_font_size = Some(points);
        self
    }

    /// Return a copy with an adaptive target page budget for PDF generation.
    #[must_use]
    pub fn with_fit_to_pages(mut self, pages: usize) -> Self {
        self.fit_to_pages = Some(pages);
        self
    }

    pub(crate) fn html_options(&self) -> HtmlOptions {
        let mut theme = self.theme.clone();
        if let Some(scale) = self.font_scale {
            theme = theme.with_font_scale(crate::FontScale::from_factor(scale));
        }
        HtmlOptions {
            theme,
            title: self.title.clone(),
            custom_css: self.custom_css.clone(),
            allow_raw_html: self.allow_raw_html,
            font_assets: self.font_assets.clone(),
            image_assets: self.pdf_image_assets.clone(),
            lang: self.lang.clone(),
            profile: self.profile,
            toc: self.toc,
            toc_depth: self.toc_depth,
            html_font_format: HtmlFontFormat::default(),
        }
    }

    pub(crate) fn pdf_options(&self) -> PdfOptions {
        let mut theme = self.theme.clone();
        let mut base_font_size = self.base_font_size;
        if let Some(scale) = self.font_scale {
            let fs = crate::FontScale::from_factor(scale);
            theme = theme.with_font_scale(fs);
            if base_font_size.is_none() {
                base_font_size = Some(fs.pdf_base_pt());
            }
        }
        PdfOptions {
            theme,
            title: self.title.clone(),
            author: self.author.clone(),
            metadata_epoch_seconds: self.metadata_epoch_seconds,
            allow_raw_html: self.allow_raw_html,
            code_line_numbers: self.code_line_numbers,
            page_numbers: self.page_numbers,
            base_font_size,
            heading_scale: self.heading_scale,
            table_font_size: self.table_font_size,
            image_assets: self.pdf_image_assets.clone(),
            font_assets: self.font_assets.clone(),
            lang: self.lang.clone(),
            profile: self.profile,
            toc: self.toc,
            toc_depth: self.toc_depth,
            fit_to_pages: self.fit_to_pages,
            microtype: self.microtype,
            gradual_demerits: false,
            river_penalty: false,
            optimal_pagination: false,
            pareto_line_breaking: false,
        }
    }
}

/// Recoverable parser diagnostic for browser/editor hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmDiagnostic {
    /// Stable severity spelling: `warning` or `error`.
    pub severity: &'static str,
    /// Diagnostic byte start offset in the original Markdown.
    pub start: usize,
    /// Diagnostic byte end offset in the original Markdown.
    pub end: usize,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Render result bytes plus metadata that a JS/TS wrapper can map into a Blob
/// and diagnostics panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmRenderOutput {
    /// Output format.
    pub format: WasmOutputFormat,
    /// Browser MIME type for `bytes`.
    pub mime_type: &'static str,
    /// Default file extension for download UI.
    pub extension: &'static str,
    /// Rendered bytes. HTML is UTF-8; PDF is binary.
    pub bytes: Vec<u8>,
    /// Recoverable parser diagnostics collected before rendering.
    pub diagnostics: Vec<WasmDiagnostic>,
    /// Source size in bytes.
    pub source_len: usize,
}

impl WasmRenderOutput {
    /// Rendered byte length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when no rendered bytes were produced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Borrow HTML output as UTF-8 text when this result is HTML.
    #[must_use]
    pub fn html(&self) -> Option<&str> {
        if self.format == WasmOutputFormat::Html {
            std::str::from_utf8(&self.bytes).ok()
        } else {
            None
        }
    }

    /// Dependency-free diagnostics JSON for editor/browser panels.
    #[must_use]
    pub fn diagnostics_json(&self) -> String {
        diagnostics_json(&self.diagnostics)
    }
}

/// Render Markdown to self-contained HTML bytes using browser/WASM-safe options.
///
/// # Errors
/// Propagates renderer errors. Use
/// [`WasmRenderOptions::with_custom_css_bytes`] to validate byte-supplied CSS
/// before rendering.
pub fn render_html(markdown: &str, options: &WasmRenderOptions) -> Result<WasmRenderOutput> {
    let parsed = parse_markdown_spanned(markdown);
    let diagnostics = wasm_diagnostics(&parsed.diagnostics);
    let doc = parsed.into_document();
    let html = render_html_document(&doc, &options.html_options())?;
    Ok(WasmRenderOutput {
        format: WasmOutputFormat::Html,
        mime_type: WasmOutputFormat::Html.mime_type(),
        extension: WasmOutputFormat::Html.extension(),
        bytes: html.into_bytes(),
        diagnostics,
        source_len: markdown.len(),
    })
}

/// Render Markdown to deterministic PDF bytes using browser/WASM-safe options.
///
/// # Errors
/// Propagates renderer errors.
pub fn render_pdf(markdown: &str, options: &WasmRenderOptions) -> Result<WasmRenderOutput> {
    let parsed = parse_markdown_spanned(markdown);
    let mut diagnostics = wasm_diagnostics(&parsed.diagnostics);
    let doc = parsed.into_document();
    let pdf_options = options.pdf_options();
    // Surface content the PDF renderer degrades rather than embeds — unresolved
    // or undecodable images (rendered as alt text) and characters with no glyph
    // (rendered as .notdef). The native CLI prints these to stderr; a browser
    // host must get the same signal so degradation is never silent.
    for warning in render_warnings(&doc, &pdf_options) {
        diagnostics.push(WasmDiagnostic {
            severity: "warning",
            start: 0,
            end: 0,
            message: warning.message(),
        });
    }
    let bytes = render_pdf_document(&doc, &pdf_options)?;
    Ok(WasmRenderOutput {
        format: WasmOutputFormat::Pdf,
        mime_type: WasmOutputFormat::Pdf.mime_type(),
        extension: WasmOutputFormat::Pdf.extension(),
        bytes,
        diagnostics,
        source_len: markdown.len(),
    })
}

/// Stable JSON capability surface for browser/WASM packaging and tests.
#[must_use]
pub fn capabilities_json() -> String {
    "{\"schema\":\"fmd-wasm-capabilities-v1\",\
     \"outputs\":[\"html\",\"pdf\",\"svg\",\"epub\",\"interactive-html\",\"diff-html\",\"book-site\",\"book-pdf\"],\
     \"input\":\"markdown_utf8\",\
     \"html\":{\"mime_type\":\"text/html; charset=utf-8\",\"self_contained\":true,\"custom_css_utf8\":true,\"image_assets\":\"png_svg_v0_host_supplied_bytes\",\"font_assets\":\"ttf_v0_host_supplied_bytes\",\"font_slot_weight\":\"css_1_to_1000_variable_wght\"},\
     \"pdf\":{\"mime_type\":\"application/pdf\",\"deterministic_metadata_epoch\":true,\"image_assets\":\"png_svg_v0_host_supplied_bytes\",\"font_assets\":\"ttf_v0_host_supplied_bytes\",\"font_slot_weight\":\"css_1_to_1000_variable_wght\"},\
     \"diagnostics\":{\"source_spans\":\"byte_offsets\",\"json\":true},\
     \"document_intelligence\":{\"stats\":true,\"readability\":true,\"structural_lint\":true,\"accessibility_audit\":true,\"search_index\":true},\
     \"workflows\":{\"semantic_ast_diff\":true,\"in_memory_book_builder\":true,\"recursive_transclusion\":true,\"mermaid_to_svg\":true},\
     \"runtime_assumptions\":{\"filesystem\":false,\"process\":false,\"network\":false,\"threads\":false},\
     \"theme\":"
        .to_string()
        + &Theme::default().to_config_json()
        + "}"
}

fn wasm_diagnostics(diagnostics: &[crate::ParseDiagnostic]) -> Vec<WasmDiagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| WasmDiagnostic {
            severity: severity_str(diagnostic.severity),
            start: diagnostic.span.start,
            end: diagnostic.span.end,
            message: diagnostic.message.clone(),
        })
        .collect()
}

const fn severity_str(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
}

fn diagnostics_json(diagnostics: &[WasmDiagnostic]) -> String {
    let mut out = String::from("[");
    for (idx, diagnostic) in diagnostics.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"severity\":\"");
        json_escape_into(diagnostic.severity, &mut out);
        out.push_str("\",\"start\":");
        out.push_str(&diagnostic.start.to_string());
        out.push_str(",\"end\":");
        out.push_str(&diagnostic.end.to_string());
        out.push_str(",\"message\":\"");
        json_escape_into(&diagnostic.message, &mut out);
        out.push_str("\"}");
    }
    out.push(']');
    out
}

fn json_escape_into(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut clean_start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let esc = match b {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0..=0x1f | 0x7f => " ",
            _ => continue,
        };
        if clean_start < i {
            out.push_str(&s[clean_start..i]);
        }
        out.push_str(esc);
        clean_start = i + 1;
    }
    if clean_start < s.len() {
        out.push_str(&s[clean_start..]);
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod render_warning_tests {
    use super::{WasmRenderOptions, render_pdf};

    #[test]
    fn pdf_render_surfaces_dropped_image_and_missing_glyph_warnings() {
        // An image with no host-supplied asset (dropped to alt text) and a CJK
        // character with no glyph in the bundled Latin fonts must both surface as
        // "warning" diagnostics, so a browser host is never blind to degraded
        // output (parity with the native CLI's stderr warnings).
        let out = render_pdf(
            "![chart](missing.png)\n\n中文 body",
            &WasmRenderOptions::default(),
        )
        .unwrap();
        assert!(!out.is_empty());
        let warnings: Vec<&str> = out
            .diagnostics
            .iter()
            .filter(|d| d.severity == "warning")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            warnings.iter().any(|m| m.contains("missing.png")),
            "expected an unresolved-image warning, got {warnings:?}"
        );
        assert!(
            warnings.iter().any(|m| m.contains("glyph")),
            "expected a missing-glyph warning, got {warnings:?}"
        );
    }

    #[test]
    fn clean_pdf_render_reports_no_render_warnings() {
        // Plain ASCII with no images must not fabricate warnings.
        let out = render_pdf("# Title\n\nPlain body.", &WasmRenderOptions::default()).unwrap();
        assert!(
            out.diagnostics.iter().all(|d| d.severity != "warning"),
            "unexpected warnings: {:?}",
            out.diagnostics
        );
    }
}
