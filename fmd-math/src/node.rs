//! The parse tree: [`Node`] and its supporting vocabulary.
//!
//! Every node carries its **byte span** into the source string — span
//! provenance is structural (the G0-3 ratification's §11.3 requirement), not
//! an afterthought: the span map that downstream consumers (`isolate`,
//! `tex_to_color_map`, `TransformMatchingTex`) use is derived from these
//! spans, so no node may ever be constructed without one.

use crate::atom::AtomClass;
use crate::error::MathError;
use crate::mbox::Color;
use crate::style::Style;

/// A half-open byte range `[start, end)` into the source string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// Byte offset of the first byte of the construct.
    pub start: usize,
    /// Byte offset one past the last byte of the construct.
    pub end: usize,
}

impl Span {
    /// Construct a span. `start` and `end` are byte offsets; `end >= start`.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The smallest span covering both `self` and `other`.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// Length in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// True when the span covers zero bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// One parse-tree node: a kind plus the byte span it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// What the node is.
    pub kind: NodeKind,
    /// Where in the source string it came from.
    pub span: Span,
}

impl Node {
    /// Construct a node.
    #[must_use]
    pub const fn new(kind: NodeKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// A delimiter as named after `\left`, `\right`, or a `\big`-class command.
///
/// `ch` is `None` for the null delimiter `.` (as in `\right.`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delim {
    /// The delimiter character (already mapped to its math codepoint, e.g.
    /// `\langle` ⇒ `⟨`), or `None` for the null delimiter.
    pub ch: Option<char>,
    /// Source span of the delimiter token.
    pub span: Span,
}

/// The four fixed delimiter sizes of the `\big` family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DelimSize {
    /// `\big` class — 8.5 pt-per-10 pt nominal.
    Big,
    /// `\Big` class.
    BBig,
    /// `\bigg` class.
    Bigg,
    /// `\Bigg` class.
    BBigg,
}

/// How a big operator places its scripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Limits {
    /// TeX's default: limits in display style for `\sum`-class operators,
    /// side scripts otherwise; `\int`-class operators default to side
    /// scripts in every style.
    #[default]
    Default,
    /// `\limits`: scripts above/below regardless of style.
    Limits,
    /// `\nolimits`: side scripts regardless of style.
    NoLimits,
}

/// The generalized-fraction flavors (rule 15's inputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FracSpec {
    /// Draw the fraction bar.
    pub bar: bool,
    /// Delimiters wrapped around the whole fraction (`\binom`/`\choose`
    /// carry `( )`); `None` for plain fractions.
    pub delims: Option<(char, char)>,
    /// A forced layout style (`\dfrac` forces display, `\tfrac` text);
    /// `None` follows the ambient style.
    pub forced_style: Option<Style>,
    /// An explicit bar thickness (`\genfrac`'s third argument, TeX's
    /// `\above`); `None` is the font's default rule. Read only with `bar`.
    pub thickness: Option<Length>,
    /// amsmath's `\cfrac`, with its numerator's alignment: a `\strut` in
    /// the numerator, `\hfill` beside it for `[l]` and `[r]`, and
    /// `\kern-\nulldelimiterspace` after the fraction. `None` otherwise.
    pub cfrac: Option<LineAlign>,
}

impl FracSpec {
    /// `\frac` and `\over`: a barred fraction in the ambient style.
    pub const PLAIN: Self = Self {
        bar: true,
        delims: None,
        forced_style: None,
        thickness: None,
        cfrac: None,
    };
    /// `\binom` and `\choose`: no bar, wrapped in parentheses.
    pub const BINOM: Self = Self {
        bar: false,
        delims: Some(('(', ')')),
        ..Self::PLAIN
    };
}

/// The accent commands (both true accents and the wide over/under class).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccentKind {
    /// `\hat`
    Hat,
    /// `\check`
    Check,
    /// `\tilde`
    Tilde,
    /// `\acute`
    Acute,
    /// `\grave`
    Grave,
    /// `\dot`
    Dot,
    /// `\ddot`
    Ddot,
    /// `\breve`
    Breve,
    /// `\bar`
    Bar,
    /// `\vec`
    Vec,
    /// `\dddot` — amsmath builds it from three dot marks in a row; so does
    /// the layout here (no bundled face carries U+20DB).
    Dddot,
    /// `\ddddot` — four dot marks, same construction.
    Ddddot,
    /// `\mathring`
    Ring,
    /// `\widehat`
    WideHat,
    /// `\widetilde`
    WideTilde,
    /// `\overline`
    OverLine,
    /// `\underline` (math mode; in text mode `\underline` is a
    /// [`TextStyle::Underline`] island)
    UnderLine,
    /// `\overbrace` (annotations attach as scripts on the wrapping
    /// [`NodeKind::Scripts`] node, exactly as TeX attaches them)
    OverBrace,
    /// `\underbrace`
    UnderBrace,
    /// `\overrightarrow`
    OverRightArrow,
    /// `\overleftarrow`
    OverLeftArrow,
}

impl AccentKind {
    /// True for accents that sit above the base (everything except the
    /// under-class accents).
    #[must_use]
    pub const fn is_over(self) -> bool {
        !matches!(self, Self::UnderLine | Self::UnderBrace)
    }
}

/// The text-mode styling islands of the TexText contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyle {
    /// `\textbf{…}`
    Bold,
    /// `\emph{…}`
    Emph,
    /// `\underline{…}` in text mode
    Underline,
    /// `\textit{…}`
    Italic,
    /// `\textrm{…}`, `\textup{…}` and `\textnormal{…}`
    Roman,
    /// `\texttt{…}`
    Typewriter,
    /// `\textsf{…}`
    SansSerif,
}

/// The argument-taking math alphabet commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathFont {
    /// `\mathbb` (and `\mathds`, which the default preamble pack maps here)
    Blackboard,
    /// `\mathcal`
    Calligraphic,
    /// `\mathscr` (mathrsfs, which the Reference's default template loads)
    Script,
    /// `\mathfrak` (amssymb's Euler Fraktur)
    Fraktur,
    /// `\mathrm`
    Roman,
    /// `\mathbf`
    Bold,
    /// `\boldsymbol`
    BoldItalic,
    /// `\mathsf`
    SansSerif,
    /// `\mathtt`
    Typewriter,
    /// `\mathit`
    Italic,
}

/// The explicit spacing commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceKind {
    /// `\,` — 3 mu
    Thin,
    /// `\:` — 4 mu
    Med,
    /// `\;` — 5 mu
    Thick,
    /// `\!` — −3 mu
    NegThin,
    /// `\quad` — 18 mu (1 em)
    Quad,
    /// `\qquad` — 36 mu (2 em)
    Qquad,
    /// `\ ` (control space) — an ordinary interword space
    ControlSpace,
}

impl SpaceKind {
    /// The width in eighteenths of a quad. For the `\mskip` spaces
    /// ([`Self::is_mu_glue`]) these are math units; `\quad` and `\qquad`
    /// are `\hskip` ems of the text font, and the control space is its
    /// interword space, close to 6 of them.
    #[must_use]
    pub const fn mu(self) -> i32 {
        match self {
            Self::Thin => 3,
            Self::Med => 4,
            Self::Thick => 5,
            Self::NegThin => -3,
            Self::Quad => 18,
            Self::Qquad => 36,
            Self::ControlSpace => 6,
        }
    }

    /// Whether math mode sets this space as `\mskip` glue in mu, which
    /// follows the math style's size (`\,` `\:` `\;` `\!`), rather than as
    /// text-font glue, which does not.
    #[must_use]
    pub const fn is_mu_glue(self) -> bool {
        matches!(self, Self::Thin | Self::Med | Self::Thick | Self::NegThin)
    }
}

/// The cancel package's strike-throughs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelKind {
    /// `\cancel`: one stroke rising left to right.
    Rising,
    /// `\bcancel`: one stroke falling left to right.
    Falling,
    /// `\xcancel`: both.
    Cross,
}

/// The kernel's logos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogoKind {
    /// `\TeX`: `T\kern-.1667em\lower.5ex\hbox{E}\kern-.125em X`.
    TeX,
    /// `\LaTeX`: L, a script-size A top-aligned with T, then `\TeX`.
    LaTeX,
}

/// What an explicit [`Length`] counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LengthUnit {
    /// A fixed length (pt, pc, in, bp, cm, mm, dd, cc, sp), converted to
    /// scaled points when parsed.
    Fixed,
    /// Ems of the text font, its quad.
    Em,
    /// Exes of the text font, its x-height.
    Ex,
}

/// An explicit length, as TeX's `scan_dimen` reads one: the value times
/// 65536, so a fixed length is in scaled points (65536 sp = 1 pt), and a
/// font-relative one counts 65536ths of an em or ex, which layout reads
/// from the text font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Length {
    /// The value times 65536.
    pub sp: i64,
    /// What the value counts.
    pub unit: LengthUnit,
}

impl Length {
    /// Parse a TeX length: an optional sign, a decimal number (`.` or `,`
    /// as the decimal mark) and a unit, with optional spaces between.
    /// `None` for anything else, including rubber lengths (`\fill`,
    /// `\stretch{…}`) and `mu`, which `\hspace` does not take.
    #[must_use]
    pub fn parse(source: &str) -> Option<Self> {
        let s = source.trim();
        let mut negative = false;
        let mut rest = s;
        while let Some(c) = rest.chars().next() {
            match c {
                '-' => negative = !negative,
                '+' => {}
                _ if c.is_whitespace() => {}
                _ => break,
            }
            rest = &rest[c.len_utf8()..];
        }
        let digits = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
            .unwrap_or(rest.len());
        let (number, unit) = rest.split_at(digits);
        let number = number.replace(',', ".");
        if number.is_empty() || number == "." || number.matches('.').count() > 1 {
            return None;
        }
        let value: f64 = number.parse().ok()?;
        let unit = unit.trim();
        let unit = unit.strip_prefix("true").map_or(unit, str::trim_start);
        // Each unit in points, as tex.web §458 converts them.
        let (points, kind) = match unit {
            "pt" => (1.0, LengthUnit::Fixed),
            "pc" => (12.0, LengthUnit::Fixed),
            "in" => (72.27, LengthUnit::Fixed),
            "bp" => (72.27 / 72.0, LengthUnit::Fixed),
            "cm" => (72.27 / 2.54, LengthUnit::Fixed),
            "mm" => (72.27 / 25.4, LengthUnit::Fixed),
            "dd" => (1238.0 / 1157.0, LengthUnit::Fixed),
            "cc" => (14856.0 / 1157.0, LengthUnit::Fixed),
            "sp" => (1.0 / 65536.0, LengthUnit::Fixed),
            "em" => (1.0, LengthUnit::Em),
            "ex" => (1.0, LengthUnit::Ex),
            _ => return None,
        };
        let scaled = (value * points * 65536.0).round();
        // TeX's \maxdimen, 2^30 - 1 sp: past it, "Dimension too large".
        if !scaled.is_finite() || scaled > f64::from(0x3FFF_FFFF_u32) {
            return None;
        }
        #[allow(clippy::cast_possible_truncation)]
        let sp = scaled as i64;
        Some(Self {
            sp: if negative { -sp } else { sp },
            unit: kind,
        })
    }
}

/// The phantom flavors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhantomKind {
    /// `\phantom` — occupies width, height, and depth.
    Full,
    /// `\hphantom` — occupies width only.
    Horizontal,
    /// `\vphantom` — occupies height and depth only.
    Vertical,
}

/// The `\stackrel`/`\overset`/`\underset` family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackKind {
    /// `\stackrel{top}{base}` — the result is a Rel atom.
    Stackrel,
    /// `\overset{top}{base}` — the result takes the base's class.
    Overset,
    /// `\underset{bottom}{base}` — the result takes the base's class.
    Underset,
}

/// A `\color` / `\textcolor` argument.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorSpec {
    /// What MathML's `mathcolor` carries: the argument verbatim, as
    /// KaTeX-flavoured HTML expects (`steelblue`, `#00ff00`), or `#rrggbb`
    /// for an explicit `[model]`.
    pub spec: String,
    /// xcolor's resolution (`crate::color`), or the precise error that
    /// layout, which needs real RGB, reports.
    pub color: Result<Color, MathError>,
}

/// The two AMS symbol families whose TeX code positions identify authored glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmsFamily {
    /// AMS-A (`msam`): checkmark, shortminus and circle arrows.
    A,
    /// AMS-B (`msbm`): barred h, negated divisibility and blackboard capitals.
    B,
}

/// What a node is. See the module docs; every variant is produced by
/// [`crate::parse`] / [`crate::parse_text`] with full span provenance.
#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    /// A horizontal list: a group's content, a cell, an argument, or the
    /// whole formula.
    List(Vec<Node>),
    /// A single character atom, already mapped to its math codepoint
    /// (`-` ⇒ `−`, `*` ⇒ `∗`, `\pi` ⇒ `π`). `class` is the intrinsic atom
    /// class before contextual Bin→Ord degradation.
    Symbol {
        /// The (mapped) character.
        ch: char,
        /// Intrinsic atom class.
        class: AtomClass,
    },
    /// A named symbol whose TeX font/code is part of its design. Keep the
    /// semantic character for MathML and provenance: shortminus and ordinary
    /// subtraction share Unicode but have different authored glyphs.
    AmsSymbol {
        /// Semantic character.
        ch: char,
        /// Intrinsic atom class.
        class: AtomClass,
        /// Authored AMS symbol family.
        family: AmsFamily,
        /// TeX code position in that family.
        code: u8,
    },
    /// A big operator (`\sum`, `\int`, …): an Op atom with a limits mode.
    BigOp {
        /// The operator character (`∑`, `∫`, …).
        ch: char,
        /// `\limits`/`\nolimits` state.
        limits: Limits,
        /// True for the `\int` class, whose default is side scripts even
        /// in display style.
        integral: bool,
    },
    /// A roman operator name (`\sin`, `\lim`, `\operatorname{…}`): an Op
    /// atom set in upright text.
    OpName {
        /// The rendered name ("sin", "lim", …).
        name: String,
        /// True for the `\lim` class, which takes under/over scripts in
        /// display style.
        limits: bool,
    },
    /// Sub/superscripts and primes attached to a base atom. `base` is
    /// `None` when the script opens the list (TeX's empty-nucleus atom).
    Scripts {
        /// The atom the scripts attach to.
        base: Option<Box<Node>>,
        /// Subscript.
        sub: Option<Box<Node>>,
        /// Superscript (primes precede it visually).
        sup: Option<Box<Node>>,
        /// The `'` primes, one source span each.
        primes: Vec<Span>,
    },
    /// A generalized fraction: `\frac`-family, `\binom`/`\choose`, or an
    /// infix `\over` that split its enclosing list.
    Frac {
        /// Numerator.
        num: Box<Node>,
        /// Denominator.
        den: Box<Node>,
        /// Bar/delimiter/style flavor.
        spec: FracSpec,
    },
    /// `\sqrt`, with an optional index (`\sqrt[3]{x}`).
    Radical {
        /// The index, if any.
        index: Option<Box<Node>>,
        /// The radicand.
        radicand: Box<Node>,
    },
    /// An accented atom.
    Accent {
        /// Which accent.
        accent: AccentKind,
        /// The base.
        base: Box<Node>,
    },
    /// `\left … \right`: an Inner atom.
    LeftRight {
        /// Opening delimiter.
        left: Delim,
        /// Closing delimiter.
        right: Delim,
        /// The enclosed list.
        body: Vec<Node>,
    },
    /// A fixed-size delimiter (`\big(`, `\Big\{`, …).
    SizedDelim {
        /// Which size.
        size: DelimSize,
        /// The atom class the variant imposes: `\bigl` ⇒ Open, `\bigr` ⇒
        /// Close, `\bigm` ⇒ Rel, plain `\big` ⇒ Ord.
        class: AtomClass,
        /// The delimiter.
        delim: Delim,
    },
    /// `\text{…}` inside mathematics: the body is text-mode content.
    Text {
        /// Text-mode body.
        body: Vec<Node>,
    },
    /// A literal run of text-mode characters, with one source span per
    /// character (decoded characters and source bytes are not linear:
    /// escapes decode two bytes to one character, whitespace collapses) —
    /// the provenance `Text[3:7]`-style slicing consumes.
    TextRun {
        /// The decoded text.
        text: String,
        /// One span per `char` of `text`, in order.
        char_spans: Vec<Span>,
    },
    /// A `\textbf`/`\emph`/`\underline` styling island (text mode, or the
    /// LaTeX text-in-math form).
    TextStyled {
        /// Which style.
        style: TextStyle,
        /// The body, in text mode.
        body: Vec<Node>,
    },
    /// `$…$` (or `$$…$$`) inside text mode: the body is math-mode
    /// content.
    MathIsland {
        /// Math-mode body.
        body: Vec<Node>,
        /// True for `$$…$$` display mathematics (lays out in display
        /// style); false for inline `$…$` (text style).
        display: bool,
    },
    /// A style-switch marker (`\displaystyle` …) applying to the remainder
    /// of the enclosing list.
    StyleChange(Style),
    /// A line-alignment declaration (`\centering`): applies to the
    /// remainder of the enclosing list, exiting at group end, exactly like
    /// LaTeX's paragraph declarations. Produces no glyphs of its own.
    AlignChange(LineAlign),
    /// A size declaration (`\small`, `\Large`, …): sets the current size
    /// factor for the remainder of the enclosing list, exiting at group
    /// end, and composing multiplicatively with the script styles (the
    /// LaTeX 10 pt ladder of `size10.clo`).
    SizeChange(f64),
    /// A `\color{…}` marker applying to the remainder of the enclosing
    /// group; `\textcolor` is a group opening with one.
    ColorChange(ColorSpec),
    /// A line-spacing declaration (`\doublespacing`): multiplies the
    /// \baselineskip of subsequent `\\`-stacked lines in the enclosing
    /// multi-line list (setspace's \baselinestretch, 10 pt ladder value).
    /// Produces no glyphs; inert inside grids, whose row rule is the
    /// environment's own.
    LineSpacing(f64),
    /// A math-alphabet command applied to one argument.
    MathFont {
        /// Which alphabet.
        font: MathFont,
        /// The argument.
        body: Box<Node>,
    },
    /// A phantom box.
    Phantom {
        /// Which dimensions it occupies.
        kind: PhantomKind,
        /// The hidden body.
        body: Box<Node>,
    },
    /// amsmath's `\boxed{…}`: `\fbox{$\displaystyle…$}`, the body in
    /// display style inside a 0.4 pt frame 3 pt clear of it.
    Boxed {
        /// The framed body.
        body: Box<Node>,
    },
    /// The cancel package's `\cancel`, `\bcancel` and `\xcancel`: the body
    /// in the style in force, struck through by 0.4 pt line-font strokes.
    Cancel {
        /// Which strokes.
        kind: CancelKind,
        /// The struck body.
        body: Box<Node>,
    },
    /// `\stackrel`/`\overset`/`\underset`.
    Stack {
        /// Which flavor.
        kind: StackKind,
        /// The small stacked element (top for stackrel/overset, bottom for
        /// underset).
        annotation: Box<Node>,
        /// The base.
        base: Box<Node>,
    },
    /// `\xrightarrow[below]{above}` / `\xmapsto[below]{above}`: a drawn
    /// arrow stretched to its script-style labels, spaced as a relation.
    XArrow {
        /// True for `\xmapsto` (the origin-bar band).
        mapsto: bool,
        /// The mandatory above-label (a `List` node).
        above: Box<Node>,
        /// The optional below-label (a `List` node).
        below: Option<Box<Node>>,
    },
    /// An explicit spacing command.
    Space(SpaceKind),
    /// `\hspace{…}` and `\hspace*{…}`: horizontal glue of an explicit
    /// length, with no stretch or shrink.
    Skip(Length),
    /// The kernel's `\TeX` and `\LaTeX` logos.
    Logo(LogoKind),
    /// TeX's `\mathchoice{D}{T}{S}{SS}`: the branch for the style in force.
    MathChoice {
        /// The display, text, script and scriptscript branches.
        branches: Box<[Node; 4]>,
    },
    /// `~` — a tie (non-breaking interword space).
    Tie,
    /// `\\` — a line break (or, at an environment's own level, the row
    /// separator, in which case it is consumed by the environment).
    Linebreak,
    /// `&` — an alignment tab (or, at an environment's own level, the cell
    /// separator, in which case it is consumed by the environment). Kept as
    /// a node at the top level because the Tex surface wraps whole strings
    /// in an `align*`-class environment.
    AlignTab,
    /// A line-alignment environment (`flushleft`, `center`, `flushright`):
    /// a text-mode block whose `\\`-split lines are aligned within the
    /// widest line's width. Lines are [`NodeKind::List`] nodes.
    AlignBlock {
        /// The line alignment.
        align: LineAlign,
        /// The lines, in order.
        lines: Vec<Node>,
    },
    /// A `\begin{name} … \end{name}` environment. Cells are
    /// [`NodeKind::List`] nodes.
    Environment {
        /// Environment name (`array`, `cases`, …).
        name: String,
        /// The column-spec argument (`array` only), kept verbatim.
        spec: Option<String>,
        /// Rows of cells.
        rows: Vec<Vec<Node>>,
    },
    /// A structural fragment: the Tex surface's multi-argument idiom makes
    /// each literal argument its own corpus string, so a piece may be a
    /// *substring of a balanced whole* (`"{a"`, `"b}"`, `"\right)"`). The
    /// grammar accepts these at the top level and marks them explicitly —
    /// never silently.
    Fragment(FragmentKind),
}

/// How the lines of a multi-line block are aligned horizontally within
/// the block's width (the widest line's width).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineAlign {
    /// `flushleft` (and the default): every line flush left.
    #[default]
    Left,
    /// `center` / `\centering`: every line centered.
    Center,
    /// `flushright`: every line flush right.
    Right,
}

impl LineAlign {
    /// The fraction of a line's slack placed to its left: `0` flush left,
    /// `1/2` centered, `1` flush right.
    #[must_use]
    pub fn slack_factor(self) -> f64 {
        match self {
            Self::Left => 0.0,
            Self::Center => 0.5,
            Self::Right => 1.0,
        }
    }
}

/// The structural fragments the top level tolerates (per-argument
/// `SingleStringTex` semantics).
#[derive(Clone, Debug, PartialEq)]
pub enum FragmentKind {
    /// An unmatched `}` whose opener lives in an earlier piece. Transparent
    /// to classification and spacing; renders nothing.
    UnmatchedClose,
    /// A `\right` whose `\left` lives in an earlier piece; renders its
    /// delimiter (class Close).
    StrayRight(Delim),
    /// A redundant `$` in a math-mode string (authors wrapping an
    /// already-math string in dollars). Transparent; renders nothing.
    RedundantMathShift,
}
