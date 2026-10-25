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

use std::sync::{PoisonError, RwLock};

use crate::atom::{AtomClass, classify_list, spacing_in_style};
use crate::error::MathError;
use crate::faces::{
    FACE_BOLD, FACE_ITALIC, FACE_MATH_EXTENSION, FACE_MATH_ITALIC, FACE_MATH_ITALIC_5,
    FACE_MATH_ITALIC_7, FACE_MATH_SYMBOLS, FACE_REGULAR, FACE_SANS, FACE_SYMBOLS, FACE_TYPEWRITER,
    FaceSet, GlyphMetrics, accent_spacing_fallback, alphabet_map, default_math_chain,
    glyph_metrics, kern_em,
};
use crate::mbox::{
    BoxKind, Color, FaceId, GlueSpec, Layout, MBox, MNode, PathContour, PathSeg, Positioned,
};
use crate::metrics::MathConstants;
use crate::node::{
    AccentKind, Delim, DelimSize, FracSpec, FragmentKind, Length, LengthUnit, Limits, LineAlign,
    MathFont, Node, NodeKind, PhantomKind, SpaceKind, Span, StackKind, TextStyle,
};
use crate::style::{Style, StyleCtx};

/// The layout engine: a face roster plus the calibrated math constants.
pub struct Engine {
    faces: FaceSet,
    consts: MathConstants,
    /// Memoized [`faces::glyph_metrics`](crate::faces::glyph_metrics) per
    /// `(face, gid)`: one lazily-grown slot vector per face. The roster is
    /// immutable after construction and `glyph_metrics` reads only font
    /// tables, so the value at a slot is a constant — a warm memo returns
    /// bit-identical metrics to a cold computation (pinned by the
    /// warm-engine determinism test). Math documents resolve to few
    /// distinct glyphs repeated across many spans, so the memo turns each
    /// repeat from a `hmtx` + `glyf` re-decode into one Vec read. An
    /// `RwLock`, not a `RefCell`, so one `Engine` stays `Send + Sync` and
    /// can be shared by threads laying out in parallel.
    glyph_memo: RwLock<Vec<Vec<Option<GlyphMetrics>>>>,
}

impl Engine {
    /// Build an engine over a face roster with the CM constants.
    #[must_use]
    pub fn new(faces: FaceSet) -> Self {
        Self {
            faces,
            consts: crate::metrics::CM,
            glyph_memo: RwLock::new(Vec::new()),
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

    /// The Appendix-G parameters TeX reads in `ctx`'s style (franken_manim
    /// fm-giir).
    ///
    /// At the default declared size they come from the fonts TeX loads at
    /// that style's size ([`MathConstants::at`]). A size declaration such as
    /// `\large` loads other optical sizes (cmsy8, cmsy6) that fmd-math does
    /// not carry, so there the text-size fonts are scaled, as before.
    fn consts_at(&self, ctx: &LayCtx) -> MathConstants {
        if (ctx.decl_size - 1.0).abs() < f64::EPSILON {
            self.consts.at(ctx.style.style)
        } else {
            self.consts.at(Style::Text)
        }
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
        self.finish(&root, StyleCtx::new(style), true)
    }

    /// Typeset a TexText-contract string (text mainland + math islands).
    ///
    /// # Errors
    ///
    /// As [`Engine::typeset`].
    pub fn typeset_text(&self, source: &str) -> Result<Layout, MathError> {
        let root = crate::parse_text(source)?;
        self.finish(&root, StyleCtx::new(Style::Text), false)
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
        self.finish(&root, StyleCtx::new(style), true)
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
        self.finish(&root, StyleCtx::new(Style::Text), false)
    }

    /// Lay out a parsed root. `align_rows`: the root is the Tex surface,
    /// whose `\\` lines are align* rows; false for TexText paragraphs.
    fn finish(&self, root: &Node, ctx: StyleCtx, align_rows: bool) -> Result<Layout, MathError> {
        let items = match &root.kind {
            NodeKind::List(items) => items.as_slice(),
            _ => std::slice::from_ref(root),
        };
        let ctx = LayCtx {
            style: ctx,
            alphabet: None,
            text_mode: false,
            decl_size: 1.0,
            line_stretch: 1.0,
            align_rows,
            color: None,
        };
        let rows = items
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Linebreak | NodeKind::AlignTab));
        let boxx = if align_rows && rows {
            self.align_star_rows(items, ctx, root.span)?
        } else {
            self.hlist(items, ctx)?
        };
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
    /// The size-declaration factor (`\small` …), group-scoped: multiplies
    /// the style's own factor, exactly like LaTeX's current size setting
    /// the design size every fontdimen is read from.
    decl_size: f64,
    /// \baselinestretch for subsequent stacked lines (\doublespacing).
    line_stretch: f64,
    /// Whether stacked `\\` lines are the Tex surface's align* rows, which
    /// amsmath opens up by \jot. False for TexText paragraphs.
    align_rows: bool,
    /// The `\color` in force, group-scoped like the size declaration;
    /// `None` leaves the consumer's own colour.
    color: Option<Color>,
}

impl LayCtx {
    fn size(&self) -> f64 {
        self.style.size_factor() * self.decl_size
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
    /// adjacent same-face character glyphs. Declarations (style switches,
    /// size changes) persist across the line split: `\\` does not end the
    /// enclosing group.
    fn hlist(&self, items: &[Node], ctx: LayCtx) -> Result<MBox, MathError> {
        let mut lines: Vec<(MBox, f64, f64)> = Vec::new();
        let mut ctx = ctx;
        let mut sources: Vec<&[Node]> = items
            .split(|n| matches!(n.kind, NodeKind::Linebreak))
            .collect();
        // A trailing `\\` ends the Tex surface's last align* row without
        // opening an empty one (\crcr), as the environments' rows do. A
        // TexText paragraph's trailing \\ does set an empty line in LaTeX.
        let align_rows = ctx.align_rows && !ctx.text_mode;
        if align_rows && sources.len() > 1 && sources.last().is_some_and(|l| l.is_empty()) {
            sources.pop();
        }
        for line in sources {
            let (boxx, after) = self.line(line, ctx)?;
            ctx = after;
            lines.push((boxx, ctx.size(), ctx.line_stretch));
        }
        if lines.len() == 1 {
            return lines
                .pop()
                .map(|(boxx, _, _)| boxx)
                .ok_or(MathError::Malformed {
                    what: "internal: empty line vector".to_owned(),
                    at: 0,
                });
        }
        // Stack lines: baseline-to-baseline is \baselineskip (grown when
        // boxes would come closer than \lineskip), read at each line's own
        // size. The box baseline is the first line's. Mathematics lines are
        // the Tex surface's align* rows, which amsmath opens up by \jot
        // (both skips); text paragraphs keep plain line spacing.
        let jot = if ctx.align_rows && !ctx.text_mode {
            self.consts.jot
        } else {
            0.0
        };
        let mut children = Vec::new();
        let mut baseline = 0.0_f64;
        let mut prev_depth = 0.0_f64;
        let mut width = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut height = 0.0_f64;
        for (i, (line, size, stretch)) in lines.into_iter().enumerate() {
            if i == 0 {
                height = line.height;
            } else {
                // setspace's \baselinestretch scales the natural skip; the
                // \lineskip floor still applies unstretched.
                let natural = (self.consts.baseline_skip * stretch + jot) * size;
                let min_gap = prev_depth + line.height + (self.consts.line_skip + jot) * size;
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

    /// One line of a horizontal list; also returns the context after the
    /// line's declarations, so `\\`-separated lines inherit them.
    fn line(&self, items: &[Node], outer: LayCtx) -> Result<(MBox, LayCtx), MathError> {
        let classes = classify_list(items);
        let mut ctx = outer;
        let mut laid_items: Vec<LineItem> = Vec::new();
        for (index, (node, class)) in items.iter().zip(classes).enumerate() {
            match &node.kind {
                NodeKind::StyleChange(style) => {
                    ctx = ctx.map(|s| StyleCtx {
                        style: *style,
                        cramped: s.cramped,
                    });
                }
                NodeKind::SizeChange(factor) => {
                    ctx.decl_size = *factor;
                }
                NodeKind::LineSpacing(stretch) => {
                    ctx.line_stretch = *stretch;
                }
                // `\color` colours the rest of the enclosing group; groups
                // laid below inherit it through the context. A colour xcolor
                // does not define is the declaration's own precise error.
                NodeKind::ColorChange(spec) => match &spec.color {
                    Ok(color) => ctx.color = Some(*color),
                    Err(error) => return Err(error.clone()),
                },
                // `\centering` and friends produce no glyphs; a line's
                // alignment is applied where lines are stacked
                // ([`NodeKind::AlignBlock`]), so mid-list the declaration
                // is inert.
                NodeKind::AlignChange(_)
                | NodeKind::AlignTab
                | NodeKind::Fragment(
                    FragmentKind::UnmatchedClose | FragmentKind::RedundantMathShift,
                ) => {}
                NodeKind::Space(kind) => {
                    let eighteenths = f64::from(kind.mu()) / 18.0;
                    let em = if ctx.text_mode {
                        // Text spaces are ems of the text face at its size.
                        eighteenths * ctx.size()
                    } else if kind.is_mu_glue() {
                        // 18 mu is family 2's quad at the current size (§716).
                        eighteenths * self.consts_at(&ctx).quad * ctx.size()
                    } else if matches!(kind, SpaceKind::ControlSpace) {
                        // `\ ` is the text font's interword space.
                        crate::tfm::CMR10.space * ctx.decl_size
                    } else {
                        // \quad and \qquad are \hskip ems of the text font,
                        // cmr10 at the declared size, in every math style.
                        eighteenths * crate::tfm::CMR10.param(6) * ctx.decl_size
                    };
                    laid_items.push(LineItem::Glue(GlueSpec {
                        natural: em,
                        stretch: 0.0,
                        shrink: 0.0,
                    }));
                }
                NodeKind::Skip(length) => {
                    laid_items.push(LineItem::Glue(GlueSpec {
                        natural: skip_width(*length, ctx),
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
                    if let Some(kern) = self.multi_integral_kern(node, ctx) {
                        laid_items.push(LineItem::Glue(GlueSpec {
                            natural: -kern,
                            stretch: 0.0,
                            shrink: 0.0,
                        }));
                    }
                    let mut laid = lone_op_box(node, self.lay_node(node, ctx)?);
                    if let Some(color) = ctx.color {
                        laid.boxx = colored_box(color, laid.boxx);
                    }
                    laid_items.push(LineItem::Atom {
                        laid,
                        class: class.unwrap_or(AtomClass::Ord),
                    });
                    if let Some(correction) =
                        slanted_text_correction(node, items.get(index + 1), ctx)
                    {
                        laid_items.push(LineItem::Glue(GlueSpec {
                            natural: correction,
                            stretch: 0.0,
                            shrink: 0.0,
                        }));
                    }
                }
            }
        }
        // The face of each item's successor when that successor is a single
        // character, for rule 17's math_text_char exception below.
        let next_char_face: Vec<Option<FaceId>> = (0..laid_items.len())
            .map(|i| match laid_items.get(i + 1) {
                Some(LineItem::Atom { laid, .. }) => laid.char_glyph.map(|(face, _)| face),
                _ => None,
            })
            .collect();
        // Assemble: inter-atom glue + kerning.
        let mut children = Vec::new();
        let mut x = 0.0_f64;
        let mut height = 0.0_f64;
        let mut depth = 0.0_f64;
        let mut prev: Option<(AtomClass, Option<(FaceId, u16)>)> = None;
        for (index, item) in laid_items.into_iter().enumerate() {
            match item {
                LineItem::Glue(glue) => {
                    x += glue.natural;
                    children.push(Positioned {
                        dx: x,
                        dy: 0.0,
                        node: MNode::Glue(glue),
                    });
                    // Inter-atom spacing looks past glue and kerns to the
                    // previous atom (tex.web §761 keeps r_type across them),
                    // so `a\,=b` gets the thin space and the Ord–Rel thick
                    // space. A font kern needs adjacent characters.
                    prev = prev.map(|(class, _)| (class, None));
                }
                LineItem::Atom { laid, class } => {
                    if let Some((prev_class, prev_glyph)) = prev {
                        let mu = spacing_in_style(prev_class, class, ctx.style.style).mu();
                        x += f64::from(mu) / 18.0 * self.consts_at(&ctx).quad * ctx.size();
                        // make_ord (tex.web §752): an Ord character kerns
                        // with the next atom's character from its font,
                        // whatever that atom's class (`V,` `d f` `f/`).
                        if prev_class == AtomClass::Ord && class != AtomClass::Inner {
                            if let (Some((pf, pg)), Some((cf, cg))) = (prev_glyph, laid.char_glyph)
                            {
                                if pf == cf {
                                    x += self.pair_kern(pf, pg, cg, ctx.text_mode) * ctx.size();
                                }
                            }
                        }
                    }
                    height = height.max(laid.boxx.height);
                    depth = depth.max(laid.boxx.depth);
                    // Rule 17 (tex.web §755): a character with no subscript
                    // is followed by a kern of its italic correction, so
                    // `f(` clears the f's overhang. A scripted character
                    // reaches here as a script box, which places its own δ
                    // (rule 18); text mode never kerns. The one exception is
                    // TeX's math_text_char: an Ord followed by another
                    // character of its family, in a font with nonzero
                    // interword space. cmmi, cmsy and cmex (space 0) always
                    // keep δ; cmr and the text faces drop it inside such a
                    // word (`\mathrm{ff}`, `\mathrm{f}(`).
                    let italic_kern = match laid.char_glyph {
                        Some((face, _)) if !ctx.text_mode => {
                            let text_char = class == AtomClass::Ord
                                && self.spaced_face(face)
                                && next_char_face[index] == Some(face);
                            if text_char { 0.0 } else { laid.italic }
                        }
                        _ => 0.0,
                    };
                    let advance = laid.boxx.width + italic_kern;
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
        Ok((
            MBox {
                kind: BoxKind::Horizontal,
                width: x,
                height,
                depth,
                children,
            },
            ctx,
        ))
    }

    /// A node set as its own formula, as TeX sets array cells, phantoms,
    /// arrow labels, a radical's degree and the bodies of `\overbrace`-style
    /// alignments (each an `\hbox{$…$}` or a `$…$` template): a lone
    /// character keeps its rule-17 italic correction there.
    fn formula_box(&self, node: &Node, ctx: LayCtx) -> Result<MBox, MathError> {
        Ok(self.clean_box(node, ctx)?.boxx)
    }

    /// TeX's clean_box (tex.web §720): a fraction part, script, limit,
    /// radicand, over- or underlined body or accent base laid as its own
    /// list. A lone character's box keeps its italic correction in its
    /// width: §721 frees the kern node but not the width hpack gave the box,
    /// so `\frac{f}{2}`'s numerator and `\tilde f`'s base are f plus δ wide
    /// (real TeX's `\showbox`). The character and its δ stay in the `Laid`
    /// for an accent's skew.
    fn clean_box(&self, node: &Node, ctx: LayCtx) -> Result<Laid, MathError> {
        let mut laid = lone_op_box(node, self.lay_node(node, ctx)?);
        if laid.char_glyph.is_some() && !ctx.text_mode {
            laid.boxx.width += laid.italic;
        }
        Ok(laid)
    }

    /// Lay one node into an atom box.
    #[allow(clippy::too_many_lines)]
    fn lay_node(&self, node: &Node, ctx: LayCtx) -> Result<Laid, MathError> {
        match &node.kind {
            // A group holding one scriptless Ord character is that character
            // (tex.web §1186): `{f}(` kerns like `f(`, `{x}^2` scripts like
            // `x^2`, and `\mathrm{f}(` is a word of the text face.
            NodeKind::List(items)
                if matches!(
                    items.as_slice(),
                    [Node {
                        kind: NodeKind::Symbol {
                            class: AtomClass::Ord,
                            ..
                        },
                        ..
                    }]
                ) =>
            {
                self.lay_node(&items[0], ctx)
            }
            NodeKind::List(items) => Ok(Laid {
                boxx: self.hlist(items, ctx)?,
                italic: 0.0,
                char_glyph: None,
            }),
            // Plain TeX's `\ldots` is `\mathinner{\ldotp\ldotp\ldotp}` and
            // `\cdots` is `\mathinner{\cdotp\cdotp\cdotp}`: three punctuation
            // dots, thin-spaced outside script styles, with the Inner atom's
            // own spacing outside. One wide ellipsis glyph is 27-34%
            // narrower and is one glyph where LaTeX sets three.
            NodeKind::Symbol {
                ch: ch @ ('…' | '⋯'),
                class: AtomClass::Inner,
            } => {
                let dot = if *ch == '…' { '.' } else { '⋅' };
                let dots = [0; 3].map(|_| {
                    Node::new(
                        NodeKind::Symbol {
                            ch: dot,
                            class: AtomClass::Punct,
                        },
                        node.span,
                    )
                });
                Ok(Laid {
                    boxx: self.hlist(&dots, ctx)?,
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            // LaTeX's `\vdots` is `\vbox{\baselineskip4\p@ \lineskiplimit\z@
            // \kern6\p@\hbox{.}\hbox{.}\hbox{.}}`: three text periods on 4 pt
            // baselines, the lowest on the baseline, under a 6 pt kern
            // (1.51 em tall in all). Points and \hbox do not scale with the
            // math style. The single ⋮ glyph is half that tall, its dots
            // closer.
            NodeKind::Symbol { ch: '⋮', .. } => {
                let dot = self.text_period(node.span, ctx)?;
                Ok(Laid {
                    boxx: MBox {
                        kind: BoxKind::Horizontal,
                        width: dot.width,
                        height: 0.6 + 0.8 + dot.height,
                        depth: dot.depth,
                        // Top to bottom, the vbox's emission order.
                        children: [0.8, 0.4, 0.0]
                            .map(|dy| Positioned {
                                dx: 0.0,
                                dy,
                                node: MNode::Box(dot.clone()),
                            })
                            .into(),
                    },
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            // LaTeX's `\ddots` is `\mathinner{\mkern1mu\raise7\p@\vbox{\kern7\p@
            // \hbox{.}}\mkern2mu\raise4\p@\hbox{.}\mkern2mu\raise\p@\hbox{.}
            // \mkern1mu}`: text periods raised 7, 4 and 1 pt, the highest
            // under a 7 pt kern, so the box is as tall as `\vdots`.
            NodeKind::Symbol { ch: '⋱', .. } => {
                let dot = self.text_period(node.span, ctx)?;
                let mu = self.consts_at(&ctx).quad * ctx.size() / 18.0;
                let children: Vec<Positioned<MNode>> = [(0.0, 0.7), (1.0, 0.4), (2.0, 0.1)]
                    .map(|(i, dy)| Positioned {
                        dx: mu + i * (dot.width + 2.0 * mu),
                        dy,
                        node: MNode::Box(dot.clone()),
                    })
                    .into();
                Ok(Laid {
                    boxx: MBox {
                        kind: BoxKind::Horizontal,
                        width: 3.0 * dot.width + 6.0 * mu,
                        height: 0.7 + 0.7 + dot.height,
                        depth: (dot.depth - 0.1).max(0.0),
                        children,
                    },
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            NodeKind::Symbol { ch: '′', .. } => Ok(Laid {
                boxx: self.tex_prime(node.span, ctx)?,
                italic: 0.0,
                char_glyph: None,
            }),
            NodeKind::Symbol { ch, .. } => self.char_atom(*ch, node.span, ctx),
            NodeKind::AmsSymbol {
                ch, family, code, ..
            } => self.ams_atom(*ch, *family, *code, node.span, ctx),
            NodeKind::BigOp { ch, .. } if multi_integral_count(*ch).is_some() => {
                let count = multi_integral_count(*ch).unwrap_or(2);
                self.multi_integral(count, self.limits_active(node, ctx), node.span, ctx)
            }
            NodeKind::BigOp { ch, integral, .. } => {
                let scale = match (ctx.style.style == Style::Display, *integral) {
                    (true, true) => self.consts.display_integral_scale,
                    (true, false) => self.consts.display_op_scale,
                    (false, _) => 1.0,
                };
                self.op_glyph(*ch, node.span, ctx, scale)
            }
            NodeKind::OpName { name, .. } => {
                self.word_box(name, node.span, None, ctx, FACE_REGULAR, false)
            }
            NodeKind::TextRun { text, char_spans } => {
                self.word_box(text, node.span, Some(char_spans), ctx, text_face(ctx), true)
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
                        TextStyle::Emph | TextStyle::Italic => MathFont::Italic,
                        TextStyle::Underline | TextStyle::Roman => MathFont::Roman,
                        TextStyle::Typewriter => MathFont::Typewriter,
                        TextStyle::SansSerif => MathFont::SansSerif,
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
                // amsmath's `\text` property: an inner math island resumes
                // the SURROUNDING mathematics — script-size text (a
                // `\substack` line, a script position) gets script-size
                // islands. The nominal island style (`$…$` text,
                // `$$…$$` display) applies only when the surroundings are
                // not already smaller: an island never enlarges its text.
                let nominal = if *display {
                    Style::Display
                } else {
                    Style::Text
                };
                let style = if ctx.style.style.size_factor() < nominal.size_factor() {
                    ctx.style.style
                } else {
                    nominal
                };
                Ok(Laid {
                    boxx: self.hlist(
                        body,
                        LayCtx {
                            style: StyleCtx {
                                style,
                                cramped: ctx.style.cramped,
                            },
                            alphabet: None,
                            text_mode: false,
                            ..ctx
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
                let inner = self.formula_box(body, ctx)?;
                let (w, h, d) = match kind {
                    PhantomKind::Full => (inner.width, inner.height, inner.depth),
                    PhantomKind::Horizontal => (inner.width, 0.0, 0.0),
                    PhantomKind::Vertical => (0.0, inner.height, inner.depth),
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
            // In its own function so this recursive frame does not grow
            // (the nesting bound's stack margin).
            NodeKind::Boxed { body } => self.boxed(body, node.span, ctx),
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
                let ann = self.clean_box(annotation, ann_ctx)?;
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
            NodeKind::XArrow {
                mapsto,
                above,
                below,
            } => {
                // amsmath's labeled extensible arrows: the drawn band
                // stretches to its widest script-style label plus padding
                // (a bare label still gets a healthy minimum arrow), sits
                // on the math axis, and takes the labels as limits.
                let size = ctx.size();
                // amsmath pads the labels with \mkern, so clean_box keeps a
                // lone character's italic correction: they are formula boxes.
                let above_box = self.formula_box(above, ctx.map(StyleCtx::sup))?;
                let below_box = below
                    .as_deref()
                    .map(|b| self.formula_box(b, ctx.map(StyleCtx::sub)))
                    .transpose()?;
                let label_w = above_box
                    .width
                    .max(below_box.as_ref().map_or(0.0, |b| b.width));
                let width = (label_w + 0.8 * size).max(1.2 * size);
                let stretch_kind = if *mapsto {
                    crate::drawn::Stretch::MapstoArrow
                } else {
                    crate::drawn::Stretch::RightArrow
                };
                let band = crate::drawn::stretch(stretch_kind, width, size);
                let axis = self.consts_at(&ctx).axis_height * size;
                let band_dy = axis - band.height / 2.0;
                let band_box = MBox {
                    kind: BoxKind::Horizontal,
                    width,
                    height: axis + band.height / 2.0,
                    depth: (band.height / 2.0 - axis).max(0.0),
                    children: vec![Positioned {
                        dx: 0.0,
                        dy: band_dy,
                        node: MNode::Path {
                            contours: band.contours,
                            span: node.span,
                        },
                    }],
                };
                Ok(Laid {
                    boxx: self.with_limits(band_box, Some(above_box), below_box, ctx, 0.0),
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
            NodeKind::AlignBlock { align, lines } => {
                // `flushleft` / `center` / `flushright`: a single-column
                // grid whose one column takes the block's line alignment.
                // The grid engine supplies the shared \baselineskip /
                // \lineskip stacking and the \vcenter axis centering the
                // matrix family uses; only the column rule differs.
                let cell_align = match align {
                    LineAlign::Left => CellAlign::Left,
                    LineAlign::Center => CellAlign::Center,
                    LineAlign::Right => CellAlign::Right,
                };
                // Each line is one single-cell row (cells are `List` nodes,
                // the same shape the environments feed the grid).
                let rows: Vec<Vec<Node>> = lines.iter().map(|line| vec![line.clone()]).collect();
                let boxx = self.grid(
                    &rows,
                    ctx,
                    &Grid {
                        align: AlignRule::Columns(vec![cell_align]),
                        ..Grid::centered(1.0)
                    },
                    node.span,
                )?;
                Ok(Laid {
                    boxx,
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            // A declaration outside a list colours nothing, but a colour
            // xcolor does not define is still refused, never dropped.
            NodeKind::ColorChange(crate::node::ColorSpec {
                color: Err(error), ..
            }) => Err(error.clone()),
            NodeKind::Fragment(_)
            | NodeKind::StyleChange(_)
            | NodeKind::SizeChange(_)
            | NodeKind::ColorChange(_)
            | NodeKind::AlignChange(_)
            | NodeKind::LineSpacing(_)
            | NodeKind::Space(_)
            | NodeKind::Skip(_)
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

    /// LaTeX's `\hbox{.}`: a period of the text face at the size in force,
    /// which the math style does not scale.
    fn text_period(&self, span: Span, ctx: LayCtx) -> Result<MBox, MathError> {
        let text = LayCtx {
            style: StyleCtx::new(Style::Text),
            alphabet: None,
            text_mode: true,
            ..ctx
        };
        Ok(self.char_atom('.', span, text)?.boxx)
    }

    /// A single-character atom.
    fn char_atom(&self, ch: char, span: Span, ctx: LayCtx) -> Result<Laid, MathError> {
        if ch == crate::commands::NOT_SLASH && !ctx.text_mode {
            if let Some(laid) = self.not_slash(span, ctx) {
                return Ok(laid);
            }
        }
        if ctx.alphabet.is_none() && !ctx.text_mode {
            if let Some(laid) = self.negated_relation(ch, span, ctx)? {
                return Ok(laid);
            }
            if let Some(laid) = self.vereq_relation(ch, span, ctx)? {
                return Ok(laid);
            }
        }
        let (face, gid, mapped) = match self.resolve_math_char(ch, span, ctx) {
            Ok(hit) => hit,
            Err(err) => {
                // The drawn-symbol mainline (fm-j5t.4, the
                // Checkmark/Exmark drawn-mark precedent): characters no
                // bundled face maps but the engine has a calibrated
                // construction for — the wasysym astronomy/biology
                // symbols ♀ ♂ ♁ — are DRAWN as paths in the same unit
                // box, never silently substituted with a different
                // glyph. Miss-only, exactly like the documented glyph
                // alternates: the moment a face gains the exact
                // codepoint, the authored glyph wins. Everything else
                // keeps the precise UnmappedChar refusal.
                if let Some(d) = crate::drawn::symbol(ch, ctx.size()) {
                    return Ok(Laid {
                        boxx: MBox {
                            kind: BoxKind::Horizontal,
                            width: d.width,
                            height: d.height,
                            depth: d.depth,
                            children: vec![Positioned {
                                dx: 0.0,
                                dy: 0.0,
                                node: MNode::Path {
                                    contours: d.contours,
                                    span,
                                },
                            }],
                        },
                        italic: 0.0,
                        char_glyph: None,
                    });
                }
                return Err(err);
            }
        };
        // TeX's own font: the optically designed 7- and 5-point cuts in
        // scripts, at the same scaled size, and cmr10 for family 0.
        let (face, gid) = self
            .faces
            .tex_face(face, mapped, ctx.style.style, ctx.text_mode)
            .unwrap_or((face, gid));
        Ok(self.glyph_atom(face, gid, mapped, span, ctx))
    }

    /// A named AMS symbol selects its authored code, independently of an
    /// enclosing alphabet. A custom roster without AMS retains semantic
    /// Unicode fallback; a present but incomplete AMS face is a precise error.
    fn ams_atom(
        &self,
        ch: char,
        family: crate::node::AmsFamily,
        code: u8,
        span: Span,
        ctx: LayCtx,
    ) -> Result<Laid, MathError> {
        let base = match family {
            crate::node::AmsFamily::A => crate::faces::FACE_AMS_A_10,
            crate::node::AmsFamily::B => crate::faces::FACE_AMS_B_10,
        };
        if self.faces.font(base).is_none() {
            return self.char_atom(ch, span, ctx);
        }
        let gid = self
            .faces
            .tex_glyph(base, code)
            .ok_or(MathError::UnmappedChar { ch, span })?;
        let (face, gid) = self
            .faces
            .optical(base, crate::faces::tex_code(code), ctx.style.style)
            .unwrap_or((base, gid));
        Ok(self.glyph_atom(
            face,
            gid,
            ch,
            span,
            LayCtx {
                text_mode: false,
                alphabet: None,
                ..ctx
            },
        ))
    }

    fn glyph_atom(&self, face: FaceId, gid: u16, ch: char, span: Span, ctx: LayCtx) -> Laid {
        let metrics = self.char_metrics(face, gid, ctx.text_mode);
        let size = ctx.size();
        Laid {
            boxx: glyph_box(face, gid, ch, span, size, metrics),
            italic: metrics.italic * size,
            char_glyph: Some((face, gid)),
        }
    }

    /// amsmath's `\boxed`, `\fbox{$\displaystyle#1$}`: a fresh formula in
    /// display style at the declared size, even in a script. \fboxsep (3 pt)
    /// and \fboxrule (0.4 pt) are lengths, which neither the math style nor
    /// a size declaration scales.
    fn boxed(&self, body: &Node, span: Span, ctx: LayCtx) -> Result<Laid, MathError> {
        const SEP: f64 = 0.3;
        const RULE: f64 = 0.04;
        let inner = self
            .clean_box(
                body,
                LayCtx {
                    style: StyleCtx::new(Style::Display),
                    alphabet: None,
                    text_mode: false,
                    align_rows: false,
                    ..ctx
                },
            )?
            .boxx;
        let (height, depth) = (inner.height, inner.depth);
        let width = inner.width + 2.0 * (SEP + RULE);
        let side = height + depth + 2.0 * SEP;
        let rule = |width: f64, height: f64| MNode::Rule {
            width,
            height,
            span,
        };
        // \fbox's emission order: the top rule, the left rule, the body, the
        // right rule and the bottom rule; the top and bottom rules span the
        // corners.
        let children = vec![
            Positioned {
                dx: 0.0,
                dy: height + SEP,
                node: rule(width, RULE),
            },
            Positioned {
                dx: 0.0,
                dy: -(depth + SEP),
                node: rule(RULE, side),
            },
            Positioned {
                dx: RULE + SEP,
                dy: 0.0,
                node: MNode::Box(inner),
            },
            Positioned {
                dx: width - RULE,
                dy: -(depth + SEP),
                node: rule(RULE, side),
            },
            Positioned {
                dx: 0.0,
                dy: -(depth + SEP + RULE),
                node: rule(width, RULE),
            },
        ];
        Ok(Laid {
            boxx: MBox {
                kind: BoxKind::Horizontal,
                width,
                height: height + SEP + RULE,
                depth: depth + SEP + RULE,
                children,
            },
            italic: 0.0,
            char_glyph: None,
        })
    }

    /// `\not` as its own atom: plain TeX's `\mathchar"3236`, cmsy's negation
    /// slash (code 0x36), a relation of zero width. A relation after it
    /// follows with no space between them (Rel–Rel), so the slash overprints
    /// it from its origin, as `\not\subset` and `\not<` set in TeX. `None`
    /// when the roster has no CM math symbols face.
    fn not_slash(&self, span: Span, ctx: LayCtx) -> Option<Laid> {
        let gid = self.faces.tex_glyph(FACE_MATH_SYMBOLS, 0x36)?;
        let (face, gid) = self
            .faces
            .optical(
                FACE_MATH_SYMBOLS,
                crate::faces::tex_code(0x36),
                ctx.style.style,
            )
            .unwrap_or((FACE_MATH_SYMBOLS, gid));
        let metrics = self.char_metrics(face, gid, ctx.text_mode);
        Some(Laid {
            boxx: glyph_box(
                face,
                gid,
                crate::commands::NOT_SLASH,
                span,
                ctx.size(),
                metrics,
            ),
            italic: 0.0,
            char_glyph: Some((face, gid)),
        })
    }

    /// Plain TeX's `\not`: cmsy's zero-width negation slash (code 0x36)
    /// shipped before the relation it overprints, so the box is the
    /// relation's. `\neq` is `\not=` and `\not\equiv` likewise sets the
    /// slash at the relation's origin; `\notin` centres it on the `∈`, as
    /// LaTeX's `\c@ncel` does. `None` for other characters, or when the
    /// roster has no CM math symbols face (the precomposed glyph then
    /// resolves through the fallback faces as before).
    fn negated_relation(
        &self,
        ch: char,
        span: Span,
        ctx: LayCtx,
    ) -> Result<Option<Laid>, MathError> {
        let (base, centred) = match ch {
            '≠' => ('=', false),
            '≢' => ('≡', false),
            '∉' => ('∈', true),
            _ => return Ok(None),
        };
        let Some(gid) = self.faces.tex_glyph(FACE_MATH_SYMBOLS, 0x36) else {
            return Ok(None);
        };
        let (face, gid) = self
            .faces
            .optical(
                FACE_MATH_SYMBOLS,
                crate::faces::tex_code(0x36),
                ctx.style.style,
            )
            .unwrap_or((FACE_MATH_SYMBOLS, gid));
        let relation = self.char_atom(base, span, ctx)?;
        let size = ctx.size();
        let slash = self.char_metrics(face, gid, ctx.text_mode);
        let dx = match (
            centred,
            self.faces.font(face).and_then(|f| f.glyph_bbox(gid)),
        ) {
            (true, Some(bbox)) => {
                let upm = self
                    .faces
                    .font(face)
                    .map_or(1000.0, |f| f64::from(f.units_per_em.max(1)));
                relation.boxx.width / 2.0 - f64::from(bbox[0] + bbox[2]) / 2.0 / upm * size
            }
            _ => 0.0,
        };
        let mut boxx = relation.boxx;
        boxx.height = boxx.height.max(slash.height * size);
        boxx.depth = boxx.depth.max(slash.depth * size);
        let inner = std::mem::take(&mut boxx.children);
        boxx.children = vec![Positioned {
            dx,
            dy: 0.0,
            node: MNode::Glyph {
                face,
                gid,
                ch,
                size,
                span,
            },
        }];
        boxx.children.extend(inner);
        Ok(Some(Laid {
            boxx,
            italic: relation.italic,
            char_glyph: None,
        }))
    }

    /// LaTeX's `\cong`, `\mathrel{\mathpalette\@vereq\sim}`, where
    /// `\@vereq#1#2` is `\lower.5\p@\vbox{\lineskiplimit\maxdimen
    /// \lineskip-.5\p@\ialign{$\m@th#1\hfil##\hfil$\crcr#2\crcr=\crcr}}`:
    /// cmsy's `∼` over cmr's `=` in the current style, each centred in the
    /// wider one's width. The rows are hboxes, so their depths floor at
    /// zero; they sit 0.5 pt closer than touching, and the stack is lowered
    /// 0.5 pt. Points do not scale with the math style. Two glyphs, as the
    /// Reference's SVG has them. `None` for every other character.
    fn vereq_relation(&self, ch: char, span: Span, ctx: LayCtx) -> Result<Option<Laid>, MathError> {
        if ch != '≅' {
            return Ok(None);
        }
        let top = self.char_atom('∼', span, ctx)?.boxx;
        let bottom = self.char_atom('=', span, ctx)?.boxx;
        let width = top.width.max(bottom.width);
        // \lineskip-.5\p@ and \lower.5\p@, in 10 pt ems.
        let (lineskip, lower) = (-0.05, 0.05);
        let top_dy = bottom.height.max(0.0) + lineskip + top.depth.max(0.0) - lower;
        let boxx = MBox {
            kind: BoxKind::Horizontal,
            width,
            height: top.height.max(0.0) + top_dy,
            depth: bottom.depth.max(0.0) + lower,
            // Top to bottom, the vbox's emission order.
            children: vec![
                Positioned {
                    dx: (width - top.width) / 2.0,
                    dy: top_dy,
                    node: MNode::Box(top),
                },
                Positioned {
                    dx: (width - bottom.width) / 2.0,
                    dy: -lower,
                    node: MNode::Box(bottom),
                },
            ],
        };
        Ok(Some(Laid {
            boxx,
            italic: 0.0,
            char_glyph: None,
        }))
    }

    /// A big-operator glyph, axis-centered. Where cmex10 has the operator the
    /// glyph is cmex's own text or display character, at its designed size,
    /// and the box is TeX's: the TFM width and italic correction of that
    /// variant ([`tex_op_frame`]). An operator cmex lacks resolves through
    /// the fallback faces, display-scaled by `scale`, with its ink centered
    /// on cmex's frame where one exists.
    fn op_glyph(&self, ch: char, span: Span, ctx: LayCtx, scale: f64) -> Result<Laid, MathError> {
        let display = ctx.style.style == Style::Display;
        let cmex = big_op_codes(ch).and_then(|(text, shown)| {
            self.faces
                .tex_glyph(FACE_MATH_EXTENSION, if display { shown } else { text })
        });
        let (face, gid, mapped, scale) = match cmex {
            Some(gid) => (FACE_MATH_EXTENSION, gid, ch, 1.0),
            None => {
                let (face, gid, mapped) = self.resolve_math_char(ch, span, ctx)?;
                (face, gid, mapped, scale)
            }
        };
        let metrics = self.char_metrics(face, gid, ctx.text_mode);
        let size = ctx.size() * scale;
        let gh = metrics.height * size;
        let gd = metrics.depth * size;
        let axis = self.consts_at(&ctx).axis_height * ctx.size();
        // Center the ink on the math axis.
        let dy = axis - (gh - gd) / 2.0;
        let ink = self.faces.font(face).and_then(|font| {
            let upm = f64::from(font.units_per_em.max(1));
            font.glyph_bbox(gid).map(|bbox| {
                (
                    f64::from(bbox[0]) / upm * size,
                    f64::from(bbox[2]) / upm * size,
                )
            })
        });
        let em = ctx.size();
        let (width, italic, dx) = match (tex_op_frame(ch, display), ink) {
            (Some((width, italic, x0, x1)), Some((g0, g1))) => (
                width * em,
                italic * em,
                (x0 + x1) / 2.0 * em - (g0 + g1) / 2.0,
            ),
            _ => (metrics.advance * size, metrics.italic * size, 0.0),
        };
        let boxx = MBox {
            kind: BoxKind::Horizontal,
            width,
            height: gh + dy,
            depth: (gd - dy).max(0.0),
            children: vec![Positioned {
                dx,
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
            italic,
            char_glyph: None,
        })
    }

    /// amsmath's `\iint`, `\iiint` and `\iiiint` (`\MultiIntegral`,
    /// amsmath.sty): `count` of cmex's `\intop` side by side, each after the
    /// first drawn back by `\intkern@` (−6 mu, a further −3 mu in display
    /// style) past the thin space TeX sets between adjacent Op atoms. The
    /// scripts belong to the last `\intop`, so its italic correction is the
    /// atom's. With limits active amsmath makes the row one `\mathop` opened
    /// by 7 mu (9 in display style), which a kern before the atom takes back
    /// ([`Self::multi_integral_kern`]); that nucleus is a list, so the limits
    /// centre over the whole row and it has no italic correction.
    ///
    /// The row is one drawn construct of the `\intop` outlines, not a glyph
    /// per `\intop`: the Reference's preamble sets `\iint` as wasysym's one
    /// character, so a consumer that counts or indexes the formula's parts
    /// (manim's `Tex(...)[0]`) finds the integral as one part there too.
    fn multi_integral(
        &self,
        count: u8,
        limits: bool,
        span: Span,
        ctx: LayCtx,
    ) -> Result<Laid, MathError> {
        let one = self.op_glyph('∫', span, ctx, 1.0)?;
        let display = ctx.style.style == Style::Display;
        let mu = self.consts_at(&ctx).quad * ctx.size() / 18.0;
        let thin = f64::from(spacing_in_style(AtomClass::Op, AtomClass::Op, ctx.style.style).mu());
        let gap = (thin - 6.0 - if display { 3.0 } else { 0.0 }) * mu;
        let lead = if limits {
            multi_integral_lead(display) * mu
        } else {
            0.0
        };
        let lone = one.boxx.width + one.italic;
        let unmapped = MathError::UnmappedChar { ch: '∫', span };
        let Some(Positioned {
            dx: glyph_dx,
            dy: glyph_dy,
            node: MNode::Glyph {
                face, gid, size, ..
            },
        }) = one.boxx.children.first()
        else {
            return Err(unmapped);
        };
        let font = self.faces.font(*face).ok_or(unmapped.clone())?;
        let outline = font.glyph_outline(*gid).map_err(|_| unmapped)?;
        let scale = *size / f64::from(font.units_per_em.max(1));
        let mut contours = Vec::with_capacity(usize::from(count) * outline.contours.len());
        for i in 0..count {
            let x0 = lead + f64::from(i) * (lone + gap) + *glyph_dx;
            let at = |p: fmd_font::outline::Point| (x0 + p.x * scale, *glyph_dy + p.y * scale);
            contours.extend(outline.contours.iter().map(|contour| {
                PathContour {
                    start: at(contour.start),
                    segments: contour
                        .segments
                        .iter()
                        .map(|seg| match seg {
                            fmd_font::outline::Segment::Line { to } => {
                                PathSeg::Line { to: at(*to) }
                            }
                            fmd_font::outline::Segment::Quad { ctrl, to } => PathSeg::Quad {
                                ctrl: at(*ctrl),
                                to: at(*to),
                            },
                        })
                        .collect(),
                }
            }));
        }
        let children = vec![Positioned {
            dx: 0.0,
            dy: 0.0,
            node: MNode::Path { contours, span },
        }];
        let full = lead + f64::from(count) * lone + f64::from(count - 1) * gap;
        let (width, italic) = if limits {
            (full, 0.0)
        } else {
            (full - one.italic, one.italic)
        };
        Ok(Laid {
            boxx: MBox {
                kind: BoxKind::Horizontal,
                width,
                height: one.boxx.height,
                depth: one.boxx.depth,
                children,
            },
            italic,
            char_glyph: None,
        })
    }

    /// The kern amsmath's `\ints@b` sets before a multiple integral whose
    /// limits are active (−7 mu, −9 mu in display style), taking back the
    /// space that opens its `\mathop`. `None` for every other atom.
    fn multi_integral_kern(&self, node: &Node, ctx: LayCtx) -> Option<f64> {
        let op = match &node.kind {
            NodeKind::Scripts {
                base: Some(base), ..
            } => base.as_ref(),
            _ => node,
        };
        let NodeKind::BigOp { ch, .. } = op.kind else {
            return None;
        };
        multi_integral_count(ch)?;
        if !self.limits_active(op, ctx) {
            return None;
        }
        let display = ctx.style.style == Style::Display;
        Some(multi_integral_lead(display) * self.consts_at(&ctx).quad * ctx.size() / 18.0)
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
        text_run: bool,
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
            // An operator name is family 0 (cmr10); a text run keeps the
            // text face.
            let (face, gid) = self
                .faces
                .tex_face(face, mapped, ctx.style.style, ctx.text_mode || text_run)
                .unwrap_or((face, gid));
            if let Some((pf, pg)) = prev {
                if pf == face {
                    x += self.pair_kern(face, pg, gid, ctx.text_mode) * size;
                }
            }
            let m = self.char_metrics(face, gid, ctx.text_mode);
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

    #[inline(always)]
    fn metrics_of(&self, face: FaceId, gid: u16) -> GlyphMetrics {
        // Hot path: one uncontended read lock + two Vec reads. Misses and the
        // table decodes live out of line in `metrics_of_miss` so the
        // inlined body at every glyph site stays as small as the
        // unmemoized call it replaced.
        if let Some(hit) = self
            .glyph_memo
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(face.0)
            .and_then(|per_face| per_face.get(usize::from(gid)))
            .and_then(|slot| *slot)
        {
            return hit;
        }
        self.metrics_of_miss(face, gid)
    }

    fn metrics_of_miss(&self, face: FaceId, gid: u16) -> GlyphMetrics {
        let Some(font) = self.faces.font(face) else {
            // A face outside the roster can never be memoized under a
            // bounded index; the default is its constant result.
            return GlyphMetrics::default();
        };
        let m = glyph_metrics(font, gid);
        // Every slot holds a constant, so a poisoned memo is still valid.
        let mut memo = self
            .glyph_memo
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if memo.len() <= face.0 {
            memo.resize(face.0 + 1, Vec::new());
        }
        let per_face = &mut memo[face.0];
        if per_face.len() <= usize::from(gid) {
            per_face.resize(usize::from(gid) + 1, None);
        }
        per_face[usize::from(gid)] = Some(m);
        m
    }

    /// A glyph's box in ems of its size. In math mode a character of a face
    /// that stands for one of TeX's fonts takes that font's TFM box, its
    /// height and depth floored at zero as the character's box is
    /// (tex.web §653); anything else is measured from its outline.
    fn char_metrics(&self, face: FaceId, gid: u16, text_mode: bool) -> GlyphMetrics {
        if !text_mode
            && let Some(tex) = self
                .faces
                .tfm(face, gid)
                .and_then(|(font, code)| font.char(code))
        {
            return GlyphMetrics {
                advance: tex.width,
                height: tex.height.max(0.0),
                depth: tex.depth.max(0.0),
                italic: tex.italic,
            };
        }
        self.metrics_of(face, gid)
    }

    /// The kern between two adjacent glyphs of one face, in ems: in math
    /// mode the TFM's lig/kern program for two of its characters, else the
    /// face's own kerning.
    fn pair_kern(&self, face: FaceId, left: u16, right: u16, text_mode: bool) -> f64 {
        if !text_mode
            && let (Some((font, l)), Some((_, r))) =
                (self.faces.tfm(face, left), self.faces.tfm(face, right))
        {
            return font.kern(l, r).unwrap_or(0.0);
        }
        self.faces
            .font(face)
            .map_or(0.0, |font| kern_em(font, left, right))
    }

    /// Whether `face`'s font has a nonzero interword space, which makes an
    /// Ord character followed by another of its font TeX's math_text_char
    /// and drops its italic correction (tex.web §752, §755). cmmi, cmsy and
    /// cmex have none; cmr and the text faces do. Without TeX's metrics the
    /// CM text italic stands in for cmmi.
    fn spaced_face(&self, face: FaceId) -> bool {
        match self.faces.tfm_font(face) {
            Some(font) => font.space != 0.0,
            None => face != FACE_ITALIC || self.faces.tfm_font(FACE_MATH_ITALIC).is_some(),
        }
    }

    /// Rule 12's skew (tex.web §738, §741): over a single character, its
    /// kern with its font's skewchar; over anything else, none. A character
    /// without TeX's metrics is skewed by half its measured italic
    /// correction.
    fn accent_skew(&self, base: &Laid, ctx: LayCtx) -> f64 {
        let Some((face, gid)) = base.char_glyph else {
            return 0.0;
        };
        match self.faces.tfm(face, gid) {
            Some((font, code)) if !ctx.text_mode => font.skew(code) * ctx.size(),
            _ => base.italic / 2.0,
        }
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
                    self.metrics_of(FACE_REGULAR, gid).advance
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
                    .map(|n| self.clean_box(n, ctx.map(StyleCtx::sup)))
                    .transpose()?
                    .map(|l| l.boxx);
                let lower = sub
                    .map(|n| self.clean_box(n, ctx.map(StyleCtx::sub)))
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
        let c = &self.consts_at(&ctx);
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
        // single characters), read from the symbol font of the size the
        // scripts are set at, t (tex.web §756).
        let (u, v) = if base_is_char {
            (0.0, 0.0)
        } else {
            let t = ctx.style.style.sup();
            let (sup_drop, sub_drop) = c.script_drops(t);
            let t_size = t.size_factor() * ctx.decl_size;
            (
                base_laid.boxx.height - sup_drop * t_size,
                base_laid.boxx.depth + sub_drop * t_size,
            )
        };
        // The superscript material: an explicit box, or a prime run.
        let sup_box = if primes.is_empty() {
            match sup {
                Some(node) => Some(self.clean_box(node, ctx.map(StyleCtx::sup))?.boxx),
                None => None,
            }
        } else {
            Some(self.prime_run(primes, ctx)?)
        };
        let sub_box = match sub {
            Some(node) => Some(self.clean_box(node, ctx.map(StyleCtx::sub))?.boxx),
            None => None,
        };
        // §756: every script box is widened by \scriptspace.
        let widen = |mut b: MBox| {
            b.width += c.script_space;
            b
        };
        let (sup_box, sub_box) = (sup_box.map(widen), sub_box.map(widen));
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
    /// `'` token's span: plain TeX makes `x''` into `x^{\prime\prime}`.
    fn prime_run(&self, primes: &[Span], ctx: LayCtx) -> Result<MBox, MathError> {
        let sup_ctx = ctx.map(StyleCtx::sup);
        let mut children = Vec::new();
        let (mut x, mut height) = (0.0, 0.0_f64);
        for span in primes {
            let prime = self.tex_prime(*span, sup_ctx)?;
            let width = prime.width;
            height = height.max(prime.height);
            children.push(Positioned {
                dx: x,
                dy: 0.0,
                node: MNode::Box(prime),
            });
            x += width;
        }
        Ok(MBox {
            kind: BoxKind::Horizontal,
            width: x,
            height,
            depth: 0.0,
            children,
        })
    }

    /// Plain TeX's `\prime`, cmsy's prime character at the size in force:
    /// its box, and the bundled prime glyph fitted to its ink. The
    /// character is drawn large and sits on the baseline so that, as a
    /// superscript, it reaches down toward the x-height. The bundled glyph
    /// is a small raised tick of a squatter design, so, as accents are, it
    /// is scaled to the character's ink width, centered on its ink and hung
    /// from its ink top (scaled to the ink height it would be twice as
    /// heavy as TeX's stroke).
    fn tex_prime(&self, span: Span, ctx: LayCtx) -> Result<MBox, MathError> {
        let unmapped = MathError::UnmappedChar { ch: '′', span };
        // cmsy's own prime at the style's optical size fits the AFM ink
        // exactly; other faces' primes are fitted to it.
        let Some((face, gid)) = self
            .faces
            .resolve('′', &[FACE_MATH_SYMBOLS, FACE_REGULAR, FACE_SYMBOLS])
            .map(|(face, gid)| {
                self.faces
                    .optical(face, '′', ctx.style.style)
                    .unwrap_or((face, gid))
            })
        else {
            return Err(unmapped);
        };
        let Some(font) = self.faces.font(face) else {
            return Err(unmapped);
        };
        let upm = f64::from(font.units_per_em.max(1));
        let bbox = font.glyph_bbox(gid).unwrap_or([0, 0, 0, 0]);
        let [gl, _, gr, gt] = bbox.map(|v| f64::from(v) / upm);
        let (advance, [tl, _, tr, tt]) = tex_prime_metrics(ctx.style.style);
        let size = ctx.size();
        let glyph_size = if gr > gl {
            size * (tr - tl) / (gr - gl)
        } else {
            size
        };
        Ok(MBox {
            kind: BoxKind::Horizontal,
            width: advance * size,
            height: tt * size,
            depth: 0.0,
            children: vec![Positioned {
                dx: (tl + tr) / 2.0 * size - (gl + gr) / 2.0 * glyph_size,
                dy: tt * size - gt * glyph_size,
                node: MNode::Glyph {
                    face,
                    gid,
                    ch: '′',
                    size: glyph_size,
                    span,
                },
            }],
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
        let c = &self.consts_at(&ctx);
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
        let lower = lower.map(|low| {
            let gap = (c.big_op_spacing2 * size).max(c.big_op_spacing4 * size - low.height);
            let dy = -(op.depth + gap + low.height);
            depth = -dy + low.depth + c.big_op_spacing5 * size;
            Positioned {
                dx: (width - low.width) / 2.0 - delta / 2.0,
                dy,
                node: MNode::Box(low),
            }
        });
        // TeX's emission order: upper limit, operator, lower limit.
        children.push(Positioned {
            dx: op_dx,
            dy: 0.0,
            node: MNode::Box(op),
        });
        children.extend(lower);
        MBox {
            kind: BoxKind::Horizontal,
            width,
            height,
            depth,
            children,
        }
    }

    /// The Tex surface's top level is the body of the Reference's align*:
    /// `\\` rows of `&`-separated cells in r,l pairs, \minalignsep between
    /// pairs, rows opened up by \jot. A row without `&` sits in the
    /// right-aligned first column. Each cell is its own formula, as in
    /// amsmath's template, so a declaration ends with its cell. Unlike the
    /// environment, the box keeps its first row's baseline.
    fn align_star_rows(&self, items: &[Node], ctx: LayCtx, span: Span) -> Result<MBox, MathError> {
        let cell = |items: &[Node]| {
            let cell_span = match (items.first(), items.last()) {
                (Some(first), Some(last)) => Span {
                    start: first.span.start,
                    end: last.span.end,
                },
                _ => Span {
                    start: span.start,
                    end: span.start,
                },
            };
            Node::new(NodeKind::List(items.to_vec()), cell_span)
        };
        let mut rows: Vec<Vec<Node>> = items
            .split(|n| matches!(n.kind, NodeKind::Linebreak))
            .map(|row| {
                row.split(|n| matches!(n.kind, NodeKind::AlignTab))
                    .map(cell)
                    .collect()
            })
            .collect();
        // A trailing `\\` ends the last row without opening an empty one
        // (\crcr).
        let empty_row = |row: &[Node]| matches!(row, [Node { kind: NodeKind::List(items), .. }] if items.is_empty());
        if rows.len() > 1 && rows.last().is_some_and(|row| empty_row(row)) {
            rows.pop();
        }
        self.grid(
            &rows,
            ctx,
            &Grid {
                align: AlignRule::AlignPairs,
                col_sep: 1.0,
                jot: self.consts.jot,
                first_baseline: true,
                ..Grid::centered(1.0)
            },
            span,
        )
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
        let c = &self.consts_at(&ctx);
        let eff = match spec.forced_style {
            Some(forced) => ctx.map(|s| StyleCtx {
                style: forced,
                cramped: s.cramped,
            }),
            None => ctx,
        };
        let display = eff.style.style == Style::Display;
        let size = eff.size();
        let mut num_box = self.clean_box(num, eff.map(StyleCtx::num))?.boxx;
        if spec.cfrac.is_some() {
            // amsmath's `\strut` in the numerator: .7 and .3 of the
            // baselineskip at the declared size, a rule of no width (and so
            // no drawn primitive).
            let skip = self.consts.baseline_skip * eff.decl_size;
            num_box.height = num_box.height.max(0.7 * skip);
            num_box.depth = num_box.depth.max(0.3 * skip);
        }
        let den_box = self.clean_box(den, eff.map(StyleCtx::den))?.boxx;
        // `\genfrac`'s explicit thickness is TeX's `\above`: a length, which
        // the math style does not scale.
        let theta = spec
            .thickness
            .map_or(c.rule_thickness * size, |length| skip_width(length, eff));
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
        // `\cfrac[l]` and `[r]` put `\hfill` (fill glue) beside the
        // numerator, which outranks rebox's `\hss` (fil): flush left or right.
        let num_dx = match spec.cfrac {
            Some(LineAlign::Left) => 0.0,
            Some(LineAlign::Right) => inner_width - num_box.width,
            _ => (inner_width - num_box.width) / 2.0,
        };
        let den_dx = (inner_width - den_box.width) / 2.0;
        let height = u + num_box.height;
        let depth = v + den_box.depth;
        // TeX's emission order: numerator, bar, denominator.
        let mut children = vec![Positioned {
            dx: num_dx,
            dy: u,
            node: MNode::Box(num_box),
        }];
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
        children.push(Positioned {
            dx: den_dx,
            dy: -v,
            node: MNode::Box(den_box),
        });
        let core = MBox {
            kind: BoxKind::Horizontal,
            width: inner_width,
            height,
            depth,
            children,
        };
        // Rule 15e: wrap in delimiters of the style-fixed size. A fraction
        // without delimiters (\frac, \over) is wrapped in null delimiters,
        // `\nulldelimiterspace` on each side, as TeX's var_delimiter has it.
        let target = if display { c.delim1 } else { c.delim2 } * size;
        let (l, r) = spec
            .delims
            .map_or((None, None), |(l, r)| (Some(l), Some(r)));
        let left = self.delimiter_box(&Delim { ch: l, span }, target, eff)?;
        let right = self.delimiter_box(&Delim { ch: r, span }, target, eff)?;
        let mut parts = vec![left, core, right];
        if spec.cfrac.is_some() {
            // `\kern-\nulldelimiterspace` after the fraction's group.
            parts.push(kern_box(-self.consts.null_delimiter_space * size));
        }
        Ok(Laid {
            boxx: hcat(parts),
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
        let c = &self.consts_at(&ctx);
        let size = ctx.size();
        let x = self.clean_box(radicand, ctx.map(StyleCtx::cramp))?.boxx;
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
        // radicand's top; the sign's top aligns with the rule's top. The
        // box's depth is the deeper of the radicand and the sign, as TeX's
        // hpack of the shifted sign and the overbar (§737) has it, and its
        // height includes the overbar's kern of one rule thickness above the
        // rule (§705).
        let rule_y = x.height + psi;
        let sign_dy = rule_y + theta - sign.height;
        let mut pen = 0.0_f64;
        let mut children = Vec::new();
        let mut total_height = rule_y + 2.0 * theta;
        let mut total_depth = x.depth.max(sign.depth - sign_dy);
        // Degree: plain TeX's \r@@t, `\mkern5mu\raise.6\dimen@\copy\rootbox
        // \mkern-10mu\box0` with \dimen@ the radical box's height minus its
        // depth. The degree is its own formula (\setbox\rootbox\hbox{$...$}).
        // A degree narrower than 5mu starts the sign left of the origin, as
        // in TeX.
        if let Some(ix) = index {
            let ix_box = self.formula_box(
                ix,
                LayCtx {
                    style: StyleCtx {
                        style: Style::ScriptScript,
                        cramped: ctx.style.cramped,
                    },
                    ..ctx
                },
            )?;
            let raise = 0.6 * (total_height - total_depth);
            pen += 5.0 / 18.0 * c.quad * size;
            total_height = total_height.max(raise + ix_box.height);
            total_depth = total_depth.max(ix_box.depth - raise);
            let ix_width = ix_box.width;
            children.push(Positioned {
                dx: pen,
                dy: raise,
                node: MNode::Box(ix_box),
            });
            pen += ix_width - 10.0 / 18.0 * c.quad * size;
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
                depth: total_depth,
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
        let c = &self.consts_at(&ctx);
        let size = ctx.size();
        match kind {
            AccentKind::OverLine => {
                let inner = self.clean_box(base, ctx.map(StyleCtx::cramp))?.boxx;
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
                let inner = self.clean_box(base, ctx)?.boxx;
                Ok(Laid {
                    boxx: self.underline_box(inner, ctx, span),
                    italic: 0.0,
                    char_glyph: None,
                })
            }
            AccentKind::OverBrace | AccentKind::UnderBrace => self.brace(kind, base, ctx, span),
            AccentKind::OverRightArrow
            | AccentKind::OverLeftArrow
            | AccentKind::WideHat
            | AccentKind::WideTilde => self.stretchy_accent(kind, base, ctx, span),
            AccentKind::Dddot | AccentKind::Ddddot => {
                // amsmath builds `\dddot`/`\ddddot` from a row of dots (no
                // face carries U+20DB/U+20DC): resolve the single dot mark
                // once and place n copies contiguously by ink width,
                // centered over the base with rule 12's raise and skew.
                let n: u8 = if matches!(kind, AccentKind::Dddot) {
                    3
                } else {
                    4
                };
                let base_laid = self.clean_box(base, ctx.map(StyleCtx::cramp))?;
                let combining = accent_char(AccentKind::Dot);
                let Some((used, (face, gid))) = self
                    .faces
                    .resolve(combining, &[FACE_REGULAR, FACE_ITALIC, FACE_SYMBOLS])
                    .map(|hit| (combining, hit))
                    .or_else(|| {
                        accent_spacing_fallback(combining).and_then(|alt| {
                            self.faces
                                .resolve(alt, &[FACE_REGULAR, FACE_SYMBOLS])
                                .map(|hit| (alt, hit))
                        })
                    })
                else {
                    return Err(MathError::UnmappedChar {
                        ch: combining,
                        span,
                    });
                };
                // In math mode the dot is cmr10's own accent character.
                let (face, gid) = self
                    .faces
                    .math_roman(face, used, ctx.text_mode)
                    .unwrap_or((face, gid));
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
                let ink_w = ink_right - ink_left;
                let row_w = ink_w * f64::from(n);
                let h = base_laid.boxx.height;
                let dy = h - h.min(c.x_height * size);
                let skew = self.accent_skew(&base_laid, ctx);
                let row_left = base_laid.boxx.width / 2.0 + skew - row_w / 2.0;
                let width = base_laid.boxx.width;
                let height = base_laid.boxx.height.max(dy + ink_top);
                let depth = base_laid.boxx.depth.max(-(dy + ink_bottom));
                let mut children = Vec::with_capacity(usize::from(n) + 1);
                for i in 0..n {
                    children.push(Positioned {
                        dx: row_left + ink_w * f64::from(i) - ink_left,
                        dy,
                        node: MNode::Glyph {
                            face,
                            gid,
                            ch: combining,
                            size,
                            span,
                        },
                    });
                }
                children.push(Positioned {
                    dx: 0.0,
                    dy: 0.0,
                    node: MNode::Box(base_laid.boxx),
                });
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
            _ => {
                let base_laid = self.clean_box(base, ctx.map(StyleCtx::cramp))?;
                let combining = accent_char(kind);
                let (math_italic, code, [tl, tb, tr, _]) = tex_accent(kind);
                // The vector is cmmi's own accent character at the style's
                // optical size, TeX's glyph as it is.
                let cmmi = if math_italic {
                    let face = math_italic_face(ctx.style.style);
                    self.faces.tex_glyph(face, code).map(|gid| (face, gid))
                } else {
                    None
                };
                let (face, gid) = match cmmi {
                    Some(hit) => hit,
                    None => {
                        // The spacing accents of CM come first (cmr10's own
                        // in math mode); a combining mark from another face
                        // only where CM has none.
                        let spacing = accent_spacing_fallback(combining);
                        let resolved = spacing
                            .and_then(|alt| {
                                self.faces
                                    .resolve(alt, &[FACE_REGULAR])
                                    .map(|hit| (alt, hit))
                            })
                            .filter(|&(_, (face, _))| face == FACE_REGULAR)
                            .or_else(|| {
                                self.faces
                                    .resolve(combining, &[FACE_REGULAR, FACE_ITALIC, FACE_SYMBOLS])
                                    .map(|hit| (combining, hit))
                            })
                            .or_else(|| {
                                spacing.and_then(|alt| {
                                    self.faces
                                        .resolve(alt, &[FACE_REGULAR, FACE_SYMBOLS])
                                        .map(|hit| (alt, hit))
                                })
                            });
                        let Some((used, (face, gid))) = resolved else {
                            return Err(MathError::UnmappedChar {
                                ch: combining,
                                span,
                            });
                        };
                        // In math mode the accent is cmr10's own accent
                        // character.
                        self.faces
                            .math_roman(face, used, ctx.text_mode)
                            .unwrap_or((face, gid))
                    }
                };
                let Some(font) = self.faces.font(face) else {
                    return Err(MathError::UnmappedChar {
                        ch: combining,
                        span,
                    });
                };
                let upm = f64::from(font.units_per_em.max(1));
                let bbox = font.glyph_bbox(gid).unwrap_or([0, 0, 0, 0]);
                let [gl, gb, gr, _] = bbox.map(|v| f64::from(v) / upm);
                // Rule 12 with TeX's accent character: its box (the
                // advance) is centered over the base, skewed by the base
                // character's skewchar kern, its baseline at h(x) - min(h(x),
                // x-height). TeX's own character sits at its box; a glyph
                // from another face is scaled to that character's ink width
                // and its ink set where that character's ink sits.
                let (advance, accent_height) = tex_accent_box(math_italic, code, ctx.style.style);
                let scale = if cmmi.is_none() && gr > gl {
                    (tr - tl) / (gr - gl)
                } else {
                    1.0
                };
                let glyph_size = size * scale;
                let h = base_laid.boxx.height;
                let box_dy = h - h.min(c.x_height * size);
                let skew = self.accent_skew(&base_laid, ctx);
                let box_dx = (base_laid.boxx.width - advance * size) / 2.0 + skew;
                let (dx, dy) = if cmmi.is_some() {
                    (box_dx, box_dy)
                } else {
                    (
                        box_dx + tl * size - gl * glyph_size,
                        box_dy + tb * size - gb * glyph_size,
                    )
                };
                let ink_bottom = gb * glyph_size;
                let width = base_laid.boxx.width;
                // The vbox of the accent's box, a kern of -min(h, x-height)
                // and the base: TeX's height, whatever the drawn ink.
                let height = h.max(box_dy + accent_height * size);
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
                                    size: glyph_size,
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
    /// specs, the `align*` class, and `substack` (amsmath's single-column
    /// script-style stack) — column measurement, per-column
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
                // amsmath's \env@matrix is an `array` of centered columns
                // (its outer \arraycolsep cancelled).
                let grid = self.grid(
                    rows,
                    text_cells,
                    &Grid {
                        array_stretch: Some(1.0),
                        ..Grid::centered(1.0)
                    },
                    span,
                )?;
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
                // columns a quad apart, behind a stretched `{`: amsmath's
                // `array{@{}l@{\quad}l@{}}` with \arraystretch 1.2.
                let grid = self.grid(
                    rows,
                    text_cells,
                    &Grid {
                        align: AlignRule::AllLeft,
                        array_stretch: Some(1.2),
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
                        array_stretch: Some(1.0),
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
                        jot: self.consts.jot,
                        ..Grid::centered(1.0)
                    },
                    span,
                )?
            }
            "gather*" | "gathered" | "equation*" => {
                // amsmath's gather: display-style rows, each centred in
                // the widest, opened by \jot; equation* is one such row.
                let cells = ctx.map(|s| StyleCtx {
                    style: Style::Display,
                    cramped: s.cramped,
                });
                self.grid(
                    rows,
                    cells,
                    &Grid {
                        jot: self.consts.jot,
                        ..Grid::centered(1.0)
                    },
                    span,
                )?
            }
            "substack" => {
                // amsmath's \subarray{c}: every line set in \scriptstyle
                // (forced, whatever the ambient style — that is what makes
                // \substack usable in base position too), centered in one
                // column, \vcenter'd on the axis: the grid engine's
                // single-column case, rows on the shared
                // \baselineskip/\lineskip rule at script size.
                let cells = ctx.map(|_| StyleCtx::new(Style::Script));
                self.grid(rows, cells, &Grid::centered(1.0), span)?
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
        let c = &self.consts_at(&cell_ctx);
        // 1. Lay every cell.
        let mut cells: Vec<Vec<MBox>> = Vec::new();
        for row in rows {
            let mut out = Vec::new();
            for (j, cell) in row.iter().enumerate() {
                // Each cell is its own formula (`$…$` in the \halign
                // template), so a lone character keeps its rule-17 kern.
                // amsmath sets an align pair's right-hand cell as `{{}##}`:
                // an empty Ord opens it, so a relation or operator there
                // keeps its left-hand space (`a &= b` is `a = b` wide).
                let box_ = if matches!(grid.align, AlignRule::AlignPairs) && j % 2 == 1 {
                    self.formula_box(&opened_by_empty_ord(cell), cell_ctx)?
                } else {
                    self.formula_box(cell, cell_ctx)?
                };
                out.push(box_);
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
        // An array row is at least as tall and deep as its \@arstrut, which
        // follows the text size in force, not the math style.
        let (strut_h, strut_d) = grid.array_stretch.map_or((0.0, 0.0), |stretch| {
            let skip = c.baseline_skip * stretch * cell_ctx.decl_size;
            (0.7 * skip, 0.3 * skip)
        });
        let row_h: Vec<f64> = cells
            .iter()
            .map(|r| r.iter().map(|b| b.height).fold(strut_h, f64::max))
            .collect();
        let row_d: Vec<f64> = cells
            .iter()
            .map(|r| r.iter().map(|b| b.depth).fold(strut_d, f64::max))
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
        // Array rows instead stack depth to height (LaTeX's array sets
        // \baselineskip and \lineskip to zero; the struts space the rows).
        let mut baselines = vec![0.0_f64; cells.len()];
        for i in 1..cells.len() {
            let pitch = if grid.array_stretch.is_some() {
                row_d[i - 1] + row_h[i]
            } else {
                let natural = (c.baseline_skip + grid.jot) * grid.row_factor * size;
                // \openup\jot raises the \lineskip floor too.
                let min_gap = row_d[i - 1] + row_h[i] + (c.line_skip + grid.jot) * size;
                natural.max(min_gap)
            };
            baselines[i] = baselines[i - 1] - pitch;
        }
        // 5. Assemble, then axis-center the whole grid (\vcenter).
        let top = row_h.first().copied().unwrap_or(0.0);
        let bottom =
            baselines.last().copied().unwrap_or(0.0) - row_d.last().copied().unwrap_or(0.0);
        let total = top - bottom;
        let axis = c.axis_height * cell_ctx.size();
        let (height, shift, depth) = if grid.first_baseline {
            (top, 0.0, (-bottom).max(0.0))
        } else {
            let height = total / 2.0 + axis;
            // `shift` is added to every child dy.
            (height, height - top, (total / 2.0 - axis).max(0.0))
        };
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
        let c = &self.consts_at(&ctx);
        let size = ctx.size();
        let axis = c.axis_height * size;
        let delta = (inner.height - axis).max(inner.depth + axis);
        let target =
            (2.0 * delta * c.delimiter_factor).max(2.0 * delta - c.delimiter_shortfall * size);
        let left_box = self.delimiter_box(&Delim { ch: left, span }, target, ctx)?;
        let right_box = self.delimiter_box(&Delim { ch: right, span }, target, ctx)?;
        Ok(hcat(vec![left_box, inner, right_box]))
    }

    /// LaTeX's `\overbrace` and `\underbrace` (fontmath.ltx): a `\mathop`
    /// alignment of the base, in display style and centered, and a brace
    /// fill 3 pt from it, with 3 pt more past the fill. The fill is four
    /// cmex pieces, 0.45 em wide and 0.12 em high, joined by rules, so it
    /// is never narrower than 1.8 em ([`Self::cmex_brace_fill`]). Without
    /// cmex10 in the roster a drawn band stands in, centered where the
    /// pieces' ink would be (it curls 0.214 em to either side of their
    /// row). cmex has one size, so neither the fill nor the kerns shrink
    /// in script styles.
    fn brace(
        &self,
        kind: AccentKind,
        base: &Node,
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        let em = ctx.decl_size;
        let under = matches!(kind, AccentKind::UnderBrace);
        let display = LayCtx {
            style: StyleCtx::new(Style::Display),
            ..ctx
        };
        let inner = self.formula_box(base, display)?;
        if let Some(fill) = self.cmex_brace_fill(under, inner.width, em, span) {
            // The fill row's baseline: 3 pt below the base's depth (less
            // the row), or 3 pt above its height; 3 pt more past the row.
            let kern = 0.3 * em;
            let (fill_dy, height, depth) = if under {
                let dy = -(inner.depth + kern + fill.height);
                (dy, inner.height, inner.depth + 2.0 * kern + fill.height)
            } else {
                let dy = inner.height + kern;
                (dy, inner.height + 2.0 * kern + fill.height, inner.depth)
            };
            let width = fill.width;
            let base = Positioned {
                dx: (width - inner.width) / 2.0,
                dy: 0.0,
                node: MNode::Box(inner),
            };
            let fill = Positioned {
                dx: 0.0,
                dy: fill_dy,
                node: MNode::Box(fill),
            };
            // TeX's emission order is top to bottom.
            let children = if under {
                vec![base, fill]
            } else {
                vec![fill, base]
            };
            return Ok(Laid {
                boxx: MBox {
                    kind: BoxKind::Horizontal,
                    width,
                    height,
                    depth,
                    children,
                },
                italic: 0.0,
                char_glyph: None,
            });
        }
        // No cmex10 in the roster: the drawn band, centred where the
        // pieces' ink would be.
        let width = inner.width.max(4.0 * 0.45 * em);
        let stretch_kind = if under {
            crate::drawn::Stretch::UnderBrace
        } else {
            crate::drawn::Stretch::OverBrace
        };
        let band = crate::drawn::stretch(stretch_kind, width, em);
        // The fill's ink center, 3 pt plus half its 0.12 em row from the
        // base; the whole construction adds both kerns and the row.
        let center = 0.3 * em + 0.06 * em;
        let extra = 0.3 * em + 0.12 * em + 0.3 * em;
        let (band_dy, height, depth) = if under {
            let dy = -(inner.depth + center) - band.height / 2.0;
            (dy, inner.height, inner.depth + extra)
        } else {
            let dy = inner.height + center - band.height / 2.0;
            (dy, inner.height + extra, inner.depth)
        };
        let inner_dx = (width - inner.width) / 2.0;
        let band = Positioned {
            dx: 0.0,
            dy: band_dy,
            node: MNode::Path {
                contours: band.contours,
                span,
            },
        };
        let base = Positioned {
            dx: inner_dx,
            dy: 0.0,
            node: MNode::Box(inner),
        };
        // TeX's emission order is top to bottom: the brace over its base,
        // the base over its brace.
        let children = if under {
            vec![base, band]
        } else {
            vec![band, base]
        };
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

    /// LaTeX's `\downbracefill` (under `\underbrace`) or `\upbracefill`
    /// (over `\overbrace`), at least `width` wide: cmex10's four brace
    /// pieces in two pairs, each pair joined by a `\leaders\vrule` as high
    /// as `\braceld` (the fill row), the two rules sharing whatever width
    /// the pieces leave. The order is TeX's `\showbox`:
    /// - `\underbrace`: 0x7C, rule, 0x7B, 0x7A, rule, 0x7D;
    /// - `\overbrace`: 0x7A, rule, 0x7D, 0x7C, rule, 0x7B.
    ///
    /// cmex has one size, so `em` is the declared size. The pieces' ink
    /// curls past the row. `None` when the roster lacks cmex10's pieces.
    fn cmex_brace_fill(&self, under: bool, width: f64, em: f64, span: Span) -> Option<MBox> {
        let codes: [u8; 4] = if under {
            [0x7C, 0x7B, 0x7A, 0x7D]
        } else {
            [0x7A, 0x7D, 0x7C, 0x7B]
        };
        let mut gids = [0_u16; 4];
        for (gid, code) in gids.iter_mut().zip(codes) {
            *gid = self.faces.tex_glyph(FACE_MATH_EXTENSION, code)?;
        }
        let metrics = gids.map(|gid| self.char_metrics(FACE_MATH_EXTENSION, gid, false));
        // \ht of \hbox{$\braceld$}: braceld is 0x7A.
        let braceld = self.faces.tex_glyph(FACE_MATH_EXTENSION, 0x7A)?;
        let row = self
            .char_metrics(FACE_MATH_EXTENSION, braceld, false)
            .height
            * em;
        let pieces = metrics.iter().map(|m| m.advance).sum::<f64>() * em;
        let rule = ((width - pieces) / 2.0).max(0.0);
        let ch = if under { '⏟' } else { '⏞' };
        let mut x = 0.0;
        let mut children = Vec::with_capacity(6);
        for (i, (gid, m)) in gids.into_iter().zip(metrics).enumerate() {
            children.push(Positioned {
                dx: x,
                dy: 0.0,
                node: MNode::Glyph {
                    face: FACE_MATH_EXTENSION,
                    gid,
                    ch,
                    size: em,
                    span,
                },
            });
            x += m.advance * em;
            // A fill of no width ships nothing.
            if i % 2 == 0 && rule > 0.0 {
                children.push(Positioned {
                    dx: x,
                    dy: 0.0,
                    node: MNode::Rule {
                        width: rule,
                        height: row,
                        span,
                    },
                });
                x += rule;
            }
        }
        Some(MBox {
            kind: BoxKind::Horizontal,
            width: pieces + 2.0 * rule,
            height: row,
            depth: 0.0,
            children,
        })
    }

    /// The stretchy over-constructions (`\widehat`, `\widetilde`,
    /// `\overrightarrow`, `\overleftarrow`): a drawn band spanning the
    /// base's width, placed with a small rule-thickness clearance (hats
    /// and tildes ride close, the way the authored accents do; arrows take
    /// a little more air). Any width draws — the constructions are total.
    fn stretchy_accent(
        &self,
        kind: AccentKind,
        base: &Node,
        ctx: LayCtx,
        span: Span,
    ) -> Result<Laid, MathError> {
        let size = ctx.size();
        let theta = self.consts_at(&ctx).rule_thickness * size;
        // Over-accents cramp their base (rule 12 / overline's rule 9). The
        // wide accents clean their base and the arrows are amsmath
        // alignments whose body is its own formula; either way a lone
        // character keeps its italic correction in the width.
        let inner = self.clean_box(base, ctx.map(StyleCtx::cramp))?.boxx;
        let stretch_kind = match kind {
            AccentKind::WideHat => crate::drawn::Stretch::Hat,
            AccentKind::WideTilde => crate::drawn::Stretch::Tilde,
            AccentKind::OverRightArrow => crate::drawn::Stretch::RightArrow,
            _ => crate::drawn::Stretch::LeftArrow,
        };
        let width = inner.width.max(0.25 * size);
        let band = crate::drawn::stretch(stretch_kind, width, size);
        let gap = match kind {
            AccentKind::WideHat | AccentKind::WideTilde => theta,
            _ => 2.0 * theta,
        };
        let band_dy = inner.height + gap;
        let (height, depth) = (inner.height + gap + band.height, inner.depth);
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
        let theta = self.consts_at(&ctx).rule_thickness * ctx.size();
        let rule_y = -(inner.depth + 3.0 * theta) - theta;
        let width = inner.width;
        MBox {
            kind: BoxKind::Horizontal,
            width,
            height: inner.height,
            depth: -rule_y + 2.0 * theta,
            // TeX's emission order: the base, then the rule under it.
            children: vec![
                Positioned {
                    dx: 0.0,
                    dy: 0.0,
                    node: MNode::Box(inner),
                },
                Positioned {
                    dx: 0.0,
                    dy: rule_y,
                    node: MNode::Rule {
                        width,
                        height: theta,
                        span,
                    },
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
        let c = &self.consts_at(&ctx);
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

    /// The ADR-0005 delimiter mechanism, complete, at the size TeX's
    /// variant search picks for the target ([`tex_delimiter_total`]):
    /// natural glyph if it covers that size; uniform scaling up to the
    /// `1.25×` ceiling; the
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
        // TeX sets the variant it finds for the target, not the target:
        // every mechanism below draws at that total.
        let target_total = if ctx.size() > 0.0 {
            tex_delimiter_total(ch, target_total / ctx.size()) * ctx.size()
        } else {
            target_total
        };
        if ctx.size() > 0.0
            && let Some(boxx) = self.cmex_delimiter(ch, target_total / ctx.size(), ctx, delim.span)
        {
            return Ok(boxx);
        }
        // The small variant is the character's own face, as TeX sets it:
        // cmsy for braces, bars, angles, floors and ceilings, cmmi for the
        // slash, and math family 0 for parentheses and brackets. Family 0 is
        // cmr10 at display and text size, not CM Unicode, whose brackets
        // carry 1.48 times cmr10's ink. Each comes at the style's optical size.
        let Some((face, gid)) = self
            .faces
            .resolve(
                ch,
                &[
                    FACE_MATH_SYMBOLS,
                    FACE_MATH_ITALIC,
                    FACE_REGULAR,
                    FACE_SYMBOLS,
                    FACE_ITALIC,
                ],
            )
            .map(|(face, gid)| {
                self.faces
                    .tex_face(face, ch, ctx.style.style, ctx.text_mode)
                    .unwrap_or((face, gid))
            })
        else {
            // No authored glyph at all (e.g. `‖` on a face without it): the
            // drawn construction serves every size, floored at a text-ish
            // height so an empty body still gets a visible delimiter.
            if let Some(d) =
                crate::drawn::delimiter(ch, target_total.max(0.7 * ctx.size()), ctx.size())
            {
                let total = target_total.max(0.7 * ctx.size());
                let axis = self.consts_at(&ctx).axis_height * ctx.size();
                let dy = axis - total / 2.0;
                let (width, dx) = framed(
                    ch,
                    total,
                    ctx.size(),
                    d.width,
                    contour_x_extent(&d.contours),
                );
                return Ok(MBox {
                    kind: BoxKind::Horizontal,
                    width,
                    height: total + dy,
                    depth: (-dy).max(0.0),
                    children: vec![Positioned {
                        dx,
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
        let m = self.char_metrics(face, gid, ctx.text_mode);
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
            let axis = self.consts_at(&ctx).axis_height * ctx.size();
            let dy = axis - target_total / 2.0;
            let (width, dx) = framed(
                ch,
                target_total,
                ctx.size(),
                d.width,
                contour_x_extent(&d.contours),
            );
            return Ok(MBox {
                kind: BoxKind::Horizontal,
                width,
                height: target_total + dy,
                depth: (-dy).max(0.0),
                children: vec![Positioned {
                    dx,
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
        let axis = self.consts_at(&ctx).axis_height * ctx.size();
        let dy = axis - (gh - gd) / 2.0;
        let ink = self.faces.font(face).and_then(|font| {
            let upm = f64::from(font.units_per_em.max(1));
            font.glyph_bbox(gid).map(|bbox| {
                (
                    f64::from(bbox[0]) / upm * size,
                    f64::from(bbox[2]) / upm * size,
                )
            })
        });
        let (width, dx) = framed(ch, target_total, ctx.size(), m.advance * size, ink);
        Ok(MBox {
            kind: BoxKind::Horizontal,
            width,
            height: gh + dy,
            depth: (gd - dy).max(0.0),
            children: vec![Positioned {
                dx,
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

impl Engine {
    /// cmex10's own delimiter at a total TeX's `var_delimiter` lands on
    /// (`total` ems, from [`tex_delimiter_total`]): a 1.2, 1.8, 2.4 or
    /// 3.0 em successor glyph, or past those the extensible recipe built
    /// from cmex's pieces as tex.web §713 stacks them — top, repeaters,
    /// middle, repeaters, bottom, each piece's char box filling its slot.
    /// Axis-centered; the box width is the glyph's TFM width. `None` at the
    /// small variant, when the roster lacks cmex10, or when `ch` has no cmex
    /// form at `total` (characters TeX does not chain), which leaves the
    /// drawn construction to serve.
    fn cmex_delimiter(&self, ch: char, total: f64, ctx: LayCtx, span: Span) -> Option<MBox> {
        if total <= 1.0 + 1e-9 {
            return None;
        }
        let size = ctx.size();
        let top = self.consts_at(&ctx).axis_height * size + total * size / 2.0;
        let piece = |code: u8| self.faces.tex_glyph(FACE_MATH_EXTENSION, code);
        let glyph = |gid: u16, dy: f64| Positioned {
            dx: 0.0,
            dy,
            node: MNode::Glyph {
                face: FACE_MATH_EXTENSION,
                gid,
                ch,
                size,
                span,
            },
        };
        let mut children = Vec::new();
        let width_gid;
        if let Some(code) = cmex_successor(ch, total) {
            let gid = piece(code)?;
            // The successor's TFM height is its ink top (0.04 em); its box,
            // height plus depth `total`, centers on the axis.
            let m = self.char_metrics(FACE_MATH_EXTENSION, gid, false);
            children.push(glyph(gid, top - m.height * size));
            width_gid = gid;
        } else {
            let recipe = cmex_recipe(ch)?;
            let fixed = recipe.top.map_or(0.0, |(_, h)| h)
                + recipe.mid.map_or(0.0, |(_, h)| h)
                + recipe.bot.map_or(0.0, |(_, h)| h);
            let (rep, rep_size) = recipe.rep;
            let sides = if recipe.mid.is_some() { 2.0 } else { 1.0 };
            let per_side = ((total - fixed) / (rep_size * sides)).round().max(0.0);
            // A recipe of at most 3.6 em of fixed pieces plus whole
            // repeaters: the count is small and exact.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let per_side = per_side as usize;
            let rep_gid = piece(rep)?;
            let mut slots: Vec<(u16, f64)> = Vec::new();
            if let Some((code, h)) = recipe.top {
                slots.push((piece(code)?, h));
            }
            slots.extend(std::iter::repeat_n((rep_gid, rep_size), per_side));
            if let Some((code, h)) = recipe.mid {
                slots.push((piece(code)?, h));
                slots.extend(std::iter::repeat_n((rep_gid, rep_size), per_side));
            }
            if let Some((code, h)) = recipe.bot {
                slots.push((piece(code)?, h));
            }
            // tex.web §713 stacks the pieces' char boxes in a vlist, so each
            // baseline sits its TFM height below the top of its slot (0.04 em
            // for the round and square tops and bottoms and the radical's
            // top); the designed overlaps then join every piece.
            let mut y = top;
            for (gid, h) in slots {
                let height = self.char_metrics(FACE_MATH_EXTENSION, gid, false).height;
                children.push(glyph(gid, y - height * size));
                y -= h * size;
            }
            width_gid = rep_gid;
        }
        // TeX's box is the TFM width; the Type 1 advance is that width
        // rounded to 1/1000 em, which would drift every delimited array.
        let width = tex_delimiter_frame(ch, total).map_or_else(
            || {
                self.char_metrics(FACE_MATH_EXTENSION, width_gid, false)
                    .advance
            },
            |(advance, _, _)| advance,
        ) * size;
        Some(MBox {
            kind: BoxKind::Horizontal,
            width,
            height: top,
            depth: (total * size - top).max(0.0),
            children,
        })
    }
}

/// cmex10's successor of `ch` at a `\big`-family total: its code at 1.2,
/// 1.8, 2.4 or 3.0 em (tex.web's `charlist` from the small variant).
fn cmex_successor(ch: char, total: f64) -> Option<u8> {
    let codes: [u8; 4] = match ch {
        '(' => [0x00, 0x10, 0x12, 0x20],
        ')' => [0x01, 0x11, 0x13, 0x21],
        '[' => [0x02, 0x68, 0x14, 0x22],
        ']' => [0x03, 0x69, 0x15, 0x23],
        '⌊' => [0x04, 0x6A, 0x16, 0x24],
        '⌋' => [0x05, 0x6B, 0x17, 0x25],
        '⌈' => [0x06, 0x6C, 0x18, 0x26],
        '⌉' => [0x07, 0x6D, 0x19, 0x27],
        '{' => [0x08, 0x6E, 0x1A, 0x28],
        '}' => [0x09, 0x6F, 0x1B, 0x29],
        '⟨' | '〈' => [0x0A, 0x44, 0x1C, 0x2A],
        '⟩' | '〉' => [0x0B, 0x45, 0x1D, 0x2B],
        '/' => [0x0E, 0x2E, 0x1E, 0x2C],
        '\\' => [0x0F, 0x2F, 0x1F, 0x2D],
        '√' => [0x70, 0x71, 0x72, 0x73],
        _ => return None,
    };
    [1.2, 1.8, 2.4, 3.0]
        .iter()
        .position(|size| (total - size).abs() < 1e-9)
        .map(|i| codes[i])
}

/// A cmex10 extensible recipe: optional top, middle and bottom pieces and
/// the repeater, each as (code, nominal total in ems).
struct CmexRecipe {
    top: Option<(u8, f64)>,
    mid: Option<(u8, f64)>,
    bot: Option<(u8, f64)>,
    rep: (u8, f64),
}

/// cmex10's extensible recipes (cmex10.tfm's `exten` table).
fn cmex_recipe(ch: char) -> Option<CmexRecipe> {
    let r =
        |top: Option<u8>, mid: Option<u8>, bot: Option<u8>, rep: u8, sizes: [f64; 4]| CmexRecipe {
            top: top.map(|c| (c, sizes[0])),
            mid: mid.map(|c| (c, sizes[1])),
            bot: bot.map(|c| (c, sizes[2])),
            rep: (rep, sizes[3]),
        };
    const ROUND: [f64; 4] = [1.8, 0.0, 1.8, 0.6];
    const BRACE: [f64; 4] = [0.9, 1.8, 0.9, 0.3];
    const RADICAL: [f64; 4] = [0.6, 0.0, 1.8, 0.6];
    Some(match ch {
        '√' => r(Some(0x76), None, Some(0x74), 0x75, RADICAL),
        '(' => r(Some(0x30), None, Some(0x40), 0x42, ROUND),
        ')' => r(Some(0x31), None, Some(0x41), 0x43, ROUND),
        '[' => r(Some(0x32), None, Some(0x34), 0x36, ROUND),
        ']' => r(Some(0x33), None, Some(0x35), 0x37, ROUND),
        '⌈' => r(Some(0x32), None, None, 0x36, ROUND),
        '⌉' => r(Some(0x33), None, None, 0x37, ROUND),
        '⌊' => r(None, None, Some(0x34), 0x36, ROUND),
        '⌋' => r(None, None, Some(0x35), 0x37, ROUND),
        '{' => r(Some(0x38), Some(0x3C), Some(0x3A), 0x3E, BRACE),
        '}' => r(Some(0x39), Some(0x3D), Some(0x3B), 0x3E, BRACE),
        '|' => r(None, None, None, 0x0C, ROUND),
        '‖' => r(None, None, None, 0x0D, ROUND),
        _ => return None,
    })
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

/// The `\big` family's total sizes in ems: the cmex variants TeX lands on,
/// 1.2, 1.8, 2.4 and 3.0 em, axis-centered. Their heights are the plain-TeX
/// struts (8.5, 11.5, 14.5 and 17.5 pt at 10 pt), which are not the totals.
fn fixed_delim_target(size: DelimSize) -> f64 {
    match size {
        DelimSize::Big => 1.2,
        DelimSize::BBig => 1.8,
        DelimSize::Bigg => 2.4,
        DelimSize::BBigg => 3.0,
    }
}

/// TeX's `var_delimiter` (tex.web §706–§713) over cmex10: the total height
/// plus depth, in ems, of the variant TeX sets for delimiter `ch` when it
/// must cover `target` ems. A target within the small variant (cmr10's and
/// cmsy10's delimiters are all 1.0 em) is the face's own glyph and comes
/// back unchanged. Past it TeX takes the first of cmex10's successors, 1.2,
/// 1.8, 2.4 and 3.0 em, that covers the target. Past those it builds the
/// extensible recipe: its fixed pieces plus as many repeaters as cover the
/// target. A brace repeats on both sides of its middle piece, 2 × 0.3 em
/// per step. Angles and slashes have no recipe, so TeX stops at their
/// largest variant even when it falls short. Bars have no successors, only
/// the repeater. Characters TeX does not chain keep the exact target.
fn tex_delimiter_total(ch: char, target: f64) -> f64 {
    // (successors, extensible recipe as (fixed pieces, step)), ems.
    const SUCCESSORS: &[f64] = &[1.2, 1.8, 2.4, 3.0];
    let (successors, recipe): (&[f64], Option<(f64, f64)>) = match ch {
        '(' | ')' | '[' | ']' | '{' | '}' => (SUCCESSORS, Some((3.6, 0.6))),
        '⌈' | '⌉' | '⌊' | '⌋' => (SUCCESSORS, Some((1.8, 0.6))),
        '√' => (SUCCESSORS, Some((2.4, 0.6))),
        '⟨' | '⟩' | '〈' | '〉' | '/' | '\\' => (SUCCESSORS, None),
        '|' | '‖' => (&[], Some((0.0, 0.6))),
        _ => return target,
    };
    if !target.is_finite() || target <= 1.0 {
        return target;
    }
    // TeX compares scaled points exactly. In decimal ems an exact hit such
    // as `\Big|`'s 1.8 em is 3 × 0.6 only up to rounding, so the
    // comparisons forgive 1e-9 em.
    let need = target - 1e-9;
    if let Some(&size) = successors.iter().find(|&&size| size >= need) {
        return size;
    }
    match recipe {
        Some((fixed, step)) => fixed + ((need - fixed) / step).ceil().max(0.0) * step,
        None => successors.last().copied().unwrap_or(target),
    }
}

/// The cmex10 frame of the variant TeX sets for `ch` at `total` ems: its box
/// width (the TFM width; for an extensible, its repeater's, as tex.web §713
/// builds it) and the left and right edges of its ink (cmex10.afm, the union
/// of an extensible's pieces), in ems. `None` for the small variant, which
/// is the face's own glyph, and for characters TeX does not chain. Rows run
/// 1.2, 1.8, 2.4 and 3.0 em, then the extensible.
fn tex_delimiter_frame(ch: char, total: f64) -> Option<(f64, f64, f64)> {
    type Frame = (f64, f64, f64);
    let frames: &[Frame] = match ch {
        '(' => &[
            (0.458336, 0.152, 0.413),
            (0.597224, 0.180, 0.560),
            (0.736115, 0.208, 0.700),
            (0.791670, 0.236, 0.757),
            (0.875003, 0.291, 0.842),
        ],
        ')' => &[
            (0.458336, 0.044, 0.305),
            (0.597224, 0.036, 0.416),
            (0.736115, 0.035, 0.527),
            (0.791670, 0.034, 0.555),
            (0.875003, 0.032, 0.583),
        ],
        '[' => &[
            (0.416669, 0.202, 0.394),
            (0.472224, 0.226, 0.453),
            (0.527781, 0.250, 0.513),
            (0.583336, 0.275, 0.571),
            (0.666669, 0.326, 0.659),
        ],
        ']' => &[
            (0.416669, 0.022, 0.214),
            (0.472224, 0.018, 0.245),
            (0.527781, 0.014, 0.277),
            (0.583336, 0.011, 0.307),
            (0.666669, 0.007, 0.340),
        ],
        '⌊' | '⌈' => &[
            (0.472224, 0.202, 0.449),
            (0.527781, 0.226, 0.509),
            (0.583336, 0.250, 0.568),
            (0.638891, 0.275, 0.627),
            (0.666669, 0.326, 0.659),
        ],
        '⌋' | '⌉' => &[
            (0.472224, 0.022, 0.269),
            (0.527781, 0.018, 0.301),
            (0.583336, 0.014, 0.332),
            (0.638891, 0.011, 0.363),
            (0.666669, 0.007, 0.340),
        ],
        '{' | '}' => &[
            (0.583336, 0.113, 0.469),
            (0.666669, 0.119, 0.547),
            (0.750002, 0.131, 0.618),
            (0.805560, 0.144, 0.661),
            (0.888891, 0.170, 0.718),
        ],
        '⟨' | '〈' => &[
            (0.472224, 0.098, 0.393),
            (0.611113, 0.112, 0.522),
            (0.750002, 0.125, 0.652),
            (0.805560, 0.139, 0.697),
        ],
        '⟩' | '〉' => &[
            (0.472224, 0.078, 0.373),
            (0.611113, 0.088, 0.498),
            (0.750002, 0.097, 0.624),
            (0.805560, 0.108, 0.666),
        ],
        '/' | '\\' => &[
            (0.577779, 0.056, 0.521),
            (0.811114, 0.056, 0.754),
            (1.044447, 0.056, 0.987),
            (1.277781, 0.056, 1.221),
        ],
        // Bars have no successors: past the small variant they are built.
        '|' => return (total > 1.0 + 1e-9).then_some((0.333334, 0.145, 0.188)),
        '‖' => return (total > 1.0 + 1e-9).then_some((0.555557, 0.145, 0.410)),
        _ => return None,
    };
    if let Some(i) = [1.2, 1.8, 2.4, 3.0]
        .iter()
        .position(|size| (total - size).abs() < 1e-9)
    {
        return frames.get(i).copied();
    }
    // Past 3.0 em only an extensible recipe (a fifth row) reaches.
    if total > 3.0 {
        frames.get(4).copied()
    } else {
        None
    }
}

/// How many `\intop`s amsmath's `\MultiIntegral` sets for a multiple
/// integral: `\iint` 2, `\iiint` 3, `\iiiint` 4. `None` for every other
/// operator, `\oiint` included (amsmath has none).
const fn multi_integral_count(ch: char) -> Option<u8> {
    match ch {
        '∬' => Some(2),
        '∭' => Some(3),
        '⨌' => Some(4),
        _ => None,
    }
}

/// The space amsmath's `\ints@b` opens a limits-carrying multiple integral's
/// `\mathop` with, in mu: `\mkern7mu\mathchoice{\mkern2mu}{}{}{}`.
const fn multi_integral_lead(display: bool) -> f64 {
    if display { 9.0 } else { 7.0 }
}

/// cmex10's text and display code positions of a big operator.
const fn big_op_codes(ch: char) -> Option<(u8, u8)> {
    Some(match ch {
        '∑' => (0x50, 0x58),
        '∏' => (0x51, 0x59),
        '∫' => (0x52, 0x5A),
        '∮' => (0x48, 0x49),
        '⋃' => (0x53, 0x5B),
        '⋂' => (0x54, 0x5C),
        '⨄' => (0x55, 0x5D),
        '⋀' => (0x56, 0x5E),
        '⋁' => (0x57, 0x5F),
        '∐' => (0x60, 0x61),
        '⨀' => (0x4A, 0x4B),
        '⨁' => (0x4C, 0x4D),
        '⨂' => (0x4E, 0x4F),
        '⨆' => (0x46, 0x47),
        _ => return None,
    })
}

/// cmex10's frame for big operator `ch` in its text or display variant: the
/// TFM width and italic correction, and the left and right edges of its ink
/// (cmex10.afm), in ems. `None` where cmex10 has no single character
/// (amsmath builds `\iint` and its kin from `\int`).
fn tex_op_frame(ch: char, display: bool) -> Option<(f64, f64, f64, f64)> {
    type Frame = (f64, f64, f64, f64);
    let [text, shown]: [Frame; 2] = match ch {
        '∑' => [(1.055559, 0.0, 0.056, 0.999), (1.444448, 0.0, 0.056, 1.387)],
        '∏' | '∐' => [(0.944447, 0.0, 0.056, 0.887), (1.277781, 0.0, 0.056, 1.221)],
        '∫' | '∮' => [
            (0.472223, 0.194446, 0.056, 0.609),
            (0.555557, 0.444446, 0.056, 0.943),
        ],
        '⋃' | '⋂' | '⨄' | '⋀' | '⋁' | '⨆' => {
            [(0.833336, 0.0, 0.056, 0.776), (1.111115, 0.0, 0.056, 1.054)]
        }
        '⨀' | '⨁' | '⨂' => [(1.111115, 0.0, 0.056, 1.054), (1.511116, 0.0, 0.056, 1.454)],
        _ => return None,
    };
    Some(if display { shown } else { text })
}

/// A big operator set without scripts keeps its italic correction in its
/// box, as TeX's clean_box of the operator character does (tex.web §749):
/// a lone display `\int` is cmex's 0.5556 em width plus its 0.4444 em
/// correction. A scripted operator reaches the line as a script box, which
/// places δ itself (the superscript sits δ further right than the
/// subscript).
fn lone_op_box(node: &Node, mut laid: Laid) -> Laid {
    if matches!(node.kind, NodeKind::BigOp { .. }) {
        laid.boxx.width += laid.italic;
        laid.italic = 0.0;
    }
    laid
}

/// The box width and ink offset for a delimiter construction of box width
/// `width` and ink `[x0, x1]` (absolute units), when TeX's variant at
/// `total` has a cmex10 frame: the box takes TeX's width, and the ink edge
/// that faces the content lands where cmex's does (the right edge of an
/// opening delimiter, the left of a closing one, the centre of a bar or
/// slash), so the gap to the content is TeX's whatever the drawn ink's
/// weight. Without a frame the construction keeps its own box.
fn framed(ch: char, total: f64, em: f64, width: f64, ink: Option<(f64, f64)>) -> (f64, f64) {
    let (Some((advance, cx0, cx1)), Some((x0, x1))) = (tex_delimiter_frame(ch, total / em), ink)
    else {
        return (width, 0.0);
    };
    let dx = match ch {
        '(' | '[' | '{' | '⌈' | '⌊' | '⟨' | '〈' => cx1 * em - x1,
        ')' | ']' | '}' | '⌉' | '⌋' | '⟩' | '〉' => cx0 * em - x0,
        _ => (cx0 + cx1) / 2.0 * em - (x0 + x1) / 2.0,
    };
    (advance * em, dx)
}

/// The horizontal ink extent of quadratic contours: line ends, and each
/// quad's true extremum where it bulges past its ends (a control point
/// alone would overstate a curve's reach).
fn contour_x_extent(contours: &[crate::mbox::PathContour]) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut take = |x: f64| {
        lo = lo.min(x);
        hi = hi.max(x);
    };
    for contour in contours {
        let mut at = contour.start.0;
        take(at);
        for seg in &contour.segments {
            match seg {
                crate::mbox::PathSeg::Line { to } => at = to.0,
                crate::mbox::PathSeg::Quad { ctrl, to } => {
                    let denom = at - 2.0 * ctrl.0 + to.0;
                    if denom != 0.0 {
                        let t = (at - ctrl.0) / denom;
                        if t > 0.0 && t < 1.0 {
                            let u = 1.0 - t;
                            take(u * u * at + 2.0 * t * u * ctrl.0 + t * t * to.0);
                        }
                    }
                    at = to.0;
                }
            }
            take(at);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

fn text_face(ctx: LayCtx) -> FaceId {
    match ctx.alphabet {
        Some(MathFont::Bold) => FACE_BOLD,
        Some(MathFont::Italic) => FACE_ITALIC,
        Some(MathFont::Typewriter) => FACE_TYPEWRITER,
        Some(MathFont::SansSerif) => FACE_SANS,
        _ => FACE_REGULAR,
    }
}

/// The accent character plain TeX's `\mathaccent` sets for `kind`: its
/// code in cmr (or cmmi, for the vector), and its ink's left, bottom, right
/// and top in ems from cmr10's (cmmi10's) AFM metrics.
const fn tex_accent(kind: AccentKind) -> (bool, u8, [f64; 4]) {
    match kind {
        AccentKind::Hat => (false, 0x5E, [0.116, 0.540, 0.383, 0.694]),
        AccentKind::Check => (false, 0x14, [0.118, 0.516, 0.381, 0.638]),
        AccentKind::Tilde => (false, 0x7E, [0.083, 0.575, 0.416, 0.668]),
        AccentKind::Acute => (false, 0x13, [0.206, 0.510, 0.392, 0.698]),
        AccentKind::Grave => (false, 0x12, [0.107, 0.510, 0.293, 0.698]),
        AccentKind::Dot => (false, 0x5F, [0.085, 0.563, 0.192, 0.669]),
        AccentKind::Ddot => (false, 0x7F, [0.103, 0.569, 0.396, 0.669]),
        AccentKind::Breve => (false, 0x15, [0.100, 0.522, 0.399, 0.694]),
        AccentKind::Bar => (false, 0x16, [0.069, 0.559, 0.430, 0.590]),
        AccentKind::Vec => (true, 0x7E, [0.182, 0.516, 0.625, 0.714]),
        AccentKind::Ring => (false, 0x17, [0.279, 0.541, 0.470, 0.716]),
        _ => (false, 0x5E, [0.116, 0.540, 0.383, 0.694]),
    }
}

/// The cmmi cut TeX sets `style` in: cmmi10, cmmi7 in script style and
/// cmmi5 in scriptscript style.
const fn math_italic_face(style: Style) -> FaceId {
    match style {
        Style::Display | Style::Text => FACE_MATH_ITALIC,
        Style::Script => FACE_MATH_ITALIC_7,
        Style::ScriptScript => FACE_MATH_ITALIC_5,
    }
}

/// The accent's TFM box in the size `style` sets it at (cmr or cmmi at 10,
/// 7 or 5 pt): TeX's char_box, whose width is the character's width plus
/// its italic correction (tex.web §709, 0.5 + 0.1538 em for cmmi10's
/// vector), and the character's height.
fn tex_accent_box(math_italic: bool, code: u8, style: Style) -> (f64, f64) {
    let font = match (math_italic, style) {
        (false, Style::Display | Style::Text) => &crate::tfm::CMR10,
        (false, Style::Script) => &crate::tfm::CMR7,
        (false, Style::ScriptScript) => &crate::tfm::CMR5,
        (true, Style::Display | Style::Text) => &crate::tfm::CMMI10,
        (true, Style::Script) => &crate::tfm::CMMI7,
        (true, Style::ScriptScript) => &crate::tfm::CMMI5,
    };
    font.char(code)
        .map_or((0.5, 0.694_445), |c| (c.width + c.italic, c.height))
}

/// cmsy's prime character from its AFM metrics, in ems of that size's
/// design: cmsy10 in the display and text styles, cmsy7 in script and
/// cmsy5 in scriptscript, the sizes TeX sets. The advance, then the ink's
/// left, bottom, right and top; the top is also the character's height.
const fn tex_prime_metrics(style: Style) -> (f64, [f64; 4]) {
    match style {
        Style::Display | Style::Text => (0.275, [0.029, 0.045, 0.262, 0.559]),
        Style::Script => (0.329, [0.048, 0.041, 0.299, 0.559]),
        Style::ScriptScript => (0.440, [0.093, 0.038, 0.370, 0.559]),
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
    /// Keep the first row's baseline (the Tex surface's align* rows)
    /// instead of `\vcenter`ing the grid on the axis.
    first_baseline: bool,
    /// LaTeX's `array` rows: every row carries `\@arstrut` (height
    /// 0.7\baselineskip, depth 0.3\baselineskip, both times this
    /// `\arraystretch`), and rows stack depth to height with no
    /// interline glue. `None` for the baselineskip-stacked grids.
    array_stretch: Option<f64>,
}

impl Grid {
    /// All-centered columns with the given separation — the matrix baseline.
    fn centered(col_sep: f64) -> Self {
        Self {
            first_baseline: false,
            array_stretch: None,
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

/// `{}` followed by `cell`'s items: amsmath's `{{}##}` right-hand align
/// cell. The empty group is an Ord atom with no ink.
fn opened_by_empty_ord(cell: &Node) -> Node {
    let start = Span {
        start: cell.span.start,
        end: cell.span.start,
    };
    let mut items = vec![Node::new(NodeKind::List(Vec::new()), start)];
    match &cell.kind {
        NodeKind::List(body) => items.extend(body.iter().cloned()),
        _ => items.push(cell.clone()),
    }
    Node::new(NodeKind::List(items), cell.span)
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

/// `boxx` laid under a `\color` declaration: the same dimensions, its
/// primitives coloured unless a nested declaration already coloured them.
fn colored_box(color: Color, boxx: MBox) -> MBox {
    MBox {
        kind: boxx.kind,
        width: boxx.width,
        height: boxx.height,
        depth: boxx.depth,
        children: vec![Positioned {
            dx: 0.0,
            dy: 0.0,
            node: MNode::Colored(color, boxx),
        }],
    }
}

impl MNode {
    fn box_height(&self) -> f64 {
        match self {
            Self::Box(b) | Self::Colored(_, b) => b.height,
            _ => 0.0,
        }
    }
    fn box_depth(&self) -> f64 {
        match self {
            Self::Box(b) | Self::Colored(_, b) => b.depth,
            _ => 0.0,
        }
    }
}

/// A single positioned glyph box.
/// LaTeX's `\check@icr`: `\textit` and `\emph`, in a slanted font, end with
/// their last character's italic correction (`\/`), cmti10's. Running text
/// that goes on with a period or comma (`\nocorrlist`) drops it. In
/// mathematics the correction sits inside the text box, so a math `.` after
/// it does not. A body that ends in a space, or in a character cmti10's OT1
/// table does not share with ASCII, takes none.
fn slanted_text_correction(node: &Node, next: Option<&Node>, ctx: LayCtx) -> Option<f64> {
    let NodeKind::TextStyled {
        style: TextStyle::Italic | TextStyle::Emph,
        body,
    } = &node.kind
    else {
        return None;
    };
    let nocorr = |node: &Node| matches!(&node.kind, NodeKind::TextRun { text, .. } if text.starts_with(['.', ',']));
    if next.is_some_and(nocorr) {
        return None;
    }
    let last = last_text_char(body)?;
    if !last.is_ascii_alphanumeric() {
        return None;
    }
    let italic = crate::tfm::CMTI10.char(u8::try_from(last).ok()?)?.italic;
    (italic > 0.0).then(|| italic * ctx.size())
}

/// The last character a text body sets, if the body ends in one.
fn last_text_char(nodes: &[Node]) -> Option<char> {
    match &nodes.last()?.kind {
        NodeKind::TextRun { text, .. } => text.chars().last(),
        NodeKind::Symbol { ch, .. } => Some(*ch),
        NodeKind::List(items)
        | NodeKind::Text { body: items }
        | NodeKind::TextStyled { body: items, .. } => last_text_char(items),
        _ => None,
    }
}

/// `\hspace`'s glue in layout units (ems of the 10 pt design size). A fixed
/// length stays fixed under size declarations and math styles, as in TeX.
/// Ems and exes are the text font's, cmr10's quad and x-height at the
/// declared size, the font `\hspace` reads in math mode too.
fn skip_width(length: Length, ctx: LayCtx) -> f64 {
    /// A point is a tenth of the 10 pt design size's em.
    const POINT: f64 = 0.1;
    #[allow(clippy::cast_precision_loss)]
    let value = length.sp as f64 / 65536.0;
    match length.unit {
        LengthUnit::Fixed => value * POINT,
        LengthUnit::Em => value * crate::tfm::CMR10.param(6) * ctx.decl_size,
        LengthUnit::Ex => value * crate::tfm::CMR10.param(5) * ctx.decl_size,
    }
}

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
