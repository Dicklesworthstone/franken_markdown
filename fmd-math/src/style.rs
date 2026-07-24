//! TeX's four math styles, cramping, and the propagation rules.
//!
//! Style propagation is exact TeX (The TeXbook, chapter 17 / appendix G):
//! scripts go D,T → S → SS with subscripts cramped; fraction interiors go
//! D → T → S → SS with denominators cramped; radicands and accent bases are
//! cramped at the current style; radical indices are set in scriptscript
//! style unconditionally. Glyph sizes follow CM's 10/7/5 pt family:
//! 1.0 / 0.7 / 0.5.
//!
//! [`style_walk`] is the *normative* propagation definition: the layout
//! stages consume the same walk, and the style-propagation fixtures lock it.

use crate::node::{Node, NodeKind, StackKind};

/// The four math styles (cramped variants ride [`StyleCtx`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Style {
    /// Display style.
    Display,
    /// Text (inline) style.
    Text,
    /// Script style (first-order scripts).
    Script,
    /// Scriptscript style (everything deeper).
    ScriptScript,
}

impl Style {
    /// True for the script styles, where medium/thick inter-atom spaces are
    /// suppressed and `\sum`-class operators stop taking display limits.
    #[must_use]
    pub const fn is_script(self) -> bool {
        matches!(self, Self::Script | Self::ScriptScript)
    }

    /// The glyph-size factor of the style relative to text size (CM's
    /// 10 pt / 7 pt / 5 pt family).
    #[must_use]
    pub const fn size_factor(self) -> f64 {
        match self {
            Self::Display | Self::Text => 1.0,
            Self::Script => 0.7,
            Self::ScriptScript => 0.5,
        }
    }

    /// The style of a superscript on an atom in `self`.
    #[must_use]
    pub const fn sup(self) -> Self {
        match self {
            Self::Display | Self::Text => Self::Script,
            Self::Script | Self::ScriptScript => Self::ScriptScript,
        }
    }

    /// The style of a fraction numerator in `self`.
    #[must_use]
    pub const fn num(self) -> Self {
        match self {
            Self::Display => Self::Text,
            Self::Text => Self::Script,
            Self::Script | Self::ScriptScript => Self::ScriptScript,
        }
    }
}

/// A style with its cramping state: the full layout context TeX threads
/// top-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StyleCtx {
    /// The current style.
    pub style: Style,
    /// Cramped: superscripts are lowered (interiors of denominators,
    /// subscripts, radicands, accent bases, …).
    pub cramped: bool,
}

impl StyleCtx {
    /// An uncramped context in the given style.
    #[must_use]
    pub const fn new(style: Style) -> Self {
        Self {
            style,
            cramped: false,
        }
    }

    /// Display, uncramped: the default whole-formula context.
    #[must_use]
    pub const fn display() -> Self {
        Self::new(Style::Display)
    }

    /// The context of a superscript: style goes up one script level,
    /// cramping is preserved.
    #[must_use]
    pub const fn sup(self) -> Self {
        Self {
            style: self.style.sup(),
            cramped: self.cramped,
        }
    }

    /// The context of a subscript: like [`Self::sup`] but always cramped.
    #[must_use]
    pub const fn sub(self) -> Self {
        Self {
            style: self.style.sup(),
            cramped: true,
        }
    }

    /// The context of a fraction numerator: style goes down one fraction
    /// level, cramping preserved.
    #[must_use]
    pub const fn num(self) -> Self {
        Self {
            style: self.style.num(),
            cramped: self.cramped,
        }
    }

    /// The context of a fraction denominator: like [`Self::num`] but always
    /// cramped.
    #[must_use]
    pub const fn den(self) -> Self {
        Self {
            style: self.style.num(),
            cramped: true,
        }
    }

    /// The same style, cramped (radicands, accent bases).
    #[must_use]
    pub const fn cramp(self) -> Self {
        Self {
            style: self.style,
            cramped: true,
        }
    }

    /// Glyph-size factor of the current style.
    #[must_use]
    pub const fn size_factor(self) -> f64 {
        self.style.size_factor()
    }
}

/// Walk `node` in pre-order, calling `visit` on every node with the style
/// context it is laid out in. This is the normative propagation definition:
///
/// - scripts: base at the current context, superscript at [`StyleCtx::sup`],
///   subscript at [`StyleCtx::sub`];
/// - fractions: `\dfrac`/`\tfrac` force the fraction's own effective style
///   first; numerator at `num`, denominator at `den` of the effective
///   context;
/// - radicals: radicand cramped at the current style, index in scriptscript
///   style (rule 11);
/// - accents: base cramped;
/// - `\stackrel`/`\overset` annotations at `sup`, `\underset` at `sub`;
/// - `\displaystyle`-class markers restyle the remainder of their enclosing
///   list, preserving cramping;
/// - `$…$` islands inside text are inline mathematics: they enter at text
///   style, uncramped;
/// - environment cells enter at text style (`array`/`matrix`-class and
///   `cases` set `\textstyle`), except the `align`-class environments,
///   whose cells keep the ambient context;
/// - everything else (groups, phantoms, math-font arguments, `\left…\right`
///   bodies, `\text` islands) inherits the current context.
pub fn style_walk<'a, F>(node: &'a Node, ctx: StyleCtx, visit: &mut F)
where
    F: FnMut(&'a Node, StyleCtx),
{
    visit(node, ctx);
    match &node.kind {
        NodeKind::List(items) => walk_list(items, ctx, visit),
        NodeKind::Scripts { base, sub, sup, .. } => {
            if let Some(b) = base {
                style_walk(b, ctx, visit);
            }
            if let Some(s) = sup {
                style_walk(s, ctx.sup(), visit);
            }
            if let Some(s) = sub {
                style_walk(s, ctx.sub(), visit);
            }
        }
        NodeKind::Frac { num, den, spec } => {
            let eff = match spec.forced_style {
                Some(forced) => StyleCtx {
                    style: forced,
                    cramped: ctx.cramped,
                },
                None => ctx,
            };
            style_walk(num, eff.num(), visit);
            style_walk(den, eff.den(), visit);
        }
        NodeKind::Radical { index, radicand } => {
            if let Some(ix) = index {
                style_walk(
                    ix,
                    StyleCtx {
                        style: Style::ScriptScript,
                        cramped: ctx.cramped,
                    },
                    visit,
                );
            }
            style_walk(radicand, ctx.cramp(), visit);
        }
        NodeKind::Accent { base, .. } => style_walk(base, ctx.cramp(), visit),
        NodeKind::LeftRight { body, .. } => walk_list(body, ctx, visit),
        NodeKind::Text { body } | NodeKind::TextStyled { body, .. } => {
            walk_list(body, ctx, visit);
        }
        NodeKind::MathIsland { body, display } => {
            let style = if *display {
                Style::Display
            } else {
                Style::Text
            };
            walk_list(body, StyleCtx::new(style), visit);
        }
        NodeKind::MathFont { body, .. } | NodeKind::Phantom { body, .. } => {
            style_walk(body, ctx, visit);
        }
        NodeKind::Stack {
            kind,
            annotation,
            base,
        } => {
            let ann_ctx = match kind {
                StackKind::Underset => ctx.sub(),
                StackKind::Stackrel | StackKind::Overset => ctx.sup(),
            };
            style_walk(annotation, ann_ctx, visit);
            style_walk(base, ctx, visit);
        }
        NodeKind::Environment { name, rows, .. } => {
            let cell_ctx = if name.starts_with("align") {
                ctx
            } else {
                StyleCtx::new(Style::Text)
            };
            for row in rows {
                for cell in row {
                    style_walk(cell, cell_ctx, visit);
                }
            }
        }
        NodeKind::Symbol { .. }
        | NodeKind::BigOp { .. }
        | NodeKind::OpName { .. }
        | NodeKind::SizedDelim { .. }
        | NodeKind::TextRun { .. }
        | NodeKind::StyleChange(_)
        | NodeKind::ColorChange(_)
        | NodeKind::Space(_)
        | NodeKind::Tie
        | NodeKind::Linebreak
        | NodeKind::AlignTab
        | NodeKind::Fragment(_) => {}
    }
}

/// Walk a horizontal list, honoring `\displaystyle`-class markers: a
/// [`NodeKind::StyleChange`] restyles the remainder of the list (cramping
/// preserved), exactly like TeX's style primitives.
fn walk_list<'a, F>(items: &'a [Node], mut ctx: StyleCtx, visit: &mut F)
where
    F: FnMut(&'a Node, StyleCtx),
{
    for item in items {
        if let NodeKind::StyleChange(style) = &item.kind {
            visit(item, ctx);
            ctx = StyleCtx {
                style: *style,
                cramped: ctx.cramped,
            };
            continue;
        }
        style_walk(item, ctx, visit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sup_chain() {
        assert_eq!(Style::Display.sup(), Style::Script);
        assert_eq!(Style::Text.sup(), Style::Script);
        assert_eq!(Style::Script.sup(), Style::ScriptScript);
        assert_eq!(Style::ScriptScript.sup(), Style::ScriptScript);
    }

    #[test]
    fn num_chain() {
        assert_eq!(Style::Display.num(), Style::Text);
        assert_eq!(Style::Text.num(), Style::Script);
        assert_eq!(Style::Script.num(), Style::ScriptScript);
        assert_eq!(Style::ScriptScript.num(), Style::ScriptScript);
    }

    #[test]
    fn sub_is_cramped_sup() {
        let ctx = StyleCtx::display();
        assert_eq!(ctx.sub().style, Style::Script);
        assert!(ctx.sub().cramped);
        assert!(!ctx.sup().cramped);
    }

    #[test]
    fn den_is_cramped_num() {
        let ctx = StyleCtx::new(Style::Text);
        assert_eq!(ctx.den().style, Style::Script);
        assert!(ctx.den().cramped);
    }

    #[test]
    fn size_factors_are_cm_10_7_5() {
        assert_eq!(Style::Display.size_factor(), 1.0);
        assert_eq!(Style::Text.size_factor(), 1.0);
        assert_eq!(Style::Script.size_factor(), 0.7);
        assert_eq!(Style::ScriptScript.size_factor(), 0.5);
    }
}
