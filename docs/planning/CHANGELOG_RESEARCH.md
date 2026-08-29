# CHANGELOG Research Notes

Scope: full repository history from initial scaffold (2026-06-26) through the current development wave on `main` (2026-08-28).

## Evidence Sources

- Git history: `git log --oneline --reverse`, `git log --oneline -n 150`, `git show --stat`
- Tag refs: `git for-each-ref refs/tags --sort=creatordate --format='%(refname:short)%x09%(creatordate:short)%x09%(subject)'`
- GitHub Releases: `gh release list --limit 100`
- Crates.io: `franken_markdown` 0.4.2, `fmd-font` 0.3.0, `fmd-math` 0.1.0
- npm registry: `@franken-suite/franken-markdown` 0.4.2 (with Sigstore provenance attestation)
- Beads tracker: `.beads/issues.jsonl`
- Planning docs: `docs/planning/COMPREHENSIVE_PLAN_FOR_FRANKEN_MARKDOWN.md`, `docs/planning/REALITY_CHECK_BRIDGE_PLAN.md`, `docs/planning/PERFORMANCE_OPTIMIZATION_PLAN.md`
- Architecture docs: `docs/PDF_A.md`, `docs/PDF_ACCESSIBILITY.md`, `docs/BATCH_ORCHESTRATION.md`, `docs/BATCH_WORKER_BUDGET.md`, `docs/SIMD_ISLAND_DESIGN.md`

## Version Spine

| Version / phase | Kind | Date | Evidence | Notes |
|---|---|---:|---|---|
| Unreleased (`main`) | Working tree | 2026-08-28 | commits `a94ec21`..`db28187` | Semantic AST diff engine (`fmd diff`), pure-Rust zero-dependency Mermaid flowchart and sequence SVG compiler (`diagrams.rs`), document stats & readability linting (`fmd stats`), advanced typography scaling WASM ABI, idea-wizard roadmap wave |
| `v0.4.2` | Release | 2026-08-28 | tag `v0.4.2` (`e1f5dc6`), commit `8efdb3f` | PDF/A-2b ISO 32000 ToUnicode EOL compliance patch, type-scale presets (`--type-size`), `fmd-font 0.3.0` crates.io publication, npm 0.4.2 release with provenance |
| `v0.4.1` | Release | 2026-08-28 | tag `v0.4.1` (`e66e020`), commit `e26de3d` | Document navigation, MathML Core HTML, multi-language hyphenation (de, fr, es, nl), GFM-plus definition lists, caret diagnostics, HTML TOC + PDF contents page with dot leaders, outline bookmarks, WOFF1 font subsets, `fmd watch`, PDF/A-2b, chunked PDF emission, `fmd doctor fonts`, Universal Apple iOS/Mac Catalyst ForgeView app, reality-check CI hardening |
| `v0.4.0` | Release | 2026-08-25 | tag `v0.4.0` (`e1ae0cd`), commit `4328835` | Major feature release: clean-room TeX-mathematics layout engine (`fmd-math`), factored `fmd-font` workspace crate (`v0.2.0`), UAX #14 CJK line breaking, expanded math symbols & math-alphanumerics (`\mathcal`, `\mathbb`), configurable typography scale, solver-emitter elasticity symmetry, page-level void budgeting |
| `0.3.5` | In-tree Cargo | 2026-07-23 | commit `5b1e6cc`..`e63e463` | UAX #14 CJK line breaking: inter-ideograph breaks with closing/opening/non-starter prohibitions, zero-width stretchable glue for Knuth-Plass, single-pass forward word splitter. *In-tree Cargo 0.3.5; not a git tag or GitHub release.* |
| `v0.3.4` | Release | 2026-07-11 | tag `v0.3.4` (`1ca275f`), commit `5b1e6cc` | Issue-driven PDF fidelity patch: hotlinked images fetched by CLI with JPEG `/DCTDecode` PDF embedding (#2), bundled Noto Sans Math symbol fallback face (#3), SVG CSS/opacity/paint cascade, `hsl()`/`hwb()` colors |
| `v0.3.3` | Release | 2026-07-09 | tag `v0.3.3` (`e2c5d72`), commit `b863967` | All-platform DSR patch: local SVG HTML embedding, vector SVG pattern strokes, stroked text, textPath, non-scaling stroke, `color-mix()` alpha preservation, DSR archives for Linux, macOS Intel, macOS Apple Silicon, Windows |
| `v0.3.2` | Release | 2026-07-08 | tag `v0.3.2` (`f4787a6`), commit `37c3b40` | PDF reading quality: vector task checkboxes, URL/long-token wrapping, TeX shrink semantics, npm publication `@franken-suite/franken-markdown`, SVG text fidelity |
| `v0.3.1` | Release | 2026-07-07 | tag `v0.3.1` (`d30e74d`), commit `5423d18` | DSR publication patch for the 0.3.0 wave, HTML base64 encoder, PDF empty-segment drawing |
| `v0.3.0` | Tag | 2026-07-07 | tag `v0.3.0` (`ba54fc5`), commit `91afecc` | SVG vector PDF drawing, Mermaid syntax highlighting, measured table allocation, local PDF assets, staged writes, batch receipts. *Plain git tag; not a GitHub Release.* |
| `v0.2.0` | Release | 2026-07-03 | tag `v0.2.0` (`4573de9`), commit `2d51cc8` | crates.io package enabled, staged native writes with rollback, stricter zlib/PNG validation, public JSON escaping |
| `v0.1.0` | Release | 2026-06-30 | tag `v0.1.0` (`be9064e`), commit `8b66477` | Initial binary release: GitHub release archives (Linux, macOS Intel, macOS ARM64, Windows), installer asset lookup, published Asupersync dependency |
| Scaffold & MVP | Foundation | 2026-06-26 | commits `8b66477`..`d2b9da3` | Zero-dependency Markdown-to-HTML engine, fmd CLI, shared theme, font reader/subsetter, deterministic PDF MVP, Knuth-Plass breaking, Liang hyphenation, accessible tagged-PDF, Asupersync batch orchestration, CommonMark conformance harness, WASM proof gate |

---

## Unreleased Research Notes (post-`v0.4.2` on `main`)

The `v0.4.2..HEAD` log contains 12 non-merge commits delivering significant new capabilities across diffing, diagram compilation, document analysis, and WASM/theme bindings:

- **Semantic AST diff engine (`src/diff.rs`, `fmd diff`, bead `64v4`):**
  - Structural LCS diffing between two parsed Markdown ASTs at block and inline levels.
  - Categorizes changes into `Unchanged`, `Inserted`, `Deleted`, and `Modified` blocks, with word-level insertions/deletions and structural similarity coefficient.
  - Multi-target visual rendering: standalone HTML diff with red/green GitHub-style side-by-side or unified styling (`to_html`), terminal ANSI formatted diff (`to_terminal`), Markdown formatted diff report (`to_markdown`), and machine-readable JSON envelope (`to_json`).
  - CLI subcommand: `fmd diff <file_a> <file_b>` with `--format text|json|html|markdown` and `--side-by-side`.
  - Representative commit: `db28187`.
- **Pure-Rust zero-dependency Mermaid diagram compiler (`src/diagrams.rs`, bead `y0vu`):**
  - Clean-room compiler turning Mermaid `flowchart` and `sequenceDiagram` syntax directly into standalone SVG vector graphics for HTML and PDF embedding.
  - Flowcharts: supports directions (`TB`/`TD`, `BT`, `LR`, `RL`), node shapes (rectangle, rounded, circle, diamond, cylinder), custom styles, edge labels, link arrowheads, subgraphs, layer assignment, topological cycle breaking, and deterministic node layout.
  - Sequence diagrams: participants, actors, solid/dotted arrows with open/filled heads, inline notes (left of, right of, over), message labels, and activation boxes.
  - Representative commit: `3b58216`.
- **Document analysis, readability scoring, and structural linting (`src/doc_stats.rs`, `fmd stats`, bead `b3df`):**
  - Metrics: word counts, character counts (total and non-whitespace), reading time estimate, sentence count, syllable count estimation.
  - Readability indices: Flesch Reading Ease, Flesch-Kincaid Grade Level, Coleman-Liau Index, and Automated Readability Index (ARI).
  - Structural linting: detecting broken footnote references (`[^ref]` without definition), orphan footnote definitions, unresolvable local anchors, and heading hierarchy skips (e.g. H1 directly to H3).
  - Surfaces via `fmd stats <input>` (and `fmd stats --text '<md>'`), with formatted terminal output and structured `--json` payload registered in `capabilities --json`.
  - Representative commits: `a94ec21`, `8ac0fa6`, `93d69ca`, `13659f3`, `ebd27e4`.
- **Advanced typography scaling and WASM ABI enhancement (`src/wasm_abi.rs`, `src/theme.rs`):**
  - Added `renderHtmlConfiguredAdvanced` ABI exposing native typography scaling (`FontScale`, `TypeScalePreset`) to WASM and host applications (including the iOS/Catalyst ForgeView bridge).
  - Representative commits: `bdcaec2`, `e8fcb4a`.
- **Roadmap planning and prioritization wave:**
  - Seeded idea-wizard epics into beads (book multi-file site builder, MCP server, microtypography opt-in, `<fmd-view>` web component, CommonMark burndown) and prioritized user-selected wave (book/microtype/fmd-view/diff/analyze/a11y/links/epub/pdfua/search/frontmatter/svg) to P1.
  - Representative commits: `9fee190`, `dcd145e`.

---

## 0.4.2 Research Notes

The `v0.4.1..v0.4.2` log contains 15 non-merge commits:

- **PDF/A-2b ISO 32000-1 EOL compliance patch (`src/pdf.rs`, `be9b7e8`):**
  - The ToUnicode stream writer omitted the required ISO 32000 newline before `endstream`. Strict readers counted the stream's own trailing newline as the separator EOL, resulting in stream `Length` declared as `actual + 1`.
  - veraPDF flagged this as ISO 19005-2 clause 6.1.7.1 violation (4 failed checks).
  - One-line fix in `src/pdf.rs`; all stream lengths re-verified byte-exact; regenerated PDF golden snapshots (`5637bad`).
- **Stepped type-scale presets (`--type-size`, `src/theme.rs`, `src/cli.rs`, `3962a9b`, `e180bc2`):**
  - Introduced `TypeScalePreset` (`compact`, `standard`, `readable`, `large`, `xlarge`, `subheading`, `display`) and uniform typographic scaling via `FontScale`.
  - Wired into CLI `--type-size`, WASM builder methods, and ForgeView UI step controls.
- **Ecosystem and distribution alignment:**
  - `fmd-font 0.3.0` published to crates.io (`79d36ce`, `8f33fa3`).
  - `@franken-suite/franken-markdown` 0.4.2 published to npm with Sigstore provenance attestation (`43e8c7f`).
  - CI veraPDF Docker image updated to supported `verapdf/cli` (`d0bb3bb`).

---

## 0.4.1 Research Notes

The `v0.4.0..v0.4.1` log contains 201 non-merge commits:

- **MathML Core HTML output (`src/html.rs`, bead `lqxy`, `d8006d0`):**
  - Native `<math>` and `<annotation encoding="application/x-tex">` rendering in self-contained HTML.
- **Multi-language hyphenation (`src/layout.rs`, bead `38re`, `efff5a4`, `2108bb5`):**
  - TeX Liang pattern tables and hyphenation dictionaries for German (`de`), French (`fr`), Spanish (`es`), and Dutch (`nl`).
  - French elision handling without breaking English contractions.
- **GFM-plus definition lists (`src/parse/mod.rs`, `src/html.rs`, `src/pdf.rs`, bead `ryu4`, `d8006d0`):**
  - `<dl>`, `<dt>`, `<dd>` in HTML and PDF layout, `--profile gfm-plus` validation.
- **Caret diagnostics (`src/caret.rs`, bead `9wse`, `4b11ae4`, `dd9cb9a`):**
  - Accurate line/column caret reporting for parse warnings and syntax errors.
- **Table of Contents and PDF contents page (`src/html.rs`, `src/pdf.rs`, bead `byqs`, `7836cf8`):**
  - HTML TOC emission with depth controls and marker support.
  - PDF contents page with dot leaders and linked page numbers via two-pass layout convergence.
- **PDF document outline bookmarks (`src/pdf.rs`, `41d597a`):**
  - Hierarchical outline bookmarks generated from Markdown heading levels.
- **WOFF1 embedded font subsets in HTML (`src/woff1.rs`, `src/html.rs`, bead `ge1t`, `2675d3e`, `bf54890`):**
  - TrueType font subsets wrapped in deterministic DEFLATE containers, achieving ~18.4% smaller showcase HTML.
  - `--html-font-format ttf|woff1` selection; byte-deterministic and browser-verified.
- **`fmd watch <dir>` (`src/watch.rs`, bead `xjld`, `5b30e7f`, `40dd855`):**
  - Std-only recursive Markdown file watching with loopback preview HTTP server and live reload SSE.
- **PDF/A-2b profile (`src/pdfa.rs`, `src/pdf.rs`, bead `q6xc`, `802fffc`, `b7db9f1`, `d2e3e12`):**
  - XMP metadata, sRGB OutputIntent, `--pdf-a 2b` / `--pdf-a-strict`, and veraPDF CI validation gate.
- **Streaming chunked PDF page emission (`src/pdf.rs`, bead `u9jt.2`, `75043e8`, `733b2fe`):**
  - Chunked page emission with monolithic byte parity and bounded thread pool compression.
- **`fmd doctor fonts` (`src/cli/font_coverage.rs`, bead `y5i9.1`, `af29124`):**
  - Corpus glyph-coverage auditor with stable JSON output.
- **Universal Apple application (`ios/`, bead `ybfn`, `4e34d78`, `ad7ef89`, `b66a73e`, `04bb488`, `e1f3d5b`):**
  - Universal iOS and Mac Catalyst application: live-highlighting Markdown editor, TOC outline navigation, PDF/HTML document export bridge, and adaptive ForgeView.
- **Reality-check CI hardening:**
  - Pinned to dated `nightly-2026-08-25` toolchain with `wasm32-unknown-unknown`.
  - Rch-resilient CI gate scripts.
  - CommonMark floor ratcheted to 381/652.
  - WASM size budget ratchets.

---

## 0.4.0 Research Notes

The `0.3.5..0.4.0` wave delivered:

- Clean-room TeX-mathematics layout engine (`fmd-math`, `fm-j5t`): grammar, atom engine, styles, Appendix-G placement, synthesized CM metrics, drawn delimiters, environments, stretchy accents, macros, packs (`4328835`, `2722c79`, `5310d87`, `4e5066c`).
- Factored `fmd-font` workspace crate (`v0.2.0`) with clean-room sfnt reader, glyf outline decoder, and bundled OFL faces.
- UAX #14 CJK line breaking: inter-ideograph breaks with closing/opening/non-starter prohibitions.
- Math-alphabet and symbol fallback coverage (`\mathcal`, `\mathbb`): U+1D49C–U+1D4CF, U+1D538–U+1D56B, double-struck digits U+1D7D8–U+1D7E1, format-12 cmap emission (`2406e21`, `8af8f95`).
- Configurable PDF typography: `base_font_size`, `heading_scale`, `table_font_size` overrides backed by materialized `TypeScale`.
- Solver-emitter microtype elasticity symmetry: glyph elasticity credit policy-gated to justified lines, eliminating ragged-tail overhangs.
- Page-level void budgeting: inter-block gaps flex to pull following blocks up when visual void exceeds 12% of content height (`45d2.4`).

---

## 0.3.4 Research Notes

- Hotlinked images fetched by CLI with JPEG `/DCTDecode` PDF embedding (#2, `5b1e6cc`).
- Bundled ~56 KiB Noto Sans Math subset as symbol fallback face (#3, `e63e463`).
- SVG CSS structural parsing: declaration splitting, quoted values, `!important`, top-level separators, trailing `var()`.
- SVG opacity/paint cascade: `initial`, `unset`, `inherit`, inherited paint keywords, paint alpha with opacity, `hsl()`/`hwb()` colors, absolute length units.
- Measured speed work across parser, HTML, PDF, and compression hot paths.
- Windows-only CLI contract assertion fixes for JSON-escaped paths.

---

## 0.3.3 Research Notes

- SVG/PDF fidelity: pattern strokes, stroked text, `textPath`, non-scaling stroke, CSS-variable URL resources, chained drop shadows, object-bounding-box patterns.
- HTML and asset fidelity: local SVG assets become self-contained data URIs, remote SVG import stripping.
- SVG `color-mix()` alpha preservation with transparent paint.
- Measured speed passes across parser, HTML, PDF writer, font cache, and compression.

---

## 0.3.2 Research Notes

- PDF reading quality: vector task checkboxes, URL/long-token wrapping, TeX-correct shrink semantics (`line_badness`).
- HTML embedded font `OS/2` fix for Chromium font sanitizer.
- Published `@franken-suite/franken-markdown` npm package with idempotent release workflow.
- SVG text fidelity: `baseline-shift`, word spacing, explicit whitespace.

---

## 0.3.1 Research Notes

- DSR publication patch for the 0.3.0 wave.
- HTML base64 encoder and PDF empty-segment drawing passes.

---

## 0.3.0 Research Notes

- SVG/PDF vector drawing for frankenmermaid diagrams: paths, shapes, text, transforms, gradients, masks, clips, markers, opacity, drop shadows, CSS variables.
- Mermaid/MMD fence highlighting in HTML and PDF.
- Measured table column allocation with wrapping-badness allocator.
- Local PDF image auto-loading and staged writes with rollback.

---

## 0.2.0 Research Notes

- crates.io publication enabled: `license-file = "LICENSE"`, package trimming.
- Staged native writes with temporary files and rollback on failure.
- Stricter zlib/PNG payload validation.

---

## 0.1.0 Research Notes

- First binary release: cross-platform release archives for Linux, macOS Intel, macOS Apple Silicon, and Windows.
- Prebuilt binary installer with checksum verification.
- Published Asupersync crate dependency for native batch mode.

---

## Initial Capability Wave (2026-06-26 to 2026-06-29)

- Clean-room zero-dependency Markdown parser and HTML emitter (`8b66477`).
- Agent-first `fmd` CLI surface (`capabilities`, `doctor`, `robot-docs`, `--text`, `--json`) (`98c7f0b`, `0ab6879`).
- Shared typed theme model (`064e4ab`, `5e1eaf4`).
- Clean-room TrueType reader, subsetter, GPOS kerning, GSUB ligatures (`102bc05`, `38621ae`, `d38bc62`, `60e7664`).
- Knuth-Plass paragraph breaker and Liang/TeX hyphenation (`789e6e1`, `22ad648`, `cef6d16`).
- Deterministic PDF 1.7 writer with embedded subset fonts, measured tables, tagged structure (`e0f07ac`, `91d4707`, `4636265`, `955dd50`).
- WASM package with native byte parity (`54dc00a`, `e999d23`).
- CommonMark 0.31.2 spec conformance harness (`2ce6f8c`).
- Asupersync native batch orchestration (`4e36b9f`, `60f09e3`).
