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

use crate::PdfOptions;
use crate::ast::{Align, Block, Document, Inline, List, Table};
use crate::error::Result;
use crate::fonts::{self, FontStyle};
use crate::highlight::{self, Tok as HighlightTok};
use crate::layout::{
    FontSize, LayoutUnit, ParagraphItem, Penalty, StyledText, TextBox, break_paragraph,
    default_interword_glue, measure_text_with_pairs,
};
use crate::text::{Font, Kerning, Ligatures};
use crate::theme::Theme;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(not(target_arch = "wasm32"))]
type PdfStageStart = std::time::Instant;
#[cfg(target_arch = "wasm32")]
type PdfStageStart = ();

const MIN_PAGE_DIM: f32 = 80.0;
const MIN_CONTENT_DIM: f32 = 40.0;
const PAGE_STREAM_COMPRESSION_MIN: usize = 4096;

// Inline-link styling: hyperlink blue (`rg` fill + `RG` underline stroke).
const LINK_COLOR: (f32, f32, f32) = (0.07, 0.20, 0.55);
const BLACK: (f32, f32, f32) = (0.0, 0.0, 0.0);

// Fenced-code panel + inline-code chip backgrounds.
const CODE_PAD_X: f32 = 8.0; // text inset inside a fenced-code line
const CODE_HANGING_INDENT: f32 = 12.0; // continuation inset for wrapped code rows
const CODE_LINE_NUMBER_GAP: f32 = 6.0;
const PANEL_PAD_V: f32 = 5.0; // vertical breathing room above/below the code
const PANEL_RADIUS: f32 = 4.0;
const PANEL_GRAY: (f32, f32, f32) = (0.960, 0.960, 0.970);
const PANEL_ASCENT_FRAC: f32 = 0.78; // glyph top above baseline (mono)
const PANEL_DESCENT_FRAC: f32 = 0.30; // glyph bottom below baseline
const CHIP_PAD_X: f32 = 2.0;
const CHIP_RADIUS: f32 = 2.5;
const CHIP_GRAY: (f32, f32, f32) = (0.930, 0.930, 0.950);
const QUOTE_BG: (f32, f32, f32) = (0.985, 0.987, 0.991);
const QUOTE_BG_PAD_X: f32 = 6.0;
const QUOTE_BG_PAD_V: f32 = 3.0;

// Table zebra-stripe tint for alternating body rows (very subtle cool gray).
const TABLE_STRIPE: (f32, f32, f32) = (0.965, 0.969, 0.975);

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
    segs: Vec<Seg>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FlowKind {
    Paragraph,
    Heading,
    Code,
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
}

impl Default for FlowMark {
    fn default() -> Self {
        Self {
            group: 0,
            index: 0,
            count: 1,
            kind: FlowKind::Other,
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
    fn load(opts: &PdfOptions) -> Option<Self> {
        let fam = opts.theme.font;
        Some(Self {
            body: fonts::load_body(fam, FontStyle::Regular).ok()?,
            bold: fonts::load_body(fam, FontStyle::Bold).ok()?,
            italic: fonts::load_body(fam, FontStyle::Italic).ok()?,
            bolditalic: fonts::load_body(fam, FontStyle::BoldItalic).ok()?,
            mono: fonts::load_mono(FontStyle::Regular).ok()?,
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

fn render_inner(doc: &Document, opts: &PdfOptions, profiled: bool) -> Result<PdfProfile> {
    let mut profiler = if profiled {
        PdfProfiler::enabled()
    } else {
        PdfProfiler::disabled()
    };
    let page = PageGeom::from_theme(&opts.theme);
    let Some(faces) = profiler.measure(
        "font_load",
        5,
        "load bundled body/bold/italic/bolditalic/mono faces",
        || Faces::load(opts),
        |_| 0,
    ) else {
        // The bundled fonts are tested to parse, so this is unreachable in
        // practice; emit a valid empty document rather than failing.
        return Ok(PdfProfile {
            bytes: empty_pdf(page),
            stages: profiler.finish(),
        });
    };
    let lines = profiler.measure(
        "layout",
        doc.blocks.len(),
        "block layout, text measuring, and paragraph line breaking",
        || layout(&doc.blocks, opts, &faces, page),
        |_| 0,
    );
    let line_count = lines.len();
    let serialize_started = profiler.checkpoint();
    let bytes = serialize(&lines, opts, &faces, page, &mut profiler);
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
    };
    layout_blocks(blocks, 0.0, &mut out, &mut cx);
    out
}

struct LayoutCx<'a> {
    opts: &'a PdfOptions,
    faces: &'a Faces,
    page: PageGeom,
    next_bg: u32,
    next_flow: u32,
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
                        segs,
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
                    segs,
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
                },
                segs: Vec::new(),
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
        },
        segs: Vec::new(),
    });
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
        };
    }
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

    // Natural (unwrapped) text width per column across the header + every row.
    let mut natural = vec![0f32; ncol];
    for (k, cell) in table.head.iter().enumerate() {
        if let Some(w) = natural.get_mut(k) {
            *w = w.max(text_width(&inline_text(cell), size, F_BOLD, faces));
        }
    }
    for row in &table.rows {
        for (k, cell) in row.iter().enumerate() {
            if let Some(w) = natural.get_mut(k) {
                *w = w.max(text_width(&inline_text(cell), size, F_BODY, faces));
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
        |cells: &[Vec<Inline>], slot: u8, gap_after: f32, kind: FlowKind, shade: bool| {
            let wrapped: Vec<Vec<String>> = (0..ncol)
                .map(|k| {
                    let text = cells.get(k).map(|c| inline_text(c)).unwrap_or_default();
                    wrap_table_cell(
                        &text,
                        colw.get(k).copied().unwrap_or(12.0),
                        size,
                        slot,
                        faces,
                    )
                })
                .collect();
            let depth = wrapped.iter().map(Vec::len).max().unwrap_or(0).max(1);
            let mut lines = Vec::with_capacity(depth);

            for row_idx in 0..depth {
                let mut segs = Vec::new();
                for k in 0..ncol {
                    let Some(text) = wrapped.get(k).and_then(|parts| parts.get(row_idx)) else {
                        continue;
                    };
                    if text.trim().is_empty() {
                        continue;
                    }
                    let w = text_width(text, size, slot, faces);
                    let cw = colw.get(k).copied().unwrap_or(0.0);
                    let base = tx.get(k).copied().unwrap_or(left);
                    let x = match table.align.get(k) {
                        Some(Align::Right) => base + (cw - w),
                        Some(Align::Center) => base + (cw - w) / 2.0,
                        _ => base,
                    };
                    segs.push(Seg {
                        x: x.max(base),
                        slot,
                        text: text.clone(),
                        link: None,
                        fill: Fill::Black,
                        strike: false,
                        width: w,
                    });
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
                    },
                    segs,
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
        },
        segs: Vec::new(),
    };

    out.extend(row_lines(
        &table.head,
        F_BOLD,
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
            F_BODY,
            if i + 1 == nrows { 3.0 } else { 2.5 },
            FlowKind::TableRow,
            i % 2 == 0,
        ));
    }
    out.push(rule(0.0));
    gap(out, 8.0);
}

fn wrap_table_cell(text: &str, max_width: f32, size: f32, slot: u8, faces: &Faces) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };

        if text_width(&candidate, size, slot, faces) <= max_width || current.is_empty() {
            current = candidate;
            if text_width(&current, size, slot, faces) > max_width {
                lines.extend(split_table_word(&current, max_width, size, slot, faces));
                current.clear();
            }
        } else {
            lines.push(std::mem::take(&mut current));
            if text_width(word, size, slot, faces) > max_width {
                lines.extend(split_table_word(word, max_width, size, slot, faces));
            } else {
                current.push_str(word);
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn split_table_word(word: &str, max_width: f32, size: f32, slot: u8, faces: &Faces) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for ch in word.chars() {
        let mut candidate = current.clone();
        candidate.push(ch);
        if !current.is_empty() && text_width(&candidate, size, slot, faces) > max_width {
            lines.push(std::mem::take(&mut current));
            current.push(ch);
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
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
        for b in rest {
            layout_block(b, content_indent, out, cx);
        }
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
        if c.is_whitespace() {
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
fn build_paragraph(
    toks: &[Tok],
    fs: FontSize,
    faces: &Faces,
) -> (Vec<ParagraphItem>, Vec<Vec<Tok>>) {
    let mut items: Vec<ParagraphItem> = Vec::new();
    let mut item_toks: Vec<Vec<Tok>> = Vec::new();
    let mut word: Vec<Tok> = Vec::new();

    let flush_word =
        |items: &mut Vec<ParagraphItem>, item_toks: &mut Vec<Vec<Tok>>, word: &mut Vec<Tok>| {
            if word.is_empty() {
                return;
            }
            let plain: String = word.iter().map(|t| t.text.as_str()).collect();
            let width = measure_word(word, fs, faces);
            items.push(ParagraphItem::Box(TextBox {
                text: plain.clone(),
                runs: StyledText::plain(&plain), // unused by breaker; width is what matters
                width,
            }));
            item_toks.push(std::mem::take(word));
        };

    for tok in toks {
        if tok.space {
            if tok.hard_break {
                if !word.is_empty() {
                    flush_word(&mut items, &mut item_toks, &mut word);
                }
                items.push(ParagraphItem::Penalty(Penalty {
                    width: LayoutUnit::ZERO,
                    penalty: crate::layout::FORCED_BREAK_PENALTY,
                    flagged: false,
                }));
                item_toks.push(Vec::new());
                continue;
            }
            if !word.is_empty() {
                flush_word(&mut items, &mut item_toks, &mut word);
            }
            // Only emit glue *between* two words (collapses runs of spaces).
            if matches!(items.last(), Some(ParagraphItem::Box(_))) {
                let gw = measure_text_with_pairs(faces.get(tok.slot), " ", fs);
                items.push(ParagraphItem::Glue(default_interword_glue(gw)));
                item_toks.push(vec![tok.clone()]);
            }
        } else {
            word.push(tok.clone());
        }
    }
    flush_word(&mut items, &mut item_toks, &mut word);

    if !matches!(
        items.last(),
        Some(ParagraphItem::Penalty(Penalty {
            penalty: crate::layout::FORCED_BREAK_PENALTY,
            ..
        }))
    ) {
        items.push(ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty: crate::layout::FORCED_BREAK_PENALTY,
            flagged: false,
        }));
        item_toks.push(Vec::new());
    }
    (items, item_toks)
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
    let (items, item_toks) = build_paragraph(&toks, fs, cx.faces);

    // No renderable words -> just advance the vertical gap (old empty behavior).
    if !items.iter().any(|it| matches!(it, ParagraphItem::Box(_))) {
        gap(out, gap_after);
        return;
    }

    let content_w = lu_from_points_f32((cx.page.content_w - indent).max(MIN_CONTENT_DIM));
    let breaks = break_paragraph(&items, content_w);
    if breaks.is_empty() {
        // Emergency fallback: the optimizer produced nothing.
        layout_inlines_greedy(toks, indent, size, gap_after, cx.faces, cx.page, out);
        mark_flow(out, start, flow.group, flow.kind);
        return;
    }

    let n = breaks.len();
    for (i, lb) in breaks.iter().enumerate() {
        let mut line: Vec<Tok> = Vec::new();
        if let Some(range) = item_toks.get(lb.start..lb.end) {
            for group in range {
                line.extend(group.iter().cloned());
            }
        }
        // Drop any trailing space tokens (e.g. an interior glue at line end).
        while line.last().is_some_and(|t| t.space) {
            line.pop();
        }
        let segs = build_segs(&line, left, size, cx.faces);
        out.push(Line {
            size,
            gap_after: if i + 1 == n { gap_after } else { 0.0 },
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark::default(),
            segs,
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
    let (items, item_toks) = build_paragraph(&toks, fs, cx.faces);

    if !items.iter().any(|it| matches!(it, ParagraphItem::Box(_))) {
        out.push(Line {
            size: spec.size,
            gap_after: spec.gap_after,
            rule: false,
            rule_x: 0.0,
            quote_bars: Vec::new(),
            bg: 0,
            shade: false,
            flow: FlowMark::default(),
            segs: vec![marker],
        });
        mark_flow(out, start, spec.flow.group, spec.flow.kind);
        return;
    }

    let content_w =
        lu_from_points_f32((cx.page.content_w - spec.content_indent).max(MIN_CONTENT_DIM));
    let breaks = break_paragraph(&items, content_w);
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
        let mut line: Vec<Tok> = Vec::new();
        if let Some(range) = item_toks.get(lb.start..lb.end) {
            for group in range {
                line.extend(group.iter().cloned());
            }
        }
        while line.last().is_some_and(|t| t.space) {
            line.pop();
        }
        let mut segs = build_segs(&line, left, spec.size, cx.faces);
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
            segs,
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
            segs,
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

/// Group consecutive same-slot, same-link tokens into positioned segments,
/// accumulating each segment's layout (non-kerned) advance width.
fn build_segs(toks: &[Tok], left: f32, size: f32, faces: &Faces) -> Vec<Seg> {
    let mut segs: Vec<Seg> = Vec::new();
    let mut x = left;
    let mut cur: Option<Seg> = None;
    for tok in toks {
        let tw = token_width(tok, size, faces);
        match &mut cur {
            Some(s) if s.slot == tok.slot && s.link == tok.link && s.strike == tok.strike => {
                s.text.push_str(&tok.text);
                s.width += tw;
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
                    width: tw,
                });
            }
        }
        x += tw;
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

fn fill_rgb(fill: Fill) -> (f32, f32, f32) {
    match fill {
        Fill::Black => BLACK,
        Fill::Link => LINK_COLOR,
        Fill::Syntax(HighlightTok::Keyword) => (0.812, 0.133, 0.180),
        Fill::Syntax(HighlightTok::Type) => (0.584, 0.220, 0.000),
        Fill::Syntax(HighlightTok::Func) => (0.400, 0.224, 0.729),
        Fill::Syntax(HighlightTok::Str) => (0.039, 0.188, 0.412),
        Fill::Syntax(HighlightTok::Number) => (0.020, 0.314, 0.682),
        Fill::Syntax(HighlightTok::Comment) => (0.431, 0.467, 0.506),
        Fill::Syntax(HighlightTok::Operator) => (0.020, 0.314, 0.682),
        Fill::Syntax(HighlightTok::Plain | HighlightTok::Punct) => BLACK,
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
) -> Vec<u8> {
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
    let mut subsets: Vec<EmbeddedFace> = Vec::new();
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
        let mut seed: Vec<u16> = keep.iter().map(|&c| source.glyph_index(c)).collect();
        seed.extend(shaped_glyphs);
        let subset_started = profiler.checkpoint();
        let Some((bytes, map)) = source.subset_glyphs(&seed, &keep) else {
            return empty_pdf(page);
        };
        let Ok(font) = Font::parse(bytes.clone()) else {
            return empty_pdf(page);
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

    // PASS 2 — per page: backgrounds (code panels + inline-code chips) FIRST,
    // then text + rules, then blockquote gutter bars. Link annotations and
    // outline destinations are collected from the same placed geometry.
    let stream_generation_started = profiler.checkpoint();
    let mut pages: Vec<PageContent> = Vec::new();
    let mut outlines: Vec<OutlineEntry> = Vec::new();
    let mut seen_heading_groups: BTreeSet<u32> = BTreeSet::new();
    for (page_idx, placed) in pages_placed.iter().enumerate() {
        let mut bg = String::new();
        let mut body = String::new();
        let mut annots = Vec::new();
        let mut marks = Vec::new();
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
                QUOTE_BG,
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
                TABLE_STRIPE,
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
                    PANEL_GRAY,
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
                    CHIP_GRAY,
                ));
            }
        }

        // (d) Text + rules.
        let mut current_fill = Fill::Black;
        for p in placed {
            let line = p.line;
            let y = p.y;
            if line.flow.kind == FlowKind::Heading
                && line.flow.group != 0
                && seen_heading_groups.insert(line.flow.group)
                && let Some(meta) = heading_meta.get(&line.flow.group)
            {
                outlines.push(OutlineEntry {
                    id: meta.id.clone(),
                    title: meta.title.clone(),
                    page_index: page_idx,
                    y: (y + line.size * 0.9).min(page.top_y()),
                });
            }
            if line.rule {
                let x2 = page.right_x();
                body.push_str(&format!(
                    "0.82 0.82 0.84 RG 0.7 w {x:.2} {yy:.2} m {x2:.2} {yy:.2} l S\n",
                    x = line.rule_x,
                    yy = y + line.size * 0.5,
                ));
            } else {
                let marked = line_has_visible_text(line);
                if marked {
                    let tag = struct_tag_for_line(line);
                    body.push_str(&format!("/{tag} <</MCID {next_mcid}>> BDC\n"));
                    marks.push(StructMark {
                        mcid: next_mcid,
                        tag,
                    });
                    next_mcid += 1;
                }
                for seg in &line.segs {
                    if seg.text.is_empty() {
                        continue;
                    }
                    if let Some(face) = subsets.iter().find(|f| f.slot == seg.slot) {
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
                        if seg.fill != current_fill {
                            let (r, g, b) = fill_rgb(seg.fill);
                            body.push_str(&format!("{r:.3} {g:.3} {b:.3} rg\n"));
                            current_fill = seg.fill;
                        }
                        body.push_str(&format!(
                            "BT /F{f} {s:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm {tj} TJ ET\n",
                            f = seg.slot,
                            s = line.size,
                            x = seg.x,
                            y = y,
                            tj = kerned_tj(&face.map, source, &face.kern, shaped),
                        ));
                        // Strikethrough: a thin stroke through the run's middle,
                        // in the text's own color (stroke `RG`, leaving the text
                        // fill `rg` untouched so `current_fill` stays in sync).
                        if seg.strike && seg.width > 0.0 {
                            let (r, g, b) = fill_rgb(seg.fill);
                            let sy = y + line.size * 0.30;
                            let sw = (line.size * 0.06).max(0.4);
                            body.push_str(&format!(
                                "{r:.3} {g:.3} {b:.3} RG {sw:.2} w \
                                 {x1:.2} {sy:.2} m {x2:.2} {sy:.2} l S\n",
                                x1 = seg.x,
                                x2 = seg.x + seg.width,
                            ));
                        }
                        if let Some(target) = &seg.link {
                            let (r, g, b) = LINK_COLOR;
                            let uy = y - line.size * 0.12;
                            let uw = (line.size * 0.06).max(0.4);
                            body.push_str(&format!(
                                "{r:.3} {g:.3} {b:.3} RG {uw:.2} w \
                                 {x1:.2} {uy:.2} m {x2:.2} {uy:.2} l S\n0 0 0 rg\n",
                                x1 = seg.x,
                                x2 = seg.x + seg.width,
                            ));
                            current_fill = Fill::Black;
                            if seg.width > 0.0 {
                                annots.push(LinkAnnotation {
                                    rect: Rect {
                                        x0: seg.x,
                                        y0: y - line.size * 0.28,
                                        x1: seg.x + seg.width,
                                        y1: y + line.size * 0.86,
                                    },
                                    target: target.clone(),
                                });
                            }
                        }
                    }
                }
                if marked {
                    body.push_str("EMC\n");
                }
            }
        }

        // (e) Blockquote gutter bars: accumulate each quote's vertical extent on
        // this page (keyed by quote id), then stroke one segment per quote.
        let mut quote_acc = quote_extents(placed);
        flush_quote_bars(&mut body, &mut quote_acc);

        pages.push(PageContent {
            stream: format!("{bg}{body}"),
            annots,
            marks,
        });
    }
    if pages.is_empty() {
        pages.push(PageContent {
            stream: String::new(),
            annots: Vec::new(),
            marks: Vec::new(),
        });
    }
    let page_stream_bytes = pages.iter().map(|page| page.stream.len()).sum();
    profiler.record_since(
        "page_content_stream_generation",
        pages.len(),
        page_stream_bytes,
        "generate page drawing operators, annotations, outlines, and structure marks",
        stream_generation_started,
    );

    build_pdf(&pages, &outlines, &subsets, opts, page, profiler)
}

struct PageContent {
    stream: String,
    annots: Vec<LinkAnnotation>,
    marks: Vec<StructMark>,
}

#[derive(Clone, Copy)]
struct StructMark {
    mcid: usize,
    tag: &'static str,
}

#[derive(Clone)]
struct LinkAnnotation {
    rect: Rect,
    target: LinkTarget,
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

fn line_has_visible_text(line: &Line) -> bool {
    line.segs.iter().any(|seg| !seg.text.is_empty())
}

fn struct_tag_for_line(line: &Line) -> &'static str {
    if line.segs.iter().any(|seg| seg.link.is_some()) {
        return "Link";
    }
    match line.flow.kind {
        FlowKind::Heading if line.size >= 23.0 => "H1",
        FlowKind::Heading if line.size >= 18.0 => "H2",
        FlowKind::Heading if line.size >= 15.0 => "H3",
        FlowKind::Heading => "H",
        FlowKind::Code => "Code",
        FlowKind::TableHeader | FlowKind::TableRow => "TR",
        FlowKind::Paragraph => "P",
        _ => "P",
    }
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
fn flush_quote_bars(content: &mut String, acc: &mut BTreeMap<usize, (f32, f32, f32)>) {
    for (x, top, bot) in acc.values() {
        content.push_str(&format!(
            "0.75 0.75 0.80 RG 2.50 w {x:.2} {top:.2} m {x:.2} {bot:.2} l S\n"
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
    opts: &PdfOptions,
    page_geom: PageGeom,
    profiler: &mut PdfProfiler,
) -> Vec<u8> {
    let build_started = profiler.checkpoint();
    let p = pages.len();
    let nf = faces.len();
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
    let mut mark_starts = Vec::with_capacity(p);
    let mut total_marks = 0usize;
    for page in pages {
        mark_starts.push(total_marks);
        total_marks += page.marks.len();
    }
    let tagged = total_marks > 0;

    // Object number plan (1-indexed):
    //   1 Catalog, 2 Pages, [3..3+p) Page objs, [3+p..3+2p) content streams,
    //   then per face k: type0, cidfont, descriptor, fontfile, tounicode (5),
    //   then link annotations, optional outline root/items, optional structure
    //   root/parent-tree/elements, then Info.
    let page_obj = |i: usize| 3 + i;
    let content_obj = |i: usize| 3 + p + i;
    let face_base = 3 + 2 * p;
    let type0_obj = |k: usize| face_base + 5 * k;
    let cid_obj = |k: usize| face_base + 5 * k + 1;
    let desc_obj = |k: usize| face_base + 5 * k + 2;
    let file_obj = |k: usize| face_base + 5 * k + 3;
    let touni_obj = |k: usize| face_base + 5 * k + 4;
    let annot_base = face_base + 5 * nf;
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
    let struct_elem_obj = |page_index: usize, local_index: usize| {
        struct_elem_base + mark_starts.get(page_index).copied().unwrap_or(0) + local_index
    };
    let info_obj = struct_base + if tagged { 2 + total_marks } else { 0 };
    let total_objs = info_obj;

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

    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = vec![0; total_objs + 1];

    let emit = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, n: usize, body: &str| {
        offsets[n] = buf.len();
        buf.extend_from_slice(format!("{n} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    emit(
        &mut buf,
        &mut offsets,
        1,
        &format!("<< /Type /Catalog /Pages 2 0 R{outline_root_ref}{structure_root_ref} >>"),
    );

    let kids = (0..p)
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect::<Vec<_>>()
        .join(" ");
    emit(
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
        emit(
            &mut buf,
            &mut offsets,
            page_obj(i),
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {media_w} {media_h}] \
                 /Resources << /Font << {font_res} >> >> /Contents {c} 0 R{annots}{struct_parent} >>",
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

        emit(
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
        emit(
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
        emit(
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
            face.font.num_glyphs as usize + face.lig_uni.len(),
            "generate selectable-text ToUnicode CMap for one subset face",
            || tounicode_cmap(&face.font, &face.lig_uni),
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

    for (page_index, page) in pages.iter().enumerate() {
        for (local_index, annot) in page
            .annots
            .iter()
            .filter(|annot| annotation_is_resolved(annot, &dest_ids))
            .enumerate()
        {
            let body = annotation_dict(annot, &dest_by_id, page_obj);
            emit(
                &mut buf,
                &mut offsets,
                annot_obj(page_index, local_index),
                &body,
            );
        }
    }

    if outline_count > 0 {
        emit(
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
            emit(
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
        let root_kids = (0..p)
            .flat_map(|page_index| {
                (0..pages[page_index].marks.len()).map(move |local_index| {
                    format!("{} 0 R", struct_elem_obj(page_index, local_index))
                })
            })
            .collect::<Vec<_>>()
            .join(" ");
        emit(
            &mut buf,
            &mut offsets,
            struct_root_obj,
            &format!(
                "<< /Type /StructTreeRoot /K [ {root_kids} ] /ParentTree {parent_tree_obj} 0 R >>"
            ),
        );

        let mut nums = String::new();
        for (page_index, page) in pages.iter().enumerate() {
            if page.marks.is_empty() {
                continue;
            }
            let refs = (0..page.marks.len())
                .map(|local_index| format!("{} 0 R", struct_elem_obj(page_index, local_index)))
                .collect::<Vec<_>>()
                .join(" ");
            nums.push_str(&format!("{page_index} [ {refs} ] "));
        }
        emit(
            &mut buf,
            &mut offsets,
            parent_tree_obj,
            &format!("<< /Nums [ {nums}] >>"),
        );

        for (page_index, page) in pages.iter().enumerate() {
            for (local_index, mark) in page.marks.iter().enumerate() {
                emit(
                    &mut buf,
                    &mut offsets,
                    struct_elem_obj(page_index, local_index),
                    &format!(
                        "<< /Type /StructElem /S /{tag} /P {parent} 0 R /Pg {page_obj} 0 R \
                         /K << /Type /MCR /Pg {page_obj} 0 R /MCID {mcid} >> >>",
                        tag = mark.tag,
                        parent = struct_root_obj,
                        page_obj = page_obj(page_index),
                        mcid = mark.mcid,
                    ),
                );
            }
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
    emit(
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
        return empty_pdf(page_geom);
    }

    // xref + trailer.
    let xref_started = profiler.checkpoint();
    let xref_pos = buf.len();
    let size = total_objs + 1;
    buf.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets.iter().take(total_objs + 1).skip(1) {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size {size} /Root 1 0 R /Info {info_obj} 0 R >>\n\
             startxref\n{xref_pos}\n%%EOF\n"
        )
        .as_bytes(),
    );
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
    buf
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
) -> String {
    let rect = format!(
        "[{} {} {} {}]",
        pdf_num(annot.rect.x0),
        pdf_num(annot.rect.y0),
        pdf_num(annot.rect.x1),
        pdf_num(annot.rect.y1),
    );
    match &annot.target {
        LinkTarget::Uri(uri) => format!(
            "<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0] \
             /A << /S /URI /URI ({uri}) >> >>",
            uri = pdf_escape(uri),
        ),
        LinkTarget::Fragment(id) => {
            let Some(dest) = dest_by_id.get(id.as_str()) else {
                return format!("<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0] >>");
            };
            format!(
                "<< /Type /Annot /Subtype /Link /Rect {rect} /Border [0 0 0] \
                 /Dest [{page} 0 R /XYZ null {y} null] >>",
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
    let mut s = String::from("0 [");
    for gid in 0..font.num_glyphs {
        let w = font.advance_width(gid) as u32 * 1000 / upm;
        s.push_str(&w.to_string());
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
        out.push_str(&format!("{:04X}", map.get(&g).copied().unwrap_or(0)));
        if let Some(&next) = shaped.get(i + 1) {
            let k = kern.pair(g, next);
            if k != 0 {
                // A TJ number shifts the next glyph left by number/1000 em, so a
                // tightening (negative) kern becomes a positive number.
                let adj = -(i32::from(k) * 1000 / upm);
                out.push_str(&format!(">{adj}<"));
            }
        }
    }
    out.push_str(">]");
    out
}

/// A `ToUnicode` CMap mapping each glyph id back to its character(s), so text
/// stays selectable. Only the glyphs the document uses appear.
fn tounicode_cmap(font: &Font, lig_uni: &BTreeMap<u16, String>) -> String {
    // (gid, UTF-16BE hex) over the chars present in the subset cmap, plus the
    // ligature glyphs (which no character maps to) so ligated text stays
    // selectable.
    let mut entries: Vec<(u16, String)> = Vec::new();
    for cp in 0x20u32..0x2C00 {
        if let Some(c) = char::from_u32(cp) {
            let g = font.glyph_index(c);
            if g != 0 {
                entries.push((g, utf16be_hex(c)));
            }
        }
    }
    for (g, s) in lig_uni {
        entries.push((*g, s.chars().map(utf16be_hex).collect()));
    }
    entries.sort_by_key(|&(g, _)| g);
    entries.dedup_by_key(|(g, _)| *g);

    let mut body = String::new();
    for chunk in entries.chunks(100) {
        body.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (g, hex) in chunk {
            body.push_str(&format!("<{g:04X}> <{hex}>\n"));
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
    let mut s = String::new();
    let mut buf = [0u16; 2];
    for u in c.encode_utf16(&mut buf) {
        s.push_str(&format!("{u:04X}"));
    }
    s
}

/// Deterministic subset PostScript name, e.g. `FMDFA1+Embedded`.
fn subset_psname(k: usize, slot: u8) -> String {
    let tag: String = (0..6)
        .map(|i| (b'A' + ((k as u8 + slot + i as u8) % 26)) as char)
        .collect();
    format!("{tag}+Embedded")
}

/// A minimal but valid empty single-page PDF (degenerate fallback).
fn empty_pdf(page: PageGeom) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets = [0usize; 4];
    let media_w = pdf_num(page.width);
    let media_h = pdf_num(page.height);
    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    for (n, body) in [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        &format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {media_w} {media_h}] >>"),
    ]
    .into_iter()
    .enumerate()
    {
        offsets[n + 1] = buf.len();
        buf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", n + 1).as_bytes());
    }
    let xref_pos = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
    );
    buf
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
    for c in s.chars() {
        match c {
            '(' => o.push_str("\\("),
            ')' => o.push_str("\\)"),
            '\\' => o.push_str("\\\\"),
            '\r' => o.push_str("\\r"),
            '\n' => o.push(' '),
            c if (c as u32) < 256 => o.push(c),
            _ => o.push('?'),
        }
    }
    o
}

fn pdf_num(value: f32) -> String {
    let mut s = format!("{value:.2}");
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_string() } else { s }
}

fn text_width(s: &str, size: f32, font: u8, faces: &Faces) -> f32 {
    s.chars().map(|c| faces.advance(font, c)).sum::<f32>() * size / 1000.0
}

fn char_width(ch: char, size: f32, font: u8, faces: &Faces) -> f32 {
    faces.advance(font, ch) * size / 1000.0
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                s.push_str(&inline_text(c));
            }
            Inline::Link { content, .. } => s.push_str(&inline_text(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::Html(html) => s.push_str(html),
        }
    }
    s
}
