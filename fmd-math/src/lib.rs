//! # fmd-math — the clean-room TeX-mathematics layout engine
//!
//! A math-*layout* engine in the KaTeX/Typst class, not a TeX macro
//! processor: the TeX mathematics grammar as actually used, TeX's published
//! layout rules (the eight atom classes, the inter-atom spacing table,
//! display/text/script/scriptscript style propagation), and — with the
//! placement beads — Appendix-G construction mathematics over metrics
//! calibrated for the bundled faces.
//!
//! This crate is contributed and consumed by the franken_manim program (its
//! Scribe subsystem typesets `Tex`/`TexText` through it; see that repo's
//! `UPSTREAM_LEDGER.md` row 2) and serves fmd's own native `$…$`
//! mathematics. The public API is **frozen to the shape recorded in
//! franken_manim's `docs/g0/G0-3-fmd-math-ratification.md`** until its G2
//! gate.
//!
//! ## The pipeline
//!
//! ```text
//! source &str
//!   → parse           tokens → Node tree; EVERY node carries its byte span
//!   → classify        the eight atom classes; Bin→Ord degradation in context
//!   → layout(Ctx)     style (D/T/S/SS × cramped) threaded top-down;
//!                     Appendix-G constructions build boxes bottom-up
//!   → Layout          positioned {glyphs, rules, drawn paths}, each glyph
//!                     naming its FACE and carrying its source span
//! ```
//!
//! This crate release carries the front of the pipeline: [`parse`] /
//! [`parse_text`], the atom engine ([`atom`]), the style machinery
//! ([`style`]), and the fixed node model plus output types ([`Layout`] and
//! friends). `Engine::new(faces…)` / `typeset` land with the placement
//! bead on exactly these shapes.
//!
//! ## Modes
//!
//! [`parse`] reads a whole string as mathematics (the `Tex` surface; `&`
//! and `\\` are legal at top level because the Reference wraps whole
//! strings in an `align*`-class environment). [`parse_text`] reads the
//! TexText contract: a text mainland with `$…$` math islands,
//! `\textbf`/`\emph`/`\underline`, and the escape set.
//!
//! ## Fragment tolerance (per-argument `SingleStringTex` semantics)
//!
//! The Tex surface's multi-argument idiom makes each literal argument its
//! own string, so a string may legitimately be a *piece of a balanced
//! whole* (`"{a"`, `"\over"`, `"b}"`, `"\left("`, `"a^"`). The grammar
//! therefore (a) lets **end of input close whatever is open** — groups,
//! `\left`, `$` islands, arguments still to come (they become empty
//! lists) — and (b) accepts, at the top level, an unmatched `}`, a stray
//! `\right`, or a redundant `$` as an explicit
//! [`node::FragmentKind`] marker, never silently. Mid-string structural
//! faults (wrong closer, double scripts, `&` in prose) remain precise
//! errors. In text mode, the Reference-era LaTeX missing-`$` recovery is
//! kept deliberately: a self-contained math command or a bare script in
//! the mainland becomes an explicit implicit [`NodeKind::MathIsland`],
//! and `$$…$$` display mathematics is recognized.
//!
//! ## The error contract (the coverage ratchet's unit)
//!
//! There is deliberately no fallback typesetter, so coverage discipline
//! replaces fallback discipline: an unsupported construct is a **precise,
//! named error** ([`MathError::UnsupportedCommand`] with the construct's
//! G0-4 table name and a tier tag in its message), and arbitrary input
//! errors cleanly — never hangs, never garbles, never panics (the chaos
//! suite locks this). [`construct_status`] answers, for any construct in
//! the table's naming scheme, whether the parse surface supports it.

#![forbid(unsafe_code)]

pub mod atom;
pub mod commands;
mod error;
mod mbox;
pub mod node;
mod parse;
pub mod style;
mod token;

pub use commands::{ConstructStatus, TIER2_TRACKING, Tier, UNTIERED_TRACKING, construct_status};
pub use error::MathError;
pub use mbox::{FaceId, Layout, PathContour, PathSeg, PlacedGlyph, PlacedPath, PlacedRule};
pub use node::{Node, NodeKind, Span};
pub use style::{Style, StyleCtx, style_walk};

/// Parse a whole source string as mathematics (the `Tex` surface). The
/// result is a [`NodeKind::List`] node spanning the whole input; every
/// descendant carries its byte span.
///
/// # Errors
///
/// [`MathError::UnsupportedCommand`] for constructs outside the implemented
/// surface (named precisely, tier-tagged); [`MathError::Malformed`] for
/// structural faults (unbalanced groups, double scripts, stray `\right`,
/// over-deep nesting), with the byte offset of the offense.
pub fn parse(source: &str) -> Result<Node, MathError> {
    parse::parse_math(source)
}

/// Parse a whole source string under the TexText contract: a text mainland
/// with `$…$` math islands.
///
/// # Errors
///
/// As [`parse`]; additionally, math-only material in the text mainland
/// (`^`, `_`, `&`, math-mode commands) is a precise [`MathError::Malformed`]
/// telling the caller to wrap it in `$…$`.
pub fn parse_text(source: &str) -> Result<Node, MathError> {
    parse::parse_text_mode(source)
}
