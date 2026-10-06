//! Display equations rendered from the shared math engine's real outlines.
//!
//! Math uses its own bundled face roster, so outlined glyphs keep their exact
//! metrics without adding a second font-subsetting protocol to the PDF writer.
//! The existing vector-image lane supplies pagination, clipping, and resources;
//! `/Formula`, `/Alt`, and `/ActualText` retain the source for assistive readers.

use std::cell::OnceCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use fmd_math::{Engine, Layout, PathContour, PathSeg, Style};

use super::{
    Block, FlowKind, FlowMark, ImageLine, LayoutCx, Line, Palette, PdfImageColor, PdfImageData,
    PdfImageStreamFilter, PdfSvgImage, RenderWarning, SvgElement, SvgPath, SvgPathOp,
    SvgPreserveAspectRatio, SvgStyle, SvgViewBox, SvgViewport, gap, pdf_text_string,
    positive_finite,
};

const MAX_SOURCE_BYTES: usize = 64 * 1024;
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

struct Formula {
    paths: Vec<Vec<PathContour>>,
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
        return Err("formula exceeds the 64 KiB source limit".to_string());
    }
    let layout = engine
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
    let mut add_path = |contours: Vec<PathContour>| -> std::result::Result<(), String> {
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
            paths.push(contours);
        }
        Ok(())
    };
    // Separate paint operations preserve overlays with opposing contour
    // windings. A glyph's own contours stay together to preserve its holes.
    let mut primitive = Layout::default();
    for glyph in layout.glyphs {
        primitive.glyphs.push(glyph);
        add_path(
            fmd_math::paths::resolve_paths(engine, &primitive)
                .map_err(|error| error.to_string())?,
        )?;
        primitive.glyphs.clear();
    }
    for rule in layout.rules {
        primitive.rules.push(rule);
        add_path(
            fmd_math::paths::resolve_paths(engine, &primitive)
                .map_err(|error| error.to_string())?,
        )?;
        primitive.rules.clear();
    }
    for path in layout.paths {
        add_path(path.contours)?;
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
    paths: Vec<Vec<PathContour>>,
    /// The layout box width.
    pub(super) advance: f32,
    /// Extent above the baseline: the layout height or the ink, whichever is taller.
    pub(super) ascent: f32,
    /// Extent below the baseline, likewise.
    pub(super) descent: f32,
}

/// Distinct inline sources kept in the process-wide memo; beyond it, layouts
/// are recomputed rather than stored.
const INLINE_MEMO_ENTRIES: usize = 4096;

/// The inline layout of `source`, memoized per process. fmd-math layout is a
/// pure function of the source, so measurement, line metrics and drawing all
/// see the same formula without any of them threading it through. An `Err`
/// is the reason the source keeps its visible TeX fallback.
pub(super) fn inline_formula(source: &str) -> std::result::Result<Arc<InlineFormula>, String> {
    type Memo = Mutex<HashMap<String, std::result::Result<Arc<InlineFormula>, String>>>;
    static ENGINE: OnceLock<std::result::Result<Engine, String>> = OnceLock::new();
    static MEMO: OnceLock<Memo> = OnceLock::new();
    let memo = MEMO.get_or_init(Memo::default);
    if let Some(hit) = memo
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(source)
    {
        return hit.clone();
    }
    let result = ENGINE
        .get_or_init(|| Engine::bundled().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|engine| typeset_formula(engine, source, Style::Text))
        .map(|formula| {
            Arc::new(InlineFormula {
                paths: formula.paths,
                advance: formula.advance as f32,
                ascent: formula.bounds.top as f32,
                descent: (-formula.bounds.bottom) as f32,
            })
        });
    let mut memo = memo.lock().unwrap_or_else(PoisonError::into_inner);
    if memo.len() < INLINE_MEMO_ENTRIES {
        memo.insert(source.to_owned(), result.clone());
    }
    result
}

/// Fill `formula`'s outlines at `size` points with the left end of its
/// baseline at `(x, baseline)`, in the current fill colour. Each glyph, rule
/// and drawn path is its own nonzero fill so overlays keep their own
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
    for contours in &formula.paths {
        for contour in contours {
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
    for contours in formula.paths {
        let mut ops = Vec::new();
        for contour in contours {
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
        },
        page_break_before: false,
        list_path: Vec::new(),
        table_cols: Vec::new(),
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
/// visible TeX source), and every one wider than the text `measure` at
/// `body_size` (it never splits, so it overflows), walking the same blocks
/// the text scan does. Each distinct source is reported once.
pub(super) fn collect_inline_warnings(
    blocks: &[Block],
    body_size: f32,
    measure: f32,
    warnings: &mut Vec<RenderWarning>,
) {
    fn walk_inlines<'a>(items: &'a [super::Inline], sources: &mut Vec<&'a str>) {
        for item in items {
            match item {
                super::Inline::Math(source) | super::Inline::DisplayMath(source) => {
                    sources.push(source);
                }
                super::Inline::Strong(inner)
                | super::Inline::Emphasis(inner)
                | super::Inline::Strikethrough(inner) => walk_inlines(inner, sources),
                super::Inline::Link { content, .. } => walk_inlines(content, sources),
                _ => {}
            }
        }
    }
    fn walk_blocks<'a>(items: &'a [Block], sources: &mut Vec<&'a str>) {
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
    for source in sources {
        if !seen.insert(source) {
            continue;
        }
        match inline_formula(source) {
            Err(reason) => warnings.push(RenderWarning::MathFallback {
                source: source.chars().take(120).collect(),
                reason,
            }),
            Ok(formula) if formula.advance * body_size > measure => {
                warnings.push(RenderWarning::MathOverflow {
                    source: source.chars().take(120).collect(),
                    width_pt: (formula.advance * body_size).ceil() as u32,
                    measure_pt: measure.floor() as u32,
                });
            }
            Ok(_) => {}
        }
    }
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
