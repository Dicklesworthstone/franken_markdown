//! Display equations rendered from the shared math engine's real outlines.
//!
//! Math uses its own bundled face roster, so outlined glyphs keep their exact
//! metrics without adding a second font-subsetting protocol to the PDF writer.
//! The existing vector-image lane supplies pagination, clipping, and resources;
//! `/Formula`, `/Alt`, and `/ActualText` retain the source for assistive readers.

use std::cell::OnceCell;
use std::collections::BTreeSet;

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
}

impl MathRenderer {
    fn typeset(&self, source: &str) -> std::result::Result<Formula, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err("formula exceeds the 64 KiB source limit".to_string());
        }
        let engine = self
            .engine
            .get_or_init(|| Engine::bundled().map_err(|error| error.to_string()))
            .as_ref()
            .map_err(Clone::clone)?;
        let layout = engine
            .typeset(source, Style::Display)
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
        bounds.left -= INK_PADDING_EM;
        bounds.right += INK_PADDING_EM;
        bounds.bottom -= INK_PADDING_EM;
        bounds.top += INK_PADDING_EM;
        Ok(Formula { paths, bounds })
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
    let Some(face) = lookup.get(subsets, super::F_BODY) else {
        return;
    };
    let source = faces.get(super::F_BODY);
    let glyph = source.glyph_index('x');
    let size = image.width_pt.min(image.height_pt).max(0.01);
    // Poppler and other readers recognize replacement text on /Span. The
    // enclosing /Formula owns the MCID and /Alt; this span is its text content.
    body.push_str("/Span <</ActualText ");
    body.push_str(&pdf_text_string(&image.alt));
    body.push_str(">> BDC\n");
    super::append_text_segment_operator_with_render_mode(
        body,
        super::F_BODY,
        size,
        x,
        y + image.height_pt * 0.2,
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
