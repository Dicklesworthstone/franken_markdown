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
    AccentKind, AmsFamily, CancelKind, DelimSize, FracSpec, LineAlign, LogoKind, MathFont,
    PhantomKind, SpaceKind, StackKind, TextStyle,
};
use crate::style::Style;

/// Where unsupported-but-known constructs are tracked.
pub const TIER2_TRACKING: &str = "franken_manim fm-j5t (the tier-2 construct program)";

/// Where the common constructs outside the G0-4 corpus are tracked.
pub const OUTSIDE_CORPUS_TRACKING: &str =
    "franken_manim fm-tex-constructs-t2-1wfq (common constructs outside the G0-4 corpus)";

/// Where a tier-2 construct is tracked. `\dx` stays with the corpus
/// program (its one corpus occurrence is Reference-broken, a standing
/// refusal); every other one is a construct outside the corpus.
#[must_use]
pub fn tier2_tracking(construct: &str) -> &'static str {
    if construct == r"\dx" {
        TIER2_TRACKING
    } else {
        OUTSIDE_CORPUS_TRACKING
    }
}

/// The numbered and line-width display environments, refused by name:
/// their equation numbers and `multline`'s flush lines sit against
/// `\linewidth`, which a formula's own box does not have. Numbered `align`
/// is refused with them rather than set without its numbers.
#[inline(always)]
pub(crate) fn env_is_t2(name: &str) -> bool {
    matches!(
        name,
        "equation" | "gather" | "align" | "multline" | "multline*"
    )
}

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
    /// A semantic symbol with an authored AMS font/code identity.
    AmsSym {
        ch: char,
        class: AtomClass,
        family: AmsFamily,
        code: u8,
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
    /// `\pmod{…}`: amsmath's parenthesized modulus.
    Pmod,
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
    /// A line-alignment declaration (`\centering`), text mode only.
    AlignDecl(LineAlign),
    /// A line-spacing declaration (`\doublespacing`), text mode only.
    LineSpacing(f64),
    /// A size declaration (`\small` …): sets the current size factor for
    /// the rest of the enclosing group.
    SizeChange(f64),
    /// A named spacing command (`\quad` …).
    Spacing(SpaceKind),
    /// `\hspace{len}` or `\hspace*{len}`: glue of an explicit length.
    Hspace,
    /// `\TeX` or `\LaTeX`.
    Logo(LogoKind),
    /// TeX's `\mathchoice{D}{T}{S}{SS}`.
    MathChoice,
    /// A phantom command.
    Phantom(PhantomKind),
    /// amsmath's `\boxed`.
    Boxed,
    /// The cancel package's `\cancel`, `\bcancel` or `\xcancel`.
    Cancel(CancelKind),
    /// amsmath's `\genfrac{ldelim}{rdelim}{thickness}{style}{num}{den}`.
    Genfrac,
    /// amsmath's `\cfrac[pos]{num}{den}`.
    CFrac,
    /// `\stackrel`/`\overset`/`\underset`.
    Stack(StackKind),
    /// `\substack{…}` — amsmath's multi-line subscript: one braced
    /// argument of `\\`-separated lines, stacked centered in script
    /// style (the same grid the `substack` environment produces).
    Substack,
    /// `\color[model]{…}`.
    Color,
    /// `\textcolor[model]{…}{body}`: `{\color{…}body}`.
    TextColor,
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
    /// `\ding{NN}` — the pifont escape (fm-j5t.4); the implemented codes
    /// live in [`ding_char`].
    Ding,
    /// `\xrightarrow`/`\xmapsto`: optional `[below]` then `{above}`.
    XArrow {
        /// True for `\xmapsto`.
        mapsto: bool,
    },
    /// Known tier-2 vocabulary: fail precisely.
    UnsupportedT2,
}

/// An environment the grammar knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EnvDef {
    /// Takes a braced column-spec argument (`array`).
    pub(crate) has_spec: bool,
}

/// `\not`'s character: U+0338, Unicode's long solidus overlay. Layout sets
/// it as plain TeX's `\mathchar"3236`, cmsy's zero-width negation slash.
pub(crate) const NOT_SLASH: char = '\u{0338}';

const fn sym(ch: char, class: AtomClass) -> Cmd {
    Cmd::Sym {
        ch,
        class,
        delim: false,
    }
}

const fn ams_sym(ch: char, class: AtomClass, family: AmsFamily, code: u8) -> Cmd {
    Cmd::AmsSym {
        ch,
        class,
        family,
        code,
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
        "hbar" => ams_sym('ℏ', Ord, AmsFamily::B, 0x7E),
        "ell" => sym('ℓ', Ord),
        "wp" => sym('℘', Ord),
        "Re" => sym('ℜ', Ord),
        "Im" => sym('ℑ', Ord),
        "aleph" => sym('ℵ', Ord),
        "imath" | "i" => sym('ı', Ord),
        "jmath" | "j" => sym('ȷ', Ord),
        "emptyset" => sym('∅', Ord),
        "varnothing" => sym('⌀', Ord),
        "exists" => sym('∃', Ord),
        "forall" => sym('∀', Ord),
        "neg" | "lnot" => sym('¬', Ord),
        "prime" => sym('′', Ord),
        "angle" => sym('∠', Ord),
        "triangle" => sym('△', Ord),
        "checkmark" => ams_sym('✓', Ord, AmsFamily::A, 0x58),
        // ── Astronomy and misc symbols (fm-j5t.4) ───────────────────────
        // © is an authored CM glyph; ♀ ♂ ♁ are unmapped in every bundled
        // face and draw through the drawn-symbol fallback in layout (the
        // Checkmark/Exmark drawn-mark precedent) — never a silent
        // substitute glyph.
        "copyright" => sym('©', Ord),
        "female" => sym('\u{2640}', Ord),
        "male" | "mars" => sym('\u{2642}', Ord),
        "earth" => sym('\u{2641}', Ord),
        "ding" => Cmd::Ding,
        "xrightarrow" => Cmd::XArrow { mapsto: false },
        "xmapsto" => Cmd::XArrow { mapsto: true },
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
        // amsmath's paired bars: `\lvert` is `\delimiter"426A30C`, an Open
        // atom, `\rvert` its Close; `\lVert`/`\rVert` likewise for `\|`.
        "lvert" => delim_sym('|', Open),
        "rvert" => delim_sym('|', Close),
        "lVert" => delim_sym('‖', Open),
        "rVert" => delim_sym('‖', Close),
        // Dots. `\ldots`-class dots are Inner atoms in TeX (\mathinner);
        // `\vdots` is a plain Ord box. LaTeX's `\hdots` is `\ldots`.
        "ldots" | "dots" | "dotsc" | "dotso" | "hdots" => sym('…', Inner),
        "cdots" | "dotsb" => sym('⋯', Inner),
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
        // the default pack reserves its name without replacing this glyph.
        "minus" => ams_sym('−', Bin, AmsFamily::A, 0x39),
        // ── Relations ───────────────────────────────────────────────────
        "le" | "leq" => sym('≤', Rel),
        "ge" | "geq" => sym('≥', Rel),
        "ne" | "neq" => sym('≠', Rel),
        "not" => sym(NOT_SLASH, Rel),
        "equiv" => sym('≡', Rel),
        "approx" => sym('≈', Rel),
        "sim" => sym('∼', Rel),
        "simeq" => sym('≃', Rel),
        "cong" => sym('≅', Rel),
        "doteq" => sym('≐', Rel),
        "propto" => sym('∝', Rel),
        "perp" => sym('⊥', Rel),
        "mid" => sym('∣', Rel),
        "nmid" => ams_sym('∤', Rel, AmsFamily::B, 0x2D),
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
        "circlearrowleft" => ams_sym('↺', Rel, AmsFamily::A, 0x09),
        "circlearrowright" => ams_sym('↻', Rel, AmsFamily::A, 0x08),
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
        "oiint" => Cmd::BigOp {
            ch: '∯',
            integral: true,
        },
        "iiint" => Cmd::BigOp {
            ch: '∭',
            integral: true,
        },
        "iiiint" => Cmd::BigOp {
            ch: '⨌',
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
        "pmod" => Cmd::Pmod,
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
        "dddot" => Cmd::Accent(AccentKind::Dddot),
        "ddddot" => Cmd::Accent(AccentKind::Ddddot),
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
        "frac" => Cmd::Frac(FracSpec::PLAIN),
        "dfrac" => Cmd::Frac(FracSpec {
            forced_style: Some(Style::Display),
            ..FracSpec::PLAIN
        }),
        "tfrac" => Cmd::Frac(FracSpec {
            forced_style: Some(Style::Text),
            ..FracSpec::PLAIN
        }),
        "binom" => Cmd::Frac(FracSpec::BINOM),
        // amsmath's `\genfrac(){0pt}{0}` and `\genfrac(){0pt}{1}`.
        "dbinom" => Cmd::Frac(FracSpec {
            forced_style: Some(Style::Display),
            ..FracSpec::BINOM
        }),
        "tbinom" => Cmd::Frac(FracSpec {
            forced_style: Some(Style::Text),
            ..FracSpec::BINOM
        }),
        "over" => Cmd::OverInfix(FracSpec::PLAIN),
        "choose" => Cmd::OverInfix(FracSpec::BINOM),
        "genfrac" => Cmd::Genfrac,
        "cfrac" => Cmd::CFrac,
        "sqrt" => Cmd::Radical,
        // ── Text islands and math alphabets ─────────────────────────────
        "text" => Cmd::Text,
        "textbf" => Cmd::TextStyled(TextStyle::Bold),
        "emph" => Cmd::TextStyled(TextStyle::Emph),
        // LaTeX's text font switches (ADR-0006's alias surface). Each one
        // sets its whole argument in one face: `\textbf{\textit{…}}` is the
        // inner face, not LaTeX's bold italic composition.
        "textit" => Cmd::TextStyled(TextStyle::Italic),
        "textrm" | "textup" | "textnormal" => Cmd::TextStyled(TextStyle::Roman),
        "texttt" => Cmd::TextStyled(TextStyle::Typewriter),
        "textsf" => Cmd::TextStyled(TextStyle::SansSerif),
        "mathbb" => Cmd::Alphabet(MathFont::Blackboard),
        // The default preamble pack maps the Reference template's dsfont
        // `\mathds` to blackboard (T1 by rank, G0-4).
        "mathds" => Cmd::Alphabet(MathFont::Blackboard),
        "mathcal" => Cmd::Alphabet(MathFont::Calligraphic),
        "mathscr" => Cmd::Alphabet(MathFont::Script),
        "mathfrak" => Cmd::Alphabet(MathFont::Fraktur),
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
        // Size declarations: group-scoped factors of the current size,
        // LaTeX's 10 pt ladder (`size10.clo`: tiny 5, footnotesize 8,
        // small 9, large 12, Large 14.4, huge 20.74 pt).
        "tiny" => Cmd::SizeChange(0.5),
        "footnotesize" => Cmd::SizeChange(0.8),
        "small" => Cmd::SizeChange(0.9),
        "large" => Cmd::SizeChange(1.2),
        "Large" => Cmd::SizeChange(1.44),
        "huge" => Cmd::SizeChange(2.074),
        "hspace" => Cmd::Hspace,
        "TeX" => Cmd::Logo(LogoKind::TeX),
        "LaTeX" => Cmd::Logo(LogoKind::LaTeX),
        "mathchoice" => Cmd::MathChoice,
        "quad" => Cmd::Spacing(SpaceKind::Quad),
        "qquad" => Cmd::Spacing(SpaceKind::Qquad),
        "boxed" => Cmd::Boxed,
        // The cancel package (not in the Reference's default template: a
        // superset, BN-05).
        "cancel" => Cmd::Cancel(CancelKind::Rising),
        "bcancel" => Cmd::Cancel(CancelKind::Falling),
        "xcancel" => Cmd::Cancel(CancelKind::Cross),
        "phantom" => Cmd::Phantom(PhantomKind::Full),
        "hphantom" => Cmd::Phantom(PhantomKind::Horizontal),
        "vphantom" => Cmd::Phantom(PhantomKind::Vertical),
        "stackrel" => Cmd::Stack(StackKind::Stackrel),
        "overset" => Cmd::Stack(StackKind::Overset),
        "underset" => Cmd::Stack(StackKind::Underset),
        "substack" => Cmd::Substack,
        "color" => Cmd::Color,
        "textcolor" => Cmd::TextColor,
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
        // ── amssymb (franken_manim fm-tex-constructs-t2-1wfq) ───────────
        // The Reference's default template loads amssymb, whose symbols
        // come from the AMS msam and msbm fonts. Each name keeps amssymb's
        // class and sets its Unicode codepoint from the bundled math faces
        // (Noto Sans Math; the thick variants from cmsy). The msam and msbm
        // faces are bundled now, but these names do not route to them yet
        // (unlike the `ams_sym` entries), so their shapes are Noto's; each
        // comment gives the AMS code a move to `ams_sym` needs (fm-0byq).
        // Names whose glyph no bundled face carries stay untiered.
        "Bbbk" => sym('𝕜', Ord),               // U+1D55C, msbm "7C
        "Bumpeq" => sym('≎', Rel),             // U+224E, msam "6D
        "Cap" => sym('⋒', Bin),                // U+22D2, msam "65
        "Cup" => sym('⋓', Bin),                // U+22D3, msam "64
        "Finv" => sym('Ⅎ', Ord),               // U+2132, msbm "60
        "Game" => sym('⅁', Ord),               // U+2141, msbm "61
        "Lleftarrow" => sym('⇚', Rel),         // U+21DA, msam "57
        "Rrightarrow" => sym('⇛', Rel),        // U+21DB, msam "56
        "Subset" => sym('⋐', Rel),             // U+22D0, msam "62
        "Supset" => sym('⋑', Rel),             // U+22D1, msam "63
        "Vdash" => sym('⊩', Rel),              // U+22A9, msam "0D
        "Vvdash" => sym('⊪', Rel),             // U+22AA, msam "0E
        "approxeq" => sym('≊', Rel),           // U+224A, msbm "75
        "backsim" => sym('∽', Rel),            // U+223D, msam "76
        "backsimeq" => sym('⋍', Rel),          // U+22CD, msam "77
        "barwedge" => sym('⊼', Bin),           // U+22BC, msam "5A
        "because" => sym('∵', Rel),            // U+2235, msam "2A
        "beth" => sym('ℶ', Ord),               // U+2136, msbm "69
        "between" => sym('≬', Rel),            // U+226C, msam "47
        "bigstar" => sym('★', Ord),            // U+2605, msam "46
        "blacksquare" => sym('■', Ord),        // U+25A0, msam "04
        "blacktriangle" => sym('▴', Ord),      // U+25B4, msam "4E
        "blacktriangledown" => sym('▾', Ord),  // U+25BE, msam "48
        "blacktriangleleft" => sym('◀', Rel),  // U+25C0, msam "4A
        "blacktriangleright" => sym('▶', Rel), // U+25B6, msam "49
        "boxdot" => sym('⊡', Bin),             // U+22A1, msam "00
        "boxminus" => sym('⊟', Bin),           // U+229F, msam "0C
        "boxplus" => sym('⊞', Bin),            // U+229E, msam "01
        "boxtimes" => sym('⊠', Bin),           // U+22A0, msam "02
        "bumpeq" => sym('≏', Rel),             // U+224F, msam "6C
        "centerdot" => sym('⋅', Bin),          // U+22C5, msam "05
        "circeq" => sym('≗', Rel),             // U+2257, msam "24
        "circledast" => sym('⊛', Bin),         // U+229B, msam "7E
        "circledcirc" => sym('⊚', Bin),        // U+229A, msam "7D
        "circleddash" => sym('⊝', Bin),        // U+229D, msam "7F
        "complement" => sym('∁', Ord),         // U+2201, msam "7B
        "curlyeqprec" => sym('⋞', Rel),        // U+22DE, msam "32
        "curlyeqsucc" => sym('⋟', Rel),        // U+22DF, msam "33
        "curlyvee" => sym('⋎', Bin),           // U+22CE, msam "67
        "curlywedge" => sym('⋏', Bin),         // U+22CF, msam "66
        "daleth" => sym('ℸ', Ord),             // U+2138, msbm "6B
        "divideontimes" => sym('⋇', Bin),      // U+22C7, msbm "3E
        "doteqdot" | "Doteq" => sym('≑', Rel), // U+2251, msam "2B
        "dotplus" => sym('∔', Bin),            // U+2214, msam "75
        "downdownarrows" => sym('⇊', Rel),     // U+21CA, msam "15
        "downharpoonleft" => sym('⇃', Rel),    // U+21C3, msam "19
        "downharpoonright" => sym('⇂', Rel),   // U+21C2, msam "17
        "eqcirc" => sym('≖', Rel),             // U+2256, msam "50
        "eqsim" => sym('≂', Rel),              // U+2242, msbm "68
        "fallingdotseq" => sym('≒', Rel),      // U+2252, msam "3B
        "geqq" => sym('≧', Rel),               // U+2267, msam "3D
        "geqslant" => sym('⩾', Rel),           // U+2A7E, msam "3E
        "ggg" | "gggtr" => sym('⋙', Rel),      // U+22D9, msam "6F
        "gimel" => sym('ℷ', Ord),              // U+2137, msbm "6A
        "gneqq" => sym('≩', Rel),              // U+2269, msbm "09
        "gnsim" => sym('⋧', Rel),              // U+22E7, msbm "13
        "gtrdot" => sym('⋗', Bin),             // U+22D7, msbm "6D
        "gtreqless" => sym('⋛', Rel),          // U+22DB, msam "52
        "gtrless" => sym('≷', Rel),            // U+2277, msam "3F
        "gtrsim" => sym('≳', Rel),             // U+2273, msam "26
        "gvertneqq" => sym('≩', Rel),          // U+2269, msbm "01
        "hslash" => ams_sym('ℏ', Ord, AmsFamily::B, 0x7D), // distinct from hbar's 0x7E
        "intercal" => sym('⊺', Bin),           // U+22BA, msam "7C
        "leftarrowtail" => sym('↢', Rel),      // U+21A2, msam "1B
        "leftleftarrows" => sym('⇇', Rel),     // U+21C7, msam "12
        "leftrightharpoons" => sym('⇋', Rel),  // U+21CB, msam "0B
        "leftrightsquigarrow" => sym('↭', Rel), // U+21AD, msam "21
        "leftthreetimes" => sym('⋋', Bin),     // U+22CB, msam "68
        "leqq" => sym('≦', Rel),               // U+2266, msam "35
        "leqslant" => sym('⩽', Rel),           // U+2A7D, msam "36
        "lessdot" => sym('⋖', Bin),            // U+22D6, msbm "6C
        "lesseqgtr" => sym('⋚', Rel),          // U+22DA, msam "51
        "lessgtr" => sym('≶', Rel),            // U+2276, msam "37
        "lesssim" => sym('≲', Rel),            // U+2272, msam "2E
        "lhd" => sym('⊲', Bin),                // U+22B2, msam "43
        "lll" | "llless" => sym('⋘', Rel),     // U+22D8, msam "6E
        "lneqq" => sym('≨', Rel),              // U+2268, msbm "08
        "lnsim" => sym('⋦', Rel),              // U+22E6, msbm "12
        "looparrowleft" => sym('↫', Rel),      // U+21AB, msam "22
        "looparrowright" => sym('↬', Rel),     // U+21AC, msam "23
        "ltimes" => sym('⋉', Bin),             // U+22C9, msbm "6E
        "lvertneqq" => sym('≨', Rel),          // U+2268, msbm "00
        "measuredangle" => sym('∡', Ord),      // U+2221, msam "5D
        "mho" => sym('℧', Ord),                // U+2127, msbm "66
        "multimap" => sym('⊸', Rel),           // U+22B8, msam "28
        "nLeftarrow" => sym('⇍', Rel),         // U+21CD, msbm "3A
        "nLeftrightarrow" => sym('⇎', Rel),    // U+21CE, msbm "3C
        "nRightarrow" => sym('⇏', Rel),        // U+21CF, msbm "3B
        "nVDash" => sym('⊯', Rel),             // U+22AF, msbm "33
        "nVdash" => sym('⊮', Rel),             // U+22AE, msbm "31
        "ncong" => sym('≇', Rel),              // U+2247, msbm "1D
        "nexists" => sym('∄', Ord),            // U+2204, msbm "40
        "ngeq" => sym('≱', Rel),               // U+2271, msbm "03
        "ngtr" => sym('≯', Rel),               // U+226F, msbm "05
        "nleftarrow" => sym('↚', Rel),         // U+219A, msbm "38
        "nleftrightarrow" => sym('↮', Rel),    // U+21AE, msbm "3D
        "nleq" => sym('≰', Rel),               // U+2270, msbm "02
        "nless" => sym('≮', Rel),              // U+226E, msbm "04
        "nparallel" => sym('∦', Rel),          // U+2226, msbm "2C
        "nprec" => sym('⊀', Rel),              // U+2280, msbm "06
        "npreceq" => sym('⋠', Rel),            // U+22E0, msbm "0E
        "nrightarrow" => sym('↛', Rel),        // U+219B, msbm "39
        "nshortmid" => ams_sym('∤', Rel, AmsFamily::B, 0x2E),
        "nshortparallel" => sym('∦', Rel),   // U+2226, msbm "2F
        "nsim" => sym('≁', Rel),             // U+2241, msbm "1C
        "nsubseteq" => sym('⊈', Rel),        // U+2288, msbm "2A
        "nsucc" => sym('⊁', Rel),            // U+2281, msbm "07
        "nsucceq" => sym('⋡', Rel),          // U+22E1, msbm "0F
        "nsupseteq" => sym('⊉', Rel),        // U+2289, msbm "2B
        "ntriangleleft" => sym('⋪', Rel),    // U+22EA, msbm "36
        "ntrianglelefteq" => sym('⋬', Rel),  // U+22EC, msbm "35
        "ntriangleright" => sym('⋫', Rel),   // U+22EB, msbm "37
        "ntrianglerighteq" => sym('⋭', Rel), // U+22ED, msbm "34
        "nvDash" => sym('⊭', Rel),           // U+22AD, msbm "32
        "nvdash" => sym('⊬', Rel),           // U+22AC, msbm "30
        "pitchfork" => sym('⋔', Rel),        // U+22D4, msam "74
        "preccurlyeq" => sym('≼', Rel),      // U+227C, msam "34
        "precnsim" => sym('⋨', Rel),         // U+22E8, msbm "10
        "precsim" => sym('≾', Rel),          // U+227E, msam "2D
        "rhd" => sym('⊳', Bin),              // U+22B3, msam "42
        "rightarrowtail" => sym('↣', Rel),   // U+21A3, msam "1A
        "rightrightarrows" => sym('⇉', Rel), // U+21C9, msam "13
        "rightsquigarrow" | "leadsto" => sym('⇝', Rel), // U+21DD, msam "20
        "rightthreetimes" => sym('⋌', Bin),  // U+22CC, msam "69
        "risingdotseq" => sym('≓', Rel),     // U+2253, msam "3A
        "rtimes" => sym('⋊', Bin),           // U+22CA, msbm "6F
        "shortmid" => sym('∣', Rel),         // U+2223, msbm "70
        "shortparallel" => sym('∥', Rel),    // U+2225, msbm "71
        "sphericalangle" => sym('∢', Ord),   // U+2222, msam "5E
        "sqsubset" => sym('⊏', Rel),         // U+228F, msam "40
        "sqsupset" => sym('⊐', Rel),         // U+2290, msam "41
        "square" | "Box" => sym('□', Ord),   // U+25A1, msam "03
        "subsetneq" => sym('⊊', Rel),        // U+228A, msbm "28
        "succcurlyeq" => sym('≽', Rel),      // U+227D, msam "3C
        "succnsim" => sym('⋩', Rel),         // U+22E9, msbm "11
        "succsim" => sym('≿', Rel),          // U+227F, msam "25
        "supsetneq" => sym('⊋', Rel),        // U+228B, msbm "29
        "therefore" => sym('∴', Rel),        // U+2234, msam "29
        "thickapprox" => sym('≈', Rel),      // U+2248, msbm "74 (cmsy's thinner ≈)
        "thicksim" => sym('∼', Rel),         // U+223C, msbm "73 (cmsy's thinner ∼)
        "triangledown" => sym('▿', Ord),     // U+25BF, msam "4F
        "trianglelefteq" => sym('⊴', Rel),   // U+22B4, msam "45
        "triangleq" => sym('≜', Rel),        // U+225C, msam "2C
        "trianglerighteq" => sym('⊵', Rel),  // U+22B5, msam "44
        "twoheadleftarrow" => sym('↞', Rel), // U+219E, msam "11
        "twoheadrightarrow" => sym('↠', Rel), // U+21A0, msam "10
        "unlhd" => sym('⊴', Bin),            // U+22B4, msam "45
        "unrhd" => sym('⊵', Bin),            // U+22B5, msam "44
        "upharpoonleft" => sym('↿', Rel),    // U+21BF, msam "18
        "upharpoonright" => sym('↾', Rel),   // U+21BE, msam "16
        "upuparrows" => sym('⇈', Rel),       // U+21C8, msam "14
        "vDash" => sym('⊨', Rel),            // U+22A8, msam "0F
        "varpropto" => sym('∝', Rel),        // U+221D, msam "5F
        "varsubsetneq" => sym('⊊', Rel),     // U+228A, msbm "20
        "varsupsetneq" => sym('⊋', Rel),     // U+228B, msbm "21
        "vartriangle" => sym('▵', Rel),      // U+25B5, msam "4D
        "vartriangleleft" => sym('⊲', Rel),  // U+22B2, msam "43
        "vartriangleright" => sym('⊳', Rel), // U+22B3, msam "42
        "veebar" => sym('⊻', Bin),           // U+22BB, msam "59
        "doublecap" => sym('⋒', Bin),        // amsfonts: \let\doublecap\Cap
        "doublecup" => sym('⋓', Bin),        // amsfonts: \let\doublecup\Cup
        // ── Structure ───────────────────────────────────────────────────
        "left" => Cmd::Left,
        "right" => Cmd::Right,
        "limits" => Cmd::Limits,
        "nolimits" => Cmd::NoLimits,
        "begin" => Cmd::Begin,
        "end" => Cmd::End,
        // Line-alignment declarations (text mode; the declaration form of
        // the `center` environment).
        "centering" => Cmd::AlignDecl(LineAlign::Center),
        // setspace's declaration form; 1.667 is its 10 pt ladder value.
        "doublespacing" => Cmd::LineSpacing(1.667),
        // ── Known tier-2 vocabulary (G0-4 `construct_table.tsv`) ────────
        // (`\centering` graduated to a real AlignDecl above.)
        "dx" => Cmd::UnsupportedT2,
        // Common constructs outside the G0-4 corpus, refused by name until
        // they land ([`OUTSIDE_CORPUS_TRACKING`]): `\tag` and `\sideset`
        // need their own layout, and `\cancelto` the cancel package's
        // arrow and its script.
        "tag" | "sideset" | "cancelto" => Cmd::UnsupportedT2,
        _ => return None,
    })
}

/// The implemented slice of the pifont `\ding` table (fm-j5t.4). The
/// G0-4 corpus uses only code 51 — the checkmark, whose glyph resolves
/// through the symbol faces as U+2713 (the `\checkmark` codepoint, which
/// those faces carry). Any other code fails precisely, by name.
#[inline(always)]
pub(crate) fn ding_char(code: u32) -> Option<char> {
    match code {
        51 => Some('✓'),
        _ => None,
    }
}

/// Look up an environment name.
#[inline(always)]
pub(crate) fn lookup_env(name: &str) -> Option<EnvDef> {
    match name {
        "array" => Some(EnvDef { has_spec: true }),
        "align*" | "aligned" | "gather*" | "gathered" | "equation*" | "cases" | "matrix"
        | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" | "smallmatrix"
        | "substack" => Some(EnvDef { has_spec: false }),
        _ => None,
    }
}

/// The line-alignment environments (`flushleft`, `center`, `flushright`):
/// text-mode blocks aligning their `\\`-split lines within the widest
/// line's width.
#[inline(always)]
pub(crate) fn line_align_env(name: &str) -> Option<LineAlign> {
    match name {
        "flushleft" => Some(LineAlign::Left),
        "center" => Some(LineAlign::Center),
        "flushright" => Some(LineAlign::Right),
        _ => None,
    }
}

/// True when the control *symbol* is known tier-2 vocabulary. Empty since
/// the text accents `\'` and `\"` graduated (fm-j5t); kept so the next
/// tier-2 control symbol has its slot back.
#[inline(always)]
pub(crate) fn control_symbol_is_t2(ch: char) -> bool {
    let _ = ch;
    false
}

/// Compose a text accent over its base letter, to the precomposed Unicode
/// character the bundled CM faces carry (probe-verified: the full Latin-1
/// acute and dieresis rows exist in CM regular and italic). `None` is a
/// precise malformed-argument error at the parse site, never silence.
pub(crate) fn compose_text_accent(accent: char, base: char) -> Option<char> {
    Some(match (accent, base) {
        ('\'', 'a') => 'á',
        ('\'', 'e') => 'é',
        ('\'', 'i') => 'í',
        ('\'', 'o') => 'ó',
        ('\'', 'u') => 'ú',
        ('\'', 'y') => 'ý',
        ('\'', 'A') => 'Á',
        ('\'', 'E') => 'É',
        ('\'', 'I') => 'Í',
        ('\'', 'O') => 'Ó',
        ('\'', 'U') => 'Ú',
        ('"', 'a') => 'ä',
        ('"', 'e') => 'ë',
        ('"', 'i') => 'ï',
        ('"', 'o') => 'ö',
        ('"', 'u') => 'ü',
        ('"', 'y') => 'ÿ',
        ('"', 'A') => 'Ä',
        ('"', 'E') => 'Ë',
        ('"', 'I') => 'Ï',
        ('"', 'O') => 'Ö',
        ('"', 'U') => 'Ü',
        _ => return None,
    })
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
        if lookup_env(env).is_some() || line_align_env(env).is_some() {
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
                // the line break, the `\|` delimiter, and the graduated
                // text accents.
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
                        | '\''
                        | '"'
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
            r"\substack",
            r"\,",
            r"\\",
            r"\{",
            "env:array",
            "env:substack",
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
        // Still pending. (The symbol shelf and the text accents graduated
        // in the fm-j5t tranches.)
        assert_eq!(
            construct_status(r"\dx"),
            ConstructStatus::UnsupportedT2,
            r"expected \dx tier-2 (the last pending construct)"
        );
        for c in [
            r"\nmid",
            r"\oiint",
            r"\i",
            r"\j",
            r"\circlearrowright",
            r"\dddot",
            r"\ddddot",
            r"\xrightarrow",
            r"\xmapsto",
            r"\doublespacing",
            r"\'",
            "\\\"",
        ] {
            assert_eq!(
                construct_status(c),
                ConstructStatus::Supported,
                "expected {c} graduated"
            );
        }
    }

    #[test]
    fn astronomy_and_misc_symbols_are_supported() {
        // fm-j5t.4: the astronomy/biology symbols, \copyright, and the
        // pifont \ding escape left the tier-2 vocabulary.
        for c in [
            r"\female",
            r"\male",
            r"\mars",
            r"\earth",
            r"\copyright",
            r"\ding",
        ] {
            assert_eq!(
                construct_status(c),
                ConstructStatus::Supported,
                "expected {c} supported"
            );
        }
    }

    #[test]
    fn line_alignment_constructs_are_supported() {
        for c in [
            r"\centering",
            "env:flushleft",
            "env:center",
            "env:flushright",
        ] {
            assert_eq!(
                construct_status(c),
                ConstructStatus::Supported,
                "expected {c} supported"
            );
        }
    }

    #[test]
    fn size_declarations_are_supported() {
        for c in [
            r"\tiny",
            r"\footnotesize",
            r"\small",
            r"\large",
            r"\Large",
            r"\huge",
        ] {
            assert_eq!(
                construct_status(c),
                ConstructStatus::Supported,
                "expected {c} supported"
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
