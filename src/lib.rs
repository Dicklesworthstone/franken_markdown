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

// Lets in-crate modules written against `franken_markdown::...` public paths
// compile both standalone (test harnesses) and in-tree.
extern crate self as franken_markdown;

pub mod ast;
pub mod book;
pub mod caret;
pub mod compress;
pub mod diagrams;
pub mod diff;
pub mod doc_stats;
pub mod error;
pub mod fonts;
pub mod highlight;
pub mod html;
pub mod layout;
pub mod md_gen;
pub mod parse;
pub mod pdf;
pub mod pdfa;
pub mod scanner;
pub mod span;
/// The font subsystem, factored into the `fmd-font` workspace crate
/// (sfnt reader + glyf outline decoder). Re-exported under its historical
/// module name so `crate::text::Font` and the public
/// `franken_markdown::text::*` surface are unchanged.
pub use fmd_font as text;
/// The TeX-mathematics layout and MathML engine, factored into the
/// `fmd-math` workspace crate.
pub use fmd_math as math;
pub mod epub;
pub mod interactive;
pub mod search_index;
pub mod svg;
pub mod theme;
pub mod transclude;
pub mod wasm;
pub mod woff1;
pub mod zip;

#[cfg(feature = "wasm-bindgen")]
pub mod wasm_abi;

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod config;
#[cfg(feature = "cli")]
pub(crate) mod file_write;
pub mod verify;
#[cfg(feature = "cli")]
pub mod watch;
// Native-only batch renderer; pulls Asupersync. Never compiled for the core,
// `--no-default-features`, or wasm builds.
#[cfg(feature = "batch")]
pub mod batch;
// Native-only MCP stdio server. Never compiled for the core or wasm builds.
#[cfg(feature = "mcp")]
pub mod mcp;

pub use ast::{Align, Block, DefinitionItem, Document, Inline, List, ListItem, Table};
pub use book::{
    Book, BookChapter, BookHeading, BookInput, book_pdf_document, build_book, chapter_headings,
    inject_book_nav, out_name, rewrite_links_for_site,
};
pub use caret::{CaretStyle, ColorMode, render_caret, render_parse_diagnostic};
pub use compress::zlib_decompress;
pub use diagrams::{is_diagram_code, render_diagram_svg};
pub use diff::{DiffBlock, DiffInline, DiffStats, DocumentDiff, compute_diff};
pub use doc_stats::{
    DocFinding, DocumentStats, DocumentStructure, OutlineHeading, compute_doc_stats,
};
pub use epub::render_epub;
pub use error::{RenderError, Result};
pub use interactive::render_interactive_html;
pub use md_gen::{ADVERSARIES, Adversary, GenOptions, Lcg, adversarial, generate};
pub use parse::{ParseProfile, ParseStageSummary, SpannedParseProfile};
pub use pdf::{
    PdfEmitOptions, PdfPageEmission, PdfProfile, PdfStageSummary, RenderWarning, render_warnings,
};
pub use pdfa::{PdfAMode, PdfASettings};
pub use scanner::{
    ByteCandidateScan, ParserLineScan, TableFenceCandidateScan, WhitespaceScan,
    classify_ascii_whitespace, find_any_special_byte, find_html_escape, find_html_text_escape,
    find_pdf_escape, scan_byte_candidates, scan_markdown_line, scan_table_or_fence_candidate,
};
pub use search_index::{SearchIndex, build_search_index, search_index_json};
pub use span::{
    DiagnosticSeverity, ParseDiagnostic, SourceSpan, Spanned, SpannedBlock, SpannedDocument,
    SpannedInline, SpannedListItem, SpannedTable,
};
pub use svg::{SvgOptions, SvgReport, render_svg, render_svg_with_report};
pub use theme::{
    CodeTheme, DarkModePolicy, FontFamily, FontScale, MonoFontFamily, PageMargins, PageSize,
    PageStyle, Theme, ThemeColors, ThemeSpacing, TypeScale, TypeScalePreset,
};
pub use transclude::expand_includes;
pub use zip::{ZipWriter, crc32};

/// Font container format for embedded `@font-face` subsets in HTML output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HtmlFontFormat {
    /// Raw TrueType subset bytes (`data:font/ttf`). Widest compatibility with
    /// non-browser tools that sniff the payload.
    Ttf,
    /// WOFF1-wrapped subset (`data:font/woff`) compressed with the renderer's
    /// own deterministic DEFLATE. ~45–55% smaller on the bundled faces and
    /// supported by every modern browser. Default since 0.4.1 (bead ge1t).
    #[default]
    Woff1,
}

impl HtmlFontFormat {
    /// Stable CLI/config spelling.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ttf" | "truetype" => Some(Self::Ttf),
            "woff" | "woff1" => Some(Self::Woff1),
            _ => None,
        }
    }

    /// Stable CLI/config spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ttf => "ttf",
            Self::Woff1 => "woff1",
        }
    }
}

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

/// Authoring profile for Markdown parsing and rendering (ryu4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    /// Standard CommonMark + GitHub Flavored Markdown (tables, task lists, strikethrough, autolinks).
    #[default]
    CommonMarkGfm,
    /// GFM-Plus: CommonMark + GFM + Footnotes + GitHub Alerts + Definition Lists.
    GfmPlus,
}

impl Profile {
    /// Parse profile from string identifier.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "commonmark" | "gfm" | "commonmark-gfm" | "default" => Some(Self::CommonMarkGfm),
            "gfm-plus" | "gfm_plus" | "plus" => Some(Self::GfmPlus),
            _ => None,
        }
    }

    /// Canonical string identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CommonMarkGfm => "commonmark-gfm",
            Self::GfmPlus => "gfm-plus",
        }
    }
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
    /// Document language tag (e.g. "en", "de", "fr", "es", "nl").
    pub lang: Option<String>,
    /// Markdown authoring profile (e.g. CommonMark/GFM default vs GFM-plus).
    pub profile: Option<Profile>,
    /// Generate a table of contents.
    pub toc: bool,
    /// Maximum heading depth for table of contents (e.g. 1..=6).
    pub toc_depth: Option<u8>,
    /// Font container format for embedded subsets. WOFF1 (the default) is
    /// byte-deterministic and ~half the bytes of raw TTF subsets.
    pub html_font_format: HtmlFontFormat,
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
    /// Document language tag for hyphenation and metadata (e.g. "en", "de", "fr", "es", "nl").
    pub lang: Option<String>,
    /// Markdown authoring profile (e.g. CommonMark/GFM default vs GFM-plus).
    pub profile: Option<Profile>,
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
    /// Generate a table of contents.
    pub toc: bool,
    /// Maximum heading depth for table of contents (e.g. 1..=6).
    pub toc_depth: Option<u8>,
    /// Optional target page count budget. When set to `Some(N)`, the PDF engine
    /// runs a micro-typographic solver to fit the document into at most N pages.
    pub fit_to_pages: Option<usize>,
    /// Opt-in microtypography for justified body paragraphs (bead 544o):
    /// optical-margin protrusion via the precomputed per-box hooks in
    /// `layout` (docs/MICROTYPOGRAPHY.md). DISABLED by default — default
    /// output stays byte-identical.
    pub microtype: crate::layout::MicrotypeOptions,
    /// Enable gradual adjacent demerits (Verna, DocEng '25) in the Knuth-Plass
    /// line breaker for justified paragraphs: replaces the coarse 4-class
    /// binary fitness check with a linear penalty proportional to the
    /// fine-grained spacing-ratio difference between consecutive lines.
    /// Default false — classic KP behavior, byte-identical output. See
    /// docs/MICROTYPOGRAPHY.md for the quality metrics this improves.
    pub gradual_demerits: bool,
    /// Enable river-seed demerits in the Knuth-Plass line breaker: penalize
    /// break candidates whose previous line's last inter-word space aligns
    /// horizontally (within 1% of the measure) with a space in the candidate
    /// line — the two-line seed of a vertical whitespace channel ("river").
    /// Default false — classic behavior, byte-identical output.
    pub river_penalty: bool,
    /// Opt-in Plass-style optimal pagination (Plass & Li, 1981): replace the
    /// greedy per-page breaker with a document-wide DP that minimizes the
    /// total of the same void-badness + keep-penalty costs the greedy path
    /// applies per page. Better page fills and fewer stranded headings when
    /// content is tight; O(lines × page-window) per render. Default false —
    /// greedy pagination, byte-identical output.
    pub optimal_pagination: bool,
    /// Enable multi-objective (Pareto) line breaking (Holkner): track bounded
    /// fronts of non-dominated states over two demerit dimensions (structure:
    /// badness/fitness/rivers/overflow; hyphenation: break penalties and
    /// flagged-flag adjacency) instead of a single scalar winner. The final
    /// pick remains min-scalar, but paths that trade structure against
    /// hyphenation survive the search. Default false — byte-identical.
    pub pareto_line_breaking: bool,
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
    render_pdf_document_emitted(doc, opts, PdfEmitOptions::default())
}

/// Render a document to PDF with an explicit page-emission mode.
///
/// [`PdfPageEmission::Chunked`] is the production writer: it paginates, draws,
/// and compresses one page at a time. [`PdfPageEmission::Monolithic`] holds
/// every placed page until the object write and exists so tests can prove the
/// two paths emit identical bytes.
///
/// # Errors
/// See [`render_pdf_document`]. [`PdfEmitOptions::max_retained_bytes`] also
/// returns [`RenderError::InvalidInput`] with a `pdf_heap_ceiling:` prefix.
pub fn render_pdf_document_emitted(
    doc: &Document,
    opts: &PdfOptions,
    emit: PdfEmitOptions,
) -> Result<Vec<u8>> {
    opts.font_assets.validate()?;
    let doc = transform_footnotes_for_pdf(doc);
    pdf::render_with_emit(&doc, opts, PdfASettings::OFF, emit)
}

/// Convenience wrapper over [`parse_markdown`] + [`render_pdf_document_emitted`].
///
/// # Errors
/// See [`render_pdf_document_emitted`].
pub fn render_pdf_emitted(src: &str, opts: &PdfOptions, emit: PdfEmitOptions) -> Result<Vec<u8>> {
    render_pdf_document_emitted(&parse_markdown(src), opts, emit)
}

/// Render a document to PDF with PDF/A-2b identification (XMP + sRGB
/// OutputIntent). [`PdfASettings::OFF`] is identical to [`render_pdf_document`].
///
/// # Errors
/// See [`render_pdf_document`]. Strict mode also returns
/// [`RenderError::InvalidInput`] with a `pdf_a_*` code for constructs PDF/A-2b
/// cannot carry (currently `javascript:` and `file:` URI actions).
pub fn render_pdf_document_pdfa(
    doc: &Document,
    opts: &PdfOptions,
    pdf_a: PdfASettings,
) -> Result<Vec<u8>> {
    opts.font_assets.validate()?;
    if pdf_a.strict {
        validate_doc_pdfa_strict(doc)?;
    }
    let doc = transform_footnotes_for_pdf(doc);
    pdf::render(&doc, opts, pdf_a)
}

fn validate_doc_pdfa_strict(doc: &Document) -> Result<()> {
    use crate::ast::{Block, Inline};

    fn check_inlines(inlines: &[Inline]) -> Result<()> {
        for inline in inlines {
            match inline {
                Inline::Link { dest, content, .. } => {
                    crate::pdfa::check_uri_action(crate::pdfa::PdfASettings::a2b_strict(), dest)?;
                    check_inlines(content)?;
                }
                Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                    check_inlines(c)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_blocks(blocks: &[Block]) -> Result<()> {
        for block in blocks {
            match block {
                Block::Paragraph(inlines) | Block::Heading { inlines, .. } => {
                    check_inlines(inlines)?;
                }
                Block::BlockQuote(b) => check_blocks(b)?,
                Block::List(list) => {
                    for item in &list.items {
                        check_blocks(&item.blocks)?;
                    }
                }
                Block::Table(table) => {
                    for cell in &table.head {
                        check_inlines(cell)?;
                    }
                    for row in &table.rows {
                        for cell in row {
                            check_inlines(cell)?;
                        }
                    }
                }
                Block::FootnoteDefinition { blocks, .. } => {
                    check_blocks(blocks)?;
                }
                Block::DefinitionList(groups) => {
                    for item in groups {
                        for t in &item.terms {
                            check_inlines(t)?;
                        }
                        for d in &item.definitions {
                            check_inlines(d)?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    check_blocks(&doc.blocks)
}

/// Convenience wrapper over [`parse_markdown`] + [`render_pdf_document_pdfa`].
///
/// # Errors
/// See [`render_pdf_document_pdfa`].
pub fn render_pdf_pdfa(src: &str, opts: &PdfOptions, pdf_a: PdfASettings) -> Result<Vec<u8>> {
    render_pdf_document_pdfa(&parse_markdown(src), opts, pdf_a)
}

/// Convert `Block::FootnoteDefinition` nodes and `Inline::FootnoteRef`
/// references into a trailing "Notes" section with numbered entries, so the
/// PDF render surfaces footnotes without per-surface changes.
///
/// The HTML renderer handles footnotes natively via the notes `<section>`;
/// this transform is PDF-only (called from `render_pdf_document`).
fn transform_footnotes_for_pdf(doc: &Document) -> Document {
    use crate::ast::{Block, Inline};

    // Pass 1: collect definitions (id -> content blocks) and assign numbers
    // by first-reference appearance.
    let mut defs: Vec<(String, Vec<Block>)> = Vec::new();
    let mut numbers: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    fn collect_defs_and_numbers(
        blocks: &[Block],
        defs: &mut Vec<(String, Vec<Block>)>,
        numbers: &mut std::collections::BTreeMap<String, usize>,
    ) {
        for block in blocks {
            match block {
                Block::FootnoteDefinition { id, blocks: inner } => {
                    if !numbers.contains_key(id.as_str()) {
                        let n = numbers.len() + 1;
                        numbers.insert(id.clone(), n);
                    }
                    defs.push((id.clone(), inner.clone()));
                }
                Block::BlockQuote(inner) => collect_defs_and_numbers(inner, defs, numbers),
                Block::List(list) => {
                    for item in &list.items {
                        collect_defs_and_numbers(&item.blocks, defs, numbers);
                    }
                }
                _ => {}
            }
        }
    }
    collect_defs_and_numbers(&doc.blocks, &mut defs, &mut numbers);

    // Pass 2: rewrite the block tree — strip FootnoteDefinitions, rewrite
    // FootnoteRef to numbered text, append the notes section.
    fn rewrite_blocks(
        blocks: &[Block],
        numbers: &std::collections::BTreeMap<String, usize>,
    ) -> Vec<Block> {
        let mut out = Vec::new();
        for block in blocks {
            match block {
                Block::FootnoteDefinition { .. } => {} // moved to the notes section
                Block::Paragraph(inlines) => {
                    out.push(Block::Paragraph(rewrite_inlines(inlines, numbers)));
                }
                Block::Heading { level, inlines } => {
                    out.push(Block::Heading {
                        level: *level,
                        inlines: rewrite_inlines(inlines, numbers),
                    });
                }
                Block::BlockQuote(inner) => {
                    out.push(Block::BlockQuote(rewrite_blocks(inner, numbers)));
                }
                Block::List(list) => {
                    let mut new_list = list.clone();
                    for item in &mut new_list.items {
                        item.blocks = rewrite_blocks(&item.blocks, numbers);
                    }
                    out.push(Block::List(new_list));
                }
                Block::DefinitionList(items) => {
                    let mut new_items = Vec::with_capacity(items.len());
                    for item in items {
                        new_items.push(crate::ast::DefinitionItem {
                            terms: item
                                .terms
                                .iter()
                                .map(|t| rewrite_inlines(t, numbers))
                                .collect(),
                            definitions: item
                                .definitions
                                .iter()
                                .map(|d| rewrite_inlines(d, numbers))
                                .collect(),
                        });
                    }
                    out.push(Block::DefinitionList(new_items));
                }
                other => out.push(other.clone()),
            }
        }
        out
    }

    fn rewrite_inlines(
        inlines: &[Inline],
        numbers: &std::collections::BTreeMap<String, usize>,
    ) -> Vec<Inline> {
        inlines
            .iter()
            .map(|inl| match inl {
                Inline::FootnoteRef { id } => {
                    let n = numbers.get(id.as_str()).copied().unwrap_or(0);
                    Inline::Text(format!("[{n}]"))
                }
                Inline::Emphasis(c) => Inline::Emphasis(rewrite_inlines(c, numbers)),
                Inline::Strong(c) => Inline::Strong(rewrite_inlines(c, numbers)),
                Inline::Strikethrough(c) => Inline::Strikethrough(rewrite_inlines(c, numbers)),
                Inline::Link {
                    dest,
                    title,
                    content,
                } => Inline::Link {
                    dest: dest.clone(),
                    title: title.clone(),
                    content: rewrite_inlines(content, numbers),
                },
                other => other.clone(),
            })
            .collect()
    }

    let mut blocks = rewrite_blocks(&doc.blocks, &numbers);

    // Synthesize the notes section.
    if !defs.is_empty() {
        blocks.push(Block::Heading {
            level: 2,
            inlines: vec![Inline::Text("Notes".to_string())],
        });
        for (def_id, def_blocks) in &defs {
            let n = numbers.get(def_id.as_str()).copied().unwrap_or(0);
            for block in def_blocks {
                if let Block::Paragraph(inlines) = block {
                    let mut numbered = vec![Inline::Text(format!("[{n}] "))];
                    numbered.extend(inlines.iter().cloned());
                    blocks.push(Block::Paragraph(numbered));
                }
            }
        }
    }

    Document { blocks }
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
