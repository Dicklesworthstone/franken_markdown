//! The layout engine: Appendix-G placement over the synthesized metrics.
//!
//! [`Engine::typeset`] runs the whole ratified pipeline — parse → classify
//! → layout(Ctx) → [`Layout`] — with the constructions implemented from
//! TeX's published rules (The TeXbook, Appendix G): generalized fractions
//! (rule 15), scripts with the simultaneous-scripts clash rules (rule 18),
//! radicals (rule 11), accents (rule 12), big-operator limits (rules
//! 13/13a, reused for the `\overset` family exactly as amsmath defines
//! them), and `\left…\right` sizing (rule 19). Every parameter comes from
//! [`MathConstants`] scaled by the current style's size factor — the
//! analogue of TeX reading fontdimens from the current size's fonts.
//!
//! **Determinism.** Layout arithmetic is pure f64 addition, subtraction,
//! multiplication, division, and comparison — no transcendental function
//! is ever called — so the same string and face set produce bit-identical
//! layouts on every platform.
//!
//! **Delimiter mechanism (ADR-0005).** Natural glyph when it covers the
//! rule-19 target; uniform scaling above; the drawn-path mainline beyond
//! the `1.25×` ceiling lands with the extensions bead (fm-kg9) — until
//! then the engine keeps scaling uniformly past the ceiling, which is
//! geometrically exact and aesthetically interim (stroke weight thins; the
//! Look Gallery adjudicates when the drawn constructions arrive).
//!
//! **Not yet laid out** (precise, named errors; the extensions bead's
//! program): environments, the stretchy over/under constructions
//! (`\overbrace`, `\underbrace`, `\overrightarrow`, `\overleftarrow`,
//! `\widehat`, `\widetilde`).

use crate::atom::{AtomClass, classify_list, spacing_in_style};
use crate::error::MathError;
use crate::faces::{
    FACE_BOLD, FACE_ITALIC, FACE_REGULAR, FACE_SYMBOLS, FaceSet, GlyphMetrics,
    accent_spacing_fallback, alphabet_map, default_math_chain, glyph_metrics, kern_em,
};
use crate::mbox::{BoxKind, FaceId, GlueSpec, Layout, MBox, MNode, Positioned};
use crate::metrics::MathConstants;
use crate::node::{
    AccentKind, Delim, DelimSize, FracSpec, FragmentKind, Limits, MathFont, Node, NodeKind,
    PhantomKind, Span, StackKind, TextStyle,
};
use crate::style::{Style, StyleCtx};

/// The layout engine: a face roster plus the calibrated math constants.
pub struct Engine {
    faces: FaceSet,
    consts: MathConstants,
}

impl Engine {
    /// Build an engine over a face roster with the CM constants.
    #[must_use]
    pub fn new(faces: FaceSet) -> Self {
        Self {
            faces,
            consts: crate::metrics::CM,
        }
    }

    /// The engine over the bundled sovereign face set.
    ///
    /// # Errors
    ///
    /// Propagates a bundled-face parse failure (build corruption).
    #[cfg(feature = "bundled-faces")]
    pub fn bundled() -> Result<Self, fmd_font::FontError> {
        Ok(Self::new(FaceSet::bundled()?))
    }

    /// The face roster.
    #[must_use]
    pub fn faces(&self) -> &FaceSet {
        &self.faces
    }

    /// The math constants in force.
    #[must_use]
    pub fn constants(&self) -> &MathConstants {
        &self.consts
    }

    /// Typeset a math-mode string at the given outer style.
    ///
    /// # Errors
    ///
    /// Parse errors pass through; layout adds [`MathError::UnmappedChar`]
    /// for characters no face covers and named layout-pending errors for
    /// the extensions bead's constructs.
    pub fn typeset(&self, source: &str, style: Style) -> Result<Layout, MathError> {
        let root = crate::parse(source)?;
        self.finish(&root, StyleCtx::new(style))
    }

    /// Typeset a TexText-contract string (text mainland + math islands).
    ///
    /// # Errors
    ///
    /// As [`Engine::typeset`].
    pub fn typeset_text(&self, source: &str) -> Result<Layout, MathError> {
        let root = crate::parse_text(source)?;
        self.finish(&root, StyleCtx::new(Style::Text))
    }

    fn finish(&self, root: &Node, ctx: StyleCtx) -> Result<Layout, MathError> {
        let items = match &root.kind {
            NodeKind::List(items) => items.as_slice(),
            _ => std::slice::from_ref(root),
        };
        let boxx = self.hlist(
            items,
            LayCtx {
                style: ctx,
                alphabet: None,
                text_mode: false,
            },
        )?;
        let mut layout = Layout {
            width: boxx.width,
            height: boxx.height,
            depth: boxx.depth,
            ..Layout::default()
        };
        boxx.flatten_into(0.0, 0.0, &mut layout);
        Ok(layout)
    }
}

/// The full layout context threaded through the recursion.
#[derive(Clone, Copy)]
struct LayCtx {
    style: StyleCtx,
    alphabet: Option<MathFont>,
    /// Laying text (islands, `\text` bodies): letters stay upright.
    text_mode: bool,
}

impl LayCtx {
    fn size(&self) -> f64 {
        self.style.size_factor()
    }
    fn map(self, f: impl FnOnce(StyleCtx) -> StyleCtx) -> Self {
        Self {
            style: f(self.style),
            ..self
        }
    }
}

/// One laid atom with the metadata scripts and kerning need.
struct Laid {
    boxx: MBox,
    /// Synthesized italic correction of the trailing glyph, ems.
    italic: f64,
    /// Set when the box is a single character glyph (rule 18's u=v=0 case,
    /// and the kerning pass).
    char_glyph: Option<(FaceId, u16)>,
}

enum LineItem {
    Atom { laid: Laid, class: AtomClass },
    Glue(GlueSpec),
}

impl Engine {
    /// Lay out a horizontal list: split at `\\` into stacked lines; within
    /// a line, walk atoms with their effective classes, inserting the
    /// inter-atom glue of the spacing table and font kerns between
    /// adjacent same-face character glyphs.
    fn hlist(&self, items: &[Node], ctx: LayCtx) -> Result<MBox, MathError> {
        let mut lines: Vec<MBox> = Vec::new();
        for line in items.split(|n| matches!(n.kind, NodeKind::Linebreak)) {
            lines.push(self.line(line, ctx)?);
        }
        if lines.len() == 1 {
            return lines.pop().ok_or(MathError::Malformed {
                what: "internal: empty line vector".to_owned(),
                at: 0,
            });
        }
        // Stack lines: baseline-to-baseline is \baselineskip (grown when
        // boxes would come closer than \lineskip). The box baseline is the
        // first line's.
        let size = ctx.size();
        let mut children = Vec::new();
        let mut baseline = 0.0_f64;
        let mut prev_depth = 0.0_f64;
        let mut width = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut height = 0.0_f64;
        for (i, line) in lines.into_iter().enumerate() {
            if i == 0 {
                height = line.height;
            } else {
                let natural = self.consts.baseline_skip * size;
                let min_gap = prev_depth + line.height + self.consts.line_skip * size;
                baseline -= natural.max(min_gap);
            }
            width = width.max(line.width);
            depth = (-baseline) + line.depth;
            prev_depth = line.depth;
            children.push(Positioned {
                dx: 0.0,
                dy: baseline,
                node: MNode::Box(line),
            });
        }
        Ok(MBox {
            kind: BoxKind::Vertical,
            width,
            height,
            depth,
            children,
        })
    }

    /// One line of a horizontal list.
    fn line(&self, items: &[Node], outer: LayCtx) -> Result<MBox, MathError> {
        let classes = classify_list(items);
        let mut ctx = outer;
        let mut laid_items: Vec<LineItem> = Vec::new();
        for (node, class) in items.iter().zip(classes) {
            match &node.kind {
                NodeKind::StyleChange(style) => {
                    ctx = ctx.map(|s| StyleCtx {
                        style: *style,
                        cramped: s.cramped,
                    });
                }
                NodeKind::ColorChange(_)
                | NodeKind::AlignTab
                | NodeKind::Fragment(
                    FragmentKind::UnmatchedClose | FragmentKind::RedundantMathShift,
                ) => {}
                NodeKind::Space(kind) => {
                    let em = f64::from(kind.mu()) / 18.0 * ctx.size();
                    laid_items.push(LineItem::Glue(GlueSpec {
                        natural: em,
                        stretch: 0.0,
                        shrink: 0.0,
                    }));
                }
                NodeKind::Tie => {
                    laid_items.push(LineItem::Glue(GlueSpec {
                        natural: self.space_width(ctx),
                        stretch: 0.0,
                        shrink: 0.0,
                    }));
                }
                _ => {
                    let laid = self.lay_node(node, ctx)?;
                    laid_items.push(LineItem::Atom {
                        laid,
                        class: class.unwrap_or(AtomClass::Ord),
                    });
                }
            }
        }
        // Assemble: inter-atom glue + kerning.
        let mut children = Vec::new();
        let mut x = 0.0_f64;
        let mut height = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut prev: Option<(AtomClass, Option<(FaceId, u16)>)> = None;
        for item in laid_items {
            match item {
                LineItem::Glue(glue) => {
                    x += glue.natural;
                    children.push(Positioned {
                        dx: x,
                        dy: 0.0,
                        node: MNode::Glue(glue),
                    });
                    prev = None;
                }
                LineItem::Atom { laid, class } => {
                    if let Some((prev_class, prev_glyph)) = prev {
                        let mu = spacing_in_style(prev_class, class, ctx.style.style).mu();
                        x += f64::from(mu) / 18.0 * ctx.size();
                        if mu == 0 && prev_class == AtomClass::Ord && class == AtomClass::Ord {
                            if let (Some((pf, pg)), Some((cf, cg))) = (prev_glyph, laid.char_glyph)
                            {
                                if pf == cf {
                                    if let Some(font) = self.faces.font(pf) {
                                        x += kern_em(font, pg, cg) * ctx.size();
                                    }
                                    let _ = cg;
                                }
                            }
                        }
                    }
                    height = height.max(laid.boxx.height);
                    depth = depth.max(laid.boxx.depth);
                    let advance = laid.boxx.width;
                    let glyph = laid.char_glyph;
                    children.push(Positioned {
                        dx: x,
                        dy: 0.0,
                        node: MNode::Box(laid.boxx),
                    });
                    x += advance;
                    prev = Some((class, glyph));
                }
            }
        }
        Ok(MBox {
            kind: BoxKind::Horizontal,
            width: x,
            height,
            depth,
            children,
        })
    }

    /// Lay one node into an atom box.
    #[allow(clippy::too_many_lines)]
    fn lay_node(&self, node: &Node, ctx: LayCtx) -> Result<Laid, MathError> {
        match &node.kind {
            NodeKind::List(items) => Ok(Laid {
                boxx: self.hlist(items, ctx)?,
                italic: 0.0,
                char_glyph: None,
            }),
            NodeKind::Symbol { ch, .. } => self.char_atom(*ch, node.span, ctx),
            NodeKind::BigOp { ch, .. } => {
                let scale = if ctx.style.style == Style::Display {
                    self.consts.display_op_scale
                } else {
                    1.0
                };
                self.op_glyph(*ch, node.span, ctx, scale)
            }
            NodeKind::OpName { name, .. } => {
                self.word_box(name, node.span, None, ctx, FACE_REGULAR)
            }
            NodeKind::TextRun { text, char_spans } => {
                self.word_box(text, node.span, Some(char_spans), ctx, text_face(ctx))
            }
            NodeKind::Text { body } => Ok(Laid {
                boxx: self.hlist(
                    body,
                    LayCtx {
                        text_mode: true,
                        ..ctx
                    },
                )?,
                italic: 0.0,
                char_glyph: None,
            }),
            NodeKind::TextStyled { style, body } => {
                let styled = LayCtx {
                    text_mode: true,
                    alphabet: Some(match style {
                        TextStyle::Bold => MathFont::Bold,
                        TextStyle::Emph => MathFont::Italic,
                        TextStyle::Underline => MathFont::Roman,
                    }),
                    ..ctx
                };
                let inner = self.hlist(body, styled)?;
                if matches!(style, TextStyle::Underline) {
                    Ok(Laid {
                        boxx: self.underline_box(inner, ctx, node.span),
                        italic: 0.0,
                        char_glyph: None,
                    })
                } else {
                    Ok(Laid {
                        boxx: inner,
                        italic: 0.0,
                        char_glyph: None,
                    })
                }
            }
            NodeKind::MathIsland { body, display } => {
                let style = if *display {
                    Style::Display
                } else {
                    Style::Text
                };
                Ok(Laid {
                    boxx: self.hlist(
                        body,
                        LayCtx {
                            style: StyleCtx::new(style),
                            alphabet: None,
                            text_mode: false,
                        },
                    )?,
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            NodeKind::MathFont { font, body } => self.lay_node(
                body,
                LayCtx {
                    alphabet: Some(*font),
                    ..ctx
                },
            ),
            NodeKind::Scripts {
                base,
                sub,
                sup,
                primes,
            } => self.scripts(base.as_deref(), sub.as_deref(), sup.as_deref(), primes, ctx),
            NodeKind::Frac { num, den, spec } => self.fraction(num, den, *spec, ctx, node.span),
            NodeKind::Radical { index, radicand } => {
                self.radical(index.as_deref(), radicand, ctx, node.span)
            }
            NodeKind::Accent { accent, base } => self.accent(*accent, base, ctx, node.span),
            NodeKind::LeftRight { left, right, body } => self.left_right(left, right, body, ctx),
            NodeKind::SizedDelim { size, delim, .. } => {
                let target = fixed_delim_target(*size) * ctx.size();
                let boxx = self.delimiter_box(delim, target, ctx)?;
                Ok(Laid {
                    boxx,
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            NodeKind::Phantom { kind, body } => {
                let inner = self.lay_node(body, ctx)?;
                let (w, h, d) = match kind {
                    PhantomKind::Full => (inner.boxx.width, inner.boxx.height, inner.boxx.depth),
                    PhantomKind::Horizontal => (inner.boxx.width, 0.0, 0.0),
                    PhantomKind::Vertical => (0.0, inner.boxx.height, inner.boxx.depth),
                };
                Ok(Laid {
                    boxx: MBox {
                        kind: BoxKind::Horizontal,
                        width: w,
                        height: h,
                        depth: d,
                        children: Vec::new(),
                    },
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            NodeKind::Stack {
                kind,
                annotation,
                base,
            } => {
                let base_laid = self.lay_node(base, ctx)?;
                let ann_ctx = match kind {
                    StackKind::Underset => ctx.map(StyleCtx::sub),
                    StackKind::Stackrel | StackKind::Overset => ctx.map(StyleCtx::sup),
                };
                let ann = self.lay_node(annotation, ann_ctx)?;
                let (upper, lower) = match kind {
                    StackKind::Underset => (None, Some(ann.boxx)),
                    StackKind::Stackrel | StackKind::Overset => (Some(ann.boxx), None),
                };
                Ok(Laid {
                    boxx: self.with_limits(base_laid.boxx, upper, lower, ctx, base_laid.italic),
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            NodeKind::Fragment(FragmentKind::StrayRight(delim)) => {
                if let Some(ch) = delim.ch {
                    self.char_atom(ch, delim.span, ctx)
                } else {
                    Ok(Laid {
                        boxx: kern_box(self.consts.null_delimiter_space * ctx.size()),
                        italic: 0.0,
                        char_glyph: None,
                    })
                }
            }
            NodeKind::Environment { name, .. } => Err(MathError::UnsupportedCommand {
                name: format!("env:{name}"),
                span: node.span,
            }),
            NodeKind::Fragment(_)
            | NodeKind::StyleChange(_)
            | NodeKind::ColorChange(_)
            | NodeKind::Space(_)
            | NodeKind::Tie
            | NodeKind::Linebreak
            | NodeKind::AlignTab => Ok(Laid {
                boxx: kern_box(0.0),
                italic: 0.0,
                char_glyph: None,
            }),
        }
    }

    // ── Glyph-level constructors ────────────────────────────────────────

    fn resolve_math_char(
        &self,
        ch: char,
        span: Span,
        ctx: LayCtx,
    ) -> Result<(FaceId, u16, char), MathError> {
        let (mapped, chain): (char, &[FaceId]) = match ctx.alphabet {
            Some(font) => alphabet_map(font, ch),
            None if ctx.text_mode => (ch, &[FACE_REGULAR, FACE_ITALIC, FACE_SYMBOLS]),
            None => (ch, default_math_chain(ch)),
        };
        match self.faces.resolve(mapped, chain) {
            Some((face, gid)) => Ok((face, gid, mapped)),
            None => Err(MathError::UnmappedChar { ch: mapped, span }),
        }
    }

    /// A single-character atom.
    fn char_atom(&self, ch: char, span: Span, ctx: LayCtx) -> Result<Laid, MathError> {
        let (face, gid, mapped) = self.resolve_math_char(ch, span, ctx)?;
        let metrics = self.metrics_of(face, gid);
        let size = ctx.size();
        Ok(Laid {
            boxx: glyph_box(face, gid, mapped, span, size, metrics),
            italic: metrics.italic * size,
            char_glyph: Some((face, gid)),
        })
    }

    /// A big-operator glyph, axis-centered, optionally display-scaled.
    fn op_glyph(&self, ch: char, span: Span, ctx: LayCtx, scale: f64) -> Result<Laid, MathError> {
        let (face, gid, mapped) = self.resolve_math_char(ch, span, ctx)?;
        let metrics = self.metrics_of(face, gid);
        let size = ctx.size() * scale;
        let gh = metrics.height * size;
        let gd = metrics.depth * size;
        let axis = self.consts.axis_height * ctx.size();
        // Center the ink on the math axis.
        let dy = axis - (gh - gd) / 2.0;
        let boxx = MBox {
            kind: BoxKind::Horizontal,
            width: metrics.advance * size,
            height: gh + dy,
            depth: (gd - dy).max(0.0),
            children: vec![Positioned {
                dx: 0.0,
                dy,
                node: MNode::Glyph {
                    face,
                    gid,
                    ch: mapped,
                    size,
                    span,
                },
            }],
        };
        Ok(Laid {
            boxx,
            italic: metrics.italic * size,
            char_glyph: None,
        })
    }

    /// A run of text glyphs from one preferred face (operator names, text
    /// runs), with kerning and interword spaces. `char_spans` carries one
    /// source span per character when the run has exact provenance (text
    /// runs); operator names fall back to the command's span, which is the
    /// documented synthetic-span policy.
    fn word_box(
        &self,
        text: &str,
        span: Span,
        char_spans: Option<&[Span]>,
        ctx: LayCtx,
        prefer: FaceId,
    ) -> Result<Laid, MathError> {
        let size = ctx.size();
        let mut children = Vec::new();
        let mut x = 0.0_f64;
        let mut height = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut prev: Option<(FaceId, u16)> = None;
        let mut last_italic = 0.0;
        for (i, ch) in text.chars().enumerate() {
            let ch_span = char_spans
                .and_then(|spans| spans.get(i))
                .copied()
                .unwrap_or(span);
            if ch == ' ' {
                x += self.space_width(ctx);
                prev = None;
                continue;
            }
            let chain = [prefer, FACE_REGULAR, FACE_SYMBOLS];
            let mapped = match ctx.alphabet {
                Some(font) => alphabet_map(font, ch).0,
                None => ch,
            };
            let Some((face, gid)) = self.faces.resolve(mapped, &chain) else {
                return Err(MathError::UnmappedChar {
                    ch: mapped,
                    span: ch_span,
                });
            };
            if let Some((pf, pg)) = prev {
                if pf == face {
                    if let Some(font) = self.faces.font(face) {
                        x += kern_em(font, pg, gid) * size;
                    }
                }
            }
            let m = self.metrics_of(face, gid);
            children.push(Positioned {
                dx: x,
                dy: 0.0,
                node: MNode::Glyph {
                    face,
                    gid,
                    ch: mapped,
                    size,
                    span: ch_span,
                },
            });
            x += m.advance * size;
            height = height.max(m.height * size);
            depth = depth.max(m.depth * size);
            last_italic = m.italic * size;
            prev = Some((face, gid));
        }
        Ok(Laid {
            boxx: MBox {
                kind: BoxKind::Horizontal,
                width: x,
                height,
                depth,
                children,
            },
            italic: last_italic,
            char_glyph: None,
        })
    }

    fn metrics_of(&self, face: FaceId, gid: u16) -> GlyphMetrics {
        self.faces
            .font(face)
            .map(|font| glyph_metrics(font, gid))
            .unwrap_or_default()
    }

    fn space_width(&self, ctx: LayCtx) -> f64 {
        let width = self
            .faces
            .font(FACE_REGULAR)
            .map(|font| {
                let gid = font.glyph_index(' ');
                if gid == 0 {
                    self.consts.fallback_space
                } else {
                    glyph_metrics(font, gid).advance
                }
            })
            .unwrap_or(self.consts.fallback_space);
        width * ctx.size()
    }

    // ── The Appendix-G constructions ────────────────────────────────────

    /// Rule 18: sub/superscripts (and primes, which are superscript
    /// material), including the simultaneous-scripts clash rules and the
    /// italic-correction shift of the superscript.
    #[allow(clippy::too_many_lines)]
    fn scripts(
        &self,
        base: Option<&Node>,
        sub: Option<&Node>,
        sup: Option<&Node>,
        primes: &[Span],
        ctx: LayCtx,
    ) -> Result<Laid, MathError> {
        // Big operators with active limits route to rule 13a instead.
        if let Some(base_node) = base {
            if self.limits_active(base_node, ctx) {
                let base_laid = self.lay_node(base_node, ctx)?;
                let upper = sup
                    .map(|n| self.lay_node(n, ctx.map(StyleCtx::sup)))
                    .transpose()?
                    .map(|l| l.boxx);
                let lower = sub
                    .map(|n| self.lay_node(n, ctx.map(StyleCtx::sub)))
                    .transpose()?
                    .map(|l| l.boxx);
                return Ok(Laid {
                    boxx: self.with_limits(base_laid.boxx, upper, lower, ctx, base_laid.italic),
                    italic: 0.0,
                    char_glyph: None,
                });
            }
        }
        let size = ctx.size();
        let c = &self.consts;
        let base_laid = match base {
            Some(node) => self.lay_node(node, ctx)?,
            None => Laid {
                boxx: kern_box(0.0),
                italic: 0.0,
                char_glyph: None,
            },
        };
        let base_is_char = base_laid.char_glyph.is_some() || base.is_none();
        let delta = base_laid.italic;
        // Rule 18a: baseline drops from the base's extremes (zero for
        // single characters).
        let (u, v) = if base_is_char {
            (0.0, 0.0)
        } else {
            (
                base_laid.boxx.height - c.sup_drop * size,
                base_laid.boxx.depth + c.sub_drop * size,
            )
        };
        // The superscript material: an explicit box, or a prime run.
        let sup_box = if primes.is_empty() {
            match sup {
                Some(node) => Some(self.lay_node(node, ctx.map(StyleCtx::sup))?.boxx),
                None => None,
            }
        } else {
            Some(self.prime_run(primes, ctx)?)
        };
        let sub_box = match sub {
            Some(node) => Some(self.lay_node(node, ctx.map(StyleCtx::sub))?.boxx),
            None => None,
        };
        let base_w = base_laid.boxx.width;
        let mut children = vec![Positioned {
            dx: 0.0,
            dy: 0.0,
            node: MNode::Box(base_laid.boxx),
        }];
        let mut width = base_w;
        let mut height = children[0].node.box_height();
        let mut depth = children[0].node.box_depth();
        match (sup_box, sub_box) {
            (None, None) => {}
            (None, Some(sb)) => {
                // Rule 18b: subscript alone.
                let shift = v
                    .max(c.sub1 * size)
                    .max(sb.height - 0.8 * c.x_height * size);
                width = width.max(base_w + sb.width);
                height = height.max(sb.height - shift);
                depth = depth.max(sb.depth + shift);
                children.push(Positioned {
                    dx: base_w,
                    dy: -shift,
                    node: MNode::Box(sb),
                });
            }
            (Some(sp), None) => {
                // Rule 18c: superscript alone.
                let p = script_p(c, ctx.style) * size;
                let shift = u.max(p).max(sp.depth + 0.25 * c.x_height * size);
                width = width.max(base_w + delta + sp.width);
                height = height.max(sp.height + shift);
                depth = depth.max(sp.depth - shift);
                children.push(Positioned {
                    dx: base_w + delta,
                    dy: shift,
                    node: MNode::Box(sp),
                });
            }
            (Some(sp), Some(sb)) => {
                // Rules 18d–f: both scripts; resolve the clash, then the
                // ⅘ x-height redistribution.
                let p = script_p(c, ctx.style) * size;
                let mut shift_up = u.max(p).max(sp.depth + 0.25 * c.x_height * size);
                let mut shift_down = v.max(c.sub2 * size);
                let theta = c.rule_thickness * size;
                let gap = (shift_up - sp.depth) - (sb.height - shift_down);
                if gap < 4.0 * theta {
                    shift_down += 4.0 * theta - gap;
                    let psi = 0.8 * c.x_height * size - (shift_up - sp.depth);
                    if psi > 0.0 {
                        shift_up += psi;
                        shift_down -= psi;
                    }
                }
                width = width.max(base_w + delta + sp.width).max(base_w + sb.width);
                height = height.max(sp.height + shift_up);
                depth = depth.max(sb.depth + shift_down);
                children.push(Positioned {
                    dx: base_w + delta,
                    dy: shift_up,
                    node: MNode::Box(sp),
                });
                children.push(Positioned {
                    dx: base_w,
                    dy: -shift_down,
                    node: MNode::Box(sb),
                });
            }
        }
        Ok(Laid {
            boxx: MBox {
                kind: BoxKind::Horizontal,
                width,
                height,
                depth,
                children,
            },
            italic: 0.0,
            char_glyph: None,
        })
    }

    fn limits_active(&self, base: &Node, ctx: LayCtx) -> bool {
        match &base.kind {
            NodeKind::BigOp {
                limits, integral, ..
            } => match limits {
                Limits::Limits => true,
                Limits::NoLimits => false,
                Limits::Default => ctx.style.style == Style::Display && !integral,
            },
            NodeKind::OpName { limits, .. } => *limits && ctx.style.style == Style::Display,
            _ => false,
        }
    }

    /// A run of prime marks as superscript material, each carrying its own
    /// `'` token's span.
    fn prime_run(&self, primes: &[Span], ctx: LayCtx) -> Result<MBox, MathError> {
        let sup_ctx = ctx.map(StyleCtx::sup);
        let size = sup_ctx.size();
        let fallback = primes.first().copied().unwrap_or(Span::new(0, 0));
        let Some((face, gid)) = self.faces.resolve('′', &[FACE_REGULAR, FACE_SYMBOLS]) else {
            return Err(MathError::UnmappedChar {
                ch: '′',
                span: fallback,
            });
        };
        let m = self.metrics_of(face, gid);
        let mut children = Vec::new();
        let mut x = 0.0;
        for span in primes {
            children.push(Positioned {
                dx: x,
                dy: 0.0,
                node: MNode::Glyph {
                    face,
                    gid,
                    ch: '′',
                    size,
                    span: *span,
                },
            });
            x += m.advance * size;
        }
        Ok(MBox {
            kind: BoxKind::Horizontal,
            width: x,
            height: m.height * size,
            depth: m.depth * size,
            children,
        })
    }

    /// Rules 13/13a: limits above and below a big operator (also the
    /// `\overset` family, which amsmath defines through the same rules).
    fn with_limits(
        &self,
        op: MBox,
        upper: Option<MBox>,
        lower: Option<MBox>,
        ctx: LayCtx,
        delta: f64,
    ) -> MBox {
        let size = ctx.size();
        let c = &self.consts;
        let width = op
            .width
            .max(upper.as_ref().map_or(0.0, |b| b.width))
            .max(lower.as_ref().map_or(0.0, |b| b.width));
        let op_dx = (width - op.width) / 2.0;
        let mut height = op.height;
        let mut depth = op.depth;
        let mut children = Vec::new();
        if let Some(up) = upper {
            let gap = (c.big_op_spacing1 * size).max(c.big_op_spacing3 * size - up.depth);
            let dy = op.height + gap + up.depth;
            height = dy + up.height + c.big_op_spacing5 * size;
            children.push(Positioned {
                dx: (width - up.width) / 2.0 + delta / 2.0,
                dy,
                node: MNode::Box(up),
            });
        }
        if let Some(low) = lower {
            let gap = (c.big_op_spacing2 * size).max(c.big_op_spacing4 * size - low.height);
            let dy = -(op.depth + gap + low.height);
            depth = -dy + low.depth + c.big_op_spacing5 * size;
            children.push(Positioned {
                dx: (width - low.width) / 2.0 - delta / 2.0,
                dy,
                node: MNode::Box(low),
            });
        }
        children.push(Positioned {
            dx: op_dx,
            dy: 0.0,
            node: MNode::Box(op),
        });
        MBox {
            kind: BoxKind::Horizontal,
            width,
            height,
            depth,
            children,
        }
    }

    /// Rule 15: generalized fractions (with or without bar, with or
    /// without wrapped delimiters).
    fn fraction(
        &self,
        num: &Node,
        den: &Node,
        spec: FracSpec,
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        let c = &self.consts;
        let eff = match spec.forced_style {
            Some(forced) => ctx.map(|s| StyleCtx {
                style: forced,
                cramped: s.cramped,
            }),
            None => ctx,
        };
        let display = eff.style.style == Style::Display;
        let size = eff.size();
        let num_box = self.lay_node(num, eff.map(StyleCtx::num))?.boxx;
        let den_box = self.lay_node(den, eff.map(StyleCtx::den))?.boxx;
        let theta = c.rule_thickness * size;
        let axis = c.axis_height * size;
        // Rule 15b: initial shifts.
        let mut u = if display {
            c.num1
        } else if spec.bar {
            c.num2
        } else {
            c.num3
        } * size;
        let mut v = if display { c.denom1 } else { c.denom2 } * size;
        if spec.bar {
            // Rule 15d: clearances against the bar.
            let phi = if display { 3.0 * theta } else { theta };
            let num_gap = (u - num_box.depth) - (axis + theta / 2.0);
            if num_gap < phi {
                u += phi - num_gap;
            }
            let den_gap = (axis - theta / 2.0) - (den_box.height - v);
            if den_gap < phi {
                v += phi - den_gap;
            }
        } else {
            // Rule 15c: minimum clearance, split evenly.
            let phi = if display { 7.0 * theta } else { 3.0 * theta };
            let gap = (u - num_box.depth) - (den_box.height - v);
            if gap < phi {
                u += (phi - gap) / 2.0;
                v += (phi - gap) / 2.0;
            }
        }
        let inner_width = num_box.width.max(den_box.width);
        let num_dx = (inner_width - num_box.width) / 2.0;
        let den_dx = (inner_width - den_box.width) / 2.0;
        let height = u + num_box.height;
        let depth = v + den_box.depth;
        let mut children = vec![
            Positioned {
                dx: num_dx,
                dy: u,
                node: MNode::Box(num_box),
            },
            Positioned {
                dx: den_dx,
                dy: -v,
                node: MNode::Box(den_box),
            },
        ];
        if spec.bar {
            children.push(Positioned {
                dx: 0.0,
                dy: axis - theta / 2.0,
                node: MNode::Rule {
                    width: inner_width,
                    height: theta,
                    span,
                },
            });
        }
        let core = MBox {
            kind: BoxKind::Horizontal,
            width: inner_width,
            height,
            depth,
            children,
        };
        let boxx = if let Some((l, r)) = spec.delims {
            // Rule 15e: wrap in delimiters of the style-fixed size.
            let target = if display { c.delim1 } else { c.delim2 } * size;
            let left = self.delimiter_box(&Delim { ch: Some(l), span }, target, eff)?;
            let right = self.delimiter_box(&Delim { ch: Some(r), span }, target, eff)?;
            hcat(vec![left, core, right])
        } else {
            core
        };
        Ok(Laid {
            boxx,
            italic: 0.0,
            char_glyph: None,
        })
    }

    /// Rule 11: radicals, with the plain-TeX degree placement.
    fn radical(
        &self,
        index: Option<&Node>,
        radicand: &Node,
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        let c = &self.consts;
        let size = ctx.size();
        let x = self.lay_node(radicand, ctx.map(StyleCtx::cramp))?.boxx;
        let theta = c.rule_thickness * size;
        let mut psi = if ctx.style.style == Style::Display {
            theta + 0.25 * c.x_height * size
        } else {
            theta + 0.25 * theta
        };
        let target = x.height + x.depth + psi + theta;
        let sign = self.delimiter_box(
            &Delim {
                ch: Some('√'),
                span,
            },
            target,
            ctx,
        )?;
        let sign_total = sign.height + sign.depth;
        let excess = sign_total - target;
        if excess > 0.0 {
            psi += excess / 2.0;
        }
        // Vertical frame: radicand baseline at 0; rule sits ψ above the
        // radicand's top; the sign's top aligns with the rule's top.
        let rule_y = x.height + psi;
        let sign_dy = rule_y + theta - sign.height;
        let mut pen = 0.0_f64;
        let mut children = Vec::new();
        let mut total_height = rule_y + theta;
        // Degree: raised 60% of the sign's extent, kerned 5mu in and
        // −10.5mu back (plain TeX's \root placement).
        if let Some(ix) = index {
            let ix_box = self
                .lay_node(
                    ix,
                    LayCtx {
                        style: StyleCtx {
                            style: Style::ScriptScript,
                            cramped: ctx.style.cramped,
                        },
                        ..ctx
                    },
                )?
                .boxx;
            // Plain TeX raises the degree by 60% of the radical construct's
            // total extent, measured from the baseline.
            let raise = 0.6 * (rule_y + theta + x.depth);
            pen += 5.0 / 18.0 * size;
            total_height = total_height.max(raise + ix_box.height);
            let ix_width = ix_box.width;
            children.push(Positioned {
                dx: pen,
                dy: raise,
                node: MNode::Box(ix_box),
            });
            pen += ix_width - 10.5 / 18.0 * size;
            pen = pen.max(0.0);
        }
        let sign_width = sign.width;
        children.push(Positioned {
            dx: pen,
            dy: sign_dy,
            node: MNode::Box(sign),
        });
        pen += sign_width;
        children.push(Positioned {
            dx: pen,
            dy: rule_y,
            node: MNode::Rule {
                width: x.width,
                height: theta,
                span,
            },
        });
        let x_width = x.width;
        let x_depth = x.depth;
        children.push(Positioned {
            dx: pen,
            dy: 0.0,
            node: MNode::Box(x),
        });
        Ok(Laid {
            boxx: MBox {
                kind: BoxKind::Horizontal,
                width: pen + x_width,
                height: total_height,
                depth: x_depth,
                children,
            },
            italic: 0.0,
            char_glyph: None,
        })
    }

    /// Rule 12: accents, with the synthesized-skew centering; rules 9/10
    /// for `\overline`/`\underline`.
    fn accent(
        &self,
        kind: AccentKind,
        base: &Node,
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        let c = &self.consts;
        let size = ctx.size();
        match kind {
            AccentKind::OverLine => {
                let inner = self.lay_node(base, ctx.map(StyleCtx::cramp))?.boxx;
                let theta = c.rule_thickness * size;
                let rule_y = inner.height + 3.0 * theta;
                let width = inner.width;
                Ok(Laid {
                    boxx: MBox {
                        kind: BoxKind::Horizontal,
                        width,
                        height: rule_y + 2.0 * theta,
                        depth: inner.depth,
                        children: vec![
                            Positioned {
                                dx: 0.0,
                                dy: rule_y,
                                node: MNode::Rule {
                                    width,
                                    height: theta,
                                    span,
                                },
                            },
                            Positioned {
                                dx: 0.0,
                                dy: 0.0,
                                node: MNode::Box(inner),
                            },
                        ],
                    },
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            AccentKind::UnderLine => {
                let inner = self.lay_node(base, ctx)?.boxx;
                Ok(Laid {
                    boxx: self.underline_box(inner, ctx, span),
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            AccentKind::OverBrace
            | AccentKind::UnderBrace
            | AccentKind::OverRightArrow
            | AccentKind::OverLeftArrow
            | AccentKind::WideHat
            | AccentKind::WideTilde => Err(MathError::UnsupportedCommand {
                name: format!("\\{}", stretchy_name(kind)),
                span,
            }),
            _ => {
                let base_laid = self.lay_node(base, ctx.map(StyleCtx::cramp))?;
                let combining = accent_char(kind);
                let resolved = self
                    .faces
                    .resolve(combining, &[FACE_REGULAR, FACE_ITALIC, FACE_SYMBOLS])
                    .or_else(|| {
                        accent_spacing_fallback(combining)
                            .and_then(|alt| self.faces.resolve(alt, &[FACE_REGULAR, FACE_SYMBOLS]))
                    });
                let Some((face, gid)) = resolved else {
                    return Err(MathError::UnmappedChar {
                        ch: combining,
                        span,
                    });
                };
                let Some(font) = self.faces.font(face) else {
                    return Err(MathError::UnmappedChar {
                        ch: combining,
                        span,
                    });
                };
                let upm = f64::from(font.units_per_em.max(1));
                let bbox = font.glyph_bbox(gid).unwrap_or([0, 0, 0, 0]);
                let ink_left = f64::from(bbox[0]) / upm * size;
                let ink_right = f64::from(bbox[2]) / upm * size;
                let ink_top = f64::from(bbox[3]) / upm * size;
                let ink_bottom = f64::from(bbox[1]) / upm * size;
                // Rule 12: raise by the base's height over x-height; center
                // ink over the base, skewed by half the italic correction.
                let dy = base_laid.boxx.height - c.x_height * size;
                let skew = base_laid.italic / 2.0;
                let dx = base_laid.boxx.width / 2.0 + skew - (ink_left + ink_right) / 2.0;
                let width = base_laid.boxx.width;
                let height = base_laid.boxx.height.max(dy + ink_top);
                let depth = base_laid.boxx.depth.max(-(dy + ink_bottom));
                let base_box = base_laid.boxx;
                Ok(Laid {
                    boxx: MBox {
                        kind: BoxKind::Horizontal,
                        width,
                        height,
                        depth,
                        children: vec![
                            Positioned {
                                dx,
                                dy,
                                node: MNode::Glyph {
                                    face,
                                    gid,
                                    ch: combining,
                                    size,
                                    span,
                                },
                            },
                            Positioned {
                                dx: 0.0,
                                dy: 0.0,
                                node: MNode::Box(base_box),
                            },
                        ],
                    },
                    italic: 0.0,
                    char_glyph: None,
                })
            }
        }
    }

    fn underline_box(&self, inner: MBox, ctx: LayCtx, span: Span) -> MBox {
        let theta = self.consts.rule_thickness * ctx.size();
        let rule_y = -(inner.depth + 3.0 * theta) - theta;
        let width = inner.width;
        MBox {
            kind: BoxKind::Horizontal,
            width,
            height: inner.height,
            depth: -rule_y + 2.0 * theta,
            children: vec![
                Positioned {
                    dx: 0.0,
                    dy: rule_y,
                    node: MNode::Rule {
                        width,
                        height: theta,
                        span,
                    },
                },
                Positioned {
                    dx: 0.0,
                    dy: 0.0,
                    node: MNode::Box(inner),
                },
            ],
        }
    }

    /// Rule 19: `\left…\right`.
    fn left_right(
        &self,
        left: &Delim,
        right: &Delim,
        body: &[Node],
        ctx: LayCtx,
    ) -> Result<Laid, MathError> {
        let c = &self.consts;
        let size = ctx.size();
        let inner = self.hlist(body, ctx)?;
        let axis = c.axis_height * size;
        let delta = (inner.height - axis).max(inner.depth + axis);
        let target =
            (2.0 * delta * c.delimiter_factor).max(2.0 * delta - c.delimiter_shortfall * size);
        let left_box = self.delimiter_box(left, target, ctx)?;
        let right_box = self.delimiter_box(right, target, ctx)?;
        Ok(Laid {
            boxx: hcat(vec![left_box, inner, right_box]),
            italic: 0.0,
            char_glyph: None,
        })
    }

    /// The ADR-0005 delimiter mechanism: natural glyph if it covers the
    /// target, uniform scaling above (drawn-path construction beyond the
    /// ceiling arrives with the extensions bead), axis-centered either
    /// way; the null delimiter is a `nulldelimiterspace` kern.
    fn delimiter_box(
        &self,
        delim: &Delim,
        target_total: f64,
        ctx: LayCtx,
    ) -> Result<MBox, MathError> {
        let Some(ch) = delim.ch else {
            return Ok(kern_box(self.consts.null_delimiter_space * ctx.size()));
        };
        let Some((face, gid)) = self
            .faces
            .resolve(ch, &[FACE_REGULAR, FACE_SYMBOLS, FACE_ITALIC])
        else {
            return Err(MathError::UnmappedChar {
                ch,
                span: delim.span,
            });
        };
        let m = self.metrics_of(face, gid);
        let natural_total = (m.height + m.depth) * ctx.size();
        let scale = if natural_total >= target_total || natural_total <= 0.0 {
            1.0
        } else {
            target_total / natural_total
        };
        let size = ctx.size() * scale;
        let gh = m.height * size;
        let gd = m.depth * size;
        let axis = self.consts.axis_height * ctx.size();
        let dy = axis - (gh - gd) / 2.0;
        Ok(MBox {
            kind: BoxKind::Horizontal,
            width: m.advance * size,
            height: gh + dy,
            depth: (gd - dy).max(0.0),
            children: vec![Positioned {
                dx: 0.0,
                dy,
                node: MNode::Glyph {
                    face,
                    gid,
                    ch,
                    size,
                    span: delim.span,
                },
            }],
        })
    }
}

/// σ13/σ14/σ15 selection for rule 18.
fn script_p(c: &MathConstants, style: StyleCtx) -> f64 {
    if style.cramped {
        c.sup3
    } else if style.style == Style::Display {
        c.sup1
    } else {
        c.sup2
    }
}

/// The `\big` family's total-size targets in ems (plain TeX's 8.5 pt /
/// 11.5 pt / 14.5 pt / 17.5 pt at 10 pt).
fn fixed_delim_target(size: DelimSize) -> f64 {
    match size {
        DelimSize::Big => 0.85,
        DelimSize::BBig => 1.15,
        DelimSize::Bigg => 1.45,
        DelimSize::BBigg => 1.75,
    }
}

fn text_face(ctx: LayCtx) -> FaceId {
    match ctx.alphabet {
        Some(MathFont::Bold) => FACE_BOLD,
        Some(MathFont::Italic) => FACE_ITALIC,
        _ => FACE_REGULAR,
    }
}

const fn accent_char(kind: AccentKind) -> char {
    match kind {
        AccentKind::Hat => '\u{0302}',
        AccentKind::Check => '\u{030C}',
        AccentKind::Tilde => '\u{0303}',
        AccentKind::Acute => '\u{0301}',
        AccentKind::Grave => '\u{0300}',
        AccentKind::Dot => '\u{0307}',
        AccentKind::Ddot => '\u{0308}',
        AccentKind::Breve => '\u{0306}',
        AccentKind::Bar => '\u{0304}',
        AccentKind::Vec => '\u{20D7}',
        AccentKind::Ring => '\u{030A}',
        _ => '\u{0302}',
    }
}

const fn stretchy_name(kind: AccentKind) -> &'static str {
    match kind {
        AccentKind::OverBrace => "overbrace",
        AccentKind::UnderBrace => "underbrace",
        AccentKind::OverRightArrow => "overrightarrow",
        AccentKind::OverLeftArrow => "overleftarrow",
        AccentKind::WideHat => "widehat",
        _ => "widetilde",
    }
}

/// A zero-height horizontal kern box.
fn kern_box(width: f64) -> MBox {
    MBox {
        kind: BoxKind::Horizontal,
        width,
        height: 0.0,
        depth: 0.0,
        children: Vec::new(),
    }
}

/// Concatenate boxes horizontally on a shared baseline.
fn hcat(boxes: Vec<MBox>) -> MBox {
    let mut x = 0.0_f64;
    let mut height = 0.0_f64;
    let mut depth = 0.0_f64;
    let mut children = Vec::new();
    for b in boxes {
        height = height.max(b.height);
        depth = depth.max(b.depth);
        let w = b.width;
        children.push(Positioned {
            dx: x,
            dy: 0.0,
            node: MNode::Box(b),
        });
        x += w;
    }
    MBox {
        kind: BoxKind::Horizontal,
        width: x,
        height,
        depth,
        children,
    }
}

impl MNode {
    fn box_height(&self) -> f64 {
        match self {
            Self::Box(b) => b.height,
            _ => 0.0,
        }
    }
    fn box_depth(&self) -> f64 {
        match self {
            Self::Box(b) => b.depth,
            _ => 0.0,
        }
    }
}

/// A single positioned glyph box.
fn glyph_box(face: FaceId, gid: u16, ch: char, span: Span, size: f64, m: GlyphMetrics) -> MBox {
    MBox {
        kind: BoxKind::Horizontal,
        width: m.advance * size,
        height: m.height * size,
        depth: m.depth * size,
        children: vec![Positioned {
            dx: 0.0,
            dy: 0.0,
            node: MNode::Glyph {
                face,
                gid,
                ch,
                size,
                span,
            },
        }],
    }
}
