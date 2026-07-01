//! Browser/WASM-facing render API over the dependency-free core.
//!
//! This module intentionally does not depend on `wasm-bindgen`, JavaScript
//! glue, filesystem access, process environment, threads, or any native runtime
//! feature. It is the stable Rust-side contract that a future package generator
//! or hand-written host shim can expose to JS/TS without changing parser,
//! theme, HTML, or PDF behavior.

use crate::{
    DarkModePolicy, DiagnosticSeverity, FontAssetSlot, FontAssets, FontFamily, HtmlOptions,
    PdfImageAsset, PdfOptions, RenderError, Result, Theme, parse_markdown_spanned,
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
    /// Caller-provided image bytes keyed by Markdown image destination.
    ///
    /// Browser hosts pass explicit bytes. The core never fetches URLs and never
    /// reads the filesystem.
    pub pdf_image_assets: Vec<PdfImageAsset>,
    /// Caller-provided font bytes. Missing slots use bundled deterministic
    /// fallback fonts.
    pub font_assets: FontAssets,
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

    /// Return a copy with one PDF image asset appended.
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

    fn html_options(&self) -> HtmlOptions {
        HtmlOptions {
            theme: self.theme.clone(),
            title: self.title.clone(),
            custom_css: self.custom_css.clone(),
            allow_raw_html: self.allow_raw_html,
            font_assets: self.font_assets.clone(),
        }
    }

    fn pdf_options(&self) -> PdfOptions {
        PdfOptions {
            theme: self.theme.clone(),
            title: self.title.clone(),
            author: self.author.clone(),
            metadata_epoch_seconds: self.metadata_epoch_seconds,
            allow_raw_html: self.allow_raw_html,
            code_line_numbers: self.code_line_numbers,
            image_assets: self.pdf_image_assets.clone(),
            font_assets: self.font_assets.clone(),
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
     \"outputs\":[\"html\",\"pdf\"],\
     \"input\":\"markdown_utf8\",\
     \"html\":{\"mime_type\":\"text/html; charset=utf-8\",\"self_contained\":true,\"custom_css_utf8\":true,\"font_assets\":\"ttf_v0_host_supplied_bytes\"},\
     \"pdf\":{\"mime_type\":\"application/pdf\",\"deterministic_metadata_epoch\":true,\"image_assets\":\"png_v0_host_supplied_bytes\",\"font_assets\":\"ttf_v0_host_supplied_bytes\"},\
     \"diagnostics\":{\"source_spans\":\"byte_offsets\",\"json\":true},\
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
        out.push_str(diagnostic.severity);
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
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod render_warning_tests {
    use super::{WasmRenderOptions, render_pdf};

    #[test]
    fn pdf_render_surfaces_dropped_image_and_missing_glyph_warnings() {
        // An image with no host-supplied asset (dropped to alt text) and a CJK
        // character with no glyph in the bundled Latin fonts must both surface as
        // "warning" diagnostics, so a browser host is never blind to degraded
        // output (parity with the native CLI's stderr warnings).
        let out = render_pdf("![chart](missing.png)\n\n中文 body", &WasmRenderOptions::default())
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
        let out =
            render_pdf("# Title\n\nPlain body.", &WasmRenderOptions::default()).unwrap();
        assert!(
            out.diagnostics.iter().all(|d| d.severity != "warning"),
            "unexpected warnings: {:?}",
            out.diagnostics
        );
    }
}
