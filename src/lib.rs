//! # franken_markdown
//!
//! A pure-Rust, dependency-lean, ultra-fast Markdown renderer. It takes a `.md`
//! file or raw Markdown text and renders it to either:
//!
//! * a **self-contained ("all-in-one") HTML** document that looks incredible by
//!   default (Cursor/GitHub-preview-like) and accepts a custom stylesheet, or
//! * a **tiny, deterministic PDF**. The current v0 writer uses built-in PDF
//!   base-14 fonts; the roadmap adds embedded curated fonts and LaTeX-grade
//!   typesetting (Knuth-Plass optimal line breaking, real kerning, ligatures,
//!   leading, hyphenation).
//!
//! The library has **zero third-party dependencies** — every component (the
//! Markdown parser, the HTML emitter, the font/text subsystem, the line-breaking
//! and layout engine, and the PDF writer) is our own focused code. See
//! `COMPREHENSIVE_PLAN_FOR_FRANKEN_MARKDOWN.md`.
//!
//! ## Status
//!
//! Pre-Phase-0 scaffold. The HTML path renders today with clean-room syntax
//! highlighting for common documentation languages. The PDF path renders a
//! compact deterministic v0; the high-typography layout/text/font subsystems
//! are still being built out (tracked in beads). Nothing here is final.
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "cli"), allow(dead_code))]

pub mod ast;
pub mod error;
pub mod highlight;
pub mod html;
pub mod layout;
pub mod parse;
pub mod pdf;
pub mod span;
pub mod text;
pub mod theme;

#[cfg(feature = "cli")]
pub mod cli;

pub use ast::Document;
pub use error::{RenderError, Result};
pub use span::{
    DiagnosticSeverity, ParseDiagnostic, SourceSpan, Spanned, SpannedBlock, SpannedDocument,
    SpannedInline, SpannedListItem, SpannedTable,
};
pub use theme::{
    CodeTheme, DarkModePolicy, FontFamily, MonoFontFamily, PageMargins, PageSize, PageStyle, Theme,
    ThemeColors, ThemeSpacing,
};

/// Options for the all-in-one HTML renderer.
#[derive(Debug, Clone, Default)]
pub struct HtmlOptions {
    /// Typography + colour theme used to build the default stylesheet.
    pub theme: Theme,
    /// Optional `<title>`; falls back to the first heading, then "Document".
    pub title: Option<String>,
    /// A complete replacement stylesheet. When `Some`, it is used verbatim
    /// instead of the generated default theme CSS (user-supplied stylesheets).
    pub custom_css: Option<String>,
    /// When false (default), raw inline/block HTML in the source is escaped and
    /// rendered as text rather than passed through — safe for untrusted input.
    pub allow_raw_html: bool,
}

/// Options for the PDF renderer.
#[derive(Debug, Clone, Default)]
pub struct PdfOptions {
    /// Typography + colour theme.
    pub theme: Theme,
    /// Optional document title metadata.
    pub title: Option<String>,
    /// When false (default), raw HTML is treated as text.
    pub allow_raw_html: bool,
}

/// Parse Markdown source into the document AST.
#[must_use]
pub fn parse(src: &str) -> Document {
    parse::parse_document(src)
}

/// Parse Markdown source into the document AST (alias of [`parse`]).
#[must_use]
pub fn parse_markdown(src: &str) -> Document {
    parse::parse_document(src)
}

/// Parse Markdown source into a spanned document with recoverable diagnostics.
///
/// This additive API is for editor/WASM integrations, diagnostics, and
/// conformance tooling. Renderer APIs continue to use [`Document`] directly.
#[must_use]
pub fn parse_markdown_spanned(src: &str) -> SpannedDocument {
    parse::parse_document_spanned(src)
}

/// Render an already-parsed document to a complete, self-contained HTML string.
///
/// Use this with [`parse_markdown`] to parse once and render multiple targets
/// (HTML and PDF) from one AST — the document-centric pipeline.
///
/// # Errors
/// Currently infallible for the HTML path, but returns [`Result`] so callers do
/// not have to change signatures as richer validation lands.
pub fn render_html_document(doc: &Document, opts: &HtmlOptions) -> Result<String> {
    Ok(html::render(doc, opts))
}

/// Render an already-parsed document to optimized PDF bytes.
///
/// # Errors
/// Propagates renderer errors; the HTML and PDF renderers share this one AST.
pub fn render_pdf_document(doc: &Document, opts: &PdfOptions) -> Result<Vec<u8>> {
    pdf::render(doc, opts)
}

/// Render Markdown source to a complete, self-contained HTML document string.
///
/// Convenience wrapper over [`parse_markdown`] + [`render_html_document`].
///
/// # Errors
/// See [`render_html_document`].
pub fn render_html(src: &str, opts: &HtmlOptions) -> Result<String> {
    render_html_document(&parse_markdown(src), opts)
}

/// Render Markdown source to optimized PDF bytes.
///
/// Convenience wrapper over [`parse_markdown`] + [`render_pdf_document`].
///
/// # Errors
/// See [`render_pdf_document`].
pub fn render_pdf(src: &str, opts: &PdfOptions) -> Result<Vec<u8>> {
    render_pdf_document(&parse_markdown(src), opts)
}
