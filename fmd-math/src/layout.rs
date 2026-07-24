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
//! **Delimiter mechanism (ADR-0005), complete.** Natural glyph when it
//! covers the rule-19 target; uniform scaling up to the `1.25×` ceiling;
//! the parametric drawn-path construction beyond ([`crate::drawn`], the
//! mainline) — no requested size can fail, by construction. The same
//! module serves the drawn surd and the stretchy over/under constructions
//! (`\overbrace`, `\underbrace`, `\overrightarrow`, `\overleftarrow`,
//! `\widehat`, `\widetilde`).
//!
//! **Environments** (the matrix family, `cases`, `array` with column
//! specs, the `align*` class) lay out through the grid engine: shared
//! column measurement, per-column alignment, `\baselineskip`/`\lineskip`
//! row stacking (`\jot`-opened for `align`), `\vcenter` axis centering,
//! and the rule-19 delimiter wrap for the delimited matrices.

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

    /// [`Engine::typeset`], against a macro set (a preamble pack and/or
    /// caller definitions).
    ///
    /// # Errors
    ///
    /// As [`Engine::typeset`], plus the macro-expansion errors.
    pub fn typeset_with_macros(
        &self,
        source: &str,
        style: Style,
        macros: &crate::macros::MacroSet,
    ) -> Result<Layout, MathError> {
        let root = crate::parse_with_macros(source, macros)?;
        self.finish(&root, StyleCtx::new(style))
    }

    /// [`Engine::typeset_text`], against a macro set.
    ///
    /// # Errors
    ///
    /// As [`Engine::typeset_with_macros`].
    pub fn typeset_text_with_macros(
        &self,
        source: &str,
        macros: &crate::macros::MacroSet,
    ) -> Result<Layout, MathError> {
        let root = crate::parse_text_with_macros(source, macros)?;
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
            NodeKind::Environment { name, spec, rows } => {
                self.environment(name, spec.as_deref(), rows, ctx, node.span)
            }
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
            // `\overbrace`/`\underbrace` are `\mathop…\limits` in both plain
            // TeX and amsmath: their scripts center above/below in every
            // style (`\overbrace{x+y}^{n}` puts the n over the brace).
            NodeKind::Accent { accent, .. } => {
                matches!(accent, AccentKind::OverBrace | AccentKind::UnderBrace)
            }
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
            | AccentKind::WideTilde => self.stretchy_accent(kind, base, ctx, span),
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

    /// Environment layout: the matrix family, `cases`, `array` with column
    /// specs, and the `align*` class — column measurement, per-column
    /// alignment, and inter-row spacing per the published rules
    /// (`\arraycolsep`-derived column separation, `\baselineskip` with
    /// `\lineskip` growth between rows, `\jot` opening the `align` rows,
    /// `\vcenter` axis centering), with the matrix family wrapped by the
    /// rule-19 delimiter mechanism.
    fn environment(
        &self,
        name: &str,
        spec: Option<&str>,
        rows: &[Vec<Node>],
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        // amsmath sets matrix/cases cells in text style (script styles keep
        // their size); align-class cells are display style.
        let text_cells = ctx.map(|s| StyleCtx {
            style: if s.style == Style::Display {
                Style::Text
            } else {
                s.style
            },
            cramped: s.cramped,
        });
        let boxx = match name {
            "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" => {
                let grid = self.grid(rows, text_cells, &Grid::centered(1.0), span)?;
                match name {
                    // A bare matrix has no delimiters at all (not even the
                    // null-delimiter kerns a `\left.` would add).
                    "matrix" => grid,
                    "pmatrix" => self.wrap_delims(grid, Some('('), Some(')'), ctx, span)?,
                    "bmatrix" => self.wrap_delims(grid, Some('['), Some(']'), ctx, span)?,
                    "Bmatrix" => self.wrap_delims(grid, Some('{'), Some('}'), ctx, span)?,
                    "vmatrix" => self.wrap_delims(grid, Some('|'), Some('|'), ctx, span)?,
                    _ => self.wrap_delims(grid, Some('‖'), Some('‖'), ctx, span)?,
                }
            }
            "smallmatrix" => {
                // The inline matrix: script-size cells, tightened columns
                // and rows (amsmath's 0.7-factor feel).
                let cells = ctx.map(StyleCtx::sup);
                self.grid(
                    rows,
                    cells,
                    &Grid {
                        col_sep: 0.35,
                        row_factor: 0.7,
                        ..Grid::centered(1.0)
                    },
                    span,
                )?
            }
            "cases" => {
                // Text-style cells, left-aligned value and condition
                // columns a quad apart, behind a stretched `{`.
                let grid = self.grid(
                    rows,
                    text_cells,
                    &Grid {
                        align: AlignRule::AllLeft,
                        ..Grid::centered(1.0)
                    },
                    span,
                )?;
                self.wrap_delims(grid, Some('{'), None, ctx, span)?
            }
            "array" => {
                let plan = parse_array_spec(spec.unwrap_or(""), span)?;
                self.grid(
                    rows,
                    text_cells,
                    &Grid {
                        align: AlignRule::Columns(plan.aligns),
                        vrules: plan.vrules,
                        outer_pad: 0.5,
                        ..Grid::centered(1.0)
                    },
                    span,
                )?
            }
            "align" | "align*" | "aligned" => {
                // Display-style cells; r,l alternation with the alignment
                // point closed up; \minalignsep between pairs; \jot opens
                // the rows.
                let cells = ctx.map(|s| StyleCtx {
                    style: Style::Display,
                    cramped: s.cramped,
                });
                self.grid(
                    rows,
                    cells,
                    &Grid {
                        align: AlignRule::AlignPairs,
                        col_sep: 1.0,
                        jot: 0.3,
                        ..Grid::centered(1.0)
                    },
                    span,
                )?
            }
            other => {
                // Parse admits only known environments; a new name landing
                // here is a construct the layout tier does not cover yet.
                return Err(MathError::UnsupportedCommand {
                    name: format!("env:{other}"),
                    span,
                });
            }
        };
        Ok(Laid {
            boxx,
            italic: 0.0,
            char_glyph: None,
        })
    }

    /// Measure and assemble a cell grid: shared column widths, per-row
    /// height/depth, baseline stacking, and `\vcenter` axis centering.
    fn grid(
        &self,
        rows: &[Vec<Node>],
        cell_ctx: LayCtx,
        grid: &Grid,
        span: Span,
    ) -> Result<MBox, MathError> {
        let size = cell_ctx.size();
        let c = &self.consts;
        // 1. Lay every cell.
        let mut cells: Vec<Vec<MBox>> = Vec::new();
        for row in rows {
            let mut out = Vec::new();
            for cell in row {
                out.push(self.lay_node(cell, cell_ctx)?.boxx);
            }
            cells.push(out);
        }
        let ncols = cells.iter().map(Vec::len).max().unwrap_or(0);
        // 2. Column widths and row metrics.
        let mut col_w = vec![0.0_f64; ncols];
        for row in &cells {
            for (j, cell) in row.iter().enumerate() {
                col_w[j] = col_w[j].max(cell.width);
            }
        }
        let row_h: Vec<f64> = cells
            .iter()
            .map(|r| r.iter().map(|b| b.height).fold(0.0, f64::max))
            .collect();
        let row_d: Vec<f64> = cells
            .iter()
            .map(|r| r.iter().map(|b| b.depth).fold(0.0, f64::max))
            .collect();
        // 3. Column x positions.
        let mut col_x = vec![0.0_f64; ncols];
        let mut x = grid.outer_pad * size;
        for (j, w) in col_w.iter().enumerate() {
            if j > 0 {
                x += grid.sep_before(j) * size;
            }
            col_x[j] = x;
            x += w;
        }
        let total_width = x + grid.outer_pad * size;
        // 4. Row baselines: \baselineskip (+\jot) grown by \lineskip when
        // boxes would touch — the same stacking rule multi-line hlists use.
        let mut baselines = vec![0.0_f64; cells.len()];
        for i in 1..cells.len() {
            let natural = (c.baseline_skip + grid.jot) * grid.row_factor * size;
            let min_gap = row_d[i - 1] + row_h[i] + c.line_skip * size;
            baselines[i] = baselines[i - 1] - natural.max(min_gap);
        }
        // 5. Assemble, then axis-center the whole grid (\vcenter).
        let top = row_h.first().copied().unwrap_or(0.0);
        let bottom =
            baselines.last().copied().unwrap_or(0.0) - row_d.last().copied().unwrap_or(0.0);
        let total = top - bottom;
        let axis = c.axis_height * cell_ctx.size();
        let height = total / 2.0 + axis;
        let shift = height - top; // added to every child dy
        let depth = (total / 2.0 - axis).max(0.0);
        let mut children = Vec::new();
        for (i, row) in cells.into_iter().enumerate() {
            for (j, cell) in row.into_iter().enumerate() {
                let slack = col_w[j] - cell.width;
                let dx = col_x[j] + grid.align.factor(j) * slack;
                children.push(Positioned {
                    dx,
                    dy: baselines[i] + shift,
                    node: MNode::Box(cell),
                });
            }
        }
        // 6. Vertical rules from an `array` spec span the full grid.
        for &before_col in &grid.vrules {
            let theta = c.rule_thickness * size;
            let rule_x = if before_col == 0 {
                (grid.outer_pad * size - theta).max(0.0) / 2.0
            } else if before_col < ncols {
                col_x[before_col] - grid.sep_before(before_col) * size / 2.0 - theta / 2.0
            } else {
                total_width - (grid.outer_pad * size - theta).max(0.0) / 2.0 - theta
            };
            children.push(Positioned {
                dx: rule_x,
                dy: -depth,
                node: MNode::Rule {
                    width: theta,
                    height: height + depth,
                    span,
                },
            });
        }
        Ok(MBox {
            kind: BoxKind::Vertical,
            width: total_width,
            height,
            depth,
            children,
        })
    }

    /// Wrap a box in rule-19-sized delimiters (the matrix family, `cases`).
    fn wrap_delims(
        &self,
        inner: MBox,
        left: Option<char>,
        right: Option<char>,
        ctx: LayCtx,
        span: Span,
    ) -> Result<MBox, MathError> {
        let c = &self.consts;
        let size = ctx.size();
        let axis = c.axis_height * size;
        let delta = (inner.height - axis).max(inner.depth + axis);
        let target =
            (2.0 * delta * c.delimiter_factor).max(2.0 * delta - c.delimiter_shortfall * size);
        let left_box = self.delimiter_box(&Delim { ch: left, span }, target, ctx)?;
        let right_box = self.delimiter_box(&Delim { ch: right, span }, target, ctx)?;
        Ok(hcat(vec![left_box, inner, right_box]))
    }

    /// The stretchy over/under constructions (`\widehat`, `\widetilde`,
    /// `\overbrace`, `\underbrace`, `\overrightarrow`, `\overleftarrow`):
    /// a drawn band spanning the base's width, placed with a small
    /// rule-thickness clearance (hats and tildes ride close, the way the
    /// authored accents do; braces and arrows take a little more air).
    /// Any width draws — the constructions are total.
    fn stretchy_accent(
        &self,
        kind: AccentKind,
        base: &Node,
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        let size = ctx.size();
        let theta = self.consts.rule_thickness * size;
        let under = matches!(kind, AccentKind::UnderBrace);
        // Over-accents cramp their base (rule 12 / overline's rule 9);
        // under-constructions do not (underline's rule 10).
        let inner = if under {
            self.lay_node(base, ctx)?.boxx
        } else {
            self.lay_node(base, ctx.map(StyleCtx::cramp))?.boxx
        };
        let stretch_kind = match kind {
            AccentKind::WideHat => crate::drawn::Stretch::Hat,
            AccentKind::WideTilde => crate::drawn::Stretch::Tilde,
            AccentKind::OverBrace => crate::drawn::Stretch::OverBrace,
            AccentKind::UnderBrace => crate::drawn::Stretch::UnderBrace,
            AccentKind::OverRightArrow => crate::drawn::Stretch::RightArrow,
            _ => crate::drawn::Stretch::LeftArrow,
        };
        let width = inner.width.max(0.25 * size);
        let band = crate::drawn::stretch(stretch_kind, width, size);
        let gap = match kind {
            AccentKind::WideHat | AccentKind::WideTilde => theta,
            _ => 2.0 * theta,
        };
        let (band_dy, height, depth) = if under {
            let dy = -(inner.depth + gap + band.height);
            (dy, inner.height, inner.depth + gap + band.height)
        } else {
            let dy = inner.height + gap;
            (dy, inner.height + gap + band.height, inner.depth)
        };
        let inner_width = inner.width;
        Ok(Laid {
            boxx: MBox {
                kind: BoxKind::Horizontal,
                width: inner_width.max(width),
                height,
                depth,
                children: vec![
                    Positioned {
                        dx: 0.0,
                        dy: band_dy,
                        node: MNode::Path {
                            contours: band.contours,
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

    /// The ADR-0005 delimiter mechanism, complete: natural glyph if it
    /// covers the target; uniform scaling up to the `1.25×` ceiling; the
    /// parametric drawn-path construction beyond (the mainline — stroke
    /// weights calibrated against the authored glyphs so the threshold
    /// seam is invisible at a glance, and no requested size can fail).
    /// Axis-centered every way; the null delimiter is a
    /// `nulldelimiterspace` kern. Characters without a drawn construction
    /// keep uniform scaling — nothing regresses past the ceiling.
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
            // No authored glyph at all (e.g. `‖` on a face without it): the
            // drawn construction serves every size, floored at a text-ish
            // height so an empty body still gets a visible delimiter.
            if let Some(d) =
                crate::drawn::delimiter(ch, target_total.max(0.7 * ctx.size()), ctx.size())
            {
                let total = target_total.max(0.7 * ctx.size());
                let axis = self.consts.axis_height * ctx.size();
                let dy = axis - total / 2.0;
                return Ok(MBox {
                    kind: BoxKind::Horizontal,
                    width: d.width,
                    height: total + dy,
                    depth: (-dy).max(0.0),
                    children: vec![Positioned {
                        dx: 0.0,
                        dy,
                        node: MNode::Path {
                            contours: d.contours,
                            span: delim.span,
                        },
                    }],
                });
            }
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
        if scale > self.consts.delimiter_scale_ceiling
            && let Some(d) = crate::drawn::delimiter(ch, target_total, ctx.size())
        {
            // The drawn mainline: contours span y ∈ [0, target]; center
            // the construction on the axis exactly as a glyph would be.
            let axis = self.consts.axis_height * ctx.size();
            let dy = axis - target_total / 2.0;
            return Ok(MBox {
                kind: BoxKind::Horizontal,
                width: d.width,
                height: target_total + dy,
                depth: (-dy).max(0.0),
                children: vec![Positioned {
                    dx: 0.0,
                    dy,
                    node: MNode::Path {
                        contours: d.contours,
                        span: delim.span,
                    },
                }],
            });
        }
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

/// Grid parameters for one environment family (ems, of the cell size).
struct Grid {
    /// Per-column horizontal alignment.
    align: AlignRule,
    /// Space between adjacent columns (`2\arraycolsep`-derived; the
    /// `align` rule reads it as the between-pairs `\minalignsep`).
    col_sep: f64,
    /// Padding at the grid's outer edges (`array`'s `\arraycolsep`).
    outer_pad: f64,
    /// Extra opening of the natural row skip (`align`'s `\jot`).
    jot: f64,
    /// Multiplier on the natural row skip (`smallmatrix` tightening).
    row_factor: f64,
    /// Vertical rules before these column indices (an `array` spec's `|`;
    /// `ncols` means after the last column).
    vrules: Vec<usize>,
}

impl Grid {
    /// All-centered columns with the given separation — the matrix baseline.
    fn centered(col_sep: f64) -> Self {
        Self {
            align: AlignRule::AllCenter,
            col_sep,
            outer_pad: 0.0,
            jot: 0.0,
            row_factor: 1.0,
            vrules: Vec::new(),
        }
    }

    /// The separation inserted before column `j` (j ≥ 1). The `align` rule
    /// closes up the r,l pair around its alignment point and separates
    /// *pairs* by `col_sep`.
    fn sep_before(&self, j: usize) -> f64 {
        if matches!(self.align, AlignRule::AlignPairs) && j % 2 == 1 {
            0.0
        } else {
            self.col_sep
        }
    }
}

/// How a grid aligns cells within their columns.
enum AlignRule {
    /// Every column centered (the matrix family).
    AllCenter,
    /// Every column left (`cases`).
    AllLeft,
    /// Per-column from an `array` spec; columns past the spec center.
    Columns(Vec<CellAlign>),
    /// `align`-class r,l alternation.
    AlignPairs,
}

impl AlignRule {
    /// The slack fraction placed left of a cell in column `j`.
    fn factor(&self, j: usize) -> f64 {
        match self {
            Self::AllCenter => 0.5,
            Self::AllLeft => 0.0,
            Self::Columns(cols) => match cols.get(j) {
                Some(CellAlign::Left) => 0.0,
                Some(CellAlign::Right) => 1.0,
                Some(CellAlign::Center) | None => 0.5,
            },
            Self::AlignPairs => {
                if j % 2 == 0 {
                    1.0 // the left column of a pair sets right, toward the point
                } else {
                    0.0 // the right column sets left, away from it
                }
            }
        }
    }
}

/// One `array` column's alignment.
enum CellAlign {
    /// `l`.
    Left,
    /// `c`.
    Center,
    /// `r`.
    Right,
}

/// A parsed `array` column spec: alignments plus vertical-rule positions.
struct ArrayPlan {
    aligns: Vec<CellAlign>,
    vrules: Vec<usize>,
}

/// Parse an `array` column spec. The tier-1 surface covers `l`, `c`, `r`,
/// and `|`; anything else is a precise refusal naming the character.
fn parse_array_spec(spec: &str, span: Span) -> Result<ArrayPlan, MathError> {
    let mut aligns = Vec::new();
    let mut vrules = Vec::new();
    for ch in spec.chars() {
        match ch {
            'l' => aligns.push(CellAlign::Left),
            'c' => aligns.push(CellAlign::Center),
            'r' => aligns.push(CellAlign::Right),
            '|' => vrules.push(aligns.len()),
            c if c.is_whitespace() => {}
            other => {
                return Err(MathError::Malformed {
                    what: format!(
                        "unsupported array column-spec character {other:?} \
                         (the tier-1 surface covers l, c, r, |)"
                    ),
                    at: span.start,
                });
            }
        }
    }
    Ok(ArrayPlan { aligns, vrules })
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
