//! Display equations rendered from the shared math engine's real outlines.
//!
//! Math uses its own bundled face roster, so outlined glyphs keep their exact
//! metrics without adding a second font-subsetting protocol to the PDF writer.
//! The existing vector-image lane supplies pagination, clipping, and resources;
//! `/Formula`, `/Alt`, and `/ActualText` retain the source for assistive readers.

use std::cell::OnceCell;
use std::collections::{BTreeSet, HashMap};
use std::mem::size_of;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use fmd_math::{Color, Engine, Layout, PathContour, PathSeg, Placed, Style};

use super::{
    Block, FlowKind, FlowMark, ImageLine, LayoutCx, Line, Palette, PdfImageColor, PdfImageData,
    PdfImageStreamFilter, PdfSvgImage, RenderWarning, SvgElement, SvgPath, SvgPathOp,
    SvgPreserveAspectRatio, SvgStyle, SvgViewBox, SvgViewport, gap, pdf_text_string,
    positive_finite,
};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const SOURCE_LIMIT_REASON: &str = "formula exceeds the 64 KiB source limit";
const MAX_PRIMITIVES: usize = 4096;
const MAX_PATH_SEGMENTS: usize = 262_144;
const MAX_COORDINATE: f64 = 1_000_000.0;
const INK_PADDING_EM: f64 = 0.10;
pub(super) const TEXT_ANCHOR: &str = "x";

#[derive(Default)]
pub(super) struct MathRenderer {
    // Render-local and lazy: prose-only documents never parse the math fonts.
    engine: OnceCell<std::result::Result<Engine, String>>,
}

/// One independently filled primitive; its explicit colour travels with its
/// contours so the cached geometry never depends on the surrounding theme.
struct FormulaPath {
    contours: Vec<PathContour>,
    color: Option<Color>,
}

struct Formula {
    paths: Vec<FormulaPath>,
    bounds: Bounds,
    /// The layout box width: the advance an inline formula occupies.
    advance: f64,
}

impl MathRenderer {
    fn typeset(&self, source: &str) -> std::result::Result<Formula, String> {
        let engine = self
            .engine
            .get_or_init(|| Engine::bundled().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(Clone::clone)?;
        let mut formula = typeset_formula(engine, source, Style::Display)?;
        formula.bounds.left -= INK_PADDING_EM;
        formula.bounds.right += INK_PADDING_EM;
        formula.bounds.bottom -= INK_PADDING_EM;
        formula.bounds.top += INK_PADDING_EM;
        Ok(formula)
    }
}

/// Lay out `source` in `style` and resolve each glyph, rule and drawn path
/// to its own outline group, in ems with the baseline at y = 0. The bounds
/// hold the layout box and all ink, unpadded.
fn typeset_formula(
    engine: &Engine,
    source: &str,
    style: Style,
) -> std::result::Result<Formula, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SOURCE_LIMIT_REASON.to_string());
    }
    let mut layout = engine
        .typeset(source, style)
        .map_err(|error| error.to_string())?;
    if layout
        .glyphs
        .len()
        .saturating_add(layout.rules.len())
        .saturating_add(layout.paths.len())
        > MAX_PRIMITIVES
    {
        return Err("formula exceeds the 4096 primitive limit".to_string());
    }
    let mut bounds = Bounds::default();
    // Keep advance/phantom space as well as all actual ink. The control
    // hull includes italic overhangs, large delimiters and drawn radicals.
    bounds.add(0.0, -layout.depth)?;
    bounds.add(layout.width, layout.height)?;
    let mut paths = Vec::new();
    let mut segment_count = 0usize;
    let mut add_path =
        |contours: Vec<PathContour>, color: Option<Color>| -> std::result::Result<(), String> {
            for contour in &contours {
                segment_count = segment_count
                    .saturating_add(contour.segments.len())
                    .saturating_add(2);
                if segment_count > MAX_PATH_SEGMENTS {
                    return Err("formula exceeds the 262144 path segment limit".to_string());
                }
                bounds.add(contour.start.0, contour.start.1)?;
                for segment in &contour.segments {
                    match segment {
                        PathSeg::Line { to } => bounds.add(to.0, to.1)?,
                        PathSeg::Quad { ctrl, to } => {
                            bounds.add(ctrl.0, ctrl.1)?;
                            bounds.add(to.0, to.1)?;
                        }
                    }
                }
            }
            if !contours.is_empty() {
                paths.push(FormulaPath { contours, color });
            }
            Ok(())
        };
    // Separate paint operations preserve overlays with opposing contour
    // windings. A glyph's own contours stay together to preserve its holes.
    let explicitly_colored = layout.glyphs.iter().any(|glyph| glyph.color.is_some())
        || layout.rules.iter().any(|rule| rule.color.is_some())
        || layout.paths.iter().any(|path| path.color.is_some());
    let mut order = std::mem::take(&mut layout.order);
    if !explicitly_colored {
        // Keep historical bytes for uncoloured formulas, whose same-ink fills
        // commute. With colour, TeX's emission order determines which ink is
        // visible where a later glyph overlaps a fraction bar or radical.
        order.clear();
        order.extend((0..layout.glyphs.len()).map(Placed::Glyph));
        order.extend((0..layout.rules.len()).map(Placed::Rule));
        order.extend((0..layout.paths.len()).map(Placed::Path));
    }
    let mut primitive = Layout::default();
    for placed in order {
        match placed {
            Placed::Glyph(index) => {
                let glyph = &layout.glyphs[index];
                primitive.glyphs.push(glyph.clone());
                add_path(
                    fmd_math::paths::resolve_paths(engine, &primitive)
                        .map_err(|error| error.to_string())?,
                    glyph.color,
                )?;
                primitive.glyphs.clear();
            }
            Placed::Rule(index) => {
                let rule = &layout.rules[index];
                primitive.rules.push(rule.clone());
                add_path(
                    fmd_math::paths::resolve_paths(engine, &primitive)
                        .map_err(|error| error.to_string())?,
                    rule.color,
                )?;
                primitive.rules.clear();
            }
            Placed::Path(index) => {
                let path = &mut layout.paths[index];
                add_path(std::mem::take(&mut path.contours), path.color)?;
            }
        }
    }
    Ok(Formula {
        paths,
        bounds,
        advance: layout.width,
    })
}

/// An inline formula (fm-djcw): text-style layout in ems at font size 1, with
/// its origin at the left end of the baseline. It is set as one unbreakable
/// box of `advance` ems whose extents grow the line it sits on.
pub(super) struct InlineFormula {
    paths: Vec<FormulaPath>,
    /// The layout box width.
    pub(super) advance: f32,
    /// Horizontal extent of both the advance box and every outline control
    /// point. Fitting must include negative kerns and italic overhangs.
    pub(super) left: f32,
    pub(super) right: f32,
    /// Extent above the baseline: the layout height or the ink, whichever is taller.
    pub(super) ascent: f32,
    /// Extent below the baseline, likewise.
    pub(super) descent: f32,
}

/// Distinct source/style pairs and owned payload kept in the process-wide memo.
/// A full memo declines new entries without changing the computed formula.
const INLINE_MEMO_ENTRIES: usize = 4096;
const INLINE_MEMO_BYTES: usize = 32 * 1024 * 1024;

type InlineResult = std::result::Result<Arc<InlineFormula>, String>;

/// The payload budget counts owned string capacities, the formula itself and
/// every outline vector's capacity. Map storage, Arc headers and allocator
/// bookkeeping are excluded; the entry cap independently bounds the number of
/// cached results. Fonts and in-flight computations are not memo payload.
struct InlineMemo {
    by_style: [HashMap<String, InlineResult>; 2],
    retained_payload_bytes: usize,
    max_entries: usize,
    max_payload_bytes: usize,
}

impl InlineMemo {
    fn new(max_entries: usize, max_payload_bytes: usize) -> Self {
        Self {
            by_style: Default::default(),
            retained_payload_bytes: 0,
            max_entries,
            max_payload_bytes,
        }
    }

    fn get(&self, source: &str, display: bool) -> Option<&InlineResult> {
        self.by_style[usize::from(display)].get(source)
    }

    /// Called with the memo mutex held, including after a racing cache miss.
    /// Keep the first result rather than replacing it or charging it twice.
    fn insert_if_room(&mut self, source: &str, display: bool, result: &InlineResult) {
        if source.len() > MAX_SOURCE_BYTES
            || self.get(source, display).is_some()
            || self.by_style.iter().map(HashMap::len).sum::<usize>() >= self.max_entries
        {
            return;
        }
        let payload_bytes = match result {
            Ok(formula) => formula.retained_payload_bytes(),
            // Cloning an error can discard spare capacity. Check its length
            // first, then account for the actual owned clone below.
            Err(reason) => Some(reason.len()),
        };
        let Some(payload_bytes) = payload_bytes else {
            return;
        };
        let remaining = self.max_payload_bytes - self.retained_payload_bytes;
        let Some(minimum_bytes) = source.len().checked_add(payload_bytes) else {
            return;
        };
        if minimum_bytes > remaining {
            return;
        }
        // Do not copy a source or error that already cannot fit the budget.
        let key = source.to_owned();
        let stored = result.clone();
        let stored_payload_bytes = match &stored {
            Ok(_) => payload_bytes,
            Err(reason) => reason.capacity(),
        };
        let Some(bytes) = key.capacity().checked_add(stored_payload_bytes) else {
            return;
        };
        if bytes > remaining {
            return;
        }
        self.by_style[usize::from(display)].insert(key, stored);
        self.retained_payload_bytes += bytes;
    }
}

impl InlineFormula {
    fn retained_payload_bytes(&self) -> Option<usize> {
        let mut bytes = size_of::<Self>().checked_add(
            self.paths
                .capacity()
                .checked_mul(size_of::<FormulaPath>())?,
        )?;
        for path in &self.paths {
            bytes = bytes.checked_add(
                path.contours
                    .capacity()
                    .checked_mul(size_of::<PathContour>())?,
            )?;
            for contour in &path.contours {
                bytes = bytes.checked_add(
                    contour
                        .segments
                        .capacity()
                        .checked_mul(size_of::<PathSeg>())?,
                )?;
            }
        }
        Some(bytes)
    }
}

/// The inline layout of `source` in its authored text or display style,
/// memoized per process. Measurement, line metrics and drawing share the same
/// formula. An `Err` keeps the complete visible TeX fallback. Separate maps
/// allow borrowed source lookups without allocating a composite cache key.
pub(super) fn inline_formula(source: &str, display: bool) -> InlineResult {
    // Refuse before any memo lookup or engine initialization. In particular,
    // never retain a rejected, arbitrarily large source as a cached error key.
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SOURCE_LIMIT_REASON.to_string());
    }
    static ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();
    static MEMO: OnceLock<Mutex<InlineMemo>> = OnceLock::new();
    let memo =
        MEMO.get_or_init(|| Mutex::new(InlineMemo::new(INLINE_MEMO_ENTRIES, INLINE_MEMO_BYTES)));
    if let Some(hit) = memo
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(source, display)
    {
        return hit.clone();
    }
    let result = ENGINE
        .get_or_init(|| Engine::bundled().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|engine| {
            typeset_formula(
                engine,
                source,
                if display { Style::Display } else { Style::Text },
            )
        })
        .map(|formula| {
            Arc::new(InlineFormula {
                paths: formula.paths,
                advance: formula.advance as f32,
                left: formula.bounds.left as f32,
                right: formula.bounds.right as f32,
                ascent: formula.bounds.top as f32,
                descent: (-formula.bounds.bottom) as f32,
            })
        });
    memo.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert_if_room(source, display, &result);
    result
}

/// Fill `formula`'s outlines at `size` points with the left end of its
/// baseline at `(x, baseline)`. Primitives inherit the current fill colour
/// unless the source explicitly overrides it. Each glyph, rule and drawn
/// path is its own nonzero fill so overlays keep their own
/// windings; quadratic segments become the exact equivalent cubics.
pub(super) fn append_inline_formula(
    out: &mut String,
    formula: &InlineFormula,
    x: f32,
    baseline: f32,
    size: f32,
) {
    let scale = f64::from(size);
    let (ox, oy) = (f64::from(x), f64::from(baseline));
    let point = |out: &mut String, (px, py): (f64, f64)| {
        super::append_pdf_num(out, (ox + px * scale) as f32);
        out.push(' ');
        super::append_pdf_num(out, (oy + py * scale) as f32);
        out.push(' ');
    };
    for path in &formula.paths {
        if let Some(color) = path.color {
            // Restore the caller's current fill, including link/quote ink,
            // before the next uncoloured primitive or surrounding prose.
            out.push_str("q\n");
            super::append_rgb_fill_operator(out, (color.r as f32, color.g as f32, color.b as f32));
        }
        for contour in &path.contours {
            let mut current = contour.start;
            point(out, current);
            out.push_str("m\n");
            for segment in &contour.segments {
                match *segment {
                    PathSeg::Line { to } => {
                        point(out, to);
                        out.push_str("l\n");
                        current = to;
                    }
                    PathSeg::Quad { ctrl, to } => {
                        let toward = |from: (f64, f64)| {
                            (
                                from.0 + 2.0 / 3.0 * (ctrl.0 - from.0),
                                from.1 + 2.0 / 3.0 * (ctrl.1 - from.1),
                            )
                        };
                        point(out, toward(current));
                        point(out, toward(to));
                        point(out, to);
                        out.push_str("c\n");
                        current = to;
                    }
                }
            }
            out.push_str("h\n");
        }
        out.push_str("f\n");
        if path.color.is_some() {
            out.push_str("Q\n");
        }
    }
}

/// Return false only when the caller should preserve the visible TeX fallback.
pub(super) fn layout_block(
    source: &str,
    indent: f32,
    out: &mut Vec<Line>,
    cx: &mut LayoutCx<'_>,
) -> bool {
    let Ok(formula) = cx.math_renderer.typeset(source) else {
        return false;
    };
    let width_em = formula.bounds.right - formula.bounds.left;
    let height_em = formula.bounds.top - formula.bounds.bottom;
    let available_width = (cx.page.content_w - indent).max(1.0);
    let available_height = (cx.page.top_y() - cx.page.bottom).max(1.0);
    let size = f64::from(positive_finite(cx.type_scale.body, 11.0))
        .min(f64::from(available_width) / width_em)
        .min(f64::from(available_height) / height_em);
    let ink_width = (width_em * size) as f32;
    let ink_height = (height_em * size) as f32;
    // PDF's SVG transform has a one-unit view-box floor and rounds matrix
    // coefficients. Store final point coordinates in an at-least-one-point
    // box so the transform is exactly 1, even for extremely wide equations.
    let width = ink_width.max(1.0);
    let height = ink_height.max(1.0);
    let mut elements = Vec::with_capacity(formula.paths.len());
    let color = Palette::from_colors(&cx.opts.theme.colors).fg;
    let point = |(x, y): (f64, f64)| {
        (
            ((x - formula.bounds.left) * size) as f32 + (width - ink_width) * 0.5,
            ((formula.bounds.top - y) * size) as f32 + (height - ink_height) * 0.5,
        )
    };
    for path in formula.paths {
        let color = path.color.map_or(color, |color| {
            (color.r as f32, color.g as f32, color.b as f32)
        });
        let mut ops = Vec::new();
        for contour in path.contours {
            let (x, y) = point(contour.start);
            ops.push(SvgPathOp::Move(x, y));
            for segment in contour.segments {
                match segment {
                    PathSeg::Line { to } => {
                        let (x, y) = point(to);
                        ops.push(SvgPathOp::Line(x, y));
                    }
                    PathSeg::Quad { ctrl, to } => {
                        let (cx, cy) = point(ctrl);
                        let (x, y) = point(to);
                        ops.push(SvgPathOp::Quad(cx, cy, x, y));
                    }
                }
            }
            ops.push(SvgPathOp::Close);
        }
        elements.push(SvgElement::Path(SvgPath {
            ops,
            style: SvgStyle {
                fill: Some(color),
                color,
                ..SvgStyle::INITIAL
            },
            marker_start: None,
            marker_mid: None,
            marker_end: None,
            link: None,
        }));
    }
    let image = PdfImageData {
        key: String::new(), // Pure vectors do not enter the image XObject map.
        width_px: width.ceil().max(1.0) as u32,
        height_px: height.ceil().max(1.0) as u32,
        vector: Some(PdfSvgImage {
            view_box: SvgViewBox {
                x: 0.0,
                y: 0.0,
                w: width,
                h: height,
            },
            viewport: SvgViewport {
                w: width,
                h: height,
            },
            preserve_aspect: SvgPreserveAspectRatio::DEFAULT,
            root_background: None,
            accessible_text: Some(source.to_string()),
            elements,
            gradients: Vec::new(),
            patterns: Vec::new(),
            clip_paths: Vec::new(),
            markers: Vec::new(),
        }),
        color: PdfImageColor::Rgb,
        data: Vec::new(),
        filter: PdfImageStreamFilter::Flate,
        smask: None,
    };
    gap(out, 5.0);
    out.push(Line {
        size: (height / 1.32).max(1.0),
        gap_after: 9.0,
        rule: false,
        rule_x: cx.page.left + indent + (available_width - width).max(0.0) / 2.0,
        quote_bars: Vec::new(),
        bg: 0,
        shade: false,
        flow: FlowMark {
            group: cx.alloc_flow(),
            index: 0,
            count: 1,
            kind: FlowKind::Image,
            list_start: false,
            heading_level: 0,
            note: 0,
            note_destination: 0,
        },
        page_break_before: false,
        list_path: Vec::new(),
        table_cols: Vec::new(),
        table_source_row: None,
        table_footing: Vec::new(),
        heading_source: None,
        text_ink: (0.0, 0.0),
        grid: None,
        hyphenated: false,
        segs: Vec::new(),
        image: Some(ImageLine {
            image,
            alt: source.to_string(),
            formula: true,
            link: None,
            marker_size: None,
            width_pt: width,
            height_pt: height,
        }),
    });
    true
}

pub(super) fn collect_warnings<'a>(
    blocks: &'a [Block],
    warnings: &mut Vec<RenderWarning>,
) -> BTreeSet<&'a str> {
    let renderer = MathRenderer::default();
    let mut supported = BTreeSet::new();
    let mut pending: Vec<&[Block]> = vec![blocks];
    while let Some(blocks) = pending.pop() {
        for block in blocks {
            match block {
                Block::MathBlock(source) => {
                    if let Err(reason) = renderer.typeset(source) {
                        warnings.push(RenderWarning::MathFallback {
                            source: source.chars().take(120).collect(),
                            reason,
                        });
                    } else {
                        supported.insert(source.as_str());
                    }
                }
                Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                    pending.push(inner);
                }
                Block::List(list) => {
                    pending.extend(list.items.iter().rev().map(|item| item.blocks.as_slice()));
                }
                _ => {}
            }
        }
    }
    supported
}

/// Report every inline `$…$` the inline lane cannot typeset (it keeps its
/// visible TeX source), walking the same blocks the text scan does. Each
/// distinct source/style pair is reported once. Successful formulas return true
/// so the caller can collect fitting diagnostics from their actual final layout.
pub(super) fn collect_inline_warnings(blocks: &[Block], warnings: &mut Vec<RenderWarning>) -> bool {
    fn walk_inlines<'a>(items: &'a [super::Inline], sources: &mut Vec<(&'a str, bool)>) {
        for item in items {
            match item {
                super::Inline::Math(source) => sources.push((source, false)),
                super::Inline::DisplayMath(source) => sources.push((source, true)),
                super::Inline::Strong(inner)
                | super::Inline::Emphasis(inner)
                | super::Inline::Strikethrough(inner) => walk_inlines(inner, sources),
                super::Inline::Link { content, .. } => walk_inlines(content, sources),
                _ => {}
            }
        }
    }
    fn walk_blocks<'a>(items: &'a [Block], sources: &mut Vec<(&'a str, bool)>) {
        for block in items {
            match block {
                Block::Heading { inlines: text, .. } | Block::Paragraph(text) => {
                    walk_inlines(text, sources);
                }
                Block::BlockQuote(inner) | Block::FootnoteDefinition { blocks: inner, .. } => {
                    walk_blocks(inner, sources);
                }
                Block::List(list) => {
                    for item in &list.items {
                        walk_blocks(&item.blocks, sources);
                    }
                }
                Block::DefinitionList(items) => {
                    for item in items {
                        for term in &item.terms {
                            walk_inlines(term, sources);
                        }
                        for definition in &item.definitions {
                            walk_inlines(definition, sources);
                        }
                    }
                }
                Block::Table(table) => {
                    for cell in table.head.iter().chain(table.rows.iter().flatten()) {
                        walk_inlines(cell, sources);
                    }
                }
                _ => {}
            }
        }
    }
    let mut sources = Vec::new();
    walk_blocks(blocks, &mut sources);
    let mut seen = BTreeSet::new();
    let mut supported = false;
    for (source, display) in sources {
        if !seen.insert((source, display)) {
            continue;
        }
        match inline_formula(source, display) {
            Err(reason) => warnings.push(RenderWarning::MathFallback {
                source: source.chars().take(120).collect(),
                reason,
            }),
            Ok(_) => supported = true,
        }
    }
    supported
}

/// PDF extractors require a text-showing operator before they honor
/// `/ActualText` on a vector formula. One invisible embedded glyph anchors the
/// source replacement inside the formula box; it contributes no visible ink.
pub(super) fn append_text_anchor(
    body: &mut String,
    image: &ImageLine,
    x: f32,
    y: f32,
    subsets: &[super::EmbeddedFace<'_>],
    lookup: &super::EmbeddedFaceLookup,
    faces: &super::Faces,
) {
    let size = image.width_pt.min(image.height_pt).max(0.01);
    append_anchor(
        body,
        &image.alt,
        x,
        y + image.height_pt * 0.2,
        size,
        subsets,
        lookup,
        faces,
    );
}

/// The same anchor for an inline formula, on its own baseline at body size.
#[allow(clippy::too_many_arguments)]
pub(super) fn append_inline_anchor(
    body: &mut String,
    source: &str,
    x: f32,
    baseline: f32,
    size: f32,
    subsets: &[super::EmbeddedFace<'_>],
    lookup: &super::EmbeddedFaceLookup,
    faces: &super::Faces,
) {
    append_anchor(
        body,
        source,
        x,
        baseline,
        size.max(0.01),
        subsets,
        lookup,
        faces,
    );
}

#[allow(clippy::too_many_arguments)]
fn append_anchor(
    body: &mut String,
    alt: &str,
    x: f32,
    y: f32,
    size: f32,
    subsets: &[super::EmbeddedFace<'_>],
    lookup: &super::EmbeddedFaceLookup,
    faces: &super::Faces,
) {
    let Some(face) = lookup.get(subsets, super::F_BODY) else {
        return;
    };
    let source = faces.get(super::F_BODY);
    let glyph = source.glyph_index('x');
    // Poppler and other readers recognize replacement text on /Span. The
    // enclosing /Formula owns the MCID and /Alt; this span is its text content.
    body.push_str("/Span <</ActualText ");
    body.push_str(&pdf_text_string(alt));
    body.push_str(">> BDC\n");
    super::append_text_segment_operator_with_render_mode(
        body,
        super::F_BODY,
        size,
        x,
        y,
        &face.map_lookup,
        source,
        face.kern,
        &[glyph],
        None,
        Some(3),
        0,
    );
    body.push_str("EMC\n");
}

#[derive(Default)]
struct Bounds {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
}

impl Bounds {
    fn add(&mut self, x: f64, y: f64) -> std::result::Result<(), String> {
        if ![x, y]
            .iter()
            .all(|value| value.is_finite() && value.abs() <= MAX_COORDINATE)
        {
            return Err("formula contains non-finite or excessive coordinates".to_string());
        }
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.bottom = self.bottom.min(y);
        self.top = self.top.max(y);
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod memo_tests {
    use super::*;

    fn outlined_formula(
        path_capacity: usize,
        contour_capacity: usize,
        segment_capacity: usize,
    ) -> Arc<InlineFormula> {
        let mut segments = Vec::with_capacity(segment_capacity);
        segments.push(PathSeg::Line { to: (1.0, 0.0) });
        segments.push(PathSeg::Quad {
            ctrl: (1.0, 1.0),
            to: (0.0, 1.0),
        });
        let mut contours = Vec::with_capacity(contour_capacity);
        contours.push(PathContour {
            start: (0.0, 0.0),
            segments,
        });
        let mut paths = Vec::with_capacity(path_capacity);
        paths.push(FormulaPath {
            contours,
            color: None,
        });
        Arc::new(InlineFormula {
            paths,
            advance: 1.0,
            left: 0.0,
            right: 1.0,
            ascent: 1.0,
            descent: 0.0,
        })
    }

    #[test]
    fn memo_admission_respects_exact_payload_boundary() {
        let formula = outlined_formula(1, 1, 2);
        let source = "x";
        let bytes = source.to_owned().capacity()
            + size_of::<InlineFormula>()
            + formula.paths.capacity() * size_of::<FormulaPath>()
            + formula.paths[0].contours.capacity() * size_of::<PathContour>()
            + formula.paths[0].contours[0].segments.capacity() * size_of::<PathSeg>();
        let result = Ok(Arc::clone(&formula));
        let mut too_small = InlineMemo::new(4, bytes - 1);
        too_small.insert_if_room(source, false, &result);
        assert!(too_small.get(source, false).is_none());
        assert_eq!(too_small.retained_payload_bytes, 0);
        assert_eq!(Arc::strong_count(&formula), 2);

        let mut exact = InlineMemo::new(4, bytes);
        exact.insert_if_room(source, false, &result);
        let retained = exact.get(source, false).unwrap().as_ref().unwrap();
        assert!(Arc::ptr_eq(retained, &formula));
        assert_eq!(exact.retained_payload_bytes, bytes);
        assert_eq!(Arc::strong_count(&formula), 3);
        exact.insert_if_room("y", false, &result);
        assert!(exact.get("y", false).is_none());
        assert_eq!(exact.retained_payload_bytes, bytes);
    }

    #[test]
    fn memo_charges_spare_capacity_at_every_outline_level() {
        // Each formula has the same one path, one contour and two segments,
        // but just one of its three owned vectors has substantial spare room.
        let length_only_bytes = 1
            + size_of::<InlineFormula>()
            + size_of::<FormulaPath>()
            + size_of::<PathContour>()
            + 2 * size_of::<PathSeg>();
        for formula in [
            outlined_formula(32, 1, 2),
            outlined_formula(1, 32, 2),
            outlined_formula(1, 1, 64),
        ] {
            let result = Ok(Arc::clone(&formula));
            let mut memo = InlineMemo::new(4, length_only_bytes);
            memo.insert_if_room("x", false, &result);
            assert!(memo.get("x", false).is_none());
            assert_eq!(memo.retained_payload_bytes, 0);
            assert_eq!(Arc::strong_count(&formula), 2);
        }
    }

    #[test]
    fn memo_charges_owned_error_and_key_storage() {
        let mut reason = String::with_capacity(4096);
        reason.push_str("unsupported command");
        let bytes = "bad".to_owned().capacity() + reason.clone().capacity();
        assert!(reason.capacity() > bytes);
        let result = Err(reason);
        let mut too_small = InlineMemo::new(4, bytes - 1);
        too_small.insert_if_room("bad", false, &result);
        assert!(too_small.get("bad", false).is_none());
        assert_eq!(too_small.retained_payload_bytes, 0);

        let mut exact = InlineMemo::new(4, bytes);
        exact.insert_if_room("bad", false, &result);
        let (key, stored) = exact.by_style[0].iter().next().unwrap();
        let Err(stored_reason) = stored else {
            panic!("the cached result must preserve the error");
        };
        assert_eq!(stored_reason, "unsupported command");
        assert_eq!(exact.retained_payload_bytes, bytes);
        assert_eq!(bytes, key.capacity() + stored_reason.capacity());
    }

    #[test]
    fn memo_never_retains_oversized_rejected_source() {
        let source = "x".repeat(MAX_SOURCE_BYTES + 1);
        let result = Err(SOURCE_LIMIT_REASON.to_string());
        let mut memo = InlineMemo::new(4, usize::MAX);
        for display in [false, true] {
            assert!(matches!(
                inline_formula(&source, display),
                Err(reason) if reason == SOURCE_LIMIT_REASON
            ));
            memo.insert_if_room(&source, display, &result);
            assert!(memo.get(&source, display).is_none());
        }
        assert_eq!(memo.retained_payload_bytes, 0);
    }

    #[test]
    fn memo_duplicate_admission_keeps_original_result_and_style_identity() {
        let first = outlined_formula(1, 1, 2);
        let second = outlined_formula(2, 2, 4);
        let first_result = Ok(Arc::clone(&first));
        let second_result = Ok(Arc::clone(&second));
        let mut memo = InlineMemo::new(4, usize::MAX);
        memo.insert_if_room("x", false, &first_result);
        let first_bytes = memo.retained_payload_bytes;

        // Two callers can compute the same miss before either takes the lock.
        // The second insertion must preserve the winner and its accounting.
        memo.insert_if_room("x", false, &second_result);
        assert_eq!(memo.retained_payload_bytes, first_bytes);
        assert!(Arc::ptr_eq(
            memo.get("x", false).unwrap().as_ref().unwrap(),
            &first
        ));
        assert_eq!(Arc::strong_count(&second), 2);

        memo.insert_if_room("x", true, &second_result);
        assert!(Arc::ptr_eq(
            memo.get("x", true).unwrap().as_ref().unwrap(),
            &second
        ));
        assert_eq!(memo.by_style.iter().map(HashMap::len).sum::<usize>(), 2);
        assert_eq!(
            memo.retained_payload_bytes,
            first_bytes + "x".to_owned().capacity() + second.retained_payload_bytes().unwrap()
        );
    }

    #[test]
    fn memo_entry_limit_is_shared_between_styles() {
        let mut memo = InlineMemo::new(2, usize::MAX);
        let result = Err("unsupported command".to_string());
        memo.insert_if_room("a", false, &result);
        memo.insert_if_room("a", true, &result);
        let bytes = memo.retained_payload_bytes;
        memo.insert_if_room("b", false, &result);
        memo.insert_if_room("b", true, &result);
        assert!(memo.get("a", false).is_some());
        assert!(memo.get("a", true).is_some());
        assert!(memo.get("b", false).is_none());
        assert!(memo.get("b", true).is_none());
        assert_eq!(memo.retained_payload_bytes, bytes);
    }

    #[test]
    fn memo_refusal_preserves_paint_and_releases_unretained_geometry() {
        let formula = outlined_formula(1, 1, 2);
        let weak = Arc::downgrade(&formula);
        let result = Ok(Arc::clone(&formula));
        let mut expected = String::new();
        append_inline_formula(&mut expected, &formula, 10.0, 20.0, 12.0);
        for mut memo in [InlineMemo::new(4, 0), InlineMemo::new(0, usize::MAX)] {
            memo.insert_if_room("x", false, &result);
            assert!(memo.get("x", false).is_none());
            let mut actual = String::new();
            append_inline_formula(&mut actual, result.as_ref().unwrap(), 10.0, 20.0, 12.0);
            assert_eq!(actual, expected);
            assert_eq!(Arc::strong_count(&formula), 2);
        }
        drop(result);
        drop(formula);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn memo_releases_admitted_geometry_when_dropped() {
        let formula = outlined_formula(1, 1, 2);
        let weak = Arc::downgrade(&formula);
        let result = Ok(Arc::clone(&formula));
        let mut memo = InlineMemo::new(4, usize::MAX);
        memo.insert_if_room("x", false, &result);
        drop(result);
        drop(formula);
        assert!(weak.upgrade().is_some());
        drop(memo);
        assert!(weak.upgrade().is_none());
    }
}
