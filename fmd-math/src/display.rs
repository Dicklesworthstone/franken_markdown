//! Explicit labels on standalone display formulas.
//!
//! The ordinary math tree and intrinsic layout API keep their existing shape.
//! A display owns one optional text-mode label, and its host supplies the width
//! of the containing column when placing that label. No document counter or
//! automatic equation numbering is inferred here.

use crate::{Engine, Layout, MathError, Node, PathSeg, Placed};

/// A standalone formula and its optional explicit `\tag` / `\tag*` label.
///
/// Both trees retain byte spans into the original source. The label is parsed
/// in text mode, including the parentheses supplied by `\tag`; `\tag*` leaves
/// those parentheses out. A label belongs to the complete display, not a row
/// inside an alignment or a nested formula.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayFormula {
    pub(crate) body: Node,
    pub(crate) tag: Option<Node>,
}

impl DisplayFormula {
    /// The ordinary math tree, with the explicit label separated out.
    #[must_use]
    pub fn body(&self) -> &Node {
        &self.body
    }

    /// The label's text-mode tree, including any generated parentheses.
    #[must_use]
    pub fn tag(&self) -> Option<&Node> {
        self.tag.as_ref()
    }

    /// Serialize the complete label as inline MathML, retaining text styling,
    /// escaped characters and any explicit math islands in its text argument.
    #[must_use]
    pub fn tag_mathml(&self) -> Option<String> {
        self.tag.as_ref().map(crate::mathml::to_label_mathml)
    }
}

pub(crate) fn validate_width(width: f64) -> Result<(), MathError> {
    if width.is_finite() && width > 0.0 && width <= 1_000_000.0 {
        Ok(())
    } else {
        Err(invalid(
            "display width must be finite, positive and at most 1000000 em",
        ))
    }
}

fn invalid(what: &str) -> MathError {
    MathError::Malformed {
        what: what.to_owned(),
        at: 0,
    }
}

/// Keep the equation centered in the complete column and its tag at the right
/// edge. A symmetric tag gutter makes these two requirements compatible. If
/// the column is narrower than the pair's minimum size, return that minimum
/// width: the host can apply its normal bounded fitting/overflow policy to the
/// complete formula, without overlapping its label or silently dropping text.
pub(crate) fn compose(
    engine: &Engine,
    mut body: Layout,
    mut tag: Layout,
    width: f64,
) -> Result<Layout, MathError> {
    let equation = Bounds::of(engine, &body)?;
    let label = Bounds::of(engine, &tag)?;
    const LABEL_GAP_EM: f64 = 1.0;
    let width = width.max(equation.width() + 2.0 * (label.width() + LABEL_GAP_EM));
    validate_width(width)?;
    let body_x = (width - equation.width()) * 0.5 - equation.left;
    let tag_x = width - label.right;
    let tag_y = (equation.top + equation.bottom - label.top - label.bottom) * 0.5;
    translate(&mut body, body_x, 0.0);
    translate(&mut tag, tag_x, tag_y);

    let glyph_offset = body.glyphs.len();
    let rule_offset = body.rules.len();
    let path_offset = body.paths.len();
    body.order
        .extend(tag.order.into_iter().map(|placed| match placed {
            Placed::Glyph(index) => Placed::Glyph(glyph_offset + index),
            Placed::Rule(index) => Placed::Rule(rule_offset + index),
            Placed::Path(index) => Placed::Path(path_offset + index),
        }));
    body.glyphs.append(&mut tag.glyphs);
    body.rules.append(&mut tag.rules);
    body.paths.append(&mut tag.paths);
    body.width = width;
    body.height = equation.top.max(label.top + tag_y);
    body.depth = -equation.bottom.min(label.bottom + tag_y);
    Ok(body)
}

fn translate(layout: &mut Layout, x: f64, y: f64) {
    for glyph in &mut layout.glyphs {
        glyph.x += x;
        glyph.y += y;
    }
    for rule in &mut layout.rules {
        rule.x += x;
        rule.y += y;
    }
    for path in &mut layout.paths {
        for contour in &mut path.contours {
            contour.start.0 += x;
            contour.start.1 += y;
            for segment in &mut contour.segments {
                match segment {
                    PathSeg::Line { to } => {
                        to.0 += x;
                        to.1 += y;
                    }
                    PathSeg::Quad { ctrl, to } => {
                        ctrl.0 += x;
                        ctrl.1 += y;
                        to.0 += x;
                        to.1 += y;
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct Bounds {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
}

impl Bounds {
    fn width(&self) -> f64 {
        self.right - self.left
    }

    fn add(&mut self, x: f64, y: f64) -> Result<(), MathError> {
        if !x.is_finite() || !y.is_finite() || x.abs() > 1_000_000.0 || y.abs() > 1_000_000.0 {
            return Err(invalid(
                "display geometry exceeds the finite coordinate limit",
            ));
        }
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.bottom = self.bottom.min(y);
        self.top = self.top.max(y);
        Ok(())
    }

    fn of(engine: &Engine, layout: &Layout) -> Result<Self, MathError> {
        let mut bounds = Self::default();
        // Keep intentional phantom/advance space as well as ink. Negative
        // kerns, italic overhangs and constructed paths cannot enter the label
        // gutter merely because they lie outside the nominal advance box.
        bounds.add(0.0, -layout.depth)?;
        bounds.add(layout.width, layout.height)?;
        for glyph in &layout.glyphs {
            let font = engine
                .faces()
                .font(glyph.face)
                .ok_or(MathError::UnmappedChar {
                    ch: glyph.ch,
                    span: glyph.span,
                })?;
            if let Some(bbox) = font.glyph_bbox(glyph.gid) {
                let scale = glyph.size / f64::from(font.units_per_em.max(1));
                bounds.add(
                    glyph.x + f64::from(bbox[0]) * scale,
                    glyph.y + f64::from(bbox[1]) * scale,
                )?;
                bounds.add(
                    glyph.x + f64::from(bbox[2]) * scale,
                    glyph.y + f64::from(bbox[3]) * scale,
                )?;
            }
        }
        for rule in &layout.rules {
            bounds.add(rule.x, rule.y)?;
            bounds.add(rule.x + rule.width, rule.y + rule.height)?;
        }
        for path in &layout.paths {
            for contour in &path.contours {
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
        }
        Ok(bounds)
    }
}
