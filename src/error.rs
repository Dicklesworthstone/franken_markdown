//! Error types for franken_markdown. Hand-rolled (no `thiserror`) so the engine
//! library stays at zero third-party dependencies.

use std::fmt;

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, RenderError>;

/// A rendering error.
#[derive(Debug)]
pub enum RenderError {
    /// An I/O error reading source or writing output (CLI paths).
    Io(std::io::Error),
    /// PDF generation failed in a way that must not be masked as a blank but
    /// "successful" document — e.g. a caller-supplied font that parses yet cannot
    /// be subset, or a writer invariant violation. Carries a short, stable
    /// selector for agent-readable diagnostics.
    PdfGeneration(&'static str),
    /// The Markdown or options were structurally invalid.
    InvalidInput(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::PdfGeneration(what) => {
                write!(f, "PDF generation failed: {what}")
            }
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RenderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl RenderError {
    /// A short, stable machine code for the error class (for robot/JSON output).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io_error",
            Self::PdfGeneration(_) => "pdf_generation",
            Self::InvalidInput(_) => "invalid_input",
        }
    }
}
