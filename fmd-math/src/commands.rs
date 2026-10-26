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

/// Where the constructs outside the G0-4 corpus that are still refused are
/// tracked: display numbering, `\sideset` and `\cancelto`, which
/// fm-tex-constructs-t2-1wfq left with no demand in either ranking corpus.
pub const OUTSIDE_CORPUS_TRACKING: &str = "franken_manim fm-tex-numbered-displays-bxl9 (numbered displays and the constructs outside the G0-4 corpus still refused)";

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
    /// `\mod{…}`: amsmath's unparenthesized modulus.
    Mod,
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
        // amsmath's `\bmod` is a Bin "mod" between 5 mu kerns that cancel
        // its medium spaces; as an operator name it sets 2 mu narrow on
        // each side. It occurs nowhere in the corpus.
        "bmod" => Cmd::OpName {
            rendered: "mod",
            limits: false,
        },
        "mod" => Cmd::Mod,
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
        // ── amssymb (franken_manim fm-tex-constructs-t2-1wfq, fm-0byq) ──
        // The Reference's default template loads amssymb, whose symbols
        // come from the AMS msam and msbm fonts. Each name keeps amssymb's
        // class and sets its authored glyph from the bundled AMS faces, at
        // the family and code amssymb.sty's \DeclareMathSymbol gives it, with
        // the TFM box; the character is its Unicode meaning.
        "Bbbk" => ams_sym('𝕜', Ord, AmsFamily::B, 0x7C), // U+1D55C
        "Bumpeq" => ams_sym('≎', Rel, AmsFamily::A, 0x6D), // U+224E
        "Cap" => ams_sym('⋒', Bin, AmsFamily::A, 0x65),  // U+22D2
        "Cup" => ams_sym('⋓', Bin, AmsFamily::A, 0x64),  // U+22D3
        "Finv" => ams_sym('Ⅎ', Ord, AmsFamily::B, 0x60), // U+2132
        "Game" => ams_sym('⅁', Ord, AmsFamily::B, 0x61), // U+2141
        "Lleftarrow" => ams_sym('⇚', Rel, AmsFamily::A, 0x57), // U+21DA
        "Rrightarrow" => ams_sym('⇛', Rel, AmsFamily::A, 0x56), // U+21DB
        "Subset" => ams_sym('⋐', Rel, AmsFamily::A, 0x62), // U+22D0
        "Supset" => ams_sym('⋑', Rel, AmsFamily::A, 0x63), // U+22D1
        "Vdash" => ams_sym('⊩', Rel, AmsFamily::A, 0x0D), // U+22A9
        "Vvdash" => ams_sym('⊪', Rel, AmsFamily::A, 0x0E), // U+22AA
        "approxeq" => ams_sym('≊', Rel, AmsFamily::B, 0x75), // U+224A
        "backsim" => ams_sym('∽', Rel, AmsFamily::A, 0x76), // U+223D
        "backsimeq" => ams_sym('⋍', Rel, AmsFamily::A, 0x77), // U+22CD
        "barwedge" => ams_sym('⊼', Bin, AmsFamily::A, 0x5A), // U+22BC
        "because" => ams_sym('∵', Rel, AmsFamily::A, 0x2A), // U+2235
        "beth" => ams_sym('ℶ', Ord, AmsFamily::B, 0x69), // U+2136
        "between" => ams_sym('≬', Rel, AmsFamily::A, 0x47), // U+226C
        "bigstar" => ams_sym('★', Ord, AmsFamily::A, 0x46), // U+2605
        "blacksquare" => ams_sym('■', Ord, AmsFamily::A, 0x04), // U+25A0
        "blacktriangle" => ams_sym('▴', Ord, AmsFamily::A, 0x4E), // U+25B4
        "blacktriangledown" => ams_sym('▾', Ord, AmsFamily::A, 0x48), // U+25BE
        "blacktriangleleft" => ams_sym('◀', Rel, AmsFamily::A, 0x4A), // U+25C0
        "blacktriangleright" => ams_sym('▶', Rel, AmsFamily::A, 0x49), // U+25B6
        "boxdot" => ams_sym('⊡', Bin, AmsFamily::A, 0x00), // U+22A1
        "boxminus" => ams_sym('⊟', Bin, AmsFamily::A, 0x0C), // U+229F
        "boxplus" => ams_sym('⊞', Bin, AmsFamily::A, 0x01), // U+229E
        "boxtimes" => ams_sym('⊠', Bin, AmsFamily::A, 0x02), // U+22A0
        "bumpeq" => ams_sym('≏', Rel, AmsFamily::A, 0x6C), // U+224F
        "centerdot" => ams_sym('⋅', Bin, AmsFamily::A, 0x05), // U+22C5
        "circeq" => ams_sym('≗', Rel, AmsFamily::A, 0x24), // U+2257
        "circledast" => ams_sym('⊛', Bin, AmsFamily::A, 0x7E), // U+229B
        "circledcirc" => ams_sym('⊚', Bin, AmsFamily::A, 0x7D), // U+229A
        "circleddash" => ams_sym('⊝', Bin, AmsFamily::A, 0x7F), // U+229D
        "complement" => ams_sym('∁', Ord, AmsFamily::A, 0x7B), // U+2201
        "curlyeqprec" => ams_sym('⋞', Rel, AmsFamily::A, 0x32), // U+22DE
        "curlyeqsucc" => ams_sym('⋟', Rel, AmsFamily::A, 0x33), // U+22DF
        "curlyvee" => ams_sym('⋎', Bin, AmsFamily::A, 0x67), // U+22CE
        "curlywedge" => ams_sym('⋏', Bin, AmsFamily::A, 0x66), // U+22CF
        "daleth" => ams_sym('ℸ', Ord, AmsFamily::B, 0x6B), // U+2138
        "divideontimes" => ams_sym('⋇', Bin, AmsFamily::B, 0x3E), // U+22C7
        "doteqdot" | "Doteq" => ams_sym('≑', Rel, AmsFamily::A, 0x2B), // U+2251
        "dotplus" => ams_sym('∔', Bin, AmsFamily::A, 0x75), // U+2214
        "downdownarrows" => ams_sym('⇊', Rel, AmsFamily::A, 0x15), // U+21CA
        "downharpoonleft" => ams_sym('⇃', Rel, AmsFamily::A, 0x19), // U+21C3
        "downharpoonright" => ams_sym('⇂', Rel, AmsFamily::A, 0x17), // U+21C2
        "eqcirc" => ams_sym('≖', Rel, AmsFamily::A, 0x50), // U+2256
        "eqsim" => ams_sym('≂', Rel, AmsFamily::B, 0x68), // U+2242
        "fallingdotseq" => ams_sym('≒', Rel, AmsFamily::A, 0x3B), // U+2252
        "geqq" => ams_sym('≧', Rel, AmsFamily::A, 0x3D), // U+2267
        "geqslant" => ams_sym('⩾', Rel, AmsFamily::A, 0x3E), // U+2A7E
        "ggg" | "gggtr" => ams_sym('⋙', Rel, AmsFamily::A, 0x6F), // U+22D9
        "gimel" => ams_sym('ℷ', Ord, AmsFamily::B, 0x6A), // U+2137
        "gneqq" => ams_sym('≩', Rel, AmsFamily::B, 0x09), // U+2269
        "gnsim" => ams_sym('⋧', Rel, AmsFamily::B, 0x13), // U+22E7
        "gtrdot" => ams_sym('⋗', Bin, AmsFamily::B, 0x6D), // U+22D7
        "gtreqless" => ams_sym('⋛', Rel, AmsFamily::A, 0x52), // U+22DB
        "gtrless" => ams_sym('≷', Rel, AmsFamily::A, 0x3F), // U+2277
        "gtrsim" => ams_sym('≳', Rel, AmsFamily::A, 0x26), // U+2273
        "gvertneqq" => ams_sym('≩', Rel, AmsFamily::B, 0x01), // U+2269
        "hslash" => ams_sym('ℏ', Ord, AmsFamily::B, 0x7D), // distinct from hbar's 0x7E
        "intercal" => ams_sym('⊺', Bin, AmsFamily::A, 0x7C), // U+22BA
        "leftarrowtail" => ams_sym('↢', Rel, AmsFamily::A, 0x1B), // U+21A2
        "leftleftarrows" => ams_sym('⇇', Rel, AmsFamily::A, 0x12), // U+21C7
        "leftrightharpoons" => ams_sym('⇋', Rel, AmsFamily::A, 0x0B), // U+21CB
        "leftrightsquigarrow" => ams_sym('↭', Rel, AmsFamily::A, 0x21), // U+21AD
        "leftthreetimes" => ams_sym('⋋', Bin, AmsFamily::A, 0x68), // U+22CB
        "leqq" => ams_sym('≦', Rel, AmsFamily::A, 0x35), // U+2266
        "leqslant" => ams_sym('⩽', Rel, AmsFamily::A, 0x36), // U+2A7D
        "lessdot" => ams_sym('⋖', Bin, AmsFamily::B, 0x6C), // U+22D6
        "lesseqgtr" => ams_sym('⋚', Rel, AmsFamily::A, 0x51), // U+22DA
        "lessgtr" => ams_sym('≶', Rel, AmsFamily::A, 0x37), // U+2276
        "lesssim" => ams_sym('≲', Rel, AmsFamily::A, 0x2E), // U+2272
        "lhd" => ams_sym('⊲', Bin, AmsFamily::A, 0x43),  // U+22B2
        "lll" | "llless" => ams_sym('⋘', Rel, AmsFamily::A, 0x6E), // U+22D8
        "lneqq" => ams_sym('≨', Rel, AmsFamily::B, 0x08), // U+2268
        "lnsim" => ams_sym('⋦', Rel, AmsFamily::B, 0x12), // U+22E6
        "looparrowleft" => ams_sym('↫', Rel, AmsFamily::A, 0x22), // U+21AB
        "looparrowright" => ams_sym('↬', Rel, AmsFamily::A, 0x23), // U+21AC
        "ltimes" => ams_sym('⋉', Bin, AmsFamily::B, 0x6E), // U+22C9
        "lvertneqq" => ams_sym('≨', Rel, AmsFamily::B, 0x00), // U+2268
        "measuredangle" => ams_sym('∡', Ord, AmsFamily::A, 0x5D), // U+2221
        "mho" => ams_sym('℧', Ord, AmsFamily::B, 0x66),  // U+2127
        "multimap" => ams_sym('⊸', Rel, AmsFamily::A, 0x28), // U+22B8
        "nLeftarrow" => ams_sym('⇍', Rel, AmsFamily::B, 0x3A), // U+21CD
        "nLeftrightarrow" => ams_sym('⇎', Rel, AmsFamily::B, 0x3C), // U+21CE
        "nRightarrow" => ams_sym('⇏', Rel, AmsFamily::B, 0x3B), // U+21CF
        "nVDash" => ams_sym('⊯', Rel, AmsFamily::B, 0x33), // U+22AF
        "nVdash" => ams_sym('⊮', Rel, AmsFamily::B, 0x31), // U+22AE
        "ncong" => ams_sym('≇', Rel, AmsFamily::B, 0x1D), // U+2247
        "nexists" => ams_sym('∄', Ord, AmsFamily::B, 0x40), // U+2204
        "ngeq" => ams_sym('≱', Rel, AmsFamily::B, 0x03), // U+2271
        "ngtr" => ams_sym('≯', Rel, AmsFamily::B, 0x05), // U+226F
        "nleftarrow" => ams_sym('↚', Rel, AmsFamily::B, 0x38), // U+219A
        "nleftrightarrow" => ams_sym('↮', Rel, AmsFamily::B, 0x3D), // U+21AE
        "nleq" => ams_sym('≰', Rel, AmsFamily::B, 0x02), // U+2270
        "nless" => ams_sym('≮', Rel, AmsFamily::B, 0x04), // U+226E
        "nparallel" => ams_sym('∦', Rel, AmsFamily::B, 0x2C), // U+2226
        "nprec" => ams_sym('⊀', Rel, AmsFamily::B, 0x06), // U+2280
        "npreceq" => ams_sym('⋠', Rel, AmsFamily::B, 0x0E), // U+22E0
        "nrightarrow" => ams_sym('↛', Rel, AmsFamily::B, 0x39), // U+219B
        "nshortmid" => ams_sym('∤', Rel, AmsFamily::B, 0x2E),
        "nshortparallel" => ams_sym('∦', Rel, AmsFamily::B, 0x2F), // U+2226
        "nsim" => ams_sym('≁', Rel, AmsFamily::B, 0x1C),           // U+2241
        "nsubseteq" => ams_sym('⊈', Rel, AmsFamily::B, 0x2A),      // U+2288
        "nsucc" => ams_sym('⊁', Rel, AmsFamily::B, 0x07),          // U+2281
        "nsucceq" => ams_sym('⋡', Rel, AmsFamily::B, 0x0F),        // U+22E1
        "nsupseteq" => ams_sym('⊉', Rel, AmsFamily::B, 0x2B),      // U+2289
        "ntriangleleft" => ams_sym('⋪', Rel, AmsFamily::B, 0x36),  // U+22EA
        "ntrianglelefteq" => ams_sym('⋬', Rel, AmsFamily::B, 0x35), // U+22EC
        "ntriangleright" => ams_sym('⋫', Rel, AmsFamily::B, 0x37), // U+22EB
        "ntrianglerighteq" => ams_sym('⋭', Rel, AmsFamily::B, 0x34), // U+22ED
        "nvDash" => ams_sym('⊭', Rel, AmsFamily::B, 0x32),         // U+22AD
        "nvdash" => ams_sym('⊬', Rel, AmsFamily::B, 0x30),         // U+22AC
        "pitchfork" => ams_sym('⋔', Rel, AmsFamily::A, 0x74),      // U+22D4
        "preccurlyeq" => ams_sym('≼', Rel, AmsFamily::A, 0x34),    // U+227C
        "precnsim" => ams_sym('⋨', Rel, AmsFamily::B, 0x10),       // U+22E8
        "precsim" => ams_sym('≾', Rel, AmsFamily::A, 0x2D),        // U+227E
        "rhd" => ams_sym('⊳', Bin, AmsFamily::A, 0x42),            // U+22B3
        "rightarrowtail" => ams_sym('↣', Rel, AmsFamily::A, 0x1A), // U+21A3
        "rightrightarrows" => ams_sym('⇉', Rel, AmsFamily::A, 0x13), // U+21C9
        "rightsquigarrow" | "leadsto" => ams_sym('⇝', Rel, AmsFamily::A, 0x20), // U+21DD
        "rightthreetimes" => ams_sym('⋌', Bin, AmsFamily::A, 0x69), // U+22CC
        "risingdotseq" => ams_sym('≓', Rel, AmsFamily::A, 0x3A),   // U+2253
        "rtimes" => ams_sym('⋊', Bin, AmsFamily::B, 0x6F),         // U+22CA
        "shortmid" => ams_sym('∣', Rel, AmsFamily::B, 0x70),       // U+2223
        "shortparallel" => ams_sym('∥', Rel, AmsFamily::B, 0x71),  // U+2225
        "sphericalangle" => ams_sym('∢', Ord, AmsFamily::A, 0x5E), // U+2222
        "sqsubset" => ams_sym('⊏', Rel, AmsFamily::A, 0x40),       // U+228F
        "sqsupset" => ams_sym('⊐', Rel, AmsFamily::A, 0x41),       // U+2290
        "square" | "Box" => ams_sym('□', Ord, AmsFamily::A, 0x03), // U+25A1
        "subsetneq" => ams_sym('⊊', Rel, AmsFamily::B, 0x28),      // U+228A
        "succcurlyeq" => ams_sym('≽', Rel, AmsFamily::A, 0x3C),    // U+227D
        "succnsim" => ams_sym('⋩', Rel, AmsFamily::B, 0x11),       // U+22E9
        "succsim" => ams_sym('≿', Rel, AmsFamily::A, 0x25),        // U+227F
        "supsetneq" => ams_sym('⊋', Rel, AmsFamily::B, 0x29),      // U+228B
        "therefore" => ams_sym('∴', Rel, AmsFamily::A, 0x29),      // U+2234
        "thickapprox" => ams_sym('≈', Rel, AmsFamily::B, 0x74),    // U+2248, msbm's thick ≈
        "thicksim" => ams_sym('∼', Rel, AmsFamily::B, 0x73),       // U+223C, msbm's thick ∼
        "triangledown" => ams_sym('▿', Ord, AmsFamily::A, 0x4F),   // U+25BF
        "trianglelefteq" => ams_sym('⊴', Rel, AmsFamily::A, 0x45), // U+22B4
        "triangleq" => ams_sym('≜', Rel, AmsFamily::A, 0x2C),      // U+225C
        "trianglerighteq" => ams_sym('⊵', Rel, AmsFamily::A, 0x44), // U+22B5
        "twoheadleftarrow" => ams_sym('↞', Rel, AmsFamily::A, 0x11), // U+219E
        "twoheadrightarrow" => ams_sym('↠', Rel, AmsFamily::A, 0x10), // U+21A0
        "unlhd" => ams_sym('⊴', Bin, AmsFamily::A, 0x45),          // U+22B4
        "unrhd" => ams_sym('⊵', Bin, AmsFamily::A, 0x44),          // U+22B5
        "upharpoonleft" => ams_sym('↿', Rel, AmsFamily::A, 0x18),  // U+21BF
        "upharpoonright" => ams_sym('↾', Rel, AmsFamily::A, 0x16), // U+21BE
        "upuparrows" => ams_sym('⇈', Rel, AmsFamily::A, 0x14),     // U+21C8
        "vDash" => ams_sym('⊨', Rel, AmsFamily::A, 0x0F),          // U+22A8
        "varpropto" => ams_sym('∝', Rel, AmsFamily::A, 0x5F),      // U+221D
        "varsubsetneq" => ams_sym('⊊', Rel, AmsFamily::B, 0x20),   // U+228A
        "varsupsetneq" => ams_sym('⊋', Rel, AmsFamily::B, 0x21),   // U+228B
        "vartriangle" => ams_sym('▵', Rel, AmsFamily::A, 0x4D),    // U+25B5
        "vartriangleleft" => ams_sym('⊲', Rel, AmsFamily::A, 0x43), // U+22B2
        "vartriangleright" => ams_sym('⊳', Rel, AmsFamily::A, 0x42), // U+22B3
        "veebar" => ams_sym('⊻', Bin, AmsFamily::A, 0x59),         // U+22BB
        "doublecap" => ams_sym('⋒', Bin, AmsFamily::A, 0x65),      // amsfonts: \let\doublecap\Cap
        "doublecup" => ams_sym('⋓', Bin, AmsFamily::A, 0x64),      // amsfonts: \let\doublecup\Cup
        // amssymb names no face carried before the AMS faces were bundled.
        // Six with no single Unicode character (\nleqslant, \ngeqslant,
        // \nleqq, \ngeqq, \nsubseteqq, \nsupseteqq) stay untiered.
        "lozenge" => ams_sym('◊', Ord, AmsFamily::A, 0x06), // U+25CA
        "blacklozenge" => ams_sym('⧫', Ord, AmsFamily::A, 0x07), // U+29EB
        "leftrightarrows" => ams_sym('⇆', Rel, AmsFamily::A, 0x1C), // U+21C6
        "rightleftarrows" => ams_sym('⇄', Rel, AmsFamily::A, 0x1D), // U+21C4
        "Lsh" => ams_sym('↰', Rel, AmsFamily::A, 0x1E),     // U+21B0
        "Rsh" => ams_sym('↱', Rel, AmsFamily::A, 0x1F),     // U+21B1
        "gtrapprox" => ams_sym('⪆', Rel, AmsFamily::A, 0x27), // U+2A86
        "lessapprox" => ams_sym('⪅', Rel, AmsFamily::A, 0x2F), // U+2A85
        "eqslantless" => ams_sym('⪕', Rel, AmsFamily::A, 0x30), // U+2A95
        "eqslantgtr" => ams_sym('⪖', Rel, AmsFamily::A, 0x31), // U+2A96
        "backprime" => ams_sym('‵', Ord, AmsFamily::A, 0x38), // U+2035
        "lesseqqgtr" => ams_sym('⪋', Rel, AmsFamily::A, 0x53), // U+2A8B
        "gtreqqless" => ams_sym('⪌', Rel, AmsFamily::A, 0x54), // U+2A8C
        "doublebarwedge" => ams_sym('⩞', Bin, AmsFamily::A, 0x5B), // U+2A5E
        "smallsmile" => ams_sym('⌣', Rel, AmsFamily::A, 0x60), // U+2323
        "smallfrown" => ams_sym('⌢', Rel, AmsFamily::A, 0x61), // U+2322
        "subseteqq" => ams_sym('⫅', Rel, AmsFamily::A, 0x6A), // U+2AC5
        "supseteqq" => ams_sym('⫆', Rel, AmsFamily::A, 0x6B), // U+2AC6
        "circledS" => ams_sym('Ⓢ', Ord, AmsFamily::A, 0x73), // U+24C8
        "lneq" => ams_sym('⪇', Rel, AmsFamily::B, 0x0C),    // U+2A87
        "gneq" => ams_sym('⪈', Rel, AmsFamily::B, 0x0D),    // U+2A88
        "precneqq" => ams_sym('⪵', Rel, AmsFamily::B, 0x16), // U+2AB5
        "succneqq" => ams_sym('⪶', Rel, AmsFamily::B, 0x17), // U+2AB6
        "precnapprox" => ams_sym('⪹', Rel, AmsFamily::B, 0x18), // U+2AB9
        "succnapprox" => ams_sym('⪺', Rel, AmsFamily::B, 0x19), // U+2ABA
        "lnapprox" => ams_sym('⪉', Rel, AmsFamily::B, 0x1A), // U+2A89
        "gnapprox" => ams_sym('⪊', Rel, AmsFamily::B, 0x1B), // U+2A8A
        "diagup" => ams_sym('╱', Ord, AmsFamily::B, 0x1E),  // U+2571
        "diagdown" => ams_sym('╲', Ord, AmsFamily::B, 0x1F), // U+2572
        "subsetneqq" => ams_sym('⫋', Rel, AmsFamily::B, 0x24), // U+2ACB
        "supsetneqq" => ams_sym('⫌', Rel, AmsFamily::B, 0x25), // U+2ACC
        "varsubsetneqq" => ams_sym('⫋', Rel, AmsFamily::B, 0x26), // U+2ACB, the variant form
        "varsupsetneqq" => ams_sym('⫌', Rel, AmsFamily::B, 0x27), // U+2ACC, the variant form
        "eth" => ams_sym('ð', Ord, AmsFamily::B, 0x67),     // U+00F0
        "succapprox" => ams_sym('⪸', Rel, AmsFamily::B, 0x76), // U+2AB8
        "precapprox" => ams_sym('⪷', Rel, AmsFamily::B, 0x77), // U+2AB7
        "curvearrowleft" => ams_sym('↶', Rel, AmsFamily::B, 0x78), // U+21B6
        "curvearrowright" => ams_sym('↷', Rel, AmsFamily::B, 0x79), // U+21B7
        "digamma" => ams_sym('ϝ', Ord, AmsFamily::B, 0x7A), // U+03DD
        "varkappa" => ams_sym('ϰ', Ord, AmsFamily::B, 0x7B), // U+03F0
        "backepsilon" => ams_sym('϶', Rel, AmsFamily::B, 0x7F), // U+03F6
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
