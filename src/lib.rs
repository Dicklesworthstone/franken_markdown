//! # franken_markdown
//!
//! A pure-Rust, dependency-lean, ultra-fast Markdown renderer. It takes a `.md`
//! file or raw Markdown text and renders it to either:
//!
//! * a **self-contained ("all-in-one") HTML** document that looks incredible by
//!   default (Cursor/GitHub-preview-like) and accepts a custom stylesheet, or
//! * a **tiny, deterministic PDF**. The current v0 writer embeds curated
//!   per-document font subsets with real metrics, focused GPOS kerning, GSUB
//!   ligatures, Knuth-Plass paragraph breaking, deterministic discretionary
//!   hyphenation/justification for body paragraphs, tagged-PDF structure, and
//!   selectable text; the roadmap adds deeper page layout (full widow/orphan
//!   control, keep-with-next, and richer block pagination).
//!
//! The library has **zero third-party dependencies** — every component (the
//! Markdown parser, the HTML emitter, the font/text subsystem, the line-breaking
//! and layout engine, and the PDF writer) is our own focused code. See
//! `COMPREHENSIVE_PLAN_FOR_FRANKEN_MARKDOWN.md`.
//!
//! ## Status
//!
//! Early but capable. The HTML path renders today with clean-room syntax
//! highlighting for common documentation languages. The PDF path renders a
//! compact deterministic embedded-font v0 with high-typography paragraph
//! layout; deeper page-builder polish is still being built out and tracked in
//! beads. Nothing here is final.
#![forbid(unsafe_code)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(not(feature = "cli"), allow(dead_code))]

pub mod ast;
pub mod compress;
pub mod error;
pub mod fonts;
pub mod highlight;
pub mod html;
pub mod layout;
pub(crate) mod line_break;
pub mod parse;
pub mod pdf;
pub mod scanner;
pub mod span;
pub mod text;
pub mod theme;
pub mod wasm;

#[cfg(feature = "wasm-bindgen")]
pub mod wasm_abi;

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod config;
#[cfg(feature = "cli")]
pub(crate) mod file_write;
// Native-only batch renderer; pulls Asupersync. Never compiled for the core,
// `--no-default-features`, or wasm builds.
#[cfg(feature = "batch")]
pub mod batch;

pub use ast::Document;
pub use error::{RenderError, Result};
pub use parse::{ParseProfile, ParseStageSummary, SpannedParseProfile};
pub use pdf::{PdfProfile, PdfStageSummary, RenderWarning, render_warnings};
pub use scanner::{
    ByteCandidateScan, ParserLineScan, TableFenceCandidateScan, WhitespaceScan,
    classify_ascii_whitespace, find_any_special_byte, find_html_escape, find_html_text_escape,
    find_pdf_escape, scan_byte_candidates, scan_markdown_line, scan_table_or_fence_candidate,
};
pub use span::{
    DiagnosticSeverity, ParseDiagnostic, SourceSpan, Spanned, SpannedBlock, SpannedDocument,
    SpannedInline, SpannedListItem, SpannedTable,
};
pub use theme::{
    CodeTheme, DarkModePolicy, FontFamily, MonoFontFamily, PageMargins, PageSize, PageStyle, Theme,
    ThemeColors, ThemeSpacing,
};

/// Crate version exposed for embedders that need renderer provenance.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Font slot for caller-supplied font bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontAssetSlot {
    /// Proportional body regular face.
    BodyRegular,
    /// Proportional body bold face.
    BodyBold,
    /// Proportional body italic face.
    BodyItalic,
    /// Proportional body bold-italic face.
    BodyBoldItalic,
    /// Monospace/code regular face.
    MonoRegular,
}

impl FontAssetSlot {
    /// Parse stable browser/config spelling.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "body-regular" | "body_regular" | "regular" => Some(Self::BodyRegular),
            "body-bold" | "body_bold" | "bold" => Some(Self::BodyBold),
            "body-italic" | "body_italic" | "italic" => Some(Self::BodyItalic),
            "body-bold-italic" | "body_bold_italic" | "bold-italic" | "bold_italic" => {
                Some(Self::BodyBoldItalic)
            }
            "mono-regular" | "mono_regular" | "mono" | "code" => Some(Self::MonoRegular),
            _ => None,
        }
    }

    /// Stable browser/config spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BodyRegular => "body-regular",
            Self::BodyBold => "body-bold",
            Self::BodyItalic => "body-italic",
            Self::BodyBoldItalic => "body-bold-italic",
            Self::MonoRegular => "mono-regular",
        }
    }
}

/// Optional caller-supplied TrueType font bytes for renderer font slots.
///
/// Missing slots use the bundled deterministic fonts. Supplied slots must be
/// parseable TrueType/sfnt fonts with `glyf` outlines so the HTML and PDF paths
/// can subset them without filesystem, fontconfig, or global mutable state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontAssets {
    pub body_regular: Option<Vec<u8>>,
    pub body_bold: Option<Vec<u8>>,
    pub body_italic: Option<Vec<u8>>,
    pub body_bold_italic: Option<Vec<u8>>,
    pub mono_regular: Option<Vec<u8>>,
}

impl FontAssets {
    /// True when every slot will use bundled fallback fonts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.body_regular.is_none()
            && self.body_bold.is_none()
            && self.body_italic.is_none()
            && self.body_bold_italic.is_none()
            && self.mono_regular.is_none()
    }

    /// Return a copy with one slot populated after deterministic validation.
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] when the bytes are empty,
    /// malformed, or not subsettable by the clean-room TrueType subsetter.
    pub fn with_slot(mut self, slot: FontAssetSlot, bytes: impl Into<Vec<u8>>) -> Result<Self> {
        self.set_slot(slot, bytes)?;
        Ok(self)
    }

    /// Populate one slot after deterministic validation.
    ///
    /// # Errors
    /// See [`Self::with_slot`].
    pub fn set_slot(&mut self, slot: FontAssetSlot, bytes: impl Into<Vec<u8>>) -> Result<()> {
        let bytes = bytes.into();
        validate_font_asset(slot, &bytes)?;
        match slot {
            FontAssetSlot::BodyRegular => self.body_regular = Some(bytes),
            FontAssetSlot::BodyBold => self.body_bold = Some(bytes),
            FontAssetSlot::BodyItalic => self.body_italic = Some(bytes),
            FontAssetSlot::BodyBoldItalic => self.body_bold_italic = Some(bytes),
            FontAssetSlot::MonoRegular => self.mono_regular = Some(bytes),
        }
        Ok(())
    }

    /// Validate all populated slots.
    ///
    /// This also protects callers who construct [`FontAssets`] directly instead
    /// of using [`Self::set_slot`].
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] for the first malformed slot.
    pub fn validate(&self) -> Result<()> {
        for (slot, bytes) in [
            (FontAssetSlot::BodyRegular, self.body_regular.as_deref()),
            (FontAssetSlot::BodyBold, self.body_bold.as_deref()),
            (FontAssetSlot::BodyItalic, self.body_italic.as_deref()),
            (
                FontAssetSlot::BodyBoldItalic,
                self.body_bold_italic.as_deref(),
            ),
            (FontAssetSlot::MonoRegular, self.mono_regular.as_deref()),
        ] {
            if let Some(bytes) = bytes {
                validate_font_asset(slot, bytes)?;
            }
        }
        Ok(())
    }
}

/// Upper bound on host-supplied font bytes per slot. A font is cloned and
/// subset, so an unbounded blob is an unmetered memory/CPU cost — the same
/// host-supplied-bytes threat the PDF image path already caps. 32 MiB is far
/// larger than any real subsettable TrueType face (even large CJK fonts) yet
/// bounds the worst case.
const MAX_FONT_ASSET_BYTES: usize = 32 * 1024 * 1024;

fn validate_font_asset(slot: FontAssetSlot, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        return Err(RenderError::InvalidInput(format!(
            "{} font bytes must not be empty",
            slot.as_str()
        )));
    }
    if bytes.len() > MAX_FONT_ASSET_BYTES {
        return Err(RenderError::InvalidInput(format!(
            "{} font bytes are {} bytes, over the {MAX_FONT_ASSET_BYTES}-byte limit",
            slot.as_str(),
            bytes.len()
        )));
    }
    let font = text::Font::parse(bytes.to_vec()).map_err(|err| {
        RenderError::InvalidInput(format!(
            "{} font bytes are not a supported TrueType font: {err}",
            slot.as_str()
        ))
    })?;
    if !font.has_glyf_outlines() {
        return Err(RenderError::InvalidInput(format!(
            "{} font bytes must contain TrueType glyf outlines for deterministic subsetting",
            slot.as_str()
        )));
    }
    Ok(())
}

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
    /// Optional caller-supplied fonts. Missing slots use bundled fonts.
    pub font_assets: FontAssets,
    /// Caller-provided image bytes keyed by the Markdown image destination.
    ///
    /// The HTML renderer emits supported host-supplied PNG/SVG/JPEG assets as data
    /// URIs after the source destination passes the normal safe-URL policy.
    /// Native CLI and browser/WASM callers resolve image destinations into
    /// explicit byte assets before rendering; the core never fetches network
    /// resources or reads files.
    pub image_assets: Vec<PdfImageAsset>,
}

/// Options for the PDF renderer.
#[derive(Debug, Clone, Default)]
pub struct PdfOptions {
    /// Typography + colour theme.
    pub theme: Theme,
    /// Optional document title metadata.
    pub title: Option<String>,
    /// Optional document author metadata.
    pub author: Option<String>,
    /// Optional UTC Unix timestamp for deterministic PDF CreationDate/ModDate.
    ///
    /// CLI callers usually populate this from `SOURCE_DATE_EPOCH`; library and
    /// WASM callers pass the value explicitly so the render core never reads
    /// process environment.
    pub metadata_epoch_seconds: Option<u64>,
    /// Raw HTML policy from the shared render surface.
    ///
    /// The PDF writer cannot pass HTML tags through as live markup. It preserves
    /// raw HTML source as visible text so PDF output does not silently drop user
    /// content when Markdown contains inline or block HTML.
    pub allow_raw_html: bool,
    /// Render muted line numbers in fenced code blocks.
    pub code_line_numbers: bool,
    /// Caller-provided image bytes keyed by the Markdown image destination.
    ///
    /// The render core never fetches network resources or reads files. Native
    /// CLI and browser/WASM callers resolve image destinations into explicit
    /// byte assets before rendering. Unsupported or missing assets fall back to
    /// visible alt text in PDF output.
    pub image_assets: Vec<PdfImageAsset>,
    /// Optional caller-supplied fonts. Missing slots use bundled fonts.
    pub font_assets: FontAssets,
}

/// Image bytes supplied by a host for PDF/HTML rendering.
///
/// `destination` is matched against the Markdown image destination after
/// trimming ASCII/Unicode whitespace. The first matching asset wins, keeping
/// behavior deterministic even if a caller accidentally supplies duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfImageAsset {
    pub destination: String,
    pub bytes: Vec<u8>,
}

impl PdfImageAsset {
    /// Construct a PDF image asset keyed by a Markdown image destination.
    #[must_use]
    pub fn new(destination: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            destination: destination.into(),
            bytes: bytes.into(),
        }
    }
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

/// Parse Markdown source into the document AST and collect parser stage timing.
#[must_use]
pub fn parse_markdown_profiled(src: &str) -> ParseProfile {
    parse::parse_document_profiled(src)
}

/// Parse Markdown source into a spanned document with recoverable diagnostics.
///
/// This additive API is for editor/WASM integrations, diagnostics, and
/// conformance tooling. Renderer APIs continue to use [`Document`] directly.
#[must_use]
pub fn parse_markdown_spanned(src: &str) -> SpannedDocument {
    parse::parse_document_spanned(src)
}

/// Parse Markdown source into a spanned document and collect parser stage timing.
#[must_use]
pub fn parse_markdown_spanned_profiled(src: &str) -> SpannedParseProfile {
    parse::parse_document_spanned_profiled(src)
}

/// Render an already-parsed document to a complete, self-contained HTML string.
///
/// Use this with [`parse_markdown`] to parse once and render multiple targets
/// (HTML and PDF) from one AST — the document-centric pipeline.
///
/// # Errors
/// Returns [`RenderError::InvalidInput`] when a host-supplied font asset is
/// invalid (empty, over the size limit, or not a subsettable TrueType face);
/// the render itself is otherwise infallible.
pub fn render_html_document(doc: &Document, opts: &HtmlOptions) -> Result<String> {
    opts.font_assets.validate()?;
    Ok(html::render(doc, opts))
}

/// Render an already-parsed document to optimized PDF bytes.
///
/// # Errors
/// Returns [`RenderError::InvalidInput`] when a host-supplied font asset is
/// invalid (empty, over the size limit, or not a subsettable TrueType face);
/// otherwise propagates renderer errors. The HTML and PDF renderers share this
/// one AST.
pub fn render_pdf_document(doc: &Document, opts: &PdfOptions) -> Result<Vec<u8>> {
    opts.font_assets.validate()?;
    pdf::render(doc, opts)
}

/// Render an already-parsed document to PDF bytes and collect per-stage timing.
///
/// This is intended for benchmarks, optimization beads, and diagnostics. Normal
/// render callers should use [`render_pdf_document`], which does not read clocks
/// or collect stage ledgers.
///
/// # Errors
/// See [`render_pdf_document`].
pub fn render_pdf_document_profiled(doc: &Document, opts: &PdfOptions) -> Result<PdfProfile> {
    opts.font_assets.validate()?;
    pdf::render_profiled(doc, opts)
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

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::{
        FontAssetSlot, FontAssets, MAX_FONT_ASSET_BYTES, PdfOptions, VERSION, parse_markdown,
        render_pdf_document_profiled,
    };

    #[test]
    fn version_constant_matches_package_metadata() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        assert!(!VERSION.trim().is_empty());
    }

    #[test]
    fn oversized_font_bytes_are_rejected() {
        // A host-supplied font over the per-slot cap is refused before it is
        // cloned and subset (bounds an unmetered memory/CPU cost).
        let mut assets = FontAssets::default();
        let too_big = vec![0u8; MAX_FONT_ASSET_BYTES + 1];
        assert!(
            assets
                .set_slot(FontAssetSlot::BodyRegular, too_big)
                .is_err(),
            "font bytes over the cap must be rejected"
        );
    }

    #[test]
    fn profiled_pdf_render_validates_font_assets_like_the_normal_path() {
        // The profiled entry point must apply the same font validation (and size
        // cap) as `render_pdf_document`; a directly-constructed FontAssets with
        // invalid bytes bypasses `set_slot`, so the render call is the guard.
        let doc = parse_markdown("# Hi");
        let opts = PdfOptions {
            font_assets: FontAssets {
                body_regular: Some(vec![0u8; 8]),
                ..FontAssets::default()
            },
            ..PdfOptions::default()
        };
        assert!(
            render_pdf_document_profiled(&doc, &opts).is_err(),
            "profiled PDF render must reject invalid host font assets"
        );
    }
}
