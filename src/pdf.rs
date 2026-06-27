//! PDF renderer (in build-out).
//!
//! The planned pipeline is fully clean-room and dependency-free:
//!
//! 1. **AST → block/inline boxes** using the shared [`crate::theme::Theme`] so the
//!    PDF matches the HTML preview.
//! 2. **Text shaping** ([`crate::text`]): map characters to glyphs via the font
//!    `cmap`, apply advances (`hmtx`), pair **kerning** (`kern`/GPOS) and
//!    **ligatures** (GSUB) — Latin-first, the focused subset we actually need.
//! 3. **Line breaking** ([`crate::layout`]): **Knuth–Plass** total-fit
//!    optimization with Liang/TeX **hyphenation**, proper **leading**, and
//!    optional justification + microtypographic protrusion — LaTeX-grade output.
//! 4. **Page assembly**: pagination with widow/orphan control, headers/footers,
//!    table and code-block layout.
//! 5. **PDF writing**: our own minimal, spec-compliant writer that embeds
//!    **subset** fonts (only the glyphs used), draws positioned glyph runs as
//!    vectors, and FlateDecode-compresses content streams for a **tiny file
//!    size**. Deterministic, byte-stable output.
//!
//! Until those subsystems land (tracked in beads), this returns a typed
//! `not-yet-implemented` refusal — the AST + theme plumbing is already wired.

use crate::PdfOptions;
use crate::ast::Document;
use crate::error::{RenderError, Result};

/// Render a document to optimized PDF bytes.
///
/// # Errors
/// Returns [`RenderError::NotYetImplemented`] until the layout/text/writer
/// subsystems land.
pub fn render(_doc: &Document, _opts: &PdfOptions) -> Result<Vec<u8>> {
    Err(RenderError::NotYetImplemented(
        "pdf pipeline (text shaping + Knuth-Plass layout + subsetting PDF writer)",
    ))
}
