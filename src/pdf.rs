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
    FORCED_BREAK_PENALTY, FontSize, Glue, HyphenationOptions, Hyphenator, LayoutUnit, LineBreak,
    ParagraphItem, ParagraphLayoutScratch, Penalty, TextBox, adjustment_to_layout_units,
    advance_to_layout_units, break_paragraph_into, default_interword_glue, is_breakable_whitespace,
};
use crate::text::{Font, Kerning, Ligatures};
use crate::theme::{Theme, ThemeColors};
use crate::{FontAssetSlot, FontAssets, PdfOptions, RenderError};
use std::borrow::Cow;
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
const CODE_FONT_SIZE: f32 = 9.5;
const CODE_DIAGRAM_MIN_FONT_SIZE: f32 = 6.0;
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
const TABLE_FONT_SIZE: f32 = 10.0;
const TABLE_COL_GUTTER: f32 = 14.0;
const TABLE_MIN_COL_WIDTH: f32 = 18.0;
const TABLE_ALLOC_MIN_UNIT_PT: f32 = 0.5;
const TABLE_ALLOC_MAX_EXTRA_STATES: usize = 900;
const TABLE_HEADER_WRAP_WEIGHT: f32 = 10.0;
const TABLE_BODY_WRAP_WEIGHT: f32 = 1.0;

const PDF_IMAGE_DPI_SCALE: f32 = 72.0 / 96.0;
const MAX_PDF_IMAGE_COMPRESSED_BYTES: usize = 32 * 1024 * 1024;
const MAX_PDF_IMAGE_PIXELS: u64 = 24_000_000;
const MAX_SVG_PATH_OPS: usize = 4096;
const MAX_SVG_ACCESSIBLE_TEXT_CHARS: usize = 512;
/// Ceiling on a PNG's *decoded* raw sample bytes (`pixels * channels * bytes/sample`).
///
/// The pixel cap alone is blind to bit depth, so a 24-megapixel 16-bit RGBA image
/// (a few-KB IDAT of zlib-compressed zeros) would drive a ~380 MB transient
/// allocation — fine on a desktop, but a memory trap on a constrained
/// browser/worker wasm instance. This bit-depth-aware cap bounds the largest
/// decode buffer (the RGBA8/16 raw samples) uniformly on every target, so output
/// stays deterministic across native and wasm. 8-bit RGBA up to the pixel cap
/// still fits (~91.5 MiB); 16-bit RGBA above ~12 MP is refused (→ alt text).
const MAX_PDF_IMAGE_DECODED_BYTES: u64 = 96 * 1024 * 1024;

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
    /// Shaped layout advance, used to size decorations and subsequent segments.
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
    vector: Option<PdfSvgImage>,
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

#[derive(Clone)]
struct PdfSvgImage {
    view_box: SvgViewBox,
    viewport: SvgViewport,
    preserve_aspect: SvgPreserveAspectRatio,
    root_background: Option<SvgRootBackground>,
    accessible_text: Option<String>,
    elements: Vec<SvgElement>,
    gradients: Vec<SvgGradientPaint>,
    patterns: Vec<SvgPatternPaint>,
    clip_paths: Vec<SvgClipPath>,
    markers: Vec<SvgMarker>,
}

#[derive(Clone, Copy)]
struct SvgViewBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
struct SvgViewport {
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
struct SvgRootGeometry {
    view_box: SvgViewBox,
    viewport: SvgViewport,
    preserve_aspect: SvgPreserveAspectRatio,
}

#[derive(Clone, Copy)]
struct SvgPreserveAspectRatio {
    mode: SvgAspectScaleMode,
    align_x: f32,
    align_y: f32,
}

impl SvgPreserveAspectRatio {
    const DEFAULT: Self = Self {
        mode: SvgAspectScaleMode::Meet,
        align_x: 0.5,
        align_y: 0.5,
    };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgAspectScaleMode {
    None,
    Meet,
    Slice,
}

#[derive(Clone, Copy, PartialEq)]
struct SvgTransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl SvgTransform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn translate(tx: f32, ty: f32) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    fn rotate_degrees(angle: f32) -> Self {
        let (sin, cos) = angle.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    fn skew_x_degrees(angle: f32) -> Self {
        Self {
            c: angle.to_radians().tan(),
            ..Self::IDENTITY
        }
    }

    fn skew_y_degrees(angle: f32) -> Self {
        Self {
            b: angle.to_radians().tan(),
            ..Self::IDENTITY
        }
    }

    fn concat(self, next: Self) -> Self {
        Self {
            a: self.a * next.a + self.c * next.b,
            b: self.b * next.a + self.d * next.b,
            c: self.a * next.c + self.c * next.d,
            d: self.b * next.c + self.d * next.d,
            e: self.a * next.e + self.c * next.f + self.e,
            f: self.b * next.e + self.d * next.f + self.f,
        }
    }

    fn is_identity(self) -> bool {
        const EPSILON: f32 = 0.000_01;
        (self.a - 1.0).abs() <= EPSILON
            && self.b.abs() <= EPSILON
            && self.c.abs() <= EPSILON
            && (self.d - 1.0).abs() <= EPSILON
            && self.e.abs() <= EPSILON
            && self.f.abs() <= EPSILON
    }

    fn apply_point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

#[derive(Clone)]
enum SvgElement {
    Rect(SvgRect),
    Ellipse(SvgEllipse),
    Line(SvgLine),
    Polyline(SvgPoly),
    Polygon(SvgPoly),
    Path(SvgPath),
    Image(SvgEmbeddedImage),
    Text(SvgText),
}

#[derive(Clone)]
struct SvgRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    ry: f32,
    style: SvgStyle,
    link: Option<LinkTarget>,
}

#[derive(Clone)]
struct SvgEllipse {
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    style: SvgStyle,
    link: Option<LinkTarget>,
}

#[derive(Clone)]
struct SvgLine {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    style: SvgStyle,
    marker_end: Option<SvgMarkerRef>,
    marker_start: Option<SvgMarkerRef>,
    link: Option<LinkTarget>,
}

#[derive(Clone)]
struct SvgPoly {
    points: Vec<(f32, f32)>,
    style: SvgStyle,
    marker_end: Option<SvgMarkerRef>,
    marker_mid: Option<SvgMarkerRef>,
    marker_start: Option<SvgMarkerRef>,
    link: Option<LinkTarget>,
}

#[derive(Clone)]
struct SvgPath {
    ops: Vec<SvgPathOp>,
    style: SvgStyle,
    marker_end: Option<SvgMarkerRef>,
    marker_mid: Option<SvgMarkerRef>,
    marker_start: Option<SvgMarkerRef>,
    link: Option<LinkTarget>,
}

#[derive(Clone)]
struct SvgEmbeddedImage {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    preserve_aspect: SvgPreserveAspectRatio,
    style: SvgStyle,
    image: Box<PdfImageData>,
    link: Option<LinkTarget>,
}

#[derive(Clone, Copy)]
enum SvgPathOp {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Quad(f32, f32, f32, f32),
    Close,
}

#[derive(Clone)]
struct SvgText {
    x: f32,
    y: f32,
    text: String,
    slot: u8,
    font_size: f32,
    letter_spacing: f32,
    text_length: Option<f32>,
    length_adjust: SvgLengthAdjust,
    baseline: SvgDominantBaseline,
    anchor: SvgTextAnchor,
    decoration: SvgTextDecoration,
    fill: (f32, f32, f32),
    opacity: f32,
    clip_path: Option<usize>,
    mask_path: Option<usize>,
    transform: SvgTransform,
    link: Option<LinkTarget>,
}

#[derive(Clone, Copy)]
struct SvgTextMatrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    x: f32,
    y: f32,
    size: f32,
}

#[derive(Clone, Copy)]
enum SvgTextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgLengthAdjust {
    Spacing,
    SpacingAndGlyphs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SvgTextDecoration {
    bits: u8,
}

impl SvgTextDecoration {
    const NONE: Self = Self { bits: 0 };
    const UNDERLINE: u8 = 1 << 0;
    const OVERLINE: u8 = 1 << 1;
    const LINE_THROUGH: u8 = 1 << 2;

    const fn with(self, bit: u8) -> Self {
        Self {
            bits: self.bits | bit,
        }
    }

    const fn contains(self, bit: u8) -> bool {
        self.bits & bit != 0
    }

    const fn is_empty(self) -> bool {
        self.bits == 0
    }
}

#[derive(Clone, Copy)]
struct SvgTextPlacement {
    x: f32,
    y: f32,
    font_size: f32,
    anchor: SvgTextAnchor,
    text_length: Option<f32>,
    length_adjust: SvgLengthAdjust,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgFontWeight {
    Normal,
    Bold,
}

impl SvgFontWeight {
    const fn is_bold(self) -> bool {
        matches!(self, Self::Bold)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgFontSlant {
    Normal,
    Italic,
}

impl SvgFontSlant {
    const fn is_italic(self) -> bool {
        matches!(self, Self::Italic)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgFontFamily {
    Body,
    Mono,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgDominantBaseline {
    Auto,
    Middle,
    Hanging,
    TextBeforeEdge,
    TextAfterEdge,
}

impl SvgDominantBaseline {
    const fn y_shift_em(self) -> f32 {
        match self {
            Self::Auto => 0.0,
            Self::Middle => 0.35,
            Self::Hanging | Self::TextBeforeEdge => 0.8,
            Self::TextAfterEdge => -0.2,
        }
    }
}

#[derive(Clone, Copy)]
enum SvgTextSpacing {
    Points(f32),
    Em(f32),
    Ex(f32),
    Percent(f32),
}

impl SvgTextSpacing {
    const ZERO: Self = Self::Points(0.0);

    fn to_points(self, font_size: f32) -> f32 {
        let value = match self {
            Self::Points(value) => value,
            Self::Em(value) => value * font_size,
            Self::Ex(value) => value * font_size * 0.5,
            Self::Percent(value) => value * font_size / 100.0,
        };
        if value.is_finite() { value } else { 0.0 }
    }
}

#[derive(Clone, Copy)]
struct SvgDashPattern {
    values: [f32; 16],
    len: u8,
    offset: f32,
}

impl SvgDashPattern {
    const NONE: Self = Self {
        values: [0.0; 16],
        len: 0,
        offset: 0.0,
    };

    fn is_empty(self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Copy)]
enum SvgLineCap {
    Butt,
    Round,
    Square,
}

impl SvgLineCap {
    const fn pdf_id(self) -> u8 {
        match self {
            Self::Butt => 0,
            Self::Round => 1,
            Self::Square => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgLineJoin {
    Miter,
    Round,
    Bevel,
}

impl SvgLineJoin {
    const fn pdf_id(self) -> u8 {
        match self {
            Self::Miter => 0,
            Self::Round => 1,
            Self::Bevel => 2,
        }
    }
}

#[derive(Clone, Copy)]
enum SvgFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgPaintLayer {
    Fill,
    Stroke,
    Markers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SvgPaintOrder {
    layers: [SvgPaintLayer; 3],
}

impl SvgPaintOrder {
    const NORMAL: Self = Self {
        layers: [
            SvgPaintLayer::Fill,
            SvgPaintLayer::Stroke,
            SvgPaintLayer::Markers,
        ],
    };
}

type SvgColor = (f32, f32, f32);
type SvgGradientStop = (f32, SvgColor);

#[derive(Clone, Copy)]
struct SvgParsedColor {
    rgb: SvgColor,
    alpha: Option<f32>,
}

#[derive(Clone)]
struct SvgRootBackground {
    color: Option<SvgRootBackgroundColor>,
    opacity: f32,
    layers: Vec<SvgRootBackgroundLayer>,
}

impl SvgRootBackground {
    fn with_opacity(mut self, opacity: f32) -> Option<Self> {
        let opacity = (self.opacity * opacity).clamp(0.0, 1.0);
        self.opacity = opacity;
        (self.is_visible()).then_some(self)
    }

    fn is_visible(&self) -> bool {
        self.opacity > 0.001 && (self.color.is_some() || !self.layers.is_empty())
    }
}

#[derive(Clone, Copy)]
struct SvgRootBackgroundColor {
    color: SvgColor,
    opacity: f32,
}

#[derive(Clone)]
enum SvgRootBackgroundLayer {
    Linear(SvgCssLinearGradient),
    Radial(SvgCssRadialGradient),
}

#[derive(Clone)]
struct SvgCssLinearGradient {
    start: (f32, f32),
    end: (f32, f32),
    stops: Vec<SvgGradientStop>,
}

#[derive(Clone)]
struct SvgCssRadialGradient {
    center: (f32, f32),
    stops: Vec<SvgGradientStop>,
}

#[derive(Clone, Copy)]
struct SvgShadow {
    dx: f32,
    dy: f32,
    color: SvgColor,
    opacity: f32,
}

impl SvgShadow {
    const FALLBACK: Self = Self {
        dx: 2.0,
        dy: 2.0,
        color: (0.890, 0.900, 0.920),
        opacity: 1.0,
    };
}

#[derive(Clone)]
struct SvgFilterShadow {
    id: String,
    shadow: SvgShadow,
}

#[derive(Clone, Copy)]
struct SvgStyle {
    color: SvgColor,
    fill: Option<(f32, f32, f32)>,
    fill_gradient: Option<usize>,
    fill_pattern: Option<usize>,
    fill_current_color: bool,
    fill_context: Option<SvgContextPaint>,
    stroke: Option<(f32, f32, f32)>,
    stroke_gradient: Option<usize>,
    stroke_current_color: bool,
    stroke_context: Option<SvgContextPaint>,
    stroke_width: f32,
    non_scaling_stroke: bool,
    opacity: f32,
    fill_opacity: f32,
    stroke_opacity: f32,
    display_visible: bool,
    visibility_visible: bool,
    visible: bool,
    shadow: Option<SvgShadow>,
    clip_path: Option<usize>,
    mask_path: Option<usize>,
    transform: SvgTransform,
    dash: SvgDashPattern,
    line_cap: SvgLineCap,
    line_join: SvgLineJoin,
    miter_limit: Option<f32>,
    fill_rule: SvgFillRule,
    paint_order: SvgPaintOrder,
    font_size: f32,
    text_anchor: SvgTextAnchor,
    font_weight: SvgFontWeight,
    font_slant: SvgFontSlant,
    font_family: SvgFontFamily,
    dominant_baseline: SvgDominantBaseline,
    letter_spacing: SvgTextSpacing,
    text_decoration: SvgTextDecoration,
}

impl SvgStyle {
    const INITIAL: Self = Self {
        color: (0.0, 0.0, 0.0),
        fill: Some((0.0, 0.0, 0.0)),
        fill_gradient: None,
        fill_pattern: None,
        fill_current_color: false,
        fill_context: None,
        stroke: None,
        stroke_gradient: None,
        stroke_current_color: false,
        stroke_context: None,
        stroke_width: 1.0,
        non_scaling_stroke: false,
        opacity: 1.0,
        fill_opacity: 1.0,
        stroke_opacity: 1.0,
        display_visible: true,
        visibility_visible: true,
        visible: true,
        shadow: None,
        clip_path: None,
        mask_path: None,
        transform: SvgTransform::IDENTITY,
        dash: SvgDashPattern::NONE,
        line_cap: SvgLineCap::Butt,
        line_join: SvgLineJoin::Miter,
        miter_limit: Some(4.0),
        fill_rule: SvgFillRule::NonZero,
        paint_order: SvgPaintOrder::NORMAL,
        font_size: 12.0,
        text_anchor: SvgTextAnchor::Start,
        font_weight: SvgFontWeight::Normal,
        font_slant: SvgFontSlant::Normal,
        font_family: SvgFontFamily::Body,
        dominant_baseline: SvgDominantBaseline::Auto,
        letter_spacing: SvgTextSpacing::ZERO,
        text_decoration: SvgTextDecoration::NONE,
    };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgContextPaint {
    Fill,
    Stroke,
}

#[derive(Clone, Copy, Default)]
struct SvgStylePatch {
    color: Option<SvgColor>,
    fill: Option<Option<(f32, f32, f32)>>,
    fill_gradient: Option<Option<usize>>,
    fill_pattern: Option<Option<usize>>,
    fill_current_color: Option<bool>,
    fill_context: Option<Option<SvgContextPaint>>,
    stroke: Option<Option<(f32, f32, f32)>>,
    stroke_gradient: Option<Option<usize>>,
    stroke_current_color: Option<bool>,
    stroke_context: Option<Option<SvgContextPaint>>,
    stroke_width: Option<f32>,
    non_scaling_stroke: Option<bool>,
    opacity: Option<f32>,
    fill_opacity: Option<f32>,
    stroke_opacity: Option<f32>,
    display_visible: Option<bool>,
    visibility_visible: Option<bool>,
    shadow: Option<Option<SvgShadow>>,
    clip_path: Option<Option<usize>>,
    mask_path: Option<Option<usize>>,
    transform: Option<SvgTransform>,
    dash: Option<SvgDashPattern>,
    dash_offset: Option<f32>,
    line_cap: Option<SvgLineCap>,
    line_join: Option<SvgLineJoin>,
    miter_limit: Option<f32>,
    fill_rule: Option<SvgFillRule>,
    paint_order: Option<SvgPaintOrder>,
    font_size: Option<f32>,
    text_anchor: Option<SvgTextAnchor>,
    font_weight: Option<SvgFontWeight>,
    font_slant: Option<SvgFontSlant>,
    font_family: Option<SvgFontFamily>,
    dominant_baseline: Option<SvgDominantBaseline>,
    letter_spacing: Option<SvgTextSpacing>,
    text_decoration: Option<SvgTextDecoration>,
}

#[derive(Clone)]
struct SvgCssRule {
    selector: SvgCssSelector,
    order: usize,
    decls: String,
}

#[derive(Clone)]
struct SvgGradientStopCssRule {
    selector: SvgCssSelector,
    order: usize,
    patch: SvgGradientStopPatch,
}

#[derive(Clone, Copy, Default)]
struct SvgGradientStopPatch {
    color: Option<SvgColor>,
    opacity: Option<f32>,
}

#[derive(Clone)]
struct SvgCssSelector {
    parts: Vec<SvgCssSelectorPart>,
    specificity: u16,
}

#[derive(Clone)]
struct SvgCssSelectorPart {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    relation: SvgCssRelation,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgCssRelation {
    Descendant,
    Child,
}

#[derive(Clone)]
struct SvgCssAncestor {
    tag: String,
    attrs: Vec<(String, String)>,
}

#[derive(Clone)]
struct SvgGradientPaint {
    id: String,
    color: (f32, f32, f32),
    linear: Option<SvgLinearGradient>,
    radial: Option<SvgRadialGradient>,
}

#[derive(Clone)]
struct SvgPatternPaint {
    id: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    transform: SvgTransform,
    color: SvgColor,
    elements: Vec<SvgElement>,
}

struct SvgPatternDefinition {
    id: String,
    attrs: Vec<(String, String)>,
    body: String,
    href: Option<String>,
}

struct SvgResolvedPattern {
    attrs: Vec<(String, String)>,
    body: String,
}

#[derive(Clone)]
struct SvgGradientDefinition {
    id: String,
    linear: bool,
    attrs: Vec<(String, String)>,
    stops: Vec<SvgGradientStop>,
    href: Option<String>,
}

struct SvgResolvedGradient {
    linear: bool,
    attrs: Vec<(String, String)>,
    stops: Vec<SvgGradientStop>,
}

#[derive(Clone, PartialEq)]
struct SvgLinearGradient {
    units: SvgGradientUnits,
    spread: SvgGradientSpread,
    transform: SvgTransform,
    x1: SvgGradientLength,
    y1: SvgGradientLength,
    x2: SvgGradientLength,
    y2: SvgGradientLength,
    stops: Vec<SvgGradientStop>,
}

#[derive(Clone, PartialEq)]
struct SvgRadialGradient {
    units: SvgGradientUnits,
    spread: SvgGradientSpread,
    transform: SvgTransform,
    cx: SvgGradientLength,
    cy: SvgGradientLength,
    r: SvgGradientLength,
    fx: SvgGradientLength,
    fy: SvgGradientLength,
    fr: SvgGradientLength,
    stops: Vec<SvgGradientStop>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgGradientUnits {
    ObjectBoundingBox,
    UserSpaceOnUse,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgGradientSpread {
    Pad,
    Repeat,
    Reflect,
}

#[derive(Clone, Copy, PartialEq)]
struct SvgGradientLength {
    value: f32,
    percent: bool,
}

#[derive(Clone, PartialEq)]
struct PdfShading {
    kind: PdfShadingKind,
    stops: Vec<SvgGradientStop>,
    extend_start: bool,
    extend_end: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum PdfShadingKind {
    Axial([f32; 4]),
    Radial([f32; 6]),
}

#[derive(Clone)]
struct SvgCssVariable {
    name: String,
    value: String,
}

#[derive(Clone)]
struct SvgReusableDef {
    id: String,
    tag: String,
    attrs: Vec<(String, String)>,
    body: Option<String>,
    view_box: Option<SvgViewBox>,
    preserve_aspect: SvgPreserveAspectRatio,
}

#[derive(Clone)]
struct SvgMarker {
    id: String,
    ref_x: f32,
    ref_y: f32,
    orient: SvgMarkerOrient,
    view_box: Option<SvgMarkerViewBox>,
    units_stroke_width: bool,
    shapes: Vec<SvgMarkerShape>,
}

#[derive(Clone, Copy)]
struct SvgMarkerViewBox {
    view_box: SvgViewBox,
    viewport: SvgViewport,
    preserve_aspect: SvgPreserveAspectRatio,
}

#[derive(Clone, Copy)]
enum SvgMarkerOrient {
    Angle(f32),
    Auto,
    AutoStartReverse,
}

#[derive(Clone, Copy)]
enum SvgMarkerPlacement {
    Start,
    Mid,
    End,
}

#[derive(Clone, Copy)]
struct SvgMarkerPaint {
    fill: Option<SvgColor>,
    stroke: Option<SvgColor>,
    stroke_width: f32,
}

#[derive(Clone)]
struct SvgMarkerShape {
    ops: Vec<SvgPathOp>,
    style: SvgStyle,
}

#[derive(Clone, Copy)]
struct SvgMarkerRef {
    index: Option<usize>,
}

#[derive(Clone, Copy, Default)]
struct SvgMarkerRefs {
    start: Option<SvgMarkerRef>,
    mid: Option<SvgMarkerRef>,
    end: Option<SvgMarkerRef>,
}

#[derive(Clone)]
struct SvgClipPath {
    id: String,
    ops: Vec<SvgPathOp>,
    fill_rule: SvgFillRule,
    units: SvgClipPathUnits,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SvgClipPathUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
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

/// One source face plus the OpenType layout tables used by PDF shaping.
struct Face {
    font: Font,
    kern: Kerning,
    lig: Ligatures,
}

impl Face {
    fn load(slot: FontAssetSlot, bytes: &[u8]) -> Result<Self> {
        let font = parse_face(slot, bytes)?;
        let kern = font.gpos_kerning();
        let lig = font.gsub_ligatures();
        Ok(Self { font, kern, lig })
    }

    fn glyph_advance_1000(&self, glyph: u16) -> u32 {
        if self.font.units_per_em == 0 {
            return 0;
        }
        self.font.advance_width(glyph) as u32 * 1000 / self.font.units_per_em as u32
    }

    fn glyph_index(&self, ch: char) -> u16 {
        self.font.glyph_index(ch)
    }

    fn shaped_width(&self, text: &str, size: FontSize) -> LayoutUnit {
        let glyphs: Vec<u16> = text.chars().map(|ch| self.font.glyph_index(ch)).collect();
        let shaped = self.lig.substitute(&glyphs);
        self.shaped_glyph_width(&shaped, size)
    }

    fn shaped_width_points(&self, text: &str, size: f32) -> f32 {
        self.shaped_width(text, font_size_of(size)).to_points_f32()
    }

    fn shaped_glyph_width(&self, glyphs: &[u16], size: FontSize) -> LayoutUnit {
        if self.font.units_per_em == 0 {
            return LayoutUnit::ZERO;
        }
        let upm = i32::from(self.font.units_per_em);
        let mut total = LayoutUnit::ZERO;
        for (idx, &glyph) in glyphs.iter().enumerate() {
            total += advance_to_layout_units(self.glyph_advance_1000(glyph), size);
            if let Some(&next) = glyphs.get(idx + 1) {
                let adjustment_1000 = i32::from(self.kern.pair(glyph, next)) * 1000 / upm;
                total += adjustment_to_layout_units(adjustment_1000, size);
            }
        }
        total.max(LayoutUnit::ZERO)
    }
}

/// The source faces resolved from the theme family + the registry.
struct Faces {
    body: Face,
    bold: Face,
    italic: Face,
    bolditalic: Face,
    mono: Face,
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
            body: Face::load(
                FontAssetSlot::BodyRegular,
                body_font_bytes(&opts.font_assets, fam, FontStyle::Regular),
            )?,
            bold: Face::load(
                FontAssetSlot::BodyBold,
                body_font_bytes(&opts.font_assets, fam, FontStyle::Bold),
            )?,
            italic: Face::load(
                FontAssetSlot::BodyItalic,
                body_font_bytes(&opts.font_assets, fam, FontStyle::Italic),
            )?,
            bolditalic: Face::load(
                FontAssetSlot::BodyBoldItalic,
                body_font_bytes(&opts.font_assets, fam, FontStyle::BoldItalic),
            )?,
            mono: Face::load(
                FontAssetSlot::MonoRegular,
                mono_font_bytes(&opts.font_assets, FontStyle::Regular),
            )?,
        })
    }

    fn face(&self, slot: u8) -> &Face {
        match slot {
            F_BOLD => &self.bold,
            F_ITALIC => &self.italic,
            F_BOLDITALIC => &self.bolditalic,
            F_MONO => &self.mono,
            _ => &self.body,
        }
    }

    fn get(&self, slot: u8) -> &Font {
        &self.face(slot).font
    }

    /// Advance of `c` in 1/1000 em (PDF text space) for the slot's face.
    fn advance(&self, slot: u8, c: char) -> f32 {
        self.get(slot).advance_1000(c) as f32
    }

    fn shaped_width(&self, slot: u8, text: &str, size: FontSize) -> LayoutUnit {
        self.face(slot).shaped_width(text, size)
    }

    fn shaped_width_points(&self, slot: u8, text: &str, size: f32) -> f32 {
        self.face(slot).shaped_width_points(text, size)
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
    item_toks: Vec<TokGroup>,
    break_toks: Vec<Option<Tok>>,
}

impl BuiltParagraph {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            item_toks: Vec::with_capacity(capacity),
            break_toks: Vec::with_capacity(capacity),
        }
    }
}

struct TokGroup {
    first: Option<Tok>,
    rest: Vec<Tok>,
}

impl TokGroup {
    fn empty() -> Self {
        Self {
            first: None,
            rest: Vec::new(),
        }
    }

    fn one(tok: Tok) -> Self {
        Self {
            first: Some(tok),
            rest: Vec::new(),
        }
    }

    fn from_vec(mut toks: Vec<Tok>) -> Self {
        match toks.len() {
            0 => Self::empty(),
            1 => Self {
                first: toks.pop(),
                rest: Vec::new(),
            },
            _ => {
                let rest = toks.split_off(1);
                Self {
                    first: toks.pop(),
                    rest,
                }
            }
        }
    }

    fn take_from(word: &mut Vec<Tok>) -> Self {
        if word.len() == 1 {
            Self {
                first: word.pop(),
                rest: Vec::new(),
            }
        } else {
            Self::from_vec(std::mem::take(word))
        }
    }
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
    width_cache: &'a RefCell<WidthCache>,
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
    /// A supplied image asset could not be decoded (e.g. an unsupported image
    /// format); rendered as alt text instead of being embedded.
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
                "image '{dest}' could not be decoded (unsupported image); rendered as alt text"
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
    let mut image_text = String::new();

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
            Some(asset) => match parse_pdf_image_asset(trimmed, &asset.bytes) {
                Some(image) => collect_svg_image_text(&image, &mut image_text),
                None => {
                    warnings.push(RenderWarning::UnsupportedImage(dest));
                }
            },
        }
    }

    if let Ok(faces) = Faces::load(opts) {
        let mut text = String::new();
        collect_text(&doc.blocks, &mut text);
        text.push_str(&image_text);
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

fn collect_svg_image_text(image: &PdfImageData, out: &mut String) {
    let Some(svg) = image.vector.as_ref() else {
        return;
    };
    for element in &svg.elements {
        if let SvgElement::Text(text) = element {
            out.push_str(&text.text);
        }
    }
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
            Block::HtmlBlock(html) => out.push_str(html),
            Block::ThematicBreak => {}
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
            Inline::Html(html) => out.push_str(html),
            Inline::SoftBreak | Inline::HardBreak => {}
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
        width_cache: RefCell::new(HashMap::new()),
        paragraph_scratch: ParagraphLayoutScratch::new(),
        line_breaks: Vec::new(),
        line_toks: Vec::new(),
        glue_adjustments: Vec::new(),
    };
    layout_blocks(blocks, 0.0, &mut out, &mut cx);
    out
}

/// Bound on the per-document hyphenation cache (distinct lowercase words). Beyond
/// this, further words are still hyphenated but not cached — a fixed cap keeps
/// memory bounded and the cached set deterministic (words are seen in document
/// order), while never changing the hyphenation result.
const HYPHEN_CACHE_MAX: usize = 16_384;
const WIDTH_CACHE_MAX: usize = 32_768;
type WidthCache = HashMap<(u8, u32), HashMap<String, LayoutUnit>>;

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
    /// Per-render shaped-width cache for PDF paragraph layout. Values are the
    /// exact `Faces::shaped_width` result for a font slot, size, and text.
    width_cache: RefCell<WidthCache>,
    /// Reused workspace for the paragraph optimizer. This avoids the allocating
    /// `break_paragraph` wrapper on every PDF paragraph while keeping all state
    /// render-call-local and deterministic.
    paragraph_scratch: ParagraphLayoutScratch,
    line_breaks: Vec<LineBreak>,
    /// Reused physical-line token workspace. Paragraph breaking returns item
    /// ranges, then PDF layout maps those ranges back to styled tokens; this
    /// buffer avoids allocating a new token vector for every emitted line.
    line_toks: Vec<LineTok>,
    /// Reused justification workspace for TeX-style glue stretch/shrink.
    glue_adjustments: Vec<(usize, f32)>,
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

    fn break_paragraph(&mut self, items: &[ParagraphItem], line_width: LayoutUnit) {
        break_paragraph_into(
            items,
            line_width,
            &mut self.paragraph_scratch,
            &mut self.line_breaks,
        );
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
            let code_area_width = (cx.page.content_w - indent - CODE_PAD_X).max(12.0);
            let preserve_lines = preserve_code_block_lines(lang.as_deref(), code);
            let code_size = if preserve_lines {
                fitted_code_font_size(
                    code,
                    code_area_width,
                    cx.opts.code_line_numbers,
                    digits,
                    CODE_FONT_SIZE,
                    CODE_DIAGRAM_MIN_FONT_SIZE,
                    cx.faces,
                )
            } else {
                CODE_FONT_SIZE
            };
            let number_col = if cx.opts.code_line_numbers {
                code_line_number_column_width(digits, code_size, cx.faces)
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
                        max_text_width: (code_area_width - number_col).max(12.0),
                        number_col,
                        size: code_size,
                        preserve_lines,
                        faces: cx.faces,
                    },
                );
                let row_count = rows.len();
                for (row_idx, segs) in rows.into_iter().enumerate() {
                    out.push(Line {
                        size: code_size,
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
                        1,
                        digits,
                        x,
                        number_col,
                        CODE_FONT_SIZE,
                        cx.faces,
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
                    size: CODE_FONT_SIZE,
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
            layout_table(
                table,
                indent,
                cx.faces,
                &cx.width_cache,
                cx.page,
                group,
                out,
            );
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
    parse_pdf_image_asset(key, &asset.bytes)
}

fn parse_pdf_image_asset(key: &str, bytes: &[u8]) -> Option<PdfImageData> {
    parse_png_image_asset(key, bytes).or_else(|| parse_svg_image_asset(key, bytes))
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

    // Fast path: 8-bit grayscale/RGB, non-interlaced, no transparency. Validate
    // that IDAT inflates to legal PNG predictor rows, then embed the original zlib
    // bytes directly and let the PDF reader run the predictor. This avoids
    // unfiltering/re-encoding while still refusing corrupt image payloads.
    if png.bit_depth == 8
        && png.interlace == 0
        && png.trns.is_empty()
        && matches!(png.color_type, 0 | 2)
    {
        let (color, components) = if png.color_type == 0 {
            (PdfImageColor::Gray, 1usize)
        } else {
            (PdfImageColor::Rgb, 3usize)
        };
        if !png_predictor_payload_is_valid(&png, components) {
            return None;
        }
        return Some(PdfImageData {
            key: key.to_string(),
            width_px: png.width,
            height_px: png.height,
            vector: None,
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
        vector: None,
        color: decoded.color,
        data,
        png_predictor: false,
        smask,
    })
}

fn parse_svg_image_asset(key: &str, bytes: &[u8]) -> Option<PdfImageData> {
    if bytes.len() > MAX_PDF_IMAGE_COMPRESSED_BYTES {
        return None;
    }
    let src = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}')
        .trim_start();
    if !svg_has_supported_root(src) {
        return None;
    }
    let vector = parse_svg_document(src)?;
    let w = vector.viewport.w.ceil().max(1.0);
    let h = vector.viewport.h.ceil().max(1.0);
    if !w.is_finite()
        || !h.is_finite()
        || (w as u64).saturating_mul(h as u64) > MAX_PDF_IMAGE_PIXELS
    {
        return None;
    }
    Some(PdfImageData {
        key: key.to_string(),
        width_px: w as u32,
        height_px: h as u32,
        vector: Some(vector),
        color: PdfImageColor::Rgb,
        data: Vec::new(),
        png_predictor: false,
        smask: None,
    })
}

fn svg_has_supported_root(src: &str) -> bool {
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                return false;
            };
            pos = open + end_rel + 7;
            continue;
        }

        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            return false;
        };
        let raw = src[open + 1..open + close_rel].trim();
        pos = open + close_rel + 1;
        if raw.is_empty() || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        if raw.starts_with('/') {
            return false;
        }
        let (tag, _) = svg_tag_parts(raw);
        return svg_local_name(tag).eq_ignore_ascii_case("svg");
    }
    false
}

fn parse_svg_document(src: &str) -> Option<PdfSvgImage> {
    let mut pos = 0usize;
    let mut view_box = None;
    let mut viewport = None;
    let mut preserve_aspect = SvgPreserveAspectRatio::DEFAULT;
    let mut root_background = None;
    let mut elements = Vec::new();
    let mut skip_depth = 0usize;
    let mut style_stack = vec![SvgStyle::INITIAL];
    let mut link_stack: Vec<Option<LinkTarget>> = vec![None];
    let mut selector_stack: Vec<SvgCssAncestor> = Vec::new();
    let css_vars = parse_svg_css_variables(src);
    let gradients = parse_svg_gradient_paints(src, &css_vars);
    let filter_shadows = parse_svg_filter_shadows(src, &css_vars);
    let mut clip_paths = parse_svg_clip_paths(src);
    clip_paths.extend(parse_svg_masks(src, &css_vars));
    let css_rules = parse_svg_document_css_rules(src);
    let patterns = parse_svg_pattern_paints(
        src,
        &gradients,
        &css_vars,
        &clip_paths,
        &css_rules,
        &filter_shadows,
    );
    let markers = parse_svg_markers(
        src,
        &gradients,
        &patterns,
        &css_vars,
        &clip_paths,
        &css_rules,
        &filter_shadows,
    );
    let reusable_refs = parse_svg_use_refs(src);
    let reusable_defs = parse_svg_reusable_defs(src, &reusable_refs);
    let accessible_text = parse_svg_accessible_text(src);

    while let Some(open_rel) = src.get(pos..)?.find('<') {
        let open = pos + open_rel;
        if src.get(open..)?.starts_with("<!--") {
            let end = open + src.get(open + 4..)?.find("-->")? + 7;
            pos = end;
            continue;
        }
        let close = open + src.get(open..)?.find('>')?;
        let raw = src.get(open + 1..close)?.trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }

        let closing = raw.starts_with('/');
        let raw = if closing { raw[1..].trim_start() } else { raw };
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        if tag.is_empty() {
            continue;
        }
        let tag_name_lower = tag.to_ascii_lowercase();
        let tag_lower = svg_local_name(&tag_name_lower);

        if closing {
            if skip_depth > 0 {
                skip_depth = skip_depth.saturating_sub(1);
            } else if matches!(tag_lower, "svg" | "g" | "a") && style_stack.len() > 1 {
                style_stack.pop();
                link_stack.pop();
                selector_stack.pop();
            }
            continue;
        }
        if skip_depth > 0 {
            if !self_closing {
                skip_depth = skip_depth.saturating_add(1);
            }
            continue;
        }
        if matches!(
            tag_lower,
            "defs" | "style" | "script" | "foreignobject" | "iframe" | "object" | "embed"
        ) {
            if !self_closing {
                skip_depth = 1;
            }
            continue;
        }

        let attrs = parse_svg_attrs(attrs_src);
        let inherited = style_stack.last().copied().unwrap_or(SvgStyle::INITIAL);
        let inherited_link = link_stack.last().cloned().unwrap_or(None);
        let container_style = parse_svg_style_with_ancestors(
            tag_lower,
            &attrs,
            inherited,
            &css_rules,
            &gradients,
            &patterns,
            &css_vars,
            &clip_paths,
            &filter_shadows,
            &selector_stack,
        );
        match tag_lower {
            "svg" => {
                let geometry = parse_svg_root_geometry(&attrs)?;
                if view_box.is_none() {
                    view_box = Some(geometry.view_box);
                    viewport = Some(geometry.viewport);
                    preserve_aspect = geometry.preserve_aspect;
                    root_background = if container_style.visible {
                        parse_svg_root_background(&attrs, &css_rules, &css_vars)
                            .and_then(|background| background.with_opacity(container_style.opacity))
                    } else {
                        None
                    };
                }
                if !self_closing {
                    style_stack.push(container_style);
                    link_stack.push(inherited_link);
                    selector_stack.push(svg_css_ancestor(tag_lower, &attrs));
                }
            }
            "g" if !self_closing => {
                style_stack.push(container_style);
                link_stack.push(inherited_link);
                selector_stack.push(svg_css_ancestor(tag_lower, &attrs));
            }
            "a" if !self_closing => {
                style_stack.push(container_style);
                link_stack.push(parse_svg_anchor_link(&attrs).unwrap_or(inherited_link));
                selector_stack.push(svg_css_ancestor(tag_lower, &attrs));
            }
            "rect" => {
                if let Some(rect) = parse_svg_rect(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Rect(rect),
                        inherited_link.as_ref(),
                    );
                }
            }
            "circle" => {
                if let Some(circle) = parse_svg_circle(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Ellipse(circle),
                        inherited_link.as_ref(),
                    );
                }
            }
            "ellipse" => {
                if let Some(ellipse) = parse_svg_ellipse(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Ellipse(ellipse),
                        inherited_link.as_ref(),
                    );
                }
            }
            "line" => {
                if let Some(line) = parse_svg_line(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &markers,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Line(line),
                        inherited_link.as_ref(),
                    );
                }
            }
            "polyline" => {
                if let Some(poly) = parse_svg_poly(
                    &attrs,
                    inherited,
                    false,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &markers,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Polyline(poly),
                        inherited_link.as_ref(),
                    );
                }
            }
            "polygon" => {
                if let Some(poly) = parse_svg_poly(
                    &attrs,
                    inherited,
                    true,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &markers,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Polygon(poly),
                        inherited_link.as_ref(),
                    );
                }
            }
            "path" => {
                if let Some(path) = parse_svg_path(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &markers,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Path(path),
                        inherited_link.as_ref(),
                    );
                }
            }
            "image" => {
                if let Some(image) = parse_svg_embedded_image(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &selector_stack,
                ) {
                    push_svg_element(
                        &mut elements,
                        SvgElement::Image(image),
                        inherited_link.as_ref(),
                    );
                }
            }
            "use" => {
                let mut used = parse_svg_use_elements(
                    &attrs,
                    inherited,
                    &css_rules,
                    &gradients,
                    &patterns,
                    &css_vars,
                    &clip_paths,
                    &filter_shadows,
                    &markers,
                    &reusable_defs,
                    &selector_stack,
                    0,
                );
                apply_svg_links(&mut used, inherited_link.as_ref());
                elements.append(&mut used);
            }
            "text" if !self_closing => {
                let needle = format!("</{tag_name_lower}");
                if let Some(end_rel) = find_ascii_case_insensitive(src.get(pos..)?, &needle) {
                    let text_src = src.get(pos..pos + end_rel).unwrap_or_default();
                    let mut text_elements = parse_svg_text_elements(
                        &attrs,
                        text_src,
                        inherited,
                        &css_rules,
                        &gradients,
                        &patterns,
                        &css_vars,
                        &clip_paths,
                        &filter_shadows,
                        &selector_stack,
                    );
                    apply_svg_links(&mut text_elements, inherited_link.as_ref());
                    elements.extend(text_elements);
                    if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
                        pos += end_rel + tag_end + 1;
                    }
                }
            }
            _ => {}
        }

        if elements.len() > 4096 {
            return None;
        }
    }

    let view_box = view_box?;
    let viewport = viewport.unwrap_or(SvgViewport {
        w: view_box.w,
        h: view_box.h,
    });
    if view_box.w <= 0.0 || view_box.h <= 0.0 {
        return None;
    }
    if elements.is_empty() && root_background.is_none() {
        return None;
    }
    Some(PdfSvgImage {
        view_box,
        viewport,
        preserve_aspect,
        root_background,
        accessible_text,
        elements,
        gradients,
        patterns,
        clip_paths,
        markers,
    })
}

#[derive(Default)]
struct SvgRootBackgroundDecl {
    color: Option<SvgRootBackgroundColor>,
    image: Option<String>,
}

fn parse_svg_root_background(
    attrs: &[(String, String)],
    css_rules: &[SvgCssRule],
    css_vars: &[SvgCssVariable],
) -> Option<SvgRootBackground> {
    let scoped_css_vars = svg_css_vars_for_element(css_vars, css_rules, &[], "svg", attrs);
    let mut decl = SvgRootBackgroundDecl::default();

    apply_svg_root_background_attr(&mut decl, "background", attrs, &scoped_css_vars);
    apply_svg_root_background_attr(&mut decl, "background-color", attrs, &scoped_css_vars);
    apply_svg_root_background_attr(&mut decl, "background-image", attrs, &scoped_css_vars);

    for rule in svg_matching_css_rules("svg", attrs, css_rules, &[]) {
        apply_svg_root_background_decls(&mut decl, &rule.decls, &scoped_css_vars);
    }

    if let Some(style_attr) = svg_attr(attrs, "style") {
        apply_svg_root_background_decls(&mut decl, style_attr, &scoped_css_vars);
    }

    let base_color = decl.color.map_or((1.0, 1.0, 1.0), |color| color.color);
    let layers = decl
        .image
        .as_deref()
        .map(|value| parse_svg_root_background_layers(value, &scoped_css_vars, base_color))
        .unwrap_or_default();
    let background = SvgRootBackground {
        color: decl.color,
        opacity: 1.0,
        layers,
    };
    background.is_visible().then_some(background)
}

fn apply_svg_root_background_attr(
    decl: &mut SvgRootBackgroundDecl,
    name: &str,
    attrs: &[(String, String)],
    css_vars: &[SvgCssVariable],
) {
    let Some(value) = svg_attr(attrs, name) else {
        return;
    };
    apply_svg_root_background_decl(decl, name, value, css_vars);
}

fn apply_svg_root_background_decls(
    decl: &mut SvgRootBackgroundDecl,
    decls: &str,
    css_vars: &[SvgCssVariable],
) {
    for declaration in decls.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        if !matches!(
            name.as_str(),
            "background" | "background-color" | "background-image"
        ) {
            continue;
        }
        apply_svg_root_background_decl(decl, &name, value, css_vars);
    }
}

fn apply_svg_root_background_decl(
    decl: &mut SvgRootBackgroundDecl,
    name: &str,
    value: &str,
    css_vars: &[SvgCssVariable],
) {
    let name = name.trim().to_ascii_lowercase();
    match name.as_str() {
        "background" => {
            let color = parse_svg_background_color_value(value, css_vars);
            let image = parse_svg_root_background_image_value(value, css_vars);
            if color.is_some() || image.is_some() {
                decl.color = color.unwrap_or(None);
                decl.image = Some(image.unwrap_or_else(|| "none".to_string()));
            }
        }
        "background-color" => {
            if let Some(parsed) = parse_svg_background_color_value(value, css_vars) {
                decl.color = parsed;
            }
        }
        "background-image" => {
            decl.image = Some(clean_svg_css_keyword_value(value).to_string());
        }
        _ => {}
    }
}

fn parse_svg_background_color_value(
    value: &str,
    css_vars: &[SvgCssVariable],
) -> Option<Option<SvgRootBackgroundColor>> {
    let value = clean_svg_css_keyword_value(value);
    if value.is_empty() {
        return None;
    }
    if let Some(color) = parse_svg_background_color_token(value, css_vars) {
        return Some(color);
    }
    for token in svg_background_top_level_tokens(value) {
        if let Some(color) = parse_svg_background_color_token(token, css_vars) {
            return Some(color);
        }
    }
    None
}

fn parse_svg_background_color_token(
    value: &str,
    css_vars: &[SvgCssVariable],
) -> Option<Option<SvgRootBackgroundColor>> {
    let value = clean_svg_css_keyword_value(value.trim_matches(','));
    if value.is_empty() {
        return None;
    }
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_background_color_token(&resolved, css_vars);
    }
    if value.eq_ignore_ascii_case("transparent") || value.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    let opacity = if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
        if alpha <= 0.001 {
            return Some(None);
        }
        alpha.clamp(0.0, 1.0)
    } else {
        1.0
    };
    parse_svg_color(value, css_vars).map(|color| Some(SvgRootBackgroundColor { color, opacity }))
}

fn svg_background_top_level_tokens(value: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    let mut quote = None;

    for (idx, ch) in value.char_indices() {
        if let Some(quote_ch) = quote {
            if ch == quote_ch {
                quote = None;
            }
            continue;
        }

        let delimiter = depth == 0 && (ch == ',' || ch.is_ascii_whitespace());
        if delimiter {
            if let Some(token_start) = start.take() {
                if token_start < idx {
                    tokens.push(value[token_start..idx].trim());
                }
            }
            continue;
        }

        if start.is_none() {
            start = Some(idx);
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => depth = depth.saturating_add(1),
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    if let Some(token_start) = start
        && token_start < value.len()
    {
        tokens.push(value[token_start..].trim());
    }
    tokens
}

fn parse_svg_root_background_image_value(
    value: &str,
    css_vars: &[SvgCssVariable],
) -> Option<String> {
    let value = clean_svg_css_keyword_value(value);
    if value.is_empty() {
        return None;
    }
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_root_background_image_value(&resolved, css_vars);
    }
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return Some("none".to_string());
    }

    let mut saw_image = false;
    let mut layers = Vec::new();
    for layer in split_svg_css_top_level_commas(value).into_iter().take(8) {
        if let Some(gradient) = svg_background_layer_gradient_token(layer) {
            saw_image = true;
            layers.push(gradient);
        } else if svg_background_layer_has_url_token(layer) {
            saw_image = true;
        }
    }
    if layers.is_empty() {
        saw_image.then(|| "none".to_string())
    } else {
        Some(layers.join(", "))
    }
}

fn svg_background_layer_gradient_token(layer: &str) -> Option<String> {
    split_svg_top_level_whitespace(layer)
        .into_iter()
        .find(|token| {
            svg_css_function_args(token, "linear-gradient").is_some()
                || svg_css_function_args(token, "radial-gradient").is_some()
        })
        .map(str::to_string)
}

fn svg_background_layer_has_url_token(layer: &str) -> bool {
    split_svg_top_level_whitespace(layer)
        .into_iter()
        .any(|token| svg_css_function_args(token, "url").is_some())
}

fn parse_svg_root_background_layers(
    value: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
) -> Vec<SvgRootBackgroundLayer> {
    let value = clean_svg_css_keyword_value(value);
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return Vec::new();
    }
    if value.starts_with("var(") {
        if let Some(resolved) = resolve_svg_css_value(value, css_vars, 0) {
            return parse_svg_root_background_layers(&resolved, css_vars, base_color);
        }
        return Vec::new();
    }
    split_svg_css_top_level_commas(value)
        .into_iter()
        .take(8)
        .filter_map(|layer| parse_svg_root_background_layer(layer, css_vars, base_color))
        .collect()
}

fn parse_svg_root_background_layer(
    value: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
) -> Option<SvgRootBackgroundLayer> {
    let value = clean_svg_css_keyword_value(value);
    if let Some(args) = svg_css_function_args(value, "linear-gradient") {
        parse_svg_css_linear_gradient(args, css_vars, base_color)
            .map(SvgRootBackgroundLayer::Linear)
    } else if let Some(args) = svg_css_function_args(value, "radial-gradient") {
        parse_svg_css_radial_gradient(args, css_vars, base_color)
            .map(SvgRootBackgroundLayer::Radial)
    } else {
        None
    }
}

fn parse_svg_css_linear_gradient(
    args: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
) -> Option<SvgCssLinearGradient> {
    let parts = split_svg_css_top_level_commas(args);
    if parts.len() < 2 {
        return None;
    }
    let ((start, end), stop_parts) =
        if let Some((start, end)) = parse_svg_css_linear_gradient_direction(parts[0]) {
            ((start, end), &parts[1..])
        } else {
            (svg_css_gradient_line_from_vector(0.0, -1.0)?, &parts[..])
        };
    let stops = parse_svg_css_gradient_stops(stop_parts, css_vars, base_color)?;
    Some(SvgCssLinearGradient { start, end, stops })
}

fn parse_svg_css_radial_gradient(
    args: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
) -> Option<SvgCssRadialGradient> {
    let parts = split_svg_css_top_level_commas(args);
    if parts.len() < 2 {
        return None;
    }
    let (center, stop_parts) =
        if let Some(center) = parse_svg_css_radial_gradient_descriptor(parts[0]) {
            (center, &parts[1..])
        } else {
            ((0.5, 0.5), &parts[..])
        };
    let stops = parse_svg_css_gradient_stops(stop_parts, css_vars, base_color)?;
    Some(SvgCssRadialGradient { center, stops })
}

fn parse_svg_css_linear_gradient_direction(value: &str) -> Option<((f32, f32), (f32, f32))> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("to ") {
        let mut vx = 0.0f32;
        let mut vy = 0.0f32;
        for word in rest.split_ascii_whitespace() {
            match word {
                "left" => vx = -1.0,
                "right" => vx = 1.0,
                "top" => vy = 1.0,
                "bottom" => vy = -1.0,
                _ => return None,
            }
        }
        return svg_css_gradient_line_from_vector(vx, vy);
    }
    let angle = lower
        .strip_suffix("deg")
        .and_then(|number| number.trim().parse::<f32>().ok())?;
    if !angle.is_finite() {
        return None;
    }
    let radians = angle.to_radians();
    svg_css_gradient_line_from_vector(radians.sin(), radians.cos())
}

fn svg_css_gradient_line_from_vector(vx: f32, vy: f32) -> Option<((f32, f32), (f32, f32))> {
    if ![vx, vy].iter().all(|value| value.is_finite()) {
        return None;
    }
    let len = (vx.abs() + vy.abs()) * 0.5;
    (len > 0.001).then_some((
        (0.5 - vx * len, 0.5 - vy * len),
        (0.5 + vx * len, 0.5 + vy * len),
    ))
}

fn parse_svg_css_radial_gradient_descriptor(value: &str) -> Option<(f32, f32)> {
    let lower = value.trim().to_ascii_lowercase();
    if let Some((_, position)) = lower.split_once(" at ") {
        return parse_svg_css_background_position(position);
    }
    matches!(
        lower.as_str(),
        "circle"
            | "ellipse"
            | "closest-side"
            | "closest-corner"
            | "farthest-side"
            | "farthest-corner"
            | "circle closest-side"
            | "circle closest-corner"
            | "circle farthest-side"
            | "circle farthest-corner"
            | "ellipse closest-side"
            | "ellipse closest-corner"
            | "ellipse farthest-side"
            | "ellipse farthest-corner"
    )
    .then_some((0.5, 0.5))
}

fn parse_svg_css_background_position(value: &str) -> Option<(f32, f32)> {
    let words = value
        .split_ascii_whitespace()
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    match words.as_slice() {
        [] => None,
        [one] => {
            if matches_ignore_ascii_case(one, "top") || matches_ignore_ascii_case(one, "bottom") {
                let y = parse_svg_css_position_component(one, false)?;
                Some((0.5, 1.0 - y))
            } else {
                let x = parse_svg_css_position_component(one, true)?;
                Some((x, 0.5))
            }
        }
        [x, y, ..] => {
            let x = parse_svg_css_position_component(x, true)?;
            let y = parse_svg_css_position_component(y, false)?;
            Some((x, 1.0 - y))
        }
    }
}

fn parse_svg_css_position_component(value: &str, horizontal: bool) -> Option<f32> {
    let value = value.trim();
    match value.to_ascii_lowercase().as_str() {
        "left" if horizontal => Some(0.0),
        "right" if horizontal => Some(1.0),
        "top" if !horizontal => Some(0.0),
        "bottom" if !horizontal => Some(1.0),
        "center" => Some(0.5),
        _ => parse_svg_gradient_offset(value),
    }
    .map(|value| value.clamp(0.0, 1.0))
}

fn matches_ignore_ascii_case(value: &str, expected: &str) -> bool {
    value.eq_ignore_ascii_case(expected)
}

fn parse_svg_css_gradient_stops(
    parts: &[&str],
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
) -> Option<Vec<SvgGradientStop>> {
    let mut stops = Vec::new();
    for part in parts.iter().take(32) {
        stops.push(parse_svg_css_gradient_stop(part, css_vars, base_color)?);
    }
    normalize_svg_css_gradient_stops(stops)
}

fn parse_svg_css_gradient_stop(
    value: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
) -> Option<(Option<f32>, SvgColor)> {
    let (color_src, rest) = split_svg_css_color_token(value)?;
    let color = parse_svg_css_color_over_background(color_src, css_vars, base_color, 0)?;
    let offset = rest
        .split_ascii_whitespace()
        .next()
        .and_then(parse_svg_gradient_offset);
    Some((offset, color))
}

fn normalize_svg_css_gradient_stops(
    mut stops: Vec<(Option<f32>, SvgColor)>,
) -> Option<Vec<SvgGradientStop>> {
    if stops.len() < 2 {
        return None;
    }
    if stops[0].0.is_none() {
        stops[0].0 = Some(0.0);
    }
    let last = stops.len() - 1;
    if stops[last].0.is_none() {
        stops[last].0 = Some(1.0);
    }

    let mut index = 0usize;
    while index < stops.len() {
        if stops[index].0.is_some() {
            index += 1;
            continue;
        }
        let run_start = index;
        while index < stops.len() && stops[index].0.is_none() {
            index += 1;
        }
        let prev = stops[run_start.saturating_sub(1)].0?;
        let next = stops.get(index).and_then(|stop| stop.0)?;
        let span = (next - prev).max(0.0);
        let count = index - run_start + 1;
        for offset in 0..index - run_start {
            let ratio = (offset + 1) as f32 / count as f32;
            stops[run_start + offset].0 = Some((prev + span * ratio).clamp(0.0, 1.0));
        }
    }

    let mut normalized = stops
        .into_iter()
        .filter_map(|(offset, color)| offset.map(|offset| (offset.clamp(0.0, 1.0), color)))
        .collect::<Vec<_>>();
    normalized.sort_by(|a, b| a.0.total_cmp(&b.0));
    svg_native_gradient_stops(&normalized)
}

fn split_svg_css_color_token(value: &str) -> Option<(&str, &str)> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(open) = value.find('(') {
        let name = value[..open].trim();
        if !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        {
            let rest = &value[open + 1..];
            let close = find_svg_css_function_close(rest)?;
            let end = open + 1 + close + 1;
            return Some((&value[..end], value[end..].trim_start()));
        }
    }
    let split = value
        .char_indices()
        .find_map(|(idx, ch)| ch.is_ascii_whitespace().then_some(idx))
        .unwrap_or(value.len());
    Some((&value[..split], value[split..].trim_start()))
}

fn parse_svg_css_color_over_background(
    value: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
    depth: usize,
) -> Option<SvgColor> {
    if depth >= 8 {
        return None;
    }
    let value = value.trim();
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_css_color_over_background(&resolved, css_vars, base_color, depth + 1);
    }
    if value.eq_ignore_ascii_case("transparent") || value.eq_ignore_ascii_case("none") {
        return Some(base_color);
    }
    if let Some(color) = parse_svg_css_color_mix_over_background(value, css_vars, base_color, depth)
    {
        return Some(color);
    }
    let alpha = parse_svg_paint_alpha(value, css_vars).unwrap_or(1.0);
    let color = parse_svg_color(value, css_vars)?;
    Some(svg_composite_color_over_background(
        color, alpha, base_color,
    ))
}

fn parse_svg_css_color_mix_over_background(
    value: &str,
    css_vars: &[SvgCssVariable],
    base_color: SvgColor,
    depth: usize,
) -> Option<SvgColor> {
    let args = svg_css_function_args(value, "color-mix")?;
    let parts = split_svg_top_level_commas(args)?;
    if parts.len() != 3 || !svg_color_mix_space_is_srgb(parts[0]) {
        return None;
    }
    let (first_color, first_alpha, first_weight) =
        parse_svg_css_color_mix_component_over_background(parts[1], css_vars, depth + 1)?;
    let (second_color, second_alpha, second_weight) =
        parse_svg_css_color_mix_component_over_background(parts[2], css_vars, depth + 1)?;
    let (first_weight, second_weight) =
        svg_color_mix_weights(first_weight, second_weight).filter(|(a, b)| *a > 0.0 || *b > 0.0)?;
    let mixed_alpha = first_alpha.mul_add(first_weight, second_alpha * second_weight);
    if !mixed_alpha.is_finite() || mixed_alpha <= 0.001 {
        return Some(base_color);
    }
    let premul = (
        first_color.0.mul_add(
            first_alpha * first_weight,
            second_color.0 * second_alpha * second_weight,
        ),
        first_color.1.mul_add(
            first_alpha * first_weight,
            second_color.1 * second_alpha * second_weight,
        ),
        first_color.2.mul_add(
            first_alpha * first_weight,
            second_color.2 * second_alpha * second_weight,
        ),
    );
    let mixed = (
        (premul.0 / mixed_alpha).clamp(0.0, 1.0),
        (premul.1 / mixed_alpha).clamp(0.0, 1.0),
        (premul.2 / mixed_alpha).clamp(0.0, 1.0),
    );
    Some(svg_composite_color_over_background(
        mixed,
        mixed_alpha,
        base_color,
    ))
}

fn parse_svg_css_color_mix_component_over_background(
    component: &str,
    css_vars: &[SvgCssVariable],
    depth: usize,
) -> Option<(SvgColor, f32, Option<f32>)> {
    let (color_src, weight) = split_svg_color_mix_component_weight(component)?;
    if svg_color_source_is_transparent(color_src, css_vars) {
        return Some(((0.0, 0.0, 0.0), 0.0, weight));
    }
    if color_src.trim().starts_with("var(") {
        let resolved = resolve_svg_css_value(color_src, css_vars, 0)?;
        return parse_svg_css_color_mix_component_over_background(&resolved, css_vars, depth + 1)
            .map(|(color, alpha, _)| (color, alpha, weight));
    }
    let alpha = parse_svg_paint_alpha(color_src, css_vars).unwrap_or(1.0);
    let color = parse_svg_color_inner(color_src, css_vars, depth + 1)?;
    Some((color, alpha, weight))
}

fn svg_composite_color_over_background(
    color: SvgColor,
    alpha: f32,
    base_color: SvgColor,
) -> SvgColor {
    let alpha = alpha.clamp(0.0, 1.0);
    (
        color
            .0
            .mul_add(alpha, base_color.0 * (1.0 - alpha))
            .clamp(0.0, 1.0),
        color
            .1
            .mul_add(alpha, base_color.1 * (1.0 - alpha))
            .clamp(0.0, 1.0),
        color
            .2
            .mul_add(alpha, base_color.2 * (1.0 - alpha))
            .clamp(0.0, 1.0),
    )
}

fn split_svg_css_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut quote = None;
    for (idx, ch) in value.char_indices() {
        if let Some(quote_ch) = quote {
            if ch == quote_ch {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => depth = depth.saturating_add(1),
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = value[start..idx].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let part = value[start..].trim();
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

fn parse_svg_accessible_text(src: &str) -> Option<String> {
    let (title, desc) = parse_svg_root_accessible_texts(src);
    match (title, desc) {
        (Some(title), Some(desc)) if title == desc => Some(title),
        (Some(title), Some(desc)) => truncate_svg_accessible_text(&format!("{title} - {desc}"))
            .filter(|text| !text.is_empty()),
        (Some(title), None) => Some(title),
        (None, Some(desc)) => Some(desc),
        (None, None) => None,
    }
}

fn parse_svg_root_accessible_texts(src: &str) -> (Option<String>, Option<String>) {
    let mut pos = 0usize;
    let mut in_root = false;
    let mut depth = 0usize;
    let mut title = None;
    let mut desc = None;

    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }

        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src[open + 1..close].trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }

        let closing = raw.starts_with('/');
        let raw = if closing { raw[1..].trim_start() } else { raw };
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, _) = svg_tag_parts(raw);
        if tag.is_empty() {
            continue;
        }
        let tag_name_lower = tag.to_ascii_lowercase();
        let local = svg_local_name(&tag_name_lower);

        if closing {
            if in_root && local == "svg" && depth == 1 {
                break;
            }
            if in_root {
                depth = depth.saturating_sub(1);
            }
            continue;
        }

        if !in_root {
            if local == "svg" {
                in_root = true;
                if self_closing {
                    break;
                }
                depth = 1;
            }
            continue;
        }

        if matches!(local, "style" | "script")
            && !self_closing
            && let Some(end_open) = find_svg_matching_end_tag(src, pos, local)
            && let Some(tag_end) = src.get(end_open..).and_then(|s| s.find('>'))
        {
            pos = end_open + tag_end + 1;
            continue;
        }

        if depth == 1 && matches!(local, "title" | "desc") && !self_closing {
            if let Some(end_open) = find_svg_matching_end_tag(src, pos, local) {
                if let Some(text) =
                    normalize_svg_accessible_text(src.get(pos..end_open).unwrap_or_default())
                {
                    if local == "title" && title.is_none() {
                        title = Some(text);
                    } else if local == "desc" && desc.is_none() {
                        desc = Some(text);
                    }
                }
                if let Some(tag_end) = src.get(end_open..).and_then(|s| s.find('>')) {
                    pos = end_open + tag_end + 1;
                } else {
                    break;
                }
                if title.is_some() && desc.is_some() {
                    break;
                }
                continue;
            }
        }

        if !self_closing {
            depth = depth.saturating_add(1);
        }
    }

    (title, desc)
}

fn normalize_svg_accessible_text(src: &str) -> Option<String> {
    let stripped = strip_svg_tags(src);
    let decoded = decode_xml_entities(&stripped);
    truncate_svg_accessible_text(&decoded).filter(|text| !text.is_empty())
}

fn truncate_svg_accessible_text(src: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = 0usize;
    for word in src.split_whitespace() {
        let sep_chars = usize::from(!out.is_empty());
        let word_chars = word.chars().count();
        if chars.saturating_add(sep_chars).saturating_add(word_chars)
            > MAX_SVG_ACCESSIBLE_TEXT_CHARS
        {
            break;
        }
        if sep_chars == 1 {
            out.push(' ');
            chars += 1;
        }
        out.push_str(word);
        chars += word_chars;
    }
    (!out.is_empty()).then_some(out)
}

fn svg_tag_parts(raw: &str) -> (&str, &str) {
    let raw = raw.trim_end_matches('/').trim();
    let name_end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            (!ch.is_ascii_alphanumeric() && !matches!(ch, ':' | '_' | '-')).then_some(idx)
        })
        .unwrap_or(raw.len());
    (&raw[..name_end], raw[name_end..].trim())
}

fn svg_local_name(tag: &str) -> &str {
    tag.rsplit_once(':').map_or(tag, |(_, local)| local)
}

fn parse_svg_attrs(src: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut pos = 0usize;
    while pos < src.len() {
        let Some((name_start, _)) = src[pos..]
            .char_indices()
            .find(|(_, ch)| !ch.is_ascii_whitespace() && *ch != '/')
        else {
            break;
        };
        pos += name_start;
        let name_end = pos
            + src[pos..]
                .char_indices()
                .find_map(|(idx, ch)| {
                    (ch.is_ascii_whitespace() || ch == '=' || ch == '/').then_some(idx)
                })
                .unwrap_or(src.len() - pos);
        let name = src[pos..name_end].trim().to_ascii_lowercase();
        pos = name_end;
        while let Some(ch) = src[pos..].chars().next() {
            if !ch.is_ascii_whitespace() {
                break;
            }
            pos += ch.len_utf8();
        }
        if !src[pos..].starts_with('=') {
            if !name.is_empty() {
                attrs.push((name, String::new()));
            }
            continue;
        }
        pos += 1;
        while let Some(ch) = src[pos..].chars().next() {
            if !ch.is_ascii_whitespace() {
                break;
            }
            pos += ch.len_utf8();
        }
        let Some(first) = src[pos..].chars().next() else {
            break;
        };
        let value;
        if first == '"' || first == '\'' {
            pos += first.len_utf8();
            let Some(end_rel) = src[pos..].find(first) else {
                break;
            };
            value = decode_xml_entities(&src[pos..pos + end_rel]);
            pos += end_rel + first.len_utf8();
        } else {
            let end_rel = src[pos..]
                .char_indices()
                .find_map(|(idx, ch)| (ch.is_ascii_whitespace() || ch == '/').then_some(idx))
                .unwrap_or(src.len() - pos);
            value = decode_xml_entities(&src[pos..pos + end_rel]);
            pos += end_rel;
        }
        if !name.is_empty() {
            attrs.push((name, value));
        }
    }
    attrs
}

fn svg_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find_map(|(k, v)| (k == name).then_some(v.as_str()))
}

fn parse_svg_anchor_link(attrs: &[(String, String)]) -> Option<Option<LinkTarget>> {
    svg_attr(attrs, "href")
        .or_else(|| svg_attr(attrs, "xlink:href"))
        .map(safe_pdf_link)
}

fn push_svg_element(
    elements: &mut Vec<SvgElement>,
    mut element: SvgElement,
    link: Option<&LinkTarget>,
) {
    apply_svg_link(&mut element, link);
    elements.push(element);
}

fn apply_svg_links(elements: &mut [SvgElement], link: Option<&LinkTarget>) {
    for element in elements {
        apply_svg_link(element, link);
    }
}

fn apply_svg_link(element: &mut SvgElement, link: Option<&LinkTarget>) {
    let Some(link) = link else {
        return;
    };
    match element {
        SvgElement::Rect(rect) if rect.link.is_none() => rect.link = Some(link.clone()),
        SvgElement::Ellipse(ellipse) if ellipse.link.is_none() => ellipse.link = Some(link.clone()),
        SvgElement::Line(line) if line.link.is_none() => line.link = Some(link.clone()),
        SvgElement::Polyline(poly) | SvgElement::Polygon(poly) if poly.link.is_none() => {
            poly.link = Some(link.clone());
        }
        SvgElement::Path(path) if path.link.is_none() => path.link = Some(link.clone()),
        SvgElement::Image(image) if image.link.is_none() => image.link = Some(link.clone()),
        SvgElement::Text(text) if text.link.is_none() => text.link = Some(link.clone()),
        _ => {}
    }
}

fn parse_svg_root_geometry(attrs: &[(String, String)]) -> Option<SvgRootGeometry> {
    let width = parse_svg_positive_attr(attrs, "width");
    let height = parse_svg_positive_attr(attrs, "height");
    let view_box = parse_svg_view_box(attrs).or_else(|| {
        let (Some(w), Some(h)) = (width, height) else {
            return None;
        };
        Some(SvgViewBox {
            x: 0.0,
            y: 0.0,
            w,
            h,
        })
    })?;
    let viewport = SvgViewport {
        w: width.unwrap_or(view_box.w),
        h: height.unwrap_or(view_box.h),
    };
    (viewport.w > 0.0 && viewport.h > 0.0).then_some(SvgRootGeometry {
        view_box,
        viewport,
        preserve_aspect: parse_svg_preserve_aspect_ratio(svg_attr(attrs, "preserveaspectratio")),
    })
}

fn parse_svg_positive_attr(attrs: &[(String, String)], name: &str) -> Option<f32> {
    let value = svg_attr(attrs, name)?.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    if value[end..].trim_start().starts_with('%') {
        return None;
    }
    let parsed = value[..end].parse::<f32>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

fn parse_svg_view_box(attrs: &[(String, String)]) -> Option<SvgViewBox> {
    if let Some(raw) = svg_attr(attrs, "viewbox") {
        let nums = parse_svg_number_list(raw);
        if nums.len() >= 4 && nums[2] > 0.0 && nums[3] > 0.0 {
            return Some(SvgViewBox {
                x: nums[0],
                y: nums[1],
                w: nums[2],
                h: nums[3],
            });
        }
    }
    None
}

fn parse_svg_preserve_aspect_ratio(value: Option<&str>) -> SvgPreserveAspectRatio {
    let Some(value) = value else {
        return SvgPreserveAspectRatio::DEFAULT;
    };
    let mut tokens = value
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty());
    let Some(first) = tokens.next() else {
        return SvgPreserveAspectRatio::DEFAULT;
    };
    let align = if first.eq_ignore_ascii_case("defer") {
        tokens.next()
    } else {
        Some(first)
    };
    let Some(align) = align else {
        return SvgPreserveAspectRatio::DEFAULT;
    };
    if align.eq_ignore_ascii_case("none") {
        return SvgPreserveAspectRatio {
            mode: SvgAspectScaleMode::None,
            align_x: 0.0,
            align_y: 0.0,
        };
    }
    let Some((align_x, align_y)) = parse_svg_preserve_aspect_align(align) else {
        return SvgPreserveAspectRatio::DEFAULT;
    };
    let mode = match tokens
        .next()
        .unwrap_or("meet")
        .to_ascii_lowercase()
        .as_str()
    {
        "slice" => SvgAspectScaleMode::Slice,
        "meet" => SvgAspectScaleMode::Meet,
        _ => SvgAspectScaleMode::Meet,
    };
    SvgPreserveAspectRatio {
        mode,
        align_x,
        align_y,
    }
}

fn parse_svg_preserve_aspect_align(value: &str) -> Option<(f32, f32)> {
    let value = value.to_ascii_lowercase();
    if !value.starts_with('x') {
        return None;
    }
    let y_pos = value.find('y')?;
    let align_x = match &value[1..y_pos] {
        "min" => 0.0,
        "mid" => 0.5,
        "max" => 1.0,
        _ => return None,
    };
    let align_y = match &value[y_pos + 1..] {
        "min" => 0.0,
        "mid" => 0.5,
        "max" => 1.0,
        _ => return None,
    };
    Some((align_x, align_y))
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_rect(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgRect> {
    let w = svg_attr(attrs, "width").and_then(parse_svg_number)?;
    let h = svg_attr(attrs, "height").and_then(parse_svg_number)?;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (rx, ry) = parse_svg_rect_radius(attrs, css_rules, css_vars, ancestors);
    Some(SvgRect {
        x: svg_attr(attrs, "x")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        y: svg_attr(attrs, "y")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        w,
        h,
        rx,
        ry,
        style: parse_svg_style_with_ancestors(
            "rect",
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        ),
        link: None,
    })
}

#[derive(Default)]
struct SvgRectRadius {
    rx: Option<f32>,
    ry: Option<f32>,
}

impl SvgRectRadius {
    fn resolved(self) -> (f32, f32) {
        let rx = self.rx.or(self.ry).unwrap_or(0.0);
        let ry = self.ry.or(self.rx).unwrap_or(0.0);
        (rx, ry)
    }
}

fn parse_svg_rect_radius(
    attrs: &[(String, String)],
    css_rules: &[SvgCssRule],
    css_vars: &[SvgCssVariable],
    ancestors: &[SvgCssAncestor],
) -> (f32, f32) {
    let scoped_css_vars = svg_css_vars_for_element(css_vars, css_rules, ancestors, "rect", attrs);
    let mut radius = SvgRectRadius {
        rx: svg_attr(attrs, "rx").and_then(|value| parse_svg_css_number(value, &scoped_css_vars)),
        ry: svg_attr(attrs, "ry").and_then(|value| parse_svg_css_number(value, &scoped_css_vars)),
    };
    for rule in svg_matching_css_rules("rect", attrs, css_rules, ancestors) {
        apply_svg_rect_radius_decls(&mut radius, &rule.decls, &scoped_css_vars);
    }
    if let Some(style_attr) = svg_attr(attrs, "style") {
        apply_svg_rect_radius_decls(&mut radius, style_attr, &scoped_css_vars);
    }
    radius.resolved()
}

fn apply_svg_rect_radius_decls(
    radius: &mut SvgRectRadius,
    decls: &str,
    css_vars: &[SvgCssVariable],
) {
    for decl in decls.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "rx" => {
                if let Some(rx) = parse_svg_css_number(value, css_vars) {
                    radius.rx = Some(rx);
                }
            }
            "ry" => {
                if let Some(ry) = parse_svg_css_number(value, css_vars) {
                    radius.ry = Some(ry);
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_circle(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgEllipse> {
    let r = svg_attr(attrs, "r").and_then(parse_svg_number)?;
    (r > 0.0).then_some(SvgEllipse {
        cx: svg_attr(attrs, "cx")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        cy: svg_attr(attrs, "cy")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        rx: r,
        ry: r,
        style: parse_svg_style_with_ancestors(
            "circle",
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        ),
        link: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_ellipse(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgEllipse> {
    let rx = svg_attr(attrs, "rx").and_then(parse_svg_number)?;
    let ry = svg_attr(attrs, "ry").and_then(parse_svg_number)?;
    (rx > 0.0 && ry > 0.0).then_some(SvgEllipse {
        cx: svg_attr(attrs, "cx")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        cy: svg_attr(attrs, "cy")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        rx,
        ry,
        style: parse_svg_style_with_ancestors(
            "ellipse",
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        ),
        link: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_line(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    markers: &[SvgMarker],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgLine> {
    let marker_refs =
        parse_svg_marker_refs_for_element("line", attrs, css_rules, css_vars, markers, ancestors);
    Some(SvgLine {
        x1: svg_attr(attrs, "x1")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        y1: svg_attr(attrs, "y1")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        x2: svg_attr(attrs, "x2")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        y2: svg_attr(attrs, "y2")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        style: parse_svg_style_with_ancestors(
            "line",
            attrs,
            SvgStyle {
                color: inherited.color,
                fill: None,
                fill_gradient: None,
                fill_pattern: None,
                fill_current_color: false,
                fill_context: None,
                stroke: inherited.stroke,
                stroke_gradient: inherited.stroke_gradient,
                stroke_current_color: inherited.stroke_current_color,
                stroke_context: inherited.stroke_context,
                stroke_width: inherited.stroke_width,
                non_scaling_stroke: inherited.non_scaling_stroke,
                opacity: inherited.opacity,
                fill_opacity: inherited.fill_opacity,
                stroke_opacity: inherited.stroke_opacity,
                display_visible: inherited.display_visible,
                visibility_visible: inherited.visibility_visible,
                visible: inherited.visible,
                shadow: inherited.shadow,
                clip_path: inherited.clip_path,
                mask_path: inherited.mask_path,
                transform: inherited.transform,
                dash: inherited.dash,
                line_cap: inherited.line_cap,
                line_join: inherited.line_join,
                miter_limit: inherited.miter_limit,
                fill_rule: inherited.fill_rule,
                paint_order: inherited.paint_order,
                font_size: inherited.font_size,
                text_anchor: inherited.text_anchor,
                font_weight: inherited.font_weight,
                font_slant: inherited.font_slant,
                font_family: inherited.font_family,
                dominant_baseline: inherited.dominant_baseline,
                letter_spacing: inherited.letter_spacing,
                text_decoration: inherited.text_decoration,
            },
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        ),
        marker_end: marker_refs.end,
        marker_start: marker_refs.start,
        link: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_poly(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    closed: bool,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    markers: &[SvgMarker],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgPoly> {
    let points = parse_svg_points(svg_attr(attrs, "points")?);
    if points.len() < if closed { 3 } else { 2 } {
        return None;
    }
    let base = if closed {
        inherited
    } else {
        SvgStyle {
            color: inherited.color,
            fill: None,
            fill_gradient: None,
            fill_pattern: None,
            fill_current_color: false,
            fill_context: None,
            stroke: inherited.stroke,
            stroke_gradient: inherited.stroke_gradient,
            stroke_current_color: inherited.stroke_current_color,
            stroke_context: inherited.stroke_context,
            stroke_width: inherited.stroke_width,
            non_scaling_stroke: inherited.non_scaling_stroke,
            opacity: inherited.opacity,
            fill_opacity: inherited.fill_opacity,
            stroke_opacity: inherited.stroke_opacity,
            display_visible: inherited.display_visible,
            visibility_visible: inherited.visibility_visible,
            visible: inherited.visible,
            shadow: inherited.shadow,
            clip_path: inherited.clip_path,
            mask_path: inherited.mask_path,
            transform: inherited.transform,
            dash: inherited.dash,
            line_cap: inherited.line_cap,
            line_join: inherited.line_join,
            miter_limit: inherited.miter_limit,
            fill_rule: inherited.fill_rule,
            paint_order: inherited.paint_order,
            font_size: inherited.font_size,
            text_anchor: inherited.text_anchor,
            font_weight: inherited.font_weight,
            font_slant: inherited.font_slant,
            font_family: inherited.font_family,
            dominant_baseline: inherited.dominant_baseline,
            letter_spacing: inherited.letter_spacing,
            text_decoration: inherited.text_decoration,
        }
    };
    let marker_refs = parse_svg_marker_refs_for_element(
        if closed { "polygon" } else { "polyline" },
        attrs,
        css_rules,
        css_vars,
        markers,
        ancestors,
    );
    Some(SvgPoly {
        points,
        style: parse_svg_style_with_ancestors(
            if closed { "polygon" } else { "polyline" },
            attrs,
            base,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        ),
        marker_end: marker_refs.end,
        marker_mid: marker_refs.mid,
        marker_start: marker_refs.start,
        link: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_path(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    markers: &[SvgMarker],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgPath> {
    let ops = parse_svg_path_data(svg_attr(attrs, "d")?)?;
    if ops.is_empty() {
        return None;
    }
    let marker_refs =
        parse_svg_marker_refs_for_element("path", attrs, css_rules, css_vars, markers, ancestors);
    Some(SvgPath {
        ops,
        style: parse_svg_style_with_ancestors(
            "path",
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        ),
        marker_end: marker_refs.end,
        marker_mid: marker_refs.mid,
        marker_start: marker_refs.start,
        link: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_embedded_image(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgEmbeddedImage> {
    let w = svg_attr(attrs, "width").and_then(parse_svg_number)?;
    let h = svg_attr(attrs, "height").and_then(parse_svg_number)?;
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let style = parse_svg_style_with_ancestors(
        "image",
        attrs,
        inherited,
        css_rules,
        gradients,
        patterns,
        css_vars,
        clip_paths,
        filter_shadows,
        ancestors,
    );
    if !style.visible {
        return None;
    }
    let href = svg_attr(attrs, "href").or_else(|| svg_attr(attrs, "xlink:href"))?;
    let png = decode_svg_png_data_uri(href)?;
    let key = svg_inline_png_key(&png);
    let image = parse_png_image_asset(&key, &png)?;
    Some(SvgEmbeddedImage {
        x: svg_attr(attrs, "x")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        y: svg_attr(attrs, "y")
            .and_then(parse_svg_number)
            .unwrap_or(0.0),
        w,
        h,
        preserve_aspect: parse_svg_preserve_aspect_ratio(svg_attr(attrs, "preserveaspectratio")),
        style,
        image: Box::new(image),
        link: None,
    })
}

fn decode_svg_png_data_uri(href: &str) -> Option<Vec<u8>> {
    let (metadata, payload) = href.trim().split_once(',')?;
    let metadata = metadata.trim().to_ascii_lowercase();
    let suffix = metadata.strip_prefix("data:image/png")?;
    if !suffix.is_empty() && !suffix.starts_with(';') {
        return None;
    }
    if !suffix
        .split(';')
        .skip(1)
        .any(|part| part.trim() == "base64")
    {
        return None;
    }
    decode_svg_base64_payload(payload)
}

fn decode_svg_base64_payload(payload: &str) -> Option<Vec<u8>> {
    let encoded_len = payload
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .count();
    if encoded_len == 0
        || encoded_len % 4 != 0
        || encoded_len.saturating_mul(3) / 4 > MAX_PDF_IMAGE_COMPRESSED_BYTES
    {
        return None;
    }
    let mut out = Vec::with_capacity(encoded_len.saturating_mul(3) / 4);
    let mut quartet = [0u8; 4];
    let mut qlen = 0usize;
    let mut finished = false;
    for byte in payload.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if finished {
            return None;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => return None,
        };
        quartet[qlen] = value;
        qlen += 1;
        if qlen != 4 {
            continue;
        }
        if quartet[0] == 64 || quartet[1] == 64 || (quartet[2] == 64 && quartet[3] != 64) {
            return None;
        }
        out.push((quartet[0] << 2) | (quartet[1] >> 4));
        if quartet[2] != 64 {
            out.push((quartet[1] << 4) | (quartet[2] >> 2));
        }
        if quartet[3] != 64 {
            out.push((quartet[2] << 6) | quartet[3]);
        }
        finished = quartet[2] == 64 || quartet[3] == 64;
        qlen = 0;
    }
    (qlen == 0).then_some(out)
}

fn svg_inline_png_key(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("svg-inline-png-{hash:016x}-{}", bytes.len())
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_text_elements(
    attrs: &[(String, String)],
    body: &str,
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> Vec<SvgElement> {
    let style = parse_svg_style_with_ancestors(
        "text",
        attrs,
        inherited,
        css_rules,
        gradients,
        patterns,
        css_vars,
        clip_paths,
        filter_shadows,
        ancestors,
    );
    if !style.display_visible || style.opacity <= 0.001 {
        return Vec::new();
    }
    let x = svg_attr(attrs, "x")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let y = svg_attr(attrs, "y")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let font_size = style.font_size;
    let anchor = style.text_anchor;
    let text_length = parse_svg_text_length_attr(attrs, font_size);
    let length_adjust = parse_svg_length_adjust(attrs, SvgLengthAdjust::Spacing);
    if !svg_text_body_has_tspan(body) {
        return svg_text_element(
            normalize_svg_text(&strip_svg_tags(body)),
            SvgTextPlacement {
                x,
                y,
                font_size,
                anchor,
                text_length,
                length_adjust,
            },
            style,
        )
        .into_iter()
        .collect();
    }

    let mut elements = Vec::new();
    let mut child_ancestors = ancestors.to_vec();
    child_ancestors.push(svg_css_ancestor("text", attrs));
    push_svg_text_body_elements(
        &mut elements,
        body,
        x,
        y,
        style,
        font_size,
        anchor,
        css_rules,
        gradients,
        patterns,
        css_vars,
        clip_paths,
        filter_shadows,
        &mut child_ancestors,
        0,
    );
    apply_svg_parent_text_length(&mut elements, text_length, length_adjust);
    elements
}

#[allow(clippy::too_many_arguments)]
fn push_svg_text_body_elements(
    elements: &mut Vec<SvgElement>,
    body: &str,
    mut current_x: f32,
    mut current_y: f32,
    style: SvgStyle,
    font_size: f32,
    anchor: SvgTextAnchor,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &mut Vec<SvgCssAncestor>,
    depth: usize,
) -> (f32, f32) {
    if depth >= 8 {
        let text = normalize_svg_text(&strip_svg_tags(body));
        let advance = svg_text_advance(&text, font_size, style.letter_spacing.to_points(font_size));
        push_svg_text_element(
            elements,
            text,
            SvgTextPlacement {
                x: current_x,
                y: current_y,
                font_size,
                anchor,
                text_length: None,
                length_adjust: SvgLengthAdjust::Spacing,
            },
            style,
        );
        return (current_x + advance, current_y);
    }

    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        let text = normalize_svg_text_node(body.get(pos..open).unwrap_or_default());
        let advance = svg_text_advance(&text, font_size, style.letter_spacing.to_points(font_size));
        push_svg_text_element(
            elements,
            text,
            SvgTextPlacement {
                x: current_x,
                y: current_y,
                font_size,
                anchor,
                text_length: None,
                length_adjust: SvgLengthAdjust::Spacing,
            },
            style,
        );
        current_x += advance;

        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("tspan") || self_closing {
            continue;
        }
        let child_attrs = parse_svg_attrs(attrs_src);
        let Some(end_open) = find_svg_matching_end_tag(body, pos, "tspan") else {
            break;
        };
        let child_body = body.get(pos..end_open).unwrap_or_default();
        let Some(end_close_rel) = body.get(end_open..).and_then(|s| s.find('>')) else {
            break;
        };
        pos = end_open + end_close_rel + 1;

        let child_style = parse_svg_style_with_ancestors(
            "tspan",
            &child_attrs,
            style,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        );
        if !child_style.display_visible || child_style.opacity <= 0.001 {
            continue;
        }
        let child_font_size = child_style.font_size;
        let child_anchor = child_style.text_anchor;
        let child_text_length = parse_svg_text_length_attr(&child_attrs, child_font_size);
        let child_length_adjust = parse_svg_length_adjust(&child_attrs, SvgLengthAdjust::Spacing);
        let mut child_x = svg_attr(&child_attrs, "x")
            .and_then(parse_svg_number)
            .unwrap_or(current_x);
        let mut child_y = svg_attr(&child_attrs, "y")
            .and_then(parse_svg_number)
            .unwrap_or(current_y);
        if let Some(dx) = svg_attr(&child_attrs, "dx")
            .and_then(|value| parse_svg_text_length(value, child_font_size))
        {
            child_x += dx;
        }
        if let Some(dy) = svg_attr(&child_attrs, "dy")
            .and_then(|value| parse_svg_text_length(value, child_font_size))
        {
            child_y += dy;
        }

        if svg_text_body_has_tspan(child_body) {
            let start_len = elements.len();
            ancestors.push(svg_css_ancestor("tspan", &child_attrs));
            let (end_x, end_y) = push_svg_text_body_elements(
                elements,
                child_body,
                child_x,
                child_y,
                child_style,
                child_font_size,
                child_anchor,
                css_rules,
                gradients,
                patterns,
                css_vars,
                clip_paths,
                filter_shadows,
                ancestors,
                depth + 1,
            );
            ancestors.pop();
            apply_svg_parent_text_length(
                &mut elements[start_len..],
                child_text_length,
                child_length_adjust,
            );
            current_x = child_text_length.map_or(end_x, |width| child_x + width);
            current_y = end_y;
        } else {
            let child_text = normalize_svg_text(&strip_svg_tags(child_body));
            let advance = child_text_length.unwrap_or_else(|| {
                svg_text_advance(
                    &child_text,
                    child_font_size,
                    child_style.letter_spacing.to_points(child_font_size),
                )
            });
            push_svg_text_element(
                elements,
                child_text,
                SvgTextPlacement {
                    x: child_x,
                    y: child_y,
                    font_size: child_font_size,
                    anchor: child_anchor,
                    text_length: child_text_length,
                    length_adjust: child_length_adjust,
                },
                child_style,
            );
            current_x = child_x + advance;
            current_y = child_y;
        }

        if elements.len() >= 256 {
            break;
        }
    }
    if pos < body.len() {
        let text = normalize_svg_text(body.get(pos..).unwrap_or_default());
        let advance = svg_text_advance(&text, font_size, style.letter_spacing.to_points(font_size));
        push_svg_text_element(
            elements,
            text,
            SvgTextPlacement {
                x: current_x,
                y: current_y,
                font_size,
                anchor,
                text_length: None,
                length_adjust: SvgLengthAdjust::Spacing,
            },
            style,
        );
        current_x += advance;
    }

    (current_x, current_y)
}

fn svg_text_body_has_tspan(body: &str) -> bool {
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let (tag, _) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if svg_local_name(&tag_lower).eq_ignore_ascii_case("tspan") {
            return true;
        }
    }
    false
}

fn push_svg_text_element(
    out: &mut Vec<SvgElement>,
    text: String,
    placement: SvgTextPlacement,
    style: SvgStyle,
) {
    if let Some(text) = svg_text_element(text, placement, style) {
        out.push(text);
    }
}

fn svg_text_element(
    text: String,
    placement: SvgTextPlacement,
    style: SvgStyle,
) -> Option<SvgElement> {
    if !style.visible {
        return None;
    }
    let fill = style.fill?;
    let opacity = svg_effective_fill_opacity(style);
    if opacity <= 0.001 {
        return None;
    }
    (!text.is_empty()).then_some(SvgElement::Text(SvgText {
        x: placement.x,
        y: placement.y,
        text,
        slot: slot_of(
            style.font_weight.is_bold(),
            style.font_slant.is_italic(),
            matches!(style.font_family, SvgFontFamily::Mono),
        ),
        font_size: placement.font_size,
        letter_spacing: style.letter_spacing.to_points(placement.font_size),
        text_length: placement.text_length,
        length_adjust: placement.length_adjust,
        baseline: style.dominant_baseline,
        anchor: placement.anchor,
        decoration: style.text_decoration,
        fill,
        opacity,
        clip_path: style.clip_path,
        mask_path: style.mask_path,
        transform: style.transform,
        link: None,
    }))
}

fn normalize_svg_text(src: &str) -> String {
    decode_xml_entities(src)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_svg_text_node(src: &str) -> String {
    let decoded = decode_xml_entities(src);
    let text = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return text;
    }
    let leading = decoded.chars().next().is_some_and(char::is_whitespace);
    let trailing = decoded.chars().next_back().is_some_and(char::is_whitespace);
    let mut out = String::with_capacity(text.len() + usize::from(leading) + usize::from(trailing));
    if leading {
        out.push(' ');
    }
    out.push_str(&text);
    if trailing {
        out.push(' ');
    }
    out
}

fn svg_text_advance(text: &str, font_size: f32, letter_spacing: f32) -> f32 {
    let mut glyph_count = 0usize;
    let width = text
        .chars()
        .map(|ch| {
            glyph_count += 1;
            if ch.is_whitespace() {
                0.33
            } else if ch.is_ascii() {
                0.56
            } else {
                0.9
            }
        })
        .sum::<f32>()
        * font_size;
    width + glyph_count.saturating_sub(1) as f32 * letter_spacing
}

fn apply_svg_parent_text_length(
    elements: &mut [SvgElement],
    text_length: Option<f32>,
    length_adjust: SvgLengthAdjust,
) {
    let Some(target) = text_length.filter(|value| value.is_finite() && *value >= 0.0) else {
        return;
    };
    let mut indices = Vec::new();
    let mut advances = Vec::new();
    let mut natural_total = 0.0f32;
    let mut first_transform: Option<SvgTransform> = None;
    let mut first_y: Option<f32> = None;

    for (idx, element) in elements.iter().enumerate() {
        let SvgElement::Text(text) = element else {
            continue;
        };
        if text.text_length.is_some() || !matches!(text.anchor, SvgTextAnchor::Start) {
            return;
        }
        if let Some(transform) = first_transform {
            if text.transform != transform {
                return;
            }
        } else {
            first_transform = Some(text.transform);
        }
        if let Some(y) = first_y {
            if (text.y - y).abs() > 0.05 {
                return;
            }
        } else {
            first_y = Some(text.y);
        }
        let advance = svg_text_advance(&text.text, text.font_size, text.letter_spacing);
        if !advance.is_finite() || advance <= 0.001 {
            return;
        }
        natural_total += advance;
        indices.push(idx);
        advances.push(advance);
    }

    if indices.is_empty() || !natural_total.is_finite() || natural_total <= 0.001 {
        return;
    }
    for pair in indices.windows(2).zip(advances.iter()) {
        let (&[prev_idx, next_idx], &advance) = pair else {
            continue;
        };
        let (SvgElement::Text(prev), SvgElement::Text(next)) =
            (&elements[prev_idx], &elements[next_idx])
        else {
            return;
        };
        if (next.x - (prev.x + advance)).abs() > 0.1 {
            return;
        }
    }

    let ratio = target / natural_total;
    if !ratio.is_finite() {
        return;
    }
    let first_x = match &elements[indices[0]] {
        SvgElement::Text(text) => text.x,
        _ => return,
    };
    let mut cursor = first_x;
    for (idx, advance) in indices.into_iter().zip(advances) {
        let SvgElement::Text(text) = &mut elements[idx] else {
            return;
        };
        let fragment_target = (advance * ratio).max(0.0);
        text.x = cursor;
        text.text_length = Some(fragment_target);
        text.length_adjust = length_adjust;
        cursor += fragment_target;
    }
}

fn apply_svg_font_size_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(size) = value
        .and_then(|value| parse_svg_css_number(value, css_vars))
        .map(|size| size.clamp(1.0, 96.0))
    {
        style.font_size = size;
    }
}

fn apply_svg_text_anchor_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(anchor) = value.and_then(|value| parse_svg_text_anchor_value(value, css_vars)) {
        style.text_anchor = anchor;
    }
}

fn parse_svg_font_size_value(
    value: &str,
    inherited: f32,
    css_vars: &[SvgCssVariable],
) -> Option<f32> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_font_size_value(&resolved, inherited, css_vars);
    }
    parse_svg_text_length(value, inherited).filter(|value| value.is_finite())
}

fn parse_svg_text_anchor_value(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgTextAnchor> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_text_anchor_value(&resolved, css_vars);
    }
    match value.to_ascii_lowercase().as_str() {
        "start" => Some(SvgTextAnchor::Start),
        "middle" => Some(SvgTextAnchor::Middle),
        "end" => Some(SvgTextAnchor::End),
        _ => None,
    }
}

fn parse_svg_text_length_attr(attrs: &[(String, String)], font_size: f32) -> Option<f32> {
    svg_attr(attrs, "textlength")
        .and_then(|value| parse_svg_text_length(value, font_size))
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn parse_svg_length_adjust(
    attrs: &[(String, String)],
    inherited: SvgLengthAdjust,
) -> SvgLengthAdjust {
    let mut adjust = svg_attr(attrs, "lengthadjust")
        .and_then(parse_svg_length_adjust_value)
        .unwrap_or(inherited);
    if let Some(style) = svg_attr(attrs, "style") {
        for decl in style.split(';') {
            let Some((name, value)) = decl.split_once(':') else {
                continue;
            };
            if name.trim().eq_ignore_ascii_case("lengthAdjust")
                && let Some(parsed) = parse_svg_length_adjust_value(value)
            {
                adjust = parsed;
            }
        }
    }
    adjust
}

fn parse_svg_length_adjust_value(value: &str) -> Option<SvgLengthAdjust> {
    match value.trim().to_ascii_lowercase().as_str() {
        "spacing" => Some(SvgLengthAdjust::Spacing),
        "spacingandglyphs" => Some(SvgLengthAdjust::SpacingAndGlyphs),
        _ => None,
    }
}

fn parse_svg_text_length(value: &str, font_size: f32) -> Option<f32> {
    let value = value.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let number = value[..end].parse::<f32>().ok()?;
    if !number.is_finite() {
        return None;
    }
    let unit = value[end..].trim_start();
    if unit.starts_with("em") {
        Some(number * font_size)
    } else if unit.starts_with("ex") {
        Some(number * font_size * 0.5)
    } else if unit.starts_with('%') {
        Some(number * font_size / 100.0)
    } else {
        Some(number)
    }
}

fn parse_svg_css_variables(src: &str) -> Vec<SvgCssVariable> {
    let mut vars = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty()
            || raw.starts_with('/')
            || raw.starts_with('?')
            || raw.starts_with('!')
            || raw.trim_end().ends_with('/')
        {
            continue;
        }
        let (tag, _) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("style") {
            continue;
        }
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        parse_svg_css_variables_from_css(
            src.get(pos..pos + end_rel).unwrap_or_default(),
            &mut vars,
        );
        if vars.len() >= 256 {
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    vars
}

fn parse_svg_document_css_rules(src: &str) -> Vec<SvgCssRule> {
    let mut rules = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty()
            || raw.starts_with('/')
            || raw.starts_with('?')
            || raw.starts_with('!')
            || raw.trim_end().ends_with('/')
        {
            continue;
        }
        let (tag, _) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("style") {
            continue;
        }
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        parse_svg_css_rules(src.get(pos..pos + end_rel).unwrap_or_default(), &mut rules);
        if rules.len() >= 1024 {
            rules.truncate(1024);
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    rules
}

fn parse_svg_css_variables_from_css(css: &str, out: &mut Vec<SvgCssVariable>) {
    let mut pos = 0usize;
    while let Some(open) = find_css_block_open(css, pos) {
        let selectors = css
            .get(pos..open)
            .unwrap_or_default()
            .rsplit(';')
            .next()
            .unwrap_or_default()
            .trim();
        let Some(close) = find_css_block_close(css, open) else {
            break;
        };

        if svg_css_variables_are_global_selectors(selectors) {
            parse_svg_css_variable_decls(css.get(open + 1..close).unwrap_or_default(), out);
        }
        if out.len() >= 256 {
            break;
        }
        pos = close + 1;
    }
}

fn parse_svg_css_variable_decls(decls: &str, out: &mut Vec<SvgCssVariable>) {
    for decl in decls.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if !name.starts_with("--") || name.len() <= 2 {
            continue;
        }
        let value = clean_svg_css_variable_value(value);
        if value.is_empty() {
            continue;
        }
        set_svg_css_variable(out, name, value);
        if out.len() >= 256 {
            break;
        }
    }
}

fn parse_svg_css_variable_decls_override(decls: &str, out: &mut Vec<SvgCssVariable>) {
    for decl in decls.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if !name.starts_with("--") || name.len() <= 2 {
            continue;
        }
        let value = clean_svg_css_variable_value(value);
        if value.is_empty() {
            continue;
        }
        set_svg_css_variable(out, name, value);
        if out.len() >= 256 {
            break;
        }
    }
}

fn set_svg_css_variable(out: &mut Vec<SvgCssVariable>, name: &str, value: &str) {
    if let Some(existing) = out.iter_mut().find(|existing| existing.name == name) {
        existing.value.clear();
        existing.value.push_str(value);
    } else if out.len() < 256 {
        out.push(SvgCssVariable {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
}

fn clean_svg_css_variable_value(value: &str) -> &str {
    clean_svg_css_keyword_value(value)
}

fn svg_css_variables_are_global_selectors(selectors: &str) -> bool {
    let selectors = selectors.trim();
    if selectors.is_empty() || selectors.starts_with('@') {
        return false;
    }
    selectors.split(',').any(|selector| {
        let selector = selector.trim();
        selector == ":root"
            || selector == "*"
            || selector.eq_ignore_ascii_case("svg")
            || selector.eq_ignore_ascii_case("svg:root")
    })
}

fn parse_svg_use_refs(src: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("use") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_use_href(&attrs) else {
            continue;
        };
        if !refs.iter().any(|existing| existing == id) {
            refs.push(id.to_string());
        }
        if refs.len() >= 256 {
            break;
        }
    }
    refs
}

fn parse_svg_reusable_defs(src: &str, referenced_ids: &[String]) -> Vec<SvgReusableDef> {
    if referenced_ids.is_empty() {
        return Vec::new();
    }
    let mut defs = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        let local = svg_local_name(&tag_lower);
        if !svg_reusable_tag(local) {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        if !referenced_ids
            .iter()
            .any(|referenced_id| referenced_id == id)
        {
            continue;
        }
        if defs
            .iter()
            .any(|existing: &SvgReusableDef| existing.id == id)
        {
            continue;
        }
        let body = if !self_closing && matches!(local, "g" | "symbol" | "a" | "text") {
            find_svg_matching_end_tag(src, pos, local).and_then(|end_open| {
                src.get(pos..end_open)
                    .and_then(|body| (body.len() <= 256 * 1024).then(|| body.to_string()))
            })
        } else {
            None
        };
        let (view_box, preserve_aspect) = if local == "symbol" {
            (
                parse_svg_view_box(&attrs),
                parse_svg_preserve_aspect_ratio(svg_attr(&attrs, "preserveaspectratio")),
            )
        } else {
            (None, SvgPreserveAspectRatio::DEFAULT)
        };
        defs.push(SvgReusableDef {
            id: id.to_string(),
            tag: local.to_string(),
            attrs,
            body,
            view_box,
            preserve_aspect,
        });
        if defs.len() >= 256 {
            break;
        }
    }
    defs
}

fn svg_reusable_tag(local: &str) -> bool {
    matches!(
        local,
        "g" | "symbol"
            | "a"
            | "use"
            | "text"
            | "rect"
            | "circle"
            | "ellipse"
            | "line"
            | "polyline"
            | "polygon"
            | "path"
    )
}

fn find_svg_matching_end_tag(src: &str, mut pos: usize, local_name: &str) -> Option<usize> {
    let mut depth = 1usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let end_rel = src.get(open + 4..)?.find("-->")?;
            pos = open + end_rel + 7;
            continue;
        }
        let close = open + src.get(open..)?.find('>')?;
        let raw = src.get(open + 1..close)?.trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let closing = raw.starts_with('/');
        let raw = if closing { raw[1..].trim_start() } else { raw };
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, _) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case(local_name) {
            continue;
        }
        if closing {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(open);
            }
        } else if !self_closing {
            depth = depth.saturating_add(1);
        }
    }
    None
}

fn parse_svg_gradient_paints(src: &str, css_vars: &[SvgCssVariable]) -> Vec<SvgGradientPaint> {
    let mut gradients = Vec::new();
    let stop_css_rules = parse_svg_gradient_stop_css_rules(src, css_vars);
    let definitions = parse_svg_gradient_definitions(src, css_vars, &stop_css_rules);
    for index in 0..definitions.len() {
        let definition = &definitions[index];
        if gradients
            .iter()
            .any(|existing: &SvgGradientPaint| existing.id == definition.id)
        {
            continue;
        }
        let Some(resolved) = resolve_svg_gradient_definition(&definitions, index, 0) else {
            continue;
        };
        if let Some(color) = svg_gradient_representative_color(&resolved.stops) {
            gradients.push(SvgGradientPaint {
                id: definition.id.clone(),
                color,
                linear: if resolved.linear {
                    parse_svg_linear_gradient(&resolved.attrs, &resolved.stops)
                } else {
                    None
                },
                radial: if resolved.linear {
                    None
                } else {
                    parse_svg_radial_gradient(&resolved.attrs, &resolved.stops)
                },
            });
        }
        if gradients.len() >= 128 {
            break;
        }
    }
    gradients
}

fn parse_svg_pattern_paints(
    src: &str,
    gradients: &[SvgGradientPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    css_rules: &[SvgCssRule],
    filter_shadows: &[SvgFilterShadow],
) -> Vec<SvgPatternPaint> {
    let mut patterns = Vec::new();
    let definitions = parse_svg_pattern_definitions(src);
    for (index, definition) in definitions.iter().enumerate() {
        if patterns.len() >= 64 {
            break;
        }
        if patterns
            .iter()
            .any(|existing: &SvgPatternPaint| existing.id == definition.id)
        {
            continue;
        }
        let Some(resolved) = resolve_svg_pattern_definition(&definitions, index, 0) else {
            continue;
        };
        let attrs = resolved.attrs;
        if !svg_attr(&attrs, "patternunits")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("userSpaceOnUse"))
        {
            continue;
        }
        let Some(w) = svg_attr(&attrs, "width").and_then(parse_svg_number) else {
            continue;
        };
        let Some(h) = svg_attr(&attrs, "height").and_then(parse_svg_number) else {
            continue;
        };
        if !w.is_finite() || !h.is_finite() || w <= 0.001 || h <= 0.001 {
            continue;
        }
        let mut ancestor_attrs = attrs.clone();
        ancestor_attrs.push(("id".to_string(), definition.id.clone()));
        let ancestors = [svg_css_ancestor("pattern", &ancestor_attrs)];
        let elements = parse_svg_reusable_body_elements(
            &resolved.body,
            SvgStyle::INITIAL,
            css_rules,
            gradients,
            &[],
            css_vars,
            clip_paths,
            filter_shadows,
            &[],
            &[],
            &ancestors,
            0,
        );
        if elements.is_empty() {
            continue;
        }
        let color = svg_pattern_representative_color(&elements).unwrap_or((0.0, 0.0, 0.0));
        patterns.push(SvgPatternPaint {
            id: definition.id.clone(),
            x: svg_attr(&attrs, "x")
                .and_then(parse_svg_number)
                .unwrap_or(0.0),
            y: svg_attr(&attrs, "y")
                .and_then(parse_svg_number)
                .unwrap_or(0.0),
            w,
            h,
            transform: svg_attr(&attrs, "patterntransform")
                .and_then(parse_svg_transform)
                .unwrap_or(SvgTransform::IDENTITY),
            color,
            elements,
        });
    }
    patterns
}

fn parse_svg_pattern_definitions(src: &str) -> Vec<SvgPatternDefinition> {
    let mut definitions = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("pattern") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        if definitions
            .iter()
            .any(|existing: &SvgPatternDefinition| existing.id == id)
        {
            continue;
        }
        let body = if self_closing {
            String::new()
        } else {
            let needle = format!("</{tag_lower}");
            let Some(end_rel) = src
                .get(pos..)
                .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
            else {
                continue;
            };
            let body = src.get(pos..pos + end_rel).unwrap_or_default().to_string();
            if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
                pos += end_rel + tag_end + 1;
            }
            body
        };
        definitions.push(SvgPatternDefinition {
            id: id.to_string(),
            href: svg_use_href(&attrs).map(str::to_string),
            attrs,
            body,
        });
        if definitions.len() >= 128 {
            break;
        }
    }
    definitions
}

fn resolve_svg_pattern_definition(
    definitions: &[SvgPatternDefinition],
    index: usize,
    depth: usize,
) -> Option<SvgResolvedPattern> {
    let definition = definitions.get(index)?;
    let mut attrs = Vec::new();
    let mut inherited_body = String::new();

    if depth < 8
        && let Some(href) = &definition.href
        && href != &definition.id
        && let Some(parent_index) = definitions
            .iter()
            .position(|candidate| candidate.id == *href)
        && let Some(parent) = resolve_svg_pattern_definition(definitions, parent_index, depth + 1)
    {
        attrs = parent.attrs;
        inherited_body = parent.body;
    }

    merge_svg_pattern_attrs(&mut attrs, &definition.attrs);
    let body = if svg_pattern_body_has_renderable_content(&definition.body) {
        definition.body.clone()
    } else {
        inherited_body
    };
    Some(SvgResolvedPattern { attrs, body })
}

fn svg_pattern_body_has_renderable_content(body: &str) -> bool {
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        let prefix = body.get(pos..open).unwrap_or_default();
        if !prefix.trim().is_empty() {
            return true;
        }
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                return false;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            return false;
        };
        let raw = body
            .get(open + 1..open + close_rel)
            .unwrap_or_default()
            .trim();
        if !raw.is_empty()
            && !raw.starts_with('/')
            && !raw.starts_with('?')
            && !raw.starts_with('!')
        {
            return true;
        }
        pos = open + close_rel + 1;
    }
    body.get(pos..).is_some_and(|tail| !tail.trim().is_empty())
}

fn merge_svg_pattern_attrs(into: &mut Vec<(String, String)>, attrs: &[(String, String)]) {
    for (name, value) in attrs {
        if matches!(name.as_str(), "id" | "href" | "xlink:href") {
            continue;
        }
        if let Some((_, existing_value)) = into.iter_mut().find(|(existing, _)| existing == name) {
            *existing_value = value.clone();
        } else {
            into.push((name.clone(), value.clone()));
        }
    }
}

fn svg_pattern_representative_color(elements: &[SvgElement]) -> Option<SvgColor> {
    elements.iter().find_map(svg_element_representative_color)
}

fn svg_element_representative_color(element: &SvgElement) -> Option<SvgColor> {
    let style = match element {
        SvgElement::Rect(rect) => Some(rect.style),
        SvgElement::Ellipse(ellipse) => Some(ellipse.style),
        SvgElement::Line(line) => Some(line.style),
        SvgElement::Polyline(poly) | SvgElement::Polygon(poly) => Some(poly.style),
        SvgElement::Path(path) => Some(path.style),
        SvgElement::Image(_) => None,
        SvgElement::Text(text) => return Some(text.fill),
    }?;
    style.fill.or(style.stroke)
}

fn parse_svg_gradient_definitions(
    src: &str,
    css_vars: &[SvgCssVariable],
    stop_css_rules: &[SvgGradientStopCssRule],
) -> Vec<SvgGradientDefinition> {
    let mut definitions = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        let local = svg_local_name(&tag_lower);
        if !matches!(local, "lineargradient" | "radialgradient") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        let body = if self_closing {
            ""
        } else {
            let needle = format!("</{tag_lower}");
            let Some(end_rel) = src
                .get(pos..)
                .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
            else {
                continue;
            };
            let body = src.get(pos..pos + end_rel).unwrap_or_default();
            if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
                pos += end_rel + tag_end + 1;
            }
            body
        };
        definitions.push(SvgGradientDefinition {
            id: id.to_string(),
            linear: local == "lineargradient",
            href: svg_use_href(&attrs).map(str::to_string),
            attrs,
            stops: parse_svg_gradient_stops(body, css_vars, stop_css_rules),
        });
        if definitions.len() >= 256 {
            break;
        }
    }
    definitions
}

fn resolve_svg_gradient_definition(
    definitions: &[SvgGradientDefinition],
    index: usize,
    depth: usize,
) -> Option<SvgResolvedGradient> {
    let definition = definitions.get(index)?;
    let mut attrs = Vec::new();
    let mut inherited_stops = Vec::new();

    if depth < 8
        && let Some(href) = &definition.href
        && href != &definition.id
        && let Some(parent_index) = definitions
            .iter()
            .position(|candidate| candidate.id == *href)
        && let Some(parent) = resolve_svg_gradient_definition(definitions, parent_index, depth + 1)
    {
        attrs = parent.attrs;
        inherited_stops = parent.stops;
    }

    merge_svg_gradient_attrs(&mut attrs, &definition.attrs);
    let stops = if definition.stops.is_empty() {
        inherited_stops
    } else {
        definition.stops.clone()
    };
    Some(SvgResolvedGradient {
        linear: definition.linear,
        attrs,
        stops,
    })
}

fn merge_svg_gradient_attrs(into: &mut Vec<(String, String)>, attrs: &[(String, String)]) {
    for (name, value) in attrs {
        if matches!(name.as_str(), "id" | "href" | "xlink:href") {
            continue;
        }
        if let Some((_, existing_value)) = into.iter_mut().find(|(existing, _)| existing == name) {
            *existing_value = value.clone();
        } else {
            into.push((name.clone(), value.clone()));
        }
    }
}

fn parse_svg_gradient_stop_css_rules(
    src: &str,
    css_vars: &[SvgCssVariable],
) -> Vec<SvgGradientStopCssRule> {
    let mut rules = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty()
            || raw.starts_with('/')
            || raw.starts_with('?')
            || raw.starts_with('!')
            || raw.trim_end().ends_with('/')
        {
            continue;
        }
        let (tag, _) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("style") {
            continue;
        }
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        parse_svg_gradient_stop_css_rules_from_css(
            src.get(pos..pos + end_rel).unwrap_or_default(),
            css_vars,
            &mut rules,
        );
        if rules.len() >= 256 {
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    rules
}

fn parse_svg_gradient_stop_css_rules_from_css(
    css: &str,
    css_vars: &[SvgCssVariable],
    out: &mut Vec<SvgGradientStopCssRule>,
) {
    let mut pos = 0usize;
    while let Some(open) = find_css_block_open(css, pos) {
        let selectors = css
            .get(pos..open)
            .unwrap_or_default()
            .rsplit(';')
            .next()
            .unwrap_or_default()
            .trim();
        let Some(close) = find_css_block_close(css, open) else {
            break;
        };

        if !selectors.starts_with('@') {
            let patch = parse_svg_gradient_stop_patch(
                css.get(open + 1..close).unwrap_or_default(),
                css_vars,
            );
            if patch.color.is_some() || patch.opacity.is_some() {
                for selector in selectors.split(',') {
                    if let Some(selector) = parse_svg_css_selector(selector) {
                        out.push(SvgGradientStopCssRule {
                            selector,
                            order: out.len(),
                            patch,
                        });
                    }
                }
            }
        }
        if out.len() >= 256 {
            break;
        }
        pos = close + 1;
    }
}

fn parse_svg_gradient_stop_patch(decls: &str, css_vars: &[SvgCssVariable]) -> SvgGradientStopPatch {
    let mut patch = SvgGradientStopPatch::default();
    for decl in decls.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        apply_svg_gradient_stop_patch_declaration(
            &mut patch,
            name.trim().to_ascii_lowercase().as_str(),
            value.trim(),
            css_vars,
        );
    }
    patch
}

fn apply_svg_gradient_stop_patch_declaration(
    patch: &mut SvgGradientStopPatch,
    name: &str,
    value: &str,
    css_vars: &[SvgCssVariable],
) {
    match name {
        "stop-color" => {
            if value.eq_ignore_ascii_case("transparent") {
                patch.opacity = Some(0.0);
            } else if let Some(color) = parse_svg_color(value, css_vars) {
                patch.color = Some(color);
            }
        }
        "stop-opacity" => {
            if let Some(opacity) = parse_svg_opacity(value, css_vars) {
                patch.opacity = Some(opacity.clamp(0.0, 1.0));
            }
        }
        _ => {}
    }
}

fn parse_svg_gradient_stops(
    body: &str,
    css_vars: &[SvgCssVariable],
    css_rules: &[SvgGradientStopCssRule],
) -> Vec<SvgGradientStop> {
    let mut stops = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.starts_with('/') {
            continue;
        }
        let (tag, attrs_src) = svg_tag_parts(raw);
        if !svg_local_name(&tag.to_ascii_lowercase()).eq_ignore_ascii_case("stop") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        if let Some(stop) = parse_svg_gradient_stop(&attrs, css_vars, css_rules) {
            stops.push(stop);
        }
        if stops.len() >= 64 {
            break;
        }
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    stops
}

fn svg_gradient_representative_color(stops: &[SvgGradientStop]) -> Option<(f32, f32, f32)> {
    if stops.is_empty() {
        return None;
    }
    if stops.len() == 1 {
        return Some(stops[0].1);
    }

    let mut weighted = (0.0f32, 0.0f32, 0.0f32);
    let mut covered = 0.0f32;
    let first = stops[0];
    if first.0 > 0.0 {
        add_svg_weighted_color(&mut weighted, first.1, first.0);
        covered += first.0;
    }
    for pair in stops.windows(2) {
        let (a_offset, a_color) = pair[0];
        let (b_offset, b_color) = pair[1];
        let span = (b_offset - a_offset).max(0.0);
        if span <= 0.0 {
            continue;
        }
        let midpoint = (
            (a_color.0 + b_color.0) * 0.5,
            (a_color.1 + b_color.1) * 0.5,
            (a_color.2 + b_color.2) * 0.5,
        );
        add_svg_weighted_color(&mut weighted, midpoint, span);
        covered += span;
    }
    let last = stops[stops.len() - 1];
    if last.0 < 1.0 {
        let span = 1.0 - last.0;
        add_svg_weighted_color(&mut weighted, last.1, span);
        covered += span;
    }
    if covered <= f32::EPSILON {
        return Some(last.1);
    }
    Some((
        (weighted.0 / covered).clamp(0.0, 1.0),
        (weighted.1 / covered).clamp(0.0, 1.0),
        (weighted.2 / covered).clamp(0.0, 1.0),
    ))
}

fn parse_svg_linear_gradient(
    attrs: &[(String, String)],
    stops: &[SvgGradientStop],
) -> Option<SvgLinearGradient> {
    let spread = parse_svg_gradient_spread(attrs)?;
    let stops = svg_native_gradient_stops(stops)?;
    if stops.len() < 2 {
        return None;
    }
    let units = match svg_attr(attrs, "gradientunits")
        .unwrap_or("objectBoundingBox")
        .to_ascii_lowercase()
        .as_str()
    {
        "userspaceonuse" => SvgGradientUnits::UserSpaceOnUse,
        _ => SvgGradientUnits::ObjectBoundingBox,
    };
    Some(SvgLinearGradient {
        units,
        spread,
        transform: svg_attr(attrs, "gradienttransform")
            .and_then(parse_svg_transform)
            .unwrap_or(SvgTransform::IDENTITY),
        x1: svg_attr(attrs, "x1")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(SvgGradientLength {
                value: 0.0,
                percent: true,
            }),
        y1: svg_attr(attrs, "y1")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(SvgGradientLength {
                value: 0.0,
                percent: true,
            }),
        x2: svg_attr(attrs, "x2")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(SvgGradientLength {
                value: 100.0,
                percent: true,
            }),
        y2: svg_attr(attrs, "y2")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(SvgGradientLength {
                value: 0.0,
                percent: true,
            }),
        stops,
    })
}

fn parse_svg_radial_gradient(
    attrs: &[(String, String)],
    stops: &[SvgGradientStop],
) -> Option<SvgRadialGradient> {
    let spread = parse_svg_gradient_spread(attrs)?;
    let stops = svg_native_gradient_stops(stops)?;
    if stops.len() < 2 {
        return None;
    }
    let units = match svg_attr(attrs, "gradientunits")
        .unwrap_or("objectBoundingBox")
        .to_ascii_lowercase()
        .as_str()
    {
        "userspaceonuse" => SvgGradientUnits::UserSpaceOnUse,
        _ => SvgGradientUnits::ObjectBoundingBox,
    };
    let default_center = SvgGradientLength {
        value: 50.0,
        percent: true,
    };
    let default_radius = SvgGradientLength {
        value: 50.0,
        percent: true,
    };
    let cx = svg_attr(attrs, "cx")
        .and_then(parse_svg_gradient_length)
        .unwrap_or(default_center);
    let cy = svg_attr(attrs, "cy")
        .and_then(parse_svg_gradient_length)
        .unwrap_or(default_center);
    Some(SvgRadialGradient {
        units,
        spread,
        transform: svg_attr(attrs, "gradienttransform")
            .and_then(parse_svg_transform)
            .unwrap_or(SvgTransform::IDENTITY),
        cx,
        cy,
        r: svg_attr(attrs, "r")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(default_radius),
        fx: svg_attr(attrs, "fx")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(cx),
        fy: svg_attr(attrs, "fy")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(cy),
        fr: svg_attr(attrs, "fr")
            .and_then(parse_svg_gradient_length)
            .unwrap_or(SvgGradientLength {
                value: 0.0,
                percent: false,
            }),
        stops,
    })
}

fn parse_svg_gradient_spread(attrs: &[(String, String)]) -> Option<SvgGradientSpread> {
    match svg_attr(attrs, "spreadmethod")
        .unwrap_or("pad")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "pad" => Some(SvgGradientSpread::Pad),
        "repeat" => Some(SvgGradientSpread::Repeat),
        "reflect" => Some(SvgGradientSpread::Reflect),
        _ => None,
    }
}

fn svg_native_gradient_stops(stops: &[SvgGradientStop]) -> Option<Vec<SvgGradientStop>> {
    if stops.len() < 2 {
        return None;
    }
    let mut normalized: Vec<(f32, (f32, f32, f32))> = Vec::with_capacity(stops.len() + 2);
    for &(offset, color) in stops.iter().take(32) {
        if !offset.is_finite() {
            continue;
        }
        let offset = offset.clamp(0.0, 1.0);
        if let Some(last) = normalized.last_mut()
            && (last.0 - offset).abs() <= 0.000_001
        {
            last.1 = color;
            continue;
        }
        normalized.push((offset, color));
    }
    if normalized.len() < 2 {
        return None;
    }
    if normalized[0].0 > 0.0 {
        normalized.insert(0, (0.0, normalized[0].1));
    }
    let last_index = normalized.len() - 1;
    if normalized[last_index].0 < 1.0 {
        normalized.push((1.0, normalized[last_index].1));
    }
    (normalized.len() >= 2).then_some(normalized)
}

fn parse_svg_gradient_length(value: &str) -> Option<SvgGradientLength> {
    let value = value.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let parsed = value[..end].parse::<f32>().ok()?;
    parsed.is_finite().then_some(SvgGradientLength {
        value: parsed,
        percent: value[end..].trim_start().starts_with('%'),
    })
}

fn add_svg_weighted_color(accum: &mut (f32, f32, f32), color: (f32, f32, f32), weight: f32) {
    accum.0 += color.0 * weight;
    accum.1 += color.1 * weight;
    accum.2 += color.2 * weight;
}

fn parse_svg_gradient_stop(
    attrs: &[(String, String)],
    css_vars: &[SvgCssVariable],
    css_rules: &[SvgGradientStopCssRule],
) -> Option<SvgGradientStop> {
    let mut color = None;
    let mut opacity = 1.0;

    apply_svg_gradient_stop_css(&mut color, &mut opacity, attrs, css_rules);
    if let Some(value) = svg_attr(attrs, "stop-color") {
        apply_svg_gradient_stop_color(&mut color, &mut opacity, value, css_vars);
    }
    if let Some(parsed) =
        svg_attr(attrs, "stop-opacity").and_then(|value| parse_svg_opacity(value, css_vars))
    {
        opacity = parsed.clamp(0.0, 1.0);
    }

    if let Some(style) = svg_attr(attrs, "style") {
        for decl in style.split(';') {
            let Some((name, value)) = decl.split_once(':') else {
                continue;
            };
            match name.trim().to_ascii_lowercase().as_str() {
                "stop-color" => {
                    apply_svg_gradient_stop_color(&mut color, &mut opacity, value.trim(), css_vars);
                }
                "stop-opacity" => {
                    if let Some(parsed) = parse_svg_opacity(value.trim(), css_vars) {
                        opacity = parsed.clamp(0.0, 1.0);
                    }
                }
                _ => {}
            }
        }
    }

    let mut color = color.unwrap_or((0.0, 0.0, 0.0));
    if opacity <= 0.001 {
        color = (1.0, 1.0, 1.0);
    } else if opacity < 0.999 {
        color = (
            color.0 * opacity + (1.0 - opacity),
            color.1 * opacity + (1.0 - opacity),
            color.2 * opacity + (1.0 - opacity),
        );
    }
    Some((
        svg_attr(attrs, "offset")
            .and_then(parse_svg_gradient_offset)
            .unwrap_or(0.0),
        color,
    ))
}

fn apply_svg_gradient_stop_css(
    color: &mut Option<SvgColor>,
    opacity: &mut f32,
    attrs: &[(String, String)],
    css_rules: &[SvgGradientStopCssRule],
) {
    if css_rules.is_empty() {
        return;
    }
    let mut matched = Vec::new();
    for rule in css_rules {
        if svg_css_selector_matches(&rule.selector, "stop", attrs, &[]) {
            matched.push(rule);
        }
    }
    matched.sort_by_key(|rule| (rule.selector.specificity, rule.order));
    for rule in matched {
        if let Some(stop_color) = rule.patch.color {
            *color = Some(stop_color);
        }
        if let Some(stop_opacity) = rule.patch.opacity {
            *opacity = stop_opacity;
        }
    }
}

fn apply_svg_gradient_stop_color(
    color: &mut Option<SvgColor>,
    opacity: &mut f32,
    value: &str,
    css_vars: &[SvgCssVariable],
) {
    if value.eq_ignore_ascii_case("transparent") {
        *opacity = 0.0;
    } else if let Some(parsed) = parse_svg_color(value, css_vars) {
        *color = Some(parsed);
    }
}

fn parse_svg_gradient_offset(value: &str) -> Option<f32> {
    let value = value.trim();
    let number = parse_svg_number(value)?;
    let offset = if value.contains('%') {
        number / 100.0
    } else {
        number
    };
    offset.is_finite().then_some(offset.clamp(0.0, 1.0))
}

fn parse_svg_markers(
    src: &str,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    css_rules: &[SvgCssRule],
    filter_shadows: &[SvgFilterShadow],
) -> Vec<SvgMarker> {
    let mut markers = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("marker") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        if self_closing {
            continue;
        }
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        let body = src.get(pos..pos + end_rel).unwrap_or_default();
        if let Some(marker) = parse_svg_marker_body(
            id,
            &attrs,
            body,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            css_rules,
            filter_shadows,
        ) {
            if !markers
                .iter()
                .any(|existing: &SvgMarker| existing.id == marker.id)
            {
                markers.push(marker);
            }
        }
        if markers.len() >= 128 {
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    markers
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_marker_body(
    id: &str,
    attrs: &[(String, String)],
    body: &str,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    css_rules: &[SvgCssRule],
    filter_shadows: &[SvgFilterShadow],
) -> Option<SvgMarker> {
    let marker_base = parse_svg_style_with_ancestors(
        "marker",
        attrs,
        SvgStyle::INITIAL,
        css_rules,
        gradients,
        patterns,
        css_vars,
        clip_paths,
        filter_shadows,
        &[],
    );
    let marker_ancestors = [svg_css_ancestor("marker", attrs)];
    let ref_x = svg_attr(attrs, "refx")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let ref_y = svg_attr(attrs, "refy")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let orient = parse_svg_marker_orient(svg_attr(attrs, "orient"));
    let view_box = parse_svg_marker_view_box(attrs);
    let units_stroke_width = !svg_attr(attrs, "markerunits")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("userSpaceOnUse"));
    let mut shapes = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        let local = svg_local_name(&tag_lower);
        let child_attrs = parse_svg_attrs(attrs_src);
        let ops = match local {
            "path" => svg_attr(&child_attrs, "d").and_then(parse_svg_path_data),
            "line" => svg_marker_line_ops(&child_attrs),
            "polyline" => svg_attr(&child_attrs, "points")
                .map(parse_svg_points)
                .filter(|points| points.len() >= 2)
                .map(|points| svg_poly_path_ops(&points, false)),
            "polygon" => svg_attr(&child_attrs, "points")
                .map(parse_svg_points)
                .filter(|points| points.len() >= 3)
                .map(|points| svg_poly_path_ops(&points, true)),
            "rect" => svg_clip_rect_ops(&child_attrs),
            "circle" => svg_clip_circle_ops(&child_attrs),
            "ellipse" => svg_clip_ellipse_ops(&child_attrs),
            _ => None,
        };
        let Some(ops) = ops.filter(|ops| !ops.is_empty()) else {
            continue;
        };
        let style = parse_svg_style_with_ancestors(
            local,
            &child_attrs,
            marker_base,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            &marker_ancestors,
        );
        if svg_style_has_marker_paint(style) {
            shapes.push(SvgMarkerShape { ops, style });
        }
        if shapes.len() >= 64 {
            break;
        }
    }
    (!shapes.is_empty()).then_some(SvgMarker {
        id: id.to_string(),
        ref_x,
        ref_y,
        orient,
        view_box,
        units_stroke_width,
        shapes,
    })
}

fn parse_svg_marker_view_box(attrs: &[(String, String)]) -> Option<SvgMarkerViewBox> {
    let view_box = parse_svg_view_box(attrs)?;
    let viewport = SvgViewport {
        w: parse_svg_positive_attr(attrs, "markerwidth").unwrap_or(3.0),
        h: parse_svg_positive_attr(attrs, "markerheight").unwrap_or(3.0),
    };
    (viewport.w > 0.0 && viewport.h > 0.0).then_some(SvgMarkerViewBox {
        view_box,
        viewport,
        preserve_aspect: parse_svg_preserve_aspect_ratio(svg_attr(attrs, "preserveaspectratio")),
    })
}

fn parse_svg_marker_orient(value: Option<&str>) -> SvgMarkerOrient {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return SvgMarkerOrient::Angle(0.0);
    };
    if value.eq_ignore_ascii_case("auto") {
        return SvgMarkerOrient::Auto;
    }
    if value.eq_ignore_ascii_case("auto-start-reverse") {
        return SvgMarkerOrient::AutoStartReverse;
    }
    parse_svg_angle_degrees(value)
        .map(SvgMarkerOrient::Angle)
        .unwrap_or(SvgMarkerOrient::Angle(0.0))
}

fn parse_svg_angle_degrees(value: &str) -> Option<f32> {
    let value = value.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let parsed = value[..end].parse::<f32>().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    let unit = value[end..].trim();
    let degrees = if unit.is_empty() || unit.eq_ignore_ascii_case("deg") {
        parsed
    } else if unit.eq_ignore_ascii_case("rad") {
        parsed.to_degrees()
    } else if unit.eq_ignore_ascii_case("grad") {
        parsed * 0.9
    } else if unit.eq_ignore_ascii_case("turn") {
        parsed * 360.0
    } else {
        return None;
    };
    degrees.is_finite().then_some(degrees)
}

fn svg_marker_line_ops(attrs: &[(String, String)]) -> Option<Vec<SvgPathOp>> {
    let x1 = svg_attr(attrs, "x1")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let y1 = svg_attr(attrs, "y1")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let x2 = svg_attr(attrs, "x2")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let y2 = svg_attr(attrs, "y2")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    Some(vec![SvgPathOp::Move(x1, y1), SvgPathOp::Line(x2, y2)])
}

fn parse_svg_clip_paths(src: &str) -> Vec<SvgClipPath> {
    let mut clip_paths = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("clippath") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        if self_closing {
            continue;
        }
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        let body = src.get(pos..pos + end_rel).unwrap_or_default();
        if let Some(clip_path) = parse_svg_clip_path_body(id, &attrs, body) {
            if !clip_paths
                .iter()
                .any(|existing: &SvgClipPath| existing.id == clip_path.id)
            {
                clip_paths.push(clip_path);
            }
        }
        if clip_paths.len() >= 128 {
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    clip_paths
}

fn parse_svg_clip_path_body(
    id: &str,
    attrs: &[(String, String)],
    body: &str,
) -> Option<SvgClipPath> {
    let mut ops = Vec::new();
    let mut fill_rule = parse_svg_clip_rule(attrs).unwrap_or(SvgFillRule::NonZero);
    let units = parse_svg_clip_path_units(attrs, "clippathunits");
    let base_transform = parse_svg_geometry_transform(attrs);
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        let local = svg_local_name(&tag_lower);
        let child_attrs = parse_svg_attrs(attrs_src);
        if let Some(rule) = parse_svg_clip_rule(&child_attrs) {
            fill_rule = rule;
        }
        if let Some(mut child_ops) = svg_clip_shape_ops(local, &child_attrs) {
            let transform = base_transform.concat(parse_svg_geometry_transform(&child_attrs));
            if !transform.is_identity() {
                transform_svg_path_ops(&mut child_ops, transform);
            }
            ops.extend(child_ops);
        }
        if ops.len() > MAX_SVG_PATH_OPS {
            return None;
        }
    }
    (!ops.is_empty()).then_some(SvgClipPath {
        id: id.to_string(),
        ops,
        fill_rule,
        units,
    })
}

fn parse_svg_masks(src: &str, css_vars: &[SvgCssVariable]) -> Vec<SvgClipPath> {
    let mut masks = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("mask") {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        if self_closing {
            continue;
        }
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        let body = src.get(pos..pos + end_rel).unwrap_or_default();
        if let Some(mask) = parse_svg_mask_body(id, &attrs, body, css_vars) {
            if !masks
                .iter()
                .any(|existing: &SvgClipPath| existing.id == mask.id)
            {
                masks.push(mask);
            }
        }
        if masks.len() >= 128 {
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    masks
}

fn parse_svg_mask_body(
    id: &str,
    attrs: &[(String, String)],
    body: &str,
    css_vars: &[SvgCssVariable],
) -> Option<SvgClipPath> {
    let mut ops = Vec::new();
    let mut fill_rule = SvgFillRule::NonZero;
    let units = parse_svg_clip_path_units(attrs, "maskcontentunits");
    let base_transform = parse_svg_geometry_transform(attrs);
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        let local = svg_local_name(&tag_lower);
        let child_attrs = parse_svg_attrs(attrs_src);
        if let Some(rule) = parse_svg_clip_rule(&child_attrs) {
            fill_rule = rule;
        }
        if !svg_mask_shape_reveals(&child_attrs, css_vars) {
            continue;
        }
        if let Some(mut child_ops) = svg_clip_shape_ops(local, &child_attrs) {
            let transform = base_transform.concat(parse_svg_geometry_transform(&child_attrs));
            if !transform.is_identity() {
                transform_svg_path_ops(&mut child_ops, transform);
            }
            ops.extend(child_ops);
        }
        if ops.len() > MAX_SVG_PATH_OPS {
            return None;
        }
    }
    (!ops.is_empty()).then_some(SvgClipPath {
        id: id.to_string(),
        ops,
        fill_rule,
        units,
    })
}

fn parse_svg_clip_path_units(attrs: &[(String, String)], name: &str) -> SvgClipPathUnits {
    match svg_attr(attrs, name).map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "objectboundingbox" => SvgClipPathUnits::ObjectBoundingBox,
        _ => SvgClipPathUnits::UserSpaceOnUse,
    }
}

fn svg_mask_shape_reveals(attrs: &[(String, String)], css_vars: &[SvgCssVariable]) -> bool {
    let mut fill = svg_attr(attrs, "fill").and_then(|value| parse_svg_color(value, css_vars));
    let mut opacity = svg_attr(attrs, "opacity")
        .and_then(|value| parse_svg_opacity(value, css_vars))
        .unwrap_or(1.0);
    let mut fill_opacity = svg_attr(attrs, "fill-opacity")
        .and_then(|value| parse_svg_opacity(value, css_vars))
        .unwrap_or(1.0);
    if let Some(style) = svg_attr(attrs, "style") {
        for decl in style.split(';') {
            let Some((name, value)) = decl.split_once(':') else {
                continue;
            };
            let name = name.trim();
            let value = value.trim();
            if name.eq_ignore_ascii_case("fill") {
                if value.eq_ignore_ascii_case("none") {
                    fill = None;
                } else if let Some(color) = parse_svg_color(value, css_vars) {
                    fill = Some(color);
                }
            } else if name.eq_ignore_ascii_case("opacity") {
                if let Some(parsed) = parse_svg_opacity(value, css_vars) {
                    opacity = parsed;
                }
            } else if name.eq_ignore_ascii_case("fill-opacity")
                && let Some(parsed) = parse_svg_opacity(value, css_vars)
            {
                fill_opacity = parsed;
            }
        }
    }
    let Some((r, g, b)) = fill else {
        return false;
    };
    let alpha = (opacity * fill_opacity).clamp(0.0, 1.0);
    if alpha <= 0.001 {
        return false;
    }
    let luminance = r * 0.2126 + g * 0.7152 + b * 0.0722;
    luminance * alpha >= 0.5
}

fn parse_svg_clip_rule(attrs: &[(String, String)]) -> Option<SvgFillRule> {
    svg_attr(attrs, "clip-rule")
        .or_else(|| svg_attr(attrs, "fill-rule"))
        .and_then(parse_svg_fill_rule)
        .or_else(|| {
            svg_attr(attrs, "style").and_then(|style| {
                style.split(';').find_map(|decl| {
                    let (name, value) = decl.split_once(':')?;
                    let name = name.trim();
                    if name.eq_ignore_ascii_case("clip-rule")
                        || name.eq_ignore_ascii_case("fill-rule")
                    {
                        parse_svg_fill_rule(value)
                    } else {
                        None
                    }
                })
            })
        })
}

fn svg_clip_rect_ops(attrs: &[(String, String)]) -> Option<Vec<SvgPathOp>> {
    let w = svg_attr(attrs, "width").and_then(parse_svg_number)?;
    let h = svg_attr(attrs, "height").and_then(parse_svg_number)?;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let x = svg_attr(attrs, "x")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let y = svg_attr(attrs, "y")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let (rx, ry) = SvgRectRadius {
        rx: svg_attr(attrs, "rx").and_then(parse_svg_number),
        ry: svg_attr(attrs, "ry").and_then(parse_svg_number),
    }
    .resolved();
    Some(svg_rect_path_ops(x, y, w, h, rx, ry))
}

fn svg_clip_circle_ops(attrs: &[(String, String)]) -> Option<Vec<SvgPathOp>> {
    let r = svg_attr(attrs, "r").and_then(parse_svg_number)?;
    (r > 0.0).then(|| {
        svg_ellipse_path_ops(
            svg_attr(attrs, "cx")
                .and_then(parse_svg_number)
                .unwrap_or(0.0),
            svg_attr(attrs, "cy")
                .and_then(parse_svg_number)
                .unwrap_or(0.0),
            r,
            r,
        )
    })
}

fn svg_clip_ellipse_ops(attrs: &[(String, String)]) -> Option<Vec<SvgPathOp>> {
    let rx = svg_attr(attrs, "rx").and_then(parse_svg_number)?;
    let ry = svg_attr(attrs, "ry").and_then(parse_svg_number)?;
    (rx > 0.0 && ry > 0.0).then(|| {
        svg_ellipse_path_ops(
            svg_attr(attrs, "cx")
                .and_then(parse_svg_number)
                .unwrap_or(0.0),
            svg_attr(attrs, "cy")
                .and_then(parse_svg_number)
                .unwrap_or(0.0),
            rx,
            ry,
        )
    })
}

fn svg_clip_shape_ops(local: &str, attrs: &[(String, String)]) -> Option<Vec<SvgPathOp>> {
    match local {
        "path" => svg_attr(attrs, "d").and_then(parse_svg_path_data),
        "rect" => svg_clip_rect_ops(attrs),
        "circle" => svg_clip_circle_ops(attrs),
        "ellipse" => svg_clip_ellipse_ops(attrs),
        "polygon" => svg_attr(attrs, "points")
            .map(parse_svg_points)
            .filter(|points| points.len() >= 3)
            .map(|points| svg_poly_path_ops(&points, true)),
        _ => None,
    }
    .filter(|ops| !ops.is_empty())
}

fn parse_svg_geometry_transform(attrs: &[(String, String)]) -> SvgTransform {
    let mut transform = SvgTransform::IDENTITY;
    if let Some(parsed) = svg_attr(attrs, "transform").and_then(parse_svg_transform) {
        transform = transform.concat(parsed);
    }
    if let Some(style) = svg_attr(attrs, "style") {
        for decl in style.split(';') {
            let Some((name, value)) = decl.split_once(':') else {
                continue;
            };
            if name.trim().eq_ignore_ascii_case("transform")
                && let Some(parsed) = parse_svg_transform(value.trim())
            {
                transform = transform.concat(parsed);
            }
        }
    }
    transform
}

fn transform_svg_path_ops(ops: &mut [SvgPathOp], transform: SvgTransform) {
    for op in ops {
        *op = match *op {
            SvgPathOp::Move(x, y) => {
                let (x, y) = transform.apply_point(x, y);
                SvgPathOp::Move(x, y)
            }
            SvgPathOp::Line(x, y) => {
                let (x, y) = transform.apply_point(x, y);
                SvgPathOp::Line(x, y)
            }
            SvgPathOp::Cubic(x1, y1, x2, y2, x, y) => {
                let (x1, y1) = transform.apply_point(x1, y1);
                let (x2, y2) = transform.apply_point(x2, y2);
                let (x, y) = transform.apply_point(x, y);
                SvgPathOp::Cubic(x1, y1, x2, y2, x, y)
            }
            SvgPathOp::Quad(x1, y1, x, y) => {
                let (x1, y1) = transform.apply_point(x1, y1);
                let (x, y) = transform.apply_point(x, y);
                SvgPathOp::Quad(x1, y1, x, y)
            }
            SvgPathOp::Close => SvgPathOp::Close,
        };
    }
}

fn svg_rect_path_ops(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> Vec<SvgPathOp> {
    let rx = rx.min(w * 0.5).max(0.0);
    let ry = ry.min(h * 0.5).max(0.0);
    if rx <= 0.0 || ry <= 0.0 {
        return vec![
            SvgPathOp::Move(x, y),
            SvgPathOp::Line(x + w, y),
            SvgPathOp::Line(x + w, y + h),
            SvgPathOp::Line(x, y + h),
            SvgPathOp::Close,
        ];
    }
    let kx = rx * 0.5523;
    let ky = ry * 0.5523;
    let x1 = x + w;
    let y1 = y + h;
    vec![
        SvgPathOp::Move(x + rx, y),
        SvgPathOp::Line(x1 - rx, y),
        SvgPathOp::Cubic(x1 - rx + kx, y, x1, y + ry - ky, x1, y + ry),
        SvgPathOp::Line(x1, y1 - ry),
        SvgPathOp::Cubic(x1, y1 - ry + ky, x1 - rx + kx, y1, x1 - rx, y1),
        SvgPathOp::Line(x + rx, y1),
        SvgPathOp::Cubic(x + rx - kx, y1, x, y1 - ry + ky, x, y1 - ry),
        SvgPathOp::Line(x, y + ry),
        SvgPathOp::Cubic(x, y + ry - ky, x + rx - kx, y, x + rx, y),
        SvgPathOp::Close,
    ]
}

fn svg_ellipse_path_ops(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<SvgPathOp> {
    let k = 0.552_284_8;
    vec![
        SvgPathOp::Move(cx - rx, cy),
        SvgPathOp::Cubic(cx - rx, cy - ry * k, cx - rx * k, cy - ry, cx, cy - ry),
        SvgPathOp::Cubic(cx + rx * k, cy - ry, cx + rx, cy - ry * k, cx + rx, cy),
        SvgPathOp::Cubic(cx + rx, cy + ry * k, cx + rx * k, cy + ry, cx, cy + ry),
        SvgPathOp::Cubic(cx - rx * k, cy + ry, cx - rx, cy + ry * k, cx - rx, cy),
        SvgPathOp::Close,
    ]
}

fn svg_poly_path_ops(points: &[(f32, f32)], closed: bool) -> Vec<SvgPathOp> {
    let mut ops = Vec::with_capacity(points.len() + usize::from(closed));
    if let Some(&(x, y)) = points.first() {
        ops.push(SvgPathOp::Move(x, y));
        for &(x, y) in &points[1..] {
            ops.push(SvgPathOp::Line(x, y));
        }
        if closed {
            ops.push(SvgPathOp::Close);
        }
    }
    ops
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_style_with_ancestors(
    tag: &str,
    attrs: &[(String, String)],
    mut style: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> SvgStyle {
    let scoped_css_vars = svg_css_vars_for_element(css_vars, css_rules, ancestors, tag, attrs);

    // `opacity` is not inherited as a normal property, but ancestor group
    // opacity multiplies the final child alpha. Keep that inherited factor
    // separate so same-element CSS/attribute declarations override each other
    // normally before we compose with the parent.
    let inherited_opacity = style.opacity;
    style.opacity = 1.0;
    let inherited_font_size = style.font_size;
    let inherited_display_visible = style.display_visible;

    apply_svg_color_attr(&mut style, svg_attr(attrs, "color"), &scoped_css_vars);
    apply_svg_transform_attr(&mut style, svg_attr(attrs, "transform"));
    apply_svg_clip_path_attr(&mut style, svg_attr(attrs, "clip-path"), clip_paths);
    apply_svg_mask_attr(&mut style, svg_attr(attrs, "mask"), clip_paths);
    apply_svg_paint_attr(
        &mut style,
        "fill",
        svg_attr(attrs, "fill"),
        gradients,
        patterns,
        &scoped_css_vars,
    );
    apply_svg_paint_attr(
        &mut style,
        "stroke",
        svg_attr(attrs, "stroke"),
        gradients,
        patterns,
        &scoped_css_vars,
    );
    apply_svg_visibility_attr(
        &mut style,
        "display",
        svg_attr(attrs, "display"),
        inherited_display_visible,
    );
    apply_svg_visibility_attr(
        &mut style,
        "visibility",
        svg_attr(attrs, "visibility"),
        inherited_display_visible,
    );
    apply_svg_filter_attr(
        &mut style,
        svg_attr(attrs, "filter"),
        filter_shadows,
        &scoped_css_vars,
    );
    apply_svg_fill_rule_attr(&mut style, svg_attr(attrs, "fill-rule"));
    apply_svg_paint_order_attr(&mut style, svg_attr(attrs, "paint-order"), &scoped_css_vars);
    apply_svg_line_cap_attr(&mut style, svg_attr(attrs, "stroke-linecap"));
    apply_svg_line_join_attr(&mut style, svg_attr(attrs, "stroke-linejoin"));
    apply_svg_miter_limit_attr(
        &mut style,
        svg_attr(attrs, "stroke-miterlimit"),
        &scoped_css_vars,
    );
    apply_svg_dash_array_attr(
        &mut style,
        svg_attr(attrs, "stroke-dasharray"),
        &scoped_css_vars,
    );
    apply_svg_dash_offset_attr(
        &mut style,
        svg_attr(attrs, "stroke-dashoffset"),
        &scoped_css_vars,
    );
    apply_svg_vector_effect_attr(&mut style, svg_attr(attrs, "vector-effect"));
    apply_svg_font_weight_attr(&mut style, svg_attr(attrs, "font-weight"), &scoped_css_vars);
    apply_svg_font_slant_attr(&mut style, svg_attr(attrs, "font-style"), &scoped_css_vars);
    apply_svg_font_family_attr(&mut style, svg_attr(attrs, "font-family"), &scoped_css_vars);
    apply_svg_font_size_attr(&mut style, svg_attr(attrs, "font-size"), &scoped_css_vars);
    apply_svg_text_anchor_attr(&mut style, svg_attr(attrs, "text-anchor"), &scoped_css_vars);
    apply_svg_letter_spacing_attr(
        &mut style,
        svg_attr(attrs, "letter-spacing"),
        &scoped_css_vars,
    );
    apply_svg_text_decoration_attr(
        &mut style,
        svg_attr(attrs, "text-decoration"),
        &scoped_css_vars,
    );
    apply_svg_text_decoration_attr(
        &mut style,
        svg_attr(attrs, "text-decoration-line"),
        &scoped_css_vars,
    );
    apply_svg_dominant_baseline_attr(
        &mut style,
        svg_attr(attrs, "dominant-baseline"),
        &scoped_css_vars,
    );
    apply_svg_dominant_baseline_attr(
        &mut style,
        svg_attr(attrs, "alignment-baseline"),
        &scoped_css_vars,
    );
    if let Some(width) = svg_attr(attrs, "stroke-width")
        .and_then(|value| parse_svg_css_number(value, &scoped_css_vars))
    {
        style.stroke_width = width.max(0.0);
    }
    apply_svg_opacity_attr(
        &mut style,
        "opacity",
        svg_attr(attrs, "opacity"),
        &scoped_css_vars,
    );
    apply_svg_opacity_attr(
        &mut style,
        "fill-opacity",
        svg_attr(attrs, "fill-opacity"),
        &scoped_css_vars,
    );
    apply_svg_opacity_attr(
        &mut style,
        "stroke-opacity",
        svg_attr(attrs, "stroke-opacity"),
        &scoped_css_vars,
    );

    apply_svg_css_styles(
        &mut style,
        tag,
        attrs,
        css_rules,
        ancestors,
        &scoped_css_vars,
        gradients,
        patterns,
        clip_paths,
        filter_shadows,
        inherited_font_size,
        inherited_display_visible,
    );

    if let Some(style_attr) = svg_attr(attrs, "style") {
        for decl in style_attr.split(';') {
            let Some((name, value)) = decl.split_once(':') else {
                continue;
            };
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            apply_svg_style_declaration(
                &mut style,
                name.as_str(),
                value,
                gradients,
                patterns,
                &scoped_css_vars,
                clip_paths,
                filter_shadows,
                inherited_font_size,
                inherited_display_visible,
            );
        }
    }
    style.opacity = (inherited_opacity * style.opacity).clamp(0.0, 1.0);
    refresh_svg_effective_visibility(&mut style);
    if style.stroke_width <= 0.0 {
        style.stroke = None;
    }
    style
}

fn parse_svg_css_rules(css: &str, out: &mut Vec<SvgCssRule>) {
    let mut pos = 0usize;
    while let Some(open) = find_css_block_open(css, pos) {
        let selectors = css
            .get(pos..open)
            .unwrap_or_default()
            .rsplit(';')
            .next()
            .unwrap_or_default()
            .trim();
        let Some(close) = find_css_block_close(css, open) else {
            break;
        };

        if !selectors.starts_with('@') {
            let decls = css.get(open + 1..close).unwrap_or_default().to_string();
            for selector in selectors.split(',') {
                if let Some(selector) = parse_svg_css_selector(selector) {
                    out.push(SvgCssRule {
                        selector,
                        order: out.len(),
                        decls: decls.clone(),
                    });
                }
            }
        }
        pos = close + 1;
    }
}

fn find_css_block_open(css: &str, mut pos: usize) -> Option<usize> {
    let bytes = css.as_bytes();
    let mut quote = 0u8;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if quote != 0 {
            if byte == b'\\' {
                pos = (pos + 2).min(bytes.len());
                continue;
            }
            if byte == quote {
                quote = 0;
            }
            pos += 1;
            continue;
        }
        if byte == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            pos += 2;
            while pos + 1 < bytes.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                pos += 1;
            }
            pos = (pos + 2).min(bytes.len());
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = byte;
            pos += 1;
            continue;
        }
        if byte == b'{' {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn find_css_block_close(css: &str, open: usize) -> Option<usize> {
    let bytes = css.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut pos = open + 1;
    let mut depth = 1usize;
    let mut quote = 0u8;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if quote != 0 {
            if byte == b'\\' {
                pos = (pos + 2).min(bytes.len());
                continue;
            }
            if byte == quote {
                quote = 0;
            }
            pos += 1;
            continue;
        }
        if byte == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            pos += 2;
            while pos + 1 < bytes.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                pos += 1;
            }
            pos = (pos + 2).min(bytes.len());
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = byte;
            pos += 1;
            continue;
        }
        if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(pos);
            }
        }
        pos += 1;
    }
    None
}

fn parse_svg_css_selector(selector: &str) -> Option<SvgCssSelector> {
    let selector = selector.trim();
    if selector.is_empty()
        || selector
            .chars()
            .any(|ch| matches!(ch, '+' | '~' | '[' | ']' | ':'))
    {
        return None;
    }

    let mut pos = 0usize;
    let mut parts = Vec::new();
    let mut pending_relation = SvgCssRelation::Descendant;
    while pos < selector.len() {
        let had_whitespace = skip_svg_css_selector_whitespace(selector, &mut pos);
        if pos >= selector.len() {
            break;
        }
        if selector.as_bytes()[pos] == b'>' {
            if parts.is_empty() {
                return None;
            }
            pos += 1;
            skip_svg_css_selector_whitespace(selector, &mut pos);
            if pos >= selector.len() || selector.as_bytes()[pos] == b'>' {
                return None;
            }
            pending_relation = SvgCssRelation::Child;
        } else if !parts.is_empty() {
            if !had_whitespace {
                return None;
            }
            pending_relation = SvgCssRelation::Descendant;
        }

        let mut part = parse_svg_css_selector_part(selector, &mut pos)?;
        part.relation = if parts.is_empty() {
            SvgCssRelation::Descendant
        } else {
            pending_relation
        };
        parts.push(part);
        if parts.len() > 6 {
            return None;
        }
        pending_relation = SvgCssRelation::Descendant;
    }

    if parts.is_empty() {
        return None;
    }
    let specificity = parts.iter().fold(0u16, |specificity, part| {
        specificity
            .saturating_add(u16::from(part.tag.is_some()))
            .saturating_add((part.classes.len() as u16).saturating_mul(10))
            .saturating_add(u16::from(part.id.is_some()).saturating_mul(100))
    });
    Some(SvgCssSelector { parts, specificity })
}

fn skip_svg_css_selector_whitespace(selector: &str, pos: &mut usize) -> bool {
    let start = *pos;
    while selector
        .as_bytes()
        .get(*pos)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        *pos += 1;
    }
    *pos != start
}

fn parse_svg_css_selector_part(selector: &str, pos: &mut usize) -> Option<SvgCssSelectorPart> {
    if *pos >= selector.len() {
        return None;
    }
    let mut tag = None;
    let mut id = None;
    let mut classes = Vec::new();
    let mut universal = false;
    if selector.as_bytes()[*pos] == b'*' {
        universal = true;
        *pos += 1;
    } else if selector
        .as_bytes()
        .get(*pos)
        .is_some_and(|byte| byte.is_ascii_alphabetic())
    {
        let start = *pos;
        while selector
            .as_bytes()
            .get(*pos)
            .is_some_and(|byte| svg_css_ident_byte(*byte))
        {
            *pos += 1;
        }
        let tag_name = selector[start..*pos].to_ascii_lowercase();
        tag = Some(svg_local_name(&tag_name).to_string());
    }

    while *pos < selector.len() {
        let marker = selector.as_bytes()[*pos];
        if marker != b'.' && marker != b'#' {
            break;
        }
        *pos += 1;
        let start = *pos;
        while selector
            .as_bytes()
            .get(*pos)
            .is_some_and(|byte| svg_css_ident_byte(*byte))
        {
            *pos += 1;
        }
        if start == *pos {
            return None;
        }
        let ident = selector[start..*pos].to_string();
        if marker == b'#' {
            if id.is_some() {
                return None;
            }
            id = Some(ident);
        } else {
            classes.push(ident);
            if classes.len() > 8 {
                return None;
            }
        }
    }

    if tag.is_none() && id.is_none() && classes.is_empty() && !universal {
        return None;
    }
    Some(SvgCssSelectorPart {
        tag,
        id,
        classes,
        relation: SvgCssRelation::Descendant,
    })
}

fn svg_css_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn parse_svg_style_patch(
    decls: &str,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    inherited_font_size: f32,
) -> SvgStylePatch {
    let mut patch = SvgStylePatch::default();
    for decl in decls.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = clean_svg_css_keyword_value(value);
        match name.as_str() {
            "color" => patch.color = parse_svg_color(value, css_vars),
            "fill" => {
                if parse_svg_current_color_paint(value, css_vars) {
                    patch.fill_current_color = Some(true);
                    patch.fill_context = Some(None);
                    patch.fill_gradient = Some(None);
                    patch.fill_pattern = Some(None);
                    if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
                        patch.fill_opacity = Some(alpha);
                    }
                } else if let Some(context) = parse_svg_context_paint(value, css_vars) {
                    patch.fill = Some(None);
                    patch.fill_current_color = Some(false);
                    patch.fill_context = Some(Some(context));
                    patch.fill_gradient = Some(None);
                    patch.fill_pattern = Some(None);
                    if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
                        patch.fill_opacity = Some(alpha);
                    }
                } else {
                    let gradient_ref = parse_svg_paint_gradient_ref(value, gradients, css_vars);
                    let pattern_ref = parse_svg_paint_pattern_ref(value, patterns, css_vars);
                    let paint = parse_svg_paint(value, gradients, css_vars).and_then(|paint| {
                        if paint.is_some() {
                            Some(paint)
                        } else {
                            pattern_ref
                                .and_then(|index| patterns.get(index))
                                .map(|pattern| Some(pattern.color))
                                .or(Some(None))
                        }
                    });
                    if let Some(paint) = paint {
                        patch.fill = Some(paint);
                        patch.fill_current_color = Some(false);
                        patch.fill_context = Some(None);
                        patch.fill_gradient =
                            Some(paint.is_some().then_some(gradient_ref).flatten());
                        patch.fill_pattern = Some(paint.is_some().then_some(pattern_ref).flatten());
                        if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
                            patch.fill_opacity = Some(alpha);
                        }
                    }
                }
            }
            "stroke" => {
                if parse_svg_current_color_paint(value, css_vars) {
                    patch.stroke_current_color = Some(true);
                    patch.stroke_context = Some(None);
                    patch.stroke_gradient = Some(None);
                    if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
                        patch.stroke_opacity = Some(alpha);
                    }
                } else if let Some(context) = parse_svg_context_paint(value, css_vars) {
                    patch.stroke = Some(None);
                    patch.stroke_current_color = Some(false);
                    patch.stroke_context = Some(Some(context));
                    patch.stroke_gradient = Some(None);
                    if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
                        patch.stroke_opacity = Some(alpha);
                    }
                } else {
                    let gradient_ref = parse_svg_paint_gradient_ref(value, gradients, css_vars);
                    if let Some(paint) = parse_svg_paint(value, gradients, css_vars) {
                        patch.stroke = Some(paint);
                        patch.stroke_gradient =
                            Some(paint.is_some().then_some(gradient_ref).flatten());
                        patch.stroke_current_color = Some(false);
                        patch.stroke_context = Some(None);
                        if let Some(alpha) = parse_svg_paint_alpha(value, css_vars) {
                            patch.stroke_opacity = Some(alpha);
                        }
                    }
                }
            }
            "display" => patch.display_visible = parse_svg_display_visible(value),
            "visibility" => patch.visibility_visible = parse_svg_visibility_visible(value),
            "filter" => patch.shadow = parse_svg_filter_shadow(value, filter_shadows, css_vars),
            "transform" => patch.transform = parse_svg_transform(value),
            "clip-path" => patch.clip_path = parse_svg_clip_path_ref(value, clip_paths),
            "mask" => patch.mask_path = parse_svg_mask_ref(value, clip_paths),
            "fill-rule" => patch.fill_rule = parse_svg_fill_rule(value),
            "paint-order" => patch.paint_order = parse_svg_paint_order(value, css_vars),
            "stroke-width" => {
                if let Some(width) = parse_svg_css_number(value, css_vars) {
                    patch.stroke_width = Some(width.max(0.0));
                }
            }
            "vector-effect" => patch.non_scaling_stroke = parse_svg_vector_effect(value),
            "stroke-linecap" => patch.line_cap = parse_svg_line_cap(value),
            "stroke-linejoin" => patch.line_join = parse_svg_line_join(value),
            "stroke-miterlimit" => patch.miter_limit = parse_svg_css_miter_limit(value, css_vars),
            "stroke-dasharray" => patch.dash = parse_svg_css_dash_array(value, css_vars),
            "stroke-dashoffset" => patch.dash_offset = parse_svg_css_number(value, css_vars),
            "font-weight" => patch.font_weight = parse_svg_font_weight(value, css_vars),
            "font-style" => patch.font_slant = parse_svg_font_slant(value, css_vars),
            "font-family" => patch.font_family = parse_svg_font_family(value, css_vars),
            "font-size" => {
                patch.font_size = parse_svg_font_size_value(value, inherited_font_size, css_vars)
                    .map(|size| size.clamp(1.0, 96.0));
            }
            "text-anchor" => patch.text_anchor = parse_svg_text_anchor_value(value, css_vars),
            "letter-spacing" => patch.letter_spacing = parse_svg_letter_spacing(value, css_vars),
            "text-decoration" | "text-decoration-line" => {
                patch.text_decoration = parse_svg_text_decoration(value, css_vars);
            }
            "dominant-baseline" | "alignment-baseline" => {
                patch.dominant_baseline = parse_svg_dominant_baseline(value, css_vars);
            }
            "opacity" => patch.opacity = parse_svg_opacity(value, css_vars),
            "fill-opacity" => patch.fill_opacity = parse_svg_opacity(value, css_vars),
            "stroke-opacity" => patch.stroke_opacity = parse_svg_opacity(value, css_vars),
            _ => {}
        }
    }
    patch
}

#[allow(clippy::too_many_arguments)]
fn apply_svg_css_styles(
    style: &mut SvgStyle,
    tag: &str,
    attrs: &[(String, String)],
    css_rules: &[SvgCssRule],
    ancestors: &[SvgCssAncestor],
    css_vars: &[SvgCssVariable],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    inherited_font_size: f32,
    inherited_display_visible: bool,
) {
    if css_rules.is_empty() {
        return;
    }
    let matched = svg_matching_css_rules(tag, attrs, css_rules, ancestors);
    for rule in matched {
        let patch = parse_svg_style_patch(
            &rule.decls,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            inherited_font_size,
        );
        apply_svg_style_patch(style, patch, inherited_display_visible);
    }
}

fn svg_css_vars_for_element(
    css_vars: &[SvgCssVariable],
    css_rules: &[SvgCssRule],
    ancestors: &[SvgCssAncestor],
    tag: &str,
    attrs: &[(String, String)],
) -> Vec<SvgCssVariable> {
    let mut scoped_vars = css_vars.to_vec();
    for index in 0..ancestors.len() {
        let ancestor = &ancestors[index];
        apply_svg_css_variable_rules(
            &mut scoped_vars,
            ancestor.tag.as_str(),
            &ancestor.attrs,
            css_rules,
            &ancestors[..index],
        );
        apply_svg_inline_variable_decls(&mut scoped_vars, &ancestor.attrs);
    }
    apply_svg_css_variable_rules(&mut scoped_vars, tag, attrs, css_rules, ancestors);
    apply_svg_inline_variable_decls(&mut scoped_vars, attrs);
    scoped_vars
}

fn apply_svg_css_variable_rules(
    vars: &mut Vec<SvgCssVariable>,
    tag: &str,
    attrs: &[(String, String)],
    css_rules: &[SvgCssRule],
    ancestors: &[SvgCssAncestor],
) {
    if css_rules.is_empty() {
        return;
    }
    let matched = svg_matching_css_rules(tag, attrs, css_rules, ancestors);
    for rule in matched {
        parse_svg_css_variable_decls_override(&rule.decls, vars);
    }
}

fn apply_svg_inline_variable_decls(vars: &mut Vec<SvgCssVariable>, attrs: &[(String, String)]) {
    let Some(style) = svg_attr(attrs, "style") else {
        return;
    };
    parse_svg_css_variable_decls_override(style, vars);
}

fn svg_matching_css_rules<'a>(
    tag: &str,
    attrs: &[(String, String)],
    css_rules: &'a [SvgCssRule],
    ancestors: &[SvgCssAncestor],
) -> Vec<&'a SvgCssRule> {
    let mut matched = Vec::new();
    for rule in css_rules {
        if svg_css_selector_matches(&rule.selector, tag, attrs, ancestors) {
            matched.push(rule);
        }
    }
    matched.sort_by_key(|rule| (rule.selector.specificity, rule.order));
    matched
}

fn svg_css_selector_matches(
    selector: &SvgCssSelector,
    tag: &str,
    attrs: &[(String, String)],
    ancestors: &[SvgCssAncestor],
) -> bool {
    let Some(current) = selector.parts.last() else {
        return false;
    };
    if !svg_css_selector_part_matches(current, tag, attrs) {
        return false;
    }
    if selector.parts.len() == 1 {
        return true;
    }

    let mut ancestor_limit = ancestors.len();
    for part_index in (0..selector.parts.len() - 1).rev() {
        let relation = selector.parts[part_index + 1].relation;
        let part = &selector.parts[part_index];
        match relation {
            SvgCssRelation::Child => {
                if ancestor_limit == 0 {
                    return false;
                }
                ancestor_limit -= 1;
                let ancestor = &ancestors[ancestor_limit];
                if !svg_css_selector_part_matches(part, ancestor.tag.as_str(), &ancestor.attrs) {
                    return false;
                }
            }
            SvgCssRelation::Descendant => {
                let Some(found) = (0..ancestor_limit).rev().find(|&index| {
                    let ancestor = &ancestors[index];
                    svg_css_selector_part_matches(part, ancestor.tag.as_str(), &ancestor.attrs)
                }) else {
                    return false;
                };
                ancestor_limit = found;
            }
        }
    }
    true
}

fn svg_css_selector_part_matches(
    part: &SvgCssSelectorPart,
    tag: &str,
    attrs: &[(String, String)],
) -> bool {
    if let Some(selector_tag) = &part.tag
        && selector_tag != tag
    {
        return false;
    }
    if let Some(selector_id) = &part.id
        && svg_attr(attrs, "id").is_none_or(|id| id != selector_id)
    {
        return false;
    }
    if part.classes.is_empty() {
        return true;
    }
    let Some(class_attr) = svg_attr(attrs, "class") else {
        return false;
    };
    part.classes.iter().all(|selector_class| {
        class_attr
            .split_ascii_whitespace()
            .any(|class_name| class_name == selector_class)
    })
}

fn svg_css_ancestor(tag: &str, attrs: &[(String, String)]) -> SvgCssAncestor {
    SvgCssAncestor {
        tag: tag.to_string(),
        attrs: attrs.to_vec(),
    }
}

fn apply_svg_style_patch(
    style: &mut SvgStyle,
    patch: SvgStylePatch,
    inherited_display_visible: bool,
) {
    if let Some(color) = patch.color {
        apply_svg_color(style, color);
    }
    if patch.fill_current_color == Some(true) {
        style.fill = Some(style.color);
        style.fill_gradient = None;
        style.fill_pattern = None;
        style.fill_current_color = true;
        style.fill_context = None;
    }
    if let Some(fill) = patch.fill {
        style.fill = fill;
        style.fill_current_color = false;
        style.fill_context = None;
    }
    if let Some(fill_context) = patch.fill_context {
        style.fill_context = fill_context;
        if fill_context.is_some() {
            style.fill = None;
            style.fill_gradient = None;
            style.fill_pattern = None;
            style.fill_current_color = false;
        }
    }
    if let Some(fill_gradient) = patch.fill_gradient {
        style.fill_gradient = fill_gradient;
    }
    if let Some(fill_pattern) = patch.fill_pattern {
        style.fill_pattern = fill_pattern;
        if fill_pattern.is_some() {
            style.fill_gradient = None;
        }
    }
    if patch.stroke_current_color == Some(true) {
        style.stroke = Some(style.color);
        style.stroke_gradient = None;
        style.stroke_current_color = true;
        style.stroke_context = None;
    }
    if let Some(stroke) = patch.stroke {
        style.stroke = stroke;
        if stroke.is_none() {
            style.stroke_gradient = None;
        }
        style.stroke_current_color = false;
        style.stroke_context = None;
    }
    if let Some(stroke_context) = patch.stroke_context {
        style.stroke_context = stroke_context;
        if stroke_context.is_some() {
            style.stroke = None;
            style.stroke_gradient = None;
            style.stroke_current_color = false;
        }
    }
    if let Some(stroke_gradient) = patch.stroke_gradient {
        style.stroke_gradient = stroke_gradient;
    }
    if let Some(stroke_width) = patch.stroke_width {
        style.stroke_width = stroke_width;
    }
    if let Some(non_scaling_stroke) = patch.non_scaling_stroke {
        style.non_scaling_stroke = non_scaling_stroke;
    }
    if let Some(opacity) = patch.opacity {
        style.opacity = opacity.clamp(0.0, 1.0);
    }
    if let Some(fill_opacity) = patch.fill_opacity {
        style.fill_opacity = fill_opacity;
    }
    if let Some(stroke_opacity) = patch.stroke_opacity {
        style.stroke_opacity = stroke_opacity;
    }
    if let Some(display_visible) = patch.display_visible {
        style.display_visible = inherited_display_visible && display_visible;
    }
    if let Some(visibility_visible) = patch.visibility_visible {
        style.visibility_visible = visibility_visible;
    }
    if let Some(shadow) = patch.shadow {
        style.shadow = shadow;
    }
    if let Some(clip_path) = patch.clip_path {
        style.clip_path = clip_path;
    }
    if let Some(mask_path) = patch.mask_path {
        style.mask_path = mask_path;
    }
    if let Some(transform) = patch.transform {
        style.transform = style.transform.concat(transform);
    }
    if let Some(dash) = patch.dash {
        style.dash = dash;
    }
    if let Some(dash_offset) = patch.dash_offset {
        style.dash.offset = dash_offset;
    }
    if let Some(line_cap) = patch.line_cap {
        style.line_cap = line_cap;
    }
    if let Some(line_join) = patch.line_join {
        style.line_join = line_join;
    }
    if let Some(miter_limit) = patch.miter_limit {
        style.miter_limit = Some(miter_limit);
    }
    if let Some(fill_rule) = patch.fill_rule {
        style.fill_rule = fill_rule;
    }
    if let Some(paint_order) = patch.paint_order {
        style.paint_order = paint_order;
    }
    if let Some(font_size) = patch.font_size {
        style.font_size = font_size;
    }
    if let Some(text_anchor) = patch.text_anchor {
        style.text_anchor = text_anchor;
    }
    if let Some(font_weight) = patch.font_weight {
        style.font_weight = font_weight;
    }
    if let Some(font_slant) = patch.font_slant {
        style.font_slant = font_slant;
    }
    if let Some(font_family) = patch.font_family {
        style.font_family = font_family;
    }
    if let Some(dominant_baseline) = patch.dominant_baseline {
        style.dominant_baseline = dominant_baseline;
    }
    if let Some(letter_spacing) = patch.letter_spacing {
        style.letter_spacing = letter_spacing;
    }
    if let Some(text_decoration) = patch.text_decoration {
        style.text_decoration = text_decoration;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_svg_style_declaration(
    style: &mut SvgStyle,
    name: &str,
    value: &str,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    inherited_font_size: f32,
    inherited_display_visible: bool,
) {
    let value = clean_svg_css_keyword_value(value);
    match name {
        "color" => apply_svg_color_attr(style, Some(value), css_vars),
        "fill" | "stroke" => {
            apply_svg_paint_attr(style, name, Some(value), gradients, patterns, css_vars);
        }
        "display" | "visibility" => {
            apply_svg_visibility_attr(style, name, Some(value), inherited_display_visible)
        }
        "filter" => apply_svg_filter_attr(style, Some(value), filter_shadows, css_vars),
        "transform" => apply_svg_transform_attr(style, Some(value)),
        "clip-path" => apply_svg_clip_path_attr(style, Some(value), clip_paths),
        "mask" => apply_svg_mask_attr(style, Some(value), clip_paths),
        "fill-rule" => apply_svg_fill_rule_attr(style, Some(value)),
        "paint-order" => apply_svg_paint_order_attr(style, Some(value), css_vars),
        "stroke-width" => {
            if let Some(width) = parse_svg_css_number(value, css_vars) {
                style.stroke_width = width.max(0.0);
            }
        }
        "stroke-linecap" => apply_svg_line_cap_attr(style, Some(value)),
        "stroke-linejoin" => apply_svg_line_join_attr(style, Some(value)),
        "stroke-miterlimit" => apply_svg_miter_limit_attr(style, Some(value), css_vars),
        "stroke-dasharray" => apply_svg_dash_array_attr(style, Some(value), css_vars),
        "stroke-dashoffset" => apply_svg_dash_offset_attr(style, Some(value), css_vars),
        "vector-effect" => apply_svg_vector_effect_attr(style, Some(value)),
        "font-weight" => apply_svg_font_weight_attr(style, Some(value), css_vars),
        "font-style" => apply_svg_font_slant_attr(style, Some(value), css_vars),
        "font-family" => apply_svg_font_family_attr(style, Some(value), css_vars),
        "font-size" => {
            if let Some(size) = parse_svg_font_size_value(value, inherited_font_size, css_vars)
                .map(|size| size.clamp(1.0, 96.0))
            {
                style.font_size = size;
            }
        }
        "text-anchor" => apply_svg_text_anchor_attr(style, Some(value), css_vars),
        "letter-spacing" => apply_svg_letter_spacing_attr(style, Some(value), css_vars),
        "text-decoration" | "text-decoration-line" => {
            apply_svg_text_decoration_attr(style, Some(value), css_vars);
        }
        "dominant-baseline" | "alignment-baseline" => {
            apply_svg_dominant_baseline_attr(style, Some(value), css_vars);
        }
        "opacity" | "fill-opacity" | "stroke-opacity" => {
            apply_svg_opacity_attr(style, name, Some(value), css_vars);
        }
        _ => {}
    }
}

fn apply_svg_opacity_attr(
    style: &mut SvgStyle,
    name: &str,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    let Some(opacity) = value.and_then(|value| parse_svg_opacity(value, css_vars)) else {
        return;
    };
    match name {
        "opacity" => {
            style.opacity = opacity;
        }
        "fill-opacity" => style.fill_opacity = opacity,
        "stroke-opacity" => style.stroke_opacity = opacity,
        _ => {}
    }
}

fn apply_svg_transform_attr(style: &mut SvgStyle, value: Option<&str>) {
    let Some(transform) = value.and_then(parse_svg_transform) else {
        return;
    };
    style.transform = style.transform.concat(transform);
}

fn apply_svg_clip_path_attr(style: &mut SvgStyle, value: Option<&str>, clip_paths: &[SvgClipPath]) {
    if let Some(clip_path) = value.and_then(|value| parse_svg_clip_path_ref(value, clip_paths)) {
        style.clip_path = clip_path;
    }
}

fn apply_svg_mask_attr(style: &mut SvgStyle, value: Option<&str>, clip_paths: &[SvgClipPath]) {
    if let Some(mask_path) = value.and_then(|value| parse_svg_mask_ref(value, clip_paths)) {
        style.mask_path = mask_path;
    }
}

fn parse_svg_clip_path_ref(value: &str, clip_paths: &[SvgClipPath]) -> Option<Option<usize>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    let id = parse_svg_paint_url_id(value)?;
    Some(clip_paths.iter().position(|clip_path| clip_path.id == id))
}

fn parse_svg_mask_ref(value: &str, clip_paths: &[SvgClipPath]) -> Option<Option<usize>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    let id = parse_svg_paint_url_id(value)?;
    Some(clip_paths.iter().rposition(|clip_path| clip_path.id == id))
}

fn parse_svg_marker_ref(value: Option<&str>, markers: &[SvgMarker]) -> Option<SvgMarkerRef> {
    parse_svg_marker_ref_declaration(value?, markers, &[]).flatten()
}

fn parse_svg_marker_ref_attr(
    attrs: &[(String, String)],
    attr: &str,
    markers: &[SvgMarker],
    fallback: Option<SvgMarkerRef>,
) -> Option<SvgMarkerRef> {
    if let Some(value) = svg_attr(attrs, attr) {
        return parse_svg_marker_ref(Some(value), markers);
    }
    fallback
}

fn parse_svg_marker_refs_for_element(
    tag: &str,
    attrs: &[(String, String)],
    css_rules: &[SvgCssRule],
    css_vars: &[SvgCssVariable],
    markers: &[SvgMarker],
    ancestors: &[SvgCssAncestor],
) -> SvgMarkerRefs {
    let marker = parse_svg_marker_ref(svg_attr(attrs, "marker"), markers);
    let mut refs = SvgMarkerRefs {
        start: parse_svg_marker_ref_attr(attrs, "marker-start", markers, marker),
        mid: parse_svg_marker_ref_attr(attrs, "marker-mid", markers, marker),
        end: parse_svg_marker_ref_attr(attrs, "marker-end", markers, marker),
    };

    let scoped_css_vars = svg_css_vars_for_element(css_vars, css_rules, ancestors, tag, attrs);
    for rule in svg_matching_css_rules(tag, attrs, css_rules, ancestors) {
        apply_svg_marker_declarations(&mut refs, &rule.decls, markers, &scoped_css_vars);
    }
    if let Some(style_attr) = svg_attr(attrs, "style") {
        apply_svg_marker_declarations(&mut refs, style_attr, markers, &scoped_css_vars);
    }
    refs
}

fn apply_svg_marker_declarations(
    refs: &mut SvgMarkerRefs,
    decls: &str,
    markers: &[SvgMarker],
    css_vars: &[SvgCssVariable],
) {
    for decl in decls.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let value = clean_svg_css_keyword_value(value);
        let marker_ref = match parse_svg_marker_ref_declaration(value, markers, css_vars) {
            Some(marker_ref) => marker_ref,
            None => continue,
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "marker" => {
                refs.start = marker_ref;
                refs.mid = marker_ref;
                refs.end = marker_ref;
            }
            "marker-start" => refs.start = marker_ref,
            "marker-mid" => refs.mid = marker_ref,
            "marker-end" => refs.end = marker_ref,
            _ => {}
        }
    }
}

fn parse_svg_marker_ref_declaration(
    value: &str,
    markers: &[SvgMarker],
    css_vars: &[SvgCssVariable],
) -> Option<Option<SvgMarkerRef>> {
    let value = value.trim();
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_marker_ref_declaration(&resolved, markers, css_vars);
    }
    if value.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    let id = parse_svg_paint_url_id(value)?;
    Some(Some(SvgMarkerRef {
        index: markers.iter().position(|marker| marker.id == id),
    }))
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_use_elements(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    markers: &[SvgMarker],
    reusable_defs: &[SvgReusableDef],
    ancestors: &[SvgCssAncestor],
    depth: usize,
) -> Vec<SvgElement> {
    if depth >= 8 {
        return Vec::new();
    }
    let Some(id) = svg_use_href(attrs) else {
        return Vec::new();
    };
    let Some(def) = reusable_defs.iter().find(|def| def.id == id) else {
        return Vec::new();
    };
    let base = parse_svg_use_base_style(
        attrs,
        inherited,
        css_rules,
        gradients,
        patterns,
        css_vars,
        clip_paths,
        filter_shadows,
        ancestors,
    );
    match def.tag.as_str() {
        "g" | "symbol" | "a" => def.body.as_deref().map_or_else(Vec::new, |body| {
            let inherited = if def.tag == "symbol" {
                parse_svg_symbol_use_base_style(attrs, base, def)
            } else {
                base
            };
            let group_base = parse_svg_style_with_ancestors(
                def.tag.as_str(),
                &def.attrs,
                inherited,
                css_rules,
                gradients,
                patterns,
                css_vars,
                clip_paths,
                filter_shadows,
                ancestors,
            );
            let mut def_ancestors = ancestors.to_vec();
            def_ancestors.push(svg_css_ancestor(def.tag.as_str(), &def.attrs));
            let mut elements = parse_svg_reusable_body_elements(
                body,
                group_base,
                css_rules,
                gradients,
                patterns,
                css_vars,
                clip_paths,
                filter_shadows,
                markers,
                reusable_defs,
                &def_ancestors,
                depth + 1,
            );
            if def.tag == "a" {
                apply_svg_links(
                    &mut elements,
                    parse_svg_anchor_link(&def.attrs).flatten().as_ref(),
                );
            }
            elements
        }),
        "use" => parse_svg_use_elements(
            &def.attrs,
            base,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            markers,
            reusable_defs,
            ancestors,
            depth + 1,
        ),
        "text" => def.body.as_deref().map_or_else(Vec::new, |body| {
            parse_svg_text_elements(
                &def.attrs,
                body,
                base,
                css_rules,
                gradients,
                patterns,
                css_vars,
                clip_paths,
                filter_shadows,
                ancestors,
            )
        }),
        _ => parse_svg_reusable_shape(
            def.tag.as_str(),
            &def.attrs,
            base,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            markers,
            ancestors,
        )
        .into_iter()
        .collect(),
    }
}

fn svg_use_href(attrs: &[(String, String)]) -> Option<&str> {
    svg_attr(attrs, "href")
        .or_else(|| svg_attr(attrs, "xlink:href"))
        .and_then(parse_svg_fragment_href)
}

fn parse_svg_fragment_href(value: &str) -> Option<&str> {
    let value = value.trim();
    if let Some(id) = value.strip_prefix('#') {
        return (!id.is_empty()).then_some(id);
    }
    parse_svg_paint_url_id(value)
}

fn parse_svg_symbol_use_base_style(
    attrs: &[(String, String)],
    mut style: SvgStyle,
    def: &SvgReusableDef,
) -> SvgStyle {
    let Some(view_box) = def.view_box else {
        return style;
    };
    let viewport = SvgViewport {
        w: parse_svg_positive_attr(attrs, "width").unwrap_or(view_box.w),
        h: parse_svg_positive_attr(attrs, "height").unwrap_or(view_box.h),
    };
    if let Some(transform) =
        svg_view_box_to_viewport_transform(view_box, viewport, def.preserve_aspect)
    {
        style.transform = style.transform.concat(transform);
    }
    style
}

fn svg_view_box_to_viewport_transform(
    view_box: SvgViewBox,
    viewport: SvgViewport,
    preserve_aspect: SvgPreserveAspectRatio,
) -> Option<SvgTransform> {
    let raw_sx = viewport.w / view_box.w;
    let raw_sy = viewport.h / view_box.h;
    if ![raw_sx, raw_sy].iter().all(|value| value.is_finite()) || raw_sx <= 0.0 || raw_sy <= 0.0 {
        return None;
    }
    match preserve_aspect.mode {
        SvgAspectScaleMode::None => Some(SvgTransform {
            a: raw_sx,
            d: raw_sy,
            e: -view_box.x * raw_sx,
            f: -view_box.y * raw_sy,
            ..SvgTransform::IDENTITY
        }),
        SvgAspectScaleMode::Meet | SvgAspectScaleMode::Slice => {
            let scale = if preserve_aspect.mode == SvgAspectScaleMode::Slice {
                raw_sx.max(raw_sy)
            } else {
                raw_sx.min(raw_sy)
            };
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            let content_w = view_box.w * scale;
            let content_h = view_box.h * scale;
            let offset_x = (viewport.w - content_w) * preserve_aspect.align_x;
            let offset_y = (viewport.h - content_h) * preserve_aspect.align_y;
            Some(SvgTransform {
                a: scale,
                d: scale,
                e: offset_x - view_box.x * scale,
                f: offset_y - view_box.y * scale,
                ..SvgTransform::IDENTITY
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_use_base_style(
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    ancestors: &[SvgCssAncestor],
) -> SvgStyle {
    let mut style = parse_svg_style_with_ancestors(
        "use",
        attrs,
        inherited,
        css_rules,
        gradients,
        patterns,
        css_vars,
        clip_paths,
        filter_shadows,
        ancestors,
    );
    let x = svg_attr(attrs, "x")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    let y = svg_attr(attrs, "y")
        .and_then(parse_svg_number)
        .unwrap_or(0.0);
    if x != 0.0 || y != 0.0 {
        style.transform = style.transform.concat(SvgTransform::translate(x, y));
    }
    style
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_reusable_body_elements(
    body: &str,
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    markers: &[SvgMarker],
    reusable_defs: &[SvgReusableDef],
    ancestors: &[SvgCssAncestor],
    depth: usize,
) -> Vec<SvgElement> {
    if depth >= 8 {
        return Vec::new();
    }
    let mut elements = Vec::new();
    let mut pos = 0usize;
    let mut skip_depth = 0usize;
    let mut style_stack = vec![inherited];
    let mut link_stack: Vec<Option<LinkTarget>> = vec![None];
    let mut selector_stack = ancestors.to_vec();
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let closing = raw.starts_with('/');
        let raw = if closing { raw[1..].trim_start() } else { raw };
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        if tag.is_empty() {
            continue;
        }
        let tag_name_lower = tag.to_ascii_lowercase();
        let tag_lower = svg_local_name(&tag_name_lower);
        if closing {
            if skip_depth > 0 {
                skip_depth = skip_depth.saturating_sub(1);
            } else if matches!(tag_lower, "g" | "symbol" | "a") && style_stack.len() > 1 {
                style_stack.pop();
                link_stack.pop();
                selector_stack.pop();
            }
            continue;
        }
        if skip_depth > 0 {
            if !self_closing {
                skip_depth = skip_depth.saturating_add(1);
            }
            continue;
        }
        if matches!(
            tag_lower,
            "defs" | "style" | "script" | "foreignobject" | "iframe" | "object" | "embed"
        ) {
            if !self_closing {
                skip_depth = 1;
            }
            continue;
        }

        let attrs = parse_svg_attrs(attrs_src);
        let inherited = style_stack.last().copied().unwrap_or(SvgStyle::INITIAL);
        let inherited_link = link_stack.last().cloned().unwrap_or(None);
        match tag_lower {
            "g" | "symbol" | "a" => {
                let container_style = parse_svg_style_with_ancestors(
                    tag_lower,
                    &attrs,
                    inherited,
                    css_rules,
                    gradients,
                    patterns,
                    css_vars,
                    clip_paths,
                    filter_shadows,
                    &selector_stack,
                );
                if !self_closing {
                    style_stack.push(container_style);
                    let next_link = if tag_lower == "a" {
                        parse_svg_anchor_link(&attrs).unwrap_or(inherited_link)
                    } else {
                        inherited_link
                    };
                    link_stack.push(next_link);
                    selector_stack.push(svg_css_ancestor(tag_lower, &attrs));
                }
            }
            "use" => {
                let mut used = parse_svg_use_elements(
                    &attrs,
                    inherited,
                    css_rules,
                    gradients,
                    patterns,
                    css_vars,
                    clip_paths,
                    filter_shadows,
                    markers,
                    reusable_defs,
                    &selector_stack,
                    depth + 1,
                );
                apply_svg_links(&mut used, inherited_link.as_ref());
                elements.append(&mut used);
            }
            "text" if !self_closing => {
                let needle = format!("</{tag_name_lower}");
                if let Some(end_rel) =
                    find_ascii_case_insensitive(body.get(pos..).unwrap_or_default(), &needle)
                {
                    let text_src = body.get(pos..pos + end_rel).unwrap_or_default();
                    let mut text_elements = parse_svg_text_elements(
                        &attrs,
                        text_src,
                        inherited,
                        css_rules,
                        gradients,
                        patterns,
                        css_vars,
                        clip_paths,
                        filter_shadows,
                        &selector_stack,
                    );
                    apply_svg_links(&mut text_elements, inherited_link.as_ref());
                    elements.extend(text_elements);
                    if let Some(tag_end) = body.get(pos + end_rel..).and_then(|s| s.find('>')) {
                        pos += end_rel + tag_end + 1;
                    }
                }
            }
            _ => {
                if let Some(element) = parse_svg_reusable_shape(
                    tag_lower,
                    &attrs,
                    inherited,
                    css_rules,
                    gradients,
                    patterns,
                    css_vars,
                    clip_paths,
                    filter_shadows,
                    markers,
                    &selector_stack,
                ) {
                    push_svg_element(&mut elements, element, inherited_link.as_ref());
                }
            }
        }
        if elements.len() > 1024 {
            break;
        }
    }
    elements
}

#[allow(clippy::too_many_arguments)]
fn parse_svg_reusable_shape(
    tag_lower: &str,
    attrs: &[(String, String)],
    inherited: SvgStyle,
    css_rules: &[SvgCssRule],
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
    clip_paths: &[SvgClipPath],
    filter_shadows: &[SvgFilterShadow],
    markers: &[SvgMarker],
    ancestors: &[SvgCssAncestor],
) -> Option<SvgElement> {
    match tag_lower {
        "rect" => parse_svg_rect(
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        )
        .map(SvgElement::Rect),
        "circle" => parse_svg_circle(
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        )
        .map(SvgElement::Ellipse),
        "ellipse" => parse_svg_ellipse(
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        )
        .map(SvgElement::Ellipse),
        "line" => parse_svg_line(
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            markers,
            ancestors,
        )
        .map(SvgElement::Line),
        "polyline" => parse_svg_poly(
            attrs,
            inherited,
            false,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            markers,
            ancestors,
        )
        .map(SvgElement::Polyline),
        "polygon" => parse_svg_poly(
            attrs,
            inherited,
            true,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            markers,
            ancestors,
        )
        .map(SvgElement::Polygon),
        "path" => parse_svg_path(
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            markers,
            ancestors,
        )
        .map(SvgElement::Path),
        "image" => parse_svg_embedded_image(
            attrs,
            inherited,
            css_rules,
            gradients,
            patterns,
            css_vars,
            clip_paths,
            filter_shadows,
            ancestors,
        )
        .map(SvgElement::Image),
        _ => None,
    }
}

fn apply_svg_visibility_attr(
    style: &mut SvgStyle,
    target: &str,
    value: Option<&str>,
    inherited_display_visible: bool,
) {
    let Some(value) = value else {
        return;
    };
    match target {
        "display" => {
            if let Some(display_visible) = parse_svg_display_visible(value) {
                style.display_visible = inherited_display_visible && display_visible;
            }
        }
        "visibility" => {
            if let Some(visibility_visible) = parse_svg_visibility_visible(value) {
                style.visibility_visible = visibility_visible;
            }
        }
        _ => {}
    }
}

fn parse_svg_display_visible(value: &str) -> Option<bool> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(false);
    }
    if SVG_VISIBLE_DISPLAY_VALUES
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
    {
        return Some(true);
    }
    None
}

const SVG_VISIBLE_DISPLAY_VALUES: &[&str] = &[
    "block",
    "compact",
    "contents",
    "flex",
    "flow-root",
    "grid",
    "inherit",
    "initial",
    "inline",
    "inline-block",
    "inline-flex",
    "inline-grid",
    "inline-table",
    "list-item",
    "marker",
    "revert",
    "revert-layer",
    "run-in",
    "table",
    "table-caption",
    "table-cell",
    "table-column",
    "table-column-group",
    "table-footer-group",
    "table-header-group",
    "table-row",
    "table-row-group",
    "unset",
];

fn parse_svg_visibility_visible(value: &str) -> Option<bool> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("visible") || value.eq_ignore_ascii_case("initial") {
        return Some(true);
    }
    if value.eq_ignore_ascii_case("hidden") || value.eq_ignore_ascii_case("collapse") {
        return Some(false);
    }
    None
}

fn refresh_svg_effective_visibility(style: &mut SvgStyle) {
    style.visible = style.display_visible && style.visibility_visible && style.opacity > 0.001;
}

fn parse_svg_filter_shadows(src: &str, css_vars: &[SvgCssVariable]) -> Vec<SvgFilterShadow> {
    let mut shadows = Vec::new();
    let mut pos = 0usize;
    while let Some(open_rel) = src.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if src.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = src.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = src.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = src.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let self_closing = raw.trim_end().ends_with('/');
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if !svg_local_name(&tag_lower).eq_ignore_ascii_case("filter") || self_closing {
            continue;
        }
        let attrs = parse_svg_attrs(attrs_src);
        let Some(id) = svg_attr(&attrs, "id").filter(|id| !id.is_empty()) else {
            continue;
        };
        let needle = format!("</{tag_lower}");
        let Some(end_rel) = src
            .get(pos..)
            .and_then(|tail| find_ascii_case_insensitive(tail, &needle))
        else {
            continue;
        };
        let body = src.get(pos..pos + end_rel).unwrap_or_default();
        if let Some(shadow) = parse_svg_filter_shadow_body(body, css_vars)
            && !shadows
                .iter()
                .any(|existing: &SvgFilterShadow| existing.id == id)
        {
            shadows.push(SvgFilterShadow {
                id: id.to_string(),
                shadow,
            });
        }
        if shadows.len() >= 128 {
            break;
        }
        if let Some(tag_end) = src.get(pos + end_rel..).and_then(|s| s.find('>')) {
            pos += end_rel + tag_end + 1;
        }
    }
    shadows
}

fn parse_svg_filter_shadow_body(body: &str, css_vars: &[SvgCssVariable]) -> Option<SvgShadow> {
    let mut pos = 0usize;
    while let Some(open_rel) = body.get(pos..).and_then(|s| s.find('<')) {
        let open = pos + open_rel;
        if body.get(open..).is_some_and(|s| s.starts_with("<!--")) {
            let Some(end_rel) = body.get(open + 4..).and_then(|s| s.find("-->")) else {
                break;
            };
            pos = open + end_rel + 7;
            continue;
        }
        let Some(close_rel) = body.get(open..).and_then(|s| s.find('>')) else {
            break;
        };
        let close = open + close_rel;
        let raw = body.get(open + 1..close).unwrap_or_default().trim();
        pos = close + 1;
        if raw.is_empty() || raw.starts_with('/') || raw.starts_with('?') || raw.starts_with('!') {
            continue;
        }
        let (tag, attrs_src) = svg_tag_parts(raw);
        let tag_lower = tag.to_ascii_lowercase();
        if svg_local_name(&tag_lower).eq_ignore_ascii_case("fedropshadow") {
            return Some(parse_svg_fe_drop_shadow(
                &parse_svg_attrs(attrs_src),
                css_vars,
            ));
        }
    }
    None
}

fn parse_svg_fe_drop_shadow(attrs: &[(String, String)], css_vars: &[SvgCssVariable]) -> SvgShadow {
    let mut shadow = SvgShadow::FALLBACK;
    if let Some(dx) = svg_attr(attrs, "dx").and_then(parse_svg_filter_length) {
        shadow.dx = dx;
    }
    if let Some(dy) = svg_attr(attrs, "dy").and_then(parse_svg_filter_length) {
        shadow.dy = dy;
    }
    if let Some(color) =
        svg_attr(attrs, "flood-color").and_then(|value| parse_svg_color(value, css_vars))
    {
        shadow.color = color;
    }
    if let Some(opacity) =
        svg_attr(attrs, "flood-opacity").and_then(|value| parse_svg_opacity(value, css_vars))
    {
        shadow.opacity = opacity;
    }
    if let Some(style) = svg_attr(attrs, "style") {
        apply_svg_fe_drop_shadow_style(&mut shadow, style, css_vars);
    }
    shadow.opacity = shadow.opacity.clamp(0.0, 1.0);
    shadow
}

fn apply_svg_fe_drop_shadow_style(
    shadow: &mut SvgShadow,
    style: &str,
    css_vars: &[SvgCssVariable],
) {
    for decl in style.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "flood-color" => {
                if let Some(color) = parse_svg_color(value, css_vars) {
                    shadow.color = color;
                }
            }
            "flood-opacity" => {
                if let Some(opacity) = parse_svg_opacity(value, css_vars) {
                    shadow.opacity = opacity;
                }
            }
            _ => {}
        }
    }
}

fn apply_svg_filter_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    filter_shadows: &[SvgFilterShadow],
    css_vars: &[SvgCssVariable],
) {
    let Some(value) = value else {
        return;
    };
    if let Some(shadow) = parse_svg_filter_shadow(value, filter_shadows, css_vars) {
        style.shadow = shadow;
    }
}

fn parse_svg_filter_shadow(
    value: &str,
    filter_shadows: &[SvgFilterShadow],
    css_vars: &[SvgCssVariable],
) -> Option<Option<SvgShadow>> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    if let Some(shadow) = parse_svg_css_drop_shadow(value, css_vars) {
        return Some(Some(shadow));
    }
    if let Some(id) = parse_svg_paint_url_id(value) {
        let shadow = filter_shadows
            .iter()
            .find(|filter| filter.id == id)
            .map(|filter| filter.shadow);
        return Some(shadow);
    }
    if value.to_ascii_lowercase().contains("drop-shadow") {
        return Some(Some(SvgShadow::FALLBACK));
    }
    None
}

fn parse_svg_css_drop_shadow(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgShadow> {
    let lower = value.to_ascii_lowercase();
    let marker = "drop-shadow(";
    let start = lower.find(marker)? + marker.len();
    let rest = value.get(start..)?;
    let close = find_svg_css_function_close(rest)?;
    parse_svg_drop_shadow_args(rest.get(..close)?, css_vars)
}

fn parse_svg_drop_shadow_args(args: &str, css_vars: &[SvgCssVariable]) -> Option<SvgShadow> {
    let mut lengths = Vec::new();
    let mut color = None;
    let mut opacity = None;
    for part in split_svg_top_level_whitespace(args) {
        if let Some(parsed) = parse_svg_color(part, css_vars) {
            color = Some(parsed);
            opacity = parse_svg_paint_alpha(part, css_vars).or(opacity);
            continue;
        }
        if lengths.len() < 3
            && let Some(length) = parse_svg_filter_length(part)
        {
            lengths.push(length);
        }
    }
    (lengths.len() >= 2).then_some(SvgShadow {
        dx: lengths[0],
        dy: lengths[1],
        color: color.unwrap_or((0.0, 0.0, 0.0)),
        opacity: opacity.unwrap_or(1.0).clamp(0.0, 1.0),
    })
}

fn split_svg_top_level_whitespace(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = None;
    let mut depth = 0usize;
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => {
                depth += 1;
                start.get_or_insert(idx);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                start.get_or_insert(idx);
            }
            _ if ch.is_ascii_whitespace() && depth == 0 => {
                if let Some(part_start) = start.take() {
                    if part_start < idx {
                        parts.push(value[part_start..idx].trim());
                    }
                    if parts.len() >= 16 {
                        return parts;
                    }
                }
            }
            _ => {
                start.get_or_insert(idx);
            }
        }
    }
    if let Some(part_start) = start
        && part_start < value.len()
    {
        parts.push(value[part_start..].trim());
    }
    parts.retain(|part| !part.is_empty());
    parts
}

fn parse_svg_filter_length(value: &str) -> Option<f32> {
    let value = value.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let number = value[..end].parse::<f32>().ok()?;
    if !number.is_finite() {
        return None;
    }
    let unit = value[end..].trim_start();
    if unit.starts_with("em") {
        Some(number * 12.0)
    } else if unit.starts_with("ex") {
        Some(number * 6.0)
    } else if unit.starts_with('%') {
        Some(number / 100.0)
    } else {
        Some(number)
    }
}

fn apply_svg_fill_rule_attr(style: &mut SvgStyle, value: Option<&str>) {
    if let Some(fill_rule) = value.and_then(parse_svg_fill_rule) {
        style.fill_rule = fill_rule;
    }
}

fn parse_svg_fill_rule(value: &str) -> Option<SvgFillRule> {
    match value.trim().to_ascii_lowercase().as_str() {
        "nonzero" => Some(SvgFillRule::NonZero),
        "evenodd" => Some(SvgFillRule::EvenOdd),
        _ => None,
    }
}

fn apply_svg_paint_order_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(paint_order) = value.and_then(|value| parse_svg_paint_order(value, css_vars)) {
        style.paint_order = paint_order;
    }
}

fn parse_svg_paint_order(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgPaintOrder> {
    let value = value.trim();
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_paint_order(&resolved, css_vars);
    }
    if value.eq_ignore_ascii_case("normal") {
        return Some(SvgPaintOrder::NORMAL);
    }

    let mut layers = [SvgPaintLayer::Fill; 3];
    let mut len = 0usize;
    let mut seen_fill = false;
    let mut seen_stroke = false;
    let mut seen_markers = false;
    for token in value.split_ascii_whitespace() {
        let layer = match token.to_ascii_lowercase().as_str() {
            "fill" if !seen_fill => {
                seen_fill = true;
                SvgPaintLayer::Fill
            }
            "stroke" if !seen_stroke => {
                seen_stroke = true;
                SvgPaintLayer::Stroke
            }
            "markers" if !seen_markers => {
                seen_markers = true;
                SvgPaintLayer::Markers
            }
            _ => return None,
        };
        if len >= layers.len() {
            return None;
        }
        layers[len] = layer;
        len += 1;
    }
    if len == 0 {
        return None;
    }
    for layer in SvgPaintOrder::NORMAL.layers {
        let already_seen = match layer {
            SvgPaintLayer::Fill => seen_fill,
            SvgPaintLayer::Stroke => seen_stroke,
            SvgPaintLayer::Markers => seen_markers,
        };
        if !already_seen {
            layers[len] = layer;
            len += 1;
        }
    }
    Some(SvgPaintOrder { layers })
}

fn apply_svg_line_cap_attr(style: &mut SvgStyle, value: Option<&str>) {
    if let Some(line_cap) = value.and_then(parse_svg_line_cap) {
        style.line_cap = line_cap;
    }
}

fn parse_svg_line_cap(value: &str) -> Option<SvgLineCap> {
    match value.trim().to_ascii_lowercase().as_str() {
        "butt" => Some(SvgLineCap::Butt),
        "round" => Some(SvgLineCap::Round),
        "square" => Some(SvgLineCap::Square),
        _ => None,
    }
}

fn apply_svg_line_join_attr(style: &mut SvgStyle, value: Option<&str>) {
    if let Some(line_join) = value.and_then(parse_svg_line_join) {
        style.line_join = line_join;
    }
}

fn parse_svg_line_join(value: &str) -> Option<SvgLineJoin> {
    match value.trim().to_ascii_lowercase().as_str() {
        "miter" | "miter-clip" | "arcs" => Some(SvgLineJoin::Miter),
        "round" => Some(SvgLineJoin::Round),
        "bevel" => Some(SvgLineJoin::Bevel),
        _ => None,
    }
}

fn apply_svg_miter_limit_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(miter_limit) = value.and_then(|value| parse_svg_css_miter_limit(value, css_vars)) {
        style.miter_limit = Some(miter_limit);
    }
}

fn parse_svg_miter_limit(value: &str) -> Option<f32> {
    let miter_limit = parse_svg_number(value)?;
    (miter_limit.is_finite() && miter_limit >= 1.0).then_some(miter_limit)
}

fn parse_svg_css_miter_limit(value: &str, css_vars: &[SvgCssVariable]) -> Option<f32> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_css_miter_limit(&resolved, css_vars);
    }
    parse_svg_miter_limit(value)
}

fn apply_svg_dash_array_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(dash) = value.and_then(|value| parse_svg_css_dash_array(value, css_vars)) {
        style.dash = dash;
    }
}

fn parse_svg_dash_array(value: &str) -> Option<SvgDashPattern> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(SvgDashPattern::NONE);
    }
    let nums = parse_svg_number_list(value);
    if nums.is_empty() {
        return None;
    }
    if nums.len() > SvgDashPattern::NONE.values.len() {
        return None;
    }
    if nums.iter().any(|num| *num < 0.0) {
        return None;
    }
    if nums.iter().all(|num| *num <= 0.0) {
        return Some(SvgDashPattern::NONE);
    }
    let mut dash = SvgDashPattern::NONE;
    for num in nums {
        dash.values[dash.len as usize] = num;
        dash.len += 1;
    }
    Some(dash)
}

fn parse_svg_css_dash_array(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgDashPattern> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_css_dash_array(&resolved, css_vars);
    }
    parse_svg_dash_array(value)
}

fn apply_svg_dash_offset_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(offset) = value.and_then(|value| parse_svg_css_number(value, css_vars)) {
        style.dash.offset = offset;
    }
}

fn apply_svg_vector_effect_attr(style: &mut SvgStyle, value: Option<&str>) {
    if let Some(non_scaling_stroke) = value.and_then(parse_svg_vector_effect) {
        style.non_scaling_stroke = non_scaling_stroke;
    }
}

fn parse_svg_vector_effect(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "non-scaling-stroke" => Some(true),
        "none" => Some(false),
        _ => None,
    }
}

fn apply_svg_font_weight_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(weight) = value.and_then(|value| parse_svg_font_weight(value, css_vars)) {
        style.font_weight = weight;
    }
}

fn parse_svg_font_weight(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgFontWeight> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_font_weight(&resolved, css_vars);
    }
    match value.to_ascii_lowercase().as_str() {
        "normal" | "lighter" => Some(SvgFontWeight::Normal),
        "bold" | "bolder" => Some(SvgFontWeight::Bold),
        _ => parse_svg_number(value).map(|weight| {
            if weight >= 600.0 {
                SvgFontWeight::Bold
            } else {
                SvgFontWeight::Normal
            }
        }),
    }
}

fn apply_svg_font_slant_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(slant) = value.and_then(|value| parse_svg_font_slant(value, css_vars)) {
        style.font_slant = slant;
    }
}

fn parse_svg_font_slant(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgFontSlant> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_font_slant(&resolved, css_vars);
    }
    let value = value.to_ascii_lowercase();
    if value == "normal" {
        Some(SvgFontSlant::Normal)
    } else if value == "italic" || value.starts_with("oblique") {
        Some(SvgFontSlant::Italic)
    } else {
        None
    }
}

fn apply_svg_font_family_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(family) = value.and_then(|value| parse_svg_font_family(value, css_vars)) {
        style.font_family = family;
    }
}

fn parse_svg_font_family(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgFontFamily> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_font_family(&resolved, css_vars);
    }
    if value.is_empty() {
        return None;
    }
    for family in split_svg_font_family_list(value).into_iter().take(16) {
        if svg_font_family_is_monospace(family) {
            return Some(SvgFontFamily::Mono);
        }
        if svg_font_family_is_body(family) {
            return Some(SvgFontFamily::Body);
        }
    }
    Some(SvgFontFamily::Body)
}

fn split_svg_font_family_list(value: &str) -> Vec<&str> {
    split_svg_css_top_level_commas(value)
        .into_iter()
        .map(|family| family.trim().trim_matches('"').trim_matches('\'').trim())
        .filter(|family| !family.is_empty())
        .collect()
}

fn svg_font_family_is_monospace(family: &str) -> bool {
    let normalized = family
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .replace([' ', '_'], "-");
    matches!(
        normalized.as_str(),
        "monospace"
            | "ui-monospace"
            | "sfmono-regular"
            | "sf-mono"
            | "menlo"
            | "monaco"
            | "consolas"
            | "courier"
            | "courier-new"
            | "liberation-mono"
            | "dejavu-sans-mono"
            | "source-code-pro"
            | "fira-code"
            | "jetbrains-mono"
            | "cascadia-code"
            | "cascadia-mono"
    )
}

fn svg_font_family_is_body(family: &str) -> bool {
    let normalized = family
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
        .replace([' ', '_'], "-");
    matches!(
        normalized.as_str(),
        "sans-serif"
            | "serif"
            | "system-ui"
            | "ui-sans-serif"
            | "inter"
            | "arial"
            | "helvetica"
            | "helvetica-neue"
            | "times"
            | "times-new-roman"
            | "georgia"
    )
}

fn apply_svg_letter_spacing_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(letter_spacing) = value.and_then(|value| parse_svg_letter_spacing(value, css_vars))
    {
        style.letter_spacing = letter_spacing;
    }
}

fn parse_svg_letter_spacing(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgTextSpacing> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_letter_spacing(&resolved, css_vars);
    }
    if value.eq_ignore_ascii_case("normal") {
        return Some(SvgTextSpacing::ZERO);
    }

    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let number = value[..end].parse::<f32>().ok()?;
    if !number.is_finite() {
        return None;
    }
    let unit = value[end..].trim_start().to_ascii_lowercase();
    if unit.starts_with("em") {
        Some(SvgTextSpacing::Em(number))
    } else if unit.starts_with("ex") {
        Some(SvgTextSpacing::Ex(number))
    } else if unit.starts_with('%') {
        Some(SvgTextSpacing::Percent(number))
    } else {
        Some(SvgTextSpacing::Points(number))
    }
}

fn apply_svg_text_decoration_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(decoration) = value.and_then(|value| parse_svg_text_decoration(value, css_vars)) {
        style.text_decoration = decoration;
    }
}

fn parse_svg_text_decoration(
    value: &str,
    css_vars: &[SvgCssVariable],
) -> Option<SvgTextDecoration> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_text_decoration(&resolved, css_vars);
    }
    let mut decoration = SvgTextDecoration::NONE;
    let mut saw_known = false;
    for token in value.split_ascii_whitespace() {
        match token.trim_matches(',').to_ascii_lowercase().as_str() {
            "none" => return Some(SvgTextDecoration::NONE),
            "underline" => {
                decoration = decoration.with(SvgTextDecoration::UNDERLINE);
                saw_known = true;
            }
            "overline" => {
                decoration = decoration.with(SvgTextDecoration::OVERLINE);
                saw_known = true;
            }
            "line-through" => {
                decoration = decoration.with(SvgTextDecoration::LINE_THROUGH);
                saw_known = true;
            }
            _ => {}
        }
    }
    saw_known.then_some(decoration)
}

fn apply_svg_dominant_baseline_attr(
    style: &mut SvgStyle,
    value: Option<&str>,
    css_vars: &[SvgCssVariable],
) {
    if let Some(baseline) = value.and_then(|value| parse_svg_dominant_baseline(value, css_vars)) {
        style.dominant_baseline = baseline;
    }
}

fn parse_svg_dominant_baseline(
    value: &str,
    css_vars: &[SvgCssVariable],
) -> Option<SvgDominantBaseline> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_dominant_baseline(&resolved, css_vars);
    }
    match value.to_ascii_lowercase().as_str() {
        "auto" | "alphabetic" | "baseline" | "no-change" | "reset-size" | "use-script" => {
            Some(SvgDominantBaseline::Auto)
        }
        "middle" | "central" | "mathematical" => Some(SvgDominantBaseline::Middle),
        "hanging" => Some(SvgDominantBaseline::Hanging),
        "text-before-edge" | "before-edge" => Some(SvgDominantBaseline::TextBeforeEdge),
        "text-after-edge" | "after-edge" | "ideographic" => {
            Some(SvgDominantBaseline::TextAfterEdge)
        }
        _ => None,
    }
}

fn clean_svg_css_keyword_value(value: &str) -> &str {
    const IMPORTANT: &[u8] = b"!important";

    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() >= IMPORTANT.len() {
        let suffix_start = bytes.len() - IMPORTANT.len();
        if bytes[suffix_start..].eq_ignore_ascii_case(IMPORTANT) {
            return value[..suffix_start].trim_end();
        }
    }
    value
}

fn apply_svg_color_attr(style: &mut SvgStyle, value: Option<&str>, css_vars: &[SvgCssVariable]) {
    if let Some(color) = value.and_then(|value| parse_svg_color(value, css_vars)) {
        apply_svg_color(style, color);
    }
}

fn apply_svg_color(style: &mut SvgStyle, color: SvgColor) {
    style.color = color;
    if style.fill_current_color {
        style.fill = Some(color);
        style.fill_gradient = None;
        style.fill_pattern = None;
    }
    if style.stroke_current_color {
        style.stroke = Some(color);
        style.stroke_gradient = None;
    }
}

fn apply_svg_paint_attr(
    style: &mut SvgStyle,
    target: &str,
    value: Option<&str>,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
) {
    let Some(value) = value else {
        return;
    };
    let paint_alpha = parse_svg_paint_alpha(value, css_vars);
    if parse_svg_current_color_paint(value, css_vars) {
        if target == "fill" {
            style.fill = Some(style.color);
            style.fill_gradient = None;
            style.fill_pattern = None;
            style.fill_current_color = true;
            style.fill_context = None;
            if let Some(alpha) = paint_alpha {
                style.fill_opacity = alpha;
            }
        } else if target == "stroke" {
            style.stroke = Some(style.color);
            style.stroke_gradient = None;
            style.stroke_current_color = true;
            style.stroke_context = None;
            if let Some(alpha) = paint_alpha {
                style.stroke_opacity = alpha;
            }
        }
        return;
    }
    if let Some(context) = parse_svg_context_paint(value, css_vars) {
        if target == "fill" {
            style.fill = None;
            style.fill_gradient = None;
            style.fill_pattern = None;
            style.fill_current_color = false;
            style.fill_context = Some(context);
            if let Some(alpha) = paint_alpha {
                style.fill_opacity = alpha;
            }
        } else if target == "stroke" {
            style.stroke = None;
            style.stroke_gradient = None;
            style.stroke_current_color = false;
            style.stroke_context = Some(context);
            if let Some(alpha) = paint_alpha {
                style.stroke_opacity = alpha;
            }
        }
        return;
    }
    let gradient_ref = parse_svg_paint_gradient_ref(value, gradients, css_vars);
    let pattern_ref = parse_svg_paint_pattern_ref(value, patterns, css_vars);
    let paint = parse_svg_paint(value, gradients, css_vars).and_then(|paint| {
        if target == "fill" && paint.is_none() {
            pattern_ref
                .and_then(|index| patterns.get(index))
                .map(|pattern| Some(pattern.color))
                .or(Some(None))
        } else {
            Some(paint)
        }
    });
    let Some(paint) = paint else {
        return;
    };
    if target == "fill" {
        style.fill = paint;
        style.fill_current_color = false;
        style.fill_context = None;
        style.fill_gradient = if style.fill.is_some() {
            gradient_ref
        } else {
            None
        };
        style.fill_pattern = if style.fill.is_some() {
            pattern_ref
        } else {
            None
        };
        if style.fill_pattern.is_some() {
            style.fill_gradient = None;
        }
        if let Some(alpha) = paint_alpha {
            style.fill_opacity = alpha;
        }
    } else if target == "stroke" {
        style.stroke = paint;
        style.stroke_gradient = if style.stroke.is_some() {
            gradient_ref
        } else {
            None
        };
        style.stroke_current_color = false;
        style.stroke_context = None;
        if let Some(alpha) = paint_alpha {
            style.stroke_opacity = alpha;
        }
    }
}

fn parse_svg_current_color_paint(value: &str, css_vars: &[SvgCssVariable]) -> bool {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(")
        && let Some(resolved) = resolve_svg_css_value(value, css_vars, 0)
    {
        return parse_svg_current_color_paint(&resolved, css_vars);
    }
    value.eq_ignore_ascii_case("currentColor")
}

fn parse_svg_context_paint(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgContextPaint> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_context_paint(&resolved, css_vars);
    }
    if value.eq_ignore_ascii_case("context-fill") {
        Some(SvgContextPaint::Fill)
    } else if value.eq_ignore_ascii_case("context-stroke") {
        Some(SvgContextPaint::Stroke)
    } else {
        None
    }
}

fn parse_svg_paint(
    value: &str,
    gradients: &[SvgGradientPaint],
    css_vars: &[SvgCssVariable],
) -> Option<Option<(f32, f32, f32)>> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_paint(&resolved, gradients, css_vars);
    }
    if value.eq_ignore_ascii_case("none") || value.eq_ignore_ascii_case("transparent") {
        return Some(None);
    }
    if let Some(id) = parse_svg_paint_url_id(value) {
        if let Some(gradient) = gradients.iter().find(|gradient| gradient.id == id) {
            return Some(Some(gradient.color));
        }
        if let Some((_, fallback)) = value.split_once(')')
            && let Some(paint) = parse_svg_paint(fallback.trim(), gradients, css_vars)
        {
            return Some(paint);
        }
        if id == "fm-node-gradient" {
            return Some(Some((0.965, 0.965, 0.965)));
        }
        return Some(None);
    }
    if let Some(parsed) = parse_svg_color_with_alpha(value, css_vars) {
        if parsed.alpha.is_some_and(|alpha| alpha <= 0.001) {
            return Some(None);
        }
        return Some(Some(parsed.rgb));
    }
    None
}

fn parse_svg_paint_gradient_ref(
    value: &str,
    gradients: &[SvgGradientPaint],
    css_vars: &[SvgCssVariable],
) -> Option<usize> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_paint_gradient_ref(&resolved, gradients, css_vars);
    }
    let id = parse_svg_paint_url_id(value)?;
    gradients.iter().position(|gradient| {
        gradient.id == id && (gradient.linear.is_some() || gradient.radial.is_some())
    })
}

fn parse_svg_paint_pattern_ref(
    value: &str,
    patterns: &[SvgPatternPaint],
    css_vars: &[SvgCssVariable],
) -> Option<usize> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_paint_pattern_ref(&resolved, patterns, css_vars);
    }
    let id = parse_svg_paint_url_id(value)?;
    patterns.iter().position(|pattern| pattern.id == id)
}

fn parse_svg_paint_alpha(value: &str, css_vars: &[SvgCssVariable]) -> Option<f32> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_paint_alpha(&resolved, css_vars);
    }
    parse_svg_color_with_alpha(value, css_vars).and_then(|parsed| parsed.alpha)
}

fn resolve_svg_css_value(value: &str, css_vars: &[SvgCssVariable], depth: usize) -> Option<String> {
    if depth >= 8 {
        return None;
    }
    let value = value.trim();
    let Some(rest) = value.strip_prefix("var(") else {
        return Some(value.to_string());
    };
    let close = find_svg_css_function_close(rest)?;
    let args = rest[..close].trim();
    let (name, fallback) = split_svg_css_var_args(args);
    let name = name.trim();
    if !name.starts_with("--") {
        return fallback.and_then(|fallback| resolve_svg_css_value(fallback, css_vars, depth + 1));
    }
    if let Some(var) = css_vars.iter().find(|var| var.name == name) {
        return resolve_svg_css_value(var.value.as_str(), css_vars, depth + 1);
    }
    fallback.and_then(|fallback| resolve_svg_css_value(fallback, css_vars, depth + 1))
}

fn split_svg_css_var_args(args: &str) -> (&str, Option<&str>) {
    let mut depth = 0usize;
    for (idx, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return (&args[..idx], Some(args[idx + 1..].trim())),
            _ => {}
        }
    }
    (args, None)
}

fn find_svg_css_function_close(rest: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_svg_paint_url_id(value: &str) -> Option<&str> {
    let value = value.trim_start();
    let rest = value.strip_prefix("url(")?;
    let close = rest.find(')')?;
    let mut target = rest[..close].trim();
    if (target.starts_with('"') && target.ends_with('"'))
        || (target.starts_with('\'') && target.ends_with('\''))
    {
        target = target.get(1..target.len().saturating_sub(1))?.trim();
    }
    target.strip_prefix('#').filter(|id| !id.is_empty())
}

fn parse_svg_color(value: &str, css_vars: &[SvgCssVariable]) -> Option<(f32, f32, f32)> {
    parse_svg_color_with_alpha(value, css_vars).map(|parsed| parsed.rgb)
}

fn parse_svg_color_with_alpha(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgParsedColor> {
    parse_svg_color_inner_with_alpha(value, css_vars, 0)
}

fn parse_svg_color_inner(
    value: &str,
    css_vars: &[SvgCssVariable],
    depth: usize,
) -> Option<SvgColor> {
    parse_svg_color_inner_with_alpha(value, css_vars, depth).map(|parsed| parsed.rgb)
}

fn parse_svg_color_inner_with_alpha(
    value: &str,
    css_vars: &[SvgCssVariable],
    depth: usize,
) -> Option<SvgParsedColor> {
    if depth >= 8 {
        return None;
    }
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_color_inner_with_alpha(&resolved, css_vars, depth + 1);
    }
    if let Some(color) = parse_svg_color_mix(value, css_vars, depth + 1) {
        return Some(SvgParsedColor {
            rgb: color,
            alpha: None,
        });
    }
    if let Some(hex) = value.strip_prefix('#') {
        return parse_svg_hex_color_with_alpha(hex);
    }
    if let Some(color) = parse_svg_rgb_function(value, css_vars) {
        return Some(color);
    }
    let rgb = match value.to_ascii_lowercase().as_str() {
        "black" => (0.0, 0.0, 0.0),
        "white" => (1.0, 1.0, 1.0),
        "red" => (1.0, 0.0, 0.0),
        "green" => (0.0, 0.5, 0.0),
        "blue" => (0.0, 0.0, 1.0),
        _ => return None,
    };
    Some(SvgParsedColor { rgb, alpha: None })
}

fn parse_svg_hex_color_with_alpha(hex: &str) -> Option<SvgParsedColor> {
    let bytes = hex.as_bytes();
    let (r, g, b, alpha) = match bytes {
        [r, g, b] => {
            let r = svg_hex_nibble(*r)?;
            let g = svg_hex_nibble(*g)?;
            let b = svg_hex_nibble(*b)?;
            (r * 17, g * 17, b * 17, None)
        }
        [r, g, b, a] => {
            let r = svg_hex_nibble(*r)?;
            let g = svg_hex_nibble(*g)?;
            let b = svg_hex_nibble(*b)?;
            let a = svg_hex_nibble(*a)?;
            (r * 17, g * 17, b * 17, Some(f32::from(a) / 15.0))
        }
        [r1, r2, g1, g2, b1, b2] => (
            svg_hex_pair(*r1, *r2)?,
            svg_hex_pair(*g1, *g2)?,
            svg_hex_pair(*b1, *b2)?,
            None,
        ),
        [r1, r2, g1, g2, b1, b2, a1, a2] => (
            svg_hex_pair(*r1, *r2)?,
            svg_hex_pair(*g1, *g2)?,
            svg_hex_pair(*b1, *b2)?,
            Some(f32::from(svg_hex_pair(*a1, *a2)?) / 255.0),
        ),
        _ => return None,
    };
    Some(SvgParsedColor {
        rgb: (
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        ),
        alpha,
    })
}

fn parse_svg_rgb_function(value: &str, css_vars: &[SvgCssVariable]) -> Option<SvgParsedColor> {
    let args =
        svg_css_function_args(value, "rgb").or_else(|| svg_css_function_args(value, "rgba"))?;
    let (channel_args, slash_alpha) = split_svg_top_level_slash(args)?;
    let comma_parts = split_svg_top_level_commas(channel_args)?;
    let (channels, comma_alpha) = if comma_parts.len() == 3 || comma_parts.len() == 4 {
        (comma_parts[..3].to_vec(), comma_parts.get(3).copied())
    } else if comma_parts.len() == 1 {
        let space_parts = split_svg_top_level_whitespace(channel_args);
        if space_parts.len() != 3 {
            return None;
        }
        (space_parts, None)
    } else {
        return None;
    };
    let r = parse_svg_rgb_channel(channels[0], css_vars)?;
    let g = parse_svg_rgb_channel(channels[1], css_vars)?;
    let b = parse_svg_rgb_channel(channels[2], css_vars)?;
    let alpha = slash_alpha
        .or(comma_alpha)
        .map(|alpha| parse_svg_opacity(alpha, css_vars));
    let alpha = match alpha {
        Some(Some(alpha)) => Some(alpha),
        Some(None) => return None,
        None => None,
    };
    Some(SvgParsedColor {
        rgb: (r, g, b),
        alpha,
    })
}

fn parse_svg_rgb_channel(value: &str, css_vars: &[SvgCssVariable]) -> Option<f32> {
    let value = clean_svg_css_keyword_value(value);
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_rgb_channel(&resolved, css_vars);
    }
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let parsed = value[..end].parse::<f32>().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    let unit = value[end..].trim_start();
    let channel = if unit.starts_with('%') {
        parsed / 100.0
    } else {
        parsed / 255.0
    };
    channel.is_finite().then_some(channel.clamp(0.0, 1.0))
}

fn svg_hex_pair(high: u8, low: u8) -> Option<u8> {
    Some((svg_hex_nibble(high)? << 4) | svg_hex_nibble(low)?)
}

fn svg_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_svg_color_mix(value: &str, css_vars: &[SvgCssVariable], depth: usize) -> Option<SvgColor> {
    if depth >= 8 {
        return None;
    }
    let args = svg_css_function_args(value, "color-mix")?;
    let parts = split_svg_top_level_commas(args)?;
    if parts.len() != 3 || !svg_color_mix_space_is_srgb(parts[0]) {
        return None;
    }
    let (first_color, first_weight) = parse_svg_color_mix_component(parts[1], css_vars, depth + 1)?;
    let (second_color, second_weight) =
        parse_svg_color_mix_component(parts[2], css_vars, depth + 1)?;
    let (first_weight, second_weight) =
        svg_color_mix_weights(first_weight, second_weight).filter(|(a, b)| *a > 0.0 || *b > 0.0)?;
    Some((
        (first_color.0 * first_weight + second_color.0 * second_weight).clamp(0.0, 1.0),
        (first_color.1 * first_weight + second_color.1 * second_weight).clamp(0.0, 1.0),
        (first_color.2 * first_weight + second_color.2 * second_weight).clamp(0.0, 1.0),
    ))
}

fn svg_css_function_args<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let value = value.trim();
    let open = value.find('(')?;
    if !value[..open].trim().eq_ignore_ascii_case(name) {
        return None;
    }
    let rest = &value[open + 1..];
    let close = find_svg_css_function_close(rest)?;
    rest[close + 1..]
        .trim()
        .is_empty()
        .then_some(&rest[..close])
}

fn split_svg_top_level_commas(value: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                parts.push(value[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    parts.push(value[start..].trim());
    Some(parts)
}

fn split_svg_top_level_slash(value: &str) -> Option<(&str, Option<&str>)> {
    let mut depth = 0usize;
    let mut slash = None;
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            '/' if depth == 0 && slash.is_some() => return None,
            '/' if depth == 0 => slash = Some(idx),
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    match slash {
        Some(idx) => Some((value[..idx].trim(), Some(value[idx + 1..].trim()))),
        None => Some((value.trim(), None)),
    }
}

fn svg_color_mix_space_is_srgb(space: &str) -> bool {
    let mut words = space.split_ascii_whitespace();
    matches!(
        (words.next(), words.next(), words.next()),
        (Some(first), Some(second), None)
            if first.eq_ignore_ascii_case("in") && second.eq_ignore_ascii_case("srgb")
    )
}

fn parse_svg_color_mix_component(
    component: &str,
    css_vars: &[SvgCssVariable],
    depth: usize,
) -> Option<(SvgColor, Option<f32>)> {
    let (color_src, weight) = split_svg_color_mix_component_weight(component)?;
    if svg_color_source_is_transparent(color_src, css_vars) {
        return None;
    }
    Some((
        parse_svg_color_inner(color_src, css_vars, depth + 1)?,
        weight,
    ))
}

fn split_svg_color_mix_component_weight(component: &str) -> Option<(&str, Option<f32>)> {
    let component = component.trim();
    if component.is_empty() {
        return None;
    }
    let trimmed_end = component.trim_end();
    if !trimmed_end.ends_with('%') {
        return Some((component, None));
    }
    let percent_idx = trimmed_end.len().saturating_sub(1);
    let mut depth = 0usize;
    let mut split = None;
    let mut in_whitespace = false;
    for (idx, ch) in trimmed_end[..percent_idx].char_indices() {
        match ch {
            '(' => {
                depth += 1;
                in_whitespace = false;
            }
            ')' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                in_whitespace = false;
            }
            _ if depth == 0 && ch.is_ascii_whitespace() => {
                if !in_whitespace {
                    split = Some(idx);
                }
                in_whitespace = true;
            }
            _ => in_whitespace = false,
        }
    }
    if depth != 0 {
        return None;
    }
    let split = split?;
    let color_src = component[..split].trim();
    let percent = parse_svg_color_mix_percentage(&trimmed_end[split..percent_idx])?;
    (!color_src.is_empty()).then_some((color_src, Some(percent)))
}

fn parse_svg_color_mix_percentage(value: &str) -> Option<f32> {
    let value = value.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    if !value[end..].trim().is_empty() {
        return None;
    }
    let percentage = value[..end].parse::<f32>().ok()?;
    (percentage.is_finite() && percentage >= 0.0).then_some(percentage / 100.0)
}

fn svg_color_mix_weights(first: Option<f32>, second: Option<f32>) -> Option<(f32, f32)> {
    let (first, second) = match (first, second) {
        (Some(first), Some(second)) => {
            let total = first + second;
            if total <= f32::EPSILON {
                return None;
            }
            (first / total, second / total)
        }
        (Some(first), None) if first <= 1.0 => (first, 1.0 - first),
        (None, Some(second)) if second <= 1.0 => (1.0 - second, second),
        (None, None) => (0.5, 0.5),
        _ => return None,
    };
    (first.is_finite() && second.is_finite()).then_some((first, second))
}

fn svg_color_source_is_transparent(value: &str, css_vars: &[SvgCssVariable]) -> bool {
    let value = value.trim();
    if value.starts_with("var(")
        && let Some(resolved) = resolve_svg_css_value(value, css_vars, 0)
    {
        return svg_color_source_is_transparent(&resolved, css_vars);
    }
    if value.eq_ignore_ascii_case("transparent") {
        return true;
    }
    parse_svg_color_with_alpha(value, css_vars)
        .and_then(|parsed| parsed.alpha)
        .is_some_and(|alpha| alpha <= 0.001)
}

fn parse_svg_number(value: &str) -> Option<f32> {
    let value = value.trim();
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let parsed = value[..end].parse::<f32>().ok()?;
    parsed.is_finite().then_some(parsed)
}

fn parse_svg_css_number(value: &str, css_vars: &[SvgCssVariable]) -> Option<f32> {
    let value = value.trim();
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_css_number(&resolved, css_vars);
    }
    parse_svg_number(value)
}

fn parse_svg_opacity(value: &str, css_vars: &[SvgCssVariable]) -> Option<f32> {
    let value = value.trim();
    if value.starts_with("var(") {
        let resolved = resolve_svg_css_value(value, css_vars, 0)?;
        return parse_svg_opacity(&resolved, css_vars);
    }
    let mut end = 0usize;
    read_svg_number_token(value, &mut end)?;
    let parsed = value[..end].parse::<f32>().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    let unit = value[end..].trim_start();
    let opacity = if unit.starts_with('%') {
        parsed / 100.0
    } else {
        parsed
    };
    opacity.is_finite().then_some(opacity.clamp(0.0, 1.0))
}

fn parse_svg_number_list(value: &str) -> Vec<f32> {
    let mut nums = Vec::new();
    let mut pos = 0usize;
    while pos < value.len() {
        skip_svg_number_separators(value, &mut pos);
        if pos >= value.len() {
            break;
        }
        let start = pos;
        if read_svg_number_token(value, &mut pos).is_none() {
            pos += value[pos..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        if let Ok(num) = value[start..pos].parse::<f32>()
            && num.is_finite()
        {
            nums.push(num);
        }
    }
    nums
}

fn parse_svg_transform(value: &str) -> Option<SvgTransform> {
    let mut transform = SvgTransform::IDENTITY;
    let mut parsed_any = false;
    let mut pos = 0usize;
    while pos < value.len() {
        skip_svg_transform_separators(value, &mut pos);
        if pos >= value.len() {
            break;
        }
        let name_start = pos;
        while let Some(ch) = value[pos..].chars().next() {
            if !(ch.is_ascii_alphabetic() || ch == '-') {
                break;
            }
            pos += ch.len_utf8();
        }
        if name_start == pos {
            return parsed_any.then_some(transform);
        }
        let name = value[name_start..pos].to_ascii_lowercase();
        while let Some(ch) = value[pos..].chars().next() {
            if !ch.is_ascii_whitespace() {
                break;
            }
            pos += ch.len_utf8();
        }
        if !value[pos..].starts_with('(') {
            return parsed_any.then_some(transform);
        }
        pos += 1;
        let args_start = pos;
        let Some(args_end_rel) = value[pos..].find(')') else {
            return parsed_any.then_some(transform);
        };
        let args_end = pos + args_end_rel;
        let nums = parse_svg_number_list(&value[args_start..args_end]);
        let next = match name.as_str() {
            "matrix" if nums.len() >= 6 => SvgTransform {
                a: nums[0],
                b: nums[1],
                c: nums[2],
                d: nums[3],
                e: nums[4],
                f: nums[5],
            },
            "translate" if !nums.is_empty() => {
                SvgTransform::translate(nums[0], nums.get(1).copied().unwrap_or(0.0))
            }
            "scale" if !nums.is_empty() => {
                SvgTransform::scale(nums[0], nums.get(1).copied().unwrap_or(nums[0]))
            }
            "rotate" if !nums.is_empty() => {
                let rotation = SvgTransform::rotate_degrees(nums[0]);
                if nums.len() >= 3 {
                    SvgTransform::translate(nums[1], nums[2])
                        .concat(rotation)
                        .concat(SvgTransform::translate(-nums[1], -nums[2]))
                } else {
                    rotation
                }
            }
            "skewx" if !nums.is_empty() => SvgTransform::skew_x_degrees(nums[0]),
            "skewy" if !nums.is_empty() => SvgTransform::skew_y_degrees(nums[0]),
            _ => return parsed_any.then_some(transform),
        };
        transform = transform.concat(next);
        parsed_any = true;
        pos = args_end + 1;
    }
    parsed_any.then_some(transform)
}

fn skip_svg_transform_separators(value: &str, pos: &mut usize) {
    while *pos < value.len() {
        let Some(ch) = value[*pos..].chars().next() else {
            break;
        };
        if !ch.is_ascii_whitespace() && ch != ',' {
            break;
        }
        *pos += ch.len_utf8();
    }
}

fn parse_svg_points(value: &str) -> Vec<(f32, f32)> {
    parse_svg_number_list(value)
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

fn parse_svg_path_data(data: &str) -> Option<Vec<SvgPathOp>> {
    let mut ops = Vec::new();
    let mut pos = 0usize;
    let mut cmd = '\0';
    let mut current = (0.0f32, 0.0f32);
    let mut subpath_start = current;
    let mut last_cubic_control: Option<(f32, f32)> = None;
    let mut last_quad_control: Option<(f32, f32)> = None;
    while pos < data.len() {
        skip_svg_number_separators(data, &mut pos);
        if pos >= data.len() {
            break;
        }
        if let Some(ch) = data[pos..].chars().next()
            && ch.is_ascii_alphabetic()
        {
            cmd = ch;
            pos += ch.len_utf8();
        }
        if cmd == '\0' {
            break;
        }
        match cmd {
            'M' | 'm' => {
                let Some(mut point) = read_svg_path_pair(data, &mut pos) else {
                    break;
                };
                if cmd == 'm' {
                    point.0 += current.0;
                    point.1 += current.1;
                }
                current = point;
                subpath_start = point;
                last_cubic_control = None;
                last_quad_control = None;
                push_svg_path_op(&mut ops, SvgPathOp::Move(point.0, point.1))?;
                let line_cmd = if cmd == 'm' { 'l' } else { 'L' };
                while let Some(mut point) = read_svg_path_pair(data, &mut pos) {
                    if line_cmd == 'l' {
                        point.0 += current.0;
                        point.1 += current.1;
                    }
                    current = point;
                    last_cubic_control = None;
                    last_quad_control = None;
                    push_svg_path_op(&mut ops, SvgPathOp::Line(point.0, point.1))?;
                }
                cmd = line_cmd;
            }
            'L' | 'l' => {
                let mut read_any = false;
                while let Some(mut point) = read_svg_path_pair(data, &mut pos) {
                    if cmd == 'l' {
                        point.0 += current.0;
                        point.1 += current.1;
                    }
                    current = point;
                    last_cubic_control = None;
                    last_quad_control = None;
                    push_svg_path_op(&mut ops, SvgPathOp::Line(point.0, point.1))?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'H' | 'h' => {
                let mut read_any = false;
                while let Some(mut x) = read_svg_path_number(data, &mut pos) {
                    if cmd == 'h' {
                        x += current.0;
                    }
                    current.0 = x;
                    last_cubic_control = None;
                    last_quad_control = None;
                    push_svg_path_op(&mut ops, SvgPathOp::Line(current.0, current.1))?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'V' | 'v' => {
                let mut read_any = false;
                while let Some(mut y) = read_svg_path_number(data, &mut pos) {
                    if cmd == 'v' {
                        y += current.1;
                    }
                    current.1 = y;
                    last_cubic_control = None;
                    last_quad_control = None;
                    push_svg_path_op(&mut ops, SvgPathOp::Line(current.0, current.1))?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'C' | 'c' => {
                let mut read_any = false;
                while let Some(values) = read_svg_path_numbers::<6>(data, &mut pos) {
                    let mut x1 = values[0];
                    let mut y1 = values[1];
                    let mut x2 = values[2];
                    let mut y2 = values[3];
                    let mut x = values[4];
                    let mut y = values[5];
                    if cmd == 'c' {
                        x1 += current.0;
                        y1 += current.1;
                        x2 += current.0;
                        y2 += current.1;
                        x += current.0;
                        y += current.1;
                    }
                    current = (x, y);
                    last_cubic_control = Some((x2, y2));
                    last_quad_control = None;
                    push_svg_path_op(&mut ops, SvgPathOp::Cubic(x1, y1, x2, y2, x, y))?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'S' | 's' => {
                let mut read_any = false;
                while let Some(values) = read_svg_path_numbers::<4>(data, &mut pos) {
                    let (x1, y1) = last_cubic_control.map_or(current, |control| {
                        (current.0 * 2.0 - control.0, current.1 * 2.0 - control.1)
                    });
                    let mut x2 = values[0];
                    let mut y2 = values[1];
                    let mut x = values[2];
                    let mut y = values[3];
                    if cmd == 's' {
                        x2 += current.0;
                        y2 += current.1;
                        x += current.0;
                        y += current.1;
                    }
                    current = (x, y);
                    last_cubic_control = Some((x2, y2));
                    last_quad_control = None;
                    push_svg_path_op(&mut ops, SvgPathOp::Cubic(x1, y1, x2, y2, x, y))?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'Q' | 'q' => {
                let mut read_any = false;
                while let Some(values) = read_svg_path_numbers::<4>(data, &mut pos) {
                    let mut x1 = values[0];
                    let mut y1 = values[1];
                    let mut x = values[2];
                    let mut y = values[3];
                    if cmd == 'q' {
                        x1 += current.0;
                        y1 += current.1;
                        x += current.0;
                        y += current.1;
                    }
                    current = (x, y);
                    last_cubic_control = None;
                    last_quad_control = Some((x1, y1));
                    push_svg_path_op(&mut ops, SvgPathOp::Quad(x1, y1, x, y))?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'T' | 't' => {
                let mut read_any = false;
                while let Some(mut point) = read_svg_path_pair(data, &mut pos) {
                    if cmd == 't' {
                        point.0 += current.0;
                        point.1 += current.1;
                    }
                    let control = last_quad_control.map_or(current, |prev| {
                        (current.0 * 2.0 - prev.0, current.1 * 2.0 - prev.1)
                    });
                    current = point;
                    last_cubic_control = None;
                    last_quad_control = Some(control);
                    push_svg_path_op(
                        &mut ops,
                        SvgPathOp::Quad(control.0, control.1, point.0, point.1),
                    )?;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'A' | 'a' => {
                let mut read_any = false;
                while let Some(values) = read_svg_path_numbers::<7>(data, &mut pos) {
                    let rx = values[0];
                    let ry = values[1];
                    let rotation = values[2];
                    let large_arc = values[3].abs() >= 0.5;
                    let sweep = values[4].abs() >= 0.5;
                    let mut x = values[5];
                    let mut y = values[6];
                    if cmd == 'a' {
                        x += current.0;
                        y += current.1;
                    }
                    append_svg_arc_ops(
                        &mut ops,
                        current,
                        (x, y),
                        rx,
                        ry,
                        rotation,
                        large_arc,
                        sweep,
                    )?;
                    current = (x, y);
                    last_cubic_control = None;
                    last_quad_control = None;
                    read_any = true;
                }
                if !read_any {
                    break;
                }
            }
            'Z' | 'z' => {
                push_svg_path_op(&mut ops, SvgPathOp::Close)?;
                current = subpath_start;
                last_cubic_control = None;
                last_quad_control = None;
                cmd = '\0';
            }
            _ => break,
        }
    }
    Some(ops)
}

fn push_svg_path_op(ops: &mut Vec<SvgPathOp>, op: SvgPathOp) -> Option<()> {
    if ops.len() >= MAX_SVG_PATH_OPS {
        return None;
    }
    ops.push(op);
    Some(())
}

#[allow(clippy::too_many_arguments)]
fn append_svg_arc_ops(
    ops: &mut Vec<SvgPathOp>,
    start: (f32, f32),
    end: (f32, f32),
    rx: f32,
    ry: f32,
    x_axis_rotation: f32,
    large_arc: bool,
    sweep: bool,
) -> Option<()> {
    if (start.0 - end.0).abs() <= f32::EPSILON && (start.1 - end.1).abs() <= f32::EPSILON {
        return Some(());
    }

    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        return push_svg_path_op(ops, SvgPathOp::Line(end.0, end.1));
    }

    let phi = x_axis_rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx2 = (start.0 - end.0) * 0.5;
    let dy2 = (start.1 - end.1) * 0.5;
    let x1p = cos_phi * dx2 + sin_phi * dy2;
    let y1p = -sin_phi * dx2 + cos_phi * dy2;

    let radii_scale = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if radii_scale > 1.0 {
        let scale = radii_scale.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let x1p2 = x1p * x1p;
    let y1p2 = y1p * y1p;
    let denom = rx2 * y1p2 + ry2 * x1p2;
    if denom <= f32::EPSILON {
        return push_svg_path_op(ops, SvgPathOp::Line(end.0, end.1));
    }

    let sign = if large_arc == sweep { -1.0 } else { 1.0 };
    let center_factor = sign
        * ((rx2 * ry2 - rx2 * y1p2 - ry2 * x1p2) / denom)
            .max(0.0)
            .sqrt();
    let cxp = center_factor * rx * y1p / ry;
    let cyp = -center_factor * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (start.0 + end.0) * 0.5;
    let cy = sin_phi * cxp + cos_phi * cyp + (start.1 + end.1) * 0.5;

    let theta1 = svg_vector_angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = svg_vector_angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    if delta.abs() <= f32::EPSILON {
        return Some(());
    }

    let segments = (delta.abs() / std::f32::consts::FRAC_PI_2).ceil() as usize;
    let step = delta / segments as f32;
    for idx in 0..segments {
        let a0 = theta1 + step * idx as f32;
        let a1 = a0 + step;
        let alpha = (4.0 / 3.0) * ((a1 - a0) * 0.25).tan();
        let (sin0, cos0) = a0.sin_cos();
        let (sin1, cos1) = a1.sin_cos();
        let c1 = (cos0 - alpha * sin0, sin0 + alpha * cos0);
        let c2 = (cos1 + alpha * sin1, sin1 - alpha * cos1);
        let p = (cos1, sin1);
        let c1 = svg_map_arc_point(c1, cx, cy, rx, ry, sin_phi, cos_phi);
        let c2 = svg_map_arc_point(c2, cx, cy, rx, ry, sin_phi, cos_phi);
        let p = if idx + 1 == segments {
            end
        } else {
            svg_map_arc_point(p, cx, cy, rx, ry, sin_phi, cos_phi)
        };
        push_svg_path_op(ops, SvgPathOp::Cubic(c1.0, c1.1, c2.0, c2.1, p.0, p.1))?;
    }
    Some(())
}

fn svg_vector_angle(ux: f32, uy: f32, vx: f32, vy: f32) -> f32 {
    (ux * vy - uy * vx).atan2(ux * vx + uy * vy)
}

fn svg_map_arc_point(
    point: (f32, f32),
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    sin_phi: f32,
    cos_phi: f32,
) -> (f32, f32) {
    (
        cx + rx * (cos_phi * point.0 - sin_phi * point.1),
        cy + ry * (sin_phi * point.0 + cos_phi * point.1),
    )
}

fn read_svg_path_pair(data: &str, pos: &mut usize) -> Option<(f32, f32)> {
    let x = read_svg_path_number(data, pos)?;
    let y = read_svg_path_number(data, pos)?;
    Some((x, y))
}

fn read_svg_path_numbers<const N: usize>(data: &str, pos: &mut usize) -> Option<[f32; N]> {
    let checkpoint = *pos;
    let mut values = [0.0f32; N];
    for value in &mut values {
        let Some(parsed) = read_svg_path_number(data, pos) else {
            *pos = checkpoint;
            return None;
        };
        *value = parsed;
    }
    Some(values)
}

fn read_svg_path_number(data: &str, pos: &mut usize) -> Option<f32> {
    skip_svg_number_separators(data, pos);
    let start = *pos;
    read_svg_number_token(data, pos)?;
    data[start..*pos]
        .parse::<f32>()
        .ok()
        .filter(|num| num.is_finite())
}

fn read_svg_number_token(data: &str, pos: &mut usize) -> Option<()> {
    let bytes = data.as_bytes();
    let len = bytes.len();
    let start = *pos;
    if *pos < len && matches!(bytes[*pos], b'+' | b'-') {
        *pos += 1;
    }
    let mut digits = 0usize;
    while *pos < len && bytes[*pos].is_ascii_digit() {
        *pos += 1;
        digits += 1;
    }
    if *pos < len && bytes[*pos] == b'.' {
        *pos += 1;
        while *pos < len && bytes[*pos].is_ascii_digit() {
            *pos += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        *pos = start;
        return None;
    }
    if *pos < len && matches!(bytes[*pos], b'e' | b'E') {
        let exp_start = *pos;
        *pos += 1;
        if *pos < len && matches!(bytes[*pos], b'+' | b'-') {
            *pos += 1;
        }
        let exp_digits_start = *pos;
        while *pos < len && bytes[*pos].is_ascii_digit() {
            *pos += 1;
        }
        if *pos == exp_digits_start {
            *pos = exp_start;
        }
    }
    Some(())
}

fn skip_svg_number_separators(data: &str, pos: &mut usize) {
    while *pos < data.len() {
        let Some(ch) = data[*pos..].chars().next() else {
            break;
        };
        if ch.is_ascii_whitespace() || ch == ',' {
            *pos += ch.len_utf8();
        } else {
            break;
        }
    }
}

fn strip_svg_tags(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_tag = false;
    for ch in src.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_xml_entities(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        rest = &rest[idx + 1..];
        let Some(end) = rest.find(';') else {
            out.push('&');
            out.push_str(rest);
            return out;
        };
        let entity = &rest[..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" | "#39" => out.push('\''),
            _ if entity.starts_with('#') => match decode_xml_numeric_entity(entity) {
                Some(ch) => out.push(ch),
                None => {
                    out.push('&');
                    out.push_str(entity);
                    out.push(';');
                }
            },
            _ => {
                out.push('&');
                out.push_str(entity);
                out.push(';');
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn decode_xml_numeric_entity(entity: &str) -> Option<char> {
    let (digits, radix) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
        .map(|digits| (digits, 16))
        .or_else(|| entity.strip_prefix('#').map(|digits| (digits, 10)))?;
    if digits.is_empty() {
        return None;
    }
    let digits_match_radix = match radix {
        16 => digits.bytes().all(|byte| byte.is_ascii_hexdigit()),
        10 => digits.bytes().all(|byte| byte.is_ascii_digit()),
        _ => false,
    };
    if !digits_match_radix {
        return None;
    }
    let code = u32::from_str_radix(digits, radix).ok();
    Some(
        code.and_then(char::from_u32)
            .filter(|ch| svg_xml_char_is_valid(*ch))
            .unwrap_or('\u{FFFD}'),
    )
}

fn svg_xml_char_is_valid(ch: char) -> bool {
    let code = ch as u32;
    matches!(code, 0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn png_predictor_payload_is_valid(png: &PngChunks, components: usize) -> bool {
    let row_bytes = (png.width as usize).checked_mul(components);
    let expected = row_bytes
        .and_then(|row| row.checked_add(1))
        .and_then(|stride| stride.checked_mul(png.height as usize));
    let Some((row_bytes, expected)) = row_bytes.zip(expected) else {
        return false;
    };
    let raw = match crate::compress::zlib_decompress(&png.idat, expected) {
        Some(raw) => raw,
        None => return false,
    };
    if raw.len() != expected {
        return false;
    }
    let stride = row_bytes + 1;
    raw.chunks_exact(stride)
        .all(|row| matches!(row.first().copied(), Some(0..=4)))
}

/// Walk a PNG's chunk stream, collecting IHDR fields, PLTE, tRNS, and IDAT while
/// rejecting malformed chunk ordering, invalid format combinations, and image
/// sizes that would exceed the decoder's bounded memory budget.
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
    let mut seen_plte = false;
    let mut seen_trns = false;
    let mut seen_idat = false;
    let mut idat_stream_closed = false;

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
                // Reject spec-invalid color-type/bit-depth PAIRS (not just each
                // independently): truecolor/gray+alpha/RGBA are 8- or 16-bit only,
                // and palette is <= 8-bit. An invalid combo is a malformed PNG and
                // must be rejected (→ alt text + warning), not decoded to garbage.
                let valid_combo = matches!(
                    (color_type, bit_depth),
                    (0, 1 | 2 | 4 | 8 | 16)
                        | (2, 8 | 16)
                        | (3, 1 | 2 | 4 | 8)
                        | (4, 8 | 16)
                        | (6, 8 | 16)
                );
                // Bit-depth-aware bound on the decoded raw sample buffer
                // (`pixels * channels * bytes/sample`), so a 16-bit image cannot
                // slip a multi-hundred-MB transient past the pixel cap.
                let channels: u64 = match color_type {
                    2 => 3, // truecolor
                    4 => 2, // grayscale + alpha
                    6 => 4, // truecolor + alpha
                    _ => 1, // grayscale (0) / palette (3): 1 sample/pixel
                };
                let sample_bytes: u64 = if bit_depth == 16 { 2 } else { 1 };
                let decoded_bytes = u64::from(width)
                    .saturating_mul(u64::from(height))
                    .saturating_mul(channels)
                    .saturating_mul(sample_bytes);
                if width == 0
                    || height == 0
                    || u64::from(width).saturating_mul(u64::from(height)) > MAX_PDF_IMAGE_PIXELS
                    || decoded_bytes > MAX_PDF_IMAGE_DECODED_BYTES
                    || compression != 0
                    || filter != 0
                    || interlace > 1
                    || !valid_combo
                {
                    return None;
                }
                seen_ihdr = true;
            }
            b"PLTE" => {
                if seen_plte
                    || seen_idat
                    || len == 0
                    || len % 3 != 0
                    || len / 3 > 256
                    || matches!(color_type, 0 | 4)
                {
                    return None;
                }
                let entry_count = len / 3;
                if color_type == 3 && entry_count > (1usize << bit_depth) {
                    return None;
                }
                palette = data.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
                seen_plte = true;
            }
            b"tRNS" => {
                if seen_trns
                    || seen_idat
                    || matches!(color_type, 4 | 6)
                    || (color_type == 3 && !seen_plte)
                {
                    return None;
                }
                trns = data.to_vec();
                seen_trns = true;
            }
            b"IDAT" => {
                if !seen_ihdr {
                    return None;
                }
                if idat_stream_closed {
                    return None;
                }
                if idat.len().saturating_add(data.len()) > MAX_PDF_IMAGE_COMPRESSED_BYTES {
                    return None;
                }
                idat.extend_from_slice(data);
                seen_idat = true;
            }
            b"IEND" => {
                // IEND terminates the PNG datastream and carries no data. Stop
                // here and ignore anything after it: real files sometimes append
                // a trailer or get concatenated, and every conformant decoder
                // renders them (the trailing bytes never enter the PDF — decode
                // discards them). A non-empty IEND is still malformed and
                // rejected. Each chunk above is bounds-checked, so ignoring the
                // tail is safe.
                if len != 0 {
                    return None;
                }
                seen_iend = true;
                break;
            }
            _ => {
                if seen_idat {
                    idat_stream_closed = true;
                }
            }
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
    if !png_trns_chunk_is_valid(png) {
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
        let expected = (width as usize)
            .checked_mul(bits_per_pixel)?
            .div_ceil(8)
            .checked_add(1)?
            .checked_mul(height as usize)?;
        if raw.len() != expected {
            return None;
        }
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
        if offset != raw.len() {
            return None;
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
                let a = self.gray_trns_alpha(line, c, chan[0])?;
                Some((g, g, g, a))
            }
            2 => {
                let (r, g, b) = (chan[0], chan[1], chan[2]);
                let a = self.rgb_trns_alpha(line, c, r, g, b)?;
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
    fn gray_trns_alpha(&self, line: &[u8], c: usize, raw_sample: u8) -> Option<u8> {
        let transparent = be_u16(self.trns, 0);
        let sample = if self.bit_depth == 16 {
            be_u16(line, c.checked_mul(2)?)?
        } else {
            u16::from(raw_sample)
        };
        Some(if transparent == Some(sample) { 0 } else { 255 })
    }

    /// Alpha for an RGB pixel given a possible single-color tRNS.
    fn rgb_trns_alpha(&self, line: &[u8], c: usize, r: u8, g: u8, b: u8) -> Option<u8> {
        let transparent = (
            be_u16(self.trns, 0),
            be_u16(self.trns, 2),
            be_u16(self.trns, 4),
        );
        let sample = if self.bit_depth == 16 {
            let base = c.checked_mul(self.channels)?.checked_mul(2)?;
            (
                be_u16(line, base)?,
                be_u16(line, base.checked_add(2)?)?,
                be_u16(line, base.checked_add(4)?)?,
            )
        } else {
            (u16::from(r), u16::from(g), u16::from(b))
        };
        Some(
            if transparent == (Some(sample.0), Some(sample.1), Some(sample.2)) {
                0
            } else {
                255
            },
        )
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

fn png_trns_chunk_is_valid(png: &PngChunks) -> bool {
    match png.color_type {
        0 => {
            if png.trns.is_empty() {
                return true;
            }
            if png.trns.len() != 2 {
                return false;
            }
            let Some(sample) = be_u16(&png.trns, 0) else {
                return false;
            };
            png_trns_sample_fits_bit_depth(sample, png.bit_depth)
        }
        2 => {
            if png.trns.is_empty() {
                return true;
            }
            if png.trns.len() != 6 {
                return false;
            }
            match png.bit_depth {
                8 => [0, 2, 4]
                    .into_iter()
                    .all(|offset| be_u16(&png.trns, offset).is_some_and(|sample| sample <= 255)),
                16 => true,
                _ => false,
            }
        }
        3 => png.trns.len() <= png.palette.len(),
        4 | 6 => png.trns.is_empty(),
        _ => false,
    }
}

fn png_trns_sample_fits_bit_depth(sample: u16, bit_depth: u8) -> bool {
    match bit_depth {
        1 | 2 | 4 | 8 => sample < (1u16 << bit_depth),
        16 => true,
        _ => false,
    }
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let b = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let b = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
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

#[derive(Clone, Default)]
struct TableColumnMetrics {
    cells: Vec<TableCellMeasure>,
    /// Min-content width: the widest single unbreakable token measured in this
    /// column. Actual rendering can still hard-split longer tokens, but this is
    /// the expensive visual outcome the allocator should avoid when possible.
    min_content: f32,
    /// Max-content width: the widest source line if it were kept on one visual
    /// line. Allocating beyond this buys no wrapping improvement for the column.
    max_content: f32,
}

impl TableColumnMetrics {
    fn push(&mut self, cell: TableCellMeasure) {
        self.min_content = self.min_content.max(cell.min_content);
        self.max_content = self.max_content.max(cell.max_content);
        self.cells.push(cell);
    }
}

#[derive(Clone)]
struct TableCellMeasure {
    lines: Vec<TableCellMeasureLine>,
    min_content: f32,
    max_content: f32,
    weight: f32,
}

#[derive(Clone, Default)]
struct TableCellMeasureLine {
    /// Widths of non-space tokens. Token boundaries are the same wrap
    /// opportunities used by `wrap_cell_styled`, including adjacent styled runs.
    words: Vec<f32>,
    /// Space widths before words `1..`; length is always `words.len() - 1`.
    spaces: Vec<f32>,
    min_content: f32,
    max_content: f32,
}

fn table_cell_measure(
    toks: &[Tok],
    size: f32,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
    header: bool,
) -> TableCellMeasure {
    let mut cell = TableCellMeasure {
        lines: Vec::new(),
        min_content: 0.0,
        max_content: 0.0,
        weight: if header {
            TABLE_HEADER_WRAP_WEIGHT
        } else {
            TABLE_BODY_WRAP_WEIGHT
        },
    };
    let mut line = TableCellMeasureLine::default();
    let mut pending_space: Option<f32> = None;

    for tok in toks {
        if tok.hard_break {
            finish_table_measure_line(&mut cell, &mut line);
            pending_space = None;
            continue;
        }

        if tok.space {
            if !line.words.is_empty() {
                pending_space = Some(text_width_cached(" ", size, tok.slot, faces, width_cache));
            }
            continue;
        }

        let word_width = text_width_cached(&tok.text, size, tok.slot, faces, width_cache);
        if !line.words.is_empty() {
            let space_width = pending_space.take().unwrap_or(0.0);
            line.spaces.push(space_width);
            line.max_content += space_width;
        }
        line.words.push(word_width);
        line.max_content += word_width;
        line.min_content = line.min_content.max(word_width);
    }

    finish_table_measure_line(&mut cell, &mut line);
    cell
}

fn finish_table_measure_line(cell: &mut TableCellMeasure, line: &mut TableCellMeasureLine) {
    if line.words.is_empty() {
        return;
    }
    cell.min_content = cell.min_content.max(line.min_content);
    cell.max_content = cell.max_content.max(line.max_content);
    cell.lines.push(std::mem::take(line));
}

/// Merge owned line tokens into runs of identical style, measuring each run.
fn build_cell_line_owned(
    toks: impl IntoIterator<Item = Tok>,
    size: f32,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
) -> CellWrapLine {
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
                text: t.text,
                link: t.link,
                strike: t.strike,
                width: 0.0,
            });
        }
    }
    let mut total = 0.0;
    for run in &mut runs {
        run.width = text_width_cached(&run.text, size, run.slot, faces, width_cache);
        total += run.width;
    }
    CellWrapLine { runs, width: total }
}

/// Greedily wrap styled cell tokens to `max_width`, preserving per-run styling.
fn wrap_cell_styled(
    toks: &[Tok],
    max_width: f32,
    size: f32,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
) -> Vec<CellWrapLine> {
    let mut lines = Vec::new();
    let mut cur: Vec<Tok> = Vec::new();
    let mut cur_w = 0.0;
    let mut pending: Option<Tok> = None;
    for t in toks {
        if t.hard_break {
            pending = None;
            if !cur.is_empty() {
                lines.push(build_cell_line_owned(
                    std::mem::take(&mut cur),
                    size,
                    faces,
                    width_cache,
                ));
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
        let ww = text_width_cached(&t.text, size, t.slot, faces, width_cache);
        // A single word wider than the whole column is hard-split on character
        // boundaries so it can never overflow the column (and run off the page);
        // this preserves the run's style. The leftover tail stays on the current
        // line so a following word can still pack after it.
        if ww > max_width && max_width > 0.0 {
            if !cur.is_empty() {
                lines.push(build_cell_line_owned(
                    std::mem::take(&mut cur),
                    size,
                    faces,
                    width_cache,
                ));
                cur_w = 0.0;
            }
            pending = None;
            let mut buf = [0u8; 4];
            let mut chunk = String::new();
            let mut chunk_w = 0.0;
            for ch in t.text.chars() {
                let cw =
                    text_width_cached(ch.encode_utf8(&mut buf), size, t.slot, faces, width_cache);
                if !chunk.is_empty() && chunk_w + cw > max_width {
                    let tok = Tok {
                        text: std::mem::take(&mut chunk),
                        slot: t.slot,
                        space: false,
                        hard_break: false,
                        link: t.link.clone(),
                        strike: t.strike,
                    };
                    lines.push(build_cell_line_owned(
                        std::iter::once(tok),
                        size,
                        faces,
                        width_cache,
                    ));
                    chunk_w = 0.0;
                }
                chunk.push(ch);
                chunk_w += cw;
            }
            if !chunk.is_empty() {
                cur.push(Tok {
                    text: chunk,
                    slot: t.slot,
                    space: false,
                    hard_break: false,
                    link: t.link.clone(),
                    strike: t.strike,
                });
                cur_w = chunk_w;
            }
            continue;
        }
        let sw = pending.as_ref().map_or(0.0, |s| {
            text_width_cached(" ", size, s.slot, faces, width_cache)
        });
        if !cur.is_empty() && cur_w + sw + ww > max_width {
            lines.push(build_cell_line_owned(
                std::mem::take(&mut cur),
                size,
                faces,
                width_cache,
            ));
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
        lines.push(build_cell_line_owned(
            std::mem::take(&mut cur),
            size,
            faces,
            width_cache,
        ));
    }
    lines
}

fn table_column_badness(column: &TableColumnMetrics, width: f32) -> f32 {
    let width = width.max(1.0);
    let mut badness = 0.0;

    for cell in &column.cells {
        badness += table_cell_wrap_badness(cell, width);
    }

    if column.min_content > width {
        let shortage = (column.min_content - width) / column.min_content.max(1.0);
        badness += shortage * shortage * 2_000.0;
    }

    if column.max_content > 0.0 {
        if width > column.max_content {
            let surplus = width - column.max_content;
            badness += surplus * surplus * 0.01;
        }
    } else {
        let surplus = (width - TABLE_MIN_COL_WIDTH).max(0.0);
        badness += surplus + surplus * surplus * 0.01;
    }

    badness
}

fn table_cell_wrap_badness(cell: &TableCellMeasure, width: f32) -> f32 {
    cell.lines
        .iter()
        .map(|line| {
            let stats = table_measure_line_wrap_stats(line, width);
            let extra_lines = stats.visual_lines.saturating_sub(1) as f32;
            let wrap_penalty = extra_lines * extra_lines * 1_000.0;
            cell.weight * (wrap_penalty + stats.split_penalty + stats.ragged_penalty)
        })
        .sum()
}

struct TableMeasureWrapStats {
    visual_lines: usize,
    split_penalty: f32,
    ragged_penalty: f32,
}

fn table_measure_line_wrap_stats(line: &TableCellMeasureLine, width: f32) -> TableMeasureWrapStats {
    if line.words.is_empty() {
        return TableMeasureWrapStats {
            visual_lines: 0,
            split_penalty: 0.0,
            ragged_penalty: 0.0,
        };
    }

    let width = width.max(1.0);
    let mut visual_lines = 1usize;
    let mut split_penalty = 0.0;
    let mut ragged_penalty = 0.0;
    let mut current = 0.0;

    for (idx, &word_width) in line.words.iter().enumerate() {
        let space_width = if current > 0.0 {
            line.spaces.get(idx.wrapping_sub(1)).copied().unwrap_or(0.0)
        } else {
            0.0
        };

        if word_width > width {
            if current > 0.0 {
                ragged_penalty += table_ragged_line_penalty(current, width);
                visual_lines += 1;
            }

            let pieces = (word_width / width).ceil().max(1.0) as usize;
            let shortage = (word_width - width).max(0.0) / width;
            split_penalty += shortage * shortage * pieces as f32 * 2_500.0;
            visual_lines += pieces.saturating_sub(1);
            current = (word_width - pieces.saturating_sub(1) as f32 * width).clamp(0.0, width);
            continue;
        }

        if current > 0.0 && current + space_width + word_width > width {
            ragged_penalty += table_ragged_line_penalty(current, width);
            visual_lines += 1;
            current = word_width;
        } else {
            current += if current > 0.0 {
                space_width + word_width
            } else {
                word_width
            };
        }
    }

    if current > 0.0 {
        ragged_penalty += table_ragged_line_penalty(current, width);
    }

    TableMeasureWrapStats {
        visual_lines,
        split_penalty,
        ragged_penalty,
    }
}

fn table_ragged_line_penalty(line_width: f32, column_width: f32) -> f32 {
    if column_width <= 0.0 || line_width <= 0.0 {
        return 0.0;
    }
    let slack = ((column_width - line_width).max(0.0) / column_width).min(1.0);
    slack * slack
}

fn allocate_table_column_widths(columns: &[TableColumnMetrics], target: f32) -> Vec<f32> {
    let ncol = columns.len();
    if ncol == 0 {
        return Vec::new();
    }

    let min_total = ncol as f32 * TABLE_MIN_COL_WIDTH;
    let target = target.max(min_total);
    let extra_target = (target - min_total).max(0.0);
    let unit = if extra_target > 0.0 {
        (extra_target / TABLE_ALLOC_MAX_EXTRA_STATES as f32).max(TABLE_ALLOC_MIN_UNIT_PT)
    } else {
        TABLE_ALLOC_MIN_UNIT_PT
    };
    let extra_units = (extra_target / unit).round().max(0.0) as usize;

    if extra_units == 0 {
        let mut widths = vec![TABLE_MIN_COL_WIDTH; ncol];
        finish_table_allocated_widths(columns, &mut widths, target);
        return widths;
    }

    let costs: Vec<Vec<f32>> = columns
        .iter()
        .map(|column| {
            (0..=extra_units)
                .map(|units| {
                    table_column_badness(column, TABLE_MIN_COL_WIDTH + units as f32 * unit)
                })
                .collect()
        })
        .collect();

    let mut previous = vec![f32::INFINITY; extra_units + 1];
    previous[0] = 0.0;
    let mut parents = vec![vec![0usize; extra_units + 1]; ncol];

    for col_idx in 0..ncol {
        let mut next = vec![f32::INFINITY; extra_units + 1];
        for (used, &base) in previous.iter().enumerate() {
            if !base.is_finite() {
                continue;
            }
            for (add, &cost) in costs[col_idx]
                .iter()
                .take(extra_units - used + 1)
                .enumerate()
            {
                let total = used + add;
                let candidate = base + cost;
                if candidate < next[total] {
                    next[total] = candidate;
                    parents[col_idx][total] = add;
                }
            }
        }
        previous = next;
    }

    if !previous[extra_units].is_finite() {
        let mut widths = vec![target / ncol as f32; ncol];
        finish_table_allocated_widths(columns, &mut widths, target);
        return widths;
    }

    let mut units_by_col = vec![0usize; ncol];
    let mut remaining = extra_units;
    for col_idx in (0..ncol).rev() {
        let add = parents[col_idx][remaining];
        units_by_col[col_idx] = add;
        remaining = remaining.saturating_sub(add);
    }

    let mut widths: Vec<f32> = units_by_col
        .iter()
        .map(|&units| TABLE_MIN_COL_WIDTH + units as f32 * unit)
        .collect();
    finish_table_allocated_widths(columns, &mut widths, target);
    widths
}

fn finish_table_allocated_widths(columns: &[TableColumnMetrics], widths: &mut [f32], target: f32) {
    let sum: f32 = widths.iter().sum();
    let delta = target - sum;
    if delta.abs() <= 0.001 || widths.is_empty() {
        return;
    }

    if delta > 0.0 {
        let mut best = 0usize;
        let mut best_deficit = f32::NEG_INFINITY;
        for (idx, width) in widths.iter().enumerate() {
            let deficit = columns
                .get(idx)
                .map_or(0.0, |column| column.max_content - *width);
            if deficit > best_deficit {
                best_deficit = deficit;
                best = idx;
            }
        }
        widths[best] += delta;
        return;
    }

    let mut remaining = -delta;
    loop {
        let mut best: Option<usize> = None;
        let mut best_room = 0.0;
        for (idx, width) in widths.iter().enumerate() {
            let room = (*width - TABLE_MIN_COL_WIDTH).max(0.0);
            if room > best_room {
                best_room = room;
                best = Some(idx);
            }
        }

        let Some(idx) = best else {
            break;
        };
        let take = remaining.min(best_room);
        widths[idx] -= take;
        remaining -= take;
        if remaining <= 0.001 {
            break;
        }
    }
}

/// Lay out a GFM pipe table as a measured-column grid: a bold header row with a
/// rule beneath it and a closing rule (booktabs-style). Column widths are chosen
/// by minimizing predicted wrapping badness under the available page measure.
fn layout_table(
    table: &Table,
    indent: f32,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
    page: PageGeom,
    group: u32,
    out: &mut Vec<Line>,
) {
    let size = TABLE_FONT_SIZE;
    let ncol = table
        .head
        .len()
        .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
    if ncol == 0 {
        return;
    }
    let left = page.left + indent;
    let avail = (page.content_w - indent).max(72.0);
    let pad = TABLE_COL_GUTTER; // inter-column gutter (half on each side of a column)

    // Tokenize each cell once. Measurement and wrapping use the same styled
    // token stream, so table layout avoids repeating inline/style work while
    // keeping header bolding, links, and strikethrough behavior identical.
    let head_toks: Vec<Vec<Tok>> = table
        .head
        .iter()
        .map(|cell| cell_tokens(cell, true))
        .collect();
    let row_toks: Vec<Vec<Vec<Tok>>> = table
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| cell_tokens(cell, false)).collect())
        .collect();

    // Measure min-content, max-content, and wrap-cost inputs once per cell so
    // the allocator can search candidate widths without reshaping text inside
    // its dynamic-programming loop.
    let mut columns = vec![TableColumnMetrics::default(); ncol];
    for (k, toks) in head_toks.iter().enumerate() {
        if let Some(column) = columns.get_mut(k) {
            column.push(table_cell_measure(toks, size, faces, width_cache, true));
        }
    }
    for row in &row_toks {
        for (k, toks) in row.iter().enumerate() {
            if let Some(column) = columns.get_mut(k) {
                column.push(table_cell_measure(toks, size, faces, width_cache, false));
            }
        }
    }

    let gutters = pad * ncol as f32;
    let target = (avail - gutters).max(ncol as f32 * TABLE_MIN_COL_WIDTH);
    let colw = allocate_table_column_widths(&columns, target);

    // Text-left x for each column (inset by half a gutter).
    let mut tx = Vec::with_capacity(ncol);
    let mut cx = left;
    for &w in &colw {
        tx.push(cx + pad / 2.0);
        cx += w + pad;
    }

    let row_lines = |cells: &[Vec<Tok>], gap_after: f32, kind: FlowKind, shade: bool| {
        // Wrap each cell's STYLED tokens to its column width so bold/italic/
        // code faces, strikethrough, and clickable links survive (they used
        // to be flattened to one plain slot per cell).
        let wrapped: Vec<Vec<CellWrapLine>> = (0..ncol)
            .map(|k| {
                let cw = colw.get(k).copied().unwrap_or(TABLE_MIN_COL_WIDTH);
                let toks = cells.get(k).map(Vec::as_slice).unwrap_or(&[]);
                wrap_cell_styled(toks, cw, size, faces, width_cache)
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
                let Some(cell_line) = wrapped.get(k).and_then(|parts| parts.get(row_idx)) else {
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

    out.extend(row_lines(&head_toks, 3.0, FlowKind::TableHeader, false));
    out.push(rule(3.0));
    let nrows = row_toks.len();
    for (i, row) in row_toks.iter().enumerate() {
        // Zebra striping: tint every other body row (0-based even rows) for a
        // modern look. Deterministic from the logical row index.
        out.extend(row_lines(
            row,
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

/// Per-slot box width: shape each same-segment run with that slot's GSUB
/// ligatures and GPOS kerning, matching the content-stream text path. Cross-slot
/// kerning is intentionally dropped because the renderer emits separate text
/// segments for distinct faces/link/strike groups.
#[cfg(test)]
fn measure_word(runs: &[Tok], fs: FontSize, faces: &Faces) -> LayoutUnit {
    if let [tok] = runs {
        return faces.shaped_width(tok.slot, &tok.text, fs);
    }

    let mut total = LayoutUnit::ZERO;
    let mut current: Option<TokMeasureRun> = None;
    for tok in runs {
        match &mut current {
            Some(run)
                if run.slot == tok.slot && run.link == tok.link && run.strike == tok.strike =>
            {
                run.text.push_str(&tok.text);
            }
            _ => {
                if let Some(run) = current.take() {
                    total += faces.shaped_width(run.slot, &run.text, fs);
                }
                current = Some(TokMeasureRun {
                    slot: tok.slot,
                    link: tok.link.clone(),
                    strike: tok.strike,
                    text: tok.text.clone(),
                });
            }
        }
    }
    if let Some(run) = current {
        total += faces.shaped_width(run.slot, &run.text, fs);
    }
    total
}

fn measure_word_cached(
    runs: &[Tok],
    fs: FontSize,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
) -> LayoutUnit {
    if let [tok] = runs {
        return cached_shaped_width(faces, width_cache, tok.slot, &tok.text, fs);
    }

    let mut total = LayoutUnit::ZERO;
    let mut current: Option<TokMeasureRun> = None;
    for tok in runs {
        match &mut current {
            Some(run)
                if run.slot == tok.slot && run.link == tok.link && run.strike == tok.strike =>
            {
                run.text.push_str(&tok.text);
            }
            _ => {
                if let Some(run) = current.take() {
                    total += cached_shaped_width(faces, width_cache, run.slot, &run.text, fs);
                }
                current = Some(TokMeasureRun {
                    slot: tok.slot,
                    link: tok.link.clone(),
                    strike: tok.strike,
                    text: tok.text.clone(),
                });
            }
        }
    }
    if let Some(run) = current {
        total += cached_shaped_width(faces, width_cache, run.slot, &run.text, fs);
    }
    total
}

fn cached_shaped_width(
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
    slot: u8,
    text: &str,
    fs: FontSize,
) -> LayoutUnit {
    let key = (slot, fs.milli_points());
    {
        let cache = width_cache.borrow();
        if let Some(width) = cache.get(&key).and_then(|slot_cache| slot_cache.get(text)) {
            return *width;
        }
    }

    let width = faces.shaped_width(slot, text, fs);
    let mut cache = width_cache.borrow_mut();
    let cache_len: usize = cache.values().map(HashMap::len).sum();
    if cache_len < WIDTH_CACHE_MAX {
        cache
            .entry(key)
            .or_default()
            .insert(text.to_string(), width);
    }
    width
}

fn shaped_width_points_for_layout(
    faces: &Faces,
    width_cache: Option<&RefCell<WidthCache>>,
    slot: u8,
    text: &str,
    fs: FontSize,
) -> f32 {
    match width_cache {
        Some(cache) => cached_shaped_width(faces, cache, slot, text, fs).to_points_f32(),
        None => faces.shaped_width(slot, text, fs).to_points_f32(),
    }
}

struct TokMeasureRun {
    slot: u8,
    link: Option<LinkTarget>,
    strike: bool,
    text: String,
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
    width_cache: &'a RefCell<WidthCache>,
) -> BuiltParagraph {
    let mut built = BuiltParagraph::with_capacity(toks.len().saturating_add(1));
    let mut word: Vec<Tok> = Vec::new();
    let hyphenator = Hyphenator::english();
    let word_cx = PdfWordContext {
        fs,
        faces,
        policy,
        hyphenator: &hyphenator,
        hyphen_cache,
        width_cache,
    };

    for tok in toks {
        if tok.space {
            if tok.hard_break {
                if !word.is_empty() {
                    flush_pdf_word(
                        &mut built.items,
                        &mut built.item_toks,
                        &mut built.break_toks,
                        &mut word,
                        word_cx,
                    );
                }
                built.items.push(ParagraphItem::Penalty(Penalty {
                    width: LayoutUnit::ZERO,
                    penalty: FORCED_BREAK_PENALTY,
                    flagged: false,
                }));
                built.item_toks.push(TokGroup::empty());
                built.break_toks.push(None);
                continue;
            }
            if !word.is_empty() {
                flush_pdf_word(
                    &mut built.items,
                    &mut built.item_toks,
                    &mut built.break_toks,
                    &mut word,
                    word_cx,
                );
            }
            // Only emit glue *between* two words (collapses runs of spaces).
            if matches!(built.items.last(), Some(ParagraphItem::Box(_))) {
                let gw = cached_shaped_width(faces, width_cache, tok.slot, " ", fs);
                built
                    .items
                    .push(ParagraphItem::Glue(default_interword_glue(gw)));
                built.item_toks.push(TokGroup::one(tok.clone()));
                built.break_toks.push(None);
            }
        } else {
            word.push(tok.clone());
        }
    }
    flush_pdf_word(
        &mut built.items,
        &mut built.item_toks,
        &mut built.break_toks,
        &mut word,
        word_cx,
    );

    if !matches!(
        built.items.last(),
        Some(ParagraphItem::Penalty(Penalty {
            penalty: FORCED_BREAK_PENALTY,
            ..
        }))
    ) {
        built.items.push(ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: FORCED_BREAK_PENALTY,
            flagged: false,
        }));
        built.item_toks.push(TokGroup::empty());
        built.break_toks.push(None);
    }
    debug_assert_eq!(built.items.len(), built.item_toks.len());
    debug_assert_eq!(built.items.len(), built.break_toks.len());
    built
}

fn flush_pdf_word(
    items: &mut Vec<ParagraphItem>,
    item_toks: &mut Vec<TokGroup>,
    break_toks: &mut Vec<Option<Tok>>,
    word: &mut Vec<Tok>,
    cx: PdfWordContext<'_>,
) {
    if word.is_empty() {
        return;
    }

    if !cx.policy.hyphenate || !pdf_word_is_ascii_alphabetic(word) {
        push_pdf_word_box(
            items,
            item_toks,
            break_toks,
            word,
            cx.fs,
            cx.faces,
            cx.width_cache,
        );
        return;
    }

    let plain: String = word.iter().map(|t| t.text.as_str()).collect();
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
    let points = cached.unwrap_or_else(|| {
        let pts = cx
            .hyphenator
            .hyphenation_points(&plain, HyphenationOptions::default());
        // Only cache words that actually hyphenate. A non-hyphenating word gains
        // nothing from caching, so skipping the insert (no key clone, no map entry)
        // keeps unique / non-hyphenating corpora from paying any cache cost — they
        // do not regress — while repeated hyphenating words still hit.
        if !pts.is_empty() {
            let mut cache = cx.hyphen_cache.borrow_mut();
            if cache.len() < HYPHEN_CACHE_MAX {
                cache.insert(key.into_owned(), pts.clone());
            }
        }
        pts
    });

    if points.is_empty() {
        push_pdf_word_box(
            items,
            item_toks,
            break_toks,
            word,
            cx.fs,
            cx.faces,
            cx.width_cache,
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
                cached_shaped_width(cx.faces, cx.width_cache, tok.slot, "-", cx.fs)
            });
            push_pdf_word_box_from_vec(
                items,
                item_toks,
                break_toks,
                part,
                cx.fs,
                cx.faces,
                cx.width_cache,
            );
            items.push(ParagraphItem::Penalty(Penalty {
                width: hyphen_width,
                penalty: 50,
                flagged: true,
            }));
            item_toks.push(TokGroup::empty());
            break_toks.push(hyphen_tok);
        }
        start = point;
    }

    let tail = split_pdf_word_tokens(word, start, plain.chars().count());
    if !tail.is_empty() {
        push_pdf_word_box_from_vec(
            items,
            item_toks,
            break_toks,
            tail,
            cx.fs,
            cx.faces,
            cx.width_cache,
        );
    }
    word.clear();
}

fn pdf_word_is_ascii_alphabetic(word: &[Tok]) -> bool {
    word.iter()
        .all(|tok| tok.text.bytes().all(|byte| byte.is_ascii_alphabetic()))
}

fn push_pdf_word_box(
    items: &mut Vec<ParagraphItem>,
    item_toks: &mut Vec<TokGroup>,
    break_toks: &mut Vec<Option<Tok>>,
    toks: &mut Vec<Tok>,
    fs: FontSize,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
) {
    if toks.is_empty() {
        return;
    }
    let width = measure_word_cached(toks, fs, faces, width_cache);
    // The PDF layout path maps chosen breaks back to `item_toks` for actual
    // rendering. `TextBox` only feeds the generic breaker, which reads `width`.
    items.push(ParagraphItem::Box(TextBox {
        text: String::new(),
        runs: Default::default(),
        width,
    }));
    item_toks.push(TokGroup::take_from(toks));
    break_toks.push(None);
}

fn push_pdf_word_box_from_vec(
    items: &mut Vec<ParagraphItem>,
    item_toks: &mut Vec<TokGroup>,
    break_toks: &mut Vec<Option<Tok>>,
    toks: Vec<Tok>,
    fs: FontSize,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
) {
    if toks.is_empty() {
        return;
    }
    let width = measure_word_cached(&toks, fs, faces, width_cache);
    items.push(ParagraphItem::Box(TextBox {
        text: String::new(),
        runs: Default::default(),
        width,
    }));
    item_toks.push(TokGroup::from_vec(toks));
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
    cx: &mut LayoutCx<'_>,
    flow: FlowSpec,
) {
    let start = out.len();
    let left = cx.page.left + indent;
    let fs = font_size_of(size);
    let policy = ParagraphPolicy::for_flow(flow.kind);
    let built = build_paragraph(
        &toks,
        fs,
        cx.faces,
        policy,
        &cx.hyphen_cache,
        &cx.width_cache,
    );

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
    cx.break_paragraph(&built.items, content_w);
    if cx.line_breaks.is_empty() {
        // Emergency fallback: the optimizer produced nothing.
        layout_inlines_greedy(toks, indent, size, gap_after, cx.faces, cx.page, out);
        mark_flow(out, start, flow.group, flow.kind);
        return;
    }

    let n = cx.line_breaks.len();
    for i in 0..n {
        let lb = cx.line_breaks[i];
        line_tokens_for_break_into(
            &built,
            &lb,
            content_w,
            policy.justify && i + 1 < n,
            &mut cx.glue_adjustments,
            &mut cx.line_toks,
        );
        let segs = build_segs_adjusted(&cx.line_toks, left, size, cx.faces, Some(&cx.width_cache));
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
    cx: &mut LayoutCx<'_>,
) {
    let start = out.len();
    let left = cx.page.left + spec.content_indent;
    let fs = font_size_of(spec.size);
    let policy = ParagraphPolicy::for_flow(spec.flow.kind);
    let built = build_paragraph(
        &toks,
        fs,
        cx.faces,
        policy,
        &cx.hyphen_cache,
        &cx.width_cache,
    );

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
    cx.break_paragraph(&built.items, content_w);
    if cx.line_breaks.is_empty() {
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

    let n = cx.line_breaks.len();
    let mut marker = Some(marker);
    for i in 0..n {
        let lb = cx.line_breaks[i];
        line_tokens_for_break_into(
            &built,
            &lb,
            content_w,
            policy.justify && i + 1 < n,
            &mut cx.glue_adjustments,
            &mut cx.line_toks,
        );
        let mut segs = build_segs_adjusted(
            &cx.line_toks,
            left,
            spec.size,
            cx.faces,
            Some(&cx.width_cache),
        );
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

fn line_tokens_for_break_into(
    built: &BuiltParagraph,
    lb: &crate::layout::LineBreak,
    line_width: LayoutUnit,
    justify: bool,
    adjustments: &mut Vec<(usize, f32)>,
    line: &mut Vec<LineTok>,
) {
    line.clear();
    glue_adjustments_into(&built.items, lb, line_width, justify, adjustments);
    if adjustments.is_empty() {
        for idx in lb.start..lb.end {
            if let Some(group) = built.item_toks.get(idx) {
                push_tok_group_line_toks(group, 0.0, line);
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
        return;
    }

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
            push_tok_group_line_toks(group, extra, line);
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
}

fn push_tok_group_line_toks(group: &TokGroup, extra: f32, line: &mut Vec<LineTok>) {
    if let Some(tok) = &group.first {
        line.push(LineTok {
            tok: tok.clone(),
            extra_advance: if tok.space { extra } else { 0.0 },
        });
    }
    for tok in &group.rest {
        line.push(LineTok {
            tok: tok.clone(),
            extra_advance: if tok.space { extra } else { 0.0 },
        });
    }
}

fn glue_adjustments_into(
    items: &[ParagraphItem],
    lb: &crate::layout::LineBreak,
    line_width: LayoutUnit,
    justify: bool,
    out: &mut Vec<(usize, f32)>,
) {
    out.clear();
    if !justify || chosen_forced_break(items, lb) {
        return;
    }
    let delta = line_width.milli_points() as i64 - lb.natural_width.milli_points() as i64;
    if delta == 0 {
        return;
    }
    let mut total = 0i64;
    let mut glue_count = 0usize;
    for item in items.iter().take(lb.end).skip(lb.start) {
        if let ParagraphItem::Glue(glue) = item {
            let flex = glue_flex(*glue, delta);
            if flex > 0 {
                total = total.saturating_add(flex);
                glue_count += 1;
            }
        }
    }
    if total <= 0 || glue_count == 0 {
        return;
    }

    out.reserve(glue_count);
    let mut assigned = 0i64;
    let mut glue_pos = 0usize;
    for (idx, item) in items.iter().enumerate().take(lb.end).skip(lb.start) {
        let ParagraphItem::Glue(glue) = item else {
            continue;
        };
        let flex = glue_flex(*glue, delta);
        if flex <= 0 {
            continue;
        }
        glue_pos += 1;
        let extra = if glue_pos == glue_count {
            delta.saturating_sub(assigned)
        } else {
            delta.saturating_mul(flex) / total
        };
        assigned = assigned.saturating_add(extra);
        out.push((idx, extra as f32 / 1000.0));
    }
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
/// accumulating each segment's shaped layout advance width.
fn build_segs(toks: &[Tok], left: f32, size: f32, faces: &Faces) -> Vec<Seg> {
    let line_toks = toks
        .iter()
        .cloned()
        .map(|tok| LineTok {
            tok,
            extra_advance: 0.0,
        })
        .collect::<Vec<_>>();
    build_segs_adjusted(&line_toks, left, size, faces, None)
}

fn build_segs_adjusted(
    toks: &[LineTok],
    left: f32,
    size: f32,
    faces: &Faces,
    width_cache: Option<&RefCell<WidthCache>>,
) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut x = left;
    let mut cur: Option<Seg> = None;
    let fs = font_size_of(size);
    for line_tok in toks {
        let tok = &line_tok.tok;
        let advance;
        match &mut cur {
            Some(s) if s.slot == tok.slot && s.link == tok.link && s.strike == tok.strike => {
                let old_width = s.width;
                s.text.push_str(&tok.text);
                s.width = shaped_width_points_for_layout(faces, width_cache, s.slot, &s.text, fs)
                    + line_tok.extra_advance;
                advance = s.width - old_width;
            }
            _ => {
                if let Some(s) = cur.take() {
                    segs.push(s);
                }
                advance =
                    shaped_width_points_for_layout(faces, width_cache, tok.slot, &tok.text, fs)
                        + line_tok.extra_advance;
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
    preserve_lines: bool,
    faces: &'a Faces,
}

fn preserve_code_block_lines(lang: Option<&str>, code: &str) -> bool {
    let Some(lang) = lang else {
        return looks_like_ascii_diagram(code);
    };
    let lang = code_language_key(lang);
    if matches!(
        lang.as_str(),
        "mermaid" | "mmd" | "dot" | "graphviz" | "ascii" | "diagram"
    ) {
        return true;
    }
    if matches!(lang.as_str(), "text" | "txt" | "plain" | "plaintext") {
        return looks_like_ascii_diagram(code);
    }
    false
}

fn code_language_key(lang: &str) -> String {
    let lower = lang.trim().to_ascii_lowercase();
    let without_prefix = lower.strip_prefix("language-").unwrap_or(&lower);
    let end = without_prefix
        .char_indices()
        .find_map(|(idx, ch)| {
            (!(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '#' | '-' | '_'))).then_some(idx)
        })
        .unwrap_or(without_prefix.len());
    without_prefix[..end].to_string()
}

fn looks_like_ascii_diagram(code: &str) -> bool {
    let mut structural_lines = 0usize;
    let mut long_structural_line = false;

    for line in code.lines().map(str::trim_end) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let char_count = trimmed.chars().count();
        let structural = trimmed
            .chars()
            .filter(|&ch| is_ascii_diagram_char(ch))
            .count();
        let has_connector = trimmed.contains("--")
            || trimmed.contains("->")
            || trimmed.contains("<-")
            || trimmed.contains("=>")
            || trimmed.contains("+")
            || trimmed.contains('|');

        if structural >= 3 && has_connector && structural * 5 >= char_count {
            structural_lines += 1;
        }
        if char_count >= 48 && structural >= 6 && has_connector {
            long_structural_line = true;
        }
    }

    structural_lines >= 2 || long_structural_line
}

const fn is_ascii_diagram_char(ch: char) -> bool {
    matches!(
        ch,
        '|' | '-'
            | '+'
            | '/'
            | '\\'
            | '_'
            | '='
            | ':'
            | '<'
            | '>'
            | '['
            | ']'
            | '{'
            | '}'
            | '('
            | ')'
            | '*'
    )
}

fn fitted_code_font_size(
    code: &str,
    code_area_width: f32,
    line_numbers: bool,
    digits: usize,
    default_size: f32,
    min_size: f32,
    faces: &Faces,
) -> f32 {
    let longest = code
        .lines()
        .map(expand_code_tabs)
        .map(|line| text_width(&line, default_size, F_MONO, faces))
        .fold(0.0f32, f32::max);
    if longest <= 0.0 {
        return default_size;
    }

    let fits = |size: f32| {
        let number_col = if line_numbers {
            code_line_number_column_width(digits, size, faces)
        } else {
            0.0
        };
        let available = (code_area_width - number_col).max(8.0);
        longest * (size / default_size) <= available
    };

    if fits(default_size) {
        return default_size;
    }

    let mut low = min_size.min(default_size);
    let mut high = default_size;
    for _ in 0..18 {
        let mid = (low + high) * 0.5;
        if fits(mid) {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

fn wrapped_code_rows(text: &str, spec: CodeWrapSpec<'_>) -> Vec<Vec<Seg>> {
    let first_text_x = spec.x0 + spec.number_col;
    if spec.preserve_lines {
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
        segs.extend(code_frags_to_segs(
            &code_fragments(spec.lang, text),
            first_text_x,
            spec.size,
            spec.faces,
        ));
        if segs.is_empty() {
            segs.push(empty_code_seg(first_text_x));
        }
        return vec![segs];
    }

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

    let mut frags = Vec::new();
    let Some(lang) = lang else {
        return vec![CodeFrag {
            text: expand_code_tabs(text),
            fill: Fill::Black,
        }];
    };

    // The highlighter falls back to one plain span for unknown languages, so
    // code blocks with a language only pay for one lexer lookup here.
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
    faces.shaped_width_points(tok.slot, &tok.text, size)
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
    let slot_texts = collect_font_slot_text_refs(lines);
    let mut used_slots: Vec<u8> = SLOTS
        .into_iter()
        .zip(slot_texts.iter())
        .filter_map(|(slot, refs)| (!refs.is_empty()).then_some(slot))
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
        let face = faces.face(slot);
        let source = &face.font;
        let lig = &face.lig;
        let collect_started = profiler.checkpoint();
        let mut chars: BTreeSet<char> = BTreeSet::new();
        let mut shaped_glyphs: BTreeSet<u16> = BTreeSet::new();
        let mut lig_src_uni: BTreeMap<u16, String> = BTreeMap::new();
        let mut segment_count = 0usize;
        let mut text_bytes = 0usize;
        let Some(slot_idx) = pdf_font_slot_index(slot) else {
            continue;
        };
        let slot_refs = &slot_texts[slot_idx];
        for seg in &slot_refs.segs {
            segment_count += 1;
            text_bytes += seg.text.len();
            chars.extend(seg.text.chars());
            let slot_cache = shaped_cache.entry(slot).or_default();
            if let Some(shaped) = slot_cache.get(seg.text.as_str()) {
                shape_cache_hits += 1;
                shape_cache_hit_bytes += seg.text.len();
                collect_shaped_run_glyphs(shaped, &mut shaped_glyphs, &mut lig_src_uni);
            } else {
                shape_cache_misses += 1;
                shape_cache_miss_bytes += seg.text.len();
                let shaped = shape_run(source, lig, &seg.text);
                collect_shaped_run_glyphs(&shaped, &mut shaped_glyphs, &mut lig_src_uni);
                slot_cache.insert(seg.text.clone(), shaped);
            }
        }
        for text in &slot_refs.svg_texts {
            segment_count += 1;
            text_bytes += text.text.len();
            chars.extend(text.text.chars());
            let slot_cache = shaped_cache.entry(slot).or_default();
            if let Some(shaped) = slot_cache.get(text.text.as_str()) {
                shape_cache_hits += 1;
                shape_cache_hit_bytes += text.text.len();
                collect_shaped_run_glyphs(shaped, &mut shaped_glyphs, &mut lig_src_uni);
            } else {
                shape_cache_misses += 1;
                shape_cache_miss_bytes += text.text.len();
                let shaped = shape_run(source, lig, &text.text);
                collect_shaped_run_glyphs(&shaped, &mut shaped_glyphs, &mut lig_src_uni);
                slot_cache.insert(text.text.clone(), shaped);
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
            kern: face.kern.clone(),
            lig: face.lig.clone(),
            map,
            cmap_chars: keep,
            lig_uni,
        });
    }
    let subset_lookup = EmbeddedFaceLookup::new(&subsets);
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
        let mut shadings = Vec::new();
        let mut annots = Vec::with_capacity(annot_capacity);
        let mut marks = Vec::with_capacity(mark_capacity);
        let mut next_mcid = 0usize;

        // (a) Blockquote backgrounds: subtle page-local panels behind quoted
        // content, using the same extents as the gutter bars.
        let mut quote_acc = quote_extents(placed);
        for (bar_x, top_y, bot_y) in quote_acc.values() {
            append_rounded_rect_fill(
                &mut bg,
                bar_x - QUOTE_BG_PAD_X,
                bot_y - QUOTE_BG_PAD_V,
                page.right_x(),
                top_y + QUOTE_BG_PAD_V,
                3.0,
                palette.quote_bg,
            );
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
            append_rounded_rect_fill(
                &mut bg,
                p.line.rule_x,
                bot_y,
                page.right_x(),
                top_y,
                0.0,
                palette.table_stripe,
            );
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
                append_rounded_rect_fill(
                    &mut bg,
                    x0,
                    bot_y,
                    x1,
                    top_y,
                    PANEL_RADIUS,
                    palette.code_panel_bg,
                );
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
                let cx0 = seg.x - CHIP_PAD_X;
                let cx1 = seg.x + seg.width + CHIP_PAD_X;
                let cy0 = p.y - p.line.size * 0.26;
                let cy1 = p.y + p.line.size * 0.74;
                append_rounded_rect_fill(
                    &mut bg,
                    cx0,
                    cy0,
                    cx1,
                    cy1,
                    CHIP_RADIUS,
                    palette.code_chip_bg,
                );
            }
        }

        // (d) Text + rules. Prime the nonstroking color to the theme body color
        // so the first run (which equals `current_fill` and would otherwise skip
        // emitting `rg`) renders in the theme `fg`, not PDF-default black.
        let mut current_fill = Fill::Black;
        append_rgb_fill_operator(&mut body, palette.fg);
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
                append_artifact_rule_stroke(
                    &mut body,
                    (rr, rg, rb),
                    0.7,
                    line.rule_x,
                    y + line.size * 0.5,
                    x2,
                );
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
                    append_marked_content_begin(&mut body, cell_tag, next_mcid);
                    draw_seg(
                        &mut body,
                        &mut annots,
                        &mut current_fill,
                        next_mcid,
                        seg,
                        line.size,
                        y,
                        &subsets,
                        &subset_lookup,
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
                    append_marked_content_begin(&mut body, leaf.tag, next_mcid);
                    let mut path = container_prefix(line);
                    path.push(leaf);
                    let (alt, bbox) = if let Some(image) = &line.image {
                        let x0 = line.rule_x;
                        let y1 = y + image.height_pt;
                        (
                            Some(pdf_image_alt_text(image)),
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
                if let Some(image) = &line.image {
                    if image.image.vector.is_some() {
                        draw_svg_image(
                            &mut body,
                            &mut annots,
                            image,
                            line.rule_x,
                            y,
                            &image_index,
                            &mut shadings,
                            &subsets,
                            &subset_lookup,
                            faces,
                            &shaped_cache,
                        );
                    } else if let Some(idx) = image_index.get(image.image.key.as_str()) {
                        append_image_xobject_do(
                            &mut body,
                            *idx,
                            image.width_pt,
                            image.height_pt,
                            line.rule_x,
                            y,
                        );
                    }
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
                        &subset_lookup,
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
            shadings,
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
            shadings: Vec::new(),
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
    shadings: Vec<PdfShading>,
    annots: Vec<LinkAnnotation>,
    marks: Vec<StructMark>,
}

/// Identity of a structure element, used to share a container across the
/// consecutive marks that belong to it (e.g. every cell of one table row reuses
/// the same `TableRow`, every wrapped line of one paragraph reuses the same
/// `Paragraph`). Two marks open/extend the same element iff their keys compare
/// equal at the same path depth.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
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
        // Existing child of a parent, keyed by (parent node, element key). Reset
        // per page so a block split across a page break becomes two siblings.
        // This lets a logical element's fragments re-find their node even when a
        // sibling intervened — e.g. a table cell that wraps onto later lines
        // reappears after the other columns, and MUST extend its existing /TD
        // rather than spawn a duplicate one (which would tear reading order).
        let mut child_by_key: BTreeMap<(usize, SKey), usize> = BTreeMap::new();

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
                if let Some(&existing) = child_by_key.get(&(parent_node, elem.key.clone())) {
                    // A fragment of an element that already exists under this
                    // parent (a wrapped/re-entered cell, list item, ...): extend it.
                    open.push((elem.key.clone(), existing));
                    continue;
                }
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
                child_by_key.insert((parent_node, elem.key.clone()), node_index);
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
        if let Some(svg) = image.image.vector.as_ref() {
            collect_svg_pdf_images(svg, &mut by_key);
        } else {
            by_key
                .entry(image.image.key.clone())
                .or_insert_with(|| image.image.clone());
        }
    }
    by_key.into_values().collect()
}

fn collect_svg_pdf_images(svg: &PdfSvgImage, by_key: &mut BTreeMap<String, PdfImageData>) {
    for element in &svg.elements {
        if let SvgElement::Image(image) = element {
            if let Some(nested_svg) = image.image.vector.as_ref() {
                collect_svg_pdf_images(nested_svg, by_key);
            } else {
                by_key
                    .entry(image.image.key.clone())
                    .or_insert_with(|| (*image.image).clone());
            }
        }
    }
}

struct FontSlotTextRefs<'a> {
    segs: Vec<&'a Seg>,
    svg_texts: Vec<&'a SvgText>,
}

impl FontSlotTextRefs<'_> {
    fn is_empty(&self) -> bool {
        self.segs.is_empty() && self.svg_texts.is_empty()
    }
}

fn collect_font_slot_text_refs(lines: &[Line]) -> [FontSlotTextRefs<'_>; SLOTS.len()] {
    let mut refs: [FontSlotTextRefs<'_>; SLOTS.len()] = std::array::from_fn(|_| FontSlotTextRefs {
        segs: Vec::new(),
        svg_texts: Vec::new(),
    });
    for line in lines {
        for seg in &line.segs {
            if seg.text.is_empty() {
                continue;
            }
            if let Some(slot_idx) = pdf_font_slot_index(seg.slot) {
                refs[slot_idx].segs.push(seg);
            }
        }
        for text in svg_texts_in_line(line) {
            if text.text.is_empty() {
                continue;
            }
            if let Some(slot_idx) = pdf_font_slot_index(text.slot) {
                refs[slot_idx].svg_texts.push(text);
            }
        }
    }
    refs
}

fn pdf_font_slot_index(slot: u8) -> Option<usize> {
    match slot {
        F_BODY => Some(0),
        F_BOLD => Some(1),
        F_ITALIC => Some(2),
        F_MONO => Some(3),
        F_BOLDITALIC => Some(4),
        _ => None,
    }
}

fn svg_texts_in_line(line: &Line) -> impl Iterator<Item = &SvgText> {
    line.image
        .as_ref()
        .and_then(|image| image.image.vector.as_ref())
        .into_iter()
        .flat_map(|svg| svg.elements.iter())
        .filter_map(|element| match element {
            SvgElement::Text(text) => Some(text),
            _ => None,
        })
}

fn image_name(index: usize) -> String {
    format!("Im{}", index + 1)
}

fn line_has_visible_content(line: &Line) -> bool {
    line.image.is_some() || line.segs.iter().any(|seg| !seg.text.is_empty())
}

#[derive(Clone, Copy)]
struct SvgPaintResources<'a> {
    gradients: &'a [SvgGradientPaint],
    patterns: &'a [SvgPatternPaint],
    clip_paths: &'a [SvgClipPath],
    markers: &'a [SvgMarker],
    image_index: &'a BTreeMap<&'a str, usize>,
    stroke_scale: Option<f32>,
}

#[derive(Clone, Copy)]
struct SvgImageTransform {
    sx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
    viewport_clip: Option<(f32, f32, f32, f32)>,
}

impl SvgImageTransform {
    fn map_point(self, x: f32, y: f32) -> (f32, f32) {
        (self.tx + x * self.sx, self.ty - y * self.sy)
    }
}

fn svg_image_transform(
    svg: &PdfSvgImage,
    image: &ImageLine,
    x: f32,
    y: f32,
) -> Option<SvgImageTransform> {
    let raw_sx = image.width_pt / svg.view_box.w.max(1.0);
    let raw_sy = image.height_pt / svg.view_box.h.max(1.0);
    if !raw_sx.is_finite() || !raw_sy.is_finite() || raw_sx <= 0.0 || raw_sy <= 0.0 {
        return None;
    }
    match svg.preserve_aspect.mode {
        SvgAspectScaleMode::None => {
            let tx = x - svg.view_box.x * raw_sx;
            let ty = y + image.height_pt + svg.view_box.y * raw_sy;
            Some(SvgImageTransform {
                sx: raw_sx,
                sy: raw_sy,
                tx,
                ty,
                viewport_clip: None,
            })
        }
        SvgAspectScaleMode::Meet | SvgAspectScaleMode::Slice => {
            let scale = if svg.preserve_aspect.mode == SvgAspectScaleMode::Slice {
                raw_sx.max(raw_sy)
            } else {
                raw_sx.min(raw_sy)
            };
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            let content_w = svg.view_box.w * scale;
            let content_h = svg.view_box.h * scale;
            let offset_x = (image.width_pt - content_w) * svg.preserve_aspect.align_x;
            let offset_y = (image.height_pt - content_h) * svg.preserve_aspect.align_y;
            let tx = x + offset_x - svg.view_box.x * scale;
            let ty = y + image.height_pt - offset_y + svg.view_box.y * scale;
            Some(SvgImageTransform {
                sx: scale,
                sy: scale,
                tx,
                ty,
                viewport_clip: (svg.preserve_aspect.mode == SvgAspectScaleMode::Slice).then_some((
                    x,
                    y,
                    image.width_pt,
                    image.height_pt,
                )),
            })
        }
    }
}

fn svg_uniform_stroke_scale(transform: SvgImageTransform) -> Option<f32> {
    let sx = transform.sx.abs();
    let sy = transform.sy.abs();
    if sx.is_finite() && sy.is_finite() && sx > 0.001 && sy > 0.001 && (sx - sy).abs() <= 0.0001 {
        Some((sx + sy) * 0.5)
    } else {
        None
    }
}

fn svg_style_with_non_scaling_stroke(style: SvgStyle, stroke_scale: Option<f32>) -> SvgStyle {
    let Some(scale) = stroke_scale else {
        return style;
    };
    if !style.non_scaling_stroke || !svg_transform_preserves_stroke_scale(style.transform) {
        return style;
    }
    SvgStyle {
        stroke_width: style.stroke_width / scale,
        ..style
    }
}

fn svg_transform_preserves_stroke_scale(transform: SvgTransform) -> bool {
    (transform.a - 1.0).abs() <= 0.0001
        && transform.b.abs() <= 0.0001
        && transform.c.abs() <= 0.0001
        && (transform.d - 1.0).abs() <= 0.0001
}

fn append_svg_image_transform_prefix(body: &mut String, transform: SvgImageTransform) {
    body.push_str("q ");
    if let Some((x, y, w, h)) = transform.viewport_clip {
        body.push_str(&format!(
            "{x} {y} {w} {h} re W n ",
            x = pdf_num(x),
            y = pdf_num(y),
            w = pdf_num(w),
            h = pdf_num(h),
        ));
    }
    body.push_str(&format!(
        "{sx} 0 0 {neg_sy} {tx} {ty} cm\n",
        sx = pdf_num(transform.sx),
        neg_sy = pdf_num(-transform.sy),
        tx = pdf_num(transform.tx),
        ty = pdf_num(transform.ty),
    ));
}

fn append_svg_root_background(
    body: &mut String,
    background: &SvgRootBackground,
    image: &ImageLine,
    x: f32,
    y: f32,
    page_shadings: &mut Vec<PdfShading>,
) {
    if ![x, y, image.width_pt, image.height_pt]
        .iter()
        .all(|value| value.is_finite())
        || image.width_pt <= 0.001
        || image.height_pt <= 0.001
    {
        return;
    }
    if let Some(color) = background.color {
        append_svg_root_background_color(body, color, background.opacity, image, x, y);
    }
    let bbox = (x, y, image.width_pt, image.height_pt);
    for layer in background.layers.iter().rev() {
        append_svg_root_background_layer(body, layer, bbox, background.opacity, page_shadings);
    }
}

fn append_svg_root_background_color(
    body: &mut String,
    background: SvgRootBackgroundColor,
    root_opacity: f32,
    image: &ImageLine,
    x: f32,
    y: f32,
) {
    let alpha = quantize_svg_alpha(background.opacity * root_opacity);
    if alpha == 0 {
        return;
    }
    let (r, g, b) = background.color;
    body.push_str("q ");
    if alpha < 1000 {
        append_svg_alpha_state(body, alpha, alpha);
        body.push(' ');
    }
    body.push_str(&format!(
        "{r} {g} {b} rg {x} {y} {w} {h} re f\nQ\n",
        r = pdf_fixed3(r),
        g = pdf_fixed3(g),
        b = pdf_fixed3(b),
        x = pdf_num(x),
        y = pdf_num(y),
        w = pdf_num(image.width_pt),
        h = pdf_num(image.height_pt),
    ));
}

fn append_svg_root_background_layer(
    body: &mut String,
    layer: &SvgRootBackgroundLayer,
    bbox: (f32, f32, f32, f32),
    opacity: f32,
    page_shadings: &mut Vec<PdfShading>,
) {
    let alpha = quantize_svg_alpha(opacity);
    if alpha == 0 {
        return;
    }
    let shadings = match layer {
        SvgRootBackgroundLayer::Linear(linear) => {
            vec![svg_root_linear_background_shading(linear, bbox)]
        }
        SvgRootBackgroundLayer::Radial(radial) => {
            vec![svg_root_radial_background_shading(radial, bbox)]
        }
    };
    if !pdf_shadings_fit(page_shadings, &shadings) {
        return;
    }
    let names = shadings
        .into_iter()
        .filter_map(|shading| register_pdf_shading(page_shadings, shading))
        .collect::<Vec<_>>();
    if names.is_empty() {
        return;
    }
    let (x, y, w, h) = bbox;
    body.push_str("q ");
    if alpha < 1000 {
        append_svg_alpha_state(body, alpha, alpha);
        body.push(' ');
    }
    body.push_str(&format!(
        "{x} {y} {w} {h} re W n ",
        x = pdf_num(x),
        y = pdf_num(y),
        w = pdf_num(w),
        h = pdf_num(h),
    ));
    for name in names {
        body.push_str(&format!("/{name} sh\n"));
    }
    body.push_str("Q\n");
}

fn svg_root_linear_background_shading(
    gradient: &SvgCssLinearGradient,
    bbox: (f32, f32, f32, f32),
) -> PdfShading {
    let (x, y, w, h) = bbox;
    let map = |point: (f32, f32)| (x + point.0 * w, y + point.1 * h);
    let (x1, y1) = map(gradient.start);
    let (x2, y2) = map(gradient.end);
    PdfShading {
        kind: PdfShadingKind::Axial([x1, y1, x2, y2]),
        stops: gradient.stops.clone(),
        extend_start: true,
        extend_end: true,
    }
}

fn svg_root_radial_background_shading(
    gradient: &SvgCssRadialGradient,
    bbox: (f32, f32, f32, f32),
) -> PdfShading {
    let (x, y, w, h) = bbox;
    let cx = x + gradient.center.0 * w;
    let cy = y + gradient.center.1 * h;
    let radius = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        .into_iter()
        .map(|(px, py)| svg_distance(cx, cy, px, py))
        .fold(0.0f32, f32::max)
        .max(1.0);
    PdfShading {
        kind: PdfShadingKind::Radial([cx, cy, 0.0, cx, cy, radius]),
        stops: gradient.stops.clone(),
        extend_start: true,
        extend_end: true,
    }
}

fn pdf_image_alt_text(image: &ImageLine) -> String {
    let alt = image.alt.trim();
    if !alt.is_empty() {
        return image.alt.clone();
    }
    image
        .image
        .vector
        .as_ref()
        .and_then(|svg| svg.accessible_text.clone())
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn draw_svg_image(
    body: &mut String,
    annots: &mut Vec<LinkAnnotation>,
    image: &ImageLine,
    x: f32,
    y: f32,
    image_index: &BTreeMap<&str, usize>,
    page_shadings: &mut Vec<PdfShading>,
    subsets: &[EmbeddedFace],
    subset_lookup: &EmbeddedFaceLookup,
    faces: &Faces,
    shaped_cache: &ShapedRunCache,
) {
    let Some(svg) = image.image.vector.as_ref() else {
        return;
    };
    let Some(transform) = svg_image_transform(svg, image, x, y) else {
        return;
    };

    if let Some(background) = svg.root_background.as_ref() {
        append_svg_root_background(body, background, image, x, y, page_shadings);
    }

    let mut in_shape_group = false;
    let resources = SvgPaintResources {
        gradients: &svg.gradients,
        patterns: &svg.patterns,
        clip_paths: &svg.clip_paths,
        markers: &svg.markers,
        image_index,
        stroke_scale: svg_uniform_stroke_scale(transform),
    };
    for element in &svg.elements {
        if let SvgElement::Text(text) = element {
            if in_shape_group {
                body.push_str("Q\n");
                in_shape_group = false;
            }
            draw_svg_text(
                body,
                text,
                transform,
                &svg.clip_paths,
                subsets,
                subset_lookup,
                faces,
                shaped_cache,
            );
            push_svg_link_annotation(annots, element, transform);
            continue;
        }
        if !in_shape_group {
            append_svg_image_transform_prefix(body, transform);
            in_shape_group = true;
        }
        draw_svg_shape(body, element, resources, page_shadings);
        push_svg_link_annotation(annots, element, transform);
    }
    if in_shape_group {
        body.push_str("Q\n");
    }
}

fn push_svg_link_annotation(
    annots: &mut Vec<LinkAnnotation>,
    element: &SvgElement,
    image_transform: SvgImageTransform,
) {
    let Some(target) = svg_element_link(element) else {
        return;
    };
    if !svg_element_visible_for_link(element) {
        return;
    }
    let Some((bbox, transform)) = svg_element_link_bbox(element) else {
        return;
    };
    let Some(rect) = svg_bbox_pdf_rect(bbox, transform, image_transform) else {
        return;
    };
    annots.push(LinkAnnotation {
        rect,
        target: target.clone(),
        owner_mcid: None,
    });
}

fn svg_element_link(element: &SvgElement) -> Option<&LinkTarget> {
    match element {
        SvgElement::Rect(rect) => rect.link.as_ref(),
        SvgElement::Ellipse(ellipse) => ellipse.link.as_ref(),
        SvgElement::Line(line) => line.link.as_ref(),
        SvgElement::Polyline(poly) | SvgElement::Polygon(poly) => poly.link.as_ref(),
        SvgElement::Path(path) => path.link.as_ref(),
        SvgElement::Image(image) => image.link.as_ref(),
        SvgElement::Text(text) => text.link.as_ref(),
    }
}

fn svg_element_visible_for_link(element: &SvgElement) -> bool {
    match element {
        SvgElement::Rect(rect) => svg_style_has_link_paint(rect.style),
        SvgElement::Ellipse(ellipse) => svg_style_has_link_paint(ellipse.style),
        SvgElement::Line(line) => svg_style_has_link_paint(line.style),
        SvgElement::Polyline(poly) | SvgElement::Polygon(poly) => {
            svg_style_has_link_paint(poly.style)
        }
        SvgElement::Path(path) => svg_style_has_link_paint(path.style),
        SvgElement::Image(image) => {
            image.style.visible && image.style.opacity > 0.001 && image.w > 0.001 && image.h > 0.001
        }
        SvgElement::Text(text) => text.opacity > 0.001 && !text.text.is_empty(),
    }
}

fn svg_style_has_link_paint(style: SvgStyle) -> bool {
    style.visible
        && ((style.fill.is_some() && svg_effective_fill_opacity(style) > 0.001)
            || (style.stroke.is_some()
                && style.stroke_width > 0.001
                && svg_effective_stroke_opacity(style) > 0.001))
}

fn svg_element_link_bbox(element: &SvgElement) -> Option<((f32, f32, f32, f32), SvgTransform)> {
    match element {
        SvgElement::Rect(rect) => {
            let bbox = (rect.x, rect.y, rect.w, rect.h);
            Some((
                svg_expand_bbox_for_stroke(bbox, rect.style),
                rect.style.transform,
            ))
        }
        SvgElement::Ellipse(ellipse) => {
            let bbox = (
                ellipse.cx - ellipse.rx,
                ellipse.cy - ellipse.ry,
                ellipse.rx * 2.0,
                ellipse.ry * 2.0,
            );
            Some((
                svg_expand_bbox_for_stroke(bbox, ellipse.style),
                ellipse.style.transform,
            ))
        }
        SvgElement::Line(line) => {
            let min_x = line.x1.min(line.x2);
            let min_y = line.y1.min(line.y2);
            let max_x = line.x1.max(line.x2);
            let max_y = line.y1.max(line.y2);
            let pad = (line.style.stroke_width * 0.5).max(0.75);
            Some((
                (
                    min_x - pad,
                    min_y - pad,
                    max_x - min_x + pad * 2.0,
                    max_y - min_y + pad * 2.0,
                ),
                line.style.transform,
            ))
        }
        SvgElement::Polyline(poly) | SvgElement::Polygon(poly) => svg_points_bbox(&poly.points)
            .map(|bbox| {
                (
                    svg_expand_bbox_for_stroke(bbox, poly.style),
                    poly.style.transform,
                )
            }),
        SvgElement::Path(path) => svg_path_bbox(&path.ops).map(|bbox| {
            (
                svg_expand_bbox_for_stroke(bbox, path.style),
                path.style.transform,
            )
        }),
        SvgElement::Image(image) => {
            Some(((image.x, image.y, image.w, image.h), image.style.transform))
        }
        SvgElement::Text(text) => svg_text_link_bbox(text).map(|bbox| (bbox, text.transform)),
    }
}

fn svg_expand_bbox_for_stroke(bbox: (f32, f32, f32, f32), style: SvgStyle) -> (f32, f32, f32, f32) {
    let pad = if style.stroke.is_some() && style.stroke_width.is_finite() {
        (style.stroke_width * 0.5).max(0.0)
    } else {
        0.0
    };
    (
        bbox.0 - pad,
        bbox.1 - pad,
        bbox.2 + pad * 2.0,
        bbox.3 + pad * 2.0,
    )
}

fn svg_text_link_bbox(text: &SvgText) -> Option<(f32, f32, f32, f32)> {
    let width = text
        .text_length
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or_else(|| svg_text_advance(&text.text, text.font_size, text.letter_spacing));
    if !width.is_finite() || width <= 0.001 || !text.font_size.is_finite() {
        return None;
    }
    let x = match text.anchor {
        SvgTextAnchor::Start => text.x,
        SvgTextAnchor::Middle => text.x - width * 0.5,
        SvgTextAnchor::End => text.x - width,
    };
    let baseline_y = text.y + text.baseline.y_shift_em() * text.font_size;
    let y = baseline_y - text.font_size * 0.9;
    let h = text.font_size * 1.15;
    Some((x, y, width, h))
}

fn svg_bbox_pdf_rect(
    bbox: (f32, f32, f32, f32),
    transform: SvgTransform,
    image_transform: SvgImageTransform,
) -> Option<Rect> {
    let (x, y, w, h) = bbox;
    if ![x, y, w, h].iter().all(|value| value.is_finite()) || w <= 0.001 || h <= 0.001 {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for (cx, cy) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
        let (sx, sy) = transform.apply_point(cx, cy);
        let (px, py) = image_transform.map_point(sx, sy);
        if !px.is_finite() || !py.is_finite() {
            return None;
        }
        min_x = min_x.min(px);
        max_x = max_x.max(px);
        min_y = min_y.min(py);
        max_y = max_y.max(py);
    }
    let mut rect = Rect {
        x0: min_x,
        y0: min_y,
        x1: max_x,
        y1: max_y,
    };
    if let Some((cx, cy, cw, ch)) = image_transform.viewport_clip {
        rect.x0 = rect.x0.max(cx);
        rect.y0 = rect.y0.max(cy);
        rect.x1 = rect.x1.min(cx + cw);
        rect.y1 = rect.y1.min(cy + ch);
    }
    (rect.x1 - rect.x0 > 0.5 && rect.y1 - rect.y0 > 0.5).then_some(rect)
}

fn draw_svg_shape(
    body: &mut String,
    element: &SvgElement,
    resources: SvgPaintResources<'_>,
    page_shadings: &mut Vec<PdfShading>,
) {
    match element {
        SvgElement::Rect(rect) => draw_svg_rect(
            body,
            rect,
            resources.gradients,
            resources.patterns,
            resources.clip_paths,
            resources.stroke_scale,
            page_shadings,
        ),
        SvgElement::Ellipse(ellipse) => draw_svg_ellipse(
            body,
            ellipse,
            resources.gradients,
            resources.patterns,
            resources.clip_paths,
            resources.stroke_scale,
            page_shadings,
        ),
        SvgElement::Line(line) => draw_svg_line(
            body,
            line,
            resources.gradients,
            resources.clip_paths,
            resources.markers,
            resources.stroke_scale,
            page_shadings,
        ),
        SvgElement::Polyline(poly) => draw_svg_poly(
            body,
            poly,
            false,
            resources.gradients,
            resources.patterns,
            resources.clip_paths,
            resources.markers,
            resources.stroke_scale,
            page_shadings,
        ),
        SvgElement::Polygon(poly) => draw_svg_poly(
            body,
            poly,
            true,
            resources.gradients,
            resources.patterns,
            resources.clip_paths,
            resources.markers,
            resources.stroke_scale,
            page_shadings,
        ),
        SvgElement::Path(path) => draw_svg_path(
            body,
            path,
            resources.gradients,
            resources.patterns,
            resources.clip_paths,
            resources.markers,
            resources.stroke_scale,
            page_shadings,
        ),
        SvgElement::Image(image) => {
            draw_svg_embedded_image(body, image, resources.clip_paths, resources.image_index)
        }
        SvgElement::Text(_) => {}
    }
}

fn draw_svg_rect(
    body: &mut String,
    rect: &SvgRect,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    stroke_scale: Option<f32>,
    page_shadings: &mut Vec<PdfShading>,
) {
    let style = svg_style_with_non_scaling_stroke(rect.style, stroke_scale);
    if !svg_style_has_paint(style) {
        return;
    }
    let transformed = append_svg_element_state_prefix(
        body,
        style,
        clip_paths,
        Some((rect.x, rect.y, rect.w, rect.h)),
    );
    if append_svg_shadow_prefix(body, style) {
        append_svg_rect_path(body, rect);
        append_svg_shadow_operator(body, style);
    }
    append_svg_painted_shape(
        body,
        style,
        gradients,
        patterns,
        clip_paths,
        page_shadings,
        stroke_scale,
        Some((rect.x, rect.y, rect.w, rect.h)),
        |body| append_svg_rect_path(body, rect),
    );
    append_svg_transform_suffix(body, transformed);
}

fn append_svg_rect_path(body: &mut String, rect: &SvgRect) {
    let rx = rect.rx.min(rect.w * 0.5).max(0.0);
    let ry = rect.ry.min(rect.h * 0.5).max(0.0);
    if rx <= 0.0 || ry <= 0.0 {
        body.push_str(&format!(
            "{x} {y} {w} {h} re ",
            x = pdf_num(rect.x),
            y = pdf_num(rect.y),
            w = pdf_num(rect.w),
            h = pdf_num(rect.h),
        ));
    } else {
        append_svg_rounded_rect_path(body, rect.x, rect.y, rect.w, rect.h, rx, ry);
    }
}

fn draw_svg_ellipse(
    body: &mut String,
    ellipse: &SvgEllipse,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    stroke_scale: Option<f32>,
    page_shadings: &mut Vec<PdfShading>,
) {
    let style = svg_style_with_non_scaling_stroke(ellipse.style, stroke_scale);
    if !svg_style_has_paint(style) {
        return;
    }
    let transformed = append_svg_element_state_prefix(
        body,
        style,
        clip_paths,
        Some((
            ellipse.cx - ellipse.rx,
            ellipse.cy - ellipse.ry,
            ellipse.rx * 2.0,
            ellipse.ry * 2.0,
        )),
    );
    if append_svg_shadow_prefix(body, style) {
        append_svg_ellipse_path(body, ellipse.cx, ellipse.cy, ellipse.rx, ellipse.ry);
        append_svg_shadow_operator(body, style);
    }
    append_svg_painted_shape(
        body,
        style,
        gradients,
        patterns,
        clip_paths,
        page_shadings,
        stroke_scale,
        Some((
            ellipse.cx - ellipse.rx,
            ellipse.cy - ellipse.ry,
            ellipse.rx * 2.0,
            ellipse.ry * 2.0,
        )),
        |body| append_svg_ellipse_path(body, ellipse.cx, ellipse.cy, ellipse.rx, ellipse.ry),
    );
    append_svg_transform_suffix(body, transformed);
}

fn draw_svg_line(
    body: &mut String,
    line: &SvgLine,
    gradients: &[SvgGradientPaint],
    clip_paths: &[SvgClipPath],
    markers: &[SvgMarker],
    stroke_scale: Option<f32>,
    page_shadings: &mut Vec<PdfShading>,
) {
    if !line.style.visible {
        return;
    }
    let has_markers = line.marker_start.is_some() || line.marker_end.is_some();
    let style = SvgStyle {
        color: line.style.color,
        fill: None,
        fill_gradient: None,
        fill_pattern: None,
        fill_current_color: false,
        fill_context: None,
        stroke: line.style.stroke,
        stroke_gradient: line.style.stroke_gradient,
        stroke_current_color: line.style.stroke_current_color,
        stroke_context: line.style.stroke_context,
        stroke_width: line.style.stroke_width,
        non_scaling_stroke: line.style.non_scaling_stroke,
        opacity: line.style.opacity,
        fill_opacity: line.style.fill_opacity,
        stroke_opacity: line.style.stroke_opacity,
        display_visible: line.style.display_visible,
        visibility_visible: line.style.visibility_visible,
        visible: line.style.visible,
        shadow: line.style.shadow,
        clip_path: line.style.clip_path,
        mask_path: line.style.mask_path,
        transform: line.style.transform,
        dash: line.style.dash,
        line_cap: line.style.line_cap,
        line_join: line.style.line_join,
        miter_limit: line.style.miter_limit,
        fill_rule: line.style.fill_rule,
        paint_order: line.style.paint_order,
        font_size: line.style.font_size,
        text_anchor: line.style.text_anchor,
        font_weight: line.style.font_weight,
        font_slant: line.style.font_slant,
        font_family: line.style.font_family,
        dominant_baseline: line.style.dominant_baseline,
        letter_spacing: line.style.letter_spacing,
        text_decoration: line.style.text_decoration,
    };
    let style = svg_style_with_non_scaling_stroke(style, stroke_scale);
    if !svg_style_has_paint(style) && !has_markers {
        return;
    }
    let line_bbox = svg_line_bbox(line, style);
    let transformed = append_svg_element_state_prefix(body, style, clip_paths, line_bbox);
    if append_svg_shadow_prefix(body, style) {
        append_svg_line_path(body, line);
        append_svg_shadow_operator(body, style);
    }
    if style.paint_order != SvgPaintOrder::NORMAL && has_markers {
        for layer in style.paint_order.layers {
            match layer {
                SvgPaintLayer::Fill => {}
                SvgPaintLayer::Stroke => {
                    if style.stroke.is_some() {
                        append_svg_line_stroke_layer(body, line, style, gradients, page_shadings);
                    }
                }
                SvgPaintLayer::Markers => {
                    append_svg_line_markers(
                        body,
                        line,
                        markers,
                        style.stroke,
                        line.style.fill,
                        style.stroke_width,
                    );
                }
            }
        }
    } else {
        if style.stroke.is_some() {
            append_svg_line_stroke_layer(body, line, style, gradients, page_shadings);
        }
        if has_markers {
            append_svg_line_markers(
                body,
                line,
                markers,
                style.stroke,
                line.style.fill,
                style.stroke_width,
            );
        }
    }
    append_svg_transform_suffix(body, transformed);
}

fn append_svg_line_stroke_layer(
    body: &mut String,
    line: &SvgLine,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    page_shadings: &mut Vec<PdfShading>,
) -> bool {
    if !append_svg_line_gradient_stroke(body, line, style, gradients, page_shadings) {
        append_svg_style(body, style);
        append_svg_line_path(body, line);
        body.push_str("S\n");
    }
    true
}

fn append_svg_line_gradient_stroke(
    body: &mut String,
    line: &SvgLine,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    page_shadings: &mut Vec<PdfShading>,
) -> bool {
    if style.dash.len > 0 || matches!(style.line_cap, SvgLineCap::Round) {
        return false;
    }
    let width = style.stroke_width;
    let dx = line.x2 - line.x1;
    let dy = line.y2 - line.y1;
    let len = (dx.mul_add(dx, dy * dy)).sqrt();
    if ![line.x1, line.y1, line.x2, line.y2, width, len]
        .iter()
        .all(|value| value.is_finite())
        || width <= 0.001
        || len <= 0.001
    {
        return false;
    }

    let half = width * 0.5;
    let ux = dx / len;
    let uy = dy / len;
    let cap = match style.line_cap {
        SvgLineCap::Butt => 0.0,
        SvgLineCap::Square => half,
        SvgLineCap::Round => return false,
    };
    let sx = line.x1 - ux * cap;
    let sy = line.y1 - uy * cap;
    let ex = line.x2 + ux * cap;
    let ey = line.y2 + uy * cap;
    let nx = -uy * half;
    let ny = ux * half;
    let points = [
        (sx + nx, sy + ny),
        (ex + nx, ey + ny),
        (ex - nx, ey - ny),
        (sx - nx, sy - ny),
    ];
    if !points.iter().all(|(x, y)| x.is_finite() && y.is_finite()) {
        return false;
    }
    let min_x = points.iter().map(|(x, _)| *x).fold(f32::INFINITY, f32::min);
    let max_x = points
        .iter()
        .map(|(x, _)| *x)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|(_, y)| *y)
        .fold(f32::NEG_INFINITY, f32::max);
    let Some(shadings) = svg_stroke_shadings(
        style,
        gradients,
        Some((min_x, min_y, max_x - min_x, max_y - min_y)),
    ) else {
        return false;
    };
    if !pdf_shadings_fit(page_shadings, &shadings) {
        return false;
    }
    let names = shadings
        .into_iter()
        .filter_map(|shading| register_pdf_shading(page_shadings, shading))
        .collect::<Vec<_>>();
    if names.is_empty() {
        return false;
    }

    body.push_str("q ");
    let stroke_alpha = quantize_svg_alpha(svg_effective_stroke_opacity(style));
    if stroke_alpha < 1000 {
        append_svg_alpha_state(body, stroke_alpha, stroke_alpha);
        body.push(' ');
    }
    append_svg_line_stroke_outline(body, points);
    body.push_str("W n ");
    for name in names {
        body.push_str(&format!("/{name} sh\n"));
    }
    body.push_str("Q\n");
    true
}

fn append_svg_line_stroke_outline(body: &mut String, points: [(f32, f32); 4]) {
    body.push_str(&format!(
        "{x0} {y0} m {x1} {y1} l {x2} {y2} l {x3} {y3} l h ",
        x0 = pdf_num(points[0].0),
        y0 = pdf_num(points[0].1),
        x1 = pdf_num(points[1].0),
        y1 = pdf_num(points[1].1),
        x2 = pdf_num(points[2].0),
        y2 = pdf_num(points[2].1),
        x3 = pdf_num(points[3].0),
        y3 = pdf_num(points[3].1),
    ));
}

fn append_svg_line_path(body: &mut String, line: &SvgLine) {
    body.push_str(&format!(
        "{x1} {y1} m {x2} {y2} l ",
        x1 = pdf_num(line.x1),
        y1 = pdf_num(line.y1),
        x2 = pdf_num(line.x2),
        y2 = pdf_num(line.y2),
    ));
}

fn svg_line_bbox(line: &SvgLine, style: SvgStyle) -> Option<(f32, f32, f32, f32)> {
    if ![line.x1, line.y1, line.x2, line.y2]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    let min_x = line.x1.min(line.x2);
    let min_y = line.y1.min(line.y2);
    let max_x = line.x1.max(line.x2);
    let max_y = line.y1.max(line.y2);
    let pad = if style.stroke.is_some() && style.stroke_width.is_finite() {
        (style.stroke_width * 0.5).max(0.0)
    } else {
        0.0
    };
    let x = min_x - pad;
    let y = min_y - pad;
    let w = max_x - min_x + pad * 2.0;
    let h = max_y - min_y + pad * 2.0;
    (w > 0.001 && h > 0.001).then_some((x, y, w, h))
}

fn append_svg_line_markers(
    body: &mut String,
    line: &SvgLine,
    markers: &[SvgMarker],
    stroke: Option<(f32, f32, f32)>,
    fill: Option<(f32, f32, f32)>,
    stroke_width: f32,
) {
    let paint = SvgMarkerPaint {
        fill,
        stroke,
        stroke_width,
    };
    if let Some(marker_ref) = line.marker_start {
        append_svg_marker_or_arrowhead(
            body,
            marker_ref,
            markers,
            SvgMarkerPlacement::Start,
            (line.x1, line.y1),
            (line.x2, line.y2),
            paint,
        );
    }
    if let Some(marker_ref) = line.marker_end {
        append_svg_marker_or_arrowhead(
            body,
            marker_ref,
            markers,
            SvgMarkerPlacement::End,
            (line.x2, line.y2),
            (line.x1, line.y1),
            paint,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_svg_poly(
    body: &mut String,
    poly: &SvgPoly,
    closed: bool,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    markers: &[SvgMarker],
    stroke_scale: Option<f32>,
    page_shadings: &mut Vec<PdfShading>,
) {
    let style = svg_style_with_non_scaling_stroke(poly.style, stroke_scale);
    let has_markers =
        poly.marker_start.is_some() || poly.marker_mid.is_some() || poly.marker_end.is_some();
    if poly.points.is_empty() || (!svg_style_has_paint(style) && !has_markers) {
        return;
    }
    let poly_bbox = svg_points_bbox(&poly.points);
    let transformed = append_svg_element_state_prefix(body, style, clip_paths, poly_bbox);
    if append_svg_shadow_prefix(body, style) {
        append_svg_poly_path(body, poly, closed);
        append_svg_shadow_operator(body, style);
    }
    if style.paint_order != SvgPaintOrder::NORMAL && has_markers {
        append_svg_ordered_painted_shape_with_markers(
            body,
            style,
            gradients,
            patterns,
            clip_paths,
            page_shadings,
            stroke_scale,
            poly_bbox,
            &|body| append_svg_poly_path(body, poly, closed),
            |body| {
                append_svg_poly_markers(
                    body,
                    poly,
                    markers,
                    style.stroke,
                    style.fill,
                    style.stroke_width,
                );
            },
        );
    } else {
        if svg_style_has_paint(style) {
            append_svg_painted_shape(
                body,
                style,
                gradients,
                patterns,
                clip_paths,
                page_shadings,
                stroke_scale,
                poly_bbox,
                |body| append_svg_poly_path(body, poly, closed),
            );
        }
        if has_markers {
            append_svg_poly_markers(
                body,
                poly,
                markers,
                style.stroke,
                style.fill,
                style.stroke_width,
            );
        }
    }
    append_svg_transform_suffix(body, transformed);
}

fn append_svg_poly_path(body: &mut String, poly: &SvgPoly, closed: bool) {
    let (x0, y0) = poly.points[0];
    body.push_str(&format!("{} {} m ", pdf_num(x0), pdf_num(y0)));
    for &(x, y) in &poly.points[1..] {
        body.push_str(&format!("{} {} l ", pdf_num(x), pdf_num(y)));
    }
    if closed {
        body.push_str("h ");
    }
}

fn append_svg_poly_markers(
    body: &mut String,
    poly: &SvgPoly,
    markers: &[SvgMarker],
    stroke: Option<(f32, f32, f32)>,
    fill: Option<(f32, f32, f32)>,
    stroke_width: f32,
) {
    if poly.points.len() < 2 {
        return;
    }
    let paint = SvgMarkerPaint {
        fill,
        stroke,
        stroke_width,
    };
    if let Some(marker_ref) = poly.marker_start {
        append_svg_marker_or_arrowhead(
            body,
            marker_ref,
            markers,
            SvgMarkerPlacement::Start,
            poly.points[0],
            poly.points[1],
            paint,
        );
    }
    if let Some(marker_ref) = poly.marker_mid {
        for window in poly.points.windows(3) {
            let Some(tail) = svg_marker_mid_tail(window[0], window[1], window[2]) else {
                continue;
            };
            append_svg_marker_or_arrowhead(
                body,
                marker_ref,
                markers,
                SvgMarkerPlacement::Mid,
                window[1],
                tail,
                paint,
            );
        }
    }
    if let Some(marker_ref) = poly.marker_end {
        let end_index = poly.points.len() - 1;
        append_svg_marker_or_arrowhead(
            body,
            marker_ref,
            markers,
            SvgMarkerPlacement::End,
            poly.points[end_index],
            poly.points[end_index - 1],
            paint,
        );
    }
}

fn svg_marker_mid_tail(
    incoming_tail: (f32, f32),
    curr: (f32, f32),
    outgoing_head: (f32, f32),
) -> Option<(f32, f32)> {
    let incoming = svg_unit_vector(incoming_tail, curr)?;
    let outgoing = svg_unit_vector(curr, outgoing_head)?;
    let mut dx = incoming.0 + outgoing.0;
    let mut dy = incoming.1 + outgoing.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len.is_finite() && len > 0.001 {
        dx /= len;
        dy /= len;
    } else {
        dx = outgoing.0;
        dy = outgoing.1;
    }
    Some((curr.0 - dx, curr.1 - dy))
}

#[derive(Clone, Copy)]
struct SvgPathSegmentTangent {
    start: (f32, f32),
    end: (f32, f32),
    outgoing_head: (f32, f32),
    incoming_tail: (f32, f32),
}

fn svg_unit_vector(from: (f32, f32), to: (f32, f32)) -> Option<(f32, f32)> {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt();
    if ![dx, dy, len].iter().all(|value| value.is_finite()) || len <= 0.001 {
        return None;
    }
    Some((dx / len, dy / len))
}

#[allow(clippy::too_many_arguments)]
fn draw_svg_path(
    body: &mut String,
    path: &SvgPath,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    markers: &[SvgMarker],
    stroke_scale: Option<f32>,
    page_shadings: &mut Vec<PdfShading>,
) {
    let style = svg_style_with_non_scaling_stroke(path.style, stroke_scale);
    let has_markers =
        path.marker_start.is_some() || path.marker_mid.is_some() || path.marker_end.is_some();
    if path.ops.is_empty() || (!svg_style_has_paint(style) && !has_markers) {
        return;
    }
    let path_bbox = svg_path_bbox(&path.ops);
    let transformed = append_svg_element_state_prefix(body, style, clip_paths, path_bbox);
    if append_svg_shadow_prefix(body, style) {
        append_svg_path_ops(body, &path.ops);
        append_svg_shadow_operator(body, style);
    }
    if style.paint_order != SvgPaintOrder::NORMAL && has_markers {
        append_svg_ordered_painted_shape_with_markers(
            body,
            style,
            gradients,
            patterns,
            clip_paths,
            page_shadings,
            stroke_scale,
            path_bbox,
            &|body| append_svg_path_ops(body, &path.ops),
            |body| {
                append_svg_path_markers(
                    body,
                    path,
                    markers,
                    style.stroke,
                    style.fill,
                    style.stroke_width,
                );
            },
        );
    } else {
        if svg_style_has_paint(style) {
            append_svg_painted_shape(
                body,
                style,
                gradients,
                patterns,
                clip_paths,
                page_shadings,
                stroke_scale,
                path_bbox,
                |body| append_svg_path_ops(body, &path.ops),
            );
        }
        if has_markers {
            append_svg_path_markers(
                body,
                path,
                markers,
                style.stroke,
                style.fill,
                style.stroke_width,
            );
        }
    }
    append_svg_transform_suffix(body, transformed);
}

fn append_svg_path_markers(
    body: &mut String,
    path: &SvgPath,
    markers: &[SvgMarker],
    stroke: Option<(f32, f32, f32)>,
    fill: Option<(f32, f32, f32)>,
    stroke_width: f32,
) {
    let marker_paint = SvgMarkerPaint {
        fill,
        stroke,
        stroke_width,
    };
    if let Some(marker_ref) = path.marker_end
        && let Some((tip, tail)) = svg_path_end_tangent(&path.ops)
    {
        append_svg_marker_or_arrowhead(
            body,
            marker_ref,
            markers,
            SvgMarkerPlacement::End,
            tip,
            tail,
            marker_paint,
        );
    }
    if let Some(marker_ref) = path.marker_mid {
        append_svg_path_mid_markers(
            body,
            &path.ops,
            marker_ref,
            markers,
            stroke,
            fill,
            stroke_width,
        );
    }
    if let Some(marker_ref) = path.marker_start
        && let Some((tip, tail)) = svg_path_start_tangent(&path.ops)
    {
        append_svg_marker_or_arrowhead(
            body,
            marker_ref,
            markers,
            SvgMarkerPlacement::Start,
            tip,
            tail,
            marker_paint,
        );
    }
}

fn append_svg_path_mid_markers(
    body: &mut String,
    ops: &[SvgPathOp],
    marker_ref: SvgMarkerRef,
    markers: &[SvgMarker],
    stroke: Option<(f32, f32, f32)>,
    fill: Option<(f32, f32, f32)>,
    stroke_width: f32,
) {
    let paint = SvgMarkerPaint {
        fill,
        stroke,
        stroke_width,
    };
    let mut current = (0.0f32, 0.0f32);
    let mut subpath_start = None;
    let mut prev_segment: Option<SvgPathSegmentTangent> = None;
    for op in ops {
        let segment = match *op {
            SvgPathOp::Move(x, y) => {
                current = (x, y);
                subpath_start = Some(current);
                prev_segment = None;
                continue;
            }
            SvgPathOp::Line(x, y) => {
                let segment = SvgPathSegmentTangent {
                    start: current,
                    end: (x, y),
                    outgoing_head: (x, y),
                    incoming_tail: current,
                };
                current = segment.end;
                segment
            }
            SvgPathOp::Cubic(x1, y1, x2, y2, x, y) => {
                let segment = SvgPathSegmentTangent {
                    start: current,
                    end: (x, y),
                    outgoing_head: (x1, y1),
                    incoming_tail: (x2, y2),
                };
                current = segment.end;
                segment
            }
            SvgPathOp::Quad(x1, y1, x, y) => {
                let segment = SvgPathSegmentTangent {
                    start: current,
                    end: (x, y),
                    outgoing_head: (x1, y1),
                    incoming_tail: (x1, y1),
                };
                current = segment.end;
                segment
            }
            SvgPathOp::Close => {
                let Some(start) = subpath_start else {
                    continue;
                };
                let segment = SvgPathSegmentTangent {
                    start: current,
                    end: start,
                    outgoing_head: start,
                    incoming_tail: current,
                };
                current = start;
                segment
            }
        };
        if let Some(prev) = prev_segment
            && svg_points_coincident(prev.end, segment.start)
            && let Some(tail) =
                svg_marker_mid_tail(prev.incoming_tail, segment.start, segment.outgoing_head)
        {
            append_svg_marker_or_arrowhead(
                body,
                marker_ref,
                markers,
                SvgMarkerPlacement::Mid,
                segment.start,
                tail,
                paint,
            );
        }
        prev_segment = Some(segment);
    }
}

fn svg_points_coincident(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() <= 0.001 && (a.1 - b.1).abs() <= 0.001
}

fn draw_svg_embedded_image(
    body: &mut String,
    image: &SvgEmbeddedImage,
    clip_paths: &[SvgClipPath],
    image_index: &BTreeMap<&str, usize>,
) {
    if !image.style.visible || image.w <= 0.0 || image.h <= 0.0 {
        return;
    }
    let Some(idx) = image_index.get(image.image.key.as_str()) else {
        return;
    };
    body.push_str("q ");
    let alpha = quantize_svg_alpha(image.style.opacity);
    if alpha < 1000 {
        append_svg_alpha_state(body, alpha, alpha);
        body.push(' ');
    }
    if !image.style.transform.is_identity() {
        body.push_str(&format!(
            "{a} {b} {c} {d} {e} {f} cm ",
            a = pdf_num(image.style.transform.a),
            b = pdf_num(image.style.transform.b),
            c = pdf_num(image.style.transform.c),
            d = pdf_num(image.style.transform.d),
            e = pdf_num(image.style.transform.e),
            f = pdf_num(image.style.transform.f),
        ));
    }
    if let Some(clip_path) = image
        .style
        .clip_path
        .and_then(|index| clip_paths.get(index))
        .filter(|clip_path| {
            svg_clip_path_applicable(clip_path, Some((image.x, image.y, image.w, image.h)))
        })
    {
        append_svg_clip_path(body, clip_path, Some((image.x, image.y, image.w, image.h)));
    }
    if let Some(mask_path) = image
        .style
        .mask_path
        .and_then(|index| clip_paths.get(index))
        .filter(|mask_path| {
            svg_clip_path_applicable(mask_path, Some((image.x, image.y, image.w, image.h)))
        })
    {
        append_svg_clip_path(body, mask_path, Some((image.x, image.y, image.w, image.h)));
    }
    let Some((draw_x, draw_y, draw_w, draw_h, viewport_clip)) = svg_embedded_image_rect(image)
    else {
        body.push_str("Q\n");
        return;
    };
    if viewport_clip {
        body.push_str(&format!(
            "{x} {y} {w} {h} re W n ",
            x = pdf_num(image.x),
            y = pdf_num(image.y),
            w = pdf_num(image.w),
            h = pdf_num(image.h),
        ));
    }
    let name = image_name(*idx);
    body.push_str(&format!(
        "{w} 0 0 {neg_h} {x} {y} cm /{name} Do Q\n",
        w = pdf_num(draw_w),
        neg_h = pdf_num(-draw_h),
        x = pdf_num(draw_x),
        y = pdf_num(draw_y + draw_h),
    ));
}

fn svg_embedded_image_rect(image: &SvgEmbeddedImage) -> Option<(f32, f32, f32, f32, bool)> {
    let intrinsic_w = image.image.width_px as f32;
    let intrinsic_h = image.image.height_px as f32;
    if ![image.x, image.y, image.w, image.h, intrinsic_w, intrinsic_h]
        .iter()
        .all(|value| value.is_finite())
        || image.w <= 0.0
        || image.h <= 0.0
        || intrinsic_w <= 0.0
        || intrinsic_h <= 0.0
    {
        return None;
    }

    match image.preserve_aspect.mode {
        SvgAspectScaleMode::None => Some((image.x, image.y, image.w, image.h, false)),
        SvgAspectScaleMode::Meet | SvgAspectScaleMode::Slice => {
            let sx = image.w / intrinsic_w;
            let sy = image.h / intrinsic_h;
            let scale = if image.preserve_aspect.mode == SvgAspectScaleMode::Slice {
                sx.max(sy)
            } else {
                sx.min(sy)
            };
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            let draw_w = intrinsic_w * scale;
            let draw_h = intrinsic_h * scale;
            let draw_x = image.x + (image.w - draw_w) * image.preserve_aspect.align_x;
            let draw_y = image.y + (image.h - draw_h) * image.preserve_aspect.align_y;
            [draw_x, draw_y, draw_w, draw_h]
                .iter()
                .all(|value| value.is_finite())
                .then_some((
                    draw_x,
                    draw_y,
                    draw_w,
                    draw_h,
                    image.preserve_aspect.mode == SvgAspectScaleMode::Slice,
                ))
        }
    }
}

fn append_svg_element_state_prefix(
    body: &mut String,
    style: SvgStyle,
    clip_paths: &[SvgClipPath],
    element_bbox: Option<(f32, f32, f32, f32)>,
) -> bool {
    let clip_path = style
        .clip_path
        .and_then(|index| clip_paths.get(index))
        .filter(|clip_path| svg_clip_path_applicable(clip_path, element_bbox));
    let mask_path = style
        .mask_path
        .and_then(|index| clip_paths.get(index))
        .filter(|mask_path| svg_clip_path_applicable(mask_path, element_bbox));
    let alpha = svg_style_alpha_values(style);
    if style.transform.is_identity()
        && style.dash.is_empty()
        && clip_path.is_none()
        && mask_path.is_none()
        && alpha.is_none()
    {
        return false;
    }
    body.push_str("q ");
    if let Some((fill_alpha, stroke_alpha)) = alpha {
        append_svg_alpha_state(body, fill_alpha, stroke_alpha);
        body.push(' ');
    }
    if !style.transform.is_identity() {
        body.push_str(&format!(
            "{a} {b} {c} {d} {e} {f} cm ",
            a = pdf_num(style.transform.a),
            b = pdf_num(style.transform.b),
            c = pdf_num(style.transform.c),
            d = pdf_num(style.transform.d),
            e = pdf_num(style.transform.e),
            f = pdf_num(style.transform.f),
        ));
    }
    if let Some(clip_path) = clip_path {
        append_svg_clip_path(body, clip_path, element_bbox);
    }
    if let Some(mask_path) = mask_path {
        append_svg_clip_path(body, mask_path, element_bbox);
    }
    true
}

fn svg_clip_path_applicable(
    clip_path: &SvgClipPath,
    element_bbox: Option<(f32, f32, f32, f32)>,
) -> bool {
    !clip_path.ops.is_empty()
        && match clip_path.units {
            SvgClipPathUnits::UserSpaceOnUse => true,
            SvgClipPathUnits::ObjectBoundingBox => {
                svg_object_bbox_transform(element_bbox).is_some()
            }
        }
}

fn append_svg_clip_path(
    body: &mut String,
    clip_path: &SvgClipPath,
    element_bbox: Option<(f32, f32, f32, f32)>,
) -> bool {
    if !svg_clip_path_applicable(clip_path, element_bbox) {
        return false;
    }
    match clip_path.units {
        SvgClipPathUnits::UserSpaceOnUse => append_svg_path_ops(body, &clip_path.ops),
        SvgClipPathUnits::ObjectBoundingBox => {
            let Some(transform) = svg_object_bbox_transform(element_bbox) else {
                return false;
            };
            let mut ops = clip_path.ops.clone();
            transform_svg_path_ops(&mut ops, transform);
            append_svg_path_ops(body, &ops);
        }
    }
    match clip_path.fill_rule {
        SvgFillRule::NonZero => body.push_str("W n "),
        SvgFillRule::EvenOdd => body.push_str("W* n "),
    }
    true
}

fn svg_object_bbox_transform(bbox: Option<(f32, f32, f32, f32)>) -> Option<SvgTransform> {
    let (x, y, w, h) = bbox?;
    if ![x, y, w, h].iter().all(|value| value.is_finite()) || w <= 0.001 || h <= 0.001 {
        return None;
    }
    Some(SvgTransform::translate(x, y).concat(SvgTransform::scale(w, h)))
}

fn append_svg_transform_suffix(body: &mut String, transformed: bool) {
    if transformed {
        body.push_str("Q\n");
    }
}

fn append_svg_shadow_prefix(body: &mut String, style: SvgStyle) -> bool {
    let Some(shadow) = style.shadow else {
        return false;
    };
    let Some(uses_fill) = svg_shadow_uses_fill(style) else {
        return false;
    };
    body.push_str("q 1 0 0 1 ");
    append_pdf_num(body, shadow.dx);
    body.push(' ');
    append_pdf_num(body, shadow.dy);
    body.push_str(" cm ");
    let paint_alpha = if uses_fill {
        svg_effective_fill_opacity(style)
    } else {
        svg_effective_stroke_opacity(style)
    };
    let alpha = quantize_svg_alpha((shadow.opacity * paint_alpha).clamp(0.0, 1.0));
    if alpha < 1000 {
        append_svg_alpha_state(body, alpha, alpha);
        body.push(' ');
    }
    let (r, g, b) = shadow.color;
    if uses_fill {
        append_rgb_fill_space_operator(body, (r, g, b));
    } else {
        append_rgb_stroke_space_operator(body, (r, g, b));
        append_svg_stroke_options(body, style);
    }
    true
}

fn append_svg_shadow_operator(body: &mut String, style: SvgStyle) {
    if svg_shadow_uses_fill(style).unwrap_or(false) {
        match style.fill_rule {
            SvgFillRule::NonZero => body.push_str("f Q\n"),
            SvgFillRule::EvenOdd => body.push_str("f* Q\n"),
        }
    } else {
        body.push_str("S Q\n");
    }
}

fn svg_shadow_uses_fill(style: SvgStyle) -> Option<bool> {
    if !style.visible {
        return None;
    }
    if style.fill.is_some() && svg_effective_fill_opacity(style) > 0.001 {
        return Some(true);
    }
    if style.stroke.is_some() && svg_effective_stroke_opacity(style) > 0.001 {
        return Some(false);
    }
    None
}

fn append_svg_style(body: &mut String, style: SvgStyle) {
    if let Some(color) = style.fill {
        append_rgb_fill_space_operator(body, color);
    }
    if let Some(color) = style.stroke {
        append_rgb_stroke_space_operator(body, color);
        append_svg_stroke_options(body, style);
    }
}

fn append_svg_fill_style(body: &mut String, style: SvgStyle) {
    if let Some(color) = style.fill {
        append_rgb_fill_space_operator(body, color);
    }
}

fn append_svg_stroke_options(body: &mut String, style: SvgStyle) {
    append_pdf_num(body, style.stroke_width.max(0.1));
    body.push_str(" w ");
    body.push((b'0' + style.line_cap.pdf_id()) as char);
    body.push_str(" J ");
    body.push((b'0' + style.line_join.pdf_id()) as char);
    body.push_str(" j ");
    if style.line_join == SvgLineJoin::Miter
        && let Some(miter_limit) = style.miter_limit
    {
        append_pdf_num(body, miter_limit);
        body.push_str(" M ");
    }
    if !style.dash.is_empty() {
        body.push('[');
        let dash_len = style.dash.len as usize;
        let repeat_count = if dash_len % 2 == 1 { 2 } else { 1 };
        for repeat in 0..repeat_count {
            for idx in 0..dash_len {
                if repeat > 0 || idx > 0 {
                    body.push(' ');
                }
                append_pdf_num(body, style.dash.values[idx]);
            }
        }
        body.push_str("] ");
        append_pdf_num(body, style.dash.offset);
        body.push_str(" d ");
    }
}

fn append_svg_fill_operator(body: &mut String, style: SvgStyle) {
    match style.fill_rule {
        SvgFillRule::NonZero => body.push_str("f\n"),
        SvgFillRule::EvenOdd => body.push_str("f*\n"),
    }
}

fn append_svg_paint_operator(body: &mut String, style: SvgStyle) {
    match (style.fill, style.stroke) {
        (Some(_), Some(_)) => match style.fill_rule {
            SvgFillRule::NonZero => body.push_str("B\n"),
            SvgFillRule::EvenOdd => body.push_str("B*\n"),
        },
        (Some(_), None) => match style.fill_rule {
            SvgFillRule::NonZero => body.push_str("f\n"),
            SvgFillRule::EvenOdd => body.push_str("f*\n"),
        },
        (None, Some(_)) => body.push_str("S\n"),
        (None, None) => body.push_str("n\n"),
    }
}

#[allow(clippy::too_many_arguments)]
fn append_svg_painted_shape<F>(
    body: &mut String,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    page_shadings: &mut Vec<PdfShading>,
    stroke_scale: Option<f32>,
    bbox: Option<(f32, f32, f32, f32)>,
    append_path: F,
) where
    F: Fn(&mut String),
{
    if style.paint_order != SvgPaintOrder::NORMAL && style.fill.is_some() && style.stroke.is_some()
    {
        append_svg_ordered_painted_shape(
            body,
            style,
            gradients,
            patterns,
            clip_paths,
            page_shadings,
            stroke_scale,
            bbox,
            &append_path,
        );
        return;
    }

    if append_svg_pattern_fill(
        body,
        style,
        gradients,
        patterns,
        clip_paths,
        page_shadings,
        stroke_scale,
        bbox,
        &append_path,
    ) {
        if style.stroke.is_some() && svg_effective_stroke_opacity(style) > 0.001 {
            append_svg_stroke_style(body, style);
            append_path(body);
            body.push_str("S\n");
        }
        return;
    }

    if let Some(shadings) = svg_fill_shadings(style, gradients, bbox) {
        if pdf_shadings_fit(page_shadings, &shadings) {
            let names = shadings
                .into_iter()
                .filter_map(|shading| register_pdf_shading(page_shadings, shading))
                .collect::<Vec<_>>();
            body.push_str("q ");
            append_path(body);
            match style.fill_rule {
                SvgFillRule::NonZero => body.push_str("W n "),
                SvgFillRule::EvenOdd => body.push_str("W* n "),
            }
            for name in names {
                body.push_str(&format!("/{name} sh\n"));
            }
            body.push_str("Q\n");
            if style.stroke.is_some() && svg_effective_stroke_opacity(style) > 0.001 {
                append_svg_stroke_style(body, style);
                append_path(body);
                body.push_str("S\n");
            }
            return;
        }
    }

    append_svg_style(body, style);
    append_path(body);
    append_svg_paint_operator(body, style);
}

#[allow(clippy::too_many_arguments)]
fn append_svg_ordered_painted_shape<F>(
    body: &mut String,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    page_shadings: &mut Vec<PdfShading>,
    stroke_scale: Option<f32>,
    bbox: Option<(f32, f32, f32, f32)>,
    append_path: &F,
) where
    F: Fn(&mut String),
{
    append_svg_ordered_painted_shape_with_markers(
        body,
        style,
        gradients,
        patterns,
        clip_paths,
        page_shadings,
        stroke_scale,
        bbox,
        append_path,
        |_| {},
    );
}

#[allow(clippy::too_many_arguments)]
fn append_svg_ordered_painted_shape_with_markers<F, M>(
    body: &mut String,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    page_shadings: &mut Vec<PdfShading>,
    stroke_scale: Option<f32>,
    bbox: Option<(f32, f32, f32, f32)>,
    append_path: &F,
    append_markers: M,
) where
    F: Fn(&mut String),
    M: Fn(&mut String),
{
    for layer in style.paint_order.layers {
        match layer {
            SvgPaintLayer::Fill => {
                append_svg_fill_layer(
                    body,
                    style,
                    gradients,
                    patterns,
                    clip_paths,
                    page_shadings,
                    stroke_scale,
                    bbox,
                    append_path,
                );
            }
            SvgPaintLayer::Stroke => {
                append_svg_stroke_layer(body, style, append_path);
            }
            SvgPaintLayer::Markers => append_markers(body),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn append_svg_fill_layer<F>(
    body: &mut String,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    page_shadings: &mut Vec<PdfShading>,
    stroke_scale: Option<f32>,
    bbox: Option<(f32, f32, f32, f32)>,
    append_path: &F,
) -> bool
where
    F: Fn(&mut String),
{
    if style.fill.is_none() || svg_effective_fill_opacity(style) <= 0.001 {
        return false;
    }
    if append_svg_pattern_fill(
        body,
        style,
        gradients,
        patterns,
        clip_paths,
        page_shadings,
        stroke_scale,
        bbox,
        append_path,
    ) {
        return true;
    }
    if let Some(shadings) = svg_fill_shadings(style, gradients, bbox)
        && pdf_shadings_fit(page_shadings, &shadings)
    {
        let names = shadings
            .into_iter()
            .filter_map(|shading| register_pdf_shading(page_shadings, shading))
            .collect::<Vec<_>>();
        body.push_str("q ");
        append_path(body);
        match style.fill_rule {
            SvgFillRule::NonZero => body.push_str("W n "),
            SvgFillRule::EvenOdd => body.push_str("W* n "),
        }
        for name in names {
            body.push_str(&format!("/{name} sh\n"));
        }
        body.push_str("Q\n");
        return true;
    }
    append_svg_fill_style(body, style);
    append_path(body);
    append_svg_fill_operator(body, style);
    true
}

fn append_svg_stroke_layer<F>(body: &mut String, style: SvgStyle, append_path: &F) -> bool
where
    F: Fn(&mut String),
{
    if style.stroke.is_none()
        || style.stroke_width <= 0.001
        || svg_effective_stroke_opacity(style) <= 0.001
    {
        return false;
    }
    append_svg_stroke_style(body, style);
    append_path(body);
    body.push_str("S\n");
    true
}

#[allow(clippy::too_many_arguments)]
fn append_svg_pattern_fill<F>(
    body: &mut String,
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    patterns: &[SvgPatternPaint],
    clip_paths: &[SvgClipPath],
    page_shadings: &mut Vec<PdfShading>,
    stroke_scale: Option<f32>,
    bbox: Option<(f32, f32, f32, f32)>,
    append_path: &F,
) -> bool
where
    F: Fn(&mut String),
{
    if style.fill.is_none() || svg_effective_fill_opacity(style) <= 0.001 {
        return false;
    }
    let Some(pattern) = style.fill_pattern.and_then(|index| patterns.get(index)) else {
        return false;
    };
    if pattern.elements.is_empty()
        || pattern.w <= 0.001
        || pattern.h <= 0.001
        || ![pattern.x, pattern.y, pattern.w, pattern.h]
            .iter()
            .all(|value| value.is_finite())
    {
        return false;
    }
    let Some((bx, by, bw, bh)) = bbox.filter(|(_, _, w, h)| *w > 0.001 && *h > 0.001) else {
        return false;
    };

    let start_x = ((bx - pattern.x) / pattern.w).floor() as i32;
    let end_x = ((bx + bw - pattern.x) / pattern.w).ceil() as i32;
    let start_y = ((by - pattern.y) / pattern.h).floor() as i32;
    let end_y = ((by + bh - pattern.y) / pattern.h).ceil() as i32;
    let cols = end_x.saturating_sub(start_x);
    let rows = end_y.saturating_sub(start_y);
    let tiles = cols.saturating_mul(rows);
    if !(1..=512).contains(&tiles) {
        return false;
    }

    body.push_str("q ");
    append_path(body);
    match style.fill_rule {
        SvgFillRule::NonZero => body.push_str("W n "),
        SvgFillRule::EvenOdd => body.push_str("W* n "),
    }
    let empty_image_index = BTreeMap::new();
    let resources = SvgPaintResources {
        gradients,
        patterns: &[],
        clip_paths,
        markers: &[],
        image_index: &empty_image_index,
        stroke_scale,
    };
    for tile_y in start_y..end_y {
        for tile_x in start_x..end_x {
            let tx = pattern.x + tile_x as f32 * pattern.w;
            let ty = pattern.y + tile_y as f32 * pattern.h;
            body.push_str(&format!("q 1 0 0 1 {} {} cm ", pdf_num(tx), pdf_num(ty)));
            if !pattern.transform.is_identity() {
                body.push_str(&format!(
                    "{} {} {} {} {} {} cm ",
                    pdf_num(pattern.transform.a),
                    pdf_num(pattern.transform.b),
                    pdf_num(pattern.transform.c),
                    pdf_num(pattern.transform.d),
                    pdf_num(pattern.transform.e),
                    pdf_num(pattern.transform.f)
                ));
            }
            for element in &pattern.elements {
                draw_svg_shape(body, element, resources, page_shadings);
            }
            body.push_str("Q\n");
        }
    }
    body.push_str("Q\n");
    true
}

fn append_svg_stroke_style(body: &mut String, style: SvgStyle) {
    if let Some((r, g, b)) = style.stroke {
        body.push_str(&format!(
            "{} {} {} RG ",
            pdf_fixed3(r),
            pdf_fixed3(g),
            pdf_fixed3(b)
        ));
        append_svg_stroke_options(body, style);
    }
}

fn svg_fill_shadings(
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    bbox: Option<(f32, f32, f32, f32)>,
) -> Option<Vec<PdfShading>> {
    if style.fill.is_none() || svg_effective_fill_opacity(style) <= 0.001 {
        return None;
    }
    let bbox = bbox.filter(|(_, _, w, h)| *w > 0.001 && *h > 0.001)?;
    let gradient = style.fill_gradient.and_then(|index| gradients.get(index))?;
    if let Some(linear) = gradient.linear.as_ref() {
        return resolve_svg_linear_gradient(linear, bbox);
    }
    gradient
        .radial
        .as_ref()
        .and_then(|radial| resolve_svg_radial_gradient(radial, bbox))
}

fn svg_stroke_shadings(
    style: SvgStyle,
    gradients: &[SvgGradientPaint],
    bbox: Option<(f32, f32, f32, f32)>,
) -> Option<Vec<PdfShading>> {
    if style.stroke.is_none() || svg_effective_stroke_opacity(style) <= 0.001 {
        return None;
    }
    let bbox = bbox.filter(|(_, _, w, h)| *w > 0.001 && *h > 0.001)?;
    let gradient = style
        .stroke_gradient
        .and_then(|index| gradients.get(index))?;
    if let Some(linear) = gradient.linear.as_ref() {
        return resolve_svg_linear_gradient(linear, bbox);
    }
    gradient
        .radial
        .as_ref()
        .and_then(|radial| resolve_svg_radial_gradient(radial, bbox))
}

fn resolve_svg_linear_gradient(
    gradient: &SvgLinearGradient,
    bbox: (f32, f32, f32, f32),
) -> Option<Vec<PdfShading>> {
    let (x1, y1) = resolve_svg_gradient_point(
        gradient.x1,
        gradient.y1,
        gradient.units,
        gradient.transform,
        bbox,
    );
    let (x2, y2) = resolve_svg_gradient_point(
        gradient.x2,
        gradient.y2,
        gradient.units,
        gradient.transform,
        bbox,
    );
    if ![x1, y1, x2, y2].iter().all(|value| value.is_finite()) {
        return None;
    }
    match gradient.spread {
        SvgGradientSpread::Pad => Some(vec![PdfShading {
            kind: PdfShadingKind::Axial([x1, y1, x2, y2]),
            stops: gradient.stops.clone(),
            extend_start: true,
            extend_end: true,
        }]),
        SvgGradientSpread::Repeat | SvgGradientSpread::Reflect => {
            resolve_svg_repeated_linear_gradient(gradient, bbox, [x1, y1, x2, y2])
        }
    }
}

fn resolve_svg_radial_gradient(
    gradient: &SvgRadialGradient,
    bbox: (f32, f32, f32, f32),
) -> Option<Vec<PdfShading>> {
    let (cx, cy, r) = resolve_svg_radial_gradient_circle(
        gradient.cx,
        gradient.cy,
        gradient.r,
        gradient.units,
        gradient.transform,
        bbox,
    )?;
    let (fx, fy, fr) = resolve_svg_radial_gradient_circle(
        gradient.fx,
        gradient.fy,
        gradient.fr,
        gradient.units,
        gradient.transform,
        bbox,
    )?;
    let focal_distance = svg_distance(fx, fy, cx, cy);
    if r <= 0.001
        || fr < -0.001
        || fr > r
        || focal_distance + fr > r + 0.001
        || ![fx, fy, fr, cx, cy, r]
            .iter()
            .all(|value| value.is_finite())
    {
        return None;
    }
    match gradient.spread {
        SvgGradientSpread::Pad => Some(vec![PdfShading {
            kind: PdfShadingKind::Radial([fx, fy, fr.max(0.0), cx, cy, r]),
            stops: gradient.stops.clone(),
            extend_start: true,
            extend_end: true,
        }]),
        SvgGradientSpread::Repeat | SvgGradientSpread::Reflect => {
            resolve_svg_repeated_radial_gradient(gradient, bbox, (cx, cy, r), (fx, fy, fr))
        }
    }
}

fn resolve_svg_repeated_linear_gradient(
    gradient: &SvgLinearGradient,
    bbox: (f32, f32, f32, f32),
    coords: [f32; 4],
) -> Option<Vec<PdfShading>> {
    let [x1, y1, x2, y2] = coords;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx.mul_add(dx, dy * dy);
    if len_sq <= 0.000_001 || !len_sq.is_finite() {
        return None;
    }

    let (bx, by, bw, bh) = bbox;
    let corners = [(bx, by), (bx + bw, by), (bx, by + bh), (bx + bw, by + bh)];
    let mut min_t = f32::INFINITY;
    let mut max_t = f32::NEG_INFINITY;
    for (x, y) in corners {
        let t = ((x - x1).mul_add(dx, (y - y1) * dy)) / len_sq;
        min_t = min_t.min(t);
        max_t = max_t.max(t);
    }
    if !min_t.is_finite() || !max_t.is_finite() {
        return None;
    }

    let first = min_t.floor();
    let last = max_t.ceil();
    if first < -1024.0 || last > 1024.0 || last <= first {
        return None;
    }
    let first = first as i32;
    let last = last as i32;
    let period_count = last.saturating_sub(first);
    if !(1..=64).contains(&period_count) {
        return None;
    }

    let mut shadings = Vec::with_capacity(period_count as usize);
    for period in first..last {
        let start_x = x1 + dx * period as f32;
        let start_y = y1 + dy * period as f32;
        let end_x = x1 + dx * (period + 1) as f32;
        let end_y = y1 + dy * (period + 1) as f32;
        if ![start_x, start_y, end_x, end_y]
            .iter()
            .all(|value| value.is_finite())
        {
            return None;
        }
        let stops = match gradient.spread {
            SvgGradientSpread::Repeat => gradient.stops.clone(),
            SvgGradientSpread::Reflect if period.rem_euclid(2) == 0 => gradient.stops.clone(),
            SvgGradientSpread::Reflect => svg_reflected_gradient_stops(&gradient.stops),
            SvgGradientSpread::Pad => return None,
        };
        shadings.push(PdfShading {
            kind: PdfShadingKind::Axial([start_x, start_y, end_x, end_y]),
            stops,
            extend_start: false,
            extend_end: false,
        });
    }
    (!shadings.is_empty()).then_some(shadings)
}

fn svg_reflected_gradient_stops(stops: &[SvgGradientStop]) -> Vec<SvgGradientStop> {
    let mut reflected = stops
        .iter()
        .map(|&(offset, color)| ((1.0 - offset).clamp(0.0, 1.0), color))
        .collect::<Vec<_>>();
    reflected.sort_by(|a, b| a.0.total_cmp(&b.0));
    reflected
}

fn resolve_svg_repeated_radial_gradient(
    gradient: &SvgRadialGradient,
    bbox: (f32, f32, f32, f32),
    outer: (f32, f32, f32),
    focal: (f32, f32, f32),
) -> Option<Vec<PdfShading>> {
    let (cx, cy, r) = outer;
    let (fx, fy, fr) = focal;
    if fr.abs() > 0.001 || svg_distance(fx, fy, cx, cy) > 0.001 || r <= 0.001 {
        return None;
    }
    let (bx, by, bw, bh) = bbox;
    let max_radius = [(bx, by), (bx + bw, by), (bx, by + bh), (bx + bw, by + bh)]
        .into_iter()
        .map(|(x, y)| svg_distance(cx, cy, x, y))
        .fold(0.0f32, f32::max);
    if !max_radius.is_finite() || max_radius <= 0.001 {
        return None;
    }
    let period_count = (max_radius / r).ceil();
    if !(1.0..=64.0).contains(&period_count) {
        return None;
    }
    let period_count = period_count as i32;
    let mut shadings = Vec::with_capacity(period_count as usize);
    for period in 0..period_count {
        let start_r = r * period as f32;
        let end_r = r * (period + 1) as f32;
        if ![start_r, end_r].iter().all(|value| value.is_finite()) {
            return None;
        }
        let stops = match gradient.spread {
            SvgGradientSpread::Repeat => gradient.stops.clone(),
            SvgGradientSpread::Reflect if period.rem_euclid(2) == 0 => gradient.stops.clone(),
            SvgGradientSpread::Reflect => svg_reflected_gradient_stops(&gradient.stops),
            SvgGradientSpread::Pad => return None,
        };
        shadings.push(PdfShading {
            kind: PdfShadingKind::Radial([cx, cy, start_r, cx, cy, end_r]),
            stops,
            extend_start: false,
            extend_end: false,
        });
    }
    (!shadings.is_empty()).then_some(shadings)
}

fn resolve_svg_radial_gradient_circle(
    x_len: SvgGradientLength,
    y_len: SvgGradientLength,
    r_len: SvgGradientLength,
    units: SvgGradientUnits,
    transform: SvgTransform,
    bbox: (f32, f32, f32, f32),
) -> Option<(f32, f32, f32)> {
    let (x, y, w, h) = bbox;
    let (cx, cy, rx, ry) = match units {
        SvgGradientUnits::ObjectBoundingBox => {
            let cx = svg_gradient_object_bbox_coord(x_len);
            let cy = svg_gradient_object_bbox_coord(y_len);
            let r = svg_gradient_object_bbox_coord(r_len).abs();
            let center = transform.apply_point(cx, cy);
            let x_edge = transform.apply_point(cx + r, cy);
            let y_edge = transform.apply_point(cx, cy + r);
            let map = |(gx, gy): (f32, f32)| (x + gx * w, y + gy * h);
            let (px, py) = map(center);
            let (xx, xy) = map(x_edge);
            let (yx, yy) = map(y_edge);
            (
                px,
                py,
                svg_distance(px, py, xx, xy),
                svg_distance(px, py, yx, yy),
            )
        }
        SvgGradientUnits::UserSpaceOnUse => {
            if x_len.percent || y_len.percent || r_len.percent {
                return None;
            }
            let cx = svg_gradient_user_space_coord(x_len, w);
            let cy = svg_gradient_user_space_coord(y_len, h);
            let r = svg_gradient_user_space_radius(r_len, w, h).abs();
            let (px, py) = transform.apply_point(cx, cy);
            let (xx, xy) = transform.apply_point(cx + r, cy);
            let (yx, yy) = transform.apply_point(cx, cy + r);
            (
                px,
                py,
                svg_distance(px, py, xx, xy),
                svg_distance(px, py, yx, yy),
            )
        }
    };
    let radius = (rx + ry) * 0.5;
    if !rx.is_finite() || !ry.is_finite() || !radius.is_finite() {
        return None;
    }
    if !svg_radii_are_circular(rx, ry, radius) {
        return None;
    }
    Some((cx, cy, radius))
}

fn resolve_svg_gradient_point(
    x_len: SvgGradientLength,
    y_len: SvgGradientLength,
    units: SvgGradientUnits,
    transform: SvgTransform,
    bbox: (f32, f32, f32, f32),
) -> (f32, f32) {
    let (x, y, w, h) = bbox;
    match units {
        SvgGradientUnits::ObjectBoundingBox => {
            let (gx, gy) = transform.apply_point(
                svg_gradient_object_bbox_coord(x_len),
                svg_gradient_object_bbox_coord(y_len),
            );
            (x + gx * w, y + gy * h)
        }
        SvgGradientUnits::UserSpaceOnUse => transform.apply_point(x_len.value, y_len.value),
    }
}

fn svg_gradient_object_bbox_coord(length: SvgGradientLength) -> f32 {
    if length.percent {
        length.value / 100.0
    } else {
        length.value
    }
}

fn svg_gradient_user_space_coord(length: SvgGradientLength, fallback_extent: f32) -> f32 {
    if length.percent {
        fallback_extent * length.value / 100.0
    } else {
        length.value
    }
}

fn svg_gradient_user_space_radius(length: SvgGradientLength, width: f32, height: f32) -> f32 {
    if length.percent {
        width.min(height) * length.value / 100.0
    } else {
        length.value
    }
}

fn svg_distance(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    ((x2 - x1).mul_add(x2 - x1, (y2 - y1) * (y2 - y1))).sqrt()
}

fn svg_radii_are_circular(rx: f32, ry: f32, radius: f32) -> bool {
    if radius <= 0.001 {
        return true;
    }
    (rx - ry).abs() <= radius.max(1.0) * 0.01
}

fn register_pdf_shading(
    page_shadings: &mut Vec<PdfShading>,
    shading: PdfShading,
) -> Option<String> {
    if let Some(index) = page_shadings
        .iter()
        .position(|existing| *existing == shading)
    {
        return Some(pdf_shading_name(index));
    }
    if page_shadings.len() >= 256 {
        return None;
    }
    page_shadings.push(shading);
    Some(pdf_shading_name(page_shadings.len() - 1))
}

fn pdf_shadings_fit(existing: &[PdfShading], shadings: &[PdfShading]) -> bool {
    let mut pending = Vec::new();
    for shading in shadings {
        if existing.iter().any(|registered| registered == shading)
            || pending
                .iter()
                .any(|registered: &&PdfShading| **registered == *shading)
        {
            continue;
        }
        pending.push(shading);
        if existing.len().saturating_add(pending.len()) > 256 {
            return false;
        }
    }
    true
}

fn pdf_shading_name(index: usize) -> String {
    format!("SG{}", index + 1)
}

fn svg_style_has_paint(style: SvgStyle) -> bool {
    style.visible
        && ((style.fill.is_some() && svg_effective_fill_opacity(style) > 0.001)
            || (style.stroke.is_some() && svg_effective_stroke_opacity(style) > 0.001))
}

fn svg_style_has_marker_paint(style: SvgStyle) -> bool {
    svg_style_has_paint(style)
        || (style.visible
            && ((style.fill_context.is_some() && svg_effective_fill_opacity(style) > 0.001)
                || (style.stroke_context.is_some() && svg_effective_stroke_opacity(style) > 0.001)))
}

fn svg_effective_fill_opacity(style: SvgStyle) -> f32 {
    (style.opacity * style.fill_opacity).clamp(0.0, 1.0)
}

fn svg_effective_stroke_opacity(style: SvgStyle) -> f32 {
    (style.opacity * style.stroke_opacity).clamp(0.0, 1.0)
}

fn svg_style_alpha_values(style: SvgStyle) -> Option<(u16, u16)> {
    let fill_alpha = if style.fill.is_some() {
        quantize_svg_alpha(svg_effective_fill_opacity(style))
    } else {
        1000
    };
    let stroke_alpha = if style.stroke.is_some() {
        quantize_svg_alpha(svg_effective_stroke_opacity(style))
    } else {
        1000
    };
    (fill_alpha < 1000 || stroke_alpha < 1000).then_some((fill_alpha, stroke_alpha))
}

fn quantize_svg_alpha(alpha: f32) -> u16 {
    (alpha.clamp(0.0, 1.0) * 1000.0).round() as u16
}

fn append_svg_alpha_state(body: &mut String, fill_alpha: u16, stroke_alpha: u16) {
    body.push_str(&format!("/GSa{fill_alpha:04}{stroke_alpha:04} gs"));
}

fn append_svg_rounded_rect_path(
    body: &mut String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rx: f32,
    ry: f32,
) {
    let kx = rx * 0.5523;
    let ky = ry * 0.5523;
    let x1 = x + w;
    let y1 = y + h;
    body.push_str(&format!(
        "{mx} {y} m {lx} {y} l \
         {c1x} {y} {x1} {c1y} {x1} {my} c \
         {x1} {ly} l \
         {x1} {c2y} {c2x} {y1} {lx} {y1} c \
         {mx} {y1} l \
         {c3x} {y1} {x} {c3y} {x} {ly} c \
         {x} {my} l \
         {x} {c4y} {c4x} {y} {mx} {y} c ",
        x = pdf_num(x),
        y = pdf_num(y),
        x1 = pdf_num(x1),
        y1 = pdf_num(y1),
        mx = pdf_num(x + rx),
        lx = pdf_num(x1 - rx),
        my = pdf_num(y + ry),
        ly = pdf_num(y1 - ry),
        c1x = pdf_num(x1 - rx + kx),
        c1y = pdf_num(y + ry - ky),
        c2y = pdf_num(y1 - ry + ky),
        c2x = pdf_num(x1 - rx + kx),
        c3x = pdf_num(x + rx - kx),
        c3y = pdf_num(y1 - ry + ky),
        c4y = pdf_num(y + ry - ky),
        c4x = pdf_num(x + rx - kx),
    ));
}

fn append_svg_ellipse_path(body: &mut String, cx: f32, cy: f32, rx: f32, ry: f32) {
    let k = 0.552_284_8;
    body.push_str(&format!(
        "{x0} {cy} m \
         {x0} {c1y} {c1x} {y0} {cx} {y0} c \
         {c2x} {y0} {x1} {c1y} {x1} {cy} c \
         {x1} {c2y} {c2x} {y1} {cx} {y1} c \
         {c1x} {y1} {x0} {c2y} {x0} {cy} c h ",
        x0 = pdf_num(cx - rx),
        x1 = pdf_num(cx + rx),
        y0 = pdf_num(cy - ry),
        y1 = pdf_num(cy + ry),
        cx = pdf_num(cx),
        cy = pdf_num(cy),
        c1x = pdf_num(cx - rx * k),
        c2x = pdf_num(cx + rx * k),
        c1y = pdf_num(cy - ry * k),
        c2y = pdf_num(cy + ry * k),
    ));
}

fn append_svg_path_ops(body: &mut String, ops: &[SvgPathOp]) {
    let mut current = (0.0f32, 0.0f32);
    let mut subpath_start = None;
    for op in ops {
        match *op {
            SvgPathOp::Move(x, y) => {
                current = (x, y);
                subpath_start = Some(current);
                body.push_str(&format!("{} {} m ", pdf_num(x), pdf_num(y)));
            }
            SvgPathOp::Line(x, y) => {
                current = (x, y);
                body.push_str(&format!("{} {} l ", pdf_num(x), pdf_num(y)));
            }
            SvgPathOp::Cubic(x1, y1, x2, y2, x, y) => {
                current = (x, y);
                body.push_str(&format!(
                    "{} {} {} {} {} {} c ",
                    pdf_num(x1),
                    pdf_num(y1),
                    pdf_num(x2),
                    pdf_num(y2),
                    pdf_num(x),
                    pdf_num(y)
                ));
            }
            SvgPathOp::Quad(x1, y1, x, y) => {
                let c1 = (
                    current.0 + (x1 - current.0) * (2.0 / 3.0),
                    current.1 + (y1 - current.1) * (2.0 / 3.0),
                );
                let c2 = (x + (x1 - x) * (2.0 / 3.0), y + (y1 - y) * (2.0 / 3.0));
                current = (x, y);
                body.push_str(&format!(
                    "{} {} {} {} {} {} c ",
                    pdf_num(c1.0),
                    pdf_num(c1.1),
                    pdf_num(c2.0),
                    pdf_num(c2.1),
                    pdf_num(x),
                    pdf_num(y)
                ));
            }
            SvgPathOp::Close => {
                if let Some(start) = subpath_start {
                    current = start;
                }
                body.push_str("h ");
            }
        }
    }
}

fn svg_points_bbox(points: &[(f32, f32)]) -> Option<(f32, f32, f32, f32)> {
    let &(first_x, first_y) = points.first()?;
    let mut min_x = first_x;
    let mut max_x = first_x;
    let mut min_y = first_y;
    let mut max_y = first_y;
    for &(x, y) in &points[1..] {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    Some((min_x, min_y, max_x - min_x, max_y - min_y))
}

fn svg_path_bbox(ops: &[SvgPathOp]) -> Option<(f32, f32, f32, f32)> {
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    let mut include = |x: f32, y: f32| {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        bounds = Some(match bounds {
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
            None => (x, y, x, y),
        });
    };
    for op in ops {
        match *op {
            SvgPathOp::Move(x, y) | SvgPathOp::Line(x, y) => include(x, y),
            SvgPathOp::Cubic(x1, y1, x2, y2, x, y) => {
                include(x1, y1);
                include(x2, y2);
                include(x, y);
            }
            SvgPathOp::Quad(x1, y1, x, y) => {
                include(x1, y1);
                include(x, y);
            }
            SvgPathOp::Close => {}
        }
    }
    bounds.map(|(min_x, min_y, max_x, max_y)| (min_x, min_y, max_x - min_x, max_y - min_y))
}

fn svg_path_end_tangent(ops: &[SvgPathOp]) -> Option<((f32, f32), (f32, f32))> {
    let mut current = (0.0f32, 0.0f32);
    let mut subpath_start = None;
    let mut last_segment = None;
    for op in ops {
        match *op {
            SvgPathOp::Move(x, y) => {
                current = (x, y);
                subpath_start = Some(current);
            }
            SvgPathOp::Line(x, y) => {
                last_segment = Some(((x, y), current));
                current = (x, y);
            }
            SvgPathOp::Cubic(_, _, x2, y2, x, y) => {
                last_segment = Some(((x, y), (x2, y2)));
                current = (x, y);
            }
            SvgPathOp::Quad(x1, y1, x, y) => {
                last_segment = Some(((x, y), (x1, y1)));
                current = (x, y);
            }
            SvgPathOp::Close => {
                if let Some(start) = subpath_start {
                    last_segment = Some((start, current));
                    current = start;
                }
            }
        }
    }
    last_segment
}

fn svg_path_start_tangent(ops: &[SvgPathOp]) -> Option<((f32, f32), (f32, f32))> {
    let mut start = None;
    for op in ops {
        match *op {
            SvgPathOp::Move(x, y) => start = Some((x, y)),
            SvgPathOp::Line(x, y) => return start.map(|tip| (tip, (x, y))),
            SvgPathOp::Cubic(x1, y1, _, _, _, _) => return start.map(|tip| (tip, (x1, y1))),
            SvgPathOp::Quad(x1, y1, _, _) => return start.map(|tip| (tip, (x1, y1))),
            SvgPathOp::Close => {}
        }
    }
    None
}

fn append_svg_marker_or_arrowhead(
    body: &mut String,
    marker_ref: SvgMarkerRef,
    markers: &[SvgMarker],
    placement: SvgMarkerPlacement,
    tip: (f32, f32),
    tail: (f32, f32),
    paint: SvgMarkerPaint,
) {
    if let Some(marker) = marker_ref.index.and_then(|index| markers.get(index))
        && append_svg_marker(body, marker, placement, tip, tail, paint)
    {
        return;
    }
    if let Some(stroke) = paint.stroke {
        append_svg_arrowhead(body, tip, tail, stroke, paint.stroke_width);
    }
}

fn append_svg_marker(
    body: &mut String,
    marker: &SvgMarker,
    placement: SvgMarkerPlacement,
    tip: (f32, f32),
    tail: (f32, f32),
    paint: SvgMarkerPaint,
) -> bool {
    if marker.shapes.is_empty() {
        return false;
    }
    let Some((ux, uy)) = svg_marker_orient_vector(marker.orient, placement, tip, tail) else {
        return false;
    };
    let scale = if marker.units_stroke_width {
        paint.stroke_width.max(0.1)
    } else {
        1.0
    };
    let ux = svg_marker_matrix_component(ux);
    let uy = svg_marker_matrix_component(uy);
    let nx = svg_marker_matrix_component(-uy);
    let ny = svg_marker_matrix_component(ux);
    let view_box_transform = svg_marker_view_box_transform(marker);
    let (ref_x, ref_y) = view_box_transform
        .map(|transform| transform.map_point(marker.ref_x, marker.ref_y))
        .unwrap_or((marker.ref_x, marker.ref_y));
    body.push_str(&format!(
        "q {ux} {uy} {nx} {ny} {tx} {ty} cm {scale} 0 0 {scale} 0 0 cm 1 0 0 1 {rx} {ry} cm ",
        ux = pdf_num(ux),
        uy = pdf_num(uy),
        nx = pdf_num(nx),
        ny = pdf_num(ny),
        tx = pdf_num(tip.0),
        ty = pdf_num(tip.1),
        scale = pdf_num(scale),
        rx = pdf_num(-ref_x),
        ry = pdf_num(-ref_y),
    ));
    if let Some(transform) = view_box_transform {
        body.push_str(&format!(
            "{sx} 0 0 {sy} {tx} {ty} cm ",
            sx = pdf_num(svg_marker_matrix_component(transform.sx)),
            sy = pdf_num(svg_marker_matrix_component(transform.sy)),
            tx = pdf_num(svg_marker_matrix_component(transform.tx)),
            ty = pdf_num(svg_marker_matrix_component(transform.ty)),
        ));
    }
    for shape in &marker.shapes {
        let shape_style = svg_style_with_marker_context(shape.style, paint);
        if !svg_style_has_paint(shape_style) {
            continue;
        }
        let transformed = append_svg_element_state_prefix(body, shape_style, &[], None);
        if append_svg_shadow_prefix(body, shape_style) {
            append_svg_path_ops(body, &shape.ops);
            append_svg_shadow_operator(body, shape_style);
        }
        append_svg_marker_shape_paint(body, shape_style, &shape.ops);
        append_svg_transform_suffix(body, transformed);
    }
    body.push_str("Q\n");
    true
}

fn append_svg_marker_shape_paint(body: &mut String, style: SvgStyle, ops: &[SvgPathOp]) {
    if style.paint_order != SvgPaintOrder::NORMAL && style.fill.is_some() && style.stroke.is_some()
    {
        for layer in style.paint_order.layers {
            match layer {
                SvgPaintLayer::Fill => {
                    if style.fill.is_some() && svg_effective_fill_opacity(style) > 0.001 {
                        append_svg_fill_style(body, style);
                        append_svg_path_ops(body, ops);
                        append_svg_fill_operator(body, style);
                    }
                }
                SvgPaintLayer::Stroke => {
                    append_svg_stroke_layer(body, style, &|body| append_svg_path_ops(body, ops));
                }
                SvgPaintLayer::Markers => {}
            }
        }
        return;
    }

    append_svg_style(body, style);
    append_svg_path_ops(body, ops);
    append_svg_paint_operator(body, style);
}

fn svg_style_with_marker_context(mut style: SvgStyle, paint: SvgMarkerPaint) -> SvgStyle {
    if let Some(context) = style.fill_context {
        style.fill = svg_marker_context_paint(context, paint);
        style.fill_gradient = None;
        style.fill_pattern = None;
        style.fill_current_color = false;
        style.fill_context = None;
    }
    if let Some(context) = style.stroke_context {
        style.stroke = svg_marker_context_paint(context, paint);
        style.stroke_gradient = None;
        style.stroke_current_color = false;
        style.stroke_context = None;
    }
    style
}

fn svg_marker_context_paint(context: SvgContextPaint, paint: SvgMarkerPaint) -> Option<SvgColor> {
    match context {
        SvgContextPaint::Fill => paint.fill,
        SvgContextPaint::Stroke => paint.stroke,
    }
}

#[derive(Clone, Copy)]
struct SvgMarkerViewBoxTransform {
    sx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
}

impl SvgMarkerViewBoxTransform {
    fn map_point(self, x: f32, y: f32) -> (f32, f32) {
        (self.tx + x * self.sx, self.ty + y * self.sy)
    }
}

fn svg_marker_view_box_transform(marker: &SvgMarker) -> Option<SvgMarkerViewBoxTransform> {
    let marker_view_box = marker.view_box?;
    let view_box = marker_view_box.view_box;
    let viewport = marker_view_box.viewport;
    let raw_sx = viewport.w / view_box.w;
    let raw_sy = viewport.h / view_box.h;
    if ![raw_sx, raw_sy].iter().all(|value| value.is_finite()) || raw_sx <= 0.0 || raw_sy <= 0.0 {
        return None;
    }
    match marker_view_box.preserve_aspect.mode {
        SvgAspectScaleMode::None => Some(SvgMarkerViewBoxTransform {
            sx: raw_sx,
            sy: raw_sy,
            tx: -view_box.x * raw_sx,
            ty: -view_box.y * raw_sy,
        }),
        SvgAspectScaleMode::Meet | SvgAspectScaleMode::Slice => {
            let scale = if marker_view_box.preserve_aspect.mode == SvgAspectScaleMode::Slice {
                raw_sx.max(raw_sy)
            } else {
                raw_sx.min(raw_sy)
            };
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            let content_w = view_box.w * scale;
            let content_h = view_box.h * scale;
            let offset_x = (viewport.w - content_w) * marker_view_box.preserve_aspect.align_x;
            let offset_y = (viewport.h - content_h) * marker_view_box.preserve_aspect.align_y;
            Some(SvgMarkerViewBoxTransform {
                sx: scale,
                sy: scale,
                tx: offset_x - view_box.x * scale,
                ty: offset_y - view_box.y * scale,
            })
        }
    }
}

fn svg_marker_orient_vector(
    orient: SvgMarkerOrient,
    placement: SvgMarkerPlacement,
    tip: (f32, f32),
    tail: (f32, f32),
) -> Option<(f32, f32)> {
    match orient {
        SvgMarkerOrient::Angle(degrees) => {
            let radians = degrees.to_radians();
            let ux = radians.cos();
            let uy = radians.sin();
            ([ux, uy].iter().all(|value| value.is_finite())).then_some((ux, uy))
        }
        SvgMarkerOrient::Auto => match placement {
            SvgMarkerPlacement::Start => svg_unit_vector(tip, tail),
            SvgMarkerPlacement::Mid | SvgMarkerPlacement::End => svg_unit_vector(tail, tip),
        },
        SvgMarkerOrient::AutoStartReverse => svg_unit_vector(tail, tip),
    }
}

fn svg_marker_matrix_component(value: f32) -> f32 {
    if value.abs() <= 0.000_01 { 0.0 } else { value }
}

fn append_svg_arrowhead(
    body: &mut String,
    tip: (f32, f32),
    tail: (f32, f32),
    color: (f32, f32, f32),
    stroke_width: f32,
) {
    let dx = tip.0 - tail.0;
    let dy = tip.1 - tail.1;
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() || len <= 0.001 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let size = (stroke_width.max(1.0) * 4.6).max(5.0);
    let half = size * 0.42;
    let base = (tip.0 - ux * size, tip.1 - uy * size);
    let perp = (-uy * half, ux * half);
    let p1 = (base.0 + perp.0, base.1 + perp.1);
    let p2 = (base.0 - perp.0, base.1 - perp.1);
    body.push_str(&format!(
        "{r} {g} {b} rg {x1} {y1} m {tx} {ty} l {x2} {y2} l h f\n",
        r = pdf_fixed3(color.0),
        g = pdf_fixed3(color.1),
        b = pdf_fixed3(color.2),
        x1 = pdf_num(p1.0),
        y1 = pdf_num(p1.1),
        tx = pdf_num(tip.0),
        ty = pdf_num(tip.1),
        x2 = pdf_num(p2.0),
        y2 = pdf_num(p2.1),
    ));
}

#[allow(clippy::too_many_arguments)]
fn draw_svg_text(
    body: &mut String,
    text: &SvgText,
    image_transform: SvgImageTransform,
    clip_paths: &[SvgClipPath],
    subsets: &[EmbeddedFace],
    subset_lookup: &EmbeddedFaceLookup,
    faces: &Faces,
    shaped_cache: &ShapedRunCache,
) {
    if text.text.is_empty() {
        return;
    }
    let slot = text.slot;
    let Some(face) = subset_lookup.get(subsets, slot) else {
        return;
    };
    let source = faces.get(slot);
    let fallback;
    let shaped = match shaped_cache
        .get(&slot)
        .and_then(|slot_cache| slot_cache.get(text.text.as_str()))
    {
        Some(run) => run.glyphs.as_slice(),
        None => {
            fallback = shape_run(source, &face.lig, &text.text);
            fallback.glyphs.as_slice()
        }
    };
    let mut matrix = svg_text_pdf_matrix(text, image_transform);
    let (spacing, width, glyph_scale) =
        svg_text_layout_adjustment(text, matrix, slot, faces, shaped);
    if glyph_scale != 1.0 {
        matrix.a *= glyph_scale;
        matrix.b *= glyph_scale;
    }
    match text.anchor {
        SvgTextAnchor::Start => {}
        SvgTextAnchor::Middle => {
            matrix.x -= matrix.a * width * 0.5;
            matrix.y -= matrix.b * width * 0.5;
        }
        SvgTextAnchor::End => {
            matrix.x -= matrix.a * width;
            matrix.y -= matrix.b * width;
        }
    }
    let (r, g, b) = text.fill;
    body.push_str("q\n");
    if let Some((x, y, w, h)) = image_transform.viewport_clip {
        body.push_str(&format!(
            "{x} {y} {w} {h} re W n\n",
            x = pdf_num(x),
            y = pdf_num(y),
            w = pdf_num(w),
            h = pdf_num(h),
        ));
    }
    if text.opacity < 0.999 {
        append_svg_alpha_state(
            body,
            quantize_svg_alpha(text.opacity),
            quantize_svg_alpha(text.opacity),
        );
        body.push('\n');
    }
    let text_bbox = svg_text_link_bbox(text);
    if let Some(clip_path) = text
        .clip_path
        .and_then(|index| clip_paths.get(index))
        .filter(|clip_path| svg_clip_path_applicable(clip_path, text_bbox))
    {
        append_svg_text_clip_path(body, clip_path, text.transform, image_transform, text_bbox);
    }
    if let Some(mask_path) = text
        .mask_path
        .and_then(|index| clip_paths.get(index))
        .filter(|mask_path| svg_clip_path_applicable(mask_path, text_bbox))
    {
        append_svg_text_clip_path(body, mask_path, text.transform, image_transform, text_bbox);
    }
    body.push_str(&format!(
        "{r} {g} {b} rg\nBT /F{font} {size} Tf {a} {b_matrix} {c} {d} {x} {y} Tm {tj} TJ ET\n",
        r = pdf_fixed3(r),
        g = pdf_fixed3(g),
        b = pdf_fixed3(b),
        font = slot,
        size = pdf_fixed2(matrix.size),
        a = pdf_num(matrix.a),
        b_matrix = pdf_num(matrix.b),
        c = pdf_num(matrix.c),
        d = pdf_num(matrix.d),
        x = pdf_fixed2(matrix.x),
        y = pdf_fixed2(matrix.y),
        tj = kerned_tj_with_spacing(
            &face.map,
            source,
            &face.kern,
            shaped,
            pdf_tj_spacing_adjust(spacing, matrix.size),
        ),
    ));
    append_svg_text_decoration(body, text.decoration, matrix, width, text.fill);
    body.push_str("Q\n");
}

fn append_svg_text_decoration(
    body: &mut String,
    decoration: SvgTextDecoration,
    matrix: SvgTextMatrix,
    width: f32,
    color: SvgColor,
) {
    if decoration.is_empty() || width <= 0.001 || !width.is_finite() || !matrix.size.is_finite() {
        return;
    }
    let stroke_width = (matrix.size * 0.055).clamp(0.35, 3.0);
    let (r, g, b) = color;
    body.push_str(&format!(
        "{r} {g} {b} RG {w} w 0 J [] 0 d\n",
        r = pdf_fixed3(r),
        g = pdf_fixed3(g),
        b = pdf_fixed3(b),
        w = pdf_num(stroke_width),
    ));
    if decoration.contains(SvgTextDecoration::UNDERLINE) {
        append_svg_text_decoration_line(body, matrix, width, -0.12 * matrix.size);
    }
    if decoration.contains(SvgTextDecoration::OVERLINE) {
        append_svg_text_decoration_line(body, matrix, width, 0.72 * matrix.size);
    }
    if decoration.contains(SvgTextDecoration::LINE_THROUGH) {
        append_svg_text_decoration_line(body, matrix, width, 0.30 * matrix.size);
    }
}

fn append_svg_text_decoration_line(
    body: &mut String,
    matrix: SvgTextMatrix,
    width: f32,
    y_offset: f32,
) {
    let x1 = matrix.x + matrix.c * y_offset;
    let y1 = matrix.y + matrix.d * y_offset;
    let x2 = x1 + matrix.a * width;
    let y2 = y1 + matrix.b * width;
    if ![x1, y1, x2, y2].iter().all(|value| value.is_finite()) {
        return;
    }
    body.push_str(&format!(
        "{x1} {y1} m {x2} {y2} l S\n",
        x1 = pdf_num(x1),
        y1 = pdf_num(y1),
        x2 = pdf_num(x2),
        y2 = pdf_num(y2),
    ));
}

fn svg_text_layout_adjustment(
    text: &SvgText,
    matrix: SvgTextMatrix,
    slot: u8,
    faces: &Faces,
    shaped: &[u16],
) -> (f32, f32, f32) {
    let base_spacing = svg_letter_spacing_for_matrix(text, matrix);
    let natural_width =
        svg_text_width_with_spacing(&text.text, matrix.size, slot, faces, base_spacing, shaped);
    let Some(target_width) = svg_text_target_width_for_matrix(text, matrix) else {
        return (base_spacing, natural_width, 1.0);
    };
    if target_width < 0.0 || !target_width.is_finite() || natural_width <= 0.001 {
        return (base_spacing, natural_width, 1.0);
    }
    match text.length_adjust {
        SvgLengthAdjust::Spacing => {
            let gaps = shaped.len().saturating_sub(1);
            if gaps == 0 {
                return (base_spacing, natural_width, 1.0);
            }
            let spacing = base_spacing + (target_width - natural_width) / gaps as f32;
            if spacing.is_finite() {
                (spacing, target_width, 1.0)
            } else {
                (base_spacing, natural_width, 1.0)
            }
        }
        SvgLengthAdjust::SpacingAndGlyphs => {
            let glyph_scale = (target_width / natural_width).clamp(0.001, 1000.0);
            if glyph_scale.is_finite() {
                (base_spacing, natural_width, glyph_scale)
            } else {
                (base_spacing, natural_width, 1.0)
            }
        }
    }
}

fn svg_text_target_width_for_matrix(text: &SvgText, matrix: SvgTextMatrix) -> Option<f32> {
    let target = text.text_length?;
    let scale = matrix.size / text.font_size.max(0.001);
    let target = target * scale;
    (target.is_finite() && target >= 0.0).then_some(target)
}

fn svg_letter_spacing_for_matrix(text: &SvgText, matrix: SvgTextMatrix) -> f32 {
    let scale = matrix.size / text.font_size.max(0.001);
    let spacing = text.letter_spacing * scale;
    if spacing.is_finite() { spacing } else { 0.0 }
}

fn svg_text_width_with_spacing(
    text: &str,
    size: f32,
    slot: u8,
    faces: &Faces,
    spacing: f32,
    shaped: &[u16],
) -> f32 {
    let width = text_width(text, size, slot, faces);
    width + shaped.len().saturating_sub(1) as f32 * spacing
}

fn pdf_tj_spacing_adjust(spacing: f32, font_size: f32) -> i32 {
    if !spacing.is_finite() || !font_size.is_finite() || font_size <= 0.001 {
        return 0;
    }
    let adjustment = -(spacing * 1000.0 / font_size).round();
    adjustment.clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn svg_text_pdf_matrix(text: &SvgText, image_transform: SvgImageTransform) -> SvgTextMatrix {
    let baseline_y = text.y + text.baseline.y_shift_em() * text.font_size;
    let (svg_x, svg_y) = text.transform.apply_point(text.x, baseline_y);
    let (x, y) = image_transform.map_point(svg_x, svg_y);

    // SVG text is emitted outside the flipped shape CTM so it remains
    // selectable. Rebuild the text matrix by converting SVG's y-down linear
    // transform into PDF's y-up text coordinates, while keeping the average
    // scale in the font size for byte-identical identity-transform output.
    let raw_a = image_transform.sx * text.transform.a;
    let raw_b = -image_transform.sy * text.transform.b;
    let raw_c = -image_transform.sx * text.transform.c;
    let raw_d = image_transform.sy * text.transform.d;
    let x_axis = (raw_a * raw_a + raw_b * raw_b).sqrt();
    let y_axis = (raw_c * raw_c + raw_d * raw_d).sqrt();
    let fallback_scale = ((image_transform.sx + image_transform.sy) * 0.5).max(0.001);
    let scale = (x_axis + y_axis) * 0.5;
    let scale = if scale.is_finite() && scale > 0.001 {
        scale
    } else {
        fallback_scale
    };
    let size = (text.font_size * scale).clamp(1.0, 96.0);
    let normalize = |value: f32| {
        let value = value / scale;
        if value.is_finite() && value.abs() > 0.000_01 {
            value
        } else {
            0.0
        }
    };
    SvgTextMatrix {
        a: normalize(raw_a),
        b: normalize(raw_b),
        c: normalize(raw_c),
        d: normalize(raw_d),
        x,
        y,
        size,
    }
}

#[allow(clippy::too_many_arguments)]
fn append_svg_text_clip_path(
    body: &mut String,
    clip_path: &SvgClipPath,
    transform: SvgTransform,
    image_transform: SvgImageTransform,
    element_bbox: Option<(f32, f32, f32, f32)>,
) {
    let transformed_ops;
    let ops = match clip_path.units {
        SvgClipPathUnits::UserSpaceOnUse => clip_path.ops.as_slice(),
        SvgClipPathUnits::ObjectBoundingBox => {
            let Some(bbox_transform) = svg_object_bbox_transform(element_bbox) else {
                return;
            };
            transformed_ops = {
                let mut ops = clip_path.ops.clone();
                transform_svg_path_ops(&mut ops, bbox_transform);
                ops
            };
            transformed_ops.as_slice()
        }
    };
    let map = |x: f32, y: f32| {
        let (x, y) = transform.apply_point(x, y);
        image_transform.map_point(x, y)
    };
    let mut current = (0.0f32, 0.0f32);
    let mut subpath_start = None;
    for op in ops {
        match *op {
            SvgPathOp::Move(x, y) => {
                let (x, y) = map(x, y);
                current = (x, y);
                subpath_start = Some(current);
                body.push_str(&format!("{} {} m ", pdf_num(x), pdf_num(y)));
            }
            SvgPathOp::Line(x, y) => {
                let (x, y) = map(x, y);
                current = (x, y);
                body.push_str(&format!("{} {} l ", pdf_num(x), pdf_num(y)));
            }
            SvgPathOp::Cubic(x1, y1, x2, y2, x, y) => {
                let (x1, y1) = map(x1, y1);
                let (x2, y2) = map(x2, y2);
                let (x, y) = map(x, y);
                current = (x, y);
                body.push_str(&format!(
                    "{} {} {} {} {} {} c ",
                    pdf_num(x1),
                    pdf_num(y1),
                    pdf_num(x2),
                    pdf_num(y2),
                    pdf_num(x),
                    pdf_num(y)
                ));
            }
            SvgPathOp::Quad(x1, y1, x, y) => {
                let (x1, y1) = map(x1, y1);
                let (x, y) = map(x, y);
                let c1 = (
                    current.0 + (x1 - current.0) * (2.0 / 3.0),
                    current.1 + (y1 - current.1) * (2.0 / 3.0),
                );
                let c2 = (x + (x1 - x) * (2.0 / 3.0), y + (y1 - y) * (2.0 / 3.0));
                current = (x, y);
                body.push_str(&format!(
                    "{} {} {} {} {} {} c ",
                    pdf_num(c1.0),
                    pdf_num(c1.1),
                    pdf_num(c2.0),
                    pdf_num(c2.1),
                    pdf_num(x),
                    pdf_num(y)
                ));
            }
            SvgPathOp::Close => {
                if let Some(start) = subpath_start {
                    current = start;
                }
                body.push_str("h ");
            }
        }
    }
    match clip_path.fill_rule {
        SvgFillRule::NonZero => body.push_str("W n\n"),
        SvgFillRule::EvenOdd => body.push_str("W* n\n"),
    }
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
    subset_lookup: &EmbeddedFaceLookup,
    faces: &Faces,
    shaped_cache: &ShapedRunCache,
    palette: &Palette,
) {
    if seg.text.is_empty() {
        return;
    }
    let Some(face) = subset_lookup.get(subsets, seg.slot) else {
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
        append_rgb_fill_operator(body, (r, g, b));
        *current_fill = seg.fill;
    }
    append_text_segment_operator(
        body, seg.slot, size, seg.x, y, &face.map, source, &face.kern, shaped,
    );
    // Strikethrough: a thin stroke through the run's middle, in the text's own
    // color (stroke `RG`, leaving the text fill `rg` untouched).
    if seg.strike && seg.width > 0.0 {
        let (r, g, b) = fill_rgb(seg.fill, palette);
        let sy = y + size * 0.30;
        let sw = (size * 0.06).max(0.4);
        append_rgb_stroke_line_operator(body, (r, g, b), sw, seg.x, sy, seg.x + seg.width);
    }
    if let Some(target) = &seg.link {
        let (r, g, b) = palette.link;
        let (fr, fg2, fb) = palette.fg;
        let uy = y - size * 0.12;
        let uw = (size * 0.06).max(0.4);
        append_rgb_stroke_line_operator(body, (r, g, b), uw, seg.x, uy, seg.x + seg.width);
        append_rgb_fill_operator(body, (fr, fg2, fb));
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

    // Code fences are visually framed blocks. Splitting a short code block is
    // especially bad for ASCII diagrams, whose geometry depends on neighboring
    // rows staying together. If a block is too tall for a page this penalty is
    // still paid by every internal candidate, so the page builder can split it.
    if before.flow.group == after.flow.group
        && before.flow.kind == FlowKind::Code
        && after.flow.kind == FlowKind::Code
    {
        penalty += 750_000.0;
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
    fn code_blocks_resist_internal_page_breaks() {
        let lines = [line(FlowKind::Code, 7, 0, 3), line(FlowKind::Code, 7, 1, 3)];
        assert!(
            break_penalty(&lines, 1) >= 750_000.0,
            "code and ASCII-diagram blocks should move as a unit when possible"
        );
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
    for (x, top, bot) in acc.values() {
        append_rgb_stroke_segment_operator(content, bar, 2.5, *x, *top, *x, *bot);
    }
    acc.clear();
}

/// Append a rounded-rectangle fill, color-isolated with `q`/`Q` so the fill
/// color never leaks into following text. Built from 4 lines + 4 cubic Beziers
/// (kappa = 0.5523). Degenerate rectangles append nothing.
fn append_rounded_rect_fill(
    out: &mut String,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: f32,
    c: (f32, f32, f32),
) {
    let x0 = finite_pdf_scalar(x0);
    let y0 = finite_pdf_scalar(y0);
    let x1 = finite_pdf_scalar(x1);
    let y1 = finite_pdf_scalar(y1);
    let r = finite_pdf_scalar(r);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let r = r.min((x1 - x0) * 0.5).min((y1 - y0) * 0.5).max(0.0);
    let k = r * 0.5523; // circle -> bezier magic constant
    let (rc, gc, bc) = c;

    out.push_str("q ");
    append_pdf_fixed3(out, rc);
    out.push(' ');
    append_pdf_fixed3(out, gc);
    out.push(' ');
    append_pdf_fixed3(out, bc);
    out.push_str(" rg ");
    append_pdf_fixed2(out, x0 + r);
    out.push(' ');
    append_pdf_fixed2(out, y0);
    out.push_str(" m ");
    append_pdf_fixed2(out, x1 - r);
    out.push(' ');
    append_pdf_fixed2(out, y0);
    out.push_str(" l ");
    append_pdf_fixed2(out, x1 - r + k);
    out.push(' ');
    append_pdf_fixed2(out, y0);
    out.push(' ');
    append_pdf_fixed2(out, x1);
    out.push(' ');
    append_pdf_fixed2(out, y0 + r - k);
    out.push(' ');
    append_pdf_fixed2(out, x1);
    out.push(' ');
    append_pdf_fixed2(out, y0 + r);
    out.push_str(" c ");
    append_pdf_fixed2(out, x1);
    out.push(' ');
    append_pdf_fixed2(out, y1 - r);
    out.push_str(" l ");
    append_pdf_fixed2(out, x1);
    out.push(' ');
    append_pdf_fixed2(out, y1 - r + k);
    out.push(' ');
    append_pdf_fixed2(out, x1 - r + k);
    out.push(' ');
    append_pdf_fixed2(out, y1);
    out.push(' ');
    append_pdf_fixed2(out, x1 - r);
    out.push(' ');
    append_pdf_fixed2(out, y1);
    out.push_str(" c ");
    append_pdf_fixed2(out, x0 + r);
    out.push(' ');
    append_pdf_fixed2(out, y1);
    out.push_str(" l ");
    append_pdf_fixed2(out, x0 + r - k);
    out.push(' ');
    append_pdf_fixed2(out, y1);
    out.push(' ');
    append_pdf_fixed2(out, x0);
    out.push(' ');
    append_pdf_fixed2(out, y1 - r + k);
    out.push(' ');
    append_pdf_fixed2(out, x0);
    out.push(' ');
    append_pdf_fixed2(out, y1 - r);
    out.push_str(" c ");
    append_pdf_fixed2(out, x0);
    out.push(' ');
    append_pdf_fixed2(out, y0 + r);
    out.push_str(" l ");
    append_pdf_fixed2(out, x0);
    out.push(' ');
    append_pdf_fixed2(out, y0 + r - k);
    out.push(' ');
    append_pdf_fixed2(out, x0 + r - k);
    out.push(' ');
    append_pdf_fixed2(out, y0);
    out.push(' ');
    append_pdf_fixed2(out, x0 + r);
    out.push(' ');
    append_pdf_fixed2(out, y0);
    out.push_str(" c f Q\n");
}

#[cfg(test)]
fn rounded_rect_fill(x0: f32, y0: f32, x1: f32, y1: f32, r: f32, c: (f32, f32, f32)) -> String {
    let mut out = String::new();
    append_rounded_rect_fill(&mut out, x0, y0, x1, y1, r, c);
    out
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

struct EmbeddedFaceLookup {
    by_slot: [Option<usize>; SLOTS.len()],
}

impl EmbeddedFaceLookup {
    fn new(faces: &[EmbeddedFace]) -> Self {
        let mut by_slot = [None; SLOTS.len()];
        for (index, face) in faces.iter().enumerate() {
            if let Some(slot_index) = pdf_font_slot_index(face.slot) {
                by_slot[slot_index] = Some(index);
            }
        }
        Self { by_slot }
    }

    fn get<'a>(&self, faces: &'a [EmbeddedFace], slot: u8) -> Option<&'a EmbeddedFace> {
        let slot_index = pdf_font_slot_index(slot)?;
        let face_index = self.by_slot[slot_index]?;
        faces.get(face_index)
    }
}

/// Shaped source glyph stream for one exact segment text in one font slot.
struct ShapedRun {
    glyphs: Vec<u16>,
    ligatures: Vec<(u16, String)>,
}

type ShapedRunCache = BTreeMap<u8, BTreeMap<String, ShapedRun>>;

struct PdfStream<'a> {
    bytes: Cow<'a, [u8]>,
    decoded_len: usize,
    flate_decode: bool,
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
    let shading_resource_bytes = pages
        .iter()
        .map(|page| page.shadings.len().saturating_mul(192))
        .sum::<usize>();

    page_stream_bytes
        .saturating_add(font_program_bytes)
        .saturating_add(font_aux_bytes)
        .saturating_add(image_bytes)
        .saturating_add(shading_resource_bytes)
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

fn append_pdf_stream_dict(out: &mut Vec<u8>, stream: &PdfStream<'_>) {
    out.extend_from_slice(b"<< /Length ");
    append_decimal_usize(out, stream.bytes.len());
    if stream.flate_decode {
        out.extend_from_slice(b" /Filter /FlateDecode /DL ");
        append_decimal_usize(out, stream.decoded_len);
    }
    out.extend_from_slice(b" >>");
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

fn append_decimal_usize_string(out: &mut String, value: usize) {
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

fn append_pdf_fixed(out: &mut String, value: f32, scale: u64) {
    let finite = finite_pdf_scalar(value);
    let scaled_float = f64::from(finite) * scale as f64;
    let frac = scaled_float.abs().fract();
    if (frac - 0.5).abs() <= f64::EPSILON * 4096.0 {
        match scale {
            100 => {
                let _ = write!(out, "{finite:.2}");
                return;
            }
            1000 => {
                let _ = write!(out, "{finite:.3}");
                return;
            }
            _ => {}
        }
    }
    let scaled = scaled_float.round() as i64;
    if scaled < 0 || (scaled == 0 && finite.is_sign_negative()) {
        out.push('-');
    }
    let abs = scaled.unsigned_abs();
    append_decimal_u64_string(out, abs / scale);
    out.push('.');
    let frac = abs % scale;
    let mut divisor = scale / 10;
    while divisor > 0 {
        out.push((b'0' + ((frac / divisor) % 10) as u8) as char);
        divisor /= 10;
    }
}

fn append_pdf_fixed2(out: &mut String, value: f32) {
    append_pdf_fixed(out, value, 100);
}

fn append_pdf_fixed3(out: &mut String, value: f32) {
    append_pdf_fixed(out, value, 1000);
}

fn append_marked_content_begin(out: &mut String, tag: &str, mcid: usize) {
    out.push('/');
    out.push_str(tag);
    out.push_str(" <</MCID ");
    append_decimal_usize_string(out, mcid);
    out.push_str(">> BDC\n");
}

fn page_stream(stream: &str) -> PdfStream<'_> {
    let raw = stream.as_bytes();
    if raw.len() < PAGE_STREAM_COMPRESSION_MIN {
        return PdfStream {
            bytes: Cow::Borrowed(raw),
            decoded_len: raw.len(),
            flate_decode: false,
        };
    }

    let compressed = crate::compress::zlib_compress(raw);
    if compressed.len() + 32 < raw.len() {
        PdfStream {
            bytes: Cow::Owned(compressed),
            decoded_len: raw.len(),
            flate_decode: true,
        }
    } else {
        PdfStream {
            bytes: Cow::Borrowed(raw),
            decoded_len: raw.len(),
            flate_decode: false,
        }
    }
}

fn svg_alpha_extgstate_resource(stream: &str) -> String {
    let states = collect_svg_alpha_states(stream.as_bytes());
    if states.is_empty() {
        return String::new();
    }
    let entries = states
        .into_iter()
        .map(|(fill_alpha, stroke_alpha)| {
            format!(
                "/GSa{fill_alpha:04}{stroke_alpha:04} << /ca {ca} /CA {stroke_ca} >>",
                ca = pdf_fixed3(f32::from(fill_alpha) / 1000.0),
                stroke_ca = pdf_fixed3(f32::from(stroke_alpha) / 1000.0),
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(" /ExtGState << {entries} >>")
}

fn pdf_shading_resource(shadings: &[PdfShading]) -> String {
    if shadings.is_empty() {
        return String::new();
    }
    let entries = shadings
        .iter()
        .take(256)
        .enumerate()
        .map(|(index, shading)| {
            let function = pdf_shading_function(&shading.stops);
            let extend_start = if shading.extend_start {
                "true"
            } else {
                "false"
            };
            let extend_end = if shading.extend_end { "true" } else { "false" };
            match shading.kind {
                PdfShadingKind::Axial(coords) => format!(
                    "/{} << /ShadingType 2 /ColorSpace /DeviceRGB /Coords [{} {} {} {}] \
                     /Function {function} /Extend [{extend_start} {extend_end}] >>",
                    pdf_shading_name(index),
                    pdf_num(coords[0]),
                    pdf_num(coords[1]),
                    pdf_num(coords[2]),
                    pdf_num(coords[3]),
                ),
                PdfShadingKind::Radial(coords) => format!(
                    "/{} << /ShadingType 3 /ColorSpace /DeviceRGB /Coords [{} {} {} {} {} {}] \
                     /Function {function} /Extend [{extend_start} {extend_end}] >>",
                    pdf_shading_name(index),
                    pdf_num(coords[0]),
                    pdf_num(coords[1]),
                    pdf_num(coords[2]),
                    pdf_num(coords[3]),
                    pdf_num(coords[4]),
                    pdf_num(coords[5]),
                ),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!(" /Shading << {entries} >>")
}

fn pdf_shading_function(stops: &[SvgGradientStop]) -> String {
    if stops.len() <= 2 {
        let start = stops.first().map(|stop| stop.1).unwrap_or((0.0, 0.0, 0.0));
        let end = stops.last().map(|stop| stop.1).unwrap_or(start);
        return pdf_exponential_interpolation_function(start, end);
    }

    let mut functions = String::new();
    for pair in stops.windows(2) {
        if !functions.is_empty() {
            functions.push(' ');
        }
        functions.push_str(&pdf_exponential_interpolation_function(
            pair[0].1, pair[1].1,
        ));
    }

    let bounds = stops[1..stops.len() - 1]
        .iter()
        .map(|(offset, _)| pdf_num(*offset))
        .collect::<Vec<_>>()
        .join(" ");
    let encode = (0..stops.len() - 1)
        .map(|_| "0 1")
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "<< /FunctionType 3 /Domain [0 1] /Functions [ {functions} ] /Bounds [ {bounds} ] /Encode [ {encode} ] >>"
    )
}

fn pdf_exponential_interpolation_function(start: (f32, f32, f32), end: (f32, f32, f32)) -> String {
    format!(
        "<< /FunctionType 2 /Domain [0 1] /C0 [{} {} {}] /C1 [{} {} {}] /N 1 >>",
        pdf_fixed3(start.0),
        pdf_fixed3(start.1),
        pdf_fixed3(start.2),
        pdf_fixed3(end.0),
        pdf_fixed3(end.1),
        pdf_fixed3(end.2),
    )
}

fn collect_svg_alpha_states(stream: &[u8]) -> BTreeSet<(u16, u16)> {
    let mut states = BTreeSet::new();
    let mut pos = 0usize;
    while let Some(rel) = stream
        .get(pos..)
        .and_then(|tail| tail.windows(4).position(|window| window == b"/GSa"))
    {
        let start = pos + rel;
        if let Some(state) = parse_svg_alpha_state_name(stream, start) {
            states.insert(state);
            if states.len() >= 256 {
                break;
            }
        }
        pos = start + 4;
    }
    states
}

fn parse_svg_alpha_state_name(stream: &[u8], start: usize) -> Option<(u16, u16)> {
    let name = stream.get(start..start + 12)?;
    if !name.starts_with(b"/GSa") || !name[4..12].iter().all(u8::is_ascii_digit) {
        return None;
    }
    if stream
        .get(start + 12)
        .is_some_and(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let fill_alpha = parse_four_ascii_digits(&name[4..8])?;
    let stroke_alpha = parse_four_ascii_digits(&name[8..12])?;
    (fill_alpha <= 1000 && stroke_alpha <= 1000).then_some((fill_alpha, stroke_alpha))
}

fn parse_four_ascii_digits(bytes: &[u8]) -> Option<u16> {
    (bytes.len() == 4 && bytes.iter().all(u8::is_ascii_digit)).then(|| {
        bytes.iter().fold(0u16, |acc, byte| {
            acc * 10 + u16::from(byte.saturating_sub(b'0'))
        })
    })
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
        let ext_gstate_res = svg_alpha_extgstate_resource(&pages[i].stream);
        let shading_res = pdf_shading_resource(&pages[i].shadings);
        append_pdf_object_str(
            &mut buf,
            &mut offsets,
            page_obj(i),
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {media_w} {media_h}] \
                 /Resources << /Font << {font_res} >>{image_res}{ext_gstate_res}{shading_res} >> /Contents {c} 0 R{annots}{struct_parent} >>",
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
        append_pdf_object_header(&mut buf, content_obj(i));
        append_pdf_stream_dict(&mut buf, &stream);
        buf.extend_from_slice(b"\nstream\n");
        buf.extend_from_slice(stream.bytes.as_ref());
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
                    "<< /Title {title} /Parent {parent} 0 R{prev}{next} \
                     /Dest [{page} 0 R /XYZ null {y} null] >>",
                    title = pdf_text_string(&outline.title),
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
                .map(|alt| format!(" /Alt {}", pdf_text_string(alt)))
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
        format!(" /Title {}", pdf_text_string(&title))
    };
    let author_entry = if author.is_empty() {
        String::new()
    } else {
        format!(" /Author {}", pdf_text_string(&author))
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
    let file_id = pdf_file_id(&buf, size, info_obj, xref_pos);
    buf.extend_from_slice(b"trailer\n<< /Size ");
    append_decimal_usize(&mut buf, size);
    buf.extend_from_slice(b" /Root 1 0 R /Info ");
    append_decimal_usize(&mut buf, info_obj);
    buf.extend_from_slice(b" 0 R /ID [");
    append_pdf_hex_string(&mut buf, &file_id);
    buf.push(b' ');
    append_pdf_hex_string(&mut buf, &file_id);
    buf.extend_from_slice(b"] >>\nstartxref\n");
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

fn pdf_file_id(pre_trailer: &[u8], size: usize, info_obj: usize, xref_pos: usize) -> [u8; 16] {
    let mut first = FNV_OFFSET;
    fnv1a64_update(&mut first, b"franken_markdown/pdf-id/v1/first");
    fnv1a64_update_usize(&mut first, size);
    fnv1a64_update_usize(&mut first, info_obj);
    fnv1a64_update_usize(&mut first, xref_pos);
    fnv1a64_update(&mut first, pre_trailer);

    let mut second = FNV_OFFSET ^ 0x9e37_79b9_7f4a_7c15;
    fnv1a64_update(&mut second, b"franken_markdown/pdf-id/v1/second");
    fnv1a64_update_u64(&mut second, first);
    fnv1a64_update_usize(&mut second, xref_pos);
    fnv1a64_update_usize(&mut second, info_obj);
    fnv1a64_update_usize(&mut second, size);
    fnv1a64_update(&mut second, pre_trailer);

    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&first.to_be_bytes());
    id[8..].copy_from_slice(&second.to_be_bytes());
    id
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a64_update(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn fnv1a64_update_u64(hash: &mut u64, value: u64) {
    fnv1a64_update(hash, &value.to_be_bytes());
}

fn fnv1a64_update_usize(hash: &mut u64, value: usize) {
    fnv1a64_update_u64(hash, value as u64);
}

fn append_pdf_hex_string(out: &mut Vec<u8>, bytes: &[u8]) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push(b'<');
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize]);
        out.push(HEX[(byte & 0x0F) as usize]);
    }
    out.push(b'>');
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
             /A << /S /URI /URI {uri} >>{sp} >>",
            uri = pdf_text_string(uri),
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

fn collect_shaped_run_glyphs(
    shaped: &ShapedRun,
    shaped_glyphs: &mut BTreeSet<u16>,
    lig_src_uni: &mut BTreeMap<u16, String>,
) {
    shaped_glyphs.extend(shaped.glyphs.iter().copied());
    for (g, s) in &shaped.ligatures {
        lig_src_uni.entry(*g).or_insert_with(|| s.clone());
    }
}

/// Build a `TJ` array (without the trailing `TJ`) from a pre-shaped SOURCE glyph
/// sequence: each glyph is emitted as its subset id via `map`, with GPOS pair
/// kerning (looked up on the original ids) inserted between glyphs.
#[cfg(test)]
fn kerned_tj(map: &BTreeMap<u16, u16>, source: &Font, kern: &Kerning, shaped: &[u16]) -> String {
    kerned_tj_with_spacing(map, source, kern, shaped, 0)
}

fn kerned_tj_with_spacing(
    map: &BTreeMap<u16, u16>,
    source: &Font,
    kern: &Kerning,
    shaped: &[u16],
    spacing_adjust: i32,
) -> String {
    let mut out = String::with_capacity(shaped.len().saturating_mul(4).saturating_add(4));
    append_kerned_tj_with_spacing(&mut out, map, source, kern, shaped, spacing_adjust);
    out
}

fn append_kerned_tj_with_spacing(
    out: &mut String,
    map: &BTreeMap<u16, u16>,
    source: &Font,
    kern: &Kerning,
    shaped: &[u16],
    spacing_adjust: i32,
) {
    let upm = i32::from(source.units_per_em.max(1));
    out.push_str("[<");
    for (i, &g) in shaped.iter().enumerate() {
        append_hex_u16(out, map.get(&g).copied().unwrap_or(0));
        if let Some(&next) = shaped.get(i + 1) {
            let k = kern.pair(g, next);
            let kern_adjust = if k != 0 {
                // A TJ number shifts the next glyph left by number/1000 em, so a
                // tightening (negative) kern becomes a positive number.
                -(i32::from(k) * 1000 / upm)
            } else {
                0
            };
            let adj = kern_adjust.saturating_add(spacing_adjust);
            if adj != 0 {
                out.push('>');
                append_i32_string(out, adj);
                out.push('<');
            }
        }
    }
    out.push_str(">]");
}

#[allow(clippy::too_many_arguments)]
fn append_text_segment_operator(
    body: &mut String,
    slot: u8,
    size: f32,
    x: f32,
    y: f32,
    map: &BTreeMap<u16, u16>,
    source: &Font,
    kern: &Kerning,
    shaped: &[u16],
) {
    body.push_str("BT /F");
    append_decimal_u64_string(body, u64::from(slot));
    body.push(' ');
    append_pdf_fixed2(body, size);
    body.push_str(" Tf 1 0 0 1 ");
    append_pdf_fixed2(body, x);
    body.push(' ');
    append_pdf_fixed2(body, y);
    body.push_str(" Tm ");
    append_kerned_tj_with_spacing(body, map, source, kern, shaped, 0);
    body.push_str(" TJ ET\n");
}

fn append_rgb_components_fixed3(out: &mut String, color: (f32, f32, f32)) {
    append_pdf_fixed3(out, color.0);
    out.push(' ');
    append_pdf_fixed3(out, color.1);
    out.push(' ');
    append_pdf_fixed3(out, color.2);
}

fn append_rgb_fill_operator(out: &mut String, color: (f32, f32, f32)) {
    append_rgb_components_fixed3(out, color);
    out.push_str(" rg\n");
}

fn append_rgb_fill_space_operator(out: &mut String, color: (f32, f32, f32)) {
    append_rgb_components_fixed3(out, color);
    out.push_str(" rg ");
}

fn append_rgb_stroke_space_operator(out: &mut String, color: (f32, f32, f32)) {
    append_rgb_components_fixed3(out, color);
    out.push_str(" RG ");
}

fn append_artifact_rule_stroke(
    out: &mut String,
    color: (f32, f32, f32),
    width: f32,
    x1: f32,
    y: f32,
    x2: f32,
) {
    out.push_str("/Artifact BMC\n");
    append_rgb_components_fixed3(out, color);
    out.push_str(" RG ");
    append_pdf_num(out, width);
    out.push_str(" w ");
    append_pdf_fixed2(out, x1);
    out.push(' ');
    append_pdf_fixed2(out, y);
    out.push_str(" m ");
    append_pdf_fixed2(out, x2);
    out.push(' ');
    append_pdf_fixed2(out, y);
    out.push_str(" l S\nEMC\n");
}

fn append_image_xobject_do(
    out: &mut String,
    image_index: usize,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
) {
    out.push_str("q ");
    append_pdf_num(out, width);
    out.push_str(" 0 0 ");
    append_pdf_num(out, height);
    out.push(' ');
    append_pdf_num(out, x);
    out.push(' ');
    append_pdf_num(out, y);
    out.push_str(" cm /Im");
    append_decimal_usize_string(out, image_index + 1);
    out.push_str(" Do Q\n");
}

fn append_rgb_stroke_segment_operator(
    out: &mut String,
    color: (f32, f32, f32),
    width: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
) {
    append_rgb_components_fixed3(out, color);
    out.push_str(" RG ");
    append_pdf_fixed2(out, width);
    out.push_str(" w ");
    append_pdf_fixed2(out, x1);
    out.push(' ');
    append_pdf_fixed2(out, y1);
    out.push_str(" m ");
    append_pdf_fixed2(out, x2);
    out.push(' ');
    append_pdf_fixed2(out, y2);
    out.push_str(" l S\n");
}

fn append_rgb_stroke_line_operator(
    out: &mut String,
    color: (f32, f32, f32),
    width: f32,
    x1: f32,
    y: f32,
    x2: f32,
) {
    append_rgb_stroke_segment_operator(out, color, width, x1, y, x2, y);
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

/// Encode `s` as a complete PDF string-object token, delimiters included.
///
/// Printable/ASCII text becomes a literal `(...)` string (the compact common
/// case, byte-identical to the historical output). Any string containing a
/// non-ASCII scalar becomes a UTF-16BE hex string `<FEFF...>` with a leading
/// byte-order mark. This matters because a PDF literal `(...)` string is decoded
/// as PDFDocEncoding, so pushing raw UTF-8 bytes for U+0080..=U+00FF produced
/// mojibake (`é` → `Ã©`) and U+0100.. was silently replaced with `?`. UTF-16BE is
/// the portable way to carry Unicode in document metadata, outline titles,
/// `/Alt` accessibility text, and `/URI` values. The returned value INCLUDES the
/// delimiters, so callers must not add their own.
fn pdf_text_string(s: &str) -> String {
    if s.is_ascii() {
        let mut o = String::with_capacity(s.len() + 2);
        o.push('(');
        append_pdf_string_escaped(&mut o, s);
        o.push(')');
        o
    } else {
        // UTF-16BE hex string: a `FEFF` BOM then two big-endian bytes per code
        // unit (surrogate pairs for astral scalars, which `encode_utf16` yields).
        let mut o = String::with_capacity(s.len() * 2 + 6);
        o.push_str("<FEFF");
        for unit in s.encode_utf16() {
            let _ = write!(o, "{unit:04X}");
        }
        o.push('>');
        o
    }
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

fn finite_pdf_scalar(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn pdf_fixed2(value: f32) -> String {
    format!("{:.2}", finite_pdf_scalar(value))
}

fn pdf_fixed3(value: f32) -> String {
    format!("{:.3}", finite_pdf_scalar(value))
}

fn text_width(s: &str, size: f32, font: u8, faces: &Faces) -> f32 {
    faces.shaped_width_points(font, s, size)
}

fn text_width_cached(
    s: &str,
    size: f32,
    font: u8,
    faces: &Faces,
    width_cache: &RefCell<WidthCache>,
) -> f32 {
    cached_shaped_width(faces, width_cache, font, s, font_size_of(size)).to_points_f32()
}

fn char_width(ch: char, size: f32, font: u8, faces: &Faces) -> f32 {
    faces.advance(font, ch) * size / 1000.0
}

#[cfg(test)]
mod pdf_writer_tests {
    use super::{
        F_BODY, F_BOLD, Faces, ParagraphPolicy, PdfStream, SvgDashPattern, SvgLineCap, SvgLineJoin,
        SvgShadow, SvgStyle, Tok, append_artifact_rule_stroke, append_decimal_u64_string,
        append_decimal_usize, append_decimal_usize_string, append_hex_u16, append_i32_string,
        append_image_xobject_do, append_marked_content_begin, append_pdf_fixed2, append_pdf_fixed3,
        append_pdf_num, append_pdf_object_str, append_pdf_stream_dict, append_pdf_string_escaped,
        append_rgb_fill_operator, append_rgb_fill_space_operator, append_rgb_stroke_line_operator,
        append_rgb_stroke_segment_operator, append_rgb_stroke_space_operator,
        append_svg_shadow_prefix, append_svg_stroke_options, append_svg_style,
        append_text_segment_operator, append_xref_in_use_row, append_xref_offset, build_paragraph,
        build_segs, decode_xml_entities, finite_pdf_scalar, font_size_of, kerned_tj, measure_word,
        normalize_svg_text_node, pdf_fixed2, pdf_fixed3, pdf_text_string, rounded_rect_fill,
        shape_run,
    };
    use std::borrow::Cow;

    #[test]
    fn pdf_word_measurement_uses_gpos_kerning() -> crate::Result<()> {
        let faces = Faces::load(&crate::PdfOptions::default())?;
        let fs = font_size_of(11.0);
        let tok = Tok {
            text: "AVATAR".to_string(),
            slot: F_BODY,
            space: false,
            hard_break: false,
            link: None,
            strike: false,
        };

        let shaped = measure_word(std::slice::from_ref(&tok), fs, &faces);
        let advance_only = crate::layout::measure_text(faces.get(F_BODY), &tok.text, fs);

        assert!(
            shaped < advance_only,
            "GPOS A/V-style pairs should tighten PDF layout measurement"
        );
        Ok(())
    }

    #[test]
    fn svg_text_node_normalization_preserves_boundary_space_only_for_text() {
        assert_eq!(normalize_svg_text_node("Alpha   "), "Alpha ");
        assert_eq!(normalize_svg_text_node("  Alpha   Beta "), " Alpha Beta ");
        assert_eq!(normalize_svg_text_node("\n  \t "), "");
    }

    #[test]
    fn svg_xml_entity_decoder_replaces_invalid_numeric_scalars_without_dropping_text() {
        let decoded = decode_xml_entities(
            "ok &#65; hex &#x41; zero &#0; surrogate &#xD800; huge &#99999999; upper &#X41;",
        );

        assert_eq!(
            decoded,
            "ok A hex A zero \u{FFFD} surrogate \u{FFFD} huge \u{FFFD} upper A"
        );
        assert!(
            !decoded.contains('\0'),
            "invalid SVG numeric XML references must not inject raw NUL bytes"
        );
    }

    #[test]
    fn svg_xml_entity_decoder_preserves_malformed_and_unknown_entities() {
        assert_eq!(
            decode_xml_entities("&; &#; &#x; &#xyz; &#12x; &not-real;"),
            "&; &#; &#x; &#xyz; &#12x; &not-real;"
        );
    }

    #[test]
    fn pdf_word_measurement_uses_gsub_ligature_advances() -> crate::Result<()> {
        let faces = Faces::load(&crate::PdfOptions::default())?;
        let face = faces.face(F_BODY);
        let fs = font_size_of(11.0);
        let tok = Tok {
            text: "fi".to_string(),
            slot: F_BODY,
            space: false,
            hard_break: false,
            link: None,
            strike: false,
        };

        let shaped = shape_run(&face.font, &face.lig, &tok.text);

        assert_eq!(
            shaped.glyphs.len(),
            1,
            "bundled body face should shape fi as one ligature glyph"
        );
        assert_eq!(
            measure_word(std::slice::from_ref(&tok), fs, &faces),
            crate::layout::advance_to_layout_units(face.glyph_advance_1000(shaped.glyphs[0]), fs),
            "PDF layout measurement should use the ligature glyph's own advance"
        );
        Ok(())
    }

    #[test]
    fn pdf_paragraph_builder_presizes_maps_from_token_count() -> crate::Result<()> {
        let faces = Faces::load(&crate::PdfOptions::default())?;
        let fs = font_size_of(11.0);
        let toks = vec![
            Tok {
                text: "alpha".to_string(),
                slot: F_BODY,
                space: false,
                hard_break: false,
                link: None,
                strike: false,
            },
            Tok {
                text: " ".to_string(),
                slot: F_BODY,
                space: true,
                hard_break: false,
                link: None,
                strike: false,
            },
            Tok {
                text: "beta".to_string(),
                slot: F_BODY,
                space: false,
                hard_break: false,
                link: None,
                strike: false,
            },
        ];
        let cache = std::cell::RefCell::new(std::collections::HashMap::new());
        let width_cache = std::cell::RefCell::new(std::collections::HashMap::new());

        let built = build_paragraph(
            &toks,
            fs,
            &faces,
            ParagraphPolicy::RAGGED,
            &cache,
            &width_cache,
        );
        let expected = toks.len().saturating_add(1);

        assert_eq!(built.items.len(), built.item_toks.len());
        assert_eq!(built.items.len(), built.break_toks.len());
        assert!(built.items.capacity() >= expected);
        assert!(built.item_toks.capacity() >= expected);
        assert!(built.break_toks.capacity() >= expected);
        Ok(())
    }

    #[test]
    fn pdf_segments_remeasure_merged_text_after_cross_token_shaping() -> crate::Result<()> {
        let faces = Faces::load(&crate::PdfOptions::default())?;
        let size = 11.0;
        let toks = vec![
            Tok {
                text: "f".to_string(),
                slot: F_BODY,
                space: false,
                hard_break: false,
                link: None,
                strike: false,
            },
            Tok {
                text: "i".to_string(),
                slot: F_BODY,
                space: false,
                hard_break: false,
                link: None,
                strike: false,
            },
            Tok {
                text: "X".to_string(),
                slot: F_BOLD,
                space: false,
                hard_break: false,
                link: None,
                strike: false,
            },
        ];

        let combined = faces.shaped_width_points(F_BODY, "fi", size);
        let separate = faces.shaped_width_points(F_BODY, "f", size)
            + faces.shaped_width_points(F_BODY, "i", size);
        assert!(
            (separate - combined).abs() > 0.01,
            "fixture must expose a cross-token ligature width difference"
        );

        let segs = build_segs(&toks, 10.0, size, &faces);
        assert_eq!(segs.len(), 2, "body run plus bold run");
        assert!(
            (segs[0].width - combined).abs() < 0.01,
            "merged segment width should match the shaped combined text"
        );
        assert!(
            (segs[1].x - (10.0 + combined)).abs() < 0.01,
            "following segment should start after the shaped combined width"
        );
        Ok(())
    }

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
    fn stream_dict_writer_preserves_raw_and_flate_shapes() {
        let raw = PdfStream {
            bytes: Cow::Borrowed(b"abc"),
            decoded_len: 3,
            flate_decode: false,
        };
        let compressed = PdfStream {
            bytes: Cow::Borrowed(b"zzzz"),
            decoded_len: 123,
            flate_decode: true,
        };
        let mut out = Vec::new();
        append_pdf_stream_dict(&mut out, &raw);
        out.push(b'\n');
        append_pdf_stream_dict(&mut out, &compressed);

        assert_eq!(
            out,
            b"<< /Length 3 >>\n<< /Length 4 /Filter /FlateDecode /DL 123 >>"
        );
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
    fn marked_content_begin_writer_preserves_bdc_shape() {
        let mut out = String::new();
        append_marked_content_begin(&mut out, "TH", 0);
        append_marked_content_begin(&mut out, "Figure", 12345);
        append_decimal_usize_string(&mut out, 987);

        assert_eq!(out, "/TH <</MCID 0>> BDC\n/Figure <</MCID 12345>> BDC\n987");
    }

    #[test]
    fn streamed_text_segment_operator_matches_legacy_format_shape() -> crate::Result<()> {
        let faces = Faces::load(&crate::PdfOptions::default())?;
        let face = faces.face(F_BODY);
        let shaped = shape_run(&face.font, &face.lig, "AVATAR");
        let map = shaped
            .glyphs
            .iter()
            .copied()
            .map(|glyph| (glyph, glyph))
            .collect::<std::collections::BTreeMap<_, _>>();

        let mut streamed = String::new();
        append_text_segment_operator(
            &mut streamed,
            F_BODY,
            11.0,
            12.5,
            700.25,
            &map,
            &face.font,
            &face.kern,
            &shaped.glyphs,
        );

        let expected = format!(
            "BT /F{f} {s} Tf 1 0 0 1 {x} {y} Tm {tj} TJ ET\n",
            f = F_BODY,
            s = pdf_fixed2(11.0),
            x = pdf_fixed2(12.5),
            y = pdf_fixed2(700.25),
            tj = kerned_tj(&map, &face.font, &face.kern, &shaped.glyphs),
        );
        assert_eq!(streamed, expected);
        Ok(())
    }

    #[test]
    fn streamed_rgb_fill_operator_matches_legacy_format_shape() {
        let mut out = String::new();
        append_rgb_fill_operator(&mut out, (0.1, 0.25, 1.0));

        assert_eq!(out, "0.100 0.250 1.000 rg\n");
    }

    #[test]
    fn streamed_rgb_stroke_line_operator_matches_legacy_format_shape() {
        let mut out = String::new();
        append_rgb_stroke_line_operator(&mut out, (0.1, 0.25, 1.0), 0.66, 12.5, 700.25, 42.0);

        assert_eq!(
            out,
            "0.100 0.250 1.000 RG 0.66 w 12.50 700.25 m 42.00 700.25 l S\n"
        );
    }

    #[test]
    fn page_content_operator_writers_preserve_legacy_format_shape() {
        let mut out = String::new();
        append_artifact_rule_stroke(&mut out, (0.1, 0.25, 1.0), 0.7, 12.5, 700.25, 42.0);
        append_image_xobject_do(&mut out, 12, 80.0, 60.5, 24.25, -3.0);
        append_rgb_stroke_segment_operator(&mut out, (1.0, 0.5, 0.0), 2.5, 11.0, 22.25, 11.0, 5.5);

        assert_eq!(
            out,
            "/Artifact BMC\n0.100 0.250 1.000 RG 0.7 w 12.50 700.25 m 42.00 700.25 l S\nEMC\nq 80 0 0 60.5 24.25 -3 cm /Im13 Do Q\n1.000 0.500 0.000 RG 2.50 w 11.00 22.25 m 11.00 5.50 l S\n"
        );
    }

    #[test]
    fn svg_style_operator_writers_preserve_legacy_format_shape() {
        let mut style = SvgStyle {
            fill: Some((1.0, 0.5, 0.0)),
            stroke: Some((0.25, 0.75, 0.5)),
            stroke_width: 2.5,
            line_cap: SvgLineCap::Round,
            line_join: SvgLineJoin::Miter,
            miter_limit: Some(3.25),
            dash: SvgDashPattern {
                values: [
                    3.0, 1.5, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                ],
                len: 3,
                offset: 0.75,
            },
            ..SvgStyle::INITIAL
        };

        let mut out = String::new();
        append_svg_style(&mut out, style);
        assert_eq!(
            out,
            "1.000 0.500 0.000 rg 0.250 0.750 0.500 RG 2.5 w 1 J 0 j 3.25 M [3 1.5 2 3 1.5 2] 0.75 d "
        );

        let mut colors = String::new();
        append_rgb_fill_space_operator(&mut colors, (0.1, 0.2, 0.3));
        append_rgb_stroke_space_operator(&mut colors, (0.4, 0.5, 0.6));
        assert_eq!(colors, "0.100 0.200 0.300 rg 0.400 0.500 0.600 RG ");

        style.shadow = Some(SvgShadow {
            dx: 2.0,
            dy: -1.25,
            color: (0.1, 0.2, 0.3),
            opacity: 1.0,
        });
        style.fill = None;
        let mut shadow = String::new();
        assert!(append_svg_shadow_prefix(&mut shadow, style));
        assert_eq!(
            shadow,
            "q 1 0 0 1 2 -1.25 cm 0.100 0.200 0.300 RG 2.5 w 1 J 0 j 3.25 M [3 1.5 2 3 1.5 2] 0.75 d "
        );

        let mut stroke_options = String::new();
        append_svg_stroke_options(
            &mut stroke_options,
            SvgStyle {
                stroke_width: 0.01,
                line_cap: SvgLineCap::Square,
                line_join: SvgLineJoin::Bevel,
                miter_limit: Some(9.0),
                dash: SvgDashPattern::NONE,
                ..SvgStyle::INITIAL
            },
        );
        assert_eq!(stroke_options, "0.1 w 2 J 2 j ");
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
    fn fixed_width_pdf_token_writers_preserve_finite_format_and_guard_non_finite() {
        assert_eq!(pdf_fixed2(5.0), "5.00");
        assert_eq!(pdf_fixed2(1.235), "1.24");
        assert_eq!(pdf_fixed2(-0.0), "-0.00");
        assert_eq!(pdf_fixed3(0.5), "0.500");
        assert_eq!(pdf_fixed3(1.2345), "1.235");

        assert_eq!(pdf_fixed2(f32::NAN), "0.00");
        assert_eq!(pdf_fixed2(f32::INFINITY), "0.00");
        assert_eq!(pdf_fixed2(f32::NEG_INFINITY), "0.00");
        assert_eq!(pdf_fixed3(f32::NAN), "0.000");
        assert_eq!(pdf_fixed3(f32::INFINITY), "0.000");
        assert_eq!(pdf_fixed3(f32::NEG_INFINITY), "0.000");

        let mut fixed = String::new();
        append_pdf_fixed2(&mut fixed, 1.2);
        fixed.push(' ');
        append_pdf_fixed2(&mut fixed, -0.0);
        fixed.push(' ');
        append_pdf_fixed3(&mut fixed, 1.2345);
        fixed.push(' ');
        append_pdf_fixed3(&mut fixed, f32::NAN);
        assert_eq!(fixed, "1.20 -0.00 1.235 0.000");

        for base in -1000..=1000 {
            for offset in [-0.005f32, -0.0049, 0.0, 0.0049, 0.005, 0.0051] {
                let value = base as f32 * 0.5 + offset;
                let mut fixed2 = String::new();
                append_pdf_fixed2(&mut fixed2, value);
                assert_eq!(fixed2, format!("{:.2}", finite_pdf_scalar(value)));

                let mut fixed3 = String::new();
                append_pdf_fixed3(&mut fixed3, value);
                assert_eq!(fixed3, format!("{:.3}", finite_pdf_scalar(value)));
            }
        }
    }

    #[test]
    fn rounded_rect_fill_coerces_non_finite_before_degenerate_guard() {
        assert!(
            rounded_rect_fill(f32::NAN, 0.0, 10.0, 10.0, 0.0, (0.1, 0.2, 0.3))
                .contains("0.00 0.00 m"),
            "NaN x0 should be coerced to a finite PDF token"
        );
        assert_eq!(
            rounded_rect_fill(0.0, 0.0, f32::NAN, 10.0, 2.0, (0.1, 0.2, 0.3)),
            "",
            "NaN x1 should coerce to 0 and produce a degenerate rectangle"
        );
    }

    #[test]
    fn rounded_rect_fill_preserves_pdf_operator_sequence() {
        assert_eq!(
            rounded_rect_fill(0.0, 0.0, 10.0, 5.0, 0.0, (0.1, 0.2, 0.3)),
            concat!(
                "q 0.100 0.200 0.300 rg ",
                "0.00 0.00 m 10.00 0.00 l ",
                "10.00 0.00 10.00 0.00 10.00 0.00 c ",
                "10.00 5.00 l ",
                "10.00 5.00 10.00 5.00 10.00 5.00 c ",
                "0.00 5.00 l ",
                "0.00 5.00 0.00 5.00 0.00 5.00 c ",
                "0.00 0.00 l ",
                "0.00 0.00 0.00 0.00 0.00 0.00 c f Q\n"
            )
        );
    }

    #[test]
    fn pdf_literal_string_escape_writer_matches_existing_policy() {
        let mut out = String::new();
        append_pdf_string_escaped(&mut out, "a(b)c\\d\re\n\u{2206}");

        assert_eq!(out, "a\\(b\\)c\\\\d\\re ?");
    }

    #[test]
    fn pdf_text_string_keeps_ascii_literal_and_encodes_unicode_as_utf16be() {
        // Pure ASCII stays a literal `(...)` string, byte-identical to the old
        // output (so ASCII metadata goldens do not change), with the same escapes.
        assert_eq!(pdf_text_string("Plain Title"), "(Plain Title)");
        assert_eq!(pdf_text_string("a(b)c\\d"), "(a\\(b\\)c\\\\d)");
        assert_eq!(pdf_text_string(""), "()");

        // Any non-ASCII scalar switches to a UTF-16BE hex string with a BOM, so
        // U+0080..=U+00FF no longer mojibakes and U+0100.. is no longer dropped to
        // `?`. `é` = U+00E9, em dash = U+2014.
        assert_eq!(pdf_text_string("é"), "<FEFF00E9>");
        assert_eq!(pdf_text_string("A—B"), "<FEFF004120140042>");
        // Astral scalar (U+1F600) becomes a surrogate pair D83D DE00.
        assert_eq!(pdf_text_string("\u{1F600}"), "<FEFFD83DDE00>");
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
                "performance-plan",
                performance_plan_markdown(),
                612.0,
                792.0,
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

    fn performance_plan_markdown() -> String {
        r#"# Performance Acceleration Plan

| Candidate | Impact | Confidence | Effort | Score | First bead |
|:--|--:|--:|--:|--:|:--|
| PDF serializer/shaping fast path: buffer sizing, fast decimal/hex writers, shaped-run cache, subset-map layout | 5 | 4 | 2 | 10.0 | `fep.6` |
| Asupersync batch renderer: file-level parallelism, deterministic receipts, queueing budgets | 5 | 5 | 3 | 8.3 | `zmd.1` |
| Parser scanner attribution and allocation reduction | 4 | 4 | 2 | 8.0 | new child under gauntlet/parser |
| PDF stage instrumentation: split layout/subset/ToUnicode/serialize timings | 4 | 5 | 3 | 6.7 | new child under `fep.6` |
| Hyphen word-result cache or trie layout compaction | 2 | 4 | 2 | 4.0 | future child after profile |
| SIMD special-byte scanner island | 4 | 3 | 4 | 3.0 | `qw1.5` |
| Active-list/page-builder parallelism inside one document | 3 | 2 | 4 | 1.5 | defer |
| AVX-512-specific path | 2 | 2 | 5 | 0.8 | reject until separate hardware proof |

```powershell
irm "https://example.test/install.ps1" | iex
# installs fmd
```

```mermaid
flowchart LR
    Markdown[large markdown file] --> Parser[AST] --> Layout[measured table/code layout] --> PDF[compact tagged PDF]
    Layout --> HTML[self-contained HTML]
```
"#
        .to_string()
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

    fn render_tree_y_label(line: &str) -> Option<&str> {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix("y="))
    }

    #[test]
    fn performance_plan_render_tree_pins_user_visible_regressions() {
        let tree = render_tree_debug(&performance_plan_markdown(), &small_opts(612.0, 792.0));

        for (col, header) in [
            (1, "Impact"),
            (2, "Confidence"),
            (3, "Effort"),
            (4, "Score"),
        ] {
            let header_literal = format!("{header:?}");
            let count = tree
                .lines()
                .filter(|line| line.contains(&format!("TH col={col} ")))
                .filter(|line| line.contains(&header_literal))
                .count();
            assert_eq!(
                count, 1,
                "compact performance-plan header {header:?} should render as one table-header segment\n{tree}"
            );
        }

        let diagram_rows = tree
            .lines()
            .filter(|line| line.contains("Code "))
            .filter(|line| {
                line.contains("\"    Markdown\"")
                    || line.contains("\"large markdown file\"")
                    || line.contains("\" Parser\"")
                    || line.contains("\"measured table\"")
                    || line.contains("\"compact tagged PDF\"")
            })
            .filter_map(render_tree_y_label)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            diagram_rows.len(),
            1,
            "the long Mermaid diagram row should stay on one rendered code row even when syntax highlighting splits it into token segments\n{tree}"
        );

        let body_fill =
            render_tree_fill_label(Fill::Black, &Palette::from_colors(&Theme::default().colors));
        let syntax_lines = tree
            .lines()
            .filter(|line| line.contains("Code "))
            .filter(|line| !line.contains(&format!("fill={body_fill}")))
            .collect::<Vec<_>>();
        assert!(
            syntax_lines.iter().any(|line| line.contains("\"irm\"")),
            "PowerShell keyword highlighting should emit a non-body fill for irm\n{tree}"
        );
        assert!(
            syntax_lines.iter().any(|line| line.contains("\"iex\"")),
            "PowerShell keyword highlighting should emit a non-body fill for iex\n{tree}"
        );
        assert!(
            syntax_lines
                .iter()
                .any(|line| line.contains("\"# installs fmd\"")),
            "PowerShell comments should emit a non-body fill\n{tree}"
        );
        assert!(
            syntax_lines
                .iter()
                .any(|line| line.contains("\"flowchart\"")),
            "Mermaid diagram keywords should emit a non-body fill in PDF code blocks\n{tree}"
        );
        assert!(
            syntax_lines.iter().any(|line| line.contains("\"-->\"")),
            "Mermaid edge operators should emit a non-body fill in PDF code blocks\n{tree}"
        );
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
        DecodedPng, PNG_ADAM7, PdfImageColor, PngChunks, decode_png_full, parse_png_chunks,
        parse_png_image_asset, png_paeth, png_pass_count,
    };
    use crate::compress::zlib_compress;

    /// Forward PNG filter (the inverse of the decoder's unfilter) so a test can
    /// build scanlines that exercise every reconstruction branch.
    fn forward_filter(row: &[u8], prev: &[u8], bpp: usize, filter: u8) -> Vec<u8> {
        let mut out = vec![0u8; row.len()];
        for i in 0..row.len() {
            let a = if i >= bpp { row[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            let pred = match filter {
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => png_paeth(a, b, c),
                _ => 0,
            };
            out[i] = row[i].wrapping_sub(pred);
        }
        out
    }

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
    fn rgba16_downsamples_color_and_alpha_to_high_byte() {
        // 16-bit RGBA: 4 channels × 2 bytes, big-endian; keep the high byte of
        // each (covers the 16-bit alpha-extraction path).
        let rows = vec![vec![
            0x12, 0x00, 0x34, 0x00, 0x56, 0x00, 0xAB, 0x00, // px0 rgba hi=12,34,56 a=AB
            0x78, 0xFF, 0x9A, 0xFF, 0xBC, 0xFF, 0x10, 0x00, // px1 rgba hi=78,9A,BC a=10
        ]];
        let png = chunks(2, 1, 16, 6, 0, &rows, vec![], vec![]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.color, PdfImageColor::Rgb);
        assert_eq!(d.samples, vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        assert_eq!(d.alpha, Some(vec![0xAB, 0x10]));
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
    fn gray1_and_gray2_subbyte_unpack_and_scale() {
        // 1-bit grayscale: two pixels in the top two bits scale to 255 and 0.
        let g1 = chunks(2, 1, 1, 0, 0, &[vec![0b1000_0000]], vec![], vec![]);
        assert_eq!(decode_png_full(&g1).unwrap().samples, vec![255, 0]);
        // 2-bit grayscale: 0b11 -> 255, 0b00 -> 0.
        let g2 = chunks(2, 1, 2, 0, 0, &[vec![0b1100_0000]], vec![], vec![]);
        assert_eq!(decode_png_full(&g2).unwrap().samples, vec![255, 0]);
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
    fn unfilter_reconstructs_every_png_filter_type() {
        // A 2px-wide RGB image with one row per PNG filter (0=None, 1=Sub,
        // 2=Up, 3=Average, 4=Paeth). Width 2 ensures a left neighbor exists, so
        // every `a`/`b`/`c` branch in the unfilter runs.
        let rows: [[u8; 6]; 5] = [
            [10, 20, 30, 40, 50, 60],
            [11, 22, 33, 44, 55, 66],
            [12, 24, 36, 48, 60, 72],
            [13, 26, 39, 52, 65, 78],
            [14, 28, 42, 56, 70, 84],
        ];
        let bpp = 3;
        let mut idat_raw = Vec::new();
        let mut prev = [0u8; 6];
        for (r, row) in rows.iter().enumerate() {
            let filter = r as u8; // 0,1,2,3,4
            idat_raw.push(filter);
            idat_raw.extend_from_slice(&forward_filter(row, &prev, bpp, filter));
            prev = *row;
        }
        let png = PngChunks {
            width: 2,
            height: 5,
            bit_depth: 8,
            color_type: 2,
            interlace: 0,
            palette: vec![],
            trns: vec![],
            idat: zlib_compress(&idat_raw),
        };
        let d = decode_png_full(&png).unwrap();
        let expect: Vec<u8> = rows.iter().flatten().copied().collect();
        assert_eq!(
            d.samples, expect,
            "all filter types must reconstruct exactly"
        );
    }

    #[test]
    fn invalid_color_type_bit_depth_combos_are_rejected() {
        let png = |ct: u8, bd: u8| -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
            let mut ihdr = Vec::new();
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&[bd, ct, 0, 0, 0]);
            let push = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                out.extend_from_slice(kind);
                out.extend_from_slice(data);
                out.extend_from_slice(&0u32.to_be_bytes());
            };
            push(&mut bytes, b"IHDR", &ihdr);
            push(&mut bytes, b"IDAT", &[0]);
            push(&mut bytes, b"IEND", &[]);
            bytes
        };
        // Spec-invalid pairs are rejected outright rather than decoded to garbage.
        assert!(parse_png_chunks(&png(6, 4)).is_none(), "RGBA @ 4-bit");
        assert!(parse_png_chunks(&png(2, 1)).is_none(), "RGB @ 1-bit");
        assert!(parse_png_chunks(&png(4, 2)).is_none(), "gray+alpha @ 2-bit");
        assert!(parse_png_chunks(&png(3, 16)).is_none(), "palette @ 16-bit");
        // Valid pairs still parse (grayscale supports every depth).
        assert!(parse_png_chunks(&png(0, 4)).is_some(), "gray @ 4-bit");
        assert!(parse_png_chunks(&png(2, 8)).is_some(), "RGB @ 8-bit");
    }

    #[test]
    fn invalid_palette_and_trns_chunk_ordering_is_rejected() {
        let png = |ct: u8, bd: u8, parts: Vec<(&[u8; 4], Vec<u8>)>| -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
            let mut ihdr = Vec::new();
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&[bd, ct, 0, 0, 0]);
            let push = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                out.extend_from_slice(kind);
                out.extend_from_slice(data);
                out.extend_from_slice(&0u32.to_be_bytes());
            };
            push(&mut bytes, b"IHDR", &ihdr);
            for (kind, data) in parts {
                push(&mut bytes, kind, &data);
            }
            push(&mut bytes, b"IEND", &[]);
            bytes
        };
        let plte = vec![1, 2, 3];
        let idat = vec![0];

        assert!(
            parse_png_chunks(&png(
                3,
                8,
                vec![
                    (b"PLTE", plte.clone()),
                    (b"tRNS", vec![255]),
                    (b"tRNS", vec![0]),
                    (b"IDAT", idat.clone()),
                ],
            ))
            .is_none(),
            "duplicate tRNS must not overwrite the first transparency chunk"
        );
        assert!(
            parse_png_chunks(&png(
                3,
                8,
                vec![
                    (b"PLTE", plte.clone()),
                    (b"IDAT", idat.clone()),
                    (b"tRNS", vec![0]),
                ],
            ))
            .is_none(),
            "tRNS after IDAT is malformed"
        );
        assert!(
            parse_png_chunks(&png(
                3,
                8,
                vec![
                    (b"tRNS", vec![0]),
                    (b"PLTE", plte.clone()),
                    (b"IDAT", idat.clone()),
                ],
            ))
            .is_none(),
            "palette tRNS must follow PLTE"
        );
        assert!(
            parse_png_chunks(&png(
                3,
                8,
                vec![
                    (b"PLTE", plte.clone()),
                    (b"PLTE", plte.clone()),
                    (b"IDAT", idat.clone()),
                ],
            ))
            .is_none(),
            "duplicate PLTE chunks are malformed"
        );
        assert!(
            parse_png_chunks(&png(
                3,
                1,
                vec![(b"PLTE", vec![1, 2, 3, 4, 5, 6, 7, 8, 9]), (b"IDAT", idat)]
            ))
            .is_none(),
            "indexed-color PLTE must not exceed the bit-depth entry capacity"
        );
    }

    #[test]
    fn non_consecutive_idat_chunks_are_rejected() {
        let png = |parts: Vec<(&[u8; 4], Vec<u8>)>| -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
            let mut ihdr = Vec::new();
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
            let push = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                out.extend_from_slice(kind);
                out.extend_from_slice(data);
                out.extend_from_slice(&0u32.to_be_bytes());
            };
            push(&mut bytes, b"IHDR", &ihdr);
            for (kind, data) in parts {
                push(&mut bytes, kind, &data);
            }
            push(&mut bytes, b"IEND", &[]);
            bytes
        };

        assert!(
            parse_png_chunks(&png(vec![
                (b"IDAT", vec![0]),
                (b"IDAT", vec![1]),
                (b"tEXt", b"note\0ok".to_vec()),
            ]))
            .is_some(),
            "consecutive IDAT chunks followed by ancillary metadata are valid"
        );
        assert!(
            parse_png_chunks(&png(vec![
                (b"IDAT", vec![0]),
                (b"tEXt", b"note\0bad".to_vec()),
                (b"IDAT", vec![1]),
            ]))
            .is_none(),
            "IDAT chunks must be consecutive; ancillary chunks cannot split the stream"
        );
    }

    #[test]
    fn oversized_decoded_image_is_rejected_by_the_byte_cap() {
        // Build a PNG whose IHDR claims large dimensions but whose IDAT is tiny —
        // the bit-depth-aware decoded-bytes cap must refuse it in IHDR validation,
        // before any multi-hundred-MB decode buffer is allocated.
        let png = |w: u32, h: u32, bd: u8, ct: u8| -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
            let mut ihdr = Vec::new();
            ihdr.extend_from_slice(&w.to_be_bytes());
            ihdr.extend_from_slice(&h.to_be_bytes());
            ihdr.extend_from_slice(&[bd, ct, 0, 0, 0]);
            let push = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                out.extend_from_slice(kind);
                out.extend_from_slice(data);
                out.extend_from_slice(&0u32.to_be_bytes());
            };
            push(&mut bytes, b"IHDR", &ihdr);
            push(&mut bytes, b"IDAT", &[0]);
            push(&mut bytes, b"IEND", &[]);
            bytes
        };
        // 24 MP 16-bit RGBA = 192 MB decoded > 96 MB cap -> refused.
        assert!(
            parse_png_chunks(&png(6000, 4000, 16, 6)).is_none(),
            "24MP RGBA16 must be refused by the decoded-bytes cap"
        );
        // The SAME pixel count at 8-bit RGBA = ~91.5 MiB <= cap -> still accepted
        // (the cap is bit-depth-aware, not a blanket pixel-count reduction).
        assert!(
            parse_png_chunks(&png(6000, 4000, 8, 6)).is_some(),
            "24MP RGBA8 is within the byte cap"
        );
        // A 16-bit RGBA image small enough to fit the byte cap is accepted.
        assert!(
            parse_png_chunks(&png(2000, 2000, 16, 6)).is_some(),
            "4MP RGBA16 fits the byte cap"
        );
    }

    #[test]
    fn malformed_or_unsupported_pngs_are_rejected() {
        // Bad signature.
        assert!(parse_png_chunks(b"definitely not a png at all").is_none());
        // Truncated below the 8-byte signature.
        assert!(parse_png_chunks(b"\x89PNG").is_none());

        // Palette color type with no PLTE cannot be decoded.
        let no_plte = chunks(2, 1, 8, 3, 0, &[vec![0, 1]], vec![], vec![]);
        assert!(decode_png_full(&no_plte).is_none());

        // Palette at 16-bit depth is unsupported (indices are <= 8-bit).
        let pal16 = chunks(1, 1, 16, 3, 0, &[vec![0, 0]], vec![[1, 2, 3]], vec![]);
        assert!(decode_png_full(&pal16).is_none());

        // A corrupt IDAT (not a valid zlib stream) fails cleanly, no panic.
        let mut bad_idat = chunks(
            2,
            1,
            8,
            6,
            0,
            &[vec![10, 20, 30, 40, 50, 60, 70, 80]],
            vec![],
            vec![],
        );
        bad_idat.idat = vec![0xFF, 0xFF, 0xFF, 0xFF];
        assert!(decode_png_full(&bad_idat).is_none());
    }

    #[test]
    fn malformed_trns_chunks_are_rejected() {
        // Grayscale tRNS must be exactly one 16-bit sample.
        let gray_short = chunks(1, 1, 8, 0, 0, &[vec![10]], vec![], vec![10]);
        assert!(decode_png_full(&gray_short).is_none());

        // Truecolor tRNS must be exactly three 16-bit samples.
        let rgb_long = chunks(
            1,
            1,
            8,
            2,
            0,
            &[vec![10, 20, 30]],
            vec![],
            vec![0, 10, 0, 20, 0, 30, 0, 40],
        );
        assert!(decode_png_full(&rgb_long).is_none());

        // Color types that already carry alpha must not also carry tRNS.
        let rgba_with_trns = chunks(1, 1, 8, 6, 0, &[vec![10, 20, 30, 255]], vec![], vec![0, 10]);
        assert!(decode_png_full(&rgba_with_trns).is_none());

        // Palette tRNS may be shorter than PLTE, but never longer.
        let palette_too_long = chunks(1, 1, 8, 3, 0, &[vec![0]], vec![[1, 2, 3]], vec![255, 0]);
        assert!(decode_png_full(&palette_too_long).is_none());

        // Grayscale tRNS values are raw samples, not scaled 8-bit values.
        let gray1_out_of_range = chunks(1, 1, 1, 0, 0, &[vec![0]], vec![], vec![0, 2]);
        assert!(decode_png_full(&gray1_out_of_range).is_none());

        // 8-bit truecolor tRNS channels must fit in the low byte.
        let rgb8_out_of_range = chunks(
            1,
            1,
            8,
            2,
            0,
            &[vec![10, 20, 30]],
            vec![],
            vec![1, 10, 0, 20, 0, 30],
        );
        assert!(decode_png_full(&rgb8_out_of_range).is_none());
    }

    #[test]
    fn full_png_decode_rejects_extra_inflated_scanline_bytes() {
        // The full-decode path must consume exactly the image scanline payload.
        // Extra inflated bytes after the expected rows indicate a malformed PNG
        // datastream and must not be silently ignored.
        let mut plain = chunks(1, 1, 8, 6, 0, &[vec![10, 20, 30, 40]], vec![], vec![]);
        plain.idat = zlib_compress(&[0, 10, 20, 30, 40, 99]);
        assert!(decode_png_full(&plain).is_none());

        let mut interlaced = chunks(1, 1, 8, 6, 1, &[vec![10, 20, 30, 40]], vec![], vec![]);
        interlaced.idat = zlib_compress(&[0, 10, 20, 30, 40, 99]);
        assert!(decode_png_full(&interlaced).is_none());
    }

    #[test]
    fn gray_with_trns_marks_the_transparent_sample() {
        // 8-bit grayscale with a single transparent value (tRNS = 2 bytes; the
        // low byte is the transparent gray level). Pixel 0 (=200) is transparent.
        let rows = vec![vec![200, 100]];
        let png = chunks(2, 1, 8, 0, 0, &rows, vec![], vec![0, 200]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.color, PdfImageColor::Gray);
        assert_eq!(d.alpha, Some(vec![0, 255]));
    }

    #[test]
    fn gray16_with_trns_requires_exact_sample() {
        // 16-bit grayscale with a tRNS key must compare the full source sample,
        // not only the high byte that is kept for the 8-bit PDF image plane.
        let rows = vec![vec![0x12, 0xFF, 0x12, 0x34]];
        let png = chunks(2, 1, 16, 0, 0, &rows, vec![], vec![0x12, 0x34]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.samples, vec![0x12, 0x12]);
        assert_eq!(d.alpha, Some(vec![255, 0]));
    }

    #[test]
    fn rgb16_with_trns_requires_exact_color_key() {
        // 16-bit RGB with a color-key tRNS (3 × 16-bit big-endian). Pixel 1 has
        // matching high bytes but different low bytes, so it must remain opaque;
        // pixel 2 matches all 16 bits and is transparent.
        let rows = vec![vec![
            0x10, 0x00, 0x20, 0x00, 0x30, 0x00, // px0 hi = 10,20,30
            0x40, 0xFF, 0x50, 0xFF, 0x60, 0xFF, // px1 hi matches, low differs
            0x40, 0x00, 0x50, 0x00, 0x60, 0x00, // px2 exact match
        ]];
        let png = chunks(
            3,
            1,
            16,
            2,
            0,
            &rows,
            vec![],
            vec![0x40, 0x00, 0x50, 0x00, 0x60, 0x00],
        );
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.alpha, Some(vec![255, 255, 0]));
    }

    #[test]
    fn rgb_with_trns_marks_the_transparent_color() {
        // 8-bit RGB with a single transparent colour (tRNS = 6 bytes, hi/lo per
        // channel). The second pixel matches and is transparent.
        let rows = vec![vec![10, 20, 30, 40, 50, 60]];
        let png = chunks(2, 1, 8, 2, 0, &rows, vec![], vec![0, 40, 0, 50, 0, 60]);
        let d = decode_png_full(&png).unwrap();
        assert_eq!(d.alpha, Some(vec![255, 0]));
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

    #[test]
    fn fast_path_rejects_invalid_predictor_payloads() {
        // The predictor fast path passes IDAT through to the PDF. It must still
        // validate the zlib stream and PNG row layout, otherwise a corrupt image
        // asset is accepted and produces a broken PDF image XObject.
        let png_with_idat = |idat: &[u8]| {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
            let mut ihdr = Vec::new();
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&1u32.to_be_bytes());
            ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
            let push_chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
                out.extend_from_slice(&(data.len() as u32).to_be_bytes());
                out.extend_from_slice(kind);
                out.extend_from_slice(data);
                out.extend_from_slice(&0u32.to_be_bytes());
            };
            push_chunk(&mut bytes, b"IHDR", &ihdr);
            push_chunk(&mut bytes, b"IDAT", idat);
            push_chunk(&mut bytes, b"IEND", &[]);
            bytes
        };

        assert!(
            parse_png_image_asset("bad-zlib.png", &png_with_idat(&[0xff, 0xff, 0xff, 0xff]))
                .is_none()
        );

        let short_row = zlib_compress(&[0, 10, 20]);
        assert!(parse_png_image_asset("short-row.png", &png_with_idat(&short_row)).is_none());

        let bad_filter = zlib_compress(&[5, 10, 20, 30]);
        assert!(parse_png_image_asset("bad-filter.png", &png_with_idat(&bad_filter)).is_none());
    }

    /// Assemble a minimal 1x1 8-bit RGB PNG, optionally with `iend_data` (an
    /// invalid non-empty IEND) and `trailer` bytes appended after IEND. CRCs are
    /// not verified by the decoder, so they are zeroed.
    fn png_1x1(iend_data: &[u8], trailer: &[u8]) -> Vec<u8> {
        let push = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&0u32.to_be_bytes());
        };
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, RGB, no interlace
        let idat = crate::compress::zlib_compress(&[0u8, 10, 20, 30]); // filter 0 + one RGB pixel
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"\x89PNG\r\n\x1A\n");
        push(&mut bytes, b"IHDR", &ihdr);
        push(&mut bytes, b"IDAT", &idat);
        push(&mut bytes, b"IEND", iend_data);
        bytes.extend_from_slice(trailer);
        bytes
    }

    #[test]
    fn trailing_bytes_after_iend_are_ignored_not_rejected() {
        // A clean PNG parses; the same PNG with a trailer (exporters append
        // metadata, files get concatenated) must still parse and decode instead
        // of being dropped to alt text.
        assert!(parse_png_chunks(&png_1x1(&[], &[])).is_some());
        let with_trailer = png_1x1(&[], b"trailing junk after IEND\n\x00\xFF");
        let parsed = parse_png_chunks(&with_trailer).expect("trailing bytes must not reject a PNG");
        assert_eq!((parsed.width, parsed.height), (1, 1));
        // And the whole asset still resolves to embeddable image data.
        assert!(parse_png_image_asset("x.png", &with_trailer).is_some());
    }

    #[test]
    fn a_non_empty_iend_is_still_rejected() {
        // IEND must carry no data; a non-empty IEND is malformed.
        assert!(parse_png_chunks(&png_1x1(b"junk", &[])).is_none());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod table_wrap_tests {
    use super::{
        CODE_DIAGRAM_MIN_FONT_SIZE, CODE_FONT_SIZE, CodeWrapSpec, F_MONO, Faces, TABLE_COL_GUTTER,
        TABLE_FONT_SIZE, TABLE_MIN_COL_WIDTH, TableColumnMetrics, allocate_table_column_widths,
        cell_tokens, fitted_code_font_size, preserve_code_block_lines, table_cell_measure,
        table_column_badness, text_width, wrap_cell_styled, wrapped_code_rows,
    };
    use crate::PdfOptions;
    use crate::ast::Inline;

    fn text_cell(text: &str) -> Vec<Inline> {
        vec![Inline::Text(text.to_string())]
    }

    fn code_cell(code: &str) -> Vec<Inline> {
        vec![Inline::Code(code.to_string())]
    }

    fn mixed_cell(parts: Vec<Inline>) -> Vec<Inline> {
        parts
    }

    fn width_cache() -> std::cell::RefCell<super::WidthCache> {
        std::cell::RefCell::new(std::collections::HashMap::new())
    }

    fn push_measured_cell(
        column: &mut TableColumnMetrics,
        cell: &[Inline],
        header: bool,
        faces: &Faces,
        width_cache: &std::cell::RefCell<super::WidthCache>,
    ) {
        let toks = cell_tokens(cell, header);
        column.push(table_cell_measure(
            &toks,
            TABLE_FONT_SIZE,
            faces,
            width_cache,
            header,
        ));
    }

    fn measured_table_columns(
        headers: &[&str],
        rows: &[Vec<Vec<Inline>>],
        faces: &Faces,
    ) -> Vec<TableColumnMetrics> {
        let width_cache = width_cache();
        let mut columns = vec![TableColumnMetrics::default(); headers.len()];
        for (idx, header) in headers.iter().enumerate() {
            push_measured_cell(
                &mut columns[idx],
                &text_cell(header),
                true,
                faces,
                &width_cache,
            );
        }
        for row in rows {
            for (idx, cell) in row.iter().enumerate() {
                push_measured_cell(&mut columns[idx], cell, false, faces, &width_cache);
            }
        }
        columns
    }

    fn measured_cell_max_content(cell: &[Inline], header: bool, faces: &Faces) -> f32 {
        let width_cache = width_cache();
        let toks = cell_tokens(cell, header);
        table_cell_measure(&toks, TABLE_FONT_SIZE, faces, &width_cache, header).max_content
    }

    fn total_table_badness(columns: &[TableColumnMetrics], widths: &[f32]) -> f32 {
        columns
            .iter()
            .zip(widths.iter())
            .map(|(column, &width)| table_column_badness(column, width))
            .sum()
    }

    fn proportional_max_content_widths(columns: &[TableColumnMetrics], target: f32) -> Vec<f32> {
        if columns.is_empty() {
            return Vec::new();
        }
        let demand: Vec<f32> = columns
            .iter()
            .map(|column| column.max_content.max(TABLE_MIN_COL_WIDTH))
            .collect();
        let demand_sum: f32 = demand.iter().sum();
        let mut widths: Vec<f32> = if demand_sum > 0.0 {
            demand
                .iter()
                .map(|width| (width / demand_sum) * target)
                .collect()
        } else {
            vec![target / columns.len() as f32; columns.len()]
        };
        for width in &mut widths {
            *width = width.max(TABLE_MIN_COL_WIDTH);
        }
        normalize_test_widths(&mut widths, target);
        widths
    }

    fn normalize_test_widths(widths: &mut [f32], target: f32) {
        if widths.is_empty() {
            return;
        }
        let mut delta = target - widths.iter().sum::<f32>();
        if delta > 0.0 {
            let extra = delta / widths.len() as f32;
            for width in widths {
                *width += extra;
            }
            return;
        }

        delta = -delta;
        while delta > 0.001 {
            let room: f32 = widths
                .iter()
                .map(|width| (*width - TABLE_MIN_COL_WIDTH).max(0.0))
                .sum();
            if room <= 0.001 {
                break;
            }
            for width in widths.iter_mut() {
                let share = (*width - TABLE_MIN_COL_WIDTH).max(0.0) / room;
                let shrink = (delta * share).min((*width - TABLE_MIN_COL_WIDTH).max(0.0));
                *width -= shrink;
            }
            delta = target - widths.iter().sum::<f32>();
            if delta >= -0.001 {
                break;
            }
            delta = -delta;
        }
    }

    #[test]
    fn over_wide_word_is_char_split_to_fit_the_column() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let width_cache = width_cache();
        // A single 200-character word with no break opportunities.
        let cell = vec![Inline::Text("X".repeat(200))];
        let toks = cell_tokens(&cell, false);
        let max_width = 100.0;
        let lines = wrap_cell_styled(&toks, max_width, 10.0, &faces, &width_cache);
        assert!(
            lines.len() > 1,
            "a long unbreakable word must wrap to multiple lines, got {}",
            lines.len()
        );
        for line in &lines {
            assert!(
                line.width <= max_width + 1.0,
                "a wrapped line ({:.1}pt) must fit the column ({:.1}pt) and not overflow the page",
                line.width,
                max_width
            );
        }
    }

    #[test]
    fn table_cell_measure_reuses_the_layout_width_cache() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let shared_width_cache = width_cache();
        let cell = mixed_cell(vec![
            Inline::Text("repeat ".to_string()),
            Inline::Strong(vec![Inline::Text("repeat".to_string())]),
            Inline::Text(" repeat".to_string()),
        ]);
        let toks = cell_tokens(&cell, false);
        let uncached = table_cell_measure(&toks, TABLE_FONT_SIZE, &faces, &width_cache(), false);
        let measured =
            table_cell_measure(&toks, TABLE_FONT_SIZE, &faces, &shared_width_cache, false);

        assert_eq!(measured.lines.len(), uncached.lines.len());
        assert_eq!(measured.min_content, uncached.min_content);
        assert_eq!(measured.max_content, uncached.max_content);

        let cache = shared_width_cache.borrow();
        let cached_words: usize = cache.values().map(std::collections::HashMap::len).sum();
        assert!(
            cached_words >= 3,
            "table measurement should cache repeated body/bold words and spaces, got {cached_words}"
        );
        assert!(
            cache
                .values()
                .any(|slot_cache| slot_cache.contains_key("repeat")),
            "the repeated table body word should be present in the shared width cache"
        );
    }

    #[test]
    fn ascii_diagram_text_fence_preserves_rows_and_fits_by_shrinking() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let diagram = "+----------------------------+\n| parser -> layout -> pdf    |\n+----------------------------+";
        assert!(preserve_code_block_lines(Some("text"), diagram));

        let size = fitted_code_font_size(
            diagram,
            120.0,
            false,
            1,
            CODE_FONT_SIZE,
            CODE_DIAGRAM_MIN_FONT_SIZE,
            &faces,
        );
        assert!(
            size < CODE_FONT_SIZE,
            "diagram font should shrink to preserve geometry"
        );

        let rows = wrapped_code_rows(
            diagram.lines().next().unwrap(),
            CodeWrapSpec {
                lang: Some("text"),
                line_no: 1,
                digits: 1,
                line_numbers: false,
                x0: 0.0,
                max_text_width: 120.0,
                number_col: 0.0,
                size,
                preserve_lines: true,
                faces: &faces,
            },
        );
        assert_eq!(rows.len(), 1, "diagram source rows must not hard-wrap");
        let row_width: f32 = rows[0]
            .iter()
            .filter(|seg| seg.slot == F_MONO)
            .map(|seg| seg.width)
            .sum();
        assert!(
            row_width <= 121.0,
            "shrunk diagram row should fit the target width, got {row_width:.1}pt"
        );
    }

    #[test]
    fn diagram_preservation_accepts_language_metadata_suffixes() {
        let diagram = "+------+\n| A->B |\n+------+";
        assert!(preserve_code_block_lines(
            Some("language-text,diagram"),
            diagram
        ));
        assert!(preserve_code_block_lines(
            Some("language-mermaid,theme=dark"),
            "graph TD\nA-->B"
        ));
    }

    #[test]
    fn ascii_diagram_fitting_accounts_for_line_number_column() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let diagram = "+----------------------------+\n| parser -> layout -> pdf    |\n+----------------------------+";
        let code_area_width = 128.0;
        let digits = 3;
        let size = fitted_code_font_size(
            diagram,
            code_area_width,
            true,
            digits,
            CODE_FONT_SIZE,
            CODE_DIAGRAM_MIN_FONT_SIZE,
            &faces,
        );
        let number_col = super::code_line_number_column_width(digits, size, &faces);
        let rows = wrapped_code_rows(
            diagram.lines().next().unwrap(),
            CodeWrapSpec {
                lang: Some("text"),
                line_no: 1,
                digits,
                line_numbers: true,
                x0: 0.0,
                max_text_width: (code_area_width - number_col).max(12.0),
                number_col,
                size,
                preserve_lines: true,
                faces: &faces,
            },
        );

        assert_eq!(rows.len(), 1, "diagram source rows must not hard-wrap");
        let row_end = rows[0]
            .iter()
            .map(|seg| seg.x + seg.width)
            .fold(0.0f32, f32::max);
        assert!(
            row_end <= code_area_width + 1.0,
            "line-numbered diagram row should fit the code area, got {row_end:.1}pt"
        );
    }

    #[test]
    fn ordinary_source_code_still_wraps_when_requested() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let rows = wrapped_code_rows(
            "let unusually_long_identifier_name = compute_the_value_from_many_inputs();",
            CodeWrapSpec {
                lang: Some("rust"),
                line_no: 1,
                digits: 1,
                line_numbers: false,
                x0: 0.0,
                max_text_width: 42.0,
                number_col: 0.0,
                size: CODE_FONT_SIZE,
                preserve_lines: false,
                faces: &faces,
            },
        );
        assert!(
            rows.len() > 1,
            "source code should keep wrapping in narrow columns"
        );
    }

    #[test]
    fn table_allocator_preserves_compact_header_columns_under_pressure() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let headers = [
            "Candidate",
            "Impact",
            "Confidence",
            "Effort",
            "Score",
            "First bead",
        ];
        let rows = performance_plan_ev_matrix_rows();
        let columns = measured_table_columns(&headers, &rows, &faces);
        let target = 468.0 - TABLE_COL_GUTTER * headers.len() as f32;
        let widths = allocate_table_column_widths(&columns, target);
        let total: f32 = widths.iter().sum();

        assert!(
            (total - target).abs() <= 0.1,
            "allocated widths should fill the target: {total:.1}"
        );
        assert!(
            widths
                .iter()
                .all(|width| *width >= TABLE_MIN_COL_WIDTH - 0.01),
            "no column should collapse below the hard minimum: {widths:?}"
        );

        let confidence_header = measured_cell_max_content(&text_cell("Confidence"), true, &faces);
        assert!(
            widths[2] >= confidence_header - 0.5,
            "the Confidence header should remain on one readable line: {widths:?}"
        );

        let compact_header_widths = ["Impact", "Effort", "Score"]
            .map(|header| measured_cell_max_content(&text_cell(header), true, &faces));
        assert!(
            widths[1] >= compact_header_widths[0] - 0.5
                && widths[3] >= compact_header_widths[1] - 0.5
                && widths[4] >= compact_header_widths[2] - 0.5,
            "compact numeric headers should fit on one line: {widths:?}"
        );

        assert!(
            widths[0] >= 140.0,
            "the long Candidate column should not be starved: {widths:?}"
        );
        assert!(
            widths[5] >= 70.0,
            "the First bead column should retain enough width for code-like labels: {widths:?}"
        );

        let proportional = proportional_max_content_widths(&columns, target);
        let optimized_badness = total_table_badness(&columns, &widths);
        let proportional_badness = total_table_badness(&columns, &proportional);
        assert!(
            optimized_badness < proportional_badness * 0.60,
            "optimized allocation should reduce measured wrapping badness; optimized {optimized_badness:.1}, proportional {proportional_badness:.1}, widths {widths:?}, proportional {proportional:?}"
        );
    }

    #[test]
    fn table_allocator_does_not_spend_width_on_empty_columns() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let headers = ["Left", "", "Right"];
        let rows = vec![vec![
            text_cell("alpha beta gamma delta epsilon"),
            text_cell(""),
            text_cell("one two three four five"),
        ]];
        let columns = measured_table_columns(&headers, &rows, &faces);
        let widths = allocate_table_column_widths(&columns, 240.0);

        assert!(
            widths[1] <= TABLE_MIN_COL_WIDTH + 0.5,
            "empty columns should stay at the hard minimum before widening content columns: {widths:?}"
        );
        assert!(
            widths[0] > widths[1] && widths[2] > widths[1],
            "non-empty columns should receive the usable extra width: {widths:?}"
        );
    }

    #[test]
    fn table_allocator_balances_surplus_after_all_columns_fit() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let headers = ["Name", "Qty", "Price"];
        let rows = vec![
            vec![text_cell("alpha"), text_cell("1"), text_cell("9.99")],
            vec![text_cell("beta"), text_cell("22"), text_cell("12.00")],
            vec![text_cell("gamma"), text_cell("333"), text_cell("7.50")],
        ];
        let columns = measured_table_columns(&headers, &rows, &faces);
        let widths = allocate_table_column_widths(&columns, 270.0);
        let total: f32 = widths.iter().sum();

        assert!(
            (total - 270.0).abs() <= 0.1,
            "allocated widths should fill the target: {total:.1}"
        );
        assert!(
            widths[1] >= 60.0 && widths[2] >= 60.0,
            "surplus should not be dumped into only the first column: {widths:?}"
        );
        assert!(
            widths[0] <= 150.0,
            "a fully fitting first column should not consume nearly all surplus: {widths:?}"
        );
    }

    fn performance_plan_ev_matrix_rows() -> Vec<Vec<Vec<Inline>>> {
        vec![
            vec![
                text_cell(
                    "PDF serializer/shaping fast path: buffer sizing, fast decimal/hex writers, shaped-run cache, subset-map layout",
                ),
                text_cell("5"),
                text_cell("4"),
                text_cell("2"),
                text_cell("10.0"),
                code_cell("fep.6"),
            ],
            vec![
                text_cell(
                    "Asupersync batch renderer: file-level parallelism, deterministic receipts, queueing budgets",
                ),
                text_cell("5"),
                text_cell("5"),
                text_cell("3"),
                text_cell("8.3"),
                code_cell("zmd.1"),
            ],
            vec![
                text_cell("Parser scanner attribution and allocation reduction"),
                text_cell("4"),
                text_cell("4"),
                text_cell("2"),
                text_cell("8.0"),
                text_cell("new child under gauntlet/parser"),
            ],
            vec![
                text_cell(
                    "PDF stage instrumentation: split layout/subset/ToUnicode/serialize timings",
                ),
                text_cell("4"),
                text_cell("5"),
                text_cell("3"),
                text_cell("6.7"),
                mixed_cell(vec![
                    Inline::Text("new child under ".to_string()),
                    Inline::Code("fep.6".to_string()),
                ]),
            ],
            vec![
                text_cell("Hyphen word-result cache or trie layout compaction"),
                text_cell("2"),
                text_cell("4"),
                text_cell("2"),
                text_cell("4.0"),
                text_cell("future child after profile"),
            ],
            vec![
                text_cell("SIMD special-byte scanner island"),
                text_cell("4"),
                text_cell("3"),
                text_cell("4"),
                text_cell("3.0"),
                code_cell("qw1.5"),
            ],
            vec![
                text_cell("Active-list/page-builder parallelism inside one document"),
                text_cell("3"),
                text_cell("2"),
                text_cell("4"),
                text_cell("1.5"),
                text_cell("defer"),
            ],
            vec![
                text_cell("AVX-512-specific path"),
                text_cell("2"),
                text_cell("2"),
                text_cell("5"),
                text_cell("0.8"),
                text_cell("reject until separate hardware proof"),
            ],
        ]
    }

    #[test]
    fn measured_diagram_width_is_larger_at_default_size() {
        let faces = Faces::load(&PdfOptions::default()).unwrap();
        let line = "+----------------------------+";
        let default_width = text_width(line, CODE_FONT_SIZE, F_MONO, &faces);
        let shrunken_width = text_width(line, CODE_DIAGRAM_MIN_FONT_SIZE, F_MONO, &faces);
        assert!(default_width > shrunken_width);
    }
}
