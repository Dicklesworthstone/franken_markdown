//! The error contract: precise, named, tier-tagged failures.
//!
//! The doctrine (franken_manim §11.5, R1): an unsupported construct is a
//! **precise, named error** — never silence, never garbage — and arbitrary
//! token streams error cleanly, never hang or garble. The `Display` formats
//! here are **standardized**: the public coverage ratchet parses construct
//! names out of [`MathError::UnsupportedCommand`] messages, so the message
//! shapes below are a stable contract locked by tests:
//!
//! - tier-2:   `` `\substack` is not yet supported; tier T2, tracked at … ``
//! - untiered: `` `\foo` is not supported; untiered (not observed in the
//!   G0-4 corpus), report at … ``
//! - malformed: `malformed mathematics at byte N: …`
//! - unmapped:  `` character 'x' (U+0078) has no glyph in the bundled math
//!   faces … ``
//! - budget:    `` resource budget `nesting` exceeded at byte N: more than
//!   64 nesting levels ``
//!
//! # Resource budgets (franken_manim §16.5, R14)
//!
//! The Studio and the Python portal hand user-controlled strings to this
//! crate, so every unbounded quantity has a cap and every breach is a
//! [`MathError::Budget`] naming its [`BudgetKind`] and limit:
//!
//! | kind | default | where |
//! |---|---:|---|
//! | `input-bytes` | 65 536 bytes | before lexing; [`Budgets`], per engine |
//! | `nesting` | 64 levels | groups, arguments, `\left…\right`, environments, islands |
//! | `macro-work` | 65 536 token work units | macro expansion |
//! | `macro-nesting` | 32 levels | macro-within-macro expansion |
//! | `glyphs` | 65 536 placed glyphs | each [`Layout`](crate::Layout); [`Budgets`] |
//! | `path-segments` | 262 144 drawn segments | each [`Layout`](crate::Layout); [`Budgets`] |
//!
//! The nesting and macro caps are fixed: the nesting bound is what keeps
//! the recursive parser and layout inside a 2 MiB thread stack. The longest
//! string in the G0-4 corpus is 1 060 bytes, so the defaults never bind real
//! formulas.

use crate::commands::{
    ConstructStatus, LAYOUT_PENDING_TRACKING, UNTIERED_TRACKING, construct_status, tier2_tracking,
};
use crate::node::Span;

/// The resource a [`MathError::Budget`] breach exhausted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BudgetKind {
    /// Source bytes of one string ([`Budgets::max_input_bytes`]).
    InputBytes,
    /// Nesting depth of groups, arguments, `\left…\right` pairs,
    /// environments, and islands.
    Nesting,
    /// Token-copy and substitution work of macro expansion.
    MacroWork,
    /// Depth of macro-within-macro expansion.
    MacroNesting,
    /// Placed glyphs in one layout ([`Budgets::max_glyphs`]).
    Glyphs,
    /// Segments of the drawn paths in one layout
    /// ([`Budgets::max_path_segments`]).
    PathSegments,
}

impl BudgetKind {
    /// The stable name in [`MathError::Budget`] messages.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::InputBytes => "input-bytes",
            Self::Nesting => "nesting",
            Self::MacroWork => "macro-work",
            Self::MacroNesting => "macro-nesting",
            Self::Glyphs => "glyphs",
            Self::PathSegments => "path-segments",
        }
    }

    /// What the limit counts, plural, as the message prints it.
    const fn unit(self) -> &'static str {
        match self {
            Self::InputBytes => "source bytes",
            Self::Nesting => "nesting levels",
            Self::MacroWork => "token work units",
            Self::MacroNesting => "macro expansion levels",
            Self::Glyphs => "placed glyphs",
            Self::PathSegments => "drawn path segments",
        }
    }
}

/// The configurable resource budgets of one typeset call (see the module
/// docs for the fixed ones). [`Engine::with_budgets`](crate::Engine::with_budgets)
/// sets them per engine; the free parse functions use [`Budgets::DEFAULT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    /// Most source bytes one string may have.
    pub max_input_bytes: usize,
    /// Most glyphs one layout may place.
    pub max_glyphs: usize,
    /// Most segments the drawn paths of one layout may have.
    pub max_path_segments: usize,
}

impl Budgets {
    /// The defaults: 64 KiB of source, 65 536 glyphs, 262 144 path segments.
    pub const DEFAULT: Self = Self {
        max_input_bytes: 1 << 16,
        max_glyphs: 1 << 16,
        max_path_segments: 1 << 18,
    };
}

impl Default for Budgets {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a source string failed to parse (or, later, to lay out).
///
/// The G0-3 ratification note froze three variants, with the construct name
/// of an unsupported command carried verbatim in the construct table's
/// naming scheme (`\substack`, `env:flushleft`, …). [`MathError::Budget`]
/// joined them for the resource budgets (franken_manim
/// fm-fmd-math-fuzz-budgets-k8fe): a string too large to typeset is not
/// malformed, and a consumer can tell the two apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathError {
    /// A construct outside the implemented surface. `name` is in the G0-4
    /// construct-table scheme (`\substack`, `env:flushleft`); the tier tag
    /// in the rendered message comes from the registry.
    UnsupportedCommand {
        /// Construct name, table scheme.
        name: String,
        /// Source span of the offending construct.
        span: Span,
    },
    /// Structurally invalid input (unbalanced braces, a double superscript,
    /// `\right` without `\left`, …).
    Malformed {
        /// What is wrong, in one clause.
        what: String,
        /// Byte offset of the offense.
        at: usize,
    },
    /// A character with no glyph in the bundled math faces (a layout-stage
    /// error; the parser accepts every character).
    UnmappedChar {
        /// The character.
        ch: char,
        /// Its source span.
        span: Span,
    },
    /// A resource budget tripped (see the module docs' table).
    Budget {
        /// Which budget.
        kind: BudgetKind,
        /// Its limit, in the kind's unit.
        limit: usize,
        /// Byte offset of the offense: the first byte past the input
        /// budget, the opener one level too deep, the macro call, or the
        /// source of the first primitive past an output budget.
        at: usize,
    },
}

impl MathError {
    /// The byte span the error points at.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::UnsupportedCommand { span, .. } | Self::UnmappedChar { span, .. } => *span,
            Self::Malformed { at, .. } | Self::Budget { at, .. } => Span::new(*at, *at),
        }
    }

    /// For [`MathError::UnsupportedCommand`], the construct name in the
    /// construct-table scheme; the ratchet counts by this.
    #[must_use]
    pub fn unsupported_construct(&self) -> Option<&str> {
        match self {
            Self::UnsupportedCommand { name, .. } => Some(name),
            _ => None,
        }
    }
}

impl core::fmt::Display for MathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedCommand { name, span } => match construct_status(name) {
                ConstructStatus::UnsupportedT2 => write!(
                    f,
                    "`{name}` is not yet supported; tier T2, tracked at {} (bytes {}..{})",
                    tier2_tracking(name),
                    span.start,
                    span.end
                ),
                ConstructStatus::Supported => write!(
                    f,
                    "`{name}` parses, but its layout is not yet implemented; tracked at \
                     {LAYOUT_PENDING_TRACKING} (bytes {}..{})",
                    span.start, span.end
                ),
                ConstructStatus::Unknown => write!(
                    f,
                    "`{name}` is not supported; untiered (not observed in the G0-4 corpus), \
                     report at {UNTIERED_TRACKING} (bytes {}..{})",
                    span.start, span.end
                ),
            },
            Self::Malformed { what, at } => {
                write!(f, "malformed mathematics at byte {at}: {what}")
            }
            Self::UnmappedChar { ch, span } => write!(
                f,
                "character '{ch}' (U+{:04X}) has no glyph in the bundled math faces \
                 (bytes {}..{})",
                *ch as u32, span.start, span.end
            ),
            Self::Budget { kind, limit, at } => write!(
                f,
                "resource budget `{}` exceeded at byte {at}: more than {limit} {}",
                kind.name(),
                kind.unit()
            ),
        }
    }
}

impl std::error::Error for MathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier2_message_format_is_stable() {
        // The example must stay a construct the T2 vocabulary still names
        // as pending (`\substack` and `\dddot` graduated).
        // `\dx` is the last pending construct — a 3b1b preamble macro
        // awaiting the corpus preamble evidence.
        let err = MathError::UnsupportedCommand {
            name: r"\dx".to_owned(),
            span: Span::new(3, 6),
        };
        assert_eq!(
            err.to_string(),
            "`\\dx` is not yet supported; tier T2, tracked at franken_manim fm-j5t \
             (the tier-2 construct program) (bytes 3..6)"
        );
    }

    #[test]
    fn untiered_message_format_is_stable() {
        let err = MathError::UnsupportedCommand {
            name: r"\notacommand".to_owned(),
            span: Span::new(0, 12),
        };
        assert_eq!(
            err.to_string(),
            "`\\notacommand` is not supported; untiered (not observed in the G0-4 corpus), \
             report at https://github.com/Dicklesworthstone/franken_manim/issues (bytes 0..12)"
        );
    }

    #[test]
    fn malformed_message_format_is_stable() {
        let err = MathError::Malformed {
            what: "unmatched '}'".to_owned(),
            at: 7,
        };
        assert_eq!(
            err.to_string(),
            "malformed mathematics at byte 7: unmatched '}'"
        );
    }

    #[test]
    fn budget_message_format_is_stable() {
        let cases = [
            (
                BudgetKind::InputBytes,
                65_536,
                65_536,
                "resource budget `input-bytes` exceeded at byte 65536: \
                 more than 65536 source bytes",
            ),
            (
                BudgetKind::Nesting,
                64,
                64,
                "resource budget `nesting` exceeded at byte 64: more than 64 nesting levels",
            ),
            (
                BudgetKind::MacroWork,
                65_536,
                3,
                "resource budget `macro-work` exceeded at byte 3: \
                 more than 65536 token work units",
            ),
            (
                BudgetKind::MacroNesting,
                32,
                0,
                "resource budget `macro-nesting` exceeded at byte 0: \
                 more than 32 macro expansion levels",
            ),
            (
                BudgetKind::Glyphs,
                65_536,
                9,
                "resource budget `glyphs` exceeded at byte 9: more than 65536 placed glyphs",
            ),
            (
                BudgetKind::PathSegments,
                262_144,
                12,
                "resource budget `path-segments` exceeded at byte 12: \
                 more than 262144 drawn path segments",
            ),
        ];
        for (kind, limit, at, message) in cases {
            let err = MathError::Budget { kind, limit, at };
            assert_eq!(err.to_string(), message);
            assert_eq!(err.span(), Span::new(at, at));
            assert_eq!(err.unsupported_construct(), None);
        }
    }

    #[test]
    fn default_budgets_are_the_documented_values() {
        assert_eq!(
            Budgets::default(),
            Budgets {
                max_input_bytes: 65_536,
                max_glyphs: 65_536,
                max_path_segments: 262_144,
            }
        );
    }
}
