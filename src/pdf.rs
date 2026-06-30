//! PDF renderer with embedded subset fonts.
//!
//! Produces a deterministic, **tiny** PDF that embeds document-specific *subsets*
//! of the bundled IBM Plex / Computer Modern faces (see [`crate::fonts`] +
//! [`crate::text::Font::subset`]). Text is laid out with the faces' real `hmtx`
//! metrics, then written as a composite `Type0` font with `Identity-H` encoding
//! (2-byte glyph ids) and a `CIDFontType2` descendant carrying the subset
//! `FontFile2`. Each face also gets a `ToUnicode` CMap so the text stays
//! selectable / copy-pasteable.
//!
//! Knuth-Plass optimal line breaking (via [`crate::layout::break_paragraph`],
//! with the original greedy wrapper kept as an emergency fallback) + automatic
//! pagination over headings, paragraphs, code blocks, lists, blockquotes, tables
//! (simple), and rules, with styled inline runs (bold / italic / code /
//! bold-italic in their own embedded faces). Inline links are colored +
//! underlined, blockquotes get a subtle gutter bar, and fenced code blocks /
//! inline code get rounded light-gray backgrounds. Focused GPOS kerning, GSUB
//! ligatures, and FlateDecode stream compression are applied today.
//!
//! Pure computation (no `std::fs`, no deps) so it stays WASM / `--no-default-features`
//! clean; the font bytes come from `include_bytes!`, not the filesystem.

use crate::ast::{Align, Block, Document, Inline, List, Table};
use crate::error::Result;
use crate::fonts::{self, FontStyle};
use crate::highlight::{self, Tok as HighlightTok};
use crate::layout::{
    FORCED_BREAK_PENALTY, FontSize, Glue, HyphenationOptions, Hyphenator, LayoutUnit,
    ParagraphItem, Penalty, StyledText, TextBox, break_paragraph, default_interword_glue,
    is_breakable_whitespace, measure_text_with_pairs,
};
use crate::text::{Font, Kerning, Ligatures};
use crate::theme::{Theme, ThemeColors};
use crate::{FontAssetSlot, FontAssets, PdfOptions, RenderError};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

#[cfg(not(target_arch = "wasm32"))]
type PdfStageStart = std::time::Instant;
#[cfg(target_arch = "wasm32")]
type PdfStageStart = ();

const MIN_PAGE_DIM: f32 = 80.0;
const MIN_CONTENT_DIM: f32 = 40.0;
const PAGE_STREAM_COMPRESSION_MIN: usize = 4096;

// Visual colors (links, code backgrounds, blockquote tint/bar, table stripe,
// rules, body text) are resolved from the shared theme tokens at render time;
// see `Palette`. This keeps PDF and HTML visually coherent (one theme model)
// rather than hardcoding divergent values here.

// Fenced-code panel + inline-code chip backgrounds.
const CODE_PAD_X: f32 = 8.0; // text inset inside a fenced-code line
const CODE_HANGING_INDENT: f32 = 12.0; // continuation inset for wrapped code rows
const CODE_LINE_NUMBER_GAP: f32 = 6.0;
const PANEL_PAD_V: f32 = 5.0; // vertical breathing room above/below the code
const PANEL_RADIUS: f32 = 4.0;
const PANEL_ASCENT_FRAC: f32 = 0.78; // glyph top above baseline (mono)
const PANEL_DESCENT_FRAC: f32 = 0.30; // glyph bottom below baseline
const CHIP_PAD_X: f32 = 2.0;
const CHIP_RADIUS: f32 = 2.5;
const QUOTE_BG_PAD_X: f32 = 6.0;
const QUOTE_BG_PAD_V: f32 = 3.0;

const PDF_IMAGE_DPI_SCALE: f32 = 72.0 / 96.0;
const MAX_PDF_IMAGE_COMPRESSED_BYTES: usize = 32 * 1024 * 1024;
const MAX_PDF_IMAGE_PIXELS: u64 = 24_000_000;

// Font slots referenced in page Resources as /F1../F5.
const F_BODY: u8 = 1;
const F_BOLD: u8 = 2;
const F_ITALIC: u8 = 3;
const F_MONO: u8 = 4;
const F_BOLDITALIC: u8 = 5;
const SLOTS: [u8; 5] = [F_BODY, F_BOLD, F_ITALIC, F_MONO, F_BOLDITALIC];

/// A positioned run of single-face text within a laid-out line.
struct Seg {
    x: f32,
    slot: u8,
    text: String,
    /// Active link target when this run is part of a safe inline link.
    link: Option<LinkTarget>,
    /// Fill color for text. Link runs use [`Fill::Link`]; syntax-highlighted
    /// fenced code uses [`Fill::Syntax`]. Normal text stays black.
    fill: Fill,
    /// `~~strikethrough~~` run: draw a thin line through the run's middle.
    strike: bool,
    /// Layout (non-kerned) advance sum, used to size the link underline.
    width: f32,
}

/// Deterministic text fill color class. This enum, rather than raw floats, lets
/// serialization track the current color exactly and avoid redundant `rg`
/// operators while still resetting after colored code/link runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fill {
    Black,
    Link,
    Syntax(HighlightTok),
}

/// Link destinations that are safe to make active in the PDF.
#[derive(Clone, PartialEq, Eq)]
enum LinkTarget {
    Uri(String),
    Fragment(String),
}

/// Tagged-PDF list membership, populated by [`layout_list`] for every line that
/// belongs to a list item. Lets the structure-tree builder group lines into
/// `/L` → `/LI` → `/LBody` containers, including nested lists. The innermost
/// list wins: a deeper `layout_list` stamps its lines first, and the enclosing
/// list only stamps still-unmarked lines, so `depth` is always the line's true
/// nesting level.
#[derive(Clone)]
struct ListMark {
    /// Unique id of the enclosing list (its first line's out-vec index). Nesting
    /// depth is encoded by this mark's position in [`Line::list_path`].
    list: u32,
    /// Unique id of the enclosing list item (the item's flow group).
    item: u32,
}

/// One laid-out line: a baseline-aligned row of styled segments, or a rule.
struct Line {
    size: f32,
    gap_after: f32,
    rule: bool,
    /// Left x of a horizontal rule (only meaningful when `rule`).
    rule_x: f32,
    /// For each blockquote enclosing this line: `(quote_id, bar_x)`. `quote_id`
    /// is the out-vec index of the quote's first line; `bar_x` is the stroke x.
    quote_bars: Vec<(usize, f32)>,
    /// Code-panel group: `0` = no background; equal nonzero ids on consecutive
    /// lines share ONE filled rounded rect (a single fenced code block).
    bg: u32,
    /// Table body-row zebra striping: when true, a subtle full-measure tint is
    /// drawn behind this line (band x0 = [`Line::rule_x`], x1 = page right edge).
    /// Deterministic and per-physical-line, so it survives page breaks.
    shade: bool,
    /// Vertical-list metadata used by the page builder.
    flow: FlowMark,
    /// Tagged-PDF list membership: the full ancestor chain of enclosing lists,
    /// outermost first, so a deeply nested list line carries every `/L`→`/LI`
    /// level above it. Empty for non-list lines. Set by [`layout_list`].
    list_path: Vec<ListMark>,
    /// Tagged-PDF table cell columns: for a table content line, the source
    /// column index of each entry in `segs` (so `table_cols.len() == segs.len()`
    /// and `table_cols[i]` is the grid column of `segs[i]`). Empty for every
    /// non-table line. Lets the structure-tree builder emit per-cell `/TH`/`/TD`
    /// and merge a cell's wrapped fragments across physical lines.
    table_cols: Vec<u32>,
    segs: Vec<Seg>,
    image: Option<ImageLine>,
}

#[derive(Clone)]
struct ImageLine {
    image: PdfImageData,
    alt: String,
    width_pt: f32,
    height_pt: f32,
}

#[derive(Clone)]
struct PdfImageData {
    key: String,
    width_px: u32,
    height_px: u32,
    color: PdfImageColor,
    /// FlateDecode stream of the image samples.
    data: Vec<u8>,
    /// When true, `data` is the raw PNG IDAT and the XObject applies the PNG
    /// adaptive predictor (`/Predictor 15`); the simple, proven zero-decode path
    /// for 8-bit grayscale/RGB PNGs. When false, `data` is our own zlib of
    /// already-unfiltered 8-bit samples (no predictor) — used for formats that
    /// must be decoded (palette, alpha, 16-bit, interlaced).
    png_predictor: bool,
    /// Optional 8-bit grayscale soft mask (FlateDecode), one sample per pixel,
    /// carrying the source image's alpha channel as a PDF `/SMask`.
    smask: Option<Vec<u8>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PdfImageColor {
    Gray,
    Rgb,
}

impl PdfImageColor {
    const fn color_space(self) -> &'static str {
        match self {
            Self::Gray => "/DeviceGray",
            Self::Rgb => "/DeviceRGB",
        }
    }

    const fn components(self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FlowKind {
    Paragraph,
    Heading,
    Code,
    Image,
    TableHeader,
    TableRule,
    TableRow,
    Rule,
    Other,
}

#[derive(Clone, Copy)]
struct FlowMark {
    group: u32,
    index: usize,
    count: usize,
    kind: FlowKind,
    /// True only on the first line of a list (the first item's marker line), so
    /// the page builder can keep a short intro/caption with the list it heads.
    list_start: bool,
}

impl Default for FlowMark {
    fn default() -> Self {
        Self {
            group: 0,
            index: 0,
            count: 1,
            kind: FlowKind::Other,
            list_start: false,
        }
    }
}

/// The source faces resolved from the theme family + the registry.
struct Faces {
    body: Font,
    bold: Font,
    italic: Font,
    bolditalic: Font,
    mono: Font,
}

/// Sanitized page geometry derived from the shared theme model.
///
/// CLI config validates page margins before they reach the renderer, but the
/// public library API can construct arbitrary `Theme` values. Keep the PDF
/// writer total by clamping malformed dimensions/margins into a printable box.
#[derive(Clone, Copy)]
struct PageGeom {
    width: f32,
    height: f32,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    content_w: f32,
}

impl PageGeom {
    fn from_theme(theme: &Theme) -> Self {
        let default_page = Theme::default().page;
        let width =
            positive_finite(theme.page.size.width_pt, default_page.size.width_pt).max(MIN_PAGE_DIM);
        let height = positive_finite(theme.page.size.height_pt, default_page.size.height_pt)
            .max(MIN_PAGE_DIM);

        let mut left = nonnegative_finite(theme.page.margins.left_pt, default_page.margins.left_pt);
        let mut right =
            nonnegative_finite(theme.page.margins.right_pt, default_page.margins.right_pt);
        let mut top = nonnegative_finite(theme.page.margins.top_pt, default_page.margins.top_pt);
        let mut bottom =
            nonnegative_finite(theme.page.margins.bottom_pt, default_page.margins.bottom_pt);

        left = left.min((width - MIN_CONTENT_DIM).max(0.0));
        right = right.min((width - left - MIN_CONTENT_DIM).max(0.0));
        top = top.min((height - MIN_CONTENT_DIM).max(0.0));
        bottom = bottom.min((height - top - MIN_CONTENT_DIM).max(0.0));

        Self {
            width,
            height,
            left,
            right,
            top,
            bottom,
            content_w: (width - left - right).max(MIN_CONTENT_DIM),
        }
    }

    fn top_y(self) -> f32 {
        self.height - self.top
    }

    fn right_x(self) -> f32 {
        self.width - self.right
    }
}

fn positive_finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn nonnegative_finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        fallback
    }
}

impl Faces {
    fn load(opts: &PdfOptions) -> Result<Self> {
        let fam = opts.theme.font;
        Ok(Self {
            body: parse_face(
                FontAssetSlot::BodyRegular,
                body_font_bytes(&opts.font_assets, fam, FontStyle::Regular),
            )?,
            bold: parse_face(
                FontAssetSlot::BodyBold,
                body_font_bytes(&opts.font_assets, fam, FontStyle::Bold),
            )?,
            italic: parse_face(
                FontAssetSlot::BodyItalic,
                body_font_bytes(&opts.font_assets, fam, FontStyle::Italic),
            )?,
            bolditalic: parse_face(
                FontAssetSlot::BodyBoldItalic,
                body_font_bytes(&opts.font_assets, fam, FontStyle::BoldItalic),
            )?,
            mono: parse_face(
                FontAssetSlot::MonoRegular,
                mono_font_bytes(&opts.font_assets, FontStyle::Regular),
            )?,
        })
    }

    fn get(&self, slot: u8) -> &Font {
        match slot {
            F_BOLD => &self.bold,
            F_ITALIC => &self.italic,
            F_BOLDITALIC => &self.bolditalic,
            F_MONO => &self.mono,
            _ => &self.body,
        }
    }

    /// Advance of `c` in 1/1000 em (PDF text space) for the slot's face.
    fn advance(&self, slot: u8, c: char) -> f32 {
        self.get(slot).advance_1000(c) as f32
    }
}

fn body_font_bytes(font_assets: &FontAssets, family: crate::FontFamily, style: FontStyle) -> &[u8] {
    match style {
        FontStyle::Regular => font_assets
            .body_regular
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
        FontStyle::Bold => font_assets
            .body_bold
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
        FontStyle::Italic => font_assets
            .body_italic
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
        FontStyle::BoldItalic => font_assets
            .body_bold_italic
            .as_deref()
            .unwrap_or_else(|| fonts::body_bytes(family, style)),
    }
}

fn mono_font_bytes(font_assets: &FontAssets, style: FontStyle) -> &[u8] {
    font_assets
        .mono_regular
        .as_deref()
        .unwrap_or_else(|| fonts::mono_bytes(style))
}

fn parse_face(slot: FontAssetSlot, bytes: &[u8]) -> Result<Font> {
    let font = Font::parse(bytes.to_vec()).map_err(|err| {
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
    Ok(font)
}

/// Resolve a font slot from inline style flags.
fn slot_of(bold: bool, italic: bool, mono: bool) -> u8 {
    if mono {
        F_MONO
    } else if bold && italic {
        F_BOLDITALIC
    } else if bold {
        F_BOLD
    } else if italic {
        F_ITALIC
    } else {
        F_BODY
    }
}

/// A line-breaking token: a maximal run of non-space chars (a word) or a single
/// inter-word space, each carrying a font slot.
#[derive(Clone)]
struct Tok {
    text: String,
    slot: u8,
    space: bool,
    hard_break: bool,
    /// Active link target when this token came from safe inline link content.
    link: Option<LinkTarget>,
    /// True when this token is inside a `~~strikethrough~~` span.
    strike: bool,
}

#[derive(Clone)]
struct LineTok {
    tok: Tok,
    /// Extra advance applied after this token. Used for deterministic
    /// TeX-style glue stretch/shrink without changing the token's selectable
    /// text bytes.
    extra_advance: f32,
}

struct BuiltParagraph {
    items: Vec<ParagraphItem>,
    item_toks: Vec<Vec<Tok>>,
    break_toks: Vec<Option<Tok>>,
}

#[derive(Clone, Copy)]
struct PdfWordContext<'a> {
    fs: FontSize,
    faces: &'a Faces,
    policy: ParagraphPolicy,
    hyphenator: &'a Hyphenator,
    /// Per-document hyphenation cache (bead qw1.7.1); shared via `&RefCell` so
    /// this `Copy` context can still read/insert.
    hyphen_cache: &'a RefCell<HashMap<String, Vec<usize>>>,
}

#[derive(Clone, Copy)]
struct ParagraphPolicy {
    hyphenate: bool,
    justify: bool,
}

impl ParagraphPolicy {
    const RAGGED: Self = Self {
        hyphenate: false,
        justify: false,
    };
    const TEX_PARAGRAPH: Self = Self {
        hyphenate: true,
        justify: true,
    };

    const fn for_flow(kind: FlowKind) -> Self {
        match kind {
            FlowKind::Paragraph => Self::TEX_PARAGRAPH,
            FlowKind::Heading => Self::RAGGED,
            _ => Self::RAGGED,
        }
    }
}

/// Render a document to PDF bytes.
///
/// # Errors
/// Infallible in practice (the bundled fonts always parse); returns [`Result`]
/// to leave room for future validation without a signature change.
pub fn render(doc: &Document, opts: &PdfOptions) -> Result<Vec<u8>> {
    render_inner(doc, opts, false).map(|profile| profile.bytes)
}

/// Render a document to PDF bytes while collecting stage-level attribution.
///
/// Timing is explicit opt-in so normal renders avoid clock reads and allocation
/// of profiling ledgers. On `wasm32` targets the stage ledger is still emitted,
/// but elapsed times are zero until a browser-facing clock provider exists.
///
/// # Errors
/// See [`render`].
pub fn render_profiled(doc: &Document, opts: &PdfOptions) -> Result<PdfProfile> {
    render_inner(doc, opts, true)
}

/// A non-fatal diagnostic about a PDF render: content that was *degraded* rather
/// than embedded, which the renderer would otherwise drop silently. Pure and
/// WASM-safe — the caller (e.g. the `fmd` CLI) decides how to surface it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderWarning {
    /// An image whose destination had no matching `PdfImageAsset`; rendered as
    /// alt text instead of being embedded.
    UnresolvedImage(String),
    /// A supplied image asset could not be decoded (e.g. an unsupported PNG
    /// variant); rendered as alt text instead of being embedded.
    UnsupportedImage(String),
    /// `count` characters had no glyph in the embedded fonts and were rendered
    /// as `.notdef` boxes (and are not selectable). `sample` shows a few.
    MissingGlyphs { count: usize, sample: String },
}

impl RenderWarning {
    /// Stable machine selector for robot/JSON output.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnresolvedImage(_) => "unresolved_image",
            Self::UnsupportedImage(_) => "unsupported_image",
            Self::MissingGlyphs { .. } => "missing_glyphs",
        }
    }

    /// Human-readable message naming the problem and, where possible, the fix.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::UnresolvedImage(dest) => format!(
                "image '{dest}' has no --pdf-image mapping; rendered as alt text \
                 (add --pdf-image '{dest}=PATH')"
            ),
            Self::UnsupportedImage(dest) => format!(
                "image '{dest}' could not be decoded (unsupported PNG); rendered as alt text"
            ),
            Self::MissingGlyphs { count, sample } => format!(
                "{count} character(s) have no glyph in the embedded fonts and were rendered \
                 as .notdef boxes (e.g. {sample:?})"
            ),
        }
    }
}

/// Compute non-fatal warnings for a PDF render of `doc` with `opts`: images that
/// will not embed (missing or undecodable assets) and characters that have no
/// glyph in the embedded fonts. Pure (no I/O); intended for the CLI to print to
/// stderr so degraded output is never silent.
#[must_use]
pub fn render_warnings(doc: &Document, opts: &PdfOptions) -> Vec<RenderWarning> {
    let mut warnings = Vec::new();

    let mut dests = Vec::new();
    collect_image_dests(&doc.blocks, &mut dests);
    for dest in dests {
        let trimmed = dest.trim();
        if trimmed.is_empty() {
            continue;
        }
        match opts
            .image_assets
            .iter()
            .find(|asset| asset.destination.trim() == trimmed)
        {
            None => warnings.push(RenderWarning::UnresolvedImage(dest)),
            Some(asset) => {
                if parse_png_image_asset(trimmed, &asset.bytes).is_none() {
                    warnings.push(RenderWarning::UnsupportedImage(dest));
                }
            }
        }
    }

    if let Ok(faces) = Faces::load(opts) {
        let mut text = String::new();
        collect_text(&doc.blocks, &mut text);
        let mut missing = 0usize;
        let mut seen = BTreeSet::new();
        let mut sample = String::new();
        for c in text.chars() {
            if c.is_whitespace() || c.is_control() {
                continue;
            }
            let mapped = faces.body.glyph_index(c) != 0
                || faces.bold.glyph_index(c) != 0
                || faces.italic.glyph_index(c) != 0
                || faces.bolditalic.glyph_index(c) != 0
                || faces.mono.glyph_index(c) != 0;
            if !mapped {
                missing += 1;
                if seen.insert(c) && sample.chars().count() < 8 {
                    sample.push(c);
                }
            }
        }
        if missing > 0 {
            warnings.push(RenderWarning::MissingGlyphs {
                count: missing,
                sample,
            });
        }
    }

    warnings
}

fn collect_image_dests(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                collect_image_dests_inlines(inlines, out);
            }
            Block::BlockQuote(inner) => collect_image_dests(inner, out),
            Block::List(list) => {
                for item in &list.items {
                    collect_image_dests(&item.blocks, out);
                }
            }
            Block::Table(table) => {
                for cell in &table.head {
                    collect_image_dests_inlines(cell, out);
                }
                for row in &table.rows {
                    for cell in row {
                        collect_image_dests_inlines(cell, out);
                    }
                }
            }
            Block::CodeBlock { .. } | Block::ThematicBreak | Block::HtmlBlock(_) => {}
        }
    }
}

fn collect_image_dests_inlines(inlines: &[Inline], out: &mut Vec<String>) {
    for inline in inlines {
        match inline {
            Inline::Image { dest, .. } => out.push(dest.clone()),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                collect_image_dests_inlines(c, out);
            }
            Inline::Link { content, .. } => collect_image_dests_inlines(content, out),
            _ => {}
        }
    }
}

fn collect_text(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Heading { inlines, .. } | Block::Paragraph(inlines) => {
                collect_text_inlines(inlines, out);
            }
            Block::CodeBlock { code, .. } => out.push_str(code),
            Block::BlockQuote(inner) => collect_text(inner, out),
            Block::List(list) => {
                for item in &list.items {
                    collect_text(&item.blocks, out);
                }
            }
            Block::Table(table) => {
                for cell in &table.head {
                    collect_text_inlines(cell, out);
                }
                for row in &table.rows {
                    for cell in row {
                        collect_text_inlines(cell, out);
                    }
                }
            }
            Block::ThematicBreak | Block::HtmlBlock(_) => {}
        }
    }
}

fn collect_text_inlines(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                collect_text_inlines(c, out);
            }
            Inline::Link { content, .. } => collect_text_inlines(content, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak | Inline::Html(_) => {}
        }
    }
}

fn render_inner(doc: &Document, opts: &PdfOptions, profiled: bool) -> Result<PdfProfile> {
    let mut profiler = if profiled {
        PdfProfiler::enabled()
    } else {
        PdfProfiler::disabled()
    };
    let page = PageGeom::from_theme(&opts.theme);
    let faces = profiler.measure(
        "font_load",
        5,
        "load body/bold/italic/bolditalic/mono faces from supplied or bundled bytes",
        || Faces::load(opts),
        |result| usize::from(result.is_ok()),
    )?;
    let lines = profiler.measure(
        "layout",
        doc.blocks.len(),
        "block layout, text measuring, and paragraph line breaking",
        || layout(&doc.blocks, opts, &faces, page),
        |_| 0,
    );
    let line_count = lines.len();
    let serialize_started = profiler.checkpoint();
    let bytes = serialize(&lines, opts, &faces, page, &mut profiler)?;
    profiler.record_since(
        "serialize_total",
        line_count,
        bytes.len(),
        "font subsetting, pagination, page streams, and PDF object writing",
        serialize_started,
    );
    Ok(PdfProfile {
        bytes,
        stages: profiler.finish(),
    })
}

/// PDF bytes plus the profiling ledger collected by [`render_profiled`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfProfile {
    /// Rendered PDF bytes. These should match [`render`] for the same inputs.
    pub bytes: Vec<u8>,
    /// Stable stage summaries in the order the renderer observed them.
    pub stages: Vec<PdfStageSummary>,
}

/// One measured PDF renderer stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfStageSummary {
    /// Stable stage identifier used by perf artifacts and Beads closeouts.
    pub stage: &'static str,
    /// Stage-specific work count: blocks, lines, segments, glyphs, pages, or objects.
    pub count: usize,
    /// Elapsed nanoseconds for this invocation. Zero on wasm32 until a browser
    /// clock provider is introduced.
    pub elapsed_ns: u128,
    /// Stage-specific byte count when meaningful, otherwise zero.
    pub bytes: usize,
    /// Short stable explanation for artifact readers.
    pub notes: &'static str,
}

struct PdfProfiler {
    enabled: bool,
    stages: Vec<PdfStageSummary>,
}

impl PdfProfiler {
    fn disabled() -> Self {
        Self {
            enabled: false,
            stages: Vec::new(),
        }
    }

    fn enabled() -> Self {
        Self {
            enabled: true,
            stages: Vec::new(),
        }
    }

    fn checkpoint(&self) -> Option<PdfStageStart> {
        if self.enabled { pdf_stage_now() } else { None }
    }

    fn record_since(
        &mut self,
        stage: &'static str,
        count: usize,
        bytes: usize,
        notes: &'static str,
        started: Option<PdfStageStart>,
    ) {
        if !self.enabled {
            return;
        }
        self.stages.push(PdfStageSummary {
            stage,
            count,
            elapsed_ns: pdf_stage_elapsed_ns(started),
            bytes,
            notes,
        });
    }

    fn measure<T, F, B>(
        &mut self,
        stage: &'static str,
        count: usize,
        notes: &'static str,
        f: F,
        bytes: B,
    ) -> T
    where
        F: FnOnce() -> T,
        B: FnOnce(&T) -> usize,
    {
        let started = self.checkpoint();
        let result = f();
        let byte_count = if self.enabled { bytes(&result) } else { 0 };
        self.record_since(stage, count, byte_count, notes, started);
        result
    }

    fn finish(self) -> Vec<PdfStageSummary> {
        self.stages
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn pdf_stage_now() -> Option<PdfStageStart> {
    Some(std::time::Instant::now())
}

#[cfg(target_arch = "wasm32")]
fn pdf_stage_now() -> Option<PdfStageStart> {
    Some(())
}

#[cfg(not(target_arch = "wasm32"))]
fn pdf_stage_elapsed_ns(started: Option<PdfStageStart>) -> u128 {
    started.map_or(0, |start| start.elapsed().as_nanos())
}

#[cfg(target_arch = "wasm32")]
fn pdf_stage_elapsed_ns(_started: Option<PdfStageStart>) -> u128 {
    0
}

// ---- layout -----------------------------------------------------------------

fn layout(blocks: &[Block], opts: &PdfOptions, faces: &Faces, page: PageGeom) -> Vec<Line> {
    let mut out = Vec::new();
    let mut cx = LayoutCx {
        opts,
        faces,
        page,
        next_bg: 0,
        next_flow: 0,
        list_stack: Vec::new(),
        hyphen_cache: RefCell::new(HashMap::new()),
    };
    layout_blocks(blocks, 0.0, &mut out, &mut cx);
    out
}

/// Bound on the per-document hyphenation cache (distinct lowercase words). Beyond
/// this, further words are still hyphenated but not cached — a fixed cap keeps
/// memory bounded and the cached set deterministic (words are seen in document
/// order), while never changing the hyphenation result.
const HYPHEN_CACHE_MAX: usize = 16_384;

/// One open list level during layout. The stack in [`LayoutCx`] lets
/// [`layout_list`] stamp every line with its full `/L`→`/LI` ancestor chain so
/// nested lists tag correctly in the structure tree.
struct ListFrame {
    /// Stable list id (the list's first out-vec index).
    list: u32,
    /// Current item's flow group (set per item as the loop advances).
    item: u32,
}

struct LayoutCx<'a> {
    opts: &'a PdfOptions,
    faces: &'a Faces,
    page: PageGeom,
    list_stack: Vec<ListFrame>,
    next_bg: u32,
    next_flow: u32,
    /// Per-document (per-render) word → hyphenation-points cache (bead qw1.7.1).
    /// Keyed by the lowercase ASCII word; `RefCell` so it can be shared through
    /// the `Copy` [`PdfWordContext`]. Lives for the whole `layout()` call and is
    /// dropped with it (render-call-local), and never changes the result.
    hyphen_cache: RefCell<HashMap<String, Vec<usize>>>,
}

impl LayoutCx<'_> {
    fn alloc_bg(&mut self) -> u32 {
        self.next_bg = self.next_bg.saturating_add(1);
        self.next_bg
    }

    fn alloc_flow(&mut self) -> u32 {
        self.next_flow = self.next_flow.saturating_add(1);
        self.next_flow
    }
}

#[derive(Clone, Copy)]
struct FlowSpec {
    group: u32,
    kind: FlowKind,
}

fn layout_blocks(blocks: &[Block], indent: f32, out: &mut Vec<Line>, cx: &mut LayoutCx<'_>) {
    for block in blocks {
        layout_block(block, indent, out, cx);
    }
}

fn layout_block(block: &Block, indent: f32, out: &mut Vec<Line>, cx: &mut LayoutCx<'_>) {
    match block {
        Block::Heading { level, inlines } => {
            let size = match level {
                1 => 24.0,
                2 => 19.0,
                3 => 16.0,
                4 => 13.5,
                5 => 12.0,
                _ => 11.0,
            };
            gap(out, heading_gap_before(*level));
            // Headings render bold; inner emphasis becomes bold-italic.
            let mut toks = Vec::new();
            tokenize(inlines, true, false, false, None, &mut toks);
            let group = cx.alloc_flow();
            // H1/H2 get a subtle full-measure hairline rule beneath the text.
            let ruled = matches!(level, 1 | 2);
            let text_gap = if ruled {
                2.5
            } else {
                heading_gap_after(*level)
            };
            let before = out.len();
            layout_inlines(
                toks,
                indent,
                size,
                text_gap,
                out,
                cx,
                FlowSpec {
                    group,
                    kind: FlowKind::Heading,
                },
            );
            if ruled && out.len() > before {
                push_heading_rule(out, indent, cx.page, group, heading_gap_after(*level));
            }
        }
        Block::Paragraph(inlines) => {
            if layout_standalone_image(inlines, indent, out, cx) {
                return;
            }
            let mut toks = Vec::new();
            tokenize(inlines, false, false, false, None, &mut toks);
            let group = cx.alloc_flow();
            layout_inlines(
                toks,
                indent,
                11.0,
                7.0,
                out,
                cx,
                FlowSpec {
                    group,
                    kind: FlowKind::Paragraph,
                },
            );
        }
        Block::CodeBlock { lang, code } => {
            let start = out.len();
            let group = cx.alloc_flow();
            let gid = cx.alloc_bg();
            let mut any = false;
            let line_count = code.lines().count().max(1);
            let digits = line_count.to_string().len().max(1);
            let number_col = if cx.opts.code_line_numbers {
                code_line_number_column_width(digits, 9.5, cx.faces)
            } else {
                0.0
            };
            for (idx, raw) in code.lines().enumerate() {
                any = true;
                let x = cx.page.left + indent + CODE_PAD_X;
                let rows = wrapped_code_rows(
                    raw,
                    CodeWrapSpec {
                        lang: lang.as_deref(),
                        line_no: idx + 1,
                        digits,
                        line_numbers: cx.opts.code_line_numbers,
                        x0: x,
                        max_text_width: (cx.page.content_w - indent - CODE_PAD_X - number_col)
                            .max(12.0),
                        number_col,
                        size: 9.5,
                        faces: cx.faces,
                    },
                );
                let row_count = rows.len();
                for (row_idx, segs) in rows.into_iter().enumerate() {
                    out.push(Line {
                        size: 9.5,
                        gap_after: if row_idx + 1 == row_count { 1.5 } else { 0.5 },
                        rule: false,
                        rule_x: 0.0,
                        quote_bars: Vec::new(),
                        bg: gid,
                        shade: false,
                        flow: FlowMark::default(),
                        list_path: Vec::new(),
                        table_cols: Vec::new(),
                        segs,
                        image: None,
                    });
                }
            }
            if !any {
                // An empty fence still gets a one-line-tall panel.
                let x = cx.page.left + indent + CODE_PAD_X;
                let mut segs = Vec::new();
                if cx.opts.code_line_numbers {
                    segs.push(code_line_number_seg(
                        1, digits, x, number_col, 9.5, cx.faces,
                    ));
                }
                segs.push(Seg {
                    x: x + number_col,
                    slot: F_MONO,
                    text: String::new(),
                    link: None,
                    fill: Fill::Black,
                    strike: false,
                    width: 0.0,
                });
                out.push(Line {
                    size: 9.5,
                    gap_after: 1.5,
                    rule: false,
                    rule_x: 0.0,
                    quote_bars: Vec::new(),
                    bg: gid,
                    shade: false,
                    flow: FlowMark::default(),
                    list_path: Vec::new(),
                    table_cols: Vec::new(),
                    segs,
                    image: None,
                });
            }
            mark_flow(out, start, group, FlowKind::Code);
            gap(out, 6.0);
        }
        Block::BlockQuote(inner) => {
            let start = out.len();
            gap(out, 4.0);
            layout_blocks(inner, indent + 18.0, out, cx);
            let bar_x = cx.page.left + indent + 6.0; // sits in the reserved 18pt gutter
            if let Some(lines) = out.get_mut(start..) {
                for line in lines {
                    line.quote_bars.push((start, bar_x)); // `start` = unique quote id
                }
            }
            gap(out, 3.0);
        }
        Block::List(list) => layout_list(list, indent, out, cx),
        Block::Table(table) => {
            let group = cx.alloc_flow();
            layout_table(table, indent, cx.faces, cx.page, group, out);
        }
        Block::ThematicBreak => {
            let group = cx.alloc_flow();
            out.push(Line {
                size: 6.0,
                gap_after: 8.0,
                rule: true,
                rule_x: cx.page.left + indent,
                quote_bars: Vec::new(),
                bg: 0,
                shade: false,
                flow: FlowMark {
                    group,
                    index: 0,
                    count: 1,
                    kind: FlowKind::Rule,
                    list_start: false,
                },
                list_path: Vec::new(),
                table_cols: Vec::new(),
                segs: Vec::new(),
                image: None,
            });
        }
        Block::HtmlBlock(html) => {
            // PDF has no raw-HTML passthrough mode. Preserve the Markdown source
            // text instead of deleting it; HTML output remains responsible for
            // actual tag passthrough when callers opt into that behavior.
            let mut toks = Vec::new();
            push_text_tokens(html, F_BODY, false, None, &mut toks);
            let group = cx.alloc_flow();
            layout_inlines(
                toks,
                indent,
                11.0,
                7.0,
                out,
                cx,
                FlowSpec {
                    group,
                    kind: FlowKind::Paragraph,
                },
            );
        }
    }
}

fn heading_gap_before(level: u8) -> f32 {
    match level {
        1 => 11.0,
        2 => 9.0,
        3 => 7.0,
        _ => 5.0,
    }
}

fn heading_gap_after(level: u8) -> f32 {
    match level {
        1 => 7.0,
        2 => 6.5,
        3 => 5.5,
        _ => 4.5,
    }
}

/// Push a subtle full-measure hairline rule just beneath an H1/H2's text. The
/// rule shares the heading's flow group so pagination keeps it with the heading
/// (and the heading-with-content keep rule), and so it never registers a second
/// outline destination (the text line claims the group first).
fn push_heading_rule(out: &mut Vec<Line>, indent: f32, page: PageGeom, group: u32, gap_after: f32) {
    out.push(Line {
        size: 3.0,
        gap_after,
        rule: true,
        rule_x: page.left + indent,
        quote_bars: Vec::new(),
        bg: 0,
        shade: false,
        flow: FlowMark {
            group,
            index: 0,
            count: 1,
            kind: FlowKind::Heading,
            list_start: false,
        },
        list_path: Vec::new(),
        table_cols: Vec::new(),
        segs: Vec::new(),
        image: None,
    });
}

fn layout_standalone_image(
    inlines: &[Inline],
    indent: f32,
    out: &mut Vec<Line>,
    cx: &mut LayoutCx<'_>,
) -> bool {
    let [Inline::Image { dest, alt, .. }] = inlines else {
        return false;
    };
    let Some(image) = resolve_pdf_image(&cx.opts.image_assets, dest) else {
        return false;
    };

    let max_w = (cx.page.content_w - indent).max(MIN_CONTENT_DIM);
    let max_h = (cx.page.top_y() - cx.page.bottom).max(MIN_CONTENT_DIM);
    let natural_w = image.width_px as f32 * PDF_IMAGE_DPI_SCALE;
    let natural_h = image.height_px as f32 * PDF_IMAGE_DPI_SCALE;
    if natural_w <= 0.0 || natural_h <= 0.0 {
        return false;
    }
    let scale = (max_w / natural_w).min(max_h / natural_h).min(1.0);
    let width_pt = natural_w * scale;
    let height_pt = natural_h * scale;
    if width_pt <= 0.0 || height_pt <= 0.0 {
        return false;
    }

    let group = cx.alloc_flow();
    out.push(Line {
        size: (height_pt / 1.32).max(1.0),
        gap_after: 7.0,
        rule: false,
        rule_x: cx.page.left + indent,
        quote_bars: Vec::new(),
        bg: 0,
        shade: false,
        flow: FlowMark {
            group,
            index: 0,
            count: 1,
            kind: FlowKind::Image,
            list_start: false,
        },
        list_path: Vec::new(),
        table_cols: Vec::new(),
        segs: Vec::new(),
        image: Some(ImageLine {
            image,
            alt: alt.clone(),
            width_pt,
            height_pt,
        }),
    });
    true
}

fn resolve_pdf_image(assets: &[crate::PdfImageAsset], dest: &str) -> Option<PdfImageData> {
    let key = dest.trim();
    if key.is_empty() {
        return None;
    }
    let asset = assets
        .iter()
        .find(|asset| asset.destination.trim() == key)?;
    parse_png_image_asset(key, &asset.bytes)
}

/// Raw chunks of a PNG, gathered before the format is interpreted.
struct PngChunks {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    interlace: u8,
    /// PLTE palette entries (`color_type == 3`).
    palette: Vec<[u8; 3]>,
    /// tRNS transparency chunk, interpreted per `color_type`.
    trns: Vec<u8>,
    /// Concatenated zlib-compressed image data.
    idat: Vec<u8>,
}

/// Fully decoded PNG: 8-bit samples plus an optional 8-bit alpha plane.
struct DecodedPng {
    width: u32,
    height: u32,
    color: PdfImageColor,
    /// Row-major 8-bit samples: 1 byte/pixel for `Gray`, 3 for `Rgb`.
    samples: Vec<u8>,
    /// Row-major 8-bit alpha, 1 byte/pixel, when the source had transparency.
    alpha: Option<Vec<u8>>,
}

fn parse_png_image_asset(key: &str, bytes: &[u8]) -> Option<PdfImageData> {
    let png = parse_png_chunks(bytes)?;

    // Fast path: 8-bit grayscale/RGB, non-interlaced, no transparency. Embed the
    // raw zlib IDAT directly and let the PDF reader run the PNG predictor — no
    // decode/re-encode needed, and byte-for-byte what the file already holds.
    if png.bit_depth == 8
        && png.interlace == 0
        && png.trns.is_empty()
        && matches!(png.color_type, 0 | 2)
    {
        let color = if png.color_type == 0 {
            PdfImageColor::Gray
        } else {
            PdfImageColor::Rgb
        };
        return Some(PdfImageData {
            key: key.to_string(),
            width_px: png.width,
            height_px: png.height,
            color,
            data: png.idat,
            png_predictor: true,
            smask: None,
        });
    }

    // Everything else (palette, alpha, 16-bit, interlaced, transparency) is fully
    // decoded to 8-bit samples; alpha becomes a real PDF soft mask, so RGBA
    // screenshots/logos render correctly instead of being silently dropped.
    let decoded = decode_png_full(&png)?;
    let data = crate::compress::zlib_compress(&decoded.samples);
    let smask = decoded.alpha.as_deref().map(crate::compress::zlib_compress);
    Some(PdfImageData {
        key: key.to_string(),
        width_px: decoded.width,
        height_px: decoded.height,
        color: decoded.color,
        data,
        png_predictor: false,
        smask,
    })
}

/// Walk a PNG's chunk stream, collecting IHDR fields, PLTE, tRNS, and IDAT. The
/// per-format restrictions are applied later by the fast path / full decoder, so
/// this stays a faithful structural read with only size/sanity guards.
fn parse_png_chunks(bytes: &[u8]) -> Option<PngChunks> {
    const PNG_SIG: &[u8; 8] = b"\x89PNG\r\n\x1A\n";
    if bytes.len() > MAX_PDF_IMAGE_COMPRESSED_BYTES || bytes.get(..8)? != PNG_SIG {
        return None;
    }

    let mut pos = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut interlace = 0u8;
    let mut palette = Vec::new();
    let mut trns = Vec::new();
    let mut idat = Vec::new();
    let mut seen_ihdr = false;
    let mut seen_iend = false;

    while pos < bytes.len() {
        let len = be_u32(bytes, pos)? as usize;
        let kind_start = pos.checked_add(4)?;
        let data_start = kind_start.checked_add(4)?;
        let data_end = data_start.checked_add(len)?;
        let next = data_end.checked_add(4)?;
        if next > bytes.len() {
            return None;
        }
        let kind = bytes.get(kind_start..data_start)?;
        let data = bytes.get(data_start..data_end)?;
        if !seen_ihdr && kind != b"IHDR" {
            return None;
        }
        match kind {
            b"IHDR" => {
                if seen_ihdr || len != 13 {
                    return None;
                }
                width = be_u32(data, 0)?;
                height = be_u32(data, 4)?;
                bit_depth = *data.get(8)?;
                color_type = *data.get(9)?;
                let compression = *data.get(10)?;
                let filter = *data.get(11)?;
                interlace = *data.get(12)?;
                if width == 0
                    || height == 0
                    || u64::from(width).saturating_mul(u64::from(height)) > MAX_PDF_IMAGE_PIXELS
                    || compression != 0
                    || filter != 0
                    || interlace > 1
                    || !matches!(bit_depth, 1 | 2 | 4 | 8 | 16)
                    || !matches!(color_type, 0 | 2 | 3 | 4 | 6)
                {
                    return None;
                }
                seen_ihdr = true;
            }
            b"PLTE" => {
                if len % 3 != 0 {
                    return None;
                }
                palette = data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
            }
            b"tRNS" => {
                trns = data.to_vec();
            }
            b"IDAT" => {
                if !seen_ihdr {
                    return None;
                }
                if idat.len().saturating_add(data.len()) > MAX_PDF_IMAGE_COMPRESSED_BYTES {
                    return None;
                }
                idat.extend_from_slice(data);
            }
            b"IEND" => {
                // IEND must be empty and the final chunk (no trailing bytes).
                if len != 0 || next != bytes.len() {
                    return None;
                }
                seen_iend = true;
                break;
            }
            _ => {}
        }
        pos = next;
    }

    if !seen_ihdr || !seen_iend || idat.is_empty() {
        return None;
    }
    Some(PngChunks {
        width,
        height,
        bit_depth,
        color_type,
        interlace,
        palette,
        trns,
        idat,
    })
}

const PNG_ADAM7: [(u32, u32, u32, u32); 7] = [
    // (x_start, x_step, y_start, y_step)
    (0, 8, 0, 8),
    (4, 8, 0, 8),
    (0, 4, 4, 8),
    (2, 4, 0, 4),
    (0, 2, 2, 4),
    (1, 2, 0, 2),
    (0, 1, 1, 2),
];

/// Number of pixels a given Adam7 axis contributes for a `total`-length axis,
/// starting at `start` with stride `step`.
fn png_pass_count(total: u32, start: u32, step: u32) -> u32 {
    if total <= start {
        0
    } else {
        (total - start).div_ceil(step)
    }
}

fn png_paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (i32::from(a), i32::from(b), i32::from(c));
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Decode any supported PNG into 8-bit samples plus an optional alpha plane.
fn decode_png_full(png: &PngChunks) -> Option<DecodedPng> {
    let width = png.width;
    let height = png.height;
    let bit_depth = png.bit_depth as usize;
    let channels: usize = match png.color_type {
        0 => 1, // grayscale
        2 => 3, // RGB
        3 => 1, // palette index
        4 => 2, // grayscale + alpha
        6 => 4, // RGBA
        _ => return None,
    };
    if png.color_type == 3 && (png.palette.is_empty() || bit_depth > 8) {
        return None;
    }
    // Output is grayscale only for true grayscale source (types 0 and 4).
    let out_gray = matches!(png.color_type, 0 | 4);
    let has_alpha = matches!(png.color_type, 4 | 6) || !png.trns.is_empty();

    let npx = (width as usize).checked_mul(height as usize)?;
    // RGBA8 intermediate (4 bytes/pixel), bounded by the IHDR pixel guard.
    let mut rgba = vec![0u8; npx.checked_mul(4)?];

    let bits_per_pixel = channels.checked_mul(bit_depth)?;
    // Generous bound on the unfiltered raw size across all interlace passes.
    let max_raw = npx
        .checked_mul(channels)?
        .checked_mul(if bit_depth == 16 { 2 } else { 1 })?
        .checked_add(height as usize)?
        .checked_mul(2)?
        .checked_add(64)?;
    let raw = crate::compress::zlib_decompress(&png.idat, max_raw)?;

    let ctx = PngSampleCtx {
        bit_depth,
        color_type: png.color_type,
        channels,
        palette: &png.palette,
        trns: &png.trns,
        width,
        height,
    };

    if png.interlace == 0 {
        ctx.place_pass(&raw, width, height, 0, 1, 0, 1, &mut rgba)?;
    } else {
        let mut offset = 0usize;
        for &(xs, xstep, ys, ystep) in &PNG_ADAM7 {
            let pw = png_pass_count(width, xs, xstep);
            let ph = png_pass_count(height, ys, ystep);
            if pw == 0 || ph == 0 {
                continue;
            }
            let row_bytes = (pw as usize)
                .checked_mul(bits_per_pixel)?
                .div_ceil(8)
                .checked_add(1)?; // + 1 filter byte
            let pass_len = row_bytes.checked_mul(ph as usize)?;
            let pass = raw.get(offset..offset.checked_add(pass_len)?)?;
            offset = offset.checked_add(pass_len)?;
            ctx.place_pass(pass, pw, ph, xs, xstep, ys, ystep, &mut rgba)?;
        }
    }

    // Collapse the RGBA8 intermediate to the output sample/alpha planes.
    let mut samples = Vec::with_capacity(npx * if out_gray { 1 } else { 3 });
    let mut alpha = if has_alpha {
        Some(Vec::with_capacity(npx))
    } else {
        None
    };
    for px in rgba.chunks_exact(4) {
        if out_gray {
            samples.push(px[0]);
        } else {
            samples.extend_from_slice(&px[..3]);
        }
        if let Some(a) = alpha.as_mut() {
            a.push(px[3]);
        }
    }

    Some(DecodedPng {
        width,
        height,
        color: if out_gray {
            PdfImageColor::Gray
        } else {
            PdfImageColor::Rgb
        },
        samples,
        alpha,
    })
}

/// Per-image context for unfiltering one (sub-)image and scattering its pixels
/// into the shared RGBA8 buffer.
struct PngSampleCtx<'a> {
    bit_depth: usize,
    color_type: u8,
    channels: usize,
    palette: &'a [[u8; 3]],
    trns: &'a [u8],
    width: u32,
    height: u32,
}

impl PngSampleCtx<'_> {
    /// Unfilter the scanlines of one pass, then write each decoded pixel into
    /// `rgba` at its interlaced destination `(ys + r*ystep, xs + c*xstep)`.
    #[allow(clippy::too_many_arguments)]
    fn place_pass(
        &self,
        pass: &[u8],
        pw: u32,
        ph: u32,
        xs: u32,
        xstep: u32,
        ys: u32,
        ystep: u32,
        rgba: &mut [u8],
    ) -> Option<()> {
        let bits_per_pixel = self.channels.checked_mul(self.bit_depth)?;
        let row_bytes = (pw as usize).checked_mul(bits_per_pixel)?.div_ceil(8);
        let bpp = bits_per_pixel.div_ceil(8).max(1);
        let stride = row_bytes.checked_add(1)?;

        let mut prev = vec![0u8; row_bytes];
        let mut line = vec![0u8; row_bytes];
        for r in 0..ph as usize {
            let row = pass.get(r * stride..r * stride + stride)?;
            let filter = row[0];
            let src = &row[1..];
            for i in 0..row_bytes {
                let x = src[i];
                let a = if i >= bpp { line[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                line[i] = match filter {
                    0 => x,
                    1 => x.wrapping_add(a),
                    2 => x.wrapping_add(b),
                    3 => x.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                    4 => x.wrapping_add(png_paeth(a, b, c)),
                    _ => return None,
                };
            }
            let dst_y = ys + (r as u32) * ystep;
            if dst_y >= self.height {
                continue;
            }
            for c in 0..pw as usize {
                let dst_x = xs + (c as u32) * xstep;
                if dst_x >= self.width {
                    continue;
                }
                let (rr, gg, bb, aa) = self.pixel(&line, c)?;
                let idx = ((dst_y as usize) * (self.width as usize) + dst_x as usize) * 4;
                let out = rgba.get_mut(idx..idx + 4)?;
                out[0] = rr;
                out[1] = gg;
                out[2] = bb;
                out[3] = aa;
            }
            std::mem::swap(&mut prev, &mut line);
        }
        Some(())
    }

    /// Decode pixel `c` of an unfiltered scanline `line` to RGBA8.
    fn pixel(&self, line: &[u8], c: usize) -> Option<(u8, u8, u8, u8)> {
        // Read the raw channel samples for this pixel as 8-bit values.
        let mut chan = [0u8; 4];
        match self.bit_depth {
            16 => {
                // Two bytes per sample, big-endian; keep the high byte.
                let base = c.checked_mul(self.channels)?.checked_mul(2)?;
                for (ch, slot) in chan.iter_mut().take(self.channels).enumerate() {
                    *slot = *line.get(base + ch * 2)?;
                }
            }
            8 => {
                let base = c.checked_mul(self.channels)?;
                for (ch, slot) in chan.iter_mut().take(self.channels).enumerate() {
                    *slot = *line.get(base + ch)?;
                }
            }
            depth => {
                // Sub-byte samples (1/2/4 bits); only single-channel sources
                // (grayscale or palette index) use these depths.
                let samples_per_byte = 8 / depth;
                let byte = *line.get(c / samples_per_byte)?;
                let within = c % samples_per_byte;
                let shift = 8 - depth * (within + 1);
                let mask = (1u16 << depth) as u8 - 1;
                chan[0] = (byte >> shift) & mask;
            }
        }

        match self.color_type {
            0 => {
                // Grayscale: scale sub-8-bit values up to the full 0..=255 range.
                let g = scale_to_8(chan[0], self.bit_depth);
                let a = self.gray_trns_alpha(chan[0]);
                Some((g, g, g, a))
            }
            2 => {
                let (r, g, b) = (chan[0], chan[1], chan[2]);
                let a = self.rgb_trns_alpha(r, g, b);
                Some((r, g, b, a))
            }
            3 => {
                let idx = chan[0] as usize;
                let rgb = self.palette.get(idx)?;
                let a = self.trns.get(idx).copied().unwrap_or(255);
                Some((rgb[0], rgb[1], rgb[2], a))
            }
            4 => {
                let g = scale_to_8(chan[0], self.bit_depth);
                let a = scale_to_8(chan[1], self.bit_depth);
                Some((g, g, g, a))
            }
            6 => Some((chan[0], chan[1], chan[2], chan[3])),
            _ => None,
        }
    }

    /// Alpha for a grayscale pixel given a possible single-value tRNS.
    fn gray_trns_alpha(&self, raw_sample: u8) -> u8 {
        // tRNS for grayscale is a single 16-bit value (2 bytes); we compare the
        // low byte against the raw sample (depths <= 8 store the value there).
        if self.trns.len() >= 2 && self.trns[1] == raw_sample {
            0
        } else {
            255
        }
    }

    /// Alpha for an RGB pixel given a possible single-color tRNS.
    fn rgb_trns_alpha(&self, r: u8, g: u8, b: u8) -> u8 {
        if self.trns.len() >= 6 && self.trns[1] == r && self.trns[3] == g && self.trns[5] == b {
            0
        } else {
            255
        }
    }
}

/// Scale a sub-8-bit sample to the full 0..=255 range (PNG bit-depth scaling).
fn scale_to_8(value: u8, bit_depth: usize) -> u8 {
    match bit_depth {
        1 => {
            if value != 0 {
                255
            } else {
                0
            }
        }
        2 => value * 85,
        4 => value * 17,
        _ => value, // 8 or 16 (already a high byte)
    }
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let b = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn mark_flow(out: &mut [Line], start: usize, group: u32, kind: FlowKind) {
    let Some(lines) = out.get_mut(start..) else {
        return;
    };
    let count = lines.len();
    if count == 0 {
        return;
    }
    for (index, line) in lines.iter_mut().enumerate() {
        line.flow = FlowMark {
            group,
            index,
            count,
            kind,
            list_start: false,
        };
    }
}

/// One styled run within a wrapped table-cell line: text drawn in a single font
/// slot, optionally linked / struck-through.
struct CellRun {
    slot: u8,
    text: String,
    link: Option<LinkTarget>,
    strike: bool,
    width: f32,
}

/// One wrapped visual line of a table cell: its styled runs and total width.
struct CellWrapLine {
    runs: Vec<CellRun>,
    width: f32,
}

/// Tokenize a cell's inlines into styled tokens, inheriting a bold base style for
/// header cells (so `**x**`/`*x*`/`` `x` ``/links inside cells keep their faces,
/// strikethrough, and clickable destinations instead of being flattened).
fn cell_tokens(inlines: &[Inline], header: bool) -> Vec<Tok> {
    let mut toks = Vec::new();
    tokenize(inlines, header, false, false, None, &mut toks);
    toks
}

/// Single-line width of a cell (word advances in each run's own slot plus one
/// space between adjacent words). Used to size columns.
fn cell_natural_width(toks: &[Tok], size: f32, faces: &Faces) -> f32 {
    let mut w = 0.0;
    let mut started = false;
    let mut pending_space: Option<u8> = None;
    for t in toks {
        if t.space {
            if started {
                pending_space = Some(t.slot);
            }
        } else {
            if let Some(slot) = pending_space.take() {
                w += text_width(" ", size, slot, faces);
            }
            w += text_width(&t.text, size, t.slot, faces);
            started = true;
        }
    }
    w
}

/// Merge a line's tokens into runs of identical style, measuring each run.
fn build_cell_line(toks: &[Tok], size: f32, faces: &Faces) -> CellWrapLine {
    let mut runs: Vec<CellRun> = Vec::new();
    for t in toks {
        let merge = runs
            .last()
            .is_some_and(|r| r.slot == t.slot && r.link == t.link && r.strike == t.strike);
        if merge {
            if let Some(last) = runs.last_mut() {
                last.text.push_str(&t.text);
            }
        } else {
            runs.push(CellRun {
                slot: t.slot,
                text: t.text.clone(),
                link: t.link.clone(),
                strike: t.strike,
                width: 0.0,
            });
        }
    }
    let mut total = 0.0;
    for run in &mut runs {
        run.width = text_width(&run.text, size, run.slot, faces);
        total += run.width;
    }
    CellWrapLine { runs, width: total }
}

/// Greedily wrap styled cell tokens to `max_width`, preserving per-run styling.
fn wrap_cell_styled(toks: &[Tok], max_width: f32, size: f32, faces: &Faces) -> Vec<CellWrapLine> {
    let mut lines = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut cur_w = 0.0;
    let mut pending: Option<Tok> = None;
    for t in toks {
        if t.hard_break {
            pending = None;
            if !cur.is_empty() {
                lines.push(build_cell_line(&cur, size, faces));
                cur.clear();
                cur_w = 0.0;
            }
            continue;
        }
        if t.space {
            if !cur.is_empty() {
                pending = Some(t.clone());
            }
            continue;
        }
        let ww = text_width(&t.text, size, t.slot, faces);
        let sw = pending
            .as_ref()
            .map_or(0.0, |s| text_width(" ", size, s.slot, faces));
        if !cur.is_empty() && cur_w + sw + ww > max_width {
            lines.push(build_cell_line(&cur, size, faces));
            cur.clear();
            cur_w = 0.0;
            pending = None;
            cur.push(t.clone());
            cur_w += ww;
        } else {
            if let Some(sp) = pending.take() {
                cur.push(sp);
                cur_w += sw;
            }
            cur.push(t.clone());
            cur_w += ww;
        }
    }
    if !cur.is_empty() {
        lines.push(build_cell_line(&cur, size, faces));
    }
    lines
}

/// Lay out a GFM pipe table as a measured-column grid: a bold header row with a
/// rule beneath it and a closing rule (booktabs-style), columns sized to their
/// content and scaled to fill the measure, with per-cell alignment.
fn layout_table(
    table: &Table,
    indent: f32,
    faces: &Faces,
    page: PageGeom,
    group: u32,
    out: &mut Vec<Line>,
) {
    let size = 10.0;
    let ncol = table
        .head
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    if ncol == 0 {
        return;
    }
    let left = page.left + indent;
    let avail = (page.content_w - indent).max(72.0);
    let pad = 14.0; // inter-column gutter (half on each side of a column)

    // Natural (unwrapped) text width per column across the header + every row,
    // measured from styled runs so mixed bold/italic/mono cells size correctly.
    let mut natural = vec![0f32; ncol];
    for (k, cell) in table.head.iter().enumerate() {
        if let Some(w) = natural.get_mut(k) {
            *w = w.max(cell_natural_width(&cell_tokens(cell, true), size, faces));
        }
    }
    for row in &table.rows {
        for (k, cell) in row.iter().enumerate() {
            if let Some(w) = natural.get_mut(k) {
                *w = w.max(cell_natural_width(&cell_tokens(cell, false), size, faces));
            }
        }
    }

    // Scale columns so (text widths + gutters) fill the available measure.
    let natural_sum: f32 = natural.iter().sum();
    let gutters = pad * ncol as f32;
    let target = (avail - gutters).max(ncol as f32 * 12.0);
    let scale = if natural_sum > 0.0 {
        target / natural_sum
    } else {
        1.0
    };
    let colw: Vec<f32> = natural.iter().map(|&w| (w * scale).max(12.0)).collect();

    // Text-left x for each column (inset by half a gutter).
    let mut tx = Vec::with_capacity(ncol);
    let mut cx = left;
    for &w in &colw {
        tx.push(cx + pad / 2.0);
        cx += w + pad;
    }

    let row_lines =
        |cells: &[Vec<Inline>], header: bool, gap_after: f32, kind: FlowKind, shade: bool| {
            // Wrap each cell's STYLED tokens to its column width so bold/italic/
            // code faces, strikethrough, and clickable links survive (they used
            // to be flattened to one plain slot per cell).
            let wrapped: Vec<Vec<CellWrapLine>> = (0..ncol)
                .map(|k| {
                    let cw = colw.get(k).copied().unwrap_or(12.0);
                    let toks = cells
                        .get(k)
                        .map(|c| cell_tokens(c, header))
                        .unwrap_or_default();
                    wrap_cell_styled(&toks, cw, size, faces)
                })
                .collect();
            let depth = wrapped.iter().map(Vec::len).max().unwrap_or(0).max(1);
            let mut lines = Vec::with_capacity(depth);

            for row_idx in 0..depth {
                let mut segs = Vec::new();
                // Source grid column of each emitted seg, kept parallel to `segs`
                // so the structure tree can tag per-cell `/TH`/`/TD`. Multiple
                // styled runs of one cell share that cell's column key, so they
                // collapse into a single `/TH`/`/TD` with several marked-content
                // runs. An empty cell emits no seg (and no column), so it is
                // simply omitted from its row's structure — a known limitation
                // (no empty-`/TD` backfill); see docs/PDF_ACCESSIBILITY.md.
                let mut cols = Vec::new();
                for k in 0..ncol {
                    let Some(cell_line) = wrapped.get(k).and_then(|parts| parts.get(row_idx))
                    else {
                        continue;
                    };
                    if cell_line.runs.is_empty() {
                        continue;
                    }
                    let cw = colw.get(k).copied().unwrap_or(0.0);
                    let base = tx.get(k).copied().unwrap_or(left);
                    let offset = match table.align.get(k) {
                        Some(Align::Right) => (cw - cell_line.width).max(0.0),
                        Some(Align::Center) => ((cw - cell_line.width) / 2.0).max(0.0),
                        _ => 0.0,
                    };
                    let mut x = base + offset;
                    for run in &cell_line.runs {
                        let fill = if run.link.is_some() {
                            Fill::Link
                        } else {
                            Fill::Black
                        };
                        segs.push(Seg {
                            x,
                            slot: run.slot,
                            text: run.text.clone(),
                            link: run.link.clone(),
                            fill,
                            strike: run.strike,
                            width: run.width,
                        });
                        cols.push(k as u32);
                        x += run.width;
                    }
                }
                lines.push(Line {
                    size,
                    gap_after: if row_idx + 1 == depth { gap_after } else { 1.0 },
                    rule: false,
                    // Reuse `rule_x` (unused on non-rule lines) to carry the stripe's
                    // left edge so PASS 2 can draw a full-measure tint to the right.
                    rule_x: if shade { left } else { 0.0 },
                    quote_bars: Vec::new(),
                    bg: 0,
                    shade,
                    flow: FlowMark {
                        group,
                        index: row_idx,
                        count: depth,
                        kind,
                        list_start: false,
                    },
                    list_path: Vec::new(),
                    table_cols: cols,
                    segs,
                    image: None,
                });
            }

            lines
        };

    let rule = |gap_after: f32| Line {
        size: 4.0,
        gap_after,
        rule: true,
        rule_x: left,
        quote_bars: Vec::new(),
        bg: 0,
        shade: false,
        flow: FlowMark {
            group,
            index: 0,
            count: 1,
            kind: FlowKind::TableRule,
            list_start: false,
        },
        list_path: Vec::new(),
        table_cols: Vec::new(),
        segs: Vec::new(),
        image: None,
    };

    out.extend(row_lines(
        &table.head,
        true,
        3.0,
        FlowKind::TableHeader,
        false,
    ));
    out.push(rule(3.0));
    let nrows = table.rows.len();
    for (i, row) in table.rows.iter().enumerate() {
        // Zebra striping: tint every other body row (0-based even rows) for a
        // modern look. Deterministic from the logical row index.
        out.extend(row_lines(
            row,
            false,
            if i + 1 == nrows { 3.0 } else { 2.5 },
            FlowKind::TableRow,
            i % 2 == 0,
        ));
    }
    out.push(rule(0.0));
    gap(out, 8.0);
}

fn layout_list(list: &List, indent: f32, out: &mut Vec<Line>, cx: &mut LayoutCx<'_>) {
    let markers: Vec<String> = list
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| match item.task {
            Some(true) => "[x]".to_string(),
            Some(false) => "[ ]".to_string(),
            None if list.ordered => format!("{}.", list.start + i as u64),
            None => "•".to_string(),
        })
        .collect();
    let marker_col = markers
        .iter()
        .map(|marker| text_width(marker, 11.0, F_BODY, cx.faces))
        .fold(0.0f32, f32::max)
        .max(8.0);
    let marker_left = cx.page.left + indent + 2.0;
    let content_indent = indent + marker_col + 11.0;
    let list_first_line = out.len();

    // Push this list onto the layout's list stack so every line laid out while
    // it is open can be stamped with its full `/L`→`/LI` ancestor chain. The
    // item id is updated as the loop advances.
    cx.list_stack.push(ListFrame {
        list: list_first_line as u32,
        item: 0,
    });

    for (i, item) in list.items.iter().enumerate() {
        let marker = markers.get(i).cloned().unwrap_or_else(|| "•".to_string());
        let marker_width = text_width(&marker, 11.0, F_BODY, cx.faces);
        let marker_seg = Seg {
            x: marker_left + (marker_col - marker_width).max(0.0),
            slot: F_BODY,
            text: marker,
            link: None,
            fill: Fill::Black,
            strike: false,
            width: marker_width,
        };
        // Split the item's blocks: only the FIRST block, when it is a paragraph,
        // shares the marker line; everything else — any further (loose)
        // paragraphs, nested lists, code, blockquotes, tables — is laid out as a
        // real child block at the item's content indent. This holds two
        // invariants:
        //   * non-paragraph children no longer silently vanish (they used to be
        //     flattened to plain text or dropped entirely); and
        //   * consecutive leading paragraphs no longer fuse — tokenizing them
        //     back-to-back dropped the inter-paragraph break and the joining
        //     space, giving "...para.Second para.".
        let split = usize::from(matches!(item.blocks.first(), Some(Block::Paragraph(_))));
        let (leading, rest) = item.blocks.split_at(split);

        let mut toks = Vec::new();
        for b in leading {
            if let Block::Paragraph(inl) = b {
                tokenize(inl, false, false, false, None, &mut toks);
            }
        }
        let group = cx.alloc_flow();
        let item_start_line = out.len();
        if let Some(top) = cx.list_stack.last_mut() {
            top.item = group;
        }
        // Always emit the marker line (even for an empty leading run) so the
        // bullet/number/task box shows at every nesting level.
        layout_prefixed_inlines(
            toks,
            marker_seg,
            PrefixSpec {
                content_indent,
                size: 11.0,
                gap_after: 2.0,
                flow: FlowSpec {
                    group,
                    kind: FlowKind::Paragraph,
                },
            },
            out,
            cx,
        );

        // Recurse into nested lists (deeper indent via `layout_block` ->
        // `layout_list`) and render other child blocks at the content indent.
        // Nested lists stamp their own lines first (deeper chain), so the loop
        // below only fills in this item's still-unstamped lines.
        for b in rest {
            layout_block(b, content_indent, out, cx);
        }

        // Stamp every line this item produced that a deeper list did not already
        // claim with the full enclosing list chain (outermost first). Nesting
        // depth is encoded by each mark's position in the chain.
        let chain: Vec<ListMark> = cx
            .list_stack
            .iter()
            .map(|frame| ListMark {
                list: frame.list,
                item: frame.item,
            })
            .collect();
        for line in &mut out[item_start_line..] {
            if line.list_path.is_empty() && line_has_visible_content(line) {
                line.list_path = chain.clone();
            }
        }
    }

    cx.list_stack.pop();
    // Mark the list's first line so a short intro/caption immediately before it
    // is kept with the list by the page builder (keep-with-next).
    if let Some(first) = out.get_mut(list_first_line) {
        first.flow.list_start = true;
    }
    gap(out, 6.0);
}

/// Tokenize inlines into styled line-breaking tokens, tracking inherited style.
///
/// `link` is the inherited safe PDF link target; the `Inline::Link` arm replaces
/// it with a sanitized destination for its content so link runs can be colored,
/// underlined, and made clickable at render time. Unsafe URL schemes render as
/// plain visible text, matching the HTML renderer's fail-closed behavior.
fn tokenize(
    inlines: &[Inline],
    bold: bool,
    italic: bool,
    strike: bool,
    link: Option<&LinkTarget>,
    out: &mut Vec<Tok>,
) {
    for inl in inlines {
        match inl {
            Inline::Text(t) => push_text_tokens(t, slot_of(bold, italic, false), strike, link, out),
            Inline::Code(t) => push_text_tokens(t, F_MONO, strike, link, out),
            Inline::Strong(c) => tokenize(c, true, italic, strike, link, out),
            Inline::Emphasis(c) => tokenize(c, bold, true, strike, link, out),
            Inline::Strikethrough(c) => tokenize(c, bold, italic, true, link, out),
            Inline::Link { dest, content, .. } => {
                if let Some(target) = safe_pdf_link(dest) {
                    tokenize(content, bold, italic, strike, Some(&target), out);
                } else {
                    tokenize(content, bold, italic, strike, None, out);
                }
            }
            Inline::Image { alt, .. } => {
                push_text_tokens(alt, slot_of(bold, italic, false), strike, link, out);
            }
            Inline::SoftBreak => out.push(Tok {
                text: " ".to_string(),
                slot: slot_of(bold, italic, false),
                space: true,
                hard_break: false,
                link: link.cloned(),
                strike,
            }),
            Inline::HardBreak => out.push(Tok {
                text: "\n".to_string(),
                slot: slot_of(bold, italic, false),
                space: true,
                hard_break: true,
                link: link.cloned(),
                strike,
            }),
            Inline::Html(h) => push_text_tokens(h, slot_of(bold, italic, false), strike, link, out),
        }
    }
}

/// Split `text` into word + single-space tokens (preserving spaces) with `slot`.
fn push_text_tokens(
    text: &str,
    slot: u8,
    strike: bool,
    link: Option<&LinkTarget>,
    out: &mut Vec<Tok>,
) {
    let mut word = String::new();
    for c in text.chars() {
        if is_breakable_whitespace(c) {
            if !word.is_empty() {
                out.push(Tok {
                    text: std::mem::take(&mut word),
                    slot,
                    space: false,
                    hard_break: false,
                    link: link.cloned(),
                    strike,
                });
            }
            out.push(Tok {
                text: " ".to_string(),
                slot,
                space: true,
                hard_break: false,
                link: link.cloned(),
                strike,
            });
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        out.push(Tok {
            text: word,
            slot,
            space: false,
            hard_break: false,
            link: link.cloned(),
            strike,
        });
    }
}

/// Saturating f32-points -> integer milli-point [`LayoutUnit`] (deterministic).
#[inline]
fn lu_from_points_f32(pts: f32) -> LayoutUnit {
    LayoutUnit::from_milli_points((pts * 1000.0).round() as i32)
}

/// f32 point size -> [`FontSize`] (milli-points).
#[inline]
fn font_size_of(size: f32) -> FontSize {
    FontSize::from_milli_points((size * 1000.0).round() as u32)
}

/// Per-slot box width: sum each slot-run via the slot's own face (which already
/// `impl PairMetrics`). Cross-slot kerning is intentionally dropped to match the
/// renderer, which applies GPOS per-segment (per-slot) only.
fn measure_word(runs: &[Tok], fs: FontSize, faces: &Faces) -> LayoutUnit {
    let mut w = LayoutUnit::ZERO;
    for t in runs {
        w += measure_text_with_pairs(faces.get(t.slot), &t.text, fs);
    }
    w
}

/// Build a TeX item stream from styled tokens, plus a parallel token map so each
/// `ParagraphItem` index can be mapped back to the exact tokens (with slots +
/// link flags) that produced it. Words -> `Box`; a single space between two
/// words -> `Glue`; a trailing forced penalty ends the paragraph. Leading,
/// duplicate, and trailing spaces are collapsed for cleaner breakpoints.
fn build_paragraph<'a>(
    toks: &[Tok],
    fs: FontSize,
    faces: &'a Faces,
    policy: ParagraphPolicy,
    hyphen_cache: &'a RefCell<HashMap<String, Vec<usize>>>,
) -> BuiltParagraph {
    let mut items: Vec<ParagraphItem> = Vec::new();
    let mut item_toks: Vec<Vec<Tok>> = Vec::new();
    let mut break_toks: Vec<Option<Tok>> = Vec::new();
    let mut word: Vec<Tok> = Vec::new();
    let hyphenator = Hyphenator::english();
    let word_cx = PdfWordContext {
        fs,
        faces,
        policy,
        hyphenator: &hyphenator,
        hyphen_cache,
    };

    for tok in toks {
        if tok.space {
            if tok.hard_break {
                if !word.is_empty() {
                    flush_pdf_word(
                        &mut items,
                        &mut item_toks,
                        &mut break_toks,
                        &mut word,
                        word_cx,
                    );
                }
                items.push(ParagraphItem::Penalty(Penalty {
                    width: LayoutUnit::ZERO,
                    penalty: FORCED_BREAK_PENALTY,
                    flagged: false,
                }));
                item_toks.push(Vec::new());
                break_toks.push(None);
                continue;
            }
            if !word.is_empty() {
                flush_pdf_word(
                    &mut items,
                    &mut item_toks,
                    &mut break_toks,
                    &mut word,
                    word_cx,
                );
            }
            // Only emit glue *between* two words (collapses runs of spaces).
            if matches!(items.last(), Some(ParagraphItem::Box(_))) {
                let gw = measure_text_with_pairs(faces.get(tok.slot), " ", fs);
                items.push(ParagraphItem::Glue(default_interword_glue(gw)));
                item_toks.push(vec![tok.clone()]);
                break_toks.push(None);
            }
        } else {
            word.push(tok.clone());
        }
    }
    flush_pdf_word(
        &mut items,
        &mut item_toks,
        &mut break_toks,
        &mut word,
        word_cx,
    );

    if !matches!(
        items.last(),
        Some(ParagraphItem::Penalty(Penalty {
            penalty: FORCED_BREAK_PENALTY,
            ..
        }))
    ) {
        items.push(ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        }));
        item_toks.push(Vec::new());
        break_toks.push(None);
    }
    debug_assert_eq!(items.len(), item_toks.len());
    debug_assert_eq!(items.len(), break_toks.len());
    BuiltParagraph {
        items,
        item_toks,
        break_toks,
    }
}

fn flush_pdf_word(
    items: &mut Vec<ParagraphItem>,
    item_toks: &mut Vec<Vec<Tok>>,
    break_toks: &mut Vec<Option<Tok>>,
    word: &mut Vec<Tok>,
    cx: PdfWordContext<'_>,
) {
    if word.is_empty() {
        return;
    }

    let plain: String = word.iter().map(|t| t.text.as_str()).collect();
    let points = if cx.policy.hyphenate && plain.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        // Hyphenation points are case-independent and depend only on the word's
        // letters, so the lowercase word is a sound cache key (opts are always the
        // default here). A cache hit returns exactly what the hyphenator would
        // compute, so output stays byte-identical. To avoid an allocation on the
        // common all-lowercase lookup, only fold case when an uppercase letter is
        // present.
        let key: std::borrow::Cow<'_, str> = if plain.bytes().any(|b| b.is_ascii_uppercase()) {
            std::borrow::Cow::Owned(plain.to_ascii_lowercase())
        } else {
            std::borrow::Cow::Borrowed(plain.as_str())
        };
        let cached = cx.hyphen_cache.borrow().get(key.as_ref()).cloned();
        cached.unwrap_or_else(|| {
            let pts = cx
                .hyphenator
                .hyphenation_points(&plain, HyphenationOptions::default());
            // Only cache words that actually hyphenate. A non-hyphenating word
            // gains nothing from caching, so skipping the insert (no key clone, no
            // map entry) keeps unique / non-hyphenating corpora from paying any
            // cache cost — they do not regress — while repeated hyphenating words
            // still hit.
            if !pts.is_empty() {
                let mut cache = cx.hyphen_cache.borrow_mut();
                if cache.len() < HYPHEN_CACHE_MAX {
                    cache.insert(key.into_owned(), pts.clone());
                }
            }
            pts
        })
    } else {
        Vec::new()
    };

    if points.is_empty() {
        push_pdf_word_box(
            items,
            item_toks,
            break_toks,
            std::mem::take(word),
            cx.fs,
            cx.faces,
        );
        return;
    }

    let mut start = 0usize;
    for point in points {
        let part = split_pdf_word_tokens(word, start, point);
        if !part.is_empty() {
            let hyphen_tok = part.last().map(|tok| Tok {
                text: "-".to_string(),
                slot: tok.slot,
                space: false,
                hard_break: false,
                link: tok.link.clone(),
                strike: tok.strike,
            });
            let hyphen_width = hyphen_tok.as_ref().map_or(LayoutUnit::ZERO, |tok| {
                measure_text_with_pairs(cx.faces.get(tok.slot), "-", cx.fs)
            });
            push_pdf_word_box(items, item_toks, break_toks, part, cx.fs, cx.faces);
            items.push(ParagraphItem::Penalty(Penalty {
                width: hyphen_width,
                penalty: 50,
                flagged: true,
            }));
            item_toks.push(Vec::new());
            break_toks.push(hyphen_tok);
        }
        start = point;
    }

    let tail = split_pdf_word_tokens(word, start, plain.chars().count());
    if !tail.is_empty() {
        push_pdf_word_box(items, item_toks, break_toks, tail, cx.fs, cx.faces);
    }
    word.clear();
}

fn push_pdf_word_box(
    items: &mut Vec<ParagraphItem>,
    item_toks: &mut Vec<Vec<Tok>>,
    break_toks: &mut Vec<Option<Tok>>,
    toks: Vec<Tok>,
    fs: FontSize,
    faces: &Faces,
) {
    if toks.is_empty() {
        return;
    }
    let plain: String = toks.iter().map(|t| t.text.as_str()).collect();
    let width = measure_word(&toks, fs, faces);
    items.push(ParagraphItem::Box(TextBox {
        text: plain.clone(),
        runs: StyledText::plain(&plain), // unused by breaker; width is what matters
        width,
    }));
    item_toks.push(toks);
    break_toks.push(None);
}

fn split_pdf_word_tokens(word: &[Tok], start_char: usize, end_char: usize) -> Vec<Tok> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = 0usize;
    for tok in word {
        let tok_len = tok.text.chars().count();
        let tok_start = cursor;
        let tok_end = tok_start.saturating_add(tok_len);
        cursor = tok_end;
        if end_char <= tok_start || start_char >= tok_end {
            continue;
        }
        let take_start = start_char.saturating_sub(tok_start);
        let take_end = end_char.min(tok_end).saturating_sub(tok_start);
        let text: String = tok
            .text
            .chars()
            .skip(take_start)
            .take(take_end.saturating_sub(take_start))
            .collect();
        if text.is_empty() {
            continue;
        }
        let mut part = tok.clone();
        part.text = text;
        out.push(part);
    }
    out
}

/// Optimal-break (Knuth-Plass) styled tokens into baseline lines of positioned
/// segments. Falls back to the greedy wrapper only if the optimizer yields
/// nothing (effectively unreachable given the trailing forced penalty).
fn layout_inlines(
    toks: Vec<Tok>,
    indent: f32,
    size: f32,
    gap_after: f32,
    out: &mut Vec<Line>,
    cx: &LayoutCx<'_>,
    flow: FlowSpec,
) {
    let start = out.len();
    let left = cx.page.left + indent;
    let fs = font_size_of(size);
    let policy = ParagraphPolicy::for_flow(flow.kind);
    let built = build_paragraph(&toks, fs, cx.faces, policy, &cx.hyphen_cache);

    // No renderable words -> just advance the vertical gap (old empty behavior).
    if !built
        .items
        .iter()
        .any(|it| matches!(it, ParagraphItem::Box(_)))
    {
        gap(out, gap_after);
        return;
    }

    let content_w = lu_from_points_f32((cx.page.content_w - indent).max(MIN_CONTENT_DIM));
    let breaks = break_paragraph(&built.items, content_w);
    if breaks.is_empty() {
        // Emergency fallback: the optimizer produced nothing.
        layout_inlines_greedy(toks, indent, size, gap_after, cx.faces, cx.page, out);
        mark_flow(out, start, flow.group, flow.kind);
        return;
    }

    let n = breaks.len();
    for (i, lb) in breaks.iter().enumerate() {
        let line = line_tokens_for_break(&built, lb, content_w, policy.justify && i + 1 < n);
        let segs = build_segs_adjusted(&line, left, size, cx.faces);
        out.push(Line {
            size,
            gap_after: if i + 1 == n { gap_after } else { 0.0 },
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark::default(),
            list_path: Vec::new(),
            table_cols: Vec::new(),
            segs,
            image: None,
        });
    }
    mark_flow(out, start, flow.group, flow.kind);
}

/// Layout a paragraph with a fixed prefix segment on the first physical line.
/// Used for list markers so wrapped item text hangs under the content column,
/// not under the bullet/number gutter.
#[derive(Clone, Copy)]
struct PrefixSpec {
    content_indent: f32,
    size: f32,
    gap_after: f32,
    flow: FlowSpec,
}

fn layout_prefixed_inlines(
    toks: Vec<Tok>,
    marker: Seg,
    spec: PrefixSpec,
    out: &mut Vec<Line>,
    cx: &LayoutCx<'_>,
) {
    let start = out.len();
    let left = cx.page.left + spec.content_indent;
    let fs = font_size_of(spec.size);
    let policy = ParagraphPolicy::for_flow(spec.flow.kind);
    let built = build_paragraph(&toks, fs, cx.faces, policy, &cx.hyphen_cache);

    if !built
        .items
        .iter()
        .any(|it| matches!(it, ParagraphItem::Box(_)))
    {
        out.push(Line {
            size: spec.size,
            gap_after: spec.gap_after,
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark::default(),
            list_path: Vec::new(),
            table_cols: Vec::new(),
            segs: vec![marker],
            image: None,
        });
        mark_flow(out, start, spec.flow.group, spec.flow.kind);
        return;
    }

    let content_w =
        lu_from_points_f32((cx.page.content_w - spec.content_indent).max(MIN_CONTENT_DIM));
    let breaks = break_paragraph(&built.items, content_w);
    if breaks.is_empty() {
        let before = out.len();
        layout_inlines_greedy(
            toks,
            spec.content_indent,
            spec.size,
            spec.gap_after,
            cx.faces,
            cx.page,
            out,
        );
        if let Some(first) = out.get_mut(before) {
            first.segs.insert(0, marker);
        }
        mark_flow(out, start, spec.flow.group, spec.flow.kind);
        return;
    }

    let n = breaks.len();
    let mut marker = Some(marker);
    for (i, lb) in breaks.iter().enumerate() {
        let line = line_tokens_for_break(&built, lb, content_w, policy.justify && i + 1 < n);
        let mut segs = build_segs_adjusted(&line, left, spec.size, cx.faces);
        if i == 0 {
            if let Some(marker) = marker.take() {
                segs.insert(0, marker);
            }
        }
        out.push(Line {
            size: spec.size,
            gap_after: if i + 1 == n { spec.gap_after } else { 0.0 },
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark::default(),
            list_path: Vec::new(),
            table_cols: Vec::new(),
            segs,
            image: None,
        });
    }
    mark_flow(out, start, spec.flow.group, spec.flow.kind);
}

/// The original greedy wrapper, kept as an emergency fallback (and as a
/// regression oracle in tests).
fn layout_inlines_greedy(
    toks: Vec<Tok>,
    indent: f32,
    size: f32,
    gap_after: f32,
    faces: &Faces,
    page: PageGeom,
    out: &mut Vec<Line>,
) {
    let left = page.left + indent;
    let max = (page.content_w - indent).max(MIN_CONTENT_DIM);
    let mut lines: Vec<Vec<Tok>> = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut cur_w = 0.0_f32;
    for tok in toks {
        let tw = token_width(&tok, size, faces);
        if tok.space {
            if !cur.is_empty() {
                cur.push(tok);
                cur_w += tw;
            }
        } else {
            if !cur.is_empty() && cur_w + tw > max {
                trim_trailing_spaces(&mut cur, &mut cur_w, size, faces);
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
            }
            cur.push(tok);
            cur_w += tw;
        }
    }
    if !cur.is_empty() {
        trim_trailing_spaces(&mut cur, &mut cur_w, size, faces);
        lines.push(cur);
    }

    if lines.is_empty() {
        gap(out, gap_after);
        return;
    }
    let n = lines.len();
    for (i, line) in lines.into_iter().enumerate() {
        let segs = build_segs(&line, left, size, faces);
        out.push(Line {
            size,
            gap_after: if i + 1 == n { gap_after } else { 0.0 },
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark::default(),
            list_path: Vec::new(),
            table_cols: Vec::new(),
            segs,
            image: None,
        });
    }
}

fn trim_trailing_spaces(cur: &mut Vec<Tok>, cur_w: &mut f32, size: f32, faces: &Faces) {
    while cur.last().is_some_and(|t| t.space) {
        if let Some(t) = cur.pop() {
            *cur_w -= token_width(&t, size, faces);
        }
    }
}

fn line_tokens_for_break(
    built: &BuiltParagraph,
    lb: &crate::layout::LineBreak,
    line_width: LayoutUnit,
    justify: bool,
) -> Vec<LineTok> {
    let mut line = Vec::new();
    let adjustments = glue_adjustments(&built.items, lb, line_width, justify);
    let mut adjustment_pos = 0usize;
    for idx in lb.start..lb.end {
        while adjustment_pos < adjustments.len() && adjustments[adjustment_pos].0 < idx {
            adjustment_pos += 1;
        }
        let extra = adjustments
            .get(adjustment_pos)
            .and_then(|(item_idx, extra)| (*item_idx == idx).then_some(*extra))
            .unwrap_or(0.0);
        if let Some(group) = built.item_toks.get(idx) {
            for tok in group {
                line.push(LineTok {
                    tok: tok.clone(),
                    extra_advance: if tok.space { extra } else { 0.0 },
                });
            }
        }
    }
    while line.last().is_some_and(|t| t.tok.space) {
        line.pop();
    }
    if let Some(Some(tok)) = built.break_toks.get(lb.end) {
        line.push(LineTok {
            tok: tok.clone(),
            extra_advance: 0.0,
        });
    }
    line
}

fn glue_adjustments(
    items: &[ParagraphItem],
    lb: &crate::layout::LineBreak,
    line_width: LayoutUnit,
    justify: bool,
) -> Vec<(usize, f32)> {
    if !justify || chosen_forced_break(items, lb) {
        return Vec::new();
    }
    let delta = line_width.milli_points() as i64 - lb.natural_width.milli_points() as i64;
    if delta == 0 {
        return Vec::new();
    }
    let mut glues = Vec::new();
    let mut total = 0i64;
    for (idx, item) in items.iter().enumerate().take(lb.end).skip(lb.start) {
        if let ParagraphItem::Glue(glue) = item {
            let flex = glue_flex(*glue, delta);
            if flex > 0 {
                total = total.saturating_add(flex);
                glues.push((idx, flex));
            }
        }
    }
    if total <= 0 || glues.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(glues.len());
    let mut assigned = 0i64;
    for (pos, (idx, flex)) in glues.iter().enumerate() {
        let extra = if pos + 1 == glues.len() {
            delta.saturating_sub(assigned)
        } else {
            delta.saturating_mul(*flex) / total
        };
        assigned = assigned.saturating_add(extra);
        out.push((*idx, extra as f32 / 1000.0));
    }
    out
}

fn chosen_forced_break(items: &[ParagraphItem], lb: &crate::layout::LineBreak) -> bool {
    matches!(
        items.get(lb.end),
        Some(ParagraphItem::Penalty(Penalty {
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
            ..
        }))
    )
}

fn glue_flex(glue: Glue, delta: i64) -> i64 {
    if delta > 0 {
        glue.stretch.milli_points() as i64
    } else {
        glue.shrink.milli_points() as i64
    }
}

/// Group consecutive same-slot, same-link tokens into positioned segments,
/// accumulating each segment's layout (non-kerned) advance width.
fn build_segs(toks: &[Tok], left: f32, size: f32, faces: &Faces) -> Vec<Seg> {
    let line_toks = toks
        .iter()
        .cloned()
        .map(|tok| LineTok {
            tok,
            extra_advance: 0.0,
        })
        .collect::<Vec<_>>();
    build_segs_adjusted(&line_toks, left, size, faces)
}

fn build_segs_adjusted(toks: &[LineTok], left: f32, size: f32, faces: &Faces) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut x = left;
    let mut cur: Option<Seg> = None;
    for line_tok in toks {
        let tok = &line_tok.tok;
        let tw = token_width(tok, size, faces);
        let advance = tw + line_tok.extra_advance;
        match &mut cur {
            Some(s) if s.slot == tok.slot && s.link == tok.link && s.strike == tok.strike => {
                s.text.push_str(&tok.text);
                s.width += advance;
            }
            _ => {
                if let Some(s) = cur.take() {
                    segs.push(s);
                }
                cur = Some(Seg {
                    x,
                    slot: tok.slot,
                    text: tok.text.clone(),
                    link: tok.link.clone(),
                    fill: if tok.link.is_some() {
                        Fill::Link
                    } else {
                        Fill::Black
                    },
                    strike: tok.strike,
                    width: advance,
                });
            }
        }
        x += advance;
        if tok.space
            && line_tok.extra_advance != 0.0
            && let Some(s) = cur.take()
        {
            segs.push(s);
        }
    }
    if let Some(s) = cur {
        segs.push(s);
    }
    segs
}

#[derive(Clone)]
struct CodeFrag {
    text: String,
    fill: Fill,
}

struct CodeWrapSpec<'a> {
    lang: Option<&'a str>,
    line_no: usize,
    digits: usize,
    line_numbers: bool,
    x0: f32,
    max_text_width: f32,
    number_col: f32,
    size: f32,
    faces: &'a Faces,
}

fn wrapped_code_rows(text: &str, spec: CodeWrapSpec<'_>) -> Vec<Vec<Seg>> {
    let first_text_x = spec.x0 + spec.number_col;
    let continuation_indent = CODE_HANGING_INDENT.min((spec.max_text_width * 0.35).max(0.0));
    let continuation_width = (spec.max_text_width - continuation_indent).max(8.0);
    let frag_lines = wrap_code_fragments(
        &code_fragments(spec.lang, text),
        spec.max_text_width.max(8.0),
        continuation_width,
        spec.size,
        spec.faces,
    );

    let mut rows = Vec::with_capacity(frag_lines.len().max(1));
    for (row_idx, frags) in frag_lines.into_iter().enumerate() {
        let mut segs = Vec::new();
        if spec.line_numbers && row_idx == 0 {
            segs.push(code_line_number_seg(
                spec.line_no,
                spec.digits,
                spec.x0,
                spec.number_col,
                spec.size,
                spec.faces,
            ));
        }
        let text_x = if row_idx == 0 {
            first_text_x
        } else {
            first_text_x + continuation_indent
        };
        if row_idx > 0 {
            segs.push(empty_code_seg(spec.x0));
        }
        segs.extend(code_frags_to_segs(&frags, text_x, spec.size, spec.faces));
        if segs.is_empty() {
            segs.push(empty_code_seg(text_x));
        }
        rows.push(segs);
    }

    if rows.is_empty() {
        let mut segs = Vec::new();
        if spec.line_numbers {
            segs.push(code_line_number_seg(
                spec.line_no,
                spec.digits,
                spec.x0,
                spec.number_col,
                spec.size,
                spec.faces,
            ));
        }
        segs.push(empty_code_seg(first_text_x));
        vec![segs]
    } else {
        rows
    }
}

fn code_fragments(lang: Option<&str>, text: &str) -> Vec<CodeFrag> {
    if text.is_empty() {
        return Vec::new();
    }

    let Some(lang) = lang.filter(|l| highlight::is_supported(l)) else {
        return vec![CodeFrag {
            text: expand_code_tabs(text),
            fill: Fill::Black,
        }];
    };

    let mut frags = Vec::new();
    for span in highlight::highlight(lang, text) {
        let Some(slice) = text.get(span.start..span.end) else {
            continue;
        };
        if slice.is_empty() {
            continue;
        }
        frags.push(CodeFrag {
            text: expand_code_tabs(slice),
            fill: fill_for_highlight(span.kind),
        });
    }
    frags
}

fn wrap_code_fragments(
    frags: &[CodeFrag],
    first_width: f32,
    continuation_width: f32,
    size: f32,
    faces: &Faces,
) -> Vec<Vec<CodeFrag>> {
    let mut lines: Vec<Vec<CodeFrag>> = Vec::new();
    let mut current: Vec<CodeFrag> = Vec::new();
    let mut width = 0.0f32;

    for frag in frags {
        for ch in frag.text.chars() {
            let cw = char_width(ch, size, F_MONO, faces);
            let limit = if lines.is_empty() {
                first_width
            } else {
                continuation_width
            };
            if !current.is_empty() && width + cw > limit {
                lines.push(std::mem::take(&mut current));
                width = 0.0;
            }
            push_code_frag_char(&mut current, ch, frag.fill);
            width += cw;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn push_code_frag_char(frags: &mut Vec<CodeFrag>, ch: char, fill: Fill) {
    if let Some(last) = frags.last_mut()
        && last.fill == fill
    {
        last.text.push(ch);
        return;
    }
    frags.push(CodeFrag {
        text: ch.to_string(),
        fill,
    });
}

fn code_frags_to_segs(frags: &[CodeFrag], x0: f32, size: f32, faces: &Faces) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut x = x0;
    for frag in frags {
        if frag.text.is_empty() {
            continue;
        }
        let width = text_width(&frag.text, size, F_MONO, faces);
        segs.push(Seg {
            x,
            slot: F_MONO,
            text: frag.text.clone(),
            link: None,
            fill: frag.fill,
            strike: false,
            width,
        });
        x += width;
    }
    segs
}

fn code_line_number_column_width(digits: usize, size: f32, faces: &Faces) -> f32 {
    text_width(&"9".repeat(digits), size, F_MONO, faces) + CODE_LINE_NUMBER_GAP
}

fn code_line_number_seg(
    line_no: usize,
    digits: usize,
    x0: f32,
    number_col: f32,
    size: f32,
    faces: &Faces,
) -> Seg {
    let text = format!("{line_no:>digits$}");
    let width = text_width(&text, size, F_MONO, faces);
    Seg {
        x: x0 + (number_col - CODE_LINE_NUMBER_GAP - width).max(0.0),
        slot: F_MONO,
        text,
        link: None,
        fill: Fill::Syntax(HighlightTok::Comment),
        strike: false,
        width,
    }
}

fn empty_code_seg(x: f32) -> Seg {
    Seg {
        x,
        slot: F_MONO,
        text: String::new(),
        link: None,
        fill: Fill::Black,
        strike: false,
        width: 0.0,
    }
}

fn expand_code_tabs(text: &str) -> String {
    if !text.contains('\t') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\t' {
            out.push_str("    ");
        } else {
            out.push(ch);
        }
    }
    out
}

fn fill_for_highlight(kind: HighlightTok) -> Fill {
    match kind {
        HighlightTok::Plain | HighlightTok::Punct => Fill::Black,
        other => Fill::Syntax(other),
    }
}

/// PDF device-RGB colors resolved once per render from the shared theme tokens
/// ([`ThemeColors`]), so the PDF and HTML surfaces stay visually coherent — the
/// "one theme model" doctrine. Each field mirrors the element-to-token mapping
/// the HTML stylesheet uses, so changing a theme color moves both surfaces
/// together. Code syntax-token colors are a separate code-theme palette and are
/// intentionally not part of this struct.
#[derive(Clone, Copy)]
struct Palette {
    /// Body text and plain/inline code glyphs (theme `fg`).
    fg: (f32, f32, f32),
    /// Hyperlink text and underline (theme `accent`).
    link: (f32, f32, f32),
    /// Fenced-code panel background (theme `code_bg`).
    code_panel_bg: (f32, f32, f32),
    /// Inline-code chip background (theme `code_bg`, the same token the HTML
    /// `code`/`pre` rule uses).
    code_chip_bg: (f32, f32, f32),
    /// Blockquote tint behind quoted content (theme `bg_subtle`).
    quote_bg: (f32, f32, f32),
    /// Blockquote gutter bar (theme `quote_bar`).
    quote_bar: (f32, f32, f32),
    /// Table zebra stripe for alternating body rows (theme `stripe`).
    table_stripe: (f32, f32, f32),
    /// Heading hairline and table rules (theme `border_muted`, matching the
    /// HTML `h1`/`h2` and table-cell borders).
    rule: (f32, f32, f32),
    /// Thematic-break rule (theme `border`, matching the HTML `hr`).
    hr: (f32, f32, f32),
}

impl Palette {
    fn from_colors(colors: &ThemeColors) -> Self {
        Self {
            fg: hex_rgb(&colors.fg),
            link: hex_rgb(&colors.accent),
            code_panel_bg: hex_rgb(&colors.code_bg),
            code_chip_bg: hex_rgb(&colors.code_bg),
            quote_bg: hex_rgb(&colors.bg_subtle),
            quote_bar: hex_rgb(&colors.quote_bar),
            table_stripe: hex_rgb(&colors.stripe),
            rule: hex_rgb(&colors.border_muted),
            hr: hex_rgb(&colors.border),
        }
    }
}

/// Parse a `#rrggbb` theme token into a PDF device-RGB triple in `0.0..=1.0`.
/// Falls back to black on malformed input so rendering stays infallible.
fn hex_rgb(hex: &str) -> (f32, f32, f32) {
    let s = hex.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    let component = |range: std::ops::Range<usize>| -> Option<f32> {
        let byte = u8::from_str_radix(s.get(range)?, 16).ok()?;
        Some(f32::from(byte) / 255.0)
    };
    match (component(0..2), component(2..4), component(4..6)) {
        (Some(r), Some(g), Some(b)) if s.len() == 6 => (r, g, b),
        _ => (0.0, 0.0, 0.0),
    }
}

fn fill_rgb(fill: Fill, palette: &Palette) -> (f32, f32, f32) {
    match fill {
        Fill::Black => palette.fg,
        Fill::Link => palette.link,
        Fill::Syntax(HighlightTok::Keyword) => (0.812, 0.133, 0.180),
        Fill::Syntax(HighlightTok::Type) => (0.584, 0.220, 0.000),
        Fill::Syntax(HighlightTok::Func) => (0.400, 0.224, 0.729),
        Fill::Syntax(HighlightTok::Str) => (0.039, 0.188, 0.412),
        Fill::Syntax(HighlightTok::Number) => (0.020, 0.314, 0.682),
        Fill::Syntax(HighlightTok::Comment) => (0.431, 0.467, 0.506),
        Fill::Syntax(HighlightTok::Operator) => (0.020, 0.314, 0.682),
        Fill::Syntax(HighlightTok::Plain | HighlightTok::Punct) => palette.fg,
    }
}

fn token_width(tok: &Tok, size: f32, faces: &Faces) -> f32 {
    tok.text
        .chars()
        .map(|c| faces.advance(tok.slot, c))
        .sum::<f32>()
        * size
        / 1000.0
}

fn gap(out: &mut [Line], amount: f32) {
    if let Some(last) = out.last_mut() {
        last.gap_after += amount;
    }
}

// ---- pagination + serialization --------------------------------------------

fn serialize(
    lines: &[Line],
    opts: &PdfOptions,
    faces: &Faces,
    page: PageGeom,
    profiler: &mut PdfProfiler,
) -> Result<Vec<u8>> {
    // Resolve PDF colors once from the shared theme tokens so PDF and HTML stay
    // visually coherent (the one-theme-model doctrine). See `Palette`.
    let palette = Palette::from_colors(&opts.theme.colors);

    // Which slots actually appear (skip embedding unused faces).
    let used_slot_started = profiler.checkpoint();
    let mut used_slots: Vec<u8> = SLOTS
        .into_iter()
        .filter(|&s| {
            lines
                .iter()
                .flat_map(|l| l.segs.iter())
                .any(|seg| seg.slot == s && !seg.text.is_empty())
        })
        .collect();
    if used_slots.is_empty() {
        used_slots.push(F_BODY); // always embed at least one face
    }
    profiler.record_since(
        "used_slot_scan",
        lines.len(),
        used_slots.len(),
        "scan laid-out segments to find required embedded font slots",
        used_slot_started,
    );

    // Subset each used face to the characters it renders, and keep the parsed
    // subset (its cmap gives the new glyph ids we encode in the content stream).
    let mut subsets: Vec<EmbeddedFace> = Vec::with_capacity(used_slots.len());
    let mut shaped_cache: ShapedRunCache = BTreeMap::new();
    let mut shape_cache_hits = 0usize;
    let mut shape_cache_hit_bytes = 0usize;
    let mut shape_cache_misses = 0usize;
    let mut shape_cache_miss_bytes = 0usize;
    for &slot in &used_slots {
        let source = faces.get(slot);
        let lig = source.gsub_ligatures();
        let collect_started = profiler.checkpoint();
        let mut chars: BTreeSet<char> = BTreeSet::new();
        let mut shaped_glyphs: BTreeSet<u16> = BTreeSet::new();
        let mut lig_src_uni: BTreeMap<u16, String> = BTreeMap::new();
        let mut segment_count = 0usize;
        let mut text_bytes = 0usize;
        for seg in lines
            .iter()
            .flat_map(|l| l.segs.iter())
            .filter(|seg| seg.slot == slot && !seg.text.is_empty())
        {
            segment_count += 1;
            text_bytes += seg.text.len();
            chars.extend(seg.text.chars());
            let slot_cache = shaped_cache.entry(slot).or_default();
            if slot_cache.contains_key(seg.text.as_str()) {
                shape_cache_hits += 1;
                shape_cache_hit_bytes += seg.text.len();
            } else {
                shape_cache_misses += 1;
                shape_cache_miss_bytes += seg.text.len();
                slot_cache.insert(seg.text.clone(), shape_run(source, &lig, &seg.text));
            }
            let Some(shaped) = slot_cache.get(seg.text.as_str()) else {
                continue;
            };
            shaped_glyphs.extend(shaped.glyphs.iter().copied());
            for (g, s) in &shaped.ligatures {
                lig_src_uni.entry(*g).or_insert_with(|| s.clone());
            }
        }
        profiler.record_since(
            "glyph_collection_and_shaping",
            segment_count,
            text_bytes,
            "collect rendered characters and source glyphs, applying GSUB ligatures",
            collect_started,
        );
        let keep: Vec<char> = chars.into_iter().collect();
        // Seed the subset with the chars' glyphs (so the cmap resolves) plus the
        // shaped glyphs (which add ligature glyphs no character maps to).
        let mut seed: Vec<u16> = Vec::with_capacity(keep.len().saturating_add(shaped_glyphs.len()));
        seed.extend(keep.iter().map(|&c| source.glyph_index(c)));
        seed.extend(shaped_glyphs);
        let subset_started = profiler.checkpoint();
        let Some((bytes, map)) = source.subset_glyphs(&seed, &keep) else {
            return Err(RenderError::PdfGeneration(
                "an embedded font could not be subset",
            ));
        };
        let Ok(font) = Font::parse(bytes.clone()) else {
            return Err(RenderError::PdfGeneration(
                "a subset font could not be re-parsed",
            ));
        };
        // Re-key ligature ToUnicode entries by the new (subset) glyph id.
        let mut lig_uni: BTreeMap<u16, String> = BTreeMap::new();
        for (src, s) in lig_src_uni {
            if let Some(&new) = map.get(&src) {
                lig_uni.insert(new, s);
            }
        }
        profiler.record_since(
            "font_subsetting",
            seed.len(),
            bytes.len(),
            "subset one embedded TrueType face and parse the subset font",
            subset_started,
        );
        subsets.push(EmbeddedFace {
            slot,
            bytes,
            font,
            kern: source.gpos_kerning(),
            lig,
            map,
            cmap_chars: keep,
            lig_uni,
        });
    }
    profiler.record_since(
        "shaped_segment_cache_hit",
        shape_cache_hits,
        shape_cache_hit_bytes,
        "reuse per-render shaped glyph streams for repeated exact segment text",
        None,
    );
    profiler.record_since(
        "shaped_segment_cache_miss",
        shape_cache_misses,
        shape_cache_miss_bytes,
        "shape distinct segment text once per font slot within a render",
        None,
    );

    // PASS 1 — build pages from a vertical list of lines. Backgrounds need each
    // panel's full vertical extent, which is only known once all its lines are
    // placed on pages.
    let pages_placed = profiler.measure(
        "pagination",
        lines.len(),
        "place laid-out lines onto PDF pages with simple keep/widow rules",
        || paginate_lines(lines, page),
        |_| 0,
    );
    let heading_meta = profiler.measure(
        "heading_metadata",
        lines.len(),
        "collect heading titles and stable destination ids for outlines",
        || heading_metadata(lines),
        |meta| meta.len(),
    );
    let images = profiler.measure(
        "image_asset_collection",
        lines.len(),
        "collect supported PDF image XObjects from laid-out image lines",
        || collect_pdf_images(lines),
        |images| {
            images
                .iter()
                .map(|image| image.data.len() + image.smask.as_ref().map_or(0, Vec::len))
                .sum()
        },
    );
    let image_index: BTreeMap<&str, usize> = images
        .iter()
        .enumerate()
        .map(|(idx, image)| (image.key.as_str(), idx))
        .collect();

    // PASS 2 — per page: backgrounds (code panels + inline-code chips) FIRST,
    // then text + rules, then blockquote gutter bars. Link annotations and
    // outline destinations are collected from the same placed geometry.
    let stream_generation_started = profiler.checkpoint();
    let mut scratch = RenderScratch::with_capacity(pages_placed.len(), heading_meta.len());
    let mut page_buffer_reserved_bytes = 0usize;
    for (page_idx, placed) in pages_placed.iter().enumerate() {
        let bg_capacity = estimated_background_capacity(placed);
        let body_capacity = estimated_body_capacity(placed);
        let annot_capacity = estimated_link_annotation_count(placed);
        let mark_capacity = estimated_mark_count(placed);
        page_buffer_reserved_bytes = page_buffer_reserved_bytes
            .saturating_add(bg_capacity)
            .saturating_add(body_capacity)
            .saturating_add(annot_capacity.saturating_mul(std::mem::size_of::<LinkAnnotation>()))
            .saturating_add(mark_capacity.saturating_mul(std::mem::size_of::<StructMark>()));
        let mut bg = String::with_capacity(bg_capacity);
        let mut body = String::with_capacity(body_capacity);
        let mut annots = Vec::with_capacity(annot_capacity);
        let mut marks = Vec::with_capacity(mark_capacity);
        let mut next_mcid = 0usize;

        // (a) Blockquote backgrounds: subtle page-local panels behind quoted
        // content, using the same extents as the gutter bars.
        let quote_bg = quote_extents(placed);
        for (bar_x, top_y, bot_y) in quote_bg.values() {
            bg.push_str(&rounded_rect_fill(
                bar_x - QUOTE_BG_PAD_X,
                bot_y - QUOTE_BG_PAD_V,
                page.right_x(),
                top_y + QUOTE_BG_PAD_V,
                3.0,
                palette.quote_bg,
            ));
        }

        // (a2) Table zebra stripes: one subtle full-measure tint per shaded body
        // line. Drawn per placed line so it survives page breaks deterministically;
        // bands tile within a wrapped row (band top of a line meets the band bottom
        // of the line above it). `rule_x` carries the stripe's left edge.
        for p in placed {
            if !p.line.shade {
                continue;
            }
            let size = p.line.size;
            let top_y = p.y + size * 0.92;
            let bot_y = p.y - size * 0.40;
            bg.push_str(&rounded_rect_fill(
                p.line.rule_x,
                bot_y,
                page.right_x(),
                top_y,
                0.0,
                palette.table_stripe,
            ));
        }

        // (b) Code panels: maximal runs of equal nonzero `bg` id within the page.
        let mut i = 0;
        while i < placed.len() {
            let Some(first) = placed.get(i) else { break };
            let gid = first.line.bg;
            if gid == 0 {
                i += 1;
                continue;
            }
            let mut j = i;
            while placed.get(j).is_some_and(|p| p.line.bg == gid) {
                j += 1;
            }
            if let (Some(head), Some(tail)) = (placed.get(i), placed.get(j.saturating_sub(1))) {
                let size = head.line.size;
                let x_text = head.line.segs.first().map_or(page.left, |s| s.x);
                let x0 = x_text - CODE_PAD_X;
                let x1 = page.right_x();
                let top_y = head.y + size * PANEL_ASCENT_FRAC + PANEL_PAD_V;
                let bot_y = tail.y - size * PANEL_DESCENT_FRAC - PANEL_PAD_V;
                bg.push_str(&rounded_rect_fill(
                    x0,
                    bot_y,
                    x1,
                    top_y,
                    PANEL_RADIUS,
                    palette.code_panel_bg,
                ));
            }
            i = j.max(i + 1);
        }

        // (c) Inline-code chips: F_MONO segs on non-panel, non-rule lines.
        for p in placed {
            if p.line.bg != 0 || p.line.rule {
                continue;
            }
            for seg in &p.line.segs {
                if seg.slot != F_MONO || seg.text.trim().is_empty() {
                    continue;
                }
                let w = text_width(&seg.text, p.line.size, F_MONO, faces);
                let cx0 = seg.x - CHIP_PAD_X;
                let cx1 = seg.x + w + CHIP_PAD_X;
                let cy0 = p.y - p.line.size * 0.26;
                let cy1 = p.y + p.line.size * 0.74;
                bg.push_str(&rounded_rect_fill(
                    cx0,
                    cy0,
                    cx1,
                    cy1,
                    CHIP_RADIUS,
                    palette.code_chip_bg,
                ));
            }
        }

        // (d) Text + rules. Prime the nonstroking color to the theme body color
        // so the first run (which equals `current_fill` and would otherwise skip
        // emitting `rg`) renders in the theme `fg`, not PDF-default black.
        let mut current_fill = Fill::Black;
        {
            let (r, g, b) = palette.fg;
            body.push_str(&format!("{r:.3} {g:.3} {b:.3} rg\n"));
        }
        // Per-page logical-row tracking for table cells: a new table fragment
        // (or a new logical row within it) is detected from the table flow group
        // and the per-row wrap index resetting to 0. The header is row 0.
        // `prev_table_kind` additionally catches an orphan body-row wrap line that
        // begins a continuation page after the repeated header: its wrap index is
        // != 0, but the header→body kind transition still starts a fresh row.
        let mut tbl_group: Option<u32> = None;
        let mut tbl_row: u32 = 0;
        let mut prev_table_kind: Option<FlowKind> = None;
        for p in placed {
            let line = p.line;
            let y = p.y;
            if line.flow.kind == FlowKind::Heading
                && line.flow.group != 0
                && scratch.seen_heading_groups.insert(line.flow.group)
                && let Some(meta) = heading_meta.get(&line.flow.group)
            {
                scratch.outlines.push(OutlineEntry {
                    id: meta.id.clone(),
                    title: meta.title.clone(),
                    page_index: page_idx,
                    y: (y + line.size * 0.9).min(page.top_y()),
                });
            }
            if line.rule {
                // Rules (heading hairlines, thematic breaks, table booktabs lines)
                // are decoration: wrap them as an /Artifact so they never enter the
                // tagged reading order.
                let x2 = page.right_x();
                let (rr, rg, rb) = if line.flow.kind == FlowKind::Rule {
                    palette.hr
                } else {
                    palette.rule
                };
                body.push_str(&format!(
                    "/Artifact BMC\n{rr:.3} {rg:.3} {rb:.3} RG 0.7 w \
                     {x:.2} {yy:.2} m {x2:.2} {yy:.2} l S\nEMC\n",
                    x = line.rule_x,
                    yy = y + line.size * 0.5,
                ));
            } else if !line.table_cols.is_empty() {
                // Table content line: emit one marked-content cell per seg so the
                // structure tree can carry true `/TH`/`/TD` semantics. Track the
                // logical row for cell grouping.
                match tbl_group {
                    Some(g) if g == line.flow.group => {
                        let into_body = line.flow.kind == FlowKind::TableRow
                            && prev_table_kind != Some(FlowKind::TableRow);
                        if line.flow.index == 0 || into_body {
                            tbl_row += 1;
                        }
                    }
                    _ => {
                        tbl_group = Some(line.flow.group);
                        tbl_row = 0;
                    }
                }
                prev_table_kind = Some(line.flow.kind);
                let header = line.flow.kind == FlowKind::TableHeader;
                let cell_tag = if header { "TH" } else { "TD" };
                let prefix = container_prefix(line);
                for (seg, &col) in line.segs.iter().zip(line.table_cols.iter()) {
                    if seg.text.is_empty() {
                        continue;
                    }
                    let mut path = prefix.clone();
                    path.push(SElem {
                        key: SKey::Table(line.flow.group),
                        tag: "Table",
                    });
                    path.push(SElem {
                        key: SKey::TableRow(line.flow.group, tbl_row),
                        tag: "TR",
                    });
                    path.push(SElem {
                        key: SKey::TableCell(line.flow.group, tbl_row, col),
                        tag: cell_tag,
                    });
                    body.push_str(&format!("/{cell_tag} <</MCID {next_mcid}>> BDC\n"));
                    draw_seg(
                        &mut body,
                        &mut annots,
                        &mut current_fill,
                        next_mcid,
                        seg,
                        line.size,
                        y,
                        &subsets,
                        faces,
                        &shaped_cache,
                        &palette,
                    );
                    body.push_str("EMC\n");
                    marks.push(StructMark {
                        mcid: next_mcid,
                        path,
                        alt: None,
                        bbox: None,
                    });
                    next_mcid += 1;
                }
            } else {
                let marked = line_has_visible_content(line);
                let owner = next_mcid;
                if marked {
                    let leaf = leaf_elem(line);
                    body.push_str(&format!(
                        "/{tag} <</MCID {next_mcid}>> BDC\n",
                        tag = leaf.tag
                    ));
                    let mut path = container_prefix(line);
                    path.push(leaf);
                    let (alt, bbox) = if let Some(image) = &line.image {
                        let x0 = line.rule_x;
                        let y1 = y + image.height_pt;
                        (
                            Some(image.alt.clone()),
                            Some([x0, y, x0 + image.width_pt, y1]),
                        )
                    } else {
                        (None, None)
                    };
                    marks.push(StructMark {
                        mcid: next_mcid,
                        path,
                        alt,
                        bbox,
                    });
                    next_mcid += 1;
                }
                if let Some(image) = &line.image
                    && let Some(idx) = image_index.get(image.image.key.as_str())
                {
                    let name = image_name(*idx);
                    body.push_str(&format!(
                        "q {w} 0 0 {h} {x} {y} cm /{name} Do Q\n",
                        w = pdf_num(image.width_pt),
                        h = pdf_num(image.height_pt),
                        x = pdf_num(line.rule_x),
                        y = pdf_num(y),
                    ));
                }
                for seg in &line.segs {
                    draw_seg(
                        &mut body,
                        &mut annots,
                        &mut current_fill,
                        owner,
                        seg,
                        line.size,
                        y,
                        &subsets,
                        faces,
                        &shaped_cache,
                        &palette,
                    );
                }
                if marked {
                    body.push_str("EMC\n");
                }
            }
        }

        // (e) Blockquote gutter bars: accumulate each quote's vertical extent on
        // this page (keyed by quote id), then stroke one segment per quote. The
        // bars are decorative, so they are wrapped as an /Artifact.
        let mut quote_acc = quote_extents(placed);
        if !quote_acc.is_empty() {
            body.push_str("/Artifact BMC\n");
            flush_quote_bars(&mut body, &mut quote_acc, palette.quote_bar);
            body.push_str("EMC\n");
        }

        // Backgrounds, panels, chips, and zebra stripes are purely decorative;
        // wrap the whole prelude as one /Artifact so it stays out of the tagged
        // reading order. (Per-rule and per-quote-bar artifacts are wrapped at
        // their draw sites above and below.)
        let mut stream =
            String::with_capacity(bg.len().saturating_add(body.len()).saturating_add(24));
        if !bg.is_empty() {
            stream.push_str("/Artifact BMC\n");
            stream.push_str(&bg);
            stream.push_str("EMC\n");
        }
        stream.push_str(&body);

        scratch.pages.push(PageContent {
            stream,
            annots,
            marks,
        });
    }
    profiler.record_since(
        "page_content_buffer_preallocation",
        scratch.pages.len(),
        page_buffer_reserved_bytes,
        "pre-size per-page content, annotation, and structure-mark buffers",
        None,
    );
    if scratch.pages.is_empty() {
        scratch.pages.push(PageContent {
            stream: String::new(),
            annots: Vec::new(),
            marks: Vec::new(),
        });
    }
    let page_stream_bytes = scratch.pages.iter().map(|page| page.stream.len()).sum();
    profiler.record_since(
        "page_content_stream_generation",
        scratch.pages.len(),
        page_stream_bytes,
        "generate page drawing operators, annotations, outlines, and structure marks",
        stream_generation_started,
    );

    build_pdf(
        &scratch.pages,
        &scratch.outlines,
        &subsets,
        &images,
        opts,
        page,
        profiler,
    )
}

struct RenderScratch {
    pages: Vec<PageContent>,
    outlines: Vec<OutlineEntry>,
    seen_heading_groups: BTreeSet<u32>,
}

impl RenderScratch {
    fn with_capacity(page_count: usize, heading_count: usize) -> Self {
        Self {
            pages: Vec::with_capacity(page_count.max(1)),
            outlines: Vec::with_capacity(heading_count),
            seen_heading_groups: BTreeSet::new(),
        }
    }
}

struct PageContent {
    stream: String,
    annots: Vec<LinkAnnotation>,
    marks: Vec<StructMark>,
}

/// Identity of a structure element, used to share a container across the
/// consecutive marks that belong to it (e.g. every cell of one table row reuses
/// the same `TableRow`, every wrapped line of one paragraph reuses the same
/// `Paragraph`). Two marks open/extend the same element iff their keys compare
/// equal at the same path depth.
#[derive(Clone, PartialEq, Eq)]
enum SKey {
    BlockQuote(usize),
    List(u32),
    ListItem(u32),
    ListBody(u32),
    Paragraph(u32),
    Heading(u32),
    Code(u32),
    Table(u32),
    TableRow(u32, u32),
    TableCell(u32, u32, u32),
    Figure(u32),
    Link(u32),
}

/// One element on a mark's container path: its sharing key plus the `/S`
/// structure type emitted for it.
#[derive(Clone)]
struct SElem {
    key: SKey,
    tag: &'static str,
}

/// A single piece of marked content (one `/MCID`) plus the structure path that
/// owns it. The path runs from just below the implicit `/Document` root down to
/// the owning element (whose `tag` is also the content-stream BDC operand). The
/// structure-tree builder diffs consecutive paths to open and reuse shared
/// ancestors, producing a properly nested tree.
#[derive(Clone)]
struct StructMark {
    mcid: usize,
    path: Vec<SElem>,
    /// `/Alt` text for the owning element (figures carry their image alt).
    alt: Option<String>,
    /// Figure bounding box `[x0, y0, x1, y1]` in page coordinates, emitted as a
    /// layout `/BBox` attribute so assistive tech can locate the image region.
    bbox: Option<[f32; 4]>,
}

#[derive(Clone)]
struct LinkAnnotation {
    rect: Rect,
    target: LinkTarget,
    /// `/MCID` of the structure leaf (a `/Link` element) that owns this
    /// annotation on its page, so the structure tree can reference the
    /// annotation back with an `/OBJR` (PDF/UA links live in the tree, not just
    /// the page `/Annots`).
    owner_mcid: Option<usize>,
}

/// One node in the assembled structure tree. Node 0 is always the `/Document`
/// root; every other node is a container or leaf element. Object numbers are
/// `struct_elem_base + node_index`, so the whole tree can be serialized after a
/// single dynamic count.
struct SNode {
    tag: &'static str,
    /// Parent node index (node 0's parent is the `/StructTreeRoot`).
    parent: usize,
    kids: Vec<SKid>,
    /// `/Alt` text (figures).
    alt: Option<String>,
    /// Figure bounding box `[x0, y0, x1, y1]`.
    bbox: Option<[f32; 4]>,
    /// True for `/TH` cells, which emit a `/Scope /Column` table attribute.
    scope_column: bool,
    /// Page of this element's marked content, emitted as `/Pg` when present.
    page: Option<usize>,
}

/// A child of a structure node: a nested element, a marked-content reference, or
/// an object reference (to a link annotation).
enum SKid {
    Node(usize),
    Mcr { page: usize, mcid: usize },
    ObjR { page: usize, local: usize },
}

struct StructTree {
    nodes: Vec<SNode>,
    /// `parent_tree[page][mcid]` = owning node index, in dense `/MCID` order.
    parent_tree: Vec<Vec<usize>>,
    /// `annot_owner[page][local]` = owning node index of the resolved link
    /// annotation (in the same filter order as annotation object numbering), or
    /// `None` when the annotation has no owning structure element. Drives the
    /// `/StructParent` back-reference required for tagged links (PDF/UA).
    annot_owner: Vec<Vec<Option<usize>>>,
}

/// Assemble the hierarchical structure tree from per-page marks. Each mark
/// carries the full container path from just below `/Document` to its owning
/// element; consecutive marks that share a prefix reuse the same container
/// nodes, so tables, lists, and blockquotes nest correctly. The open stack is
/// reset per page (a block split across a page break becomes two sibling
/// elements under `/Document`, which is valid and keeps the parent tree simple).
fn build_struct_tree(pages: &[PageContent], dest_ids: &BTreeSet<&str>) -> StructTree {
    let mut nodes: Vec<SNode> = vec![SNode {
        tag: "Document",
        parent: 0,
        kids: Vec::new(),
        alt: None,
        bbox: None,
        scope_column: false,
        page: None,
    }];
    let mut parent_tree: Vec<Vec<usize>> = Vec::with_capacity(pages.len());
    let mut annot_owner: Vec<Vec<Option<usize>>> = Vec::with_capacity(pages.len());

    for (page_idx, page) in pages.iter().enumerate() {
        let mut leaf_for_mcid: Vec<usize> = Vec::new();
        // Currently open path elements below /Document: (sharing key, node index).
        let mut open: Vec<(SKey, usize)> = Vec::new();

        let mut marks: Vec<&StructMark> = page.marks.iter().collect();
        marks.sort_by_key(|m| m.mcid);

        for mark in marks {
            // Reuse the longest shared prefix with the currently open path, then
            // open the remaining elements.
            let mut common = 0;
            while common < open.len()
                && common < mark.path.len()
                && open[common].0 == mark.path[common].key
            {
                common += 1;
            }
            open.truncate(common);
            for elem in &mark.path[common..] {
                let parent_node = open.last().map_or(0, |&(_, idx)| idx);
                let node_index = nodes.len();
                nodes.push(SNode {
                    tag: elem.tag,
                    parent: parent_node,
                    kids: Vec::new(),
                    alt: None,
                    bbox: None,
                    scope_column: elem.tag == "TH",
                    page: None,
                });
                nodes[parent_node].kids.push(SKid::Node(node_index));
                open.push((elem.key.clone(), node_index));
            }

            let owner = open.last().map_or(0, |&(_, idx)| idx);
            if mark.alt.is_some() {
                nodes[owner].alt = mark.alt.clone();
            }
            if mark.bbox.is_some() {
                nodes[owner].bbox = mark.bbox;
            }
            nodes[owner].kids.push(SKid::Mcr {
                page: page_idx,
                mcid: mark.mcid,
            });
            if nodes[owner].page.is_none() {
                nodes[owner].page = Some(page_idx);
            }
            if leaf_for_mcid.len() <= mark.mcid {
                leaf_for_mcid.resize(mark.mcid + 1, 0);
            }
            leaf_for_mcid[mark.mcid] = owner;
        }

        // Reference each resolved link annotation back from its owning /Link
        // element with an /OBJR, in the same filtered order used for annotation
        // object numbering, and record the owning element so the annotation can
        // carry the reverse /StructParent.
        let mut owners_this_page: Vec<Option<usize>> = Vec::new();
        let mut local = 0usize;
        for annot in &page.annots {
            if !annotation_is_resolved(annot, dest_ids) {
                continue;
            }
            let owner = annot.owner_mcid.and_then(|m| leaf_for_mcid.get(m).copied());
            if let Some(owner) = owner {
                nodes[owner].kids.push(SKid::ObjR {
                    page: page_idx,
                    local,
                });
            }
            owners_this_page.push(owner);
            local += 1;
        }
        annot_owner.push(owners_this_page);

        parent_tree.push(leaf_for_mcid);
    }

    StructTree {
        nodes,
        parent_tree,
        annot_owner,
    }
}

fn estimated_background_capacity(placed: &[Placed<'_>]) -> usize {
    let quote_bars = placed
        .iter()
        .map(|placed| placed.line.quote_bars.len())
        .sum::<usize>();
    let shaded_lines = placed.iter().filter(|placed| placed.line.shade).count();
    let panel_lines = placed.iter().filter(|placed| placed.line.bg != 0).count();
    let mono_chips = placed
        .iter()
        .flat_map(|placed| placed.line.segs.iter())
        .filter(|seg| seg.slot == F_MONO && !seg.text.trim().is_empty())
        .count();

    quote_bars
        .saturating_mul(160)
        .saturating_add(shaded_lines.saturating_mul(160))
        .saturating_add(panel_lines.saturating_mul(48))
        .saturating_add(mono_chips.saturating_mul(160))
}

fn estimated_body_capacity(placed: &[Placed<'_>]) -> usize {
    let mut text_bytes = 0usize;
    let mut segments = 0usize;
    let mut struck = 0usize;
    let mut linked = 0usize;
    let mut images = 0usize;
    let mut rules = 0usize;
    for placed in placed {
        if placed.line.rule {
            rules += 1;
        }
        if placed.line.image.is_some() {
            images += 1;
        }
        for seg in &placed.line.segs {
            if seg.text.is_empty() {
                continue;
            }
            segments += 1;
            text_bytes = text_bytes.saturating_add(seg.text.len());
            if seg.strike {
                struck += 1;
            }
            if seg.link.is_some() {
                linked += 1;
            }
        }
    }

    placed
        .len()
        .saturating_mul(48)
        .saturating_add(segments.saturating_mul(96))
        .saturating_add(text_bytes.saturating_mul(6))
        .saturating_add(struck.saturating_mul(96))
        .saturating_add(linked.saturating_mul(160))
        .saturating_add(images.saturating_mul(96))
        .saturating_add(rules.saturating_mul(96))
}

fn estimated_link_annotation_count(placed: &[Placed<'_>]) -> usize {
    placed
        .iter()
        .flat_map(|placed| placed.line.segs.iter())
        .filter(|seg| seg.link.is_some() && seg.width > 0.0)
        .count()
}

fn estimated_mark_count(placed: &[Placed<'_>]) -> usize {
    placed
        .iter()
        .filter(|placed| line_has_visible_content(placed.line))
        .count()
}

#[derive(Clone, Copy)]
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

struct OutlineEntry {
    id: String,
    title: String,
    page_index: usize,
    y: f32,
}

struct HeadingMeta {
    id: String,
    title: String,
}

#[derive(Default)]
struct HeadingIdState {
    used: BTreeSet<String>,
    next_suffix: BTreeMap<String, usize>,
}

impl HeadingIdState {
    fn heading_id(&mut self, text: &str) -> String {
        let mut base = slug(text);
        if base.is_empty() {
            base.push_str("section");
        }

        let mut suffix = self.next_suffix.get(&base).copied().unwrap_or(1);
        loop {
            let candidate = if suffix == 1 {
                base.clone()
            } else {
                format!("{base}-{suffix}")
            };
            suffix += 1;
            if self.used.insert(candidate.clone()) {
                self.next_suffix.insert(base, suffix);
                return candidate;
            }
        }
    }
}

fn heading_metadata(lines: &[Line]) -> BTreeMap<u32, HeadingMeta> {
    let mut order = Vec::new();
    let mut titles: BTreeMap<u32, String> = BTreeMap::new();
    for line in lines
        .iter()
        .filter(|line| line.flow.kind == FlowKind::Heading && line.flow.group != 0)
    {
        if !titles.contains_key(&line.flow.group) {
            order.push(line.flow.group);
        }
        let title = titles.entry(line.flow.group).or_default();
        if !title.is_empty() && !title.ends_with(' ') {
            title.push(' ');
        }
        for seg in &line.segs {
            title.push_str(&seg.text);
        }
    }

    let mut state = HeadingIdState::default();
    let mut out = BTreeMap::new();
    for group in order {
        let title = titles
            .remove(&group)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Section".to_string());
        let id = state.heading_id(&title);
        out.insert(group, HeadingMeta { id, title });
    }
    out
}

fn collect_pdf_images(lines: &[Line]) -> Vec<PdfImageData> {
    let mut by_key: BTreeMap<String, PdfImageData> = BTreeMap::new();
    for image in lines.iter().filter_map(|line| line.image.as_ref()) {
        by_key
            .entry(image.image.key.clone())
            .or_insert_with(|| image.image.clone());
    }
    by_key.into_values().collect()
}

fn image_name(index: usize) -> String {
    format!("Im{}", index + 1)
}

fn line_has_visible_content(line: &Line) -> bool {
    line.image.is_some() || line.segs.iter().any(|seg| !seg.text.is_empty())
}

/// Draw one styled segment's text (with strikethrough, link underline, and link
/// annotation) into `body` at baseline `y`. Shared by the whole-line and
/// per-table-cell rendering paths so both emit byte-identical glyph runs. Link
/// annotations record `owner_mcid` so the structure tree can reference them with
/// an `/OBJR`. `current_fill` is threaded so redundant `rg` operators are still
/// elided across calls exactly as the old single-pass loop did.
#[allow(clippy::too_many_arguments)]
fn draw_seg(
    body: &mut String,
    annots: &mut Vec<LinkAnnotation>,
    current_fill: &mut Fill,
    owner_mcid: usize,
    seg: &Seg,
    size: f32,
    y: f32,
    subsets: &[EmbeddedFace],
    faces: &Faces,
    shaped_cache: &ShapedRunCache,
    palette: &Palette,
) {
    if seg.text.is_empty() {
        return;
    }
    let Some(face) = subsets.iter().find(|f| f.slot == seg.slot) else {
        return;
    };
    let source = faces.get(seg.slot);
    let fallback;
    let shaped = match shaped_cache
        .get(&seg.slot)
        .and_then(|slot_cache| slot_cache.get(seg.text.as_str()))
    {
        Some(run) => run.glyphs.as_slice(),
        None => {
            fallback = shape_run(source, &face.lig, &seg.text);
            fallback.glyphs.as_slice()
        }
    };
    if seg.fill != *current_fill {
        let (r, g, b) = fill_rgb(seg.fill, palette);
        body.push_str(&format!("{r:.3} {g:.3} {b:.3} rg\n"));
        *current_fill = seg.fill;
    }
    body.push_str(&format!(
        "BT /F{f} {s:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm {tj} TJ ET\n",
        f = seg.slot,
        s = size,
        x = seg.x,
        tj = kerned_tj(&face.map, source, &face.kern, shaped),
    ));
    // Strikethrough: a thin stroke through the run's middle, in the text's own
    // color (stroke `RG`, leaving the text fill `rg` untouched).
    if seg.strike && seg.width > 0.0 {
        let (r, g, b) = fill_rgb(seg.fill, palette);
        let sy = y + size * 0.30;
        let sw = (size * 0.06).max(0.4);
        body.push_str(&format!(
            "{r:.3} {g:.3} {b:.3} RG {sw:.2} w {x1:.2} {sy:.2} m {x2:.2} {sy:.2} l S\n",
            x1 = seg.x,
            x2 = seg.x + seg.width,
        ));
    }
    if let Some(target) = &seg.link {
        let (r, g, b) = palette.link;
        let (fr, fg2, fb) = palette.fg;
        let uy = y - size * 0.12;
        let uw = (size * 0.06).max(0.4);
        body.push_str(&format!(
            "{r:.3} {g:.3} {b:.3} RG {uw:.2} w \
             {x1:.2} {uy:.2} m {x2:.2} {uy:.2} l S\n{fr:.3} {fg2:.3} {fb:.3} rg\n",
            x1 = seg.x,
            x2 = seg.x + seg.width,
        ));
        *current_fill = Fill::Black;
        if seg.width > 0.0 {
            annots.push(LinkAnnotation {
                rect: Rect {
                    x0: seg.x,
                    y0: y - size * 0.28,
                    x1: seg.x + seg.width,
                    y1: y + size * 0.86,
                },
                target: target.clone(),
                owner_mcid: Some(owner_mcid),
            });
        }
    }
}

/// The `/H1`..`/H6`/`/H` structure tag for a heading line, by its display size.
/// Sizes below H3 collapse to the generic `/H` (the writer cannot recover the
/// exact source level from size alone for H4–H6, which share the body measure).
fn heading_tag(size: f32) -> &'static str {
    if size >= 23.0 {
        "H1"
    } else if size >= 18.0 {
        "H2"
    } else if size >= 15.0 {
        "H3"
    } else {
        "H"
    }
}

/// The structure element that directly owns a non-table line's marked content.
/// `Figure`/`Link` are detected first (image, then any link run); headings keep
/// their heading semantics even when they contain a link.
fn leaf_elem(line: &Line) -> SElem {
    if line.image.is_some() {
        return SElem {
            key: SKey::Figure(line.flow.group),
            tag: "Figure",
        };
    }
    match line.flow.kind {
        FlowKind::Heading => SElem {
            key: SKey::Heading(line.flow.group),
            tag: heading_tag(line.size),
        },
        FlowKind::Code => SElem {
            key: SKey::Code(line.bg),
            tag: "Code",
        },
        _ => {
            if line.segs.iter().any(|seg| seg.link.is_some()) {
                SElem {
                    key: SKey::Link(line.flow.group),
                    tag: "Link",
                }
            } else {
                SElem {
                    key: SKey::Paragraph(line.flow.group),
                    tag: "P",
                }
            }
        }
    }
}

/// The enclosing container path (below `/Document`) shared by every leaf on this
/// line: the blockquotes and lists that enclose it, outermost-first. Tables and
/// leaves append their own elements after this.
///
/// Blockquotes and lists are merged by their out-vec start index (every
/// blockquote id and list id is the index where that block opened). Markdown
/// block structure is a tree, so start order is exactly nesting order — this
/// gets `> - item` (list inside quote) and `- > quote` (quote inside list)
/// right, rather than always nesting one kind inside the other.
fn container_prefix(line: &Line) -> Vec<SElem> {
    enum Container {
        Quote(usize),
        List { list: u32, item: u32 },
    }
    let mut containers: Vec<(usize, Container)> =
        Vec::with_capacity(line.quote_bars.len().saturating_add(line.list_path.len()));
    for (qid, _x) in &line.quote_bars {
        containers.push((*qid, Container::Quote(*qid)));
    }
    for lm in &line.list_path {
        containers.push((
            lm.list as usize,
            Container::List {
                list: lm.list,
                item: lm.item,
            },
        ));
    }
    containers.sort_by_key(|(start, _)| *start);

    let mut path = Vec::new();
    for (_, container) in containers {
        match container {
            Container::Quote(qid) => path.push(SElem {
                key: SKey::BlockQuote(qid),
                tag: "BlockQuote",
            }),
            Container::List { list, item } => {
                path.push(SElem {
                    key: SKey::List(list),
                    tag: "L",
                });
                path.push(SElem {
                    key: SKey::ListItem(item),
                    tag: "LI",
                });
                path.push(SElem {
                    key: SKey::ListBody(item),
                    tag: "LBody",
                });
            }
        }
    }
    path
}

fn paginate_lines<'a>(lines: &'a [Line], page: PageGeom) -> Vec<Vec<Placed<'a>>> {
    if lines.is_empty() {
        return vec![Vec::new()];
    }

    let mut pages = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let end = choose_page_break(lines, start, page);
        pages.push(place_lines(lines, start, end, page));
        start = end;
    }
    pages
}

fn choose_page_break(lines: &[Line], start: usize, page: PageGeom) -> usize {
    let full_capacity = (page.top_y() - page.bottom).max(MIN_CONTENT_DIM);
    let capacity =
        (full_capacity - repeated_table_header_height(lines, start)).max(MIN_CONTENT_DIM);
    let mut used = 0.0f32;
    let mut last_fit = start;

    for (idx, line) in lines.iter().enumerate().skip(start) {
        let leading = line_leading(line);
        if idx > start && used + leading > capacity {
            break;
        }
        used += leading + line.gap_after;
        last_fit = idx + 1;
    }

    if last_fit <= start + 1 || last_fit >= lines.len() {
        return last_fit.max(start + 1).min(lines.len());
    }

    let mut best = last_fit;
    let mut best_score = f32::INFINITY;
    for candidate in (start + 1)..=last_fit {
        let score = break_score(lines, start, candidate, capacity);
        if score < best_score {
            best_score = score;
            best = candidate;
        }
    }

    best.max(start + 1).min(lines.len())
}

fn break_score(lines: &[Line], start: usize, candidate: usize, capacity: f32) -> f32 {
    let used = vertical_height(&lines[start..candidate]);
    let remaining = (capacity - used).max(0.0);
    let fill_badness = (remaining / capacity.max(1.0)).powi(2) * 10_000.0;
    fill_badness + break_penalty(lines, candidate)
}

fn break_penalty(lines: &[Line], candidate: usize) -> f32 {
    if candidate == 0 || candidate >= lines.len() {
        return 0.0;
    }

    let before = &lines[candidate - 1];
    let after = &lines[candidate];
    let mut penalty = 0.0;

    // Keep headings with at least the first following content line. This is the
    // PDF analogue of TeX's high after-heading penalty.
    if before.flow.kind == FlowKind::Heading {
        penalty += 1_000_000.0;
    }

    if before.flow.group == after.flow.group
        && matches!(
            before.flow.kind,
            FlowKind::TableHeader | FlowKind::TableRule
        )
        && after.flow.kind == FlowKind::TableRow
    {
        penalty += 900_000.0;
    }

    // Generalized keep-with-next: keep a short intro/caption paragraph with the
    // structured block it introduces (a table, code block, figure/image, or
    // list), so a one- or two-line caption never strands alone at the foot of a
    // page. This extends the heading keep above to captioned blocks. List items
    // share the Paragraph kind, so the list's first line is tagged with
    // `flow.list_start` in `layout_list` to make a list start detectable here.
    let before_ends_short_intro = before.flow.kind == FlowKind::Paragraph
        && before.flow.group != after.flow.group
        && before.flow.index + 1 == before.flow.count
        && before.flow.count <= 2;
    let after_starts_captioned_block = (matches!(
        after.flow.kind,
        FlowKind::TableHeader | FlowKind::Code | FlowKind::Image
    ) && after.flow.index == 0)
        || after.flow.list_start;
    if before_ends_short_intro && after_starts_captioned_block {
        penalty += 700_000.0;
    }

    // Avoid club/widow breaks when splitting a paragraph-like group: at least
    // two lines should remain on both sides when the paragraph has enough lines.
    if before.flow.group == after.flow.group && before.flow.kind == FlowKind::Paragraph {
        let before_count = before.flow.index + 1;
        let after_count = after.flow.count.saturating_sub(after.flow.index);
        if before.flow.count >= 4 && (before_count < 2 || after_count < 2) {
            penalty += 850_000.0;
        }
    }

    penalty
}

// ---- render-tree golden support (bead qw1.1.2) ------------------------------
//
// A deterministic textual dump of the *paginated layout* — the placed lines with
// their baselines, structure roles, and per-segment x/size/font/fill/text. Byte
// determinism proves the writer is stable run-to-run; this render tree, pinned
// as a golden, additionally catches appearance regressions across code changes
// (baseline shifts, x-position/layout drift, text-color drift, wrong structure)
// that byte-identical-but-wrong output would otherwise hide.

#[cfg(all(test, target_os = "linux"))]
fn render_tree_fill_label(fill: Fill, palette: &Palette) -> String {
    // Label a fill by its emitted device-RGB so color drift shows up in the
    // golden diff exactly as it would in the rendered page.
    let (r, g, b) = fill_rgb(fill, palette);
    format!("{r:.3},{g:.3},{b:.3}")
}

/// Serialize the paginated layout to a stable, human-reviewable render tree.
#[cfg(all(test, target_os = "linux"))]
fn serialize_render_tree(pages: &[Vec<Placed<'_>>], page: PageGeom, palette: &Palette) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "page {:.2}x{:.2} content-w {:.2} | {} page(s)",
        page.width,
        page.height,
        page.content_w,
        pages.len()
    );
    for (page_index, placed) in pages.iter().enumerate() {
        let _ = writeln!(out, "=== page {page_index} ({} lines) ===", placed.len());
        for p in placed {
            let line = p.line;
            let path = container_prefix(line)
                .iter()
                .map(|elem| elem.tag)
                .collect::<Vec<_>>()
                .join(">");
            let prefix = if path.is_empty() {
                String::new()
            } else {
                format!("[{path}] ")
            };
            if line.rule {
                let _ = writeln!(
                    out,
                    "  rule y={:.2} x={:.2} kind={:?}",
                    p.y + line.size * 0.5,
                    line.rule_x,
                    line.flow.kind
                );
            } else if let Some(image) = &line.image {
                let _ = writeln!(
                    out,
                    "  {prefix}figure y={:.2} x={:.2} w={:.2} h={:.2} alt={:?}",
                    p.y, line.rule_x, image.width_pt, image.height_pt, image.alt
                );
            } else if !line.table_cols.is_empty() {
                let header = line.flow.kind == FlowKind::TableHeader;
                let cell = if header { "TH" } else { "TD" };
                for (seg, &col) in line.segs.iter().zip(line.table_cols.iter()) {
                    if seg.text.is_empty() {
                        continue;
                    }
                    let _ = writeln!(
                        out,
                        "  {prefix}{cell} col={col} x={:.2} y={:.2} size={:.1} f={} fill={} {:?}",
                        seg.x,
                        p.y,
                        line.size,
                        seg.slot,
                        render_tree_fill_label(seg.fill, palette),
                        seg.text
                    );
                }
            } else {
                let tag = leaf_elem(line).tag;
                for seg in &line.segs {
                    if seg.text.is_empty() {
                        continue;
                    }
                    let strike = if seg.strike { " strike" } else { "" };
                    let link = if seg.link.is_some() { " link" } else { "" };
                    let _ = writeln!(
                        out,
                        "  {prefix}{tag} x={:.2} y={:.2} size={:.1} f={} fill={}{strike}{link} {:?}",
                        seg.x,
                        p.y,
                        line.size,
                        seg.slot,
                        render_tree_fill_label(seg.fill, palette),
                        seg.text
                    );
                }
            }
        }
    }
    out
}

/// Render Markdown to the deterministic render tree (parse -> layout -> paginate
/// -> serialize), reusing the exact pipeline the PDF writer uses.
#[cfg(all(test, target_os = "linux"))]
fn render_tree_debug(markdown: &str, opts: &PdfOptions) -> String {
    let doc = crate::parse_markdown(markdown);
    let page = PageGeom::from_theme(&opts.theme);
    let Ok(faces) = Faces::load(opts) else {
        return "FONT LOAD ERROR\n".to_string();
    };
    let palette = Palette::from_colors(&opts.theme.colors);
    let lines = layout(&doc.blocks, opts, &faces, page);
    let pages = paginate_lines(&lines, page);
    serialize_render_tree(&pages, page, &palette)
}

#[cfg(test)]
mod keep_with_next_tests {
    use super::*;

    fn line(kind: FlowKind, group: u32, index: usize, count: usize) -> Line {
        Line {
            size: 11.0,
            gap_after: 0.0,
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark {
                group,
                index,
                count,
                kind,
                list_start: false,
            },
            list_path: Vec::new(),
            table_cols: Vec::new(),
            segs: Vec::new(),
            image: None,
        }
    }

    #[test]
    fn short_caption_before_table_code_or_image_is_kept() {
        for kind in [FlowKind::TableHeader, FlowKind::Code, FlowKind::Image] {
            let lines = [line(FlowKind::Paragraph, 1, 0, 1), line(kind, 2, 0, 1)];
            assert!(
                break_penalty(&lines, 1) >= 700_000.0,
                "a short caption before {kind:?} must be kept with it"
            );
        }
    }

    #[test]
    fn long_body_paragraph_before_a_table_is_not_treated_as_a_caption() {
        // A 4-line body paragraph ending right before a table is not a caption.
        let lines = [
            line(FlowKind::Paragraph, 1, 3, 4),
            line(FlowKind::TableHeader, 2, 0, 1),
        ];
        assert_eq!(
            break_penalty(&lines, 1),
            0.0,
            "a long body paragraph before a table must not trigger the caption keep"
        );
    }

    #[test]
    fn caption_keep_requires_the_block_to_start_at_its_first_line() {
        // Crossing into the MIDDLE of a table row run is not a caption boundary.
        let lines = [
            line(FlowKind::Paragraph, 1, 0, 1),
            line(FlowKind::TableRow, 2, 1, 3),
        ];
        assert_eq!(break_penalty(&lines, 1), 0.0);
    }

    #[test]
    fn short_intro_before_a_list_is_kept_but_a_long_paragraph_is_not() {
        let mut list_start = line(FlowKind::Paragraph, 2, 0, 3);
        list_start.flow.list_start = true;
        let kept = [line(FlowKind::Paragraph, 1, 0, 1), list_start];
        assert!(
            break_penalty(&kept, 1) >= 700_000.0,
            "a short intro before a list must be kept with it"
        );

        let mut list_start2 = line(FlowKind::Paragraph, 2, 0, 3);
        list_start2.flow.list_start = true;
        let not_kept = [line(FlowKind::Paragraph, 1, 3, 4), list_start2];
        assert_eq!(
            break_penalty(&not_kept, 1),
            0.0,
            "a long body paragraph before a list is not a caption/intro"
        );

        // A paragraph before a non-list-start paragraph is unaffected.
        let plain = [
            line(FlowKind::Paragraph, 1, 0, 1),
            line(FlowKind::Paragraph, 2, 0, 1),
        ];
        assert_eq!(break_penalty(&plain, 1), 0.0);
    }

    #[test]
    fn existing_heading_and_table_header_keeps_are_unchanged() {
        let heading = [
            line(FlowKind::Heading, 1, 0, 1),
            line(FlowKind::Paragraph, 2, 0, 3),
        ];
        assert!(
            break_penalty(&heading, 1) >= 1_000_000.0,
            "heading keep-with-next regression"
        );
        let table = [
            line(FlowKind::TableHeader, 5, 0, 3),
            line(FlowKind::TableRow, 5, 1, 3),
        ];
        assert!(
            break_penalty(&table, 1) >= 900_000.0,
            "table-header keep-with-first-row regression"
        );
    }
}

fn vertical_height(lines: &[Line]) -> f32 {
    lines
        .iter()
        .map(|line| line_leading(line) + line.gap_after)
        .sum()
}

fn line_leading(line: &Line) -> f32 {
    line.size * 1.32
}

fn place_lines<'a>(lines: &'a [Line], start: usize, end: usize, page: PageGeom) -> Vec<Placed<'a>> {
    let repeated = repeated_table_header_lines(lines, start);
    let mut placed = Vec::with_capacity(repeated.len() + end.saturating_sub(start));
    let mut y = page.top_y();
    for line in repeated.into_iter().chain(lines[start..end].iter()) {
        y -= line_leading(line);
        placed.push(Placed { line, y });
        y -= line.gap_after;
    }
    placed
}

fn repeated_table_header_height(lines: &[Line], start: usize) -> f32 {
    repeated_table_header_lines(lines, start)
        .iter()
        .copied()
        .map(|line| line_leading(line) + line.gap_after)
        .sum()
}

fn repeated_table_header_lines(lines: &[Line], start: usize) -> Vec<&Line> {
    let Some(first) = lines.get(start) else {
        return Vec::new();
    };
    if first.flow.kind != FlowKind::TableRow {
        return Vec::new();
    }
    let group = first.flow.group;
    let mut header_start = None;
    let mut header_end = None;
    for (idx, line) in lines[..start].iter().enumerate().rev() {
        if line.flow.group != group {
            break;
        }
        if matches!(line.flow.kind, FlowKind::TableHeader | FlowKind::TableRule) {
            header_start = Some(idx);
            if header_end.is_none() {
                header_end = Some(idx + 1);
            }
        } else if header_start.is_some() {
            break;
        }
    }

    let (Some(start_idx), Some(end_idx)) = (header_start, header_end) else {
        return Vec::new();
    };
    lines[start_idx..end_idx]
        .iter()
        .filter(|line| matches!(line.flow.kind, FlowKind::TableHeader | FlowKind::TableRule))
        .collect()
}

/// A line placed on a page with its computed baseline `y`.
struct Placed<'a> {
    line: &'a Line,
    y: f32,
}

fn quote_extents(placed: &[Placed<'_>]) -> BTreeMap<usize, (f32, f32, f32)> {
    let mut acc: BTreeMap<usize, (f32, f32, f32)> = BTreeMap::new();
    for p in placed {
        for &(id, bar_x) in &p.line.quote_bars {
            let top_y = p.y + p.line.size * 0.85;
            let bot_y = p.y - p.line.size * 0.20;
            acc.entry(id)
                .and_modify(|e| e.2 = bot_y)
                .or_insert((bar_x, top_y, bot_y));
        }
    }
    acc
}

/// Stroke one subtle vertical bar per accumulated blockquote, then clear.
fn flush_quote_bars(
    content: &mut String,
    acc: &mut BTreeMap<usize, (f32, f32, f32)>,
    bar: (f32, f32, f32),
) {
    let (br, bg, bb) = bar;
    for (x, top, bot) in acc.values() {
        content.push_str(&format!(
            "{br:.3} {bg:.3} {bb:.3} RG 2.50 w {x:.2} {top:.2} m {x:.2} {bot:.2} l S\n"
        ));
    }
    acc.clear();
}

/// A light-gray rounded-rectangle fill, color-isolated with `q`/`Q` so the fill
/// color never leaks into following text. Built from 4 lines + 4 cubic Beziers
/// (kappa = 0.5523). Returns an empty string for degenerate rectangles.
fn rounded_rect_fill(x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: (f32, f32, f32)) -> String {
    if x1 <= x0 || y1 <= y0 {
        return String::new();
    }
    let r = r.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5).max(0.0);
    let k = r * 0.5523; // circle -> bezier magic constant
    let (rc, gc, bc) = c;
    format!(
        "q {rc:.3} {gc:.3} {bc:.3} rg \
         {xa:.2} {y0:.2} m {xb:.2} {y0:.2} l \
         {br1x:.2} {y0:.2} {x1:.2} {br2y:.2} {x1:.2} {ya:.2} c \
         {x1:.2} {yb:.2} l \
         {x1:.2} {tr1y:.2} {tr2x:.2} {y1:.2} {xb:.2} {y1:.2} c \
         {xa:.2} {y1:.2} l \
         {tl1x:.2} {y1:.2} {x0:.2} {tl2y:.2} {x0:.2} {yb:.2} c \
         {x0:.2} {ya:.2} l \
         {x0:.2} {bl1y:.2} {bl2x:.2} {y0:.2} {xa:.2} {y0:.2} c f Q\n",
        xa = x0 + r,
        xb = x1 - r,
        ya = y0 + r,
        yb = y1 - r,
        br1x = x1 - r + k,
        br2y = y0 + r - k,
        tr1y = y1 - r + k,
        tr2x = x1 - r + k,
        tl1x = x0 + r - k,
        tl2y = y1 - r + k,
        bl1y = y0 + r - k,
        bl2x = x0 + r - k,
    )
}

/// A subset face ready to embed.
struct EmbeddedFace {
    slot: u8,
    bytes: Vec<u8>,
    font: Font,
    /// GPOS pair kerning of the SOURCE face (the subset drops GPOS), keyed by
    /// original glyph ids — used to position glyphs in the content stream.
    kern: Kerning,
    /// GSUB ligatures of the SOURCE face, applied to shape content lines.
    lig: Ligatures,
    /// Source glyph id -> subset (renumbered) glyph id.
    map: BTreeMap<u16, u16>,
    /// Sorted document characters retained in the subset cmap.
    cmap_chars: Vec<char>,
    /// Subset glyph id -> its source characters, for ligature glyphs that no
    /// character maps to (keeps ligated text selectable via ToUnicode).
    lig_uni: BTreeMap<u16, String>,
}

/// Shaped source glyph stream for one exact segment text in one font slot.
struct ShapedRun {
    glyphs: Vec<u16>,
    ligatures: Vec<(u16, String)>,
}

type ShapedRunCache = BTreeMap<u8, BTreeMap<String, ShapedRun>>;

struct PdfStream {
    dict: String,
    bytes: Vec<u8>,
}

fn estimated_pdf_buffer_capacity(
    pages: &[PageContent],
    faces: &[EmbeddedFace],
    images: &[PdfImageData],
    outline_count: usize,
    annot_count: usize,
    mark_count: usize,
    total_objs: usize,
) -> usize {
    let page_stream_bytes = pages.iter().map(|page| page.stream.len()).sum::<usize>();
    let page_mark_bytes = pages
        .iter()
        .flat_map(|page| page.marks.iter())
        .filter_map(|mark| mark.alt.as_ref())
        .map(String::len)
        .sum::<usize>();
    let page_annot_bytes = pages
        .iter()
        .flat_map(|page| page.annots.iter())
        .map(|annot| match &annot.target {
            LinkTarget::Uri(uri) | LinkTarget::Fragment(uri) => uri.len(),
        })
        .sum::<usize>();
    let font_program_bytes = faces.iter().map(|face| face.bytes.len()).sum::<usize>();
    let font_aux_bytes = faces
        .iter()
        .map(|face| {
            face.font.num_glyphs as usize * 8
                + face.cmap_chars.len() * 18
                + face.lig_uni.values().map(String::len).sum::<usize>() * 4
        })
        .sum::<usize>();
    let image_bytes = images
        .iter()
        .map(|image| image.data.len() + image.smask.as_ref().map_or(0, Vec::len))
        .sum::<usize>();

    page_stream_bytes
        .saturating_add(font_program_bytes)
        .saturating_add(font_aux_bytes)
        .saturating_add(image_bytes)
        .saturating_add(page_mark_bytes.saturating_mul(2))
        .saturating_add(page_annot_bytes.saturating_mul(2))
        .saturating_add(total_objs.saturating_mul(192))
        .saturating_add(pages.len().saturating_mul(256))
        .saturating_add(outline_count.saturating_mul(192))
        .saturating_add(annot_count.saturating_mul(192))
        .saturating_add(mark_count.saturating_mul(192))
        .saturating_add(4096)
}

fn append_decimal_usize(out: &mut Vec<u8>, value: usize) {
    let mut buf = [0u8; 20];
    let mut n = value;
    let mut pos = buf.len();
    loop {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[pos..]);
}

fn append_xref_offset(out: &mut Vec<u8>, offset: usize) {
    let mut buf = [0u8; 20];
    let mut n = offset;
    let mut pos = buf.len();
    loop {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let digits = &buf[pos..];
    for _ in digits.len()..10 {
        out.push(b'0');
    }
    out.extend_from_slice(digits);
}

fn append_pdf_object_header(out: &mut Vec<u8>, object_id: usize) {
    append_decimal_usize(out, object_id);
    out.extend_from_slice(b" 0 obj\n");
}

fn append_pdf_object_str(out: &mut Vec<u8>, offsets: &mut [usize], object_id: usize, body: &str) {
    offsets[object_id] = out.len();
    append_pdf_object_header(out, object_id);
    out.extend_from_slice(body.as_bytes());
    out.extend_from_slice(b"\nendobj\n");
}

fn append_xref_in_use_row(out: &mut Vec<u8>, offset: usize) {
    append_xref_offset(out, offset);
    out.extend_from_slice(b" 00000 n \n");
}

fn append_decimal_u64_string(out: &mut String, value: u64) {
    let mut buf = [0u8; 20];
    let mut n = value;
    let mut pos = buf.len();
    loop {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &byte in &buf[pos..] {
        out.push(byte as char);
    }
}

fn append_i32_string(out: &mut String, value: i32) {
    if value < 0 {
        out.push('-');
    }
    append_decimal_u64_string(out, i64::from(value).unsigned_abs());
}

fn append_pdf_num(out: &mut String, value: f32) {
    let finite = if value.is_finite() { value } else { 0.0 };
    let scaled = (f64::from(finite) * 100.0).round() as i64;
    if scaled < 0 || (scaled == 0 && finite.is_sign_negative()) {
        out.push('-');
    }
    let abs = scaled.unsigned_abs();
    append_decimal_u64_string(out, abs / 100);
    let frac = abs % 100;
    if frac == 0 {
        return;
    }
    out.push('.');
    if frac < 10 {
        out.push('0');
        append_decimal_u64_string(out, frac);
    } else if frac % 10 == 0 {
        append_decimal_u64_string(out, frac / 10);
    } else {
        append_decimal_u64_string(out, frac);
    }
}

fn page_stream(stream: &str) -> PdfStream {
    let raw = stream.as_bytes();
    if raw.len() < PAGE_STREAM_COMPRESSION_MIN {
        return PdfStream {
            dict: format!("<< /Length {} >>", raw.len()),
            bytes: raw.to_vec(),
        };
    }

    let compressed = crate::compress::zlib_compress(raw);
    if compressed.len() + 32 < raw.len() {
        PdfStream {
            dict: format!(
                "<< /Length {} /Filter /FlateDecode /DL {} >>",
                compressed.len(),
                raw.len()
            ),
            bytes: compressed,
        }
    } else {
        PdfStream {
            dict: format!("<< /Length {} >>", raw.len()),
            bytes: raw.to_vec(),
        }
    }
}

fn build_pdf(
    pages: &[PageContent],
    outlines: &[OutlineEntry],
    faces: &[EmbeddedFace],
    images: &[PdfImageData],
    opts: &PdfOptions,
    page_geom: PageGeom,
    profiler: &mut PdfProfiler,
) -> Result<Vec<u8>> {
    let build_started = profiler.checkpoint();
    let p = pages.len();
    let nf = faces.len();
    let ni = images.len();
    let title = opts.title.clone().unwrap_or_default();
    let author = opts.author.clone().unwrap_or_default();
    let outline_count = outlines.len();
    let dest_ids: BTreeSet<&str> = outlines.iter().map(|o| o.id.as_str()).collect();
    let dest_by_id: BTreeMap<&str, &OutlineEntry> =
        outlines.iter().map(|o| (o.id.as_str(), o)).collect();

    let mut annot_starts = Vec::with_capacity(p);
    let mut annot_counts = Vec::with_capacity(p);
    let mut total_annots = 0usize;
    for page in pages {
        annot_starts.push(total_annots);
        let count = page
            .annots
            .iter()
            .filter(|annot| annotation_is_resolved(annot, &dest_ids))
            .count();
        annot_counts.push(count);
        total_annots += count;
    }
    let mut total_marks = 0usize;
    for page in pages {
        total_marks += page.marks.len();
    }
    let tagged = total_marks > 0;
    // Assemble the hierarchical structure tree up front so its node count drives
    // object numbering (containers + leaves are all numbered objects).
    let stree = if tagged {
        build_struct_tree(pages, &dest_ids)
    } else {
        StructTree {
            nodes: Vec::new(),
            parent_tree: Vec::new(),
            annot_owner: Vec::new(),
        }
    };
    let struct_node_count = stree.nodes.len();

    // Assign a `/StructParent` key to each OBJR-referenced link annotation. Page
    // content uses keys `0..p` (the `/StructParents` on page objects), so
    // annotation keys start at `p` to stay unique. `annot_struct_parent[page]
    // [local]` is `Some(key)` for an annotation with an owning element;
    // `struct_annot_nums` collects the parent-tree back-references `key -> owner`.
    let mut annot_struct_parent: Vec<Vec<Option<usize>>> = Vec::with_capacity(p);
    let mut struct_annot_nums: Vec<(usize, usize)> = Vec::new();
    let mut next_struct_key = p;
    for page_owners in &stree.annot_owner {
        let mut keys = Vec::with_capacity(page_owners.len());
        for owner in page_owners {
            match owner {
                Some(node) => {
                    let key = next_struct_key;
                    next_struct_key += 1;
                    keys.push(Some(key));
                    struct_annot_nums.push((key, *node));
                }
                None => keys.push(None),
            }
        }
        annot_struct_parent.push(keys);
    }
    let parent_tree_next_key = next_struct_key;

    // Object number plan (1-indexed):
    //   1 Catalog, 2 Pages, [3..3+p) Page objs, [3+p..3+2p) content streams,
    //   then per face k: type0, cidfont, descriptor, fontfile, tounicode (5),
    //   then image XObjects, link annotations, optional outline root/items, optional structure
    //   root/parent-tree/elements, then Info.
    let page_obj = |i: usize| 3 + i;
    let content_obj = |i: usize| 3 + p + i;
    let face_base = 3 + 2 * p;
    let type0_obj = |k: usize| face_base + 5 * k;
    let cid_obj = |k: usize| face_base + 5 * k + 1;
    let desc_obj = |k: usize| face_base + 5 * k + 2;
    let file_obj = |k: usize| face_base + 5 * k + 3;
    let touni_obj = |k: usize| face_base + 5 * k + 4;
    let image_base = face_base + 5 * nf;
    let image_obj = |k: usize| image_base + k;
    let annot_base = image_base + ni;
    let annot_obj = |page_index: usize, local_index: usize| {
        annot_base + annot_starts.get(page_index).copied().unwrap_or(0) + local_index
    };
    let outline_base = annot_base + total_annots;
    let outline_root_obj = outline_base;
    let outline_item_obj = |i: usize| outline_base + 1 + i;
    let struct_base = outline_base
        + if outline_count == 0 {
            0
        } else {
            1 + outline_count
        };
    let struct_root_obj = struct_base;
    let parent_tree_obj = struct_base + 1;
    let struct_elem_base = struct_base + 2;
    let struct_elem_obj = |node_index: usize| struct_elem_base + node_index;
    let info_obj = struct_base + if tagged { 2 + struct_node_count } else { 0 };
    // Soft-mask XObjects for images carrying alpha are numbered AFTER Info, so
    // adding them never renumbers any object above. `smask_for_image[k]` is the
    // object number of image k's `/SMask`, or `None` when it has no alpha.
    let smask_for_image: Vec<Option<usize>> = {
        let mut next = info_obj + 1;
        images
            .iter()
            .map(|image| {
                if image.smask.is_some() {
                    let n = next;
                    next += 1;
                    Some(n)
                } else {
                    None
                }
            })
            .collect()
    };
    let n_smask = smask_for_image.iter().filter(|s| s.is_some()).count();
    let total_objs = info_obj + n_smask;

    let outline_root_ref = if outline_count == 0 {
        String::new()
    } else {
        format!(" /Outlines {outline_root_obj} 0 R /PageMode /UseOutlines")
    };
    let structure_root_ref = if tagged {
        format!(" /MarkInfo << /Marked true >> /StructTreeRoot {struct_root_obj} 0 R /Lang (en-US)")
    } else {
        String::new()
    };

    let pdf_buffer_capacity = estimated_pdf_buffer_capacity(
        pages,
        faces,
        images,
        outline_count,
        total_annots,
        total_marks,
        total_objs,
    );
    profiler.record_since(
        "pdf_buffer_preallocation",
        total_objs,
        pdf_buffer_capacity,
        "pre-size final PDF byte buffer from page streams, embedded assets, and object counts",
        None,
    );
    let mut buf: Vec<u8> = Vec::with_capacity(pdf_buffer_capacity);
    let mut offsets: Vec<usize> = vec![0; total_objs + 1];

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    append_pdf_object_str(
        &mut buf,
        &mut offsets,
        1,
        &format!("<< /Type /Catalog /Pages 2 0 R{outline_root_ref}{structure_root_ref} >>"),
    );

    let kids = (0..p)
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect::<Vec<_>>()
        .join(" ");
    append_pdf_object_str(
        &mut buf,
        &mut offsets,
        2,
        &format!("<< /Type /Pages /Count {p} /Kids [ {kids} ] >>"),
    );

    // Shared font resource dict referencing every embedded face's Type0 object.
    let font_res = faces
        .iter()
        .enumerate()
        .map(|(k, f)| format!("/F{} {} 0 R", f.slot, type0_obj(k)))
        .collect::<Vec<_>>()
        .join(" ");
    let image_res = if images.is_empty() {
        String::new()
    } else {
        let refs = images
            .iter()
            .enumerate()
            .map(|(k, _)| format!("/{} {} 0 R", image_name(k), image_obj(k)))
            .collect::<Vec<_>>()
            .join(" ");
        format!(" /XObject << {refs} >>")
    };
    let media_w = pdf_num(page_geom.width);
    let media_h = pdf_num(page_geom.height);
    for i in 0..p {
        let annots = if annot_counts.get(i).copied().unwrap_or(0) == 0 {
            String::new()
        } else {
            let refs = (0..annot_counts[i])
                .map(|j| format!("{} 0 R", annot_obj(i, j)))
                .collect::<Vec<_>>()
                .join(" ");
            format!(" /Annots [ {refs} ]")
        };
        let struct_parent = if tagged && !pages[i].marks.is_empty() {
            format!(" /StructParents {i} /Tabs /S")
        } else {
            String::new()
        };
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            page_obj(i),
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {media_w} {media_h}] \
                 /Resources << /Font << {font_res} >>{image_res} >> /Contents {c} 0 R{annots}{struct_parent} >>",
                c = content_obj(i),
            ),
        );
    }

    for (i, page) in pages.iter().enumerate() {
        offsets[content_obj(i)] = buf.len();
        let stream = profiler.measure(
            "page_stream_compression",
            page.stream.len(),
            "encode page content stream and apply FlateDecode when it wins",
            || page_stream(&page.stream),
            |stream| stream.bytes.len(),
        );
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n{dict}\nstream\n",
                n = content_obj(i),
                dict = stream.dict,
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&stream.bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
    }

    // Embedded font object groups.
    for (k, face) in faces.iter().enumerate() {
        let psname = subset_psname(k, face.slot);
        let m = FaceMetrics::of(&face.font);
        let widths = profiler.measure(
            "widths_array_generation",
            face.font.num_glyphs as usize,
            "write composite-font CID width table for the subset face",
            || widths_array(&face.font),
            |widths| widths.len(),
        );

        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            type0_obj(k),
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{psname} /Encoding /Identity-H \
                 /DescendantFonts [{cid} 0 R] /ToUnicode {tu} 0 R >>",
                cid = cid_obj(k),
                tu = touni_obj(k),
            ),
        );
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            cid_obj(k),
            &format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{psname} \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                 /FontDescriptor {desc} 0 R /CIDToGIDMap /Identity /DW 1000 /W [{w}] >>",
                desc = desc_obj(k),
                w = widths,
            ),
        );
        let italic_angle = if matches!(face.slot, F_ITALIC | F_BOLDITALIC) {
            -12
        } else {
            0
        };
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            desc_obj(k),
            &format!(
                "<< /Type /FontDescriptor /FontName /{psname} /Flags 4 \
                 /FontBBox [{bx0} {by0} {bx1} {by1}] /ItalicAngle {italic_angle} \
                 /Ascent {asc} /Descent {desc} /CapHeight {cap} /StemV 80 /FontFile2 {ff} 0 R >>",
                bx0 = -200,
                by0 = m.descent - 50,
                bx1 = 1100,
                by1 = m.ascent + 50,
                asc = m.ascent,
                desc = m.descent,
                cap = m.cap_height,
                ff = file_obj(k),
            ),
        );
        // FontFile2: FlateDecode-compressed subset font program. /Length1 is the
        // UNCOMPRESSED program length per the PDF spec.
        offsets[file_obj(k)] = buf.len();
        let font_comp = profiler.measure(
            "font_stream_compression",
            face.bytes.len(),
            "Flate-compress one embedded subset font program",
            || crate::compress::zlib_compress(&face.bytes),
            |bytes| bytes.len(),
        );
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {clen} /Length1 {olen} /Filter /FlateDecode >>\nstream\n",
                n = file_obj(k),
                clen = font_comp.len(),
                olen = face.bytes.len(),
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&font_comp);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        // ToUnicode CMap (left uncompressed so it stays greppable + tiny).
        let cmap = profiler.measure(
            "tounicode_generation",
            face.cmap_chars.len() + face.lig_uni.len(),
            "generate selectable-text ToUnicode CMap for one subset face",
            || tounicode_cmap(&face.font, &face.cmap_chars, &face.lig_uni),
            |cmap| cmap.len(),
        );
        offsets[touni_obj(k)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} >>\nstream\n{cmap}endstream\nendobj\n",
                n = touni_obj(k),
                len = cmap.len(),
            )
            .as_bytes(),
        );
    }

    for (k, image) in images.iter().enumerate() {
        offsets[image_obj(k)] = buf.len();
        let colors = image.color.components();
        let color_space = image.color.color_space();
        // The zero-decode fast path embeds the raw PNG IDAT and runs the PNG
        // adaptive predictor; the full-decode path embeds our own zlib of the
        // unfiltered samples and needs no predictor.
        let decode_parms = if image.png_predictor {
            format!(
                " /DecodeParms << /Predictor 15 /Colors {colors} /BitsPerComponent 8 /Columns {w} >>",
                w = image.width_px,
            )
        } else {
            String::new()
        };
        let smask_ref = match smask_for_image.get(k).copied().flatten() {
            Some(n) => format!(" /SMask {n} 0 R"),
            None => String::new(),
        };
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
                 /ColorSpace {color_space} /BitsPerComponent 8 /Filter /FlateDecode\
                 {decode_parms}{smask_ref} /Length {len} >>\nstream\n",
                n = image_obj(k),
                w = image.width_px,
                h = image.height_px,
                len = image.data.len(),
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&image.data);
        buf.extend_from_slice(b"\nendstream\nendobj\n");

        // The soft-mask XObject (8-bit grayscale alpha), if this image has one.
        if let (Some(smask_obj), Some(alpha)) = (
            smask_for_image.get(k).copied().flatten(),
            image.smask.as_ref(),
        ) {
            offsets[smask_obj] = buf.len();
            buf.extend_from_slice(
                format!(
                    "{smask_obj} 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
                     /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode \
                     /Length {len} >>\nstream\n",
                    w = image.width_px,
                    h = image.height_px,
                    len = alpha.len(),
                )
                .as_bytes(),
            );
            buf.extend_from_slice(alpha);
            buf.extend_from_slice(b"\nendstream\nendobj\n");
        }
    }

    for (page_index, page) in pages.iter().enumerate() {
        for (local_index, annot) in page
            .annots
            .iter()
            .filter(|annot| annotation_is_resolved(annot, &dest_ids))
            .enumerate()
        {
            let struct_parent = annot_struct_parent
                .get(page_index)
                .and_then(|keys| keys.get(local_index).copied().flatten());
            let body = annotation_dict(annot, &dest_by_id, page_obj, struct_parent);
            append_pdf_object_str(
                &mut buf,
                &mut offsets,
                annot_obj(page_index, local_index),
                &body,
            );
        }
    }

    if outline_count > 0 {
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            outline_root_obj,
            &format!(
                "<< /Type /Outlines /First {first} 0 R /Last {last} 0 R /Count {count} >>",
                first = outline_item_obj(0),
                last = outline_item_obj(outline_count - 1),
                count = outline_count,
            ),
        );
        for (i, outline) in outlines.iter().enumerate() {
            let prev = if i == 0 {
                String::new()
            } else {
                format!(" /Prev {} 0 R", outline_item_obj(i - 1))
            };
            let next = if i + 1 == outline_count {
                String::new()
            } else {
                format!(" /Next {} 0 R", outline_item_obj(i + 1))
            };
            append_pdf_object_str(
                &mut buf,
                &mut offsets,
                outline_item_obj(i),
                &format!(
                    "<< /Title ({title}) /Parent {parent} 0 R{prev}{next} \
                     /Dest [{page} 0 R /XYZ null {y} null] >>",
                    title = pdf_escape(&outline.title),
                    parent = outline_root_obj,
                    page = page_obj(outline.page_index),
                    y = pdf_num(outline.y),
                ),
            );
        }
    }

    if tagged {
        // StructTreeRoot points at the single /Document node (node 0).
        // /ParentTreeNextKey is one past the highest key used in /Nums.
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            struct_root_obj,
            &format!(
                "<< /Type /StructTreeRoot /K [ {doc} 0 R ] /ParentTree {parent_tree_obj} 0 R \
                 /ParentTreeNextKey {parent_tree_next_key} >>",
                doc = struct_elem_obj(0),
            ),
        );

        // Parent tree: page keys (0..p) map their MCIDs in order to the owning
        // element; annotation keys (p..) map a single OBJR-referenced link
        // annotation back to its owning /Link element. Keys stay sorted because
        // every page key is < p <= every annotation key.
        let mut nums = String::new();
        for (page_index, leaf_for_mcid) in stree.parent_tree.iter().enumerate() {
            if leaf_for_mcid.is_empty() {
                continue;
            }
            let refs = leaf_for_mcid
                .iter()
                .map(|&node| format!("{} 0 R", struct_elem_obj(node)))
                .collect::<Vec<_>>()
                .join(" ");
            nums.push_str(&format!("{page_index} [ {refs} ] "));
        }
        for (key, owner_node) in &struct_annot_nums {
            nums.push_str(&format!("{key} {} 0 R ", struct_elem_obj(*owner_node)));
        }
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            parent_tree_obj,
            &format!("<< /Nums [ {nums}] >>"),
        );

        // Serialize each structure element.
        for (i, node) in stree.nodes.iter().enumerate() {
            let parent_ref = if i == 0 {
                struct_root_obj
            } else {
                struct_elem_obj(node.parent)
            };
            let kids = node
                .kids
                .iter()
                .map(|kid| match kid {
                    SKid::Node(n) => format!("{} 0 R", struct_elem_obj(*n)),
                    SKid::Mcr { page, mcid } => format!(
                        "<< /Type /MCR /Pg {pg} 0 R /MCID {mcid} >>",
                        pg = page_obj(*page),
                    ),
                    SKid::ObjR { page, local } => format!(
                        "<< /Type /OBJR /Pg {pg} 0 R /Obj {obj} 0 R >>",
                        pg = page_obj(*page),
                        obj = annot_obj(*page, *local),
                    ),
                })
                .collect::<Vec<_>>()
                .join(" ");
            let k_entry = if node.kids.is_empty() {
                String::new()
            } else {
                format!(" /K [ {kids} ]")
            };
            let pg = node
                .page
                .map(|page| format!(" /Pg {} 0 R", page_obj(page)))
                .unwrap_or_default();
            let alt = node
                .alt
                .as_ref()
                .filter(|alt| !alt.is_empty())
                .map(|alt| format!(" /Alt ({})", pdf_escape(alt)))
                .unwrap_or_default();
            // Table header cells advertise a column scope; figures carry a layout
            // bounding box so assistive tech can locate the image region.
            let attr = if node.scope_column {
                " /A << /O /Table /Scope /Column >>".to_string()
            } else if let Some(b) = node.bbox {
                format!(
                    " /A << /O /Layout /BBox [ {x0} {y0} {x1} {y1} ] >>",
                    x0 = pdf_num(b[0]),
                    y0 = pdf_num(b[1]),
                    x1 = pdf_num(b[2]),
                    y1 = pdf_num(b[3]),
                )
            } else {
                String::new()
            };
            append_pdf_object_str(
                &mut buf,
                &mut offsets,
                struct_elem_obj(i),
                &format!(
                    "<< /Type /StructElem /S /{tag} /P {parent_ref} 0 R{pg}{k_entry}{alt}{attr} >>",
                    tag = node.tag,
                ),
            );
        }
    }

    let title_entry = if title.is_empty() {
        String::new()
    } else {
        format!(" /Title ({})", pdf_escape(&title))
    };
    let author_entry = if author.is_empty() {
        String::new()
    } else {
        format!(" /Author ({})", pdf_escape(&author))
    };
    let info_date = pdf_info_date(opts.metadata_epoch_seconds);
    append_pdf_object_str(
        &mut buf,
        &mut offsets,
        info_obj,
        &format!(
            "<< /Producer (franken_markdown) /Creator (fmd) \
             /CreationDate ({info_date}) /ModDate ({info_date})\
             {title_entry}{author_entry} >>"
        ),
    );

    if offsets.iter().skip(1).any(|&offset| offset == 0) {
        return Err(RenderError::PdfGeneration(
            "internal: a PDF object was left unwritten (zero xref offset)",
        ));
    }

    // xref + trailer.
    let xref_started = profiler.checkpoint();
    let xref_pos = buf.len();
    let size = total_objs + 1;
    buf.extend_from_slice(b"xref\n0 ");
    append_decimal_usize(&mut buf, size);
    buf.extend_from_slice(b"\n0000000000 65535 f \n");
    for offset in offsets.iter().take(total_objs + 1).skip(1) {
        append_xref_in_use_row(&mut buf, *offset);
    }
    buf.extend_from_slice(b"trailer\n<< /Size ");
    append_decimal_usize(&mut buf, size);
    buf.extend_from_slice(b" /Root 1 0 R /Info ");
    append_decimal_usize(&mut buf, info_obj);
    buf.extend_from_slice(b" 0 R >>\nstartxref\n");
    append_decimal_usize(&mut buf, xref_pos);
    buf.extend_from_slice(b"\n%%EOF\n");
    profiler.record_since(
        "xref_trailer_writing",
        total_objs + 1,
        buf.len().saturating_sub(xref_pos),
        "write classic xref table and trailer",
        xref_started,
    );
    profiler.record_since(
        "pdf_object_serialization_total",
        total_objs,
        buf.len(),
        "serialize all PDF objects, streams, xref, and trailer",
        build_started,
    );
    Ok(buf)
}

fn pdf_info_date(epoch_seconds: Option<u64>) -> String {
    const MAX_PDF_DATE_EPOCH: u64 = 253_402_300_799; // 9999-12-31T23:59:59Z
    let epoch = epoch_seconds.unwrap_or(0).min(MAX_PDF_DATE_EPOCH);
    let days = (epoch / 86_400) as i64;
    let secs = epoch % 86_400;
    let (year, month, day) = civil_from_unix_days(days);
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    format!("D:{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}Z")
}

fn civil_from_unix_days(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's civil-from-days algorithm. It is tiny, deterministic,
    // proleptic-Gregorian, and avoids pulling in a time crate for one Info date.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

fn annotation_is_resolved(annot: &LinkAnnotation, dest_ids: &BTreeSet<&str>) -> bool {
    match &annot.target {
        LinkTarget::Uri(uri) => !uri.is_empty(),
        LinkTarget::Fragment(id) => dest_ids.contains(id.as_str()),
    }
}

fn annotation_dict(
    annot: &LinkAnnotation,
    dest_by_id: &BTreeMap<&str, &OutlineEntry>,
    page_obj: impl Fn(usize) -> usize,
    struct_parent: Option<usize>,
) -> String {
    let rect = format!(
        "[{} {} {} {}]",
        pdf_num(annot.rect.x0),
        pdf_num(annot.rect.y0),
        pdf_num(annot.rect.x1),
        pdf_num(annot.rect.y1),
    );
    // The reverse of the owning /Link element's /OBJR: maps this annotation back
    // through the parent tree to its structure element (required for tagged
    // links, PDF/UA).
    let sp = struct_parent
        .map(|key| format!(" /StructParent {key}"))
        .unwrap_or_default();
    match &annot.target {
        LinkTarget::Uri(uri) => format!(
            "<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0] \
             /A << /S /URI /URI ({uri}) >>{sp} >>",
            uri = pdf_escape(uri),
        ),
        LinkTarget::Fragment(id) => {
            let Some(dest) = dest_by_id.get(id.as_str()) else {
                return format!(
                    "<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0]{sp} >>"
                );
            };
            format!(
                "<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0] \
                 /Dest [{page} 0 R /XYZ null {y} null]{sp} >>",
                page = page_obj(dest.page_index),
                y = pdf_num(dest.y),
            )
        }
    }
}

/// FontDescriptor metrics in 1/1000 em.
struct FaceMetrics {
    ascent: i32,
    descent: i32,
    cap_height: i32,
}

impl FaceMetrics {
    fn of(font: &Font) -> Self {
        let upm = font.units_per_em.max(1) as i32;
        let scale = |v: i32| v * 1000 / upm;
        Self {
            ascent: scale(font.ascent as i32),
            descent: scale(font.descent as i32),
            cap_height: scale((font.ascent as i32 * 7) / 10),
        }
    }
}

/// `/W` widths array `[ 0 [w0 w1 ...] ]` (1/1000 em, indexed by glyph id = CID).
fn widths_array(font: &Font) -> String {
    let upm = font.units_per_em.max(1) as u32;
    let glyph_count = font.num_glyphs as usize;
    let mut s = String::with_capacity(4 + glyph_count.saturating_mul(6));
    s.push_str("0 [");
    for gid in 0..font.num_glyphs {
        let w = font.advance_width(gid) as u32 * 1000 / upm;
        append_decimal_u64_string(&mut s, u64::from(w));
        s.push(' ');
    }
    s.push(']');
    s
}

/// Shape `text` with `source`'s ligatures, returning the shaped SOURCE glyph ids
/// and, for each emitted ligature, its source characters (so a `ToUnicode` entry
/// can keep the ligated text selectable).
fn shape_run(source: &Font, lig: &Ligatures, text: &str) -> ShapedRun {
    let chars: Vec<char> = text.chars().collect();
    let gids: Vec<u16> = chars.iter().map(|&c| source.glyph_index(c)).collect();
    let mut shaped = Vec::with_capacity(gids.len());
    let mut lig_uni = Vec::new();
    let mut ci = 0;
    for (gid, count) in lig.substitute_with_spans(&gids) {
        shaped.push(gid);
        if count > 1 {
            let s: String = chars.get(ci..ci + count).unwrap_or(&[]).iter().collect();
            lig_uni.push((gid, s));
        }
        ci += count;
    }
    ShapedRun {
        glyphs: shaped,
        ligatures: lig_uni,
    }
}

/// Build a `TJ` array (without the trailing `TJ`) from a pre-shaped SOURCE glyph
/// sequence: each glyph is emitted as its subset id via `map`, with GPOS pair
/// kerning (looked up on the original ids) inserted between glyphs.
fn kerned_tj(map: &BTreeMap<u16, u16>, source: &Font, kern: &Kerning, shaped: &[u16]) -> String {
    let upm = i32::from(source.units_per_em.max(1));
    let mut out = String::from("[<");
    for (i, &g) in shaped.iter().enumerate() {
        append_hex_u16(&mut out, map.get(&g).copied().unwrap_or(0));
        if let Some(&next) = shaped.get(i + 1) {
            let k = kern.pair(g, next);
            if k != 0 {
                // A TJ number shifts the next glyph left by number/1000 em, so a
                // tightening (negative) kern becomes a positive number.
                let adj = -(i32::from(k) * 1000 / upm);
                out.push('>');
                append_i32_string(&mut out, adj);
                out.push('<');
            }
        }
    }
    out.push_str(">]");
    out
}

/// A `ToUnicode` CMap mapping each glyph id back to its character(s), so text
/// stays selectable. Only the glyphs the document uses appear.
fn tounicode_cmap(font: &Font, cmap_chars: &[char], lig_uni: &BTreeMap<u16, String>) -> String {
    // (gid, UTF-16BE hex) over the chars known to be present in the subset cmap,
    // plus ligature glyphs (which no single character maps to) so ligated text
    // stays selectable. This avoids scanning broad Unicode ranges for every
    // embedded face.
    let mut entries: Vec<(u16, String)> =
        Vec::with_capacity(cmap_chars.len().saturating_add(lig_uni.len()));
    for &c in cmap_chars {
        let g = font.glyph_index(c);
        if g != 0 {
            entries.push((g, utf16be_hex(c)));
        }
    }
    for (g, s) in lig_uni {
        let mut hex = String::with_capacity(s.len().saturating_mul(4));
        for c in s.chars() {
            append_utf16be_hex(c, &mut hex);
        }
        entries.push((*g, hex));
    }
    entries.sort_by_key(|&(g, _)| g);
    entries.dedup_by_key(|(g, _)| *g);

    let mut body = String::with_capacity(entries.len().saturating_mul(18).saturating_add(64));
    for chunk in entries.chunks(100) {
        let _ = writeln!(&mut body, "{} beginbfchar", chunk.len());
        for (g, hex) in chunk {
            let _ = writeln!(&mut body, "<{g:04X}> <{hex}>");
        }
        body.push_str("endbfchar\n");
    }
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {body}endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

fn utf16be_hex(c: char) -> String {
    let mut s = String::with_capacity(8);
    append_utf16be_hex(c, &mut s);
    s
}

fn append_utf16be_hex(c: char, out: &mut String) {
    let mut buf = [0u16; 2];
    for u in c.encode_utf16(&mut buf) {
        append_hex_u16(out, *u);
    }
}

fn append_hex_u16(out: &mut String, value: u16) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(HEX[((value >> 12) & 0xF) as usize] as char);
    out.push(HEX[((value >> 8) & 0xF) as usize] as char);
    out.push(HEX[((value >> 4) & 0xF) as usize] as char);
    out.push(HEX[(value & 0xF) as usize] as char);
}

/// Deterministic subset PostScript name, e.g. `FMDFA1+Embedded`.
fn subset_psname(k: usize, slot: u8) -> String {
    let tag: String = (0..6)
        .map(|i| (b'A' + ((k as u8 + slot + i as u8) % 26)) as char)
        .collect();
    format!("{tag}+Embedded")
}

// ---- text helpers -----------------------------------------------------------

fn safe_pdf_link(url: &str) -> Option<LinkTarget> {
    let trimmed = url.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    if trimmed.is_empty() {
        return None;
    }
    if let Some(fragment) = trimmed.strip_prefix('#') {
        let id = fragment.trim_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
        return valid_pdf_fragment(id).then(|| LinkTarget::Fragment(id.to_string()));
    }
    match pdf_url_scheme(trimmed) {
        PdfUrlScheme::None => Some(LinkTarget::Uri(trimmed.to_string())),
        PdfUrlScheme::Scheme(scheme) if allowed_pdf_url_scheme(&scheme) => {
            Some(LinkTarget::Uri(trimmed.to_string()))
        }
        PdfUrlScheme::Scheme(_) | PdfUrlScheme::Suspicious => None,
    }
}

enum PdfUrlScheme {
    None,
    Scheme(String),
    Suspicious,
}

fn pdf_url_scheme(url: &str) -> PdfUrlScheme {
    let mut scheme = String::new();
    let mut skipped_gap = false;
    for ch in url.chars() {
        if matches!(ch, '/' | '?' | '#') {
            return PdfUrlScheme::None;
        }
        if ch == ':' {
            if skipped_gap || !valid_pdf_url_scheme(&scheme) {
                return PdfUrlScheme::Suspicious;
            }
            return PdfUrlScheme::Scheme(scheme.to_ascii_lowercase());
        }
        if ch.is_ascii_whitespace() || ch.is_control() {
            skipped_gap = true;
            continue;
        }
        scheme.push(ch);
    }
    PdfUrlScheme::None
}

fn valid_pdf_url_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn allowed_pdf_url_scheme(scheme: &str) -> bool {
    matches!(scheme, "http" | "https" | "mailto" | "tel")
}

fn valid_pdf_fragment(fragment: &str) -> bool {
    !fragment.is_empty() && !fragment.chars().any(|c| c.is_ascii_control())
}

fn slug(text: &str) -> String {
    let mut s = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            s.push('-');
        }
    }
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    s.trim_matches('-').to_string()
}

fn pdf_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 4);
    append_pdf_string_escaped(&mut o, s);
    o
}

fn append_pdf_string_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push(' '),
            c if (c as u32) < 256 => out.push(c),
            _ => out.push('?'),
        }
    }
}

fn pdf_num(value: f32) -> String {
    let mut s = String::new();
    append_pdf_num(&mut s, value);
    s
}

fn text_width(s: &str, size: f32, font: u8, faces: &Faces) -> f32 {
    s.chars().map(|c| faces.advance(font, c)).sum::<f32>() * size / 1000.0
}

fn char_width(ch: char, size: f32, font: u8, faces: &Faces) -> f32 {
    faces.advance(font, ch) * size / 1000.0
}

#[cfg(test)]
mod pdf_writer_tests {
    use super::{
        append_decimal_u64_string, append_decimal_usize, append_hex_u16, append_i32_string,
        append_pdf_num, append_pdf_object_str, append_pdf_string_escaped, append_xref_in_use_row,
        append_xref_offset,
    };

    #[test]
    fn decimal_writer_covers_boundary_values() {
        let mut out = Vec::new();
        append_decimal_usize(&mut out, 0);
        out.push(b' ');
        append_decimal_usize(&mut out, 7);
        out.push(b' ');
        append_decimal_usize(&mut out, 42);
        out.push(b' ');
        append_decimal_usize(&mut out, 9_876_543_210);

        assert_eq!(out, b"0 7 42 9876543210");
    }

    #[test]
    fn xref_writer_uses_classic_ten_digit_padding() {
        let mut out = Vec::new();
        append_xref_offset(&mut out, 0);
        out.push(b'\n');
        append_xref_offset(&mut out, 42);
        out.push(b'\n');
        append_xref_offset(&mut out, 1_234_567_890);
        out.push(b'\n');
        append_xref_offset(&mut out, 10_000_000_000);

        assert_eq!(out, b"0000000000\n0000000042\n1234567890\n10000000000");
    }

    #[test]
    fn xref_row_writer_preserves_pdf_spacing() {
        let mut out = Vec::new();
        append_xref_in_use_row(&mut out, 123);

        assert_eq!(out, b"0000000123 00000 n \n");
    }

    #[test]
    fn object_writer_records_offset_and_envelope_exactly() {
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = [0usize; 3];
        append_pdf_object_str(&mut out, &mut offsets, 2, "<< /Type /Example >>");

        assert_eq!(offsets[2], b"%PDF-1.7\n".len());
        assert_eq!(out, b"%PDF-1.7\n2 0 obj\n<< /Type /Example >>\nendobj\n");
    }

    #[test]
    fn glyph_hex_writer_is_uppercase_and_zero_padded() {
        let mut out = String::new();
        append_hex_u16(&mut out, 0);
        out.push(' ');
        append_hex_u16(&mut out, 0x00AF);
        out.push(' ');
        append_hex_u16(&mut out, 0xBEEF);

        assert_eq!(out, "0000 00AF BEEF");
    }

    #[test]
    fn string_decimal_and_signed_integer_writers_cover_boundaries() {
        let mut out = String::new();
        append_decimal_u64_string(&mut out, 0);
        out.push(' ');
        append_decimal_u64_string(&mut out, 12_345_678_901);
        out.push(' ');
        append_i32_string(&mut out, -120);
        out.push(' ');
        append_i32_string(&mut out, 0);
        out.push(' ');
        append_i32_string(&mut out, 456);

        assert_eq!(out, "0 12345678901 -120 0 456");
    }

    #[test]
    fn fixed_precision_pdf_number_writer_rounds_and_trims_like_pdf_points() {
        let mut out = String::new();
        for value in [0.0, 1.0, 1.2, 1.23, 1.235, -2.5, -2.345] {
            if !out.is_empty() {
                out.push(' ');
            }
            append_pdf_num(&mut out, value);
        }

        assert_eq!(out, "0 1 1.2 1.23 1.24 -2.5 -2.35");
    }

    #[test]
    fn fixed_precision_pdf_number_writer_matches_legacy_format_policy() {
        fn legacy_pdf_num(value: f32) -> String {
            let mut s = format!("{value:.2}");
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
            if s.is_empty() { "0".to_string() } else { s }
        }

        for value in [
            -1200.0, -25.5, -2.345, -0.25, -0.004, 0.0, 0.004, 0.005, 0.25, 1.234, 1.235, 12.0,
            9999.995,
        ] {
            let mut out = String::new();
            append_pdf_num(&mut out, value);
            assert_eq!(out, legacy_pdf_num(value), "value {value}");
        }
    }

    #[test]
    fn pdf_literal_string_escape_writer_matches_existing_policy() {
        let mut out = String::new();
        append_pdf_string_escaped(&mut out, "a(b)c\\d\re\n\u{2206}");

        assert_eq!(out, "a\\(b\\)c\\\\d\\re ?");
    }
}

/// Deterministic render-tree goldens (bead qw1.1.2). Each fixture is rendered
/// through the real layout/pagination pipeline and its render tree is pinned to
/// a committed golden under `tests/golden/render_tree/`. A code change that
/// shifts a baseline, moves an x-position, drifts a text color, or alters the
/// structure produces a golden diff — the appearance-regression signal that
/// byte determinism alone cannot give.
///
/// Pinned on Linux (the CI quality runner) so f32 layout values are compared on
/// one platform; cross-OS byte stability is gated separately by
/// `scripts/check-determinism.sh`. Regenerate after an intentional change with
/// `UPDATE_RENDER_TREE=1 cargo test render_tree`.
#[cfg(all(test, target_os = "linux"))]
mod render_tree_golden_tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;
    use crate::theme::{PageMargins, PageSize, Theme};
    use std::path::PathBuf;

    fn small_opts(width_pt: f32, height_pt: f32) -> PdfOptions {
        let mut theme = Theme::default();
        theme.page.size = PageSize {
            name: "render-tree-test",
            width_pt,
            height_pt,
        };
        theme.page.margins = PageMargins {
            top_pt: 24.0,
            right_pt: 24.0,
            bottom_pt: 24.0,
            left_pt: 24.0,
        };
        PdfOptions {
            theme,
            ..PdfOptions::default()
        }
    }

    fn golden_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/render_tree")
            .join(format!("{name}.txt"))
    }

    /// (name, markdown, page width, page height). Pages are kept narrow so prose
    /// wraps, hyphenation fires, and multi-page pagination is exercised.
    fn cases() -> Vec<(&'static str, String, f32, f32)> {
        vec![
            (
                "prose",
                "# Heading One\n\n## Heading Two\n\nA paragraph of body text long \
                 enough that it wraps across several measured lines under a narrow \
                 column, exercising baselines and the line breaker.\n\n\
                 Another paragraph with *emphasis*, **strong**, `code`, and a \
                 [link](https://example.com/docs) inline.\n"
                    .to_string(),
                320.0,
                400.0,
            ),
            (
                "lists",
                "- first item\n- second item\n  - nested bullet\n  - nested two\n\
                 - third item\n\n1. ordered one\n2. ordered two\n\n- [ ] todo\n\
                 - [x] done\n"
                    .to_string(),
                320.0,
                400.0,
            ),
            (
                "table",
                "| Name | Qty | Price |\n|:---|---:|:--:|\n| alpha | 1 | 9.99 |\n\
                 | beta | 22 | 12.00 |\n| gamma | 333 | 7.50 |\n"
                    .to_string(),
                360.0,
                400.0,
            ),
            (
                "quote-code",
                "> A quoted paragraph that is long enough to wrap inside the quote \
                 gutter at this measure.\n>\n> - quoted list item\n\n\
                 ```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n\
                 Body with ~~struck~~ text after the code block.\n"
                    .to_string(),
                320.0,
                400.0,
            ),
            (
                "hyphenation",
                "Internationalization and antidisestablishmentarianism are \
                 representative hyphenation candidates in a deliberately narrow \
                 measure.\n"
                    .to_string(),
                150.0,
                300.0,
            ),
        ]
    }

    #[test]
    fn render_tree_goldens_match() {
        let update = std::env::var("UPDATE_RENDER_TREE").is_ok();
        let mut mismatches = Vec::new();
        for (name, md, w, h) in cases() {
            let tree = render_tree_debug(&md, &small_opts(w, h));
            let path = golden_path(name);
            if update {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).expect("create golden dir");
                }
                std::fs::write(&path, &tree).expect("write golden");
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(expected) if expected == tree => {}
                Ok(_expected) => {
                    mismatches.push(format!("{name}: render tree differs from golden"))
                }
                Err(e) => mismatches.push(format!("{name}: cannot read golden {path:?}: {e}")),
            }
        }
        assert!(
            mismatches.is_empty(),
            "render-tree golden mismatch ({}). If the change is intentional, \
             regenerate with `UPDATE_RENDER_TREE=1 cargo test render_tree` and \
             review the diff.\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
    }

    #[test]
    fn render_tree_is_stable_across_repeated_renders() {
        // The render tree must itself be deterministic (a prerequisite for the
        // golden to mean anything).
        let opts = small_opts(320.0, 400.0);
        let md = cases()[0].1.clone();
        assert_eq!(render_tree_debug(&md, &opts), render_tree_debug(&md, &opts));
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::needless_range_loop,
    clippy::too_many_arguments
)]
mod png_decode_tests {
    use super::{
        DecodedPng, PNG_ADAM7, PdfImageColor, PngChunks, decode_png_full, parse_png_image_asset,
        png_pass_count,
    };
    use crate::compress::zlib_compress;

    /// Build a `PngChunks` from raw (unfiltered, filter-type-0) scanlines.
    fn chunks(
        width: u32,
        height: u32,
        bit_depth: u8,
        color_type: u8,
        interlace: u8,
        rows: &[Vec<u8>],
        palette: Vec<[u8; 3]>,
        trns: Vec<u8>,
    ) -> PngChunks {
        let mut raw = Vec::new();
        for r in rows {
            raw.push(0u8); // filter type 0
            raw.extend_from_slice(r);
        }
        PngChunks {
            width,
            height,
            bit_depth,
            color_type,
            interlace,
            palette,
            trns,
            idat: zlib_compress(&raw),
        }
    }

    #[test]
    fn rgba8_splits_color_and_alpha_into_smask() {
        // 2x2 RGBA with distinct values per channel.
        let rows = vec![
            vec![10, 20, 30, 255, 40, 50, 60, 128],
            vec![70, 80, 90, 0, 100, 110, 120, 200],
        ];
        let png = chunks(2, 2, 8, 6, 0, &rows, vec![], vec![]);
        let DecodedPng {
            color,
            samples,
            alpha,
            ..
        } = decode_png_full(&png).unwrap();
        assert_eq!(color, PdfImageColor::Rgb);
        assert_eq!(
            samples,
            vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120]
        );
        assert_eq!(alpha, Some(vec![255, 128, 0, 200]));
    }

    #[test]
    fn palette_with_trns_expands_to_rgb_plus_alpha() {
        let palette = vec![[255, 0, 0], [0, 255, 0], [0, 0, 255], [9, 9, 9]];
        let trns = vec![0, 128, 255]; // alpha for first 3 entries; 4th -> opaque
        let rows = vec![vec![0, 1, 3]];
        let png = chunks(3, 1, 8, 3, 0, &rows, palette, trns);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.color, PdfImageColor::Rgb);
        assert_eq!(d.samples, vec![255, 0, 0, 0, 255, 0, 9, 9, 9]);
        assert_eq!(d.alpha, Some(vec![0, 128, 255]));
    }

    #[test]
    fn gray_alpha8_keeps_gray_color_space() {
        let rows = vec![vec![50, 255, 200, 128]];
        let png = chunks(2, 1, 8, 4, 0, &rows, vec![], vec![]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.color, PdfImageColor::Gray);
        assert_eq!(d.samples, vec![50, 200]);
        assert_eq!(d.alpha, Some(vec![255, 128]));
    }

    #[test]
    fn rgb16_downsamples_to_high_byte() {
        // 2 pixels, each channel 16-bit big-endian; keep the high byte.
        let rows = vec![vec![
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x11, 0x22, 0x22,
        ]];
        let png = chunks(2, 1, 16, 2, 0, &rows, vec![], vec![]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.samples, vec![0x12, 0x56, 0x9a, 0xde, 0x11, 0x22]);
        assert_eq!(d.alpha, None);
    }

    #[test]
    fn gray4_unpacks_and_scales_subbyte_samples() {
        // Two 4-bit gray samples packed in one byte: 0x0F -> (15, 0) -> (255, 0).
        let rows = vec![vec![0xF0]];
        let png = chunks(2, 1, 4, 0, 0, &rows, vec![], vec![]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.color, PdfImageColor::Gray);
        assert_eq!(d.samples, vec![255, 0]);
        assert_eq!(d.alpha, None);
    }

    #[test]
    fn adam7_interlaced_rgb_deinterlaces_exactly() {
        // 5x5 RGB image; build the 7-pass raw stream the way an encoder would.
        let (w, h) = (5u32, 5u32);
        let img: Vec<Vec<[u8; 3]>> = (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| [(x * 40) as u8, (y * 40) as u8, ((x + y) * 20 % 256) as u8])
                    .collect()
            })
            .collect();
        let mut rows: Vec<Vec<u8>> = Vec::new();
        for &(xs, xstep, ys, ystep) in &PNG_ADAM7 {
            let pw = png_pass_count(w, xs, xstep);
            let ph = png_pass_count(h, ys, ystep);
            if pw == 0 || ph == 0 {
                continue;
            }
            for r in 0..ph {
                let yy = (ys + r * ystep) as usize;
                let mut line = Vec::new();
                for c in 0..pw {
                    let xx = (xs + c * xstep) as usize;
                    line.extend_from_slice(&img[yy][xx]);
                }
                rows.push(line);
            }
        }
        let png = chunks(w, h, 8, 2, 1, &rows, vec![], vec![]);
        let d = decode_png_full(&png).unwrap();
        let mut expect = Vec::new();
        for y in 0..h as usize {
            for x in 0..w as usize {
                expect.extend_from_slice(&img[y][x]);
            }
        }
        assert_eq!(d.samples, expect);
    }

    #[test]
    fn fast_path_8bit_rgb_passes_idat_through_with_predictor() {
        // An 8-bit RGB non-interlaced PNG keeps the zero-decode predictor path.
        let rows = vec![vec![1, 2, 3, 4, 5, 6]];
        let png = chunks(2, 1, 8, 2, 0, &rows, vec![], vec![]);
        // Re-assemble a full PNG byte stream to exercise parse_png_image_asset.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        let push_chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&0u32.to_be_bytes());
        };
        push_chunk(&mut bytes, b"IHDR", &ihdr);
        push_chunk(&mut bytes, b"IDAT", &png.idat);
        push_chunk(&mut bytes, b"IEND", &[]);
        let data = parse_png_image_asset("k", &bytes).unwrap();
        assert!(
            data.png_predictor,
            "8-bit RGB stays on the predictor fast path"
        );
        assert!(data.smask.is_none());
        assert_eq!(data.color, PdfImageColor::Rgb);
    }
}
