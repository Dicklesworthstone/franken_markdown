//! The layout node model — the engine shape the G0-3 ratification froze —
//! and the public output types of the frozen API sketch.
//!
//! Two layers:
//!
//! 1. The **public output**: [`Layout`] — flat, positioned
//!    [`PlacedGlyph`]/[`PlacedRule`]/[`PlacedPath`] lists in em units, y-up,
//!    baseline at 0 — is what consumers (franken_manim's fmn-tex; fmd's
//!    HTML/PDF renderers) read. Every glyph names its face (multi-face
//!    layout is structural: `∑ ∫ ∏` resolve through the math-symbol
//!    fallback face) and carries its source span (§11.3 span provenance).
//! 2. The **internal node model**: [`MBox`]/[`MNode`] — boxes, glue, and
//!    kerns, built bottom-up by the Appendix-G constructions and flattened
//!    into a [`Layout`] at the end. Fixed here (this bead) so the placement
//!    mathematics and the extension beads build on one shape.
//!
//! `typeset` itself lands with the placement bead (fm-hk9); this module
//! fixes the shapes both sides compile against.

use crate::node::Span;

/// Identifies one of the faces handed to the engine (an index into the
/// engine's face list, in construction order). Multi-face layout is
/// structural: face selection is data on every glyph, never a rendering
/// afterthought.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FaceId(pub usize);

/// A positioned glyph in the final layout. Units are ems of the base
/// (text-style) size; `y` is the baseline-relative vertical position,
/// positive up; `size` is the glyph's own size factor (1.0 / 0.7 / 0.5 per
/// style).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// The face the glyph comes from.
    pub face: FaceId,
    /// Glyph id in that face.
    pub gid: u16,
    /// The character the glyph renders.
    pub ch: char,
    /// Horizontal position of the glyph origin, in ems.
    pub x: f64,
    /// Baseline-relative vertical position, in ems, y-up.
    pub y: f64,
    /// Size factor relative to the base size.
    pub size: f64,
    /// Source span of the construct that produced the glyph.
    pub span: Span,
}

/// A positioned rectangular rule (fraction bars, `\overline`s, radical
/// overbars). `x`/`y` name the rule's left-bottom corner in ems, y-up.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRule {
    /// Left edge, ems.
    pub x: f64,
    /// Bottom edge, baseline-relative ems, y-up.
    pub y: f64,
    /// Width, ems.
    pub width: f64,
    /// Height (thickness), ems.
    pub height: f64,
    /// Source span of the construct that produced the rule.
    pub span: Span,
}

/// One segment of a drawn-path contour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSeg {
    /// A straight line to the endpoint.
    Line {
        /// Endpoint, ems.
        to: (f64, f64),
    },
    /// A quadratic Bézier through one control point.
    Quad {
        /// Control point, ems.
        ctrl: (f64, f64),
        /// Endpoint, ems.
        to: (f64, f64),
    },
}

/// One closed contour of a drawn path: a start point plus segments; the
/// contour closes back to the start implicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct PathContour {
    /// Start point, ems.
    pub start: (f64, f64),
    /// The segments.
    pub segments: Vec<PathSeg>,
}

/// A positioned drawn-path construction (parametric delimiters past the
/// glyph-scaling threshold, the drawn radical, braces): quadratic contours
/// in ems, y-up, the same path model franken_manim's geometry kernel and
/// fmd-font outlines share.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedPath {
    /// The closed contours.
    pub contours: Vec<PathContour>,
    /// Source span of the construct that produced the path.
    pub span: Span,
}

/// The final layout of a formula: flat positioned primitives plus overall
/// metrics. Everything is in ems of the base size, y-up, baseline at 0;
/// `width` spans the whole formula, `height` rises above the baseline,
/// `depth` extends below it (a positive number).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Layout {
    /// Every glyph, positioned.
    pub glyphs: Vec<PlacedGlyph>,
    /// Every rule, positioned.
    pub rules: Vec<PlacedRule>,
    /// Every drawn path, positioned.
    pub paths: Vec<PlacedPath>,
    /// Total advance width, ems.
    pub width: f64,
    /// Extent above the baseline, ems.
    pub height: f64,
    /// Extent below the baseline, ems (positive).
    pub depth: f64,
}

// ── The internal node model (crate-visible; the placement bead's input) ──
//
// The `allow(dead_code)` on these items is deliberate and temporary: the
// engine shape is fixed by THIS bead (franken_manim fm-wgl) so the
// Appendix-G placement bead (fm-hk9) builds on it; until that bead lands,
// the lib target only reaches the model through its tests.

/// TeX-style glue: a natural width with stretch and shrink allowances, in
/// ems. The inter-atom spaces of the spacing table are glue (thin space
/// stretches, medium and thick spaces stretch and shrink per plain TeX);
/// this bead records natural widths, and the placement bead applies
/// stretching when alignment demands it.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub(crate) struct GlueSpec {
    /// Natural width, ems.
    pub(crate) natural: f64,
    /// Stretch allowance, ems.
    pub(crate) stretch: f64,
    /// Shrink allowance, ems.
    pub(crate) shrink: f64,
}

/// A child of a box, positioned relative to the box's origin (its left
/// edge, on its baseline).
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Positioned<T> {
    /// Horizontal offset from the box origin, ems.
    pub(crate) dx: f64,
    /// Vertical offset from the box baseline, ems, y-up.
    pub(crate) dy: f64,
    /// The child.
    pub(crate) node: T,
}

/// One node of the internal layout tree.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MNode {
    /// A glyph at the box-local origin.
    Glyph {
        face: FaceId,
        gid: u16,
        ch: char,
        size: f64,
        span: Span,
    },
    /// A rule.
    Rule { width: f64, height: f64, span: Span },
    /// A drawn path.
    Path {
        contours: Vec<PathContour>,
        span: Span,
    },
    /// A fixed horizontal advance.
    Kern(f64),
    /// Stretchable space.
    Glue(GlueSpec),
    /// A nested box.
    Box(MBox),
}

/// How a box stacks its children.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoxKind {
    /// Children advance horizontally along the baseline.
    Horizontal,
    /// Children stack vertically (the Appendix-G constructions).
    Vertical,
}

/// A layout box: dimensions plus positioned children. The Appendix-G
/// constructions build these bottom-up; flattening walks the tree
/// accumulating offsets into a [`Layout`].
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MBox {
    pub(crate) kind: BoxKind,
    /// Advance width, ems.
    pub(crate) width: f64,
    /// Extent above the baseline, ems.
    pub(crate) height: f64,
    /// Extent below the baseline, ems, positive.
    pub(crate) depth: f64,
    /// The children.
    pub(crate) children: Vec<Positioned<MNode>>,
}

#[allow(dead_code)]
impl MBox {
    /// An empty box of the given kind.
    pub(crate) fn empty(kind: BoxKind) -> Self {
        Self {
            kind,
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            children: Vec::new(),
        }
    }

    /// Flatten the box tree into a [`Layout`], accumulating offsets from
    /// `(x, y)`.
    pub(crate) fn flatten_into(&self, x: f64, y: f64, out: &mut Layout) {
        for child in &self.children {
            let cx = x + child.dx;
            let cy = y + child.dy;
            match &child.node {
                MNode::Glyph {
                    face,
                    gid,
                    ch,
                    size,
                    span,
                } => out.glyphs.push(PlacedGlyph {
                    face: *face,
                    gid: *gid,
                    ch: *ch,
                    x: cx,
                    y: cy,
                    size: *size,
                    span: *span,
                }),
                MNode::Rule {
                    width,
                    height,
                    span,
                } => out.rules.push(PlacedRule {
                    x: cx,
                    y: cy,
                    width: *width,
                    height: *height,
                    span: *span,
                }),
                MNode::Path { contours, span } => {
                    let moved = contours
                        .iter()
                        .map(|c| PathContour {
                            start: (c.start.0 + cx, c.start.1 + cy),
                            segments: c
                                .segments
                                .iter()
                                .map(|s| match s {
                                    PathSeg::Line { to } => PathSeg::Line {
                                        to: (to.0 + cx, to.1 + cy),
                                    },
                                    PathSeg::Quad { ctrl, to } => PathSeg::Quad {
                                        ctrl: (ctrl.0 + cx, ctrl.1 + cy),
                                        to: (to.0 + cx, to.1 + cy),
                                    },
                                })
                                .collect(),
                        })
                        .collect();
                    out.paths.push(PlacedPath {
                        contours: moved,
                        span: *span,
                    });
                }
                MNode::Kern(_) | MNode::Glue(_) => {}
                MNode::Box(inner) => inner.flatten_into(cx, cy, out),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_covers_the_whole_node_model() {
        // A vertical construction holding a rule, a drawn path, a kern, and
        // glue: kerns and glue contribute spacing only (they must not emit
        // primitives), everything else translates by the accumulated offset.
        let mut vbox = MBox::empty(BoxKind::Vertical);
        vbox.width = 1.0;
        vbox.height = 1.0;
        vbox.children = vec![
            Positioned {
                dx: 0.1,
                dy: 0.4,
                node: MNode::Rule {
                    width: 0.8,
                    height: 0.04,
                    span: Span::new(0, 4),
                },
            },
            Positioned {
                dx: 0.0,
                dy: 0.0,
                node: MNode::Kern(0.25),
            },
            Positioned {
                dx: 0.0,
                dy: 0.0,
                node: MNode::Glue(GlueSpec {
                    natural: 3.0 / 18.0,
                    stretch: 1.5 / 18.0,
                    shrink: 1.0 / 18.0,
                }),
            },
            Positioned {
                dx: 0.2,
                dy: -0.3,
                node: MNode::Path {
                    contours: vec![PathContour {
                        start: (0.0, 0.0),
                        segments: vec![
                            PathSeg::Line { to: (0.5, 0.0) },
                            PathSeg::Quad {
                                ctrl: (0.5, 0.5),
                                to: (0.0, 0.5),
                            },
                        ],
                    }],
                    span: Span::new(4, 9),
                },
            },
        ];
        let mut layout = Layout::default();
        MBox {
            kind: BoxKind::Horizontal,
            width: 2.0,
            height: 1.0,
            depth: 0.0,
            children: vec![Positioned {
                dx: 1.0,
                dy: 0.5,
                node: MNode::Box(vbox),
            }],
        }
        .flatten_into(0.0, 0.0, &mut layout);
        assert!(layout.glyphs.is_empty());
        assert_eq!(layout.rules.len(), 1);
        assert_eq!(layout.rules[0].x, 1.1);
        assert_eq!(layout.rules[0].y, 0.9);
        assert_eq!(layout.paths.len(), 1);
        assert_eq!(layout.paths[0].contours[0].start, (1.2, 0.2));
        match layout.paths[0].contours[0].segments[1] {
            PathSeg::Quad { ctrl, to } => {
                assert_eq!(ctrl, (1.7, 0.7));
                assert_eq!(to, (1.2, 0.7));
            }
            PathSeg::Line { .. } => unreachable!("second segment is the quad"),
        }
    }

    #[test]
    fn flatten_accumulates_offsets() {
        let inner = MBox {
            kind: BoxKind::Horizontal,
            width: 1.0,
            height: 0.7,
            depth: 0.0,
            children: vec![Positioned {
                dx: 0.25,
                dy: 0.0,
                node: MNode::Glyph {
                    face: FaceId(0),
                    gid: 7,
                    ch: 'x',
                    size: 1.0,
                    span: Span::new(0, 1),
                },
            }],
        };
        let outer = MBox {
            kind: BoxKind::Horizontal,
            width: 2.0,
            height: 0.7,
            depth: 0.0,
            children: vec![Positioned {
                dx: 1.0,
                dy: 0.5,
                node: MNode::Box(inner),
            }],
        };
        let mut layout = Layout::default();
        outer.flatten_into(0.0, 0.0, &mut layout);
        assert_eq!(layout.glyphs.len(), 1);
        assert_eq!(layout.glyphs[0].x, 1.25);
        assert_eq!(layout.glyphs[0].y, 0.5);
    }
}
