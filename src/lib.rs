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
pub mod caret;
pub mod compress;
pub mod error;
pub mod fonts;
pub mod highlight;
pub mod html;
pub mod layout;
pub mod md_gen;
pub mod parse;
pub mod pdf;
pub mod scanner;
pub mod span;
/// The font subsystem, factored into the `fmd-font` workspace crate
/// (sfnt reader + glyf outline decoder). Re-exported under its historical
/// module name so `crate::text::Font` and the public
/// `franken_markdown::text::*` surface are unchanged.
pub use fmd_font as text;
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
#[cfg(feature = "cli")]
pub mod verify;
#[cfg(feature = "cli")]
pub mod watch;
// Native-only batch renderer; pulls Asupersync. Never compiled for the core,
// `--no-default-features`, or wasm builds.
#[cfg(feature = "batch")]
pub mod batch;

pub use ast::Document;
pub use caret::{CaretStyle, ColorMode, render_caret, render_parse_diagnostic};
pub use compress::zlib_decompress;
pub use error::{RenderError, Result};
pub use md_gen::{ADVERSARIES, Adversary, GenOptions, Lcg, adversarial, generate};
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
    /// All renderer slots, in stable order.
    pub const ALL: [Self; 5] = [
        Self::BodyRegular,
        Self::BodyBold,
        Self::BodyItalic,
        Self::BodyBoldItalic,
        Self::MonoRegular,
    ];

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

    /// CSS `font-weight` used when the slot has no explicit pin.
    ///
    /// Regular / italic / mono default to 400; bold / bold-italic default to 700.
    #[must_use]
    pub const fn default_weight(self) -> u16 {
        match self {
            Self::BodyBold | Self::BodyBoldItalic => 700,
            Self::BodyRegular | Self::BodyItalic | Self::MonoRegular => 400,
        }
    }
}

/// Optional caller-supplied TrueType font bytes for renderer font slots.
///
/// Missing slots use the bundled deterministic fonts. Supplied slots must be
/// parseable TrueType/sfnt fonts with `glyf` outlines so the HTML and PDF paths
/// can subset them without filesystem, fontconfig, or global mutable state.
///
/// Per-slot `*_weight` pins CSS `font-weight` for **variable** host faces
/// (`wght` axis). The HTML/PDF loaders instance the face at that location and
/// embed the resulting static glyf. Static faces ignore the pin (a
/// [`RenderWarning::FontWeightIgnoredStatic`] is recorded). When `body_bold` is
/// empty and `body_regular` is a `wght` variable face, bold resolves from that
/// same file at the bold slot's effective weight (default 700).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontAssets {
    pub body_regular: Option<Vec<u8>>,
    pub body_bold: Option<Vec<u8>>,
    pub body_italic: Option<Vec<u8>>,
    pub body_bold_italic: Option<Vec<u8>>,
    pub mono_regular: Option<Vec<u8>>,
    /// Pinned CSS font-weight for [`FontAssetSlot::BodyRegular`]. `None` → 400.
    pub body_regular_weight: Option<u16>,
    /// Pinned CSS font-weight for [`FontAssetSlot::BodyBold`]. `None` → 700.
    pub body_bold_weight: Option<u16>,
    /// Pinned CSS font-weight for [`FontAssetSlot::BodyItalic`]. `None` → 400.
    pub body_italic_weight: Option<u16>,
    /// Pinned CSS font-weight for [`FontAssetSlot::BodyBoldItalic`]. `None` → 700.
    pub body_bold_italic_weight: Option<u16>,
    /// Pinned CSS font-weight for [`FontAssetSlot::MonoRegular`]. `None` → 400.
    pub mono_regular_weight: Option<u16>,
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
        *self.bytes_mut(slot) = Some(bytes);
        Ok(())
    }

    /// Return a copy with a CSS `font-weight` pin for `slot`.
    ///
    /// Variable (`wght`) host faces instance at this location. Static faces
    /// ignore the pin. Valid range is 1..=1000 (CSS `font-weight`).
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] when `weight` is outside 1..=1000.
    pub fn with_slot_weight(mut self, slot: FontAssetSlot, weight: u16) -> Result<Self> {
        self.set_slot_weight(slot, weight)?;
        Ok(self)
    }

    /// Pin CSS `font-weight` for `slot`.
    ///
    /// # Errors
    /// See [`Self::with_slot_weight`].
    pub fn set_slot_weight(&mut self, slot: FontAssetSlot, weight: u16) -> Result<()> {
        validate_font_weight(slot, weight)?;
        *self.weight_mut(slot) = Some(weight);
        Ok(())
    }

    /// Explicit pin for `slot`, if any.
    #[must_use]
    pub fn slot_weight(&self, slot: FontAssetSlot) -> Option<u16> {
        match slot {
            FontAssetSlot::BodyRegular => self.body_regular_weight,
            FontAssetSlot::BodyBold => self.body_bold_weight,
            FontAssetSlot::BodyItalic => self.body_italic_weight,
            FontAssetSlot::BodyBoldItalic => self.body_bold_italic_weight,
            FontAssetSlot::MonoRegular => self.mono_regular_weight,
        }
    }

    /// Pin if set, otherwise [`FontAssetSlot::default_weight`].
    #[must_use]
    pub fn effective_weight(&self, slot: FontAssetSlot) -> u16 {
        self.slot_weight(slot).unwrap_or(slot.default_weight())
    }

    /// Caller-supplied bytes for `slot`, without variable-font sharing.
    #[must_use]
    pub fn slot_bytes(&self, slot: FontAssetSlot) -> Option<&[u8]> {
        match slot {
            FontAssetSlot::BodyRegular => self.body_regular.as_deref(),
            FontAssetSlot::BodyBold => self.body_bold.as_deref(),
            FontAssetSlot::BodyItalic => self.body_italic.as_deref(),
            FontAssetSlot::BodyBoldItalic => self.body_bold_italic.as_deref(),
            FontAssetSlot::MonoRegular => self.mono_regular.as_deref(),
        }
    }

    /// Bytes used to load `slot`: own bytes, or `body_regular` when this is
    /// the bold slot, bold is empty, and regular is a `wght` variable face.
    #[must_use]
    pub fn resolved_bytes(&self, slot: FontAssetSlot) -> Option<&[u8]> {
        if let Some(bytes) = self.slot_bytes(slot) {
            return Some(bytes);
        }
        if slot == FontAssetSlot::BodyBold {
            let regular = self.body_regular.as_deref()?;
            if font_bytes_have_wght(regular) {
                return Some(regular);
            }
        }
        None
    }

    /// Validate all populated slots and weight pins.
    ///
    /// This also protects callers who construct [`FontAssets`] directly instead
    /// of using [`Self::set_slot`].
    ///
    /// # Errors
    /// Returns [`RenderError::InvalidInput`] for the first malformed slot.
    pub fn validate(&self) -> Result<()> {
        for slot in FontAssetSlot::ALL {
            if let Some(bytes) = self.slot_bytes(slot) {
                validate_font_asset(slot, bytes)?;
            }
            if let Some(weight) = self.slot_weight(slot) {
                validate_font_weight(slot, weight)?;
            }
        }
        Ok(())
    }

    fn bytes_mut(&mut self, slot: FontAssetSlot) -> &mut Option<Vec<u8>> {
        match slot {
            FontAssetSlot::BodyRegular => &mut self.body_regular,
            FontAssetSlot::BodyBold => &mut self.body_bold,
            FontAssetSlot::BodyItalic => &mut self.body_italic,
            FontAssetSlot::BodyBoldItalic => &mut self.body_bold_italic,
            FontAssetSlot::MonoRegular => &mut self.mono_regular,
        }
    }

    fn weight_mut(&mut self, slot: FontAssetSlot) -> &mut Option<u16> {
        match slot {
            FontAssetSlot::BodyRegular => &mut self.body_regular_weight,
            FontAssetSlot::BodyBold => &mut self.body_bold_weight,
            FontAssetSlot::BodyItalic => &mut self.body_italic_weight,
            FontAssetSlot::BodyBoldItalic => &mut self.body_bold_italic_weight,
            FontAssetSlot::MonoRegular => &mut self.mono_regular_weight,
        }
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

fn validate_font_weight(slot: FontAssetSlot, weight: u16) -> Result<()> {
    if !(1..=1000).contains(&weight) {
        return Err(RenderError::InvalidInput(format!(
            "{} font-weight {weight} is out of range; CSS font-weight is 1..=1000",
            slot.as_str()
        )));
    }
    Ok(())
}

fn font_bytes_have_wght(bytes: &[u8]) -> bool {
    text::Font::parse(bytes.to_vec())
        .ok()
        .is_some_and(|font| font.instance_bounds(*b"wght").is_some())
}

/// Parse host font bytes and, when they carry a `wght` axis, instance at
/// `weight`. Static faces are returned unchanged (the pin is ignored).
pub(crate) fn instance_host_font(
    slot: FontAssetSlot,
    bytes: &[u8],
    weight: u16,
) -> Result<text::Font> {
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
    if font.instance_bounds(*b"wght").is_none() {
        return Ok(font);
    }
    Ok(font.instance(f32::from(weight)).unwrap_or(font))
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
    /// Render running page numbers in the bottom margin of PDF pages.
    pub page_numbers: bool,
    /// Optional base body size override, in points (clamped to [6, 24]).
    ///
    /// Scales the whole typographic hierarchy proportionally when no explicit
    /// heading scale is supplied. `None` keeps the theme's 11 pt default.
    pub base_font_size: Option<f32>,
    /// Optional per-step heading ratio (e.g. 1.25 = Major Third, clamped to
    /// [1.05, 2.0]). Rebuilds H1..H6 geometrically around an H1 anchor of
    /// `(24/11) x base_font_size`. `None` keeps the historical ladder.
    pub heading_scale: Option<f32>,
    /// Optional nominal table cell size override, in points
    /// (clamped to [5, base]). Adaptive table scaling still applies on top.
    pub table_font_size: Option<f32>,
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

impl PdfOptions {
    /// Effective materialized [`TypeScale`](crate::theme::TypeScale) after
    /// applying this render's typography overrides.
    #[must_use]
    pub fn type_scale(&self) -> crate::theme::TypeScale {
        crate::theme::TypeScale::resolve(
            self.base_font_size,
            self.heading_scale,
            self.table_font_size,
        )
    }
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
