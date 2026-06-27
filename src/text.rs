//! Text + font subsystem (in build-out): the clean-room font reader and shaper
//! that feeds glyph runs to the layout engine and the PDF writer.
//!
//! Planned design (dependency-free, Latin-first — the focused subset we need):
//!
//! * **Font reader** — parse just the tables we use from TTF/OTF: `head`,
//!   `cmap` (char → glyph), `hmtx` (advances), `kern` and GPOS pair-adjust
//!   (kerning), GSUB `liga` (ligatures), `glyf`/`loca` or `CFF ` (outlines for
//!   PDF embedding), `name`/`OS2` metadata.
//! * **Shaping** — character-to-glyph mapping with kerning and common ligatures;
//!   no complex-script/bidi shaping in v0 (added later if needed), which keeps
//!   the path tiny and fast for the Latin documents that dominate Markdown.
//! * **Subsetting** — keep only the glyphs a document uses, rewriting `cmap`,
//!   `glyf`/`loca`/`CFF `, and `hmtx`, for a **tiny embedded font** in the PDF.
//! * **Embedded curated fonts** — a small set of OFL families (sans / serif /
//!   mono) shipped as data so output is byte-identical on every OS and in WASM
//!   (no system-font discovery, hence no `fontconfig`/C dependency).
//!
//! Implementation is tracked in beads.
