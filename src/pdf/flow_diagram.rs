//! The first-party diagram compiler's vector output as a bounded Flow scene.
//!
//! This is deliberately not an SVG display API. The only production input is
//! diagram source, compiled by `diagrams::figure_svg`. Reusing the PDF parser
//! keeps its CSS, rounded corners, arcs and marker geometry consistent with
//! the other outputs. The bridge admits only the compiler's drawing profile;
//! any missing primitive or unsupported paint refuses the complete figure.

use super::{
    F_BODY, F_BOLD, PdfSvgImage, SvgDominantBaseline, SvgElement, SvgFillRule, SvgLineCap,
    SvgLineJoin, SvgMarkerPlacement, SvgMarkerRef, SvgPaintOrder, SvgPathOp, SvgStyle, SvgText,
    SvgTextAnchor, SvgTransform, SvgWhiteSpace, normalize_svg_text_for_style, parse_svg_document,
    svg_effective_fill_opacity, svg_effective_stroke_opacity, svg_ellipse_path_ops,
    svg_marker_orient_vector, svg_marker_paint, svg_marker_view_box_transform,
    svg_path_end_tangent, svg_path_start_tangent, svg_poly_path_ops, svg_rect_path_ops,
    svg_style_with_marker_context, svg_tag_parts, transform_svg_path_ops,
};
use crate::display::{
    DisplayPathCommand, DisplayRect, MAX_DISPLAY_PATH_COMMANDS, MAX_DISPLAY_TOTAL_PATH_COMMANDS,
};
use std::borrow::Cow;

const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;
const MAX_ITEMS: usize = 4096;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_COORDINATE: f32 = 1_000_000.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Scene {
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item {
    Path(Path),
    Text(Text),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Path {
    /// Complete control hull, expanded conservatively for the stroke's caps
    /// and joins. Coordinates have their SVG meaning: positive y is downward.
    pub bounds: DisplayRect,
    pub commands: Vec<DisplayPathCommand>,
    pub fill: Option<[f32; 4]>,
    pub stroke: Option<Stroke>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Stroke {
    pub color: [f32; 4],
    pub width: f32,
    pub dash: Vec<f32>,
    pub dash_offset: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Text {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub bold: bool,
    pub anchor: Anchor,
    pub baseline: Baseline,
    pub fill: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Baseline {
    Alphabetic,
    Middle,
}

impl From<LineCap> for crate::display::DisplayLineCap {
    fn from(value: LineCap) -> Self {
        match value {
            LineCap::Butt => Self::Butt,
            LineCap::Round => Self::Round,
            LineCap::Square => Self::Square,
        }
    }
}

impl From<LineJoin> for crate::display::DisplayLineJoin {
    fn from(value: LineJoin) -> Self {
        match value {
            LineJoin::Miter => Self::Miter,
            LineJoin::Round => Self::Round,
            LineJoin::Bevel => Self::Bevel,
        }
    }
}

/// Compile a complete supported Mermaid diagram into a native vector scene.
/// Source and geometry refusals are recoverable: the caller retains the
/// original fenced block. No SVG, image decoder or ambient resource escapes.
pub(crate) fn compile(source: &str, language: &str) -> Result<Scene, &'static str> {
    if language.len() > 128 {
        return Err("diagram language exceeds its 128 byte limit");
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err("diagram exceeds the 64 KiB source limit");
    }
    let (svg, _) = crate::diagrams::figure_svg(source, language)
        .ok_or("diagram syntax is outside the supported compiler profile")?;
    scene_from_svg(&svg)
}

fn scene_from_svg(svg: &str) -> Result<Scene, &'static str> {
    if svg.len() > MAX_SVG_BYTES {
        return Err("compiled diagram exceeds the 4 MiB vector limit");
    }
    let inventory = generated_inventory(svg)?;
    let image = parse_svg_document(svg).ok_or("compiled diagram has invalid vector geometry")?;
    if image.elements.len() != inventory.elements
        || image.markers.len() != inventory.markers
        || image
            .markers
            .iter()
            .map(|marker| marker.shapes.len())
            .sum::<usize>()
            != inventory.marker_shapes
    {
        return Err("compiled diagram contains an unrepresented primitive");
    }
    if !image.gradients.is_empty()
        || !image.patterns.is_empty()
        || !image.clip_paths.is_empty()
        || image.root_background.is_some()
        || image.view_box.x != 0.0
        || image.view_box.y != 0.0
        || image.view_box.w != image.viewport.w
        || image.view_box.h != image.viewport.h
        || !valid_positive(image.view_box.w)
        || !valid_positive(image.view_box.h)
    {
        return Err("compiled diagram uses an unsupported viewport or resource");
    }
    let mut builder = Builder {
        image: &image,
        items: Vec::new(),
        commands: 0,
        text_bytes: 0,
    };
    for element in &image.elements {
        builder.element(element)?;
    }
    Ok(Scene {
        width: image.view_box.w,
        height: image.view_box.h,
        items: builder.items,
    })
}

#[derive(Default)]
struct Inventory {
    elements: usize,
    markers: usize,
    marker_shapes: usize,
}

/// The compiler emits this small, closed XML vocabulary. Check it before the
/// more permissive PDF SVG parser, and count visible primitives independently
/// so its graceful omission of a malformed element cannot lose diagram data.
fn generated_inventory(svg: &str) -> Result<Inventory, &'static str> {
    let mut inventory = Inventory::default();
    let mut stack = Vec::new();
    let mut roots = 0;
    let mut position = 0;
    let preserve = svg.contains("class=\"fmd-diagram fmd-sequence\"")
        || svg.contains("class=\"fmd-diagram fmd-class\"")
        || svg.contains("class=\"fmd-diagram fmd-er\"");
    while let Some(relative) = svg[position..].find('<') {
        let start = position + relative;
        let end = start + svg[start..].find('>').ok_or("incomplete diagram tag")?;
        let raw = svg[start + 1..end].trim();
        position = end + 1;
        if let Some(closing) = raw.strip_prefix('/') {
            if stack.pop() != Some(closing.trim()) {
                return Err("unbalanced diagram structure");
            }
            continue;
        }
        let (tag, _) = svg_tag_parts(raw);
        match tag {
            "svg" if stack.is_empty() && roots == 0 => {
                roots += 1;
                stack.push(tag);
            }
            "defs" if stack.as_slice() == ["svg"] => stack.push(tag),
            "marker" if stack.as_slice() == ["svg", "defs"] => {
                inventory.markers += 1;
                stack.push(tag);
            }
            "style" | "title" | "desc" | "text" => {
                if stack.is_empty()
                    || (tag == "text" && stack.as_slice() != ["svg"])
                    || (matches!(tag, "title" | "desc") && stack.as_slice() != ["svg"])
                {
                    return Err("unsupported diagram text structure");
                }
                let closing = format!("</{tag}>");
                let end = position
                    + svg[position..]
                        .find(&closing)
                        .ok_or("incomplete diagram text element")?;
                let body = &svg[position..end];
                if body.contains('<') {
                    return Err("nested diagram text markup is outside the drawing profile");
                }
                if tag == "text"
                    && !normalize_svg_text_for_style(
                        body,
                        if preserve {
                            SvgWhiteSpace::Preserve
                        } else {
                            SvgWhiteSpace::Collapse
                        },
                    )
                    .is_empty()
                {
                    inventory.elements += 1;
                }
                position = end + closing.len();
            }
            "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path"
                if raw.ends_with('/') =>
            {
                if stack.as_slice() == ["svg"] {
                    inventory.elements += 1;
                } else if stack.as_slice() == ["svg", "defs", "marker"] {
                    inventory.marker_shapes += 1;
                } else {
                    return Err("unsupported diagram shape structure");
                }
            }
            _ => return Err("compiled diagram uses an unsupported vector element"),
        }
        if inventory.elements > MAX_ITEMS || inventory.marker_shapes > MAX_ITEMS {
            return Err("diagram exceeds the 4096 primitive limit");
        }
    }
    if roots != 1 || !stack.is_empty() || inventory.elements == 0 {
        return Err("incomplete diagram vector document");
    }
    Ok(inventory)
}

struct Builder<'a> {
    image: &'a PdfSvgImage,
    items: Vec<Item>,
    commands: usize,
    text_bytes: usize,
}

impl Builder<'_> {
    fn element(&mut self, element: &SvgElement) -> Result<(), &'static str> {
        let (ops, style, start, end) = match element {
            SvgElement::Rect(rect) => {
                if rect.link.is_some() {
                    return Err("compiled diagram contains an interactive shape");
                }
                (
                    Cow::Owned(svg_rect_path_ops(
                        rect.x, rect.y, rect.w, rect.h, rect.rx, rect.ry,
                    )),
                    rect.style,
                    None,
                    None,
                )
            }
            SvgElement::Ellipse(ellipse) => {
                if ellipse.link.is_some() {
                    return Err("compiled diagram contains an interactive shape");
                }
                (
                    Cow::Owned(svg_ellipse_path_ops(
                        ellipse.cx, ellipse.cy, ellipse.rx, ellipse.ry,
                    )),
                    ellipse.style,
                    None,
                    None,
                )
            }
            SvgElement::Line(line) => {
                if line.link.is_some() {
                    return Err("compiled diagram contains an interactive shape");
                }
                let mut style = line.style;
                // SVG lines have no fill area, even when the inherited fill is
                // black. Retaining it would close a stroke-only display path.
                style.fill = None;
                (
                    Cow::Owned(vec![
                        SvgPathOp::Move(line.x1, line.y1),
                        SvgPathOp::Line(line.x2, line.y2),
                    ]),
                    style,
                    line.marker_start,
                    line.marker_end,
                )
            }
            SvgElement::Polyline(poly) | SvgElement::Polygon(poly) => {
                if poly.link.is_some() || poly.marker_mid.is_some() {
                    return Err("compiled diagram uses unsupported shape metadata");
                }
                (
                    Cow::Owned(svg_poly_path_ops(
                        &poly.points,
                        matches!(element, SvgElement::Polygon(_)),
                    )),
                    poly.style,
                    poly.marker_start,
                    poly.marker_end,
                )
            }
            SvgElement::Path(path) => {
                if path.link.is_some() || path.marker_mid.is_some() {
                    return Err("compiled diagram uses unsupported shape metadata");
                }
                (
                    Cow::Borrowed(path.ops.as_slice()),
                    path.style,
                    path.marker_start,
                    path.marker_end,
                )
            }
            SvgElement::Text(text) => return self.text(text),
            SvgElement::Image(_) => {
                return Err("diagram image resources are outside the drawing profile");
            }
        };
        self.path(&ops, style, SvgTransform::IDENTITY, 1.0)?;
        // The compiler uses normal fill/stroke/marker ordering. Keep markers
        // immediately above their own edge and below subsequent labels/nodes.
        if let Some(reference) = start {
            let (tip, tail) =
                svg_path_start_tangent(&ops).ok_or("diagram marker has no start tangent")?;
            self.marker(reference, SvgMarkerPlacement::Start, tip, tail, style)?;
        }
        if let Some(reference) = end {
            let (tip, tail) =
                svg_path_end_tangent(&ops).ok_or("diagram marker has no end tangent")?;
            self.marker(reference, SvgMarkerPlacement::End, tip, tail, style)?;
        }
        Ok(())
    }

    fn path(
        &mut self,
        ops: &[SvgPathOp],
        style: SvgStyle,
        transform: SvgTransform,
        stroke_scale: f32,
    ) -> Result<(), &'static str> {
        check_style(style)?;
        if ops.is_empty() || ops.len() > MAX_DISPLAY_PATH_COMMANDS {
            return Err("diagram path exceeds its command limit");
        }
        self.commands = self
            .commands
            .checked_add(ops.len())
            .ok_or("diagram path budget overflow")?;
        if self.commands > MAX_DISPLAY_TOTAL_PATH_COMMANDS {
            return Err("diagram exceeds the 262144 path command limit");
        }
        let fill = style
            .fill
            .map(|color| rgba(color, svg_effective_fill_opacity(style)))
            .transpose()?;
        let stroke = stroke(style, stroke_scale)?;
        if fill.is_none() && stroke.is_none() {
            return Err("compiled diagram contains an unpainted primitive");
        }
        let mut ops = ops.to_vec();
        transform_svg_path_ops(&mut ops, transform);
        let (commands, mut bounds, closed) = commands(&ops)?;
        if fill.is_some() && !closed {
            return Err("open diagram fills are outside the drawing profile");
        }
        if let Some(stroke) = &stroke {
            let join = if stroke.join == LineJoin::Miter {
                stroke.miter_limit
            } else {
                1.0
            };
            let cap = if stroke.cap == LineCap::Square {
                std::f32::consts::SQRT_2
            } else {
                1.0
            };
            let padding = stroke.width * 0.5 * join.max(cap);
            bounds = DisplayRect::new(
                bounds.x - padding,
                bounds.y - padding,
                bounds.width + 2.0 * padding,
                bounds.height + 2.0 * padding,
            );
        }
        if ![bounds.x, bounds.y, bounds.right(), bounds.bottom()]
            .into_iter()
            .all(valid_coordinate)
        {
            return Err("diagram stroke exceeds the coordinate limit");
        }
        self.push(Item::Path(Path {
            bounds,
            commands,
            fill,
            stroke,
        }))
    }

    fn marker(
        &mut self,
        reference: SvgMarkerRef,
        placement: SvgMarkerPlacement,
        tip: (f32, f32),
        tail: (f32, f32),
        parent: SvgStyle,
    ) -> Result<(), &'static str> {
        let image = self.image;
        let marker = reference
            .index
            .and_then(|index| image.markers.get(index))
            .ok_or("diagram arrow marker could not be resolved")?;
        let (ux, uy) = svg_marker_orient_vector(marker.orient, placement, tip, tail)
            .ok_or("diagram arrow marker has a degenerate direction")?;
        let scale = if marker.units_stroke_width {
            parent.stroke_width
        } else {
            1.0
        };
        if !valid_positive(scale) {
            return Err("diagram arrow marker has invalid scale");
        }
        let view = svg_marker_view_box_transform(marker);
        let (ref_x, ref_y) = view.map_or((marker.ref_x, marker.ref_y), |view| {
            view.map_point(marker.ref_x, marker.ref_y)
        });
        let mut transform = SvgTransform {
            a: ux,
            b: uy,
            c: -uy,
            d: ux,
            e: tip.0,
            f: tip.1,
        }
        .concat(SvgTransform::scale(scale, scale))
        .concat(SvgTransform::translate(-ref_x, -ref_y));
        let view_scale = if let Some(view) = view {
            if view.sx != view.sy || !valid_positive(view.sx) {
                return Err("nonuniform diagram markers are outside the drawing profile");
            }
            transform = transform.concat(SvgTransform {
                a: view.sx,
                b: 0.0,
                c: 0.0,
                d: view.sy,
                e: view.tx,
                f: view.ty,
            });
            view.sx
        } else {
            1.0
        };
        let paint = svg_marker_paint(
            parent.fill,
            svg_effective_fill_opacity(parent),
            parent.stroke,
            svg_effective_stroke_opacity(parent),
            parent.stroke_width,
        );
        for shape in &marker.shapes {
            self.path(
                &shape.ops,
                svg_style_with_marker_context(shape.style, paint),
                transform,
                scale * view_scale,
            )?;
        }
        Ok(())
    }

    fn text(&mut self, text: &SvgText) -> Result<(), &'static str> {
        if !text.transform.is_identity()
            || text.link.is_some()
            || text.clip_path.is_some()
            || text.mask_path.is_some()
            || text.stroke.is_some()
            || text.non_scaling_stroke
            || text.paint_order != SvgPaintOrder::NORMAL
            || text.text_length.is_some()
            || text.letter_spacing != 0.0
            || text.word_spacing != 0.0
            || text.baseline_shift != 0.0
            || !text.decoration.is_empty()
            || !matches!(text.slot, F_BODY | F_BOLD)
            || !valid_coordinate(text.x)
            || !valid_coordinate(text.y)
            || !valid_positive(text.font_size)
        {
            return Err("compiled diagram uses unsupported text layout");
        }
        let baseline = match text.baseline {
            SvgDominantBaseline::Auto => Baseline::Alphabetic,
            SvgDominantBaseline::Middle => Baseline::Middle,
            _ => return Err("compiled diagram uses an unsupported text baseline"),
        };
        let fill = rgba(
            text.fill.ok_or("diagram label has no fill")?,
            text.fill_opacity,
        )?;
        self.text_bytes = self
            .text_bytes
            .checked_add(text.text.len())
            .ok_or("diagram label budget overflow")?;
        if self.text_bytes > MAX_TEXT_BYTES {
            return Err("diagram exceeds the 256 KiB label limit");
        }
        self.push(Item::Text(Text {
            x: text.x,
            y: text.y,
            text: text.text.clone(),
            font_size: text.font_size,
            bold: text.slot == F_BOLD,
            anchor: match text.anchor {
                SvgTextAnchor::Start => Anchor::Start,
                SvgTextAnchor::Middle => Anchor::Middle,
                SvgTextAnchor::End => Anchor::End,
            },
            baseline,
            fill,
        }))
    }

    fn push(&mut self, item: Item) -> Result<(), &'static str> {
        if self.items.len() >= MAX_ITEMS {
            return Err("diagram exceeds the 4096 display primitive limit");
        }
        self.items.push(item);
        Ok(())
    }
}

fn valid_coordinate(value: f32) -> bool {
    value.is_finite() && value.abs() <= MAX_COORDINATE
}

fn valid_positive(value: f32) -> bool {
    valid_coordinate(value) && value > 0.0
}

fn rgba((red, green, blue): (f32, f32, f32), alpha: f32) -> Result<[f32; 4], &'static str> {
    let color = [red, green, blue, alpha];
    if color
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    {
        Ok(color)
    } else {
        Err("compiled diagram contains an invalid color")
    }
}

fn check_style(style: SvgStyle) -> Result<(), &'static str> {
    if !style.visible
        || !style.transform.is_identity()
        || style.non_scaling_stroke
        || style.fill_gradient.is_some()
        || style.fill_pattern.is_some()
        || style.stroke_gradient.is_some()
        || style.stroke_pattern.is_some()
        || style.fill_context.is_some()
        || style.stroke_context.is_some()
        || style.clip_path.is_some()
        || style.mask_path.is_some()
        || style.shadow.is_some()
        || !matches!(style.fill_rule, SvgFillRule::NonZero)
        || style.paint_order != SvgPaintOrder::NORMAL
    {
        Err("compiled diagram uses paint outside the drawing profile")
    } else {
        Ok(())
    }
}

fn stroke(style: SvgStyle, scale: f32) -> Result<Option<Stroke>, &'static str> {
    let Some(color) = style.stroke else {
        return Ok(None);
    };
    let width = style.stroke_width * scale;
    let miter_limit = style.miter_limit.unwrap_or(4.0);
    let dash_offset = style.dash.offset * scale;
    let dash: Vec<_> = style.dash.values[..usize::from(style.dash.len)]
        .iter()
        .map(|value| value * scale)
        .collect();
    if !valid_positive(width)
        || !valid_positive(miter_limit)
        || miter_limit < 1.0
        || !valid_coordinate(dash_offset)
        || dash
            .iter()
            .any(|value| !valid_coordinate(*value) || *value < 0.0)
        || (!dash.is_empty() && !dash.iter().any(|value| *value > 0.0))
    {
        return Err("compiled diagram contains invalid stroke geometry");
    }
    Ok(Some(Stroke {
        color: rgba(color, svg_effective_stroke_opacity(style))?,
        width,
        dash,
        dash_offset,
        cap: match style.line_cap {
            SvgLineCap::Butt => LineCap::Butt,
            SvgLineCap::Round => LineCap::Round,
            SvgLineCap::Square => LineCap::Square,
        },
        join: match style.line_join {
            SvgLineJoin::Miter => LineJoin::Miter,
            SvgLineJoin::Round => LineJoin::Round,
            SvgLineJoin::Bevel => LineJoin::Bevel,
        },
        miter_limit,
    }))
}

fn commands(
    ops: &[SvgPathOp],
) -> Result<(Vec<DisplayPathCommand>, DisplayRect, bool), &'static str> {
    let mut out = Vec::with_capacity(ops.len());
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    let mut point = |x: f32, y: f32| -> Result<(), &'static str> {
        if !valid_coordinate(x) || !valid_coordinate(y) {
            return Err("diagram path exceeds the coordinate limit");
        }
        bounds = Some(match bounds {
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            None => (x, y, x, y),
        });
        Ok(())
    };
    let mut open = false;
    let mut drawn = false;
    let mut all_closed = true;
    for op in ops {
        out.push(match *op {
            SvgPathOp::Move(x, y) => {
                if open {
                    if !drawn {
                        return Err("diagram path contains an empty contour");
                    }
                    all_closed = false;
                }
                point(x, y)?;
                open = true;
                drawn = false;
                DisplayPathCommand::MoveTo { x, y }
            }
            SvgPathOp::Line(x, y) if open => {
                point(x, y)?;
                drawn = true;
                DisplayPathCommand::LineTo { x, y }
            }
            SvgPathOp::Quad(cx, cy, x, y) if open => {
                point(cx, cy)?;
                point(x, y)?;
                drawn = true;
                DisplayPathCommand::QuadTo { cx, cy, x, y }
            }
            SvgPathOp::Cubic(cx1, cy1, cx2, cy2, x, y) if open => {
                point(cx1, cy1)?;
                point(cx2, cy2)?;
                point(x, y)?;
                drawn = true;
                DisplayPathCommand::CubicTo {
                    cx1,
                    cy1,
                    cx2,
                    cy2,
                    x,
                    y,
                }
            }
            SvgPathOp::Close if open && drawn => {
                open = false;
                DisplayPathCommand::Close
            }
            _ => return Err("diagram path contains an invalid contour"),
        });
    }
    if open {
        if !drawn {
            return Err("diagram path contains an empty contour");
        }
        all_closed = false;
    }
    let (x0, y0, x1, y1) = bounds.ok_or("diagram path has no geometry")?;
    Ok((out, DisplayRect::new(x0, y0, x1 - x0, y1 - y0), all_closed))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn paths(scene: &Scene) -> impl Iterator<Item = &Path> {
        scene.items.iter().filter_map(|item| match item {
            Item::Path(path) => Some(path),
            _ => None,
        })
    }

    fn labels(scene: &Scene) -> Vec<&str> {
        scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 0.002, "{actual} != {expected}");
    }

    #[test]
    fn complete_flowcharts_keep_return_routes_open_edges_and_paint() {
        for direction in ["TD", "LR", "BT", "RL"] {
            let source = format!(
                "flowchart {direction}\nA([Start]) -. Yes .-> B{{Ready?}}\nB ==> C((End))\nC --> A\nA --> A\nA --- D[Open]\n"
            );
            let scene = compile(&source, "mermaid").unwrap();
            for label in ["Start", "Ready?", "End", "Open", "Yes"] {
                assert!(labels(&scene).contains(&label), "{direction} {label}");
            }
            assert!(paths(&scene).any(|path| {
                path.stroke
                    .as_ref()
                    .is_some_and(|stroke| stroke.dash == [4.0, 4.0])
            }));
            assert!(paths(&scene).any(|path| {
                path.stroke
                    .as_ref()
                    .is_some_and(|stroke| stroke.width == 3.0)
            }));
            assert!(
                paths(&scene).any(|path| path.fill.is_none() && path.commands.len() == 6),
                "return route {direction}"
            );
            assert!(
                paths(&scene).any(|path| path
                    .commands
                    .iter()
                    .any(|command| matches!(command, DisplayPathCommand::CubicTo { .. }))),
                "rounded nodes {direction}"
            );
            assert!(
                paths(&scene).any(|path| path.fill.is_some_and(|fill| fill[3] == 0.9)),
                "label opacity {direction}"
            );
        }
    }

    #[test]
    fn marker_geometry_uses_the_compilers_viewbox_reference_and_stroke_scale() {
        let scene = compile("graph LR\nA[One] --> B[Two]", "mermaid").unwrap();
        let mut paths = paths(&scene);
        let edge = paths.next().unwrap();
        let DisplayPathCommand::LineTo { x, y } = edge.commands[1] else {
            panic!("straight edge expected");
        };
        let marker = paths.next().unwrap();
        // viewBox10 -> markerWidth6, multiplied by the edge's1.5px stroke.
        // The (8,5) reference point is the actual arrow tip.
        let DisplayPathCommand::MoveTo { x: left, y: top } = marker.commands[0] else {
            panic!();
        };
        let DisplayPathCommand::LineTo { x: tip_x, y: tip_y } = marker.commands[1] else {
            panic!();
        };
        near(tip_x, x);
        near(tip_y, y);
        near(left, x - 7.2);
        near(top, y - 3.15);
        assert!(marker.fill.is_some() && marker.stroke.is_none());

        let open = compile("graph LR\nA[One] --- B[Two]", "mermaid").unwrap();
        assert_eq!(scene.items.len(), open.items.len() + 1);
        assert_eq!(labels(&scene), labels(&open));
    }

    #[test]
    fn sequence_frames_notes_actors_and_crossed_arrows_keep_all_labels() {
        let source = "sequenceDiagram\n title Requests\n autonumber 3 2\n actor User\n participant API as Application\n User->>+API: Send & preserve<br>next line\n loop Retry\n API->>API: Again\n alt accepted\n API-->>User: Accepted\n else overloaded\n API--xUser: Busy\n end\n end\n Note left of User: Keep <data>\n API-->>-User: Complete\n";
        let scene = compile(source, "mermaid").unwrap();
        let labels = labels(&scene);
        for wanted in [
            "Requests",
            "User",
            "Application",
            "next line",
            "Keep <data>",
        ] {
            assert!(labels.contains(&wanted), "missing {wanted:?} in {labels:?}");
        }
        for wanted in [
            "Send & preserve",
            "Again",
            "Accepted",
            "Busy",
            "Complete",
            "Retry",
            "accepted",
            "overloaded",
        ] {
            assert!(
                labels.iter().any(|label| label.contains(wanted)),
                "missing {wanted:?}"
            );
        }
        assert!(paths(&scene).any(|path| {
            path.stroke
                .as_ref()
                .is_some_and(|stroke| stroke.dash == [4.0, 4.0])
        }));
        assert!(
            paths(&scene).any(|path| path.fill.is_none()
                && path
                    .commands
                    .iter()
                    .filter(|op| matches!(op, DisplayPathCommand::MoveTo { .. }))
                    .count()
                    > 1),
            "actor and crossed arrows retain separate open contours"
        );
        assert!(
            paths(&scene).any(|path| path
                .fill
                .is_some_and(|fill| fill[0] == 1.0 && fill[1] > 0.9 && fill[2] < 0.9)),
            "yellow note fill"
        );
    }

    #[test]
    fn pie_wedges_keep_curves_exact_values_and_white_bold_percentages() {
        let scene = compile("pie showData title Costs\n\"Engineering\" : 42.25\n\"Operations\" : 31.50\n\"Support\" : 26.25", "mermaid").unwrap();
        assert!(labels(&scene).iter().any(|label| label.contains("31.50")));
        assert_eq!(scene.width, 640.0);
        assert!(
            paths(&scene)
                .filter(|path| path
                    .stroke
                    .as_ref()
                    .is_some_and(|stroke| stroke.width == 1.0 && stroke.color == [1.0; 4]))
                .count()
                >= 3
        );
        assert!(paths(&scene).any(|path| {
            path.commands
                .iter()
                .any(|command| matches!(command, DisplayPathCommand::CubicTo { .. }))
        }));
        assert!(scene.items.iter().any(|item| matches!(item, Item::Text(text) if text.text == "42.3%" && text.fill == [1.0; 4] && text.bold)));
        assert!(labels(&scene).iter().any(|text| text.contains("42.25")));
        let one = compile("pie\n\"Only\" : 1", "pie").unwrap();
        assert!(labels(&one).contains(&"100%"));
        assert!(paths(&one).any(|path| path.commands.len() == 6 && path.stroke.is_none()));
    }

    #[test]
    fn empty_labels_do_not_remove_nodes_and_escaped_labels_are_literal() {
        let scene = compile("graph TD\nA[] --> B[\"<script> & literal\"]", "flowchart").unwrap();
        assert_eq!(labels(&scene), ["<script> & literal"]);
        assert_eq!(paths(&scene).count(), 4);
    }

    #[test]
    fn unsupported_or_excessive_source_is_refused_as_a_complete_figure() {
        for source in [
            "gantt\ntitle Schedule\nsection Rendering\nCompile :2026-01-01, 1d",
            "graph TD\nA-->B\nsubgraph hidden\nC-->D\nend",
            "%%{init: {}}%%\ngraph TD\nA-->B",
        ] {
            assert!(compile(source, "mermaid").is_err());
        }
        assert!(
            compile(&"x".repeat(MAX_SOURCE_BYTES + 1), "mermaid")
                .unwrap_err()
                .contains("64 KiB")
        );
        assert!(compile("<svg><script>alert(1)</script></svg>", "svg").is_err());
    }

    #[test]
    fn compiler_profile_drift_cannot_silently_drop_a_shape_or_label() {
        let (svg, _) =
            crate::diagrams::figure_svg("graph TD\nA[One] --> B[Two]", "mermaid").unwrap();
        for addition in [
            "<image href=\"remote.png\" width=\"20\" height=\"20\"/>",
            "<g><rect width=\"20\" height=\"20\"/></g>",
            "<rect width=\"0\" height=\"20\"/>",
            "<path d=\"M 0 0 X 4 4\"/>",
            "<text x=\"1\" y=\"2\" textLength=\"100\">Stretched</text>",
            "<text x=\"1\" y=\"2\"><tspan>Nested</tspan></text>",
            "<rect width=\"20\" height=\"20\" transform=\"rotate(30)\"/>",
        ] {
            let changed = svg.replace("</svg>", &format!("{addition}</svg>"));
            assert!(scene_from_svg(&changed).is_err(), "accepted {addition}");
        }
    }
}
