//! # franken_markdown
//!
//! A pure-Rust, dependency-lean, ultra-fast Markdown renderer. It takes a `.md`
//! file or raw Markdown text and renders it to either:
//!
//! * a **self-contained ("all-in-one") HTML** document that looks incredible by
//!   default (Cursor/GitHub-preview-like) and accepts a custom stylesheet, or
//! * a **tiny, optimized PDF** with beautiful styling, colours, and fonts and
//!   LaTeX-grade typesetting (Knuth–Plass optimal line breaking, real kerning,
//!   ligatures, leading, hyphenation).
//!
//! The library has **zero third-party dependencies** — every component (the
//! Markdown parser, the HTML emitter, the font/text subsystem, the line-breaking
//! and layout engine, and the PDF writer) is our own focused code. See
//! `COMPREHENSIVE_PLAN_FOR_FRANKEN_MARKDOWN.md`.
//!
//! ## Status
//!
//! Pre-Phase-0 scaffold. The HTML path renders today; the PDF path is wired as a
//! typed `not-yet-implemented` refusal while the layout/text/PDF subsystems are
//! built out (tracked in beads). Nothing here is final.
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "cli"), allow(dead_code))]

pub mod ast;
pub mod error;
pub mod html;
pub mod layout;
pub mod parse;
pub mod pdf;
pub mod text;
pub mod theme;

#[cfg(feature = "cli")]
pub mod cli;

pub use ast::Document;
pub use error::{RenderError, Result};
pub use theme::{FontFamily, Theme};

/// Options for the all-in-one HTML renderer.
#[derive(Debug, Clone)]
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

impl Default for HtmlOptions {
    fn default() -> Self {
        Self { theme: Theme::default(), title: None, custom_css: None, allow_raw_html: false }
    }
}

/// Options for the PDF renderer (the layout/text/PDF subsystems are in build-out).
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

/// Render Markdown source to a complete, self-contained HTML document string.
///
/// # Errors
/// Currently infallible for the HTML path, but returns [`Result`] so callers do
/// not have to change signatures as richer validation lands.
pub fn render_html(src: &str, opts: &HtmlOptions) -> Result<String> {
    let doc = parse(src);
    Ok(html::render(&doc, opts))
}

/// Render Markdown source to optimized PDF bytes.
///
/// # Errors
/// Returns [`RenderError::NotYetImplemented`] until the layout/text/PDF
/// subsystems land (tracked in beads); the AST + theme plumbing is already wired.
pub fn render_pdf(src: &str, opts: &PdfOptions) -> Result<Vec<u8>> {
    let doc = parse(src);
    pdf::render(&doc, opts)
}
