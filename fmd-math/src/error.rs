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

use crate::commands::{
    ConstructStatus, LAYOUT_PENDING_TRACKING, TIER2_TRACKING, UNTIERED_TRACKING, construct_status,
};
use crate::node::Span;

/// Why a source string failed to parse (or, later, to lay out).
///
/// The shape is frozen by the G0-3 ratification note: three variants, with
/// the construct name of an unsupported command carried verbatim in the
/// construct table's naming scheme (`\substack`, `env:flushleft`, …).
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
}

impl MathError {
    /// The byte span the error points at.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::UnsupportedCommand { span, .. } | Self::UnmappedChar { span, .. } => *span,
            Self::Malformed { at, .. } => Span::new(*at, *at),
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
                    "`{name}` is not yet supported; tier T2, tracked at {TIER2_TRACKING} \
                     (bytes {}..{})",
                    span.start, span.end
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
        }
    }
}

impl std::error::Error for MathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier2_message_format_is_stable() {
        let err = MathError::UnsupportedCommand {
            name: r"\substack".to_owned(),
            span: Span::new(3, 12),
        };
        assert_eq!(
            err.to_string(),
            "`\\substack` is not yet supported; tier T2, tracked at franken_manim fm-j5t \
             (the tier-2 construct program) (bytes 3..12)"
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
}
