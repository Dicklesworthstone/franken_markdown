//! The command registry: every control word and environment the tier-1
//! surface knows, each with its parse behavior, plus the *known tier-2*
//! vocabulary so unsupported constructs fail as precise, named, tier-tagged
//! errors (the coverage ratchet's unit — G0-4's normative counting rules).
//!
//! The tier-1 surface is the §11.4 seed vocabulary united with the G0-4
//! rank extension (`construct_table.tsv` in franken_manim); the two
//! T1-by-rank preamble commands the Reference's default template defines —
//! `\minus` and `\mathds` — are built in here as the seed of the default
//! preamble pack (generalized macro packs land with their own bead).

use crate::atom::AtomClass;
use crate::node::{
    AccentKind, DelimSize, FracSpec, MathFont, PhantomKind, SpaceKind, StackKind, TextStyle,
};
use crate::style::Style;

/// Where unsupported-but-known constructs are tracked.
pub const TIER2_TRACKING: &str = "franken_manim fm-j5t (the tier-2 construct program)";

/// Where unknown (untiered) constructs should be reported.
pub const UNTIERED_TRACKING: &str = "https://github.com/Dicklesworthstone/franken_manim/issues";

/// Where parse-supported constructs whose layout has not landed yet are
/// tracked (environments, drawn/stretchy constructions, macro packs).
pub const LAYOUT_PENDING_TRACKING: &str = "franken_manim fm-kg9 (the fmd-math extensions bead)";

/// Construct tiers, per the G0-4 harvest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Tier 1: the shipped surface (§11.4 seed ∪ rank extension).
    T1,
    /// Tier 2: the observed long tail, scheduled by corpus rank.
    T2,
}

/// Parse-level support status of a construct, queryable in the construct
/// table's own naming scheme (`\frac`, `env:array`, `script:sup`,
/// `char:U+00F6`, …). This is what the coverage ratchet and the corpus
/// goldens consume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstructStatus {
    /// The parser understands it end-to-end.
    Supported,
    /// Known tier-2 vocabulary: parsing fails with a precise, tier-tagged
    /// error.
    UnsupportedT2,
    /// Not in any tier: parsing fails with a precise, named error pointing
    /// at the issue tracker.
    Unknown,
}

/// What the parser should do with a control word.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cmd {
    /// A character symbol with an intrinsic class (and whether it may
    /// follow `\left`/`\right`/`\big`).
    Sym {
        ch: char,
        class: AtomClass,
        delim: bool,
    },
    /// A big operator.
    BigOp { ch: char, integral: bool },
    /// A predefined roman operator name (`limits` marks the `\lim` class).
    OpName {
        rendered: &'static str,
        limits: bool,
    },
    /// `\operatorname{…}`.
    OperatorName,
    /// An accent command.
    Accent(AccentKind),
    /// A two-argument fraction command.
    Frac(FracSpec),
    /// An infix fraction command (`\over`, `\choose`) splitting the
    /// enclosing group.
    OverInfix(FracSpec),
    /// `\sqrt`.
    Radical,
    /// `\text`.
    Text,
    /// `\textbf`/`\emph`/`\underline`-in-text.
    TextStyled(TextStyle),
    /// A math-alphabet command.
    Alphabet(MathFont),
    /// A style primitive (`\displaystyle` …).
    StyleSwitch(Style),
    /// A named spacing command (`\quad` …).
    Spacing(SpaceKind),
    /// A phantom command.
    Phantom(PhantomKind),
    /// `\stackrel`/`\overset`/`\underset`.
    Stack(StackKind),
    /// `\color{…}`.
    Color,
    /// A `\big`-family fixed-size delimiter command; `class` is imposed by
    /// the l/r/m variant, `None` (⇒ Ord) for the plain form.
    SizedDelim {
        size: DelimSize,
        class: Option<AtomClass>,
    },
    /// `\left`.
    Left,
    /// `\right`.
    Right,
    /// `\limits`.
    Limits,
    /// `\nolimits`.
    NoLimits,
    /// `\begin`.
    Begin,
    /// `\end`.
    End,
    /// Known tier-2 vocabulary: fail precisely.
    UnsupportedT2,
}

/// An environment the grammar knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EnvDef {
    /// Takes a braced column-spec argument (`array`).
    pub(crate) has_spec: bool,
}

const fn sym(ch: char, class: AtomClass) -> Cmd {
    Cmd::Sym {
        ch,
        class,
        delim: false,
    }
}

const fn delim_sym(ch: char, class: AtomClass) -> Cmd {
    Cmd::Sym {
        ch,
        class,
        delim: true,
    }
}

/// Look up a control word (name without the backslash).
#[allow(clippy::too_many_lines)]
pub(crate) fn lookup(name: &str) -> Option<Cmd> {
    use AtomClass::{Bin, Close, Inner, Open, Ord, Punct, Rel};
    Some(match name {
        // ── Greek, lowercase ────────────────────────────────────────────
        "alpha" => sym('α', Ord),
        "beta" => sym('β', Ord),
        "gamma" => sym('γ', Ord),
        "delta" => sym('δ', Ord),
        "epsilon" => sym('ϵ', Ord),
        "varepsilon" => sym('ε', Ord),
        "zeta" => sym('ζ', Ord),
        "eta" => sym('η', Ord),
        "theta" => sym('θ', Ord),
        "vartheta" => sym('ϑ', Ord),
        "iota" => sym('ι', Ord),
        "kappa" => sym('κ', Ord),
        "lambda" => sym('λ', Ord),
        "mu" => sym('μ', Ord),
        "nu" => sym('ν', Ord),
        "xi" => sym('ξ', Ord),
        "pi" => sym('π', Ord),
        "varpi" => sym('ϖ', Ord),
        "rho" => sym('ρ', Ord),
        "varrho" => sym('ϱ', Ord),
        "sigma" => sym('σ', Ord),
        "varsigma" => sym('ς', Ord),
        "tau" => sym('τ', Ord),
        "upsilon" => sym('υ', Ord),
        "phi" => sym('ϕ', Ord),
        "varphi" => sym('φ', Ord),
        "chi" => sym('χ', Ord),
        "psi" => sym('ψ', Ord),
        "omega" => sym('ω', Ord),
        // ── Greek, uppercase ────────────────────────────────────────────
        "Gamma" => sym('Γ', Ord),
        "Delta" => sym('Δ', Ord),
        "Theta" => sym('Θ', Ord),
        "Lambda" => sym('Λ', Ord),
        "Xi" => sym('Ξ', Ord),
        "Pi" => sym('Π', Ord),
        "Sigma" => sym('Σ', Ord),
        "Upsilon" => sym('ϒ', Ord),
        "Phi" => sym('Φ', Ord),
        "Psi" => sym('Ψ', Ord),
        "Omega" => sym('Ω', Ord),
        // ── Ordinary symbols ────────────────────────────────────────────
        "infty" => sym('∞', Ord),
        "partial" => sym('∂', Ord),
        "nabla" => sym('∇', Ord),
        "hbar" => sym('ℏ', Ord),
        "ell" => sym('ℓ', Ord),
        "wp" => sym('℘', Ord),
        "Re" => sym('ℜ', Ord),
        "Im" => sym('ℑ', Ord),
        "aleph" => sym('ℵ', Ord),
        "imath" => sym('ı', Ord),
        "jmath" => sym('ȷ', Ord),
        "emptyset" => sym('∅', Ord),
        "varnothing" => sym('⌀', Ord),
        "exists" => sym('∃', Ord),
        "forall" => sym('∀', Ord),
        "neg" | "lnot" => sym('¬', Ord),
        "prime" => sym('′', Ord),
        "angle" => sym('∠', Ord),
        "triangle" => sym('△', Ord),
        "checkmark" => sym('✓', Ord),
        "top" => sym('⊤', Ord),
        "bot" => sym('⊥', Ord),
        "flat" => sym('♭', Ord),
        "natural" => sym('♮', Ord),
        "sharp" => sym('♯', Ord),
        "clubsuit" => sym('♣', Ord),
        "diamondsuit" => sym('♢', Ord),
        "heartsuit" => sym('♡', Ord),
        "spadesuit" => sym('♠', Ord),
        "backslash" => delim_sym('\\', Ord),
        "vert" => delim_sym('|', Ord),
        "Vert" => delim_sym('‖', Ord),
        // Dots. `\ldots`-class dots are Inner atoms in TeX (\mathinner);
        // `\vdots` is a plain Ord box.
        "ldots" | "dots" | "dotsc" | "dotso" => sym('…', Inner),
        "cdots" | "dotsb" | "hdots" => sym('⋯', Inner),
        "vdots" => sym('⋮', Ord),
        "ddots" => sym('⋱', Inner),
        // ── Binary operations ───────────────────────────────────────────
        "pm" => sym('±', Bin),
        "mp" => sym('∓', Bin),
        "cdot" => sym('⋅', Bin),
        "times" => sym('×', Bin),
        "div" => sym('÷', Bin),
        "ast" => sym('∗', Bin),
        "star" => sym('⋆', Bin),
        "circ" => sym('∘', Bin),
        "bullet" => sym('∙', Bin),
        "oplus" => sym('⊕', Bin),
        "ominus" => sym('⊖', Bin),
        "otimes" => sym('⊗', Bin),
        "oslash" => sym('⊘', Bin),
        "odot" => sym('⊙', Bin),
        "cup" => sym('∪', Bin),
        "cap" => sym('∩', Bin),
        "sqcup" => sym('⊔', Bin),
        "sqcap" => sym('⊓', Bin),
        "vee" | "lor" => sym('∨', Bin),
        "wedge" | "land" => sym('∧', Bin),
        "setminus" | "smallsetminus" => sym('∖', Bin),
        "wr" => sym('≀', Bin),
        "diamond" => sym('⋄', Bin),
        "triangleleft" => sym('◁', Bin),
        "triangleright" => sym('▷', Bin),
        "uplus" => sym('⊎', Bin),
        "amalg" => sym('⨿', Bin),
        "dagger" => sym('†', Bin),
        "ddagger" => sym('‡', Bin),
        // The Reference's default template defines `\minus` (T1 by rank);
        // the built-in default pack maps it to a binary minus sign.
        "minus" => sym('−', Bin),
        // ── Relations ───────────────────────────────────────────────────
        "le" | "leq" => sym('≤', Rel),
        "ge" | "geq" => sym('≥', Rel),
        "ne" | "neq" => sym('≠', Rel),
        "equiv" => sym('≡', Rel),
        "approx" => sym('≈', Rel),
        "sim" => sym('∼', Rel),
        "simeq" => sym('≃', Rel),
        "cong" => sym('≅', Rel),
        "doteq" => sym('≐', Rel),
        "propto" => sym('∝', Rel),
        "perp" => sym('⊥', Rel),
        "mid" => sym('∣', Rel),
        "parallel" => sym('∥', Rel),
        "in" => sym('∈', Rel),
        "ni" => sym('∋', Rel),
        "notin" => sym('∉', Rel),
        "subset" => sym('⊂', Rel),
        "supset" => sym('⊃', Rel),
        "subseteq" => sym('⊆', Rel),
        "supseteq" => sym('⊇', Rel),
        "sqsubseteq" => sym('⊑', Rel),
        "sqsupseteq" => sym('⊒', Rel),
        "ll" => sym('≪', Rel),
        "gg" => sym('≫', Rel),
        "prec" => sym('≺', Rel),
        "succ" => sym('≻', Rel),
        "preceq" => sym('⪯', Rel),
        "succeq" => sym('⪰', Rel),
        "asymp" => sym('≍', Rel),
        "bowtie" => sym('⋈', Rel),
        "models" => sym('⊨', Rel),
        "vdash" => sym('⊢', Rel),
        "dashv" => sym('⊣', Rel),
        "smile" => sym('⌣', Rel),
        "frown" => sym('⌢', Rel),
        "colon" => sym(':', Punct),
        // ── Arrows (relations; the vertical ones are delimiters too) ────
        "rightarrow" | "to" => sym('→', Rel),
        "leftarrow" | "gets" => sym('←', Rel),
        "leftrightarrow" => sym('↔', Rel),
        "Rightarrow" => sym('⇒', Rel),
        "Leftarrow" => sym('⇐', Rel),
        "Leftrightarrow" => sym('⇔', Rel),
        "longrightarrow" => sym('⟶', Rel),
        "longleftarrow" => sym('⟵', Rel),
        "longleftrightarrow" => sym('⟷', Rel),
        "Longrightarrow" | "implies" => sym('⟹', Rel),
        "Longleftarrow" | "impliedby" => sym('⟸', Rel),
        "Longleftrightarrow" | "iff" => sym('⟺', Rel),
        "mapsto" => sym('↦', Rel),
        "longmapsto" => sym('⟼', Rel),
        "uparrow" => delim_sym('↑', Rel),
        "downarrow" => delim_sym('↓', Rel),
        "updownarrow" => delim_sym('↕', Rel),
        "Uparrow" => delim_sym('⇑', Rel),
        "Downarrow" => delim_sym('⇓', Rel),
        "Updownarrow" => delim_sym('⇕', Rel),
        "nearrow" => sym('↗', Rel),
        "searrow" => sym('↘', Rel),
        "swarrow" => sym('↙', Rel),
        "nwarrow" => sym('↖', Rel),
        "hookrightarrow" => sym('↪', Rel),
        "hookleftarrow" => sym('↩', Rel),
        "rightharpoonup" => sym('⇀', Rel),
        "rightharpoondown" => sym('⇁', Rel),
        "leftharpoonup" => sym('↼', Rel),
        "leftharpoondown" => sym('↽', Rel),
        "rightleftharpoons" => sym('⇌', Rel),
        // ── Delimiter symbols ───────────────────────────────────────────
        "langle" => delim_sym('⟨', Open),
        "rangle" => delim_sym('⟩', Close),
        "lceil" => delim_sym('⌈', Open),
        "rceil" => delim_sym('⌉', Close),
        "lfloor" => delim_sym('⌊', Open),
        "rfloor" => delim_sym('⌋', Close),
        "lbrace" => delim_sym('{', Open),
        "rbrace" => delim_sym('}', Close),
        "lbrack" => delim_sym('[', Open),
        "rbrack" => delim_sym(']', Close),
        // ── Big operators ───────────────────────────────────────────────
        "sum" => Cmd::BigOp {
            ch: '∑',
            integral: false,
        },
        "prod" => Cmd::BigOp {
            ch: '∏',
            integral: false,
        },
        "coprod" => Cmd::BigOp {
            ch: '∐',
            integral: false,
        },
        "int" => Cmd::BigOp {
            ch: '∫',
            integral: true,
        },
        "oint" => Cmd::BigOp {
            ch: '∮',
            integral: true,
        },
        "iint" => Cmd::BigOp {
            ch: '∬',
            integral: true,
        },
        "iiint" => Cmd::BigOp {
            ch: '∭',
            integral: true,
        },
        "bigcup" => Cmd::BigOp {
            ch: '⋃',
            integral: false,
        },
        "bigcap" => Cmd::BigOp {
            ch: '⋂',
            integral: false,
        },
        "bigvee" => Cmd::BigOp {
            ch: '⋁',
            integral: false,
        },
        "bigwedge" => Cmd::BigOp {
            ch: '⋀',
            integral: false,
        },
        "bigoplus" => Cmd::BigOp {
            ch: '⨁',
            integral: false,
        },
        "bigotimes" => Cmd::BigOp {
            ch: '⨂',
            integral: false,
        },
        "bigodot" => Cmd::BigOp {
            ch: '⨀',
            integral: false,
        },
        "biguplus" => Cmd::BigOp {
            ch: '⨄',
            integral: false,
        },
        "bigsqcup" => Cmd::BigOp {
            ch: '⨆',
            integral: false,
        },
        // ── Operator names ──────────────────────────────────────────────
        "sin" => Cmd::OpName {
            rendered: "sin",
            limits: false,
        },
        "cos" => Cmd::OpName {
            rendered: "cos",
            limits: false,
        },
        "tan" => Cmd::OpName {
            rendered: "tan",
            limits: false,
        },
        "cot" => Cmd::OpName {
            rendered: "cot",
            limits: false,
        },
        "sec" => Cmd::OpName {
            rendered: "sec",
            limits: false,
        },
        "csc" => Cmd::OpName {
            rendered: "csc",
            limits: false,
        },
        "arcsin" => Cmd::OpName {
            rendered: "arcsin",
            limits: false,
        },
        "arccos" => Cmd::OpName {
            rendered: "arccos",
            limits: false,
        },
        "arctan" => Cmd::OpName {
            rendered: "arctan",
            limits: false,
        },
        "sinh" => Cmd::OpName {
            rendered: "sinh",
            limits: false,
        },
        "cosh" => Cmd::OpName {
            rendered: "cosh",
            limits: false,
        },
        "tanh" => Cmd::OpName {
            rendered: "tanh",
            limits: false,
        },
        "coth" => Cmd::OpName {
            rendered: "coth",
            limits: false,
        },
        "exp" => Cmd::OpName {
            rendered: "exp",
            limits: false,
        },
        "log" => Cmd::OpName {
            rendered: "log",
            limits: false,
        },
        "lg" => Cmd::OpName {
            rendered: "lg",
            limits: false,
        },
        "ln" => Cmd::OpName {
            rendered: "ln",
            limits: false,
        },
        "arg" => Cmd::OpName {
            rendered: "arg",
            limits: false,
        },
        "deg" => Cmd::OpName {
            rendered: "deg",
            limits: false,
        },
        "dim" => Cmd::OpName {
            rendered: "dim",
            limits: false,
        },
        "hom" => Cmd::OpName {
            rendered: "hom",
            limits: false,
        },
        "ker" => Cmd::OpName {
            rendered: "ker",
            limits: false,
        },
        // `\mod`/`\bmod` carry their own leading-space quirk; the layout
        // bead refines the spacing, the parse surface is an operator name.
        "mod" | "bmod" => Cmd::OpName {
            rendered: "mod",
            limits: false,
        },
        "lim" => Cmd::OpName {
            rendered: "lim",
            limits: true,
        },
        "limsup" => Cmd::OpName {
            rendered: "lim sup",
            limits: true,
        },
        "liminf" => Cmd::OpName {
            rendered: "lim inf",
            limits: true,
        },
        "max" => Cmd::OpName {
            rendered: "max",
            limits: true,
        },
        "min" => Cmd::OpName {
            rendered: "min",
            limits: true,
        },
        "sup" => Cmd::OpName {
            rendered: "sup",
            limits: true,
        },
        "inf" => Cmd::OpName {
            rendered: "inf",
            limits: true,
        },
        "det" => Cmd::OpName {
            rendered: "det",
            limits: true,
        },
        "gcd" => Cmd::OpName {
            rendered: "gcd",
            limits: true,
        },
        "Pr" => Cmd::OpName {
            rendered: "Pr",
            limits: true,
        },
        "operatorname" => Cmd::OperatorName,
        // ── Accents ─────────────────────────────────────────────────────
        "hat" => Cmd::Accent(AccentKind::Hat),
        "check" => Cmd::Accent(AccentKind::Check),
        "tilde" => Cmd::Accent(AccentKind::Tilde),
        "acute" => Cmd::Accent(AccentKind::Acute),
        "grave" => Cmd::Accent(AccentKind::Grave),
        "dot" => Cmd::Accent(AccentKind::Dot),
        "ddot" => Cmd::Accent(AccentKind::Ddot),
        "breve" => Cmd::Accent(AccentKind::Breve),
        "bar" => Cmd::Accent(AccentKind::Bar),
        "vec" => Cmd::Accent(AccentKind::Vec),
        "mathring" => Cmd::Accent(AccentKind::Ring),
        "widehat" => Cmd::Accent(AccentKind::WideHat),
        "widetilde" => Cmd::Accent(AccentKind::WideTilde),
        "overline" => Cmd::Accent(AccentKind::OverLine),
        "overbrace" => Cmd::Accent(AccentKind::OverBrace),
        "underbrace" => Cmd::Accent(AccentKind::UnderBrace),
        "overrightarrow" => Cmd::Accent(AccentKind::OverRightArrow),
        "overleftarrow" => Cmd::Accent(AccentKind::OverLeftArrow),
        // `\underline` is mode-dependent: a math accent here, a text
        // styling island in text mode (the parser dispatches by mode).
        "underline" => Cmd::Accent(AccentKind::UnderLine),
        // ── Fractions ───────────────────────────────────────────────────
        "frac" => Cmd::Frac(FracSpec {
            bar: true,
            delims: None,
            forced_style: None,
        }),
        "dfrac" => Cmd::Frac(FracSpec {
            bar: true,
            delims: None,
            forced_style: Some(Style::Display),
        }),
        "tfrac" => Cmd::Frac(FracSpec {
            bar: true,
            delims: None,
            forced_style: Some(Style::Text),
        }),
        "binom" => Cmd::Frac(FracSpec {
            bar: false,
            delims: Some(('(', ')')),
            forced_style: None,
        }),
        "over" => Cmd::OverInfix(FracSpec {
            bar: true,
            delims: None,
            forced_style: None,
        }),
        "choose" => Cmd::OverInfix(FracSpec {
            bar: false,
            delims: Some(('(', ')')),
            forced_style: None,
        }),
        "sqrt" => Cmd::Radical,
        // ── Text islands and math alphabets ─────────────────────────────
        "text" => Cmd::Text,
        "textbf" => Cmd::TextStyled(TextStyle::Bold),
        "emph" => Cmd::TextStyled(TextStyle::Emph),
        "mathbb" => Cmd::Alphabet(MathFont::Blackboard),
        // The default preamble pack maps the Reference template's dsfont
        // `\mathds` to blackboard (T1 by rank, G0-4).
        "mathds" => Cmd::Alphabet(MathFont::Blackboard),
        "mathcal" => Cmd::Alphabet(MathFont::Calligraphic),
        "mathrm" => Cmd::Alphabet(MathFont::Roman),
        "mathbf" => Cmd::Alphabet(MathFont::Bold),
        "boldsymbol" => Cmd::Alphabet(MathFont::BoldItalic),
        "mathsf" => Cmd::Alphabet(MathFont::SansSerif),
        "mathtt" => Cmd::Alphabet(MathFont::Typewriter),
        "mathit" => Cmd::Alphabet(MathFont::Italic),
        // ── Styles, spacing, phantoms, stacks, color ────────────────────
        "displaystyle" => Cmd::StyleSwitch(Style::Display),
        "textstyle" => Cmd::StyleSwitch(Style::Text),
        "scriptstyle" => Cmd::StyleSwitch(Style::Script),
        "scriptscriptstyle" => Cmd::StyleSwitch(Style::ScriptScript),
        "quad" => Cmd::Spacing(SpaceKind::Quad),
        "qquad" => Cmd::Spacing(SpaceKind::Qquad),
        "phantom" => Cmd::Phantom(PhantomKind::Full),
        "hphantom" => Cmd::Phantom(PhantomKind::Horizontal),
        "vphantom" => Cmd::Phantom(PhantomKind::Vertical),
        "stackrel" => Cmd::Stack(StackKind::Stackrel),
        "overset" => Cmd::Stack(StackKind::Overset),
        "underset" => Cmd::Stack(StackKind::Underset),
        "color" => Cmd::Color,
        // ── Sized delimiters ────────────────────────────────────────────
        "big" => Cmd::SizedDelim {
            size: DelimSize::Big,
            class: None,
        },
        "Big" => Cmd::SizedDelim {
            size: DelimSize::BBig,
            class: None,
        },
        "bigg" => Cmd::SizedDelim {
            size: DelimSize::Bigg,
            class: None,
        },
        "Bigg" => Cmd::SizedDelim {
            size: DelimSize::BBigg,
            class: None,
        },
        "bigl" => Cmd::SizedDelim {
            size: DelimSize::Big,
            class: Some(AtomClass::Open),
        },
        "Bigl" => Cmd::SizedDelim {
            size: DelimSize::BBig,
            class: Some(AtomClass::Open),
        },
        "biggl" => Cmd::SizedDelim {
            size: DelimSize::Bigg,
            class: Some(AtomClass::Open),
        },
        "Biggl" => Cmd::SizedDelim {
            size: DelimSize::BBigg,
            class: Some(AtomClass::Open),
        },
        "bigr" => Cmd::SizedDelim {
            size: DelimSize::Big,
            class: Some(AtomClass::Close),
        },
        "Bigr" => Cmd::SizedDelim {
            size: DelimSize::BBig,
            class: Some(AtomClass::Close),
        },
        "biggr" => Cmd::SizedDelim {
            size: DelimSize::Bigg,
            class: Some(AtomClass::Close),
        },
        "Biggr" => Cmd::SizedDelim {
            size: DelimSize::BBigg,
            class: Some(AtomClass::Close),
        },
        "bigm" => Cmd::SizedDelim {
            size: DelimSize::Big,
            class: Some(AtomClass::Rel),
        },
        "Bigm" => Cmd::SizedDelim {
            size: DelimSize::BBig,
            class: Some(AtomClass::Rel),
        },
        "biggm" => Cmd::SizedDelim {
            size: DelimSize::Bigg,
            class: Some(AtomClass::Rel),
        },
        "Biggm" => Cmd::SizedDelim {
            size: DelimSize::BBigg,
            class: Some(AtomClass::Rel),
        },
        // ── Structure ───────────────────────────────────────────────────
        "left" => Cmd::Left,
        "right" => Cmd::Right,
        "limits" => Cmd::Limits,
        "nolimits" => Cmd::NoLimits,
        "begin" => Cmd::Begin,
        "end" => Cmd::End,
        // ── Known tier-2 vocabulary (G0-4 `construct_table.tsv`) ────────
        "centering" | "female" | "male" | "earth" | "mars" | "small" | "Large" | "large"
        | "huge" | "tiny" | "footnotesize" | "doublespacing" | "substack" | "i" | "j" | "ding"
        | "nmid" | "dx" | "copyright" | "oiint" | "xmapsto" | "xrightarrow"
        | "circlearrowright" | "circlearrowleft" | "dddot" | "ddddot" => Cmd::UnsupportedT2,
        _ => return None,
    })
}

/// Look up an environment name.
pub(crate) fn lookup_env(name: &str) -> Option<EnvDef> {
    match name {
        "array" => Some(EnvDef { has_spec: true }),
        "align" | "align*" | "aligned" | "cases" | "matrix" | "pmatrix" | "bmatrix" | "Bmatrix"
        | "vmatrix" | "Vmatrix" | "smallmatrix" => Some(EnvDef { has_spec: false }),
        _ => None,
    }
}

/// True when the environment is known tier-2 vocabulary.
pub(crate) fn env_is_t2(name: &str) -> bool {
    matches!(name, "flushleft" | "flushright" | "center")
}

/// True when the control *symbol* is known tier-2 vocabulary (the text
/// accents `\'` and `\"`).
pub(crate) fn control_symbol_is_t2(ch: char) -> bool {
    matches!(ch, '\'' | '"')
}

/// The characters that may follow `\left`, `\right`, or a `\big`-class
/// command directly (`.` is the null delimiter, valid there too).
pub(crate) fn char_is_delim(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '|' | '/' | '<' | '>' | '.')
}

/// Parse-level support status for a construct named in the G0-4 construct
/// table's scheme: `\frac` (control word), `\\,` -- i.e. a backslash plus
/// one non-letter char (control symbol), `env:name`, `script:sup`,
/// `script:sub`, `prime`, `tie`, `alignment-tab`, `math-island`, or
/// `char:U+XXXX`.
#[must_use]
pub fn construct_status(construct: &str) -> ConstructStatus {
    // Structural constructs and character coverage are parse-supported by
    // construction.
    match construct {
        "script:sup" | "script:sub" | "prime" | "tie" | "alignment-tab" | "math-island" => {
            return ConstructStatus::Supported;
        }
        _ => {}
    }
    if construct.starts_with("char:U+") {
        return ConstructStatus::Supported;
    }
    if let Some(env) = construct.strip_prefix("env:") {
        if lookup_env(env).is_some() {
            return ConstructStatus::Supported;
        }
        if env_is_t2(env) {
            return ConstructStatus::UnsupportedT2;
        }
        return ConstructStatus::Unknown;
    }
    if let Some(name) = construct.strip_prefix('\\') {
        // Control symbols: a single non-letter character.
        let mut chars = name.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if !c.is_ascii_alphabetic() {
                if control_symbol_is_t2(c) {
                    return ConstructStatus::UnsupportedT2;
                }
                // The supported control-symbol set: escapes, spacing,
                // the line break, and the `\|` delimiter.
                return if matches!(
                    c,
                    '\\' | ','
                        | ':'
                        | ';'
                        | '!'
                        | ' '
                        | '{'
                        | '}'
                        | '%'
                        | '$'
                        | '&'
                        | '#'
                        | '_'
                        | '|'
                ) {
                    ConstructStatus::Supported
                } else {
                    ConstructStatus::Unknown
                };
            }
        }
        // The macro-definition commands are consumed by the token-level
        // expansion pass before the grammar ever runs.
        if matches!(name, "newcommand" | "renewcommand") {
            return ConstructStatus::Supported;
        }
        return match lookup(name) {
            Some(Cmd::UnsupportedT2) => ConstructStatus::UnsupportedT2,
            Some(_) => ConstructStatus::Supported,
            None => ConstructStatus::Unknown,
        };
    }
    ConstructStatus::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_headliners_are_supported() {
        for c in [
            r"\frac",
            r"\over",
            r"\text",
            r"\pi",
            r"\cdot",
            r"\left",
            r"\right",
            r"\sqrt",
            r"\sum",
            r"\mathds",
            r"\minus",
            r"\checkmark",
            r"\textbf",
            r"\emph",
            r"\big",
            r"\,",
            r"\\",
            r"\{",
            "env:array",
            "env:align*",
            "env:cases",
            "env:aligned",
            "script:sup",
            "math-island",
            "prime",
            "alignment-tab",
            "tie",
        ] {
            assert_eq!(
                construct_status(c),
                ConstructStatus::Supported,
                "expected {c} supported"
            );
        }
    }

    #[test]
    fn t2_vocabulary_is_tiered() {
        for c in [
            r"\substack",
            r"\centering",
            r"\small",
            r"\Large",
            r"\ding",
            r"\nmid",
            r"\dx",
            r"\oiint",
            r"\xrightarrow",
            r"\i",
            r"\j",
            r"\'",
            "env:flushleft",
        ] {
            assert_eq!(
                construct_status(c),
                ConstructStatus::UnsupportedT2,
                "expected {c} tier-2"
            );
        }
    }

    #[test]
    fn unknown_vocabulary_is_unknown() {
        assert_eq!(construct_status(r"\notacommand"), ConstructStatus::Unknown);
        assert_eq!(construct_status("env:mystery"), ConstructStatus::Unknown);
    }

    #[test]
    fn char_constructs_are_parse_supported() {
        assert_eq!(construct_status("char:U+00F6"), ConstructStatus::Supported);
    }
}
