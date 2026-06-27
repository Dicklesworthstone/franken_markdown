//! Layout engine (in build-out): the LaTeX-grade paragraph and page builder used
//! by the PDF renderer.
//!
//! Planned design (clean-room, dependency-free):
//!
//! * **Box / glue / penalty model** — the TeX paragraph representation that makes
//!   high-quality breaking possible.
//! * **Knuth–Plass optimal line breaking** — total-fit minimization of demerits
//!   over the whole paragraph (not greedy), giving even spacing and few
//!   hyphens, with badness/penalty tuning per block type.
//! * **Hyphenation** — Liang's algorithm with the TeX hyphenation patterns
//!   (compiled to a compact trie), enabling good justification.
//! * **Leading** — baseline-to-baseline spacing derived from the theme; ragged
//!   or justified setting per element.
//! * **Microtypography** — optional margin protrusion / glyph-edge kerning for
//!   a smooth optical margin.
//! * **Page assembly** — pagination with widow/orphan control, keep-with-next
//!   for headings, and table/code-block breaking.
//!
//! The HTML path delegates line-breaking to the browser (with
//! `text-wrap: pretty; hyphens: auto`); this engine is what makes the **PDF**
//! output beautiful. Implementation is tracked in beads.
