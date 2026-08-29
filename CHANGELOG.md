# Changelog

All notable changes to `franken_markdown` and the `fmd` CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- **SOTA typography optimization (Verna DocEng '25 + Hàn Thế Thành pdfTeX):**
  - Added `--typography-homogeneous`: gradual adjacent demerits in the Knuth-Plass line breaker. Replaces the coarse 4-class binary fitness check with a linear penalty proportional to the fine-grained spacing-ratio difference between consecutive lines (`fitness_ratio_milli` + `line_demerits` gradual arm in `src/layout.rs`), producing more homogeneous inter-word spacing in justified paragraphs. Default off — byte-identical classic KP output.
  - Implemented the microtype expansion emitter: `--microtype expansion` renders the justifier's ±1.5‰ glyph-elasticity credit as true horizontal glyph scaling via the PDF `Tz` operator (uniform per line — exact, because the credit distribution in `glue_adjustments_into` is proportional to box width), instead of flat letter-spacing (`build_segs_adjusted` + `append_text_segment_operator_with_render_mode` in `src/pdf.rs`). `--microtype protrusion` now carries both effects.
  - Tests pin the contract: default renders byte-identical (no `Tz`), opt-in deterministic, every `Tz` factor within the ±1.5% budget, page counts unchanged (`tests/gradual_demerits_test.rs`, `tests/microtype_test.rs`).

- **Multi-Chapter Book & Site Builder (`fmd book`, epic `7tus`, beads `qqst`, `qpqv`, `j0o4`):**
  - Added the `fmd book <dir> [--out-dir DIR] [--to html|pdf|both] [--json]` command to compile a directory of Markdown files into a unified multi-page HTML site and/or a single merged PDF book from one shared AST representation (`src/book.rs`, `src/cli.rs`).
  - Added frontmatter metadata parsing (`title=`, `author=`, `lang=`, `toc=`, `toc_depth=`) supporting key-value frontmatter fences (`---`) before block parsing (`src/parse/frontmatter.rs`).
  - Added the recursive transclusion engine (`{{#include relative/path.md}}`) with cycle detection, depth limit of 16, and canonical document root sandboxing to prevent directory traversal (`src/transclude.rs`).
  - Implemented automatic cross-file Markdown link rewriting (`other.md#anchor` → `other.html#anchor`) for site generation, preserving non-book links (`rewrite_links_for_site` in `src/book.rs`).
  - Implemented shared zero-JavaScript sidebar navigation injection (`<nav class="fmd-book-nav">`) with active chapter markers (`class="current"`), breadcrumb links, and inlined styles (`inject_book_nav` in `src/book.rs`).
  - Added deterministic `index.html` generation with meta refresh redirection pointing directly to the first chapter in the reading order.
  - Added multi-chapter PDF merge (`book_pdf_document`) inserting `Block::PageBreak` between chapter ASTs to guarantee chapter start on a fresh page while preserving continuous page numbering and generating a unified global outline tree.
  - Added machine-readable JSON receipt output on stdout (`--json`, schema `fmd-book-receipt-v1`) with chapter counts, per-file byte counts, page metrics, and unresolved link tallies.
  - Added optional manifest support (`book.toml`) for explicit chapter ordering (`order = [...]`) and book title overrides (`parse_book_manifest` in `src/cli.rs`).

- **Semantic AST Diff Engine (`fmd diff`, bead `64v4`):**
  - Added AST-level diffing (`src/diff.rs`) computing longest common subsequence (LCS) alignments across block and inline nodes.
  - Classifies AST changes into `Unchanged`, `Inserted`, `Deleted`, and `Modified` blocks, with granular word-level diffing and structural similarity ratio calculations.
  - Added multi-target diff renderers: side-by-side or unified HTML diff (`to_html`), ANSI color terminal output (`to_terminal`), Markdown report format (`to_markdown`), and machine-readable JSON schema (`to_json`).

- **Pure-Rust Zero-Dependency Mermaid Diagram Compiler (`src/diagrams.rs`, bead `y0vu`):**
  - Added a clean-room compiler transforming Mermaid `flowchart` and `sequenceDiagram` source fences directly into standalone SVG vector graphics for HTML and PDF rendering without external JavaScript or browser runtimes.
  - Flowcharts: supports layout directions (`TB`/`TD`, `BT`, `LR`, `RL`), node shapes (rectangle, rounded, circle, diamond, cylinder), custom styles, edge labels, link arrowheads, subgraphs, layer assignment, topological cycle breaking, and deterministic node coordinates.
  - Sequence diagrams: supports participants, actors, solid/dotted message lines with open/filled heads, inline notes (left of, right of, over), and activation boxes.

- **Standalone Vector SVG Poster Output (`--to svg`):**
  - Added the `--to svg` rendering target emitting standalone vector SVG posters with glyphs converted to vector paths, eliminating runtime font dependencies.

- **EPUB 3 E-Book Exporter (`src/epub.rs`, `--to epub`, bead `28t8`):**
  - Added single-file binary EPUB 3 export with a dependency-free pure-Rust OCF ZIP packaging engine, generating valid `mimetype`, `META-INF/container.xml`, `package.opf`, `nav.xhtml`, and XHTML content documents.

- **Deterministic JSON Search Index Generator (`--search-index <path>`, bead `r9z4`):**
  - Added search index generation exporting headings and anchored paragraphs into a structured JSON schema (`fmd-search-index-v1`) for static documentation search engines.

- **Document Intelligence, Readability Scoring, and Structural Linting (`src/doc_stats.rs`, `fmd stats`, bead `b3df`):**
  - Added document telemetry metrics: word counts, character counts (total and non-whitespace), reading time estimates, sentence counts, and syllable estimations.
  - Added automated readability formulas: Flesch Reading Ease, Flesch-Kincaid Grade Level, Coleman-Liau Index, and Automated Readability Index (ARI).
  - Added structural health linting: flags broken footnote references (`[^ref]` without definition), orphan footnote definitions, unresolvable local anchor links, and heading level hierarchy skips (e.g. H1 directly to H3).
  - Exposed via `fmd stats <input>` with human-readable CLI reporting and structured `--json` payload.

- **Accessibility Auditing and Verification (`src/verify.rs`, `fmd verify --a11y`, bead `jqls`):**
  - Added document accessibility auditing rules detecting missing image alt text, heading hierarchy skips, generic link labels ("click here", "link"), and headerless tables.

- **Optical-Margin Microtypography Protrusion (`--microtype protrusion`, bead `544o`):**
  - Added optical-margin protrusion for justified PDF body paragraphs, allowing punctuation marks (hyphens, periods, commas, quotes) to hang into the margin for cleaner visual alignment.

- **Self-Hosting Interactive Single-File HTML Workspace (`--interactive-html` / `--self-hosting`):**
  - Added single-file HTML workspace compilation bundling a live editor, real-time preview, and client-side PDF export into a self-contained zero-dependency HTML document.

- **PDF/UA-1 Tagged Structure Verification Gate (`scripts/check-pdf-ua.sh`, bead `8b3i`):**
  - Added automated CI verification checking emitted tagged PDF structure trees against veraPDF ISO 14289-1 profiles.

### Performance

- **30-Pass Extreme Software Optimization Loop (`skill-loop` passes 1..30):**
  - *Pass 1:* Branchless table-based line scanner and byte classifiers in `scanner.rs`.
  - *Pass 2:* Consolidated inline emphasis parsing from an intrusive doubly-linked list into a contiguous `EmphasisArena` to eliminate heap fragmentation.
  - *Pass 3:* Fast-path entity decoding for common HTML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;`) with zero heap allocations.
  - *Pass 4–5:* Pre-sized HTML string buffers, early-exit clean escape paths, and direct tag chunk emission in `html.rs`.
  - *Pass 6–7:* Fast-path ASCII loops in syntax highlighter whitespace, numeric literal, and identifier scanning in `highlight.rs`.
  - *Pass 8–10:* Unrolled sfnt font table checksum calculations in 16-byte blocks and pre-sized font subset emission buffers in `fmd-font`.
  - *Pass 11:* Utilized `Cow<[u8]>` for uncompressed WOFF1 table payloads to eliminate redundant clones during font subset wrapping.
  - *Pass 12–14:* Unrolled Adler-32 checksum calculations in 16-byte chunks in `compress.rs`.
  - *Pass 15–17:* Optimized Liang hyphenation trie root lookups using a direct 128-entry ASCII lookup table in `layout.rs`.
  - *Pass 18–21:* Implemented contiguous slice serialization for decimal numbers and fractions in `pdf.rs` to avoid `format!` allocations in content streams.
  - *Pass 22–24:* Added fast-path clean XML string emission in `pdfa.rs` bypassing escape scanners on ASCII alphanumeric strings.
  - *Pass 25–27:* Pre-allocated CLI input buffers using file metadata length hints.
  - *Pass 28–29:* Implemented branchless static lookup tables for hex nibble decoding in the SVG color parser.
  - *Pass 30:* Finalized regression benchmarks and verified byte-for-byte golden output fidelity across all targets.

---

## [0.4.2] - 2026-08-28

### Added
- **Typographic Scale Presets (`--type-size` / `--font-scale`):**
  - Added `TypeScalePreset` (`compact`, `standard`, `readable`, `large`, `xlarge`, `subheading`, `display`) and uniform proportional typographic scaling (`FontScale`) across HTML and PDF (`src/theme.rs`, `src/cli.rs`).
  - Added `renderHtmlConfiguredAdvanced` ABI exposing native font scaling controls to WASM host environments and mobile bridges (`src/wasm_abi.rs`).

### Fixed
- **PDF/A-2b ISO 32000-1 EOL Stream Separation:**
  - Fixed a compliance violation where ToUnicode CMAP streams omitted the required ISO 32000 newline before `endstream`, causing strict readers to count the stream's trailing newline as the separator and report stream length as `actual + 1` (veraPDF ISO 19005-2 clause 6.1.7.1 failure).

---

## [0.4.1] - 2026-08-28

### Added
- **MathML Core HTML Output:**
  - Added native `<math>` and `<annotation encoding="application/x-tex">` rendering in self-contained HTML output (`src/html.rs`, bead `lqxy`).
- **Multi-Language Hyphenation:**
  - Added Liang hyphenation pattern tables for German (`de`), French (`fr`), Spanish (`es`), and Dutch (`nl`), including French apostrophe elision handling (`src/layout.rs`, bead `38re`).
- **GFM-Plus Definition Lists:**
  - Added `<dl>`, `<dt>`, and `<dd>` parsing and rendering in HTML and PDF layout (`src/parse/mod.rs`, `src/html.rs`, `src/pdf.rs`, bead `ryu4`).
- **Caret Diagnostics:**
  - Added line/column caret rendering for parse warnings and syntax errors (`src/caret.rs`, bead `9wse`).
- **Table of Contents & PDF Contents Page:**
  - Added HTML TOC generation and PDF contents page with dot leaders and linked page numbers using two-pass layout convergence (`src/html.rs`, `src/pdf.rs`, bead `byqs`).
- **PDF Document Outline Bookmarks:**
  - Added hierarchical PDF outline bookmarks derived from Markdown heading levels (`src/pdf.rs`).
- **WOFF1 Embedded Font Subsets in HTML:**
  - Added TrueType font subset packaging in deterministic DEFLATE containers for HTML, reducing document size by ~18.4% (`src/woff1.rs`, `src/html.rs`, bead `ge1t`).
- **Recursive Watch Mode (`fmd watch`):**
  - Added zero-dependency recursive directory file watching with built-in loopback HTTP preview server and SSE live-reloading (`src/watch.rs`, bead `xjld`).
- **PDF/A-2b Archival Profile:**
  - Added XMP metadata embedding, sRGB OutputIntent dictionaries, `--pdf-a 2b`, and `--pdf-a-strict` validation (`src/pdfa.rs`, `src/pdf.rs`, bead `q6xc`).
- **Streaming Chunked PDF Emission:**
  - Added chunked page emission pipeline with bounded thread pool compression and monolithic byte-for-byte parity (`src/pdf.rs`, bead `u9jt.2`).
- **Corpus Glyph Coverage Auditor (`fmd doctor fonts`):**
  - Added glyph coverage auditor inspecting Markdown documents against bundled fonts with structured JSON reporting (`src/cli/font_coverage.rs`, bead `y5i9.1`).
- **Universal Apple iOS and Mac Catalyst Application:**
  - Added native iOS/Mac Catalyst workspace (`ios/`, bead `ybfn`) with live-highlighting Markdown editor, TOC navigation, and export bridges.

---

## [0.4.0] - 2026-08-25

### Added
- **Clean-Room TeX Mathematics Layout Engine (`fmd-math`, `fm-j5t`):**
  - Added pure-Rust TeX math layout engine implementing TeX Appendix G algorithms: atom classification, style stepping, synthesized Computer Modern metrics, stretchy delimiters, matrix environments, and math-alphanumerics (`\mathcal`, `\mathbb`).
- **Factored Font Subsystem Crate (`fmd-font` v0.2.0):**
  - Factored out `fmd-font` workspace crate with clean-room sfnt table reader, glyf outline decoder, CFF decoder, and bundled OFL fonts.
- **UAX #14 CJK Line Breaking:**
  - Implemented Unicode Standard Annex #14 CJK line breaking: inter-ideograph breaking rules with opening, closing, and non-starter character prohibitions.
- **Configurable PDF Typography:**
  - Added `base_font_size`, `heading_scale`, and `table_font_size` layout overrides backed by a materialized `TypeScale` model.
- **Microtype Elasticity Symmetry & Void Budgeting:**
  - Enforced solver-emitter elasticity symmetry to eliminate ragged-tail overhangs.
  - Added inter-block gap flexing to reclaim space when page void exceeds 12% of content height.

---

## [0.3.5] - 2026-07-23

### Added
- **CJK Line Breaking & Knuth-Plass Glue:**
  - Added UAX #14 CJK line breaking support with zero-width stretchable glue integration for the Knuth-Plass optimal paragraph breaking solver.

---

## [0.3.4] - 2026-07-10

### Added
- **Remote Image Fetching & Lossless JPEG PDF Embedding (#2):**
  - Added CLI remote image fetching with direct `/DCTDecode` JPEG stream embedding for compact PDF rendering.
- **Bundled Noto Sans Math Fallback Face (#3):**
  - Bundled a curated ~56 KiB Noto Sans Math font subset to provide fallback glyphs for math symbols and arrows.
- **SVG CSS & Paint Cascade Engine:**
  - Implemented CSS declaration parsing, quoted values, `!important`, opacity/paint inheritance, and `hsl()`/`hwb()` color support.

---

## [0.3.3] - 2026-07-09

### Added
- **Vector SVG Drawing Operators:**
  - Added vector pattern strokes, stroked text paths, `textPath`, non-scaling strokes, and `color-mix()` alpha transparency preservation in the PDF SVG renderer.
- **Self-Contained Local SVG Embedding:**
  - Local SVG image references automatically embed as base64 data URIs in HTML preview documents.

---

## [0.3.2] - 2026-07-08

### Added
- **PDF Reading Quality Enhancements:**
  - Added vector task checkboxes, URL and long-token break rules, and TeX-correct shrink semantics (`line_badness`).
- **NPM Package Release:**
  - Published `@franken-suite/franken-markdown` npm package with automated release workflows.

---

## [0.3.1] - 2026-07-07

### Fixed
- Fixed HTML base64 encoder padding and PDF zero-length segment drawing bounds.

---

## [0.3.0] - 2026-07-07

### Added
- **Vector SVG PDF Drawing Engine:**
  - Implemented direct SVG-to-PDF vector conversion for paths, shapes, text, transforms, linear/radial gradients, masks, clips, markers, and CSS variables.
- **Mermaid & MMD Syntax Highlighting:**
  - Added syntax highlighting for Mermaid and MMD code blocks in HTML and PDF.
- **Constrained Table Column Allocator:**
  - Added min/max column width measurement and wrapping-badness optimization for PDF tables.

---

## [0.2.0] - 2026-07-03

### Added
- **crates.io Publishing & Safety Controls:**
  - Configured crates.io package release metadata with zero dependencies for the core library.
  - Implemented staged native file writes using temporary files and automatic rollback on failure.
  - Added strict zlib and PNG header validation.

---

## [0.1.0] - 2026-06-29

### Added
- **Initial Cross-Platform Release:**
  - Published release archives for Linux (`x86_64-unknown-linux-gnu`), macOS Intel (`x86_64-apple-darwin`), macOS Apple Silicon (`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
  - Added automated installer scripts (`install.sh`, `install.ps1`) with SHA-256 checksum verification.
  - Integrated Asupersync for structured concurrency in native batch rendering.

---

## [0.0.1] - 2026-06-26

### Added
- **Foundation & Clean-Room Core Engine:**
  - Clean-room zero-dependency Markdown parser and HTML emitter.
  - Agent-first `fmd` CLI with `capabilities --json`, `doctor --json`, `robot-docs guide`, and stable exit codes.
  - Shared typed theme model driving coherent HTML and PDF formatting.
  - TrueType font reader, subsetter, GPOS kerning, and GSUB ligature engine.
  - Knuth-Plass optimal paragraph breaking and Liang/TeX hyphenation.
  - Deterministic PDF 1.7 writer with embedded subset fonts, measured tables, and accessible tagged-PDF logical trees.
  - WASM compilation target with byte-for-byte output parity.
  - CommonMark 0.31.2 conformance test harness and verification test suite.

[Unreleased]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.5...v0.4.0
[0.3.5]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/8b66477...v0.1.0
