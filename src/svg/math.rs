//! Adapter from the shared TeX engine's y-up em geometry to poster paths.
//! No MathML/foreignObject, browser typesetter, or runtime font lookup.

use std::rc::Rc;
use franken_markdown::math::{self, FaceId, PathSeg, Style};
use super::{Ink, Op, Poster, RStyle, SLOT_COUNT, SvgWarning, Word, push_q2};

const MAX_FORMULA_BYTES: usize = 64 * 1024;
const MAX_PRIMITIVES: usize = 65_536;
const MAX_COORD: f64 = 1_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Formula {
    layout: math::Layout,
    pub(super) width: f64,
    pub(super) ascent: f64,
    pub(super) descent: f64,
    left: f64,
}

#[derive(Debug, Clone)]
pub(super) struct MathRun {
    pub(super) formula: Rc<Formula>,
    pub(super) size: f64,
}

impl Poster {
    pub(super) fn math_word(&self, source: &str, display: bool, style: RStyle, size: f64,
        width: f64) -> Word
    {
        match self.typeset_formula(source, display) {
            Ok(formula) => {
                let size = fitted_size(&formula, size, width);
                Word {
                    text: String::new(), style, w: formula.width * size, gap: 0.0,
                    formula: Some(MathRun { formula, size }), warning: None,
                }
            }
            Err(warning) => {
                let style = RStyle { mono: true, ..style };
                Word {
                    text: source.to_string(), style, w: self.measure(source, style, size),
                    gap: 0.0, formula: None, warning: Some(warning),
                }
            }
        }
    }

    fn typeset_formula(&self, source: &str, display: bool) -> Result<Rc<Formula>, SvgWarning> {
        if source.len() > MAX_FORMULA_BYTES {
            return Err(warning("svg_math_limit", "formula exceeds the 64 KiB source limit"));
        }
        let engine = self.math_engine.get_or_init(|| math::Engine::bundled().ok())
            .as_ref().ok_or_else(|| warning("svg_math_fonts", "bundled math faces unavailable"))?;
        let layout = engine.typeset(source, if display { Style::Display } else { Style::Text })
            .map_err(|error| warning("svg_math_unsupported", &error.to_string()))?;
        if layout.glyphs.len().saturating_add(layout.rules.len())
            .saturating_add(layout.paths.len()) > MAX_PRIMITIVES
        {
            return Err(warning("svg_math_limit", "formula exceeds the primitive limit"));
        }
        // Use actual ink bounds as well as advance metrics. Italic overhang,
        // negative kerns, radicals and large delimiters must stay in the box.
        let mut bounds = Bounds::default();
        bounds.add(0.0, -layout.depth)?;
        bounds.add(layout.width, layout.height)?;
        for glyph in &layout.glyphs {
            if !valid(glyph.size) || glyph.size < 0.0 {
                return Err(warning("svg_math_geometry", "invalid glyph scale"));
            }
            bounds.add(glyph.x, glyph.y)?;
            let font = engine.faces().font(glyph.face)
                .ok_or_else(|| warning("svg_math_fonts", "formula names an absent face"))?;
            if let Some(bbox) = font.glyph_bbox(glyph.gid) {
                let scale = glyph.size / f64::from(font.units_per_em.max(1));
                bounds.add(glyph.x + f64::from(bbox[0]) * scale,
                    glyph.y + f64::from(bbox[1]) * scale)?;
                bounds.add(glyph.x + f64::from(bbox[2]) * scale,
                    glyph.y + f64::from(bbox[3]) * scale)?;
            }
        }
        for rule in &layout.rules {
            if rule.width < 0.0 || rule.height < 0.0 {
                return Err(warning("svg_math_geometry", "negative rule dimensions"));
            }
            bounds.add(rule.x, rule.y)?;
            bounds.add(rule.x + rule.width, rule.y + rule.height)?;
        }
        let mut segments = 0usize;
        for path in &layout.paths {
            for contour in &path.contours {
                segments = segments.saturating_add(contour.segments.len()).saturating_add(1);
                if segments > MAX_PRIMITIVES {
                    return Err(warning("svg_math_limit", "formula exceeds the path segment limit"));
                }
                bounds.add(contour.start.0, contour.start.1)?;
                for segment in &contour.segments {
                    match segment {
                        PathSeg::Line { to } => bounds.add(to.0, to.1)?,
                        PathSeg::Quad { ctrl, to } => {
                            // A Bezier stays inside its control hull.
                            bounds.add(ctrl.0, ctrl.1)?;
                            bounds.add(to.0, to.1)?;
                        }
                    }
                }
            }
        }
        Ok(Rc::new(Formula {
            layout, width: bounds.right - bounds.left, ascent: bounds.top,
            descent: -bounds.bottom, left: bounds.left,
        }))
    }

    pub(super) fn font_for_slot(&self, slot: usize) -> Option<&franken_markdown::text::Font> {
        if slot < SLOT_COUNT {
            self.faces[slot].as_ref()
        } else {
            self.math_engine.get()?.as_ref()?.faces().font(FaceId(slot - SLOT_COUNT))
        }
    }

    pub(super) fn draw_math(&mut self, run: &MathRun, x: f64, baseline: f64, ink: Ink) {
        let formula = &run.formula;
        let size = run.size;
        let origin = x - formula.left * size;
        for glyph in &formula.layout.glyphs {
            if !glyph.ch.is_whitespace() {
                self.ops.push(Op::Glyph {
                    slot: SLOT_COUNT + glyph.face.0, gid: glyph.gid,
                    x: origin + glyph.x * size, y: baseline - glyph.y * size,
                    size: glyph.size * size, ink,
                });
            }
        }
        for rule in &formula.layout.rules {
            self.ops.push(Op::Rect {
                x: origin + rule.x * size, y: baseline - (rule.y + rule.height) * size,
                w: rule.width * size, h: rule.height * size, fill: ink, stroke: None,
            });
        }
        for path in &formula.layout.paths {
            let mut data = String::new();
            for contour in &path.contours {
                data.push('M');
                point(&mut data, contour.start, origin, baseline, size);
                for segment in &contour.segments {
                    match segment {
                        PathSeg::Line { to } => {
                            data.push('L');
                            point(&mut data, *to, origin, baseline, size);
                        }
                        PathSeg::Quad { ctrl, to } => {
                            data.push('Q');
                            point(&mut data, *ctrl, origin, baseline, size);
                            data.push(' ');
                            point(&mut data, *to, origin, baseline, size);
                        }
                    }
                }
                data.push('Z');
            }
            self.ops.push(Op::Path { data, ink });
        }
    }

    pub(super) fn math_block(&mut self, source: &str, l: f64, r: f64, quote: bool) {
        match self.typeset_formula(source, true) {
            Ok(formula) => {
                let available = (r - l).max(1.0);
                let size = fitted_size(&formula, f64::from(self.scale.body), available);
                let x = l + (available - formula.width * size).max(0.0) / 2.0;
                let pad = f64::from(self.scale.body) * 0.5;
                let baseline = self.y + pad + formula.ascent * size;
                let height = (formula.ascent + formula.descent) * size;
                self.draw_math(&MathRun { formula, size }, x, baseline, Self::default_ink(quote));
                self.y += height + 2.0 * pad;
            }
            Err(warning) => {
                self.warnings.push(warning);
                self.code_panel(source, l, r);
            }
        }
    }

    /// Keep historical prose baselines unchanged; grow only lines whose math
    /// ink needs more ascent or descent than the normal text line provides.
    pub(super) fn line_metrics(&self, words: &[Word], ascent: f64, leading: f64) -> (f64, f64) {
        if !words.iter().any(|word| word.formula.is_some()) {
            return (ascent, leading);
        }
        let mut above = ascent;
        let mut below = (leading - ascent).max(0.0);
        for run in words.iter().filter_map(|word| word.formula.as_ref()) {
            above = above.max(run.formula.ascent * run.size);
            below = below.max(run.formula.descent * run.size);
        }
        (above, above + below)
    }
}

fn fitted_size(formula: &Formula, size: f64, width: f64) -> f64 {
    if formula.width > 0.0 { size.min(width.max(1.0) / formula.width) } else { size }
}

fn point(out: &mut String, p: (f64, f64), x: f64, baseline: f64, size: f64) {
    push_q2(out, x + p.0 * size);
    out.push(' ');
    push_q2(out, baseline - p.1 * size);
}

fn warning(code: &'static str, message: &str) -> SvgWarning {
    SvgWarning { code, message: message.to_string() }
}

fn valid(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_COORD
}

#[derive(Default)]
struct Bounds {
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
}

impl Bounds {
    fn add(&mut self, x: f64, y: f64) -> Result<(), SvgWarning> {
        if !valid(x) || !valid(y) {
            return Err(warning("svg_math_geometry", "non-finite or excessive formula coordinates"));
        }
        self.left = self.left.min(x);
        self.right = self.right.max(x);
        self.bottom = self.bottom.min(y);
        self.top = self.top.max(y);
        Ok(())
    }
}
