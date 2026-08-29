# Changelog

## Scope and methodology

This changelog reconstructs the project's history from the git log, the
repository files, the checked-in beads tracker (`.beads/`), release metadata,
and the docs under `docs/` (planning docs now live in `docs/planning/`). It is
organized by landed capability rather than raw commit order, with a date-based
version timeline kept visible so chronology is never lost. Representative
commits and release artifacts are linked directly.

This changelog began as reconstructed pre-release development history and now
records shipped binary, crate, and npm releases alongside active development
waves. GitHub Releases ship standalone `fmd` CLI archives with SHA-256 sidecars;
the `franken_markdown` library is published to crates.io alongside `fmd-font` and
`fmd-math`; and the WASM package `@franken-suite/franken-markdown` is assembled
by the tag-gated workflow with Sigstore provenance attestation. Conformance and
status numbers below are the measured, ratcheted floors enforced in CI, not
aspirational targets.

**Release vs tag:** GitHub Releases exist for
[`v0.1.0`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.1.0),
[`v0.2.0`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.2.0),
[`v0.3.1`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.1),
[`v0.3.2`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.2),
[`v0.3.3`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.3),
[`v0.3.4`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.4),
[`v0.4.0`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.0),
[`v0.4.1`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.1), and
[`v0.4.2`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.2).
The `v0.3.0` entry is a plain git tag
([`v0.3.0`](https://github.com/Dicklesworthstone/franken_markdown/tree/v0.3.0)).
The latest GitHub Release is
[`v0.4.2`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.2)
(2026-08-28). In-tree Cargo on `main` is `0.4.2`.

Scope window: 2026-06-26 through 2026-08-28.

- Sources: git history, the working tree, `.beads/issues.jsonl`, `docs/`, GitHub Releases, crates.io, and npm registry.
- Version state: **In-tree Cargo `0.4.2` on `main`; latest GitHub Release `v0.4.2` (2026-08-28); crates.io `0.4.2` (`fmd-font 0.3.0`, `fmd-math 0.1.0`); npm `@franken-suite/franken-markdown 0.4.2`.**
- Commit links use the form `https://github.com/Dicklesworthstone/franken_markdown/commit/<hash>`.
- Release links use the form `https://github.com/Dicklesworthstone/franken_markdown/releases/tag/<tag>`.
- Plain tag links use the form `https://github.com/Dicklesworthstone/franken_markdown/tree/<tag>`.

## Version Timeline

`Kind` distinguishes a published GitHub Release from a plain git tag or in-tree development state.

| Version | Kind | Date | Headline |
|---|---|---|---|
| Unreleased (`main`) | Development | 2026-08-28 | Semantic AST diff engine (`fmd diff`), pure-Rust zero-dependency Mermaid flowchart/sequence SVG compiler (`diagrams.rs`), document stats & readability linting (`fmd stats`), advanced typography scaling WASM ABI |
| [`v0.4.2`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.2) | Release | 2026-08-28 | PDF/A-2b ISO 32000 ToUnicode EOL compliance patch, stepped type-scale presets (`--type-size`), `fmd-font 0.3.0` crates.io publication, npm 0.4.2 release with Sigstore provenance |
| [`v0.4.1`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.1) | Release | 2026-08-28 | MathML Core HTML output, multi-language hyphenation (de, fr, es, nl), GFM-plus definition lists, caret diagnostics, HTML TOC + PDF contents page with dot leaders, PDF outline bookmarks, WOFF1 font subsets, `fmd watch`, PDF/A-2b, chunked PDF emission, `fmd doctor fonts`, Universal Apple iOS/Mac Catalyst ForgeView app |
| [`v0.4.0`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.4.0) | Release | 2026-08-25 | Clean-room TeX-mathematics layout engine (`fmd-math`), factored font crate (`fmd-font` `v0.2.0`), UAX #14 CJK line breaking, expanded math symbol fallbacks (`\mathcal`, `\mathbb`), configurable typography scale, solver-emitter elasticity credit symmetry, page-level void budgeting |
| `0.3.5` | In-tree Cargo | 2026-07-23 | UAX #14 CJK line breaking: inter-ideograph breaks with closing/opening/non-starter prohibitions, zero-width stretchable glue for Knuth-Plass, single-pass forward word splitter |
| [`v0.3.4`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.4) | Release | 2026-07-11 | Issue-driven PDF fidelity patch: hotlinked remote images fetched by CLI with JPEG `/DCTDecode` PDF embedding (#2), bundled Noto Sans Math symbol fallback face (#3), SVG CSS/opacity/paint structural cascade, `hsl()`/`hwb()` colors |
| [`v0.3.3`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.3) | Release | 2026-07-09 | All-platform DSR patch: local SVG HTML embedding, vector SVG pattern strokes, stroked text, textPath, non-scaling stroke, `color-mix()` alpha preservation, DSR release archives for Linux, macOS Intel, macOS Apple Silicon, Windows |
| [`v0.3.2`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.2) | Release | 2026-07-08 | PDF reading-quality: vector task checkboxes, URL/long-token wrapping, TeX-correct shrink semantics, npm publication `@franken-suite/franken-markdown`, SVG text fidelity |
| [`v0.3.1`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.3.1) | Release | 2026-07-07 | DSR publication patch for the 0.3.0 wave, HTML base64 encoder, PDF empty-segment drawing |
| [`v0.3.0`](https://github.com/Dicklesworthstone/franken_markdown/tree/v0.3.0) | Tag | 2026-07-07 | SVG vector PDF drawing for frankenmermaid diagrams, Mermaid/MMD syntax highlighting, measured table allocation, local PDF assets, staged writes, batch receipts |
| [`v0.2.0`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.2.0) | Release | 2026-07-03 | crates.io package enabled, staged native writes with rollback, stricter zlib/PNG validation, public JSON escaping |
| [`v0.1.0`](https://github.com/Dicklesworthstone/franken_markdown/releases/tag/v0.1.0) | Release | 2026-06-30 | Initial binary release: cross-platform release archives (Linux, macOS Intel, macOS Apple Silicon, Windows), installer asset lookup, published Asupersync dependency |
| Scaffold & MVP | Foundation | 2026-06-26 | Zero-dependency Markdown-to-HTML engine, `fmd` CLI, shared theme, font reader/subsetter, deterministic PDF MVP, Knuth-Plass breaking, Liang hyphenation, accessible tagged-PDF, Asupersync batch orchestration, CommonMark conformance harness, WASM proof gate |

---

## Unreleased

Feature and capability expansion on `main` post-`v0.4.2` (12 commits): semantic
Markdown AST diff engine (`fmd diff`), pure-Rust zero-dependency Mermaid diagram
compiler (`diagrams.rs`), document analysis and readability linting (`fmd
stats`), advanced typography scaling in WASM ABI, and roadmap wave
prioritization.

### Delivered capability

- **Semantic AST diff engine (`64v4`):** Pure-Rust structural LCS diffing between
  two Markdown ASTs at block and inline levels, categorizing blocks as
  `Unchanged`, `Inserted`, `Deleted`, or `Modified` with word-level delta
  accounting and structural similarity scoring; multi-target visual rendering to
  standalone HTML (`to_html`), terminal ANSI (`to_terminal`), Markdown report
  (`to_markdown`), and machine-readable JSON (`to_json`); surfaced via
  `fmd diff <file_a> <file_b>` with `--format` and `--side-by-side` flags.
- **Zero-dependency Mermaid diagram compiler (`y0vu`):** Pure-Rust in-process
  compiler (`src/diagrams.rs`) converting Mermaid `flowchart` and
  `sequenceDiagram` source directly into standalone SVG vector graphics for
  seamless HTML and PDF embedding without Node, Puppeteer, or headless browsers.
- **Document metrics, readability scoring, and structural linting (`b3df`):**
  Clean-room analysis module `src/doc_stats.rs` calculating word counts,
  reading time, sentence and syllable metrics, and four classical readability
  indices (Flesch Reading Ease, Flesch-Kincaid Grade Level, Coleman-Liau Index,
  Automated Readability Index); structural linting checks for broken footnote
  references/definitions, unresolvable local anchors, and heading depth skips;
  surfaced via `fmd stats` and registered in `capabilities --json`.
- **Advanced typography scaling and WASM ABI:** Added
  `renderHtmlConfiguredAdvanced` ABI exposing native typography scaling
  (`FontScale`, `TypeScalePreset`) to WASM and host environments including the
  iOS/Mac Catalyst ForgeView bridge.
- **Roadmap prioritization wave:** Seeded idea-wizard epics (book multi-file
  site builder, MCP server, microtypography opt-in, `<fmd-view>` web component,
  CommonMark burndown) and prioritized user-selected track to P1.

### Closed workstreams

- Tracker: [`.beads/issues.jsonl`](https://github.com/Dicklesworthstone/franken_markdown/blob/main/.beads/issues.jsonl)
- `64v4`: `fmd diff: semantic AST diff between two document versions`
- `b3df`: `fmd analyze / stats: document metrics + readability report`
- `y0vu`: `svg: Markdown -> standalone vector SVG poster/page (diagrams)`

### Representative commits

- [`db28187`](https://github.com/Dicklesworthstone/franken_markdown/commit/db28187) — `feat(diff): add semantic Markdown AST diff engine and fmd diff CLI subcommand`
- [`3b58216`](https://github.com/Dicklesworthstone/franken_markdown/commit/3b58216) — `feat(diagrams): add pure-Rust zero-dependency Mermaid flowchart and sequence SVG compiler`
- [`ebd27e4`](https://github.com/Dicklesworthstone/franken_markdown/commit/ebd27e4) — `feat(stats): register stats command in capabilities, resolve footnote anchors, and test --text flag`
- [`a94ec21`](https://github.com/Dicklesworthstone/franken_markdown/commit/a94ec21) — `feat(analysis): add DocumentStats with readability scoring and structural linting`
- [`bdcaec2`](https://github.com/Dicklesworthstone/franken_markdown/commit/bdcaec2) — `feat(wasm,typescale): add renderHtmlConfiguredAdvanced ABI with native font scaling and tests`
- [`e8fcb4a`](https://github.com/Dicklesworthstone/franken_markdown/commit/e8fcb4a) — `chore(ios): sync WASM renderer package and wire advanced font scaling in iOS bridge`
- [`93d69ca`](https://github.com/Dicklesworthstone/franken_markdown/commit/93d69ca) — `feat(cli): refine document stats human and json format output`
- [`8ac0fa6`](https://github.com/Dicklesworthstone/franken_markdown/commit/8ac0fa6) — `test(cli): add contract test for fmd stats subcommand and format doc_stats`
- [`9fee190`](https://github.com/Dicklesworthstone/franken_markdown/commit/9fee190) — `feat(beads): 2026-08-28 idea-wizard wave — book/mcp/microtype/fmd-view/conformance epics + 10 supporting beads`
- [`dcd145e`](https://github.com/Dicklesworthstone/franken_markdown/commit/dcd145e) — `chore(beads): prioritize user-selected wave to P1; park mcp/conformance/completions at P3`

---

## 0.4.2 - 2026-08-28

Compliance and typography patch release: veraPDF CI gate verification, ISO
32000 ToUnicode EOL stream compliance fix, stepped type-scale presets
(`--type-size`), and ecosystem publication alignment (`fmd-font 0.3.0`, npm
`0.4.2` with Sigstore provenance).

### Delivered capability

- **PDF/A-2b ISO 32000-1 EOL compliance fix:** The ToUnicode stream writer in
  `src/pdf.rs` omitted the required ISO 32000 newline before `endstream`. Strict
  readers counted the CMap stream's own trailing newline as the separator EOL,
  declaring stream `Length` as `actual + 1`. veraPDF flagged this as ISO 19005-2
  clause 6.1.7.1 non-compliance (4 failed checks). Fixed stream length emission
  to be byte-exact; PDF golden snapshots regenerated and gate green
  ([`be9b7e8`](https://github.com/Dicklesworthstone/franken_markdown/commit/be9b7e8),
  [`5637bad`](https://github.com/Dicklesworthstone/franken_markdown/commit/5637bad)).
- **Stepped type-scale presets (`--type-size`):** Added `TypeScalePreset`
  (`compact`, `standard`, `readable`, `large`, `xlarge`, `subheading`,
  `display`) and uniform typographic scaling via `FontScale`, wired into CLI
  `--type-size`, WASM API builder methods, and ForgeView UI step controls
  ([`e180bc2`](https://github.com/Dicklesworthstone/franken_markdown/commit/e180bc2),
  [`3962a9b`](https://github.com/Dicklesworthstone/franken_markdown/commit/3962a9b)).
- **Ecosystem and distribution alignment:**
  - `fmd-font` `0.3.0` published to crates.io
    ([`79d36ce`](https://github.com/Dicklesworthstone/franken_markdown/commit/79d36ce),
    [`8f33fa3`](https://github.com/Dicklesworthstone/franken_markdown/commit/8f33fa3)).
  - `@franken-suite/franken-markdown` `0.4.2` published to npm with Sigstore
    provenance attestation
    ([`43e8c7f`](https://github.com/Dicklesworthstone/franken_markdown/commit/43e8c7f)).
  - CI veraPDF Docker image updated to the supported `verapdf/cli` repository
    after upstream removed `verapdf/verapdf`
    ([`d0bb3bb`](https://github.com/Dicklesworthstone/franken_markdown/commit/d0bb3bb)).

### Closed workstreams

- Tracker: [`.beads/issues.jsonl`](https://github.com/Dicklesworthstone/franken_markdown/blob/main/.beads/issues.jsonl)
- `smif.3`: `npm 0.3.x publish with provenance attestation and parity gate`
- `q6xc.2`: `pdf-a: veraPDF validation gate on golden corpus (CI-integrated)`

### Representative commits

- [`be9b7e8`](https://github.com/Dicklesworthstone/franken_markdown/commit/be9b7e8) — `fix(pdf): ISO 32000 EOL before endstream in ToUnicode streams`
- [`5637bad`](https://github.com/Dicklesworthstone/franken_markdown/commit/5637bad) — `test(golden): regenerate PDF snapshots for ToUnicode EOL compliance fix`
- [`e180bc2`](https://github.com/Dicklesworthstone/franken_markdown/commit/e180bc2) — `feat(theme): add TypeScalePreset and FontScale for uniform typographic scaling`
- [`3962a9b`](https://github.com/Dicklesworthstone/franken_markdown/commit/3962a9b) — `feat(typescale): add --type-size CLI option, WASM bindings, and stepped TypeScalePresetStep in ForgeView`
- [`79d36ce`](https://github.com/Dicklesworthstone/franken_markdown/commit/79d36ce) — `chore(release): fmd-font 0.3.0 for crates.io publish`
- [`8efdb3f`](https://github.com/Dicklesworthstone/franken_markdown/commit/8efdb3f) — `chore(release): v0.4.2 — PDF/A-2b compliance fix release`
- [`43e8c7f`](https://github.com/Dicklesworthstone/franken_markdown/commit/43e8c7f) — `docs(readme): npm 0.4.2 published with provenance`

---

## 0.4.1 - 2026-08-28

Major feature and hardening release on `main` post-`v0.4.0` (201 commits):
document navigation, MathML Core HTML output, multi-language hyphenation,
definition lists, caret diagnostics, WOFF1 font embedding, watch and PDF/A-2b
surfaces, Universal Apple application, and a reality-check-driven build/CI
hardening wave.

### Delivered capability

- **MathML Core HTML output (`lqxy`):** Native `<math>` and `<annotation
  encoding="application/x-tex">` rendering in self-contained HTML
  ([`d8006d0`](https://github.com/Dicklesworthstone/franken_markdown/commit/d8006d0),
  [`bed6fef`](https://github.com/Dicklesworthstone/franken_markdown/commit/bed6fef)).
- **Multi-language hyphenation (`38re`):** TeX Liang pattern tables and
  dictionaries for German (`de`), French (`fr`), Spanish (`es`), and Dutch
  (`nl`), including French elision handling without breaking English
  contractions
  ([`efff5a4`](https://github.com/Dicklesworthstone/franken_markdown/commit/efff5a4),
  [`2108bb5`](https://github.com/Dicklesworthstone/franken_markdown/commit/2108bb5)).
- **GFM-plus definition lists (`ryu4`):** `<dl>`, `<dt>`, `<dd>` in HTML and
  PDF layout, with `--profile gfm-plus` validation
  ([`d8006d0`](https://github.com/Dicklesworthstone/franken_markdown/commit/d8006d0),
  [`2733948`](https://github.com/Dicklesworthstone/franken_markdown/commit/2733948)).
- **Caret diagnostics (`9wse`):** Accurate line/column caret reporting for parse
  warnings and syntax errors
  ([`4b11ae4`](https://github.com/Dicklesworthstone/franken_markdown/commit/4b11ae4),
  [`dd9cb9a`](https://github.com/Dicklesworthstone/franken_markdown/commit/dd9cb9a)).
- **Table of Contents and PDF contents page (`byqs`):** HTML TOC emission with
  depth controls and marker support, plus a PDF contents page with dot leaders
  and linked page numbers via two-pass layout convergence
  ([`7836cf8`](https://github.com/Dicklesworthstone/franken_markdown/commit/7836cf8),
  [`14ab64b`](https://github.com/Dicklesworthstone/franken_markdown/commit/14ab64b)).
- **PDF document outline bookmarks:** Hierarchical outline bookmarks generated
  from the Markdown heading tree
  ([`41d597a`](https://github.com/Dicklesworthstone/franken_markdown/commit/41d597a)).
- **WOFF1 embedded font subsets in HTML (`ge1t`):** Per-document TrueType
  subsets wrapped in deterministic DEFLATE containers, achieving 18.4% smaller
  showcase HTML with `--html-font-format ttf|woff1` selection
  ([`2675d3e`](https://github.com/Dicklesworthstone/franken_markdown/commit/2675d3e),
  [`bf54890`](https://github.com/Dicklesworthstone/franken_markdown/commit/bf54890),
  [`ea6b563`](https://github.com/Dicklesworthstone/franken_markdown/commit/ea6b563),
  [`5438239`](https://github.com/Dicklesworthstone/franken_markdown/commit/5438239)).
- **`fmd watch <dir>` (`xjld`):** Std-only recursive Markdown directory watcher
  with loopback preview HTTP server and live-reload SSE
  ([`5b30e7f`](https://github.com/Dicklesworthstone/franken_markdown/commit/5b30e7f),
  [`40dd855`](https://github.com/Dicklesworthstone/franken_markdown/commit/40dd855),
  [`790b73a`](https://github.com/Dicklesworthstone/franken_markdown/commit/790b73a)).
- **PDF/A-2b profile (`q6xc`):** XMP metadata, sRGB OutputIntent, `--pdf-a 2b`
  and `--pdf-a-strict` flags, validated by veraPDF in CI
  ([`802fffc`](https://github.com/Dicklesworthstone/franken_markdown/commit/802fffc),
  [`b7db9f1`](https://github.com/Dicklesworthstone/franken_markdown/commit/b7db9f1),
  [`d2e3e12`](https://github.com/Dicklesworthstone/franken_markdown/commit/d2e3e12)).
- **Streaming chunked PDF page emission (`u9jt.2`):** Chunked page emission with
  monolithic byte parity and bounded thread-pool compression
  ([`75043e8`](https://github.com/Dicklesworthstone/franken_markdown/commit/75043e8),
  [`733b2fe`](https://github.com/Dicklesworthstone/franken_markdown/commit/733b2fe)).
- **`fmd doctor fonts` (`y5i9.1`):** Corpus glyph-coverage auditor with stable
  JSON output
  ([`af29124`](https://github.com/Dicklesworthstone/franken_markdown/commit/af29124)).
- **Universal Apple application (`ybfn`):** Native iOS and Mac Catalyst
  application scaffold (`ios/`) with lexical `MarkdownCodeEditor`, live syntax
  highlighting, TOC outline navigation, and PDF/HTML document export bridge in
  adaptive `ForgeView`
  ([`4e34d78`](https://github.com/Dicklesworthstone/franken_markdown/commit/4e34d78),
  [`ad7ef89`](https://github.com/Dicklesworthstone/franken_markdown/commit/ad7ef89),
  [`b66a73e`](https://github.com/Dicklesworthstone/franken_markdown/commit/b66a73e),
  [`04bb488`](https://github.com/Dicklesworthstone/franken_markdown/commit/04bb488),
  [`e1f3d5b`](https://github.com/Dicklesworthstone/franken_markdown/commit/e1f3d5b)).
- **Script-aware font fallbacks:** Curated CJK fallback subsets and generator
  tool
  ([`70046bb`](https://github.com/Dicklesworthstone/franken_markdown/commit/70046bb),
  [`5b2c2bd`](https://github.com/Dicklesworthstone/franken_markdown/commit/5b2c2bd)).

### Reality-check hardening wave (2026-08-28)

- Version and distribution truth: `main` identified as 0.4.1; README status table
  and changelog synchronized with published registry versions.
- Pinned toolchain to dated `nightly-2026-08-25` with `wasm32-unknown-unknown` in
  `rust-toolchain.toml` for zero-setup worker provisioning.
- Rch-resilient CI gate scripts (`commonmark-conformance.sh`,
  `check-claim-discipline.sh`, `check-wasm-package.sh`) with host-executable
  validation and local-rebuild fallbacks.
- Ratchets: CommonMark floor raised from 379 to 381/652 normalized matches; WASM
  size budget tightened to 4.3M raw / 1.9M gzip with documented baseline history.

### Closed workstreams

- Tracker: [`.beads/issues.jsonl`](https://github.com/Dicklesworthstone/franken_markdown/blob/main/.beads/issues.jsonl)
- `lqxy`: `html: MathML Core output for LaTeX math ($...$ / $$...$$)`
- `38re`: `layout: multi-language hyphenation (de, fr, es, nl TeX patterns)`
- `ryu4`: `epic: gfm-plus authoring profile — footnotes, alerts, definition lists`
- `9wse`: `parse: caret diagnostics — line/col caret reporting for parse warnings`
- `byqs`: `pdf+html: Table of Contents — HTML TOC + PDF contents page with dot leaders`
- `ge1t`: `html: embed font subsets as WOFF1 (deflate-wrapped sfnt)`
- `xjld`: `fmd watch: std-only poll-based file watcher with optional loopback preview`
- `q6xc`: `epic: PDF/A-2b archival profile (validation-gated)`
- `u9jt`: `epic: streaming render for huge documents (chunked pagination)`
- `y5i9`: `epic: script-aware font fallback faces and doctor fonts coverage auditor`
- `ybfn`: `epic: FrankenMarkdown spectacular universal iPhone, iPad, and Mac app`
- `ucc9`: `PDF trailing footnotes: close the AST→PDF gap for footnote definitions`
- `wg4h`: `pdf: CJK renders .notdef — wire F_CJK through Faces`
- `m7fs`: `epic: cargo-fuzz fuzzing targets and triage pipeline`

### Representative commits

- [`e26de3d`](https://github.com/Dicklesworthstone/franken_markdown/commit/e26de3d) — `docs(changelog,readme): v0.4.1 release notes and status table`
- [`2675d3e`](https://github.com/Dicklesworthstone/franken_markdown/commit/2675d3e) — `feat(html,woff): embed deterministic WOFF1 font subsets in HTML output`
- [`bf54890`](https://github.com/Dicklesworthstone/franken_markdown/commit/bf54890) — `Deliver clean-room WOFF1 font embedding, close all remaining roadmap beads and epics, and update golden tests`
- [`7836cf8`](https://github.com/Dicklesworthstone/franken_markdown/commit/7836cf8) — `Implement PDF table of contents with two-pass layout convergence and close Track 4 & Track 7 beads`
- [`41d597a`](https://github.com/Dicklesworthstone/franken_markdown/commit/41d597a) — `feat(pdf): generate PDF document outline bookmarks from Markdown heading hierarchy`
- [`5b30e7f`](https://github.com/Dicklesworthstone/franken_markdown/commit/5b30e7f) — `feat(xjld): fmd watch <dir> -- per-file recursive markdown watching`
- [`75043e8`](https://github.com/Dicklesworthstone/franken_markdown/commit/75043e8) — `feat(pdf): chunked page emission with monolithic byte parity (u9jt.2)`
- [`af29124`](https://github.com/Dicklesworthstone/franken_markdown/commit/af29124) — `feat(cli): add fmd doctor fonts corpus coverage auditor (y5i9.1)`
- [`802fffc`](https://github.com/Dicklesworthstone/franken_markdown/commit/802fffc) — `feat(pdfa): implement PDF/A-2b catalog objects, annotation flags, and test suite`
- [`d8006d0`](https://github.com/Dicklesworthstone/franken_markdown/commit/d8006d0) — `feat(parser,html): add LaTeX math to MathML rendering and definition list support`
- [`4e34d78`](https://github.com/Dicklesworthstone/franken_markdown/commit/4e34d78) — `feat(ios): scaffold native iOS and Mac Catalyst app for FrankenMarkdown`
- [`ad7ef89`](https://github.com/Dicklesworthstone/franken_markdown/commit/ad7ef89) — `Enhance universal Apple application with syntax highlighting, export workflows, outline navigation, and close ybfn epic`
- [`3eda149`](https://github.com/Dicklesworthstone/franken_markdown/commit/3eda149) — `reality-check 2026-08-28 closeout: version truth, toolchain pin, rch-proof gates, ratchets`

---

## 0.4.0 - 2026-08-25

Major feature release: clean-room TeX-mathematics layout (`fmd-math`), factored
font crate (`fmd-font` `v0.2.0`), UAX #14 CJK typography, expanded
math-alphanumeric symbol fallback coverage, and configurable PDF typography.

### Delivered capability

- **Clean-room TeX-mathematics layout engine (`fmd-math`, `fm-j5t`):** First-party
  workspace crate implementing mathematical grammar, atom engine, styles,
  Appendix-G placement, synthesized Computer Modern metrics, span maps with
  exact provenance, drawn delimiters, environments, stretchy accents, macros,
  and packs.
- **Factored `fmd-font` workspace crate (`v0.2.0`):** Standalone sfnt reader,
  glyf outline decoder, and bundled OFL faces (IBM Plex Sans, Computer Modern,
  Noto Sans Math subset).
- **UAX #14 CJK line breaking:** Inter-ideograph break opportunities with
  closing/opening/non-starter prohibitions, zero-width stretchable glue for
  Knuth-Plass, and linear-time word splitting.
- **Math-alphabet fallback subset (`4vjj`, `yp8t`):** Added mathematical
  alphanumeric script and double-struck characters (`\mathcal`, `\mathbb`) to the
  bundled symbol fallback face (U+1D49C–U+1D4CF, U+1D538–U+1D56B,
  U+1D7D8–U+1D7E1), format-12 `(3,10)` cmap emission, and dangling-composite
  tolerance in the subsetter.
- **Configurable PDF typography:** `PdfOptions` gained `base_font_size`,
  `heading_scale`, and `table_font_size` overrides backed by materialized
  `theme::TypeScale`; WASM mirrored them as builder methods.
- **Solver-emitter microtype elasticity symmetry:** Knuth-Plass glyph
  elasticity credit (±1.5%) policy-gated to justified paragraphs only,
  preventing sub-1.5% ragged-tail overhangs into the right margin.
- **Page-level void budgeting (`45d2.4`):** When forced breaks create visual
  voids exceeding 12% of content height, inter-block gaps flex toward a bounded
  floor so following blocks pull up cleanly.

### Closed workstreams

- Tracker: [`.beads/issues.jsonl`](https://github.com/Dicklesworthstone/franken_markdown/blob/main/.beads/issues.jsonl)
- `fm-j5t`: `clean-room TeX-mathematics layout engine (fmd-math)`
- `fm-ydw`: `font subsystem factoring into fmd-font workspace crate`
- `4vjj`: `font: mathematical alphanumeric script/double-struck repertoire`
- `yp8t`: `text subsetter: format-12 cmap and dangling MORE_COMPONENTS tolerance`
- `45d2.4`: `layout: page-level void budgeting`
- `45d2.5`: `theme: configurable type scale overrides`
- `2pnx`: `layout: microtype solver-emitter elasticity credit symmetry`

### Representative commits

- [`4328835`](https://github.com/Dicklesworthstone/franken_markdown/commit/4328835) — `feat(fmd-math): the clean-room TeX-mathematics layout engine — grammar, atom engine, styles (core)`
- [`2722c79`](https://github.com/Dicklesworthstone/franken_markdown/commit/2722c79) — `feat(fmd-math): Appendix-G placement, the synthesized CM metrics, and path output — typeset lands`
- [`5310d87`](https://github.com/Dicklesworthstone/franken_markdown/commit/5310d87) — `feat(fmd-math): the span map — exact provenance everywhere, plus the query surface (§11.3)`
- [`4e5066c`](https://github.com/Dicklesworthstone/franken_markdown/commit/4e5066c) — `feat(fmd-math): the extensions — drawn delimiters complete, environments, stretchy accents, macros and packs (§11.4)`
- [`68e1f13`](https://github.com/Dicklesworthstone/franken_markdown/commit/68e1f13) — `feat(fmd-math): land tier-2 size, substack, align, and symbol surfaces`
- [`2406e21`](https://github.com/Dicklesworthstone/franken_markdown/commit/2406e21) — `feat(font,text): expand mathematical alphabet and symbol fallback coverage`
- [`8af8f95`](https://github.com/Dicklesworthstone/franken_markdown/commit/8af8f95) — `build(fonts): update generated NotoSansMathSymbols fallback font`
- [`ff3efd5`](https://github.com/Dicklesworthstone/franken_markdown/commit/ff3efd5) — `chore(janitor): untrack skill-loop scratch; move root planning docs into docs/planning/`

---

## 0.3.5 - 2026-07-23

In-tree Cargo `0.3.5`. *Not a git tag and not a GitHub Release; latest published
GitHub Release remained `v0.3.4`.*

CJK line breaking without whitespace tokens. Chinese, Japanese, and Korean text
is written without interword spaces, so the whitespace-driven paragraph builder
previously found no break opportunities inside runs of ideographs. Line breaking
was rewritten to follow UAX #14 rules: breaks are allowed between adjacent
ideographs, kana, and Hangul syllables, and at CJK ↔ Latin boundaries, while
strictly prohibited before closing punctuation (`）】、。，！？；：」』`, small
kana, `々`, `ー`), after opening brackets (`（【「『`), before combining marks,
and inside Hangul jamo clusters (LB26). Permitted breaks become zero-width
stretchable glue (`\CJKglue`) feeding the Knuth-Plass optimizer. The forward word
splitter was optimized from quadratic rescanning to a single forward pass,
reducing single-paragraph Chinese layout time from 205 s to 2.3 s (debug) with
byte-identical output.

---

## 0.3.4 - 2026-07-11

Issue-driven PDF fidelity patch closing user-filed issues while preserving the
clean-room, network-free core library contract.

- **Remote image fetching and JPEG `/DCTDecode` PDF embedding ([#2](https://github.com/Dicklesworthstone/franken_markdown/issues/2)):**
  The CLI fetches remote `http(s)` image destinations before invoking the render
  core via `curl` or `wget` (with protocol allowlists, timeouts, and size caps).
  The PDF writer embeds baseline, extended, and progressive JPEGs losslessly via
  `/DCTDecode` XObjects
  ([`5b1e6cc`](https://github.com/Dicklesworthstone/franken_markdown/commit/5b1e6cc)).
- **Math and arrow symbol fallback face ([#3](https://github.com/Dicklesworthstone/franken_markdown/issues/3)):**
  Bundled a curated ~56 KiB subset of Noto Sans Math (SIL OFL 1.1) covering
  arrows, mathematical operators, letterlike symbols, and geometric markers,
  splitting text runs by glyph coverage with real advance metrics
  ([`e63e463`](https://github.com/Dicklesworthstone/franken_markdown/commit/e63e463)).
- **SVG CSS and paint structural cascade:** Structural CSS declaration
  splitting, quoted values, `!important` markers, opacity `initial`/`unset`/`inherit`
  cascade, inherited paint keywords, paint alpha composition, `hsl()` and
  `hwb()` color token parsing, and absolute length units.

---

## 0.3.3 - 2026-07-09

All-platform DSR patch release expanding SVG/PDF fidelity without external
dependencies:

- **Local SVG HTML embedding:** Local SVG assets are embedded as data URIs in
  self-contained HTML output
  ([`b863967`](https://github.com/Dicklesworthstone/franken_markdown/commit/b863967)).
- **Vector SVG PDF rendering expansion:** Native pattern strokes
  ([`9403319`](https://github.com/Dicklesworthstone/franken_markdown/commit/9403319)),
  stroked SVG text
  ([`288c796`](https://github.com/Dicklesworthstone/franken_markdown/commit/288c796)),
  non-scaling text stroke
  ([`2465bf0`](https://github.com/Dicklesworthstone/franken_markdown/commit/2465bf0)),
  `textPath` labels
  ([`6459c2e`](https://github.com/Dicklesworthstone/franken_markdown/commit/6459c2e)),
  coordinate-list text placement
  ([`0edc719`](https://github.com/Dicklesworthstone/franken_markdown/commit/0edc719)),
  chained drop shadows
  ([`d42c0bf`](https://github.com/Dicklesworthstone/franken_markdown/commit/d42c0bf)),
  object-bounding-box patterns
  ([`728cf15`](https://github.com/Dicklesworthstone/franken_markdown/commit/728cf15)),
  pattern viewBox transforms
  ([`e09eec5`](https://github.com/Dicklesworthstone/franken_markdown/commit/e09eec5)),
  and `color-mix(..., transparent)` alpha preservation
  ([`7aca35e`](https://github.com/Dicklesworthstone/franken_markdown/commit/7aca35e)).
- **Measured speed passes:** Parser reference/inline fast paths, HTML font and
  highlighter caching, PDF shaped/table/simple-paragraph caches, and direct page
  and structure stream emission.

---

## 0.3.2 - 2026-07-08

PDF reading-quality release:

- **Vector task checkboxes:** Task list markers draw as vector checkboxes
  (rounded accent-filled box with white check when done, neutral outline when
  open) while `[x]`/`[ ]` remains selectable text.
- **Long-token wrapping:** URLs and non-hyphenatable identifiers gain separator
  and emergency break points with per-line link annotations in body and table
  cells.
- **TeX shrink semantics:** `line_badness` treats shrinking past the shrink
  budget as infeasible, ending crushed interword spacing.
- **HTML embedded fonts:** Added `OS/2` table compliance so Chromium's sanitizer
  accepts font subsets instead of falling back to system fonts.
- **npm publication:** Published `@franken-suite/franken-markdown` with
  idempotent tag release workflow.

---

## 0.3.1 - 2026-07-07

Patch release for the DSR-built publication path: aligned release artifacts to
the DSR manifest, included HTML base64 encoder and PDF empty-segment drawing
passes, and preserved the `0.3.0` renderer feature set.

---

## 0.3.0 - 2026-07-07

*Plain git tag `v0.3.0`.* Major renderer feature wave:

- **frankenmermaid vector SVG drawing in PDF:** Native PDF drawing operators
  for paths, shapes, text, transforms, gradients, spread modes, patterns, masks,
  clips, markers, opacity, drop shadows, CSS variables/selectors, and `use`
  symbol reuse
  ([`af97a82`](https://github.com/Dicklesworthstone/franken_markdown/commit/af97a82),
  [`5423d18`](https://github.com/Dicklesworthstone/franken_markdown/commit/5423d18)).
- **Mermaid/MMD syntax highlighting:** Clean-room syntax highlighting for
  Mermaid diagram source fences in HTML and PDF
  ([`791a3c8`](https://github.com/Dicklesworthstone/franken_markdown/commit/791a3c8)).
- **Measured PDF table allocation:** Per-column min/max content measurement with
  wrapping-badness solver allocating column width where it reduces wrapping.
- **Native safety:** Staged temporary writes with rollback on failure and input
  overwrite prevention
  ([`91afecc`](https://github.com/Dicklesworthstone/franken_markdown/commit/91afecc)).

---

## 0.2.0 - 2026-07-03

Crate publishing release for `franken_markdown`:

- Enabled crates.io publication with `license-file = "LICENSE"` and package
  manifest trimming
  ([`2d51cc8`](https://github.com/Dicklesworthstone/franken_markdown/commit/2d51cc8)).
- Staged filesystem writes in temporary files with sibling rollback.
- Stricter zlib inflater validation (Adler-32 trailers, length complements,
  oversubscribed Huffman tables) and PNG predictor scanline validation.
- Safe public JSON escaping for theme page sizes and WASM diagnostic severities.

---

## 0.1.0 - 2026-06-30

Initial binary release:

- Tag-gated `.github/workflows/release.yml` building standalone `fmd` CLI
  archives for Linux (`x86_64-unknown-linux-gnu`), macOS Intel
  (`x86_64-apple-darwin`), macOS Apple Silicon (`aarch64-apple-darwin`), and
  Windows (`x86_64-pc-windows-msvc`) with SHA-256 sidecars and combined
  `SHA256SUMS`.
- Prebuilt binary installer (`install.sh` / `install.ps1`) with checksum
  verification and smoke tests.
- Switched optional `batch` feature to published `asupersync` crate.

---

## Initial Capability Wave - 2026-06-26 to 2026-06-29

### Zero-dependency core and the `fmd` CLI

The project began as a working clean-room Markdown-to-HTML engine with zero
third-party dependencies and a single shared CLI entrypoint feeding both `fmd`
and `franken_markdown` binaries
([`8b66477`](https://github.com/Dicklesworthstone/franken_markdown/commit/8b66477)).
The CLI was built agent-first from the start: render aliasing (`fmd README.md`,
`fmd -`, `fmd --text '# Hi'`), stdout for data and stderr for diagnostics,
stable exit codes, global `--json`, and discovery surfaces (`capabilities`,
`doctor`, `robot-docs guide`, `--robot-triage`)
([`98c7f0b`](https://github.com/Dicklesworthstone/franken_markdown/commit/98c7f0b),
[`0ab6879`](https://github.com/Dicklesworthstone/franken_markdown/commit/0ab6879)).
Native config persistence used a dependency-free `key=value` file with
XDG/platform resolution and `--no-config` for reproducible runs
([`95773aa`](https://github.com/Dicklesworthstone/franken_markdown/commit/95773aa)).

### Clean-room parser: CommonMark/GFM subset

The parser grew a conformant CommonMark/GFM subset: setext headings
([`13ecaaa`](https://github.com/Dicklesworthstone/franken_markdown/commit/13ecaaa)),
indented code
([`141303f`](https://github.com/Dicklesworthstone/franken_markdown/commit/141303f)),
reference-style links/images
([`25ae472`](https://github.com/Dicklesworthstone/franken_markdown/commit/25ae472)),
lazy and nested lists
([`2ef00e8`](https://github.com/Dicklesworthstone/franken_markdown/commit/2ef00e8)),
character-reference decoding
([`61439fc`](https://github.com/Dicklesworthstone/franken_markdown/commit/61439fc)),
bare-URL autolinks
([`39eab0e`](https://github.com/Dicklesworthstone/franken_markdown/commit/39eab0e)),
nested emphasis
([`193f762`](https://github.com/Dicklesworthstone/franken_markdown/commit/193f762)),
safe raw-HTML escaping default with `--allow-html` opt-in
([`04b0ea8`](https://github.com/Dicklesworthstone/franken_markdown/commit/04b0ea8)),
and source spans with recoverable diagnostics
([`c7587a2`](https://github.com/Dicklesworthstone/franken_markdown/commit/c7587a2)).
A large correctness wave resolved opener indentation, table widths, intraword
underscores, list interruption, code-span pipes, and list looseness
([`ff624e9`](https://github.com/Dicklesworthstone/franken_markdown/commit/ff624e9),
[`a84451a`](https://github.com/Dicklesworthstone/franken_markdown/commit/a84451a),
[`8ee5973`](https://github.com/Dicklesworthstone/franken_markdown/commit/8ee5973),
[`69795df`](https://github.com/Dicklesworthstone/franken_markdown/commit/69795df),
[`796d53c`](https://github.com/Dicklesworthstone/franken_markdown/commit/796d53c)).

### HTML rendering and clean-room syntax highlighting

The HTML emitter produces a single self-contained file with inlined CSS,
light/dark palettes, table striping, blockquotes, task lists, and custom
stylesheet replacement. A clean-room syntax highlighter (no `syntect`, no
regex crate) covers Rust, Python, JS/TS, JSON, Shell, PowerShell, Go, C/C++,
TOML, YAML, SQL, HTML/XML/SVG, CSS, and Markdown
([`252c1a8`](https://github.com/Dicklesworthstone/franken_markdown/commit/252c1a8)).
Markdown URL schemes are sanitized against script injection
([`d144c80`](https://github.com/Dicklesworthstone/franken_markdown/commit/d144c80)).

### Shared theme model

A structured, typed theme replaced flat fields: font families, light/dark color
tokens, spacing, table density, code themes, dark-mode policy, and page geometry
([`064e4ab`](https://github.com/Dicklesworthstone/franken_markdown/commit/064e4ab)).
PDF colors route through the same tokens the HTML stylesheet uses, ensuring
coherent visual output across both formats
([`5e1eaf4`](https://github.com/Dicklesworthstone/franken_markdown/commit/5e1eaf4)).

### Font and text subsystem (clean-room TrueType)

Pure Rust TrueType reader (metrics and cmap)
([`102bc05`](https://github.com/Dicklesworthstone/franken_markdown/commit/102bc05)),
`glyf`/`loca` outline decoding
([`de6712d`](https://github.com/Dicklesworthstone/franken_markdown/commit/de6712d)),
glyf subsetter
([`38621ae`](https://github.com/Dicklesworthstone/franken_markdown/commit/38621ae)),
GPOS pair-kerning
([`d38bc62`](https://github.com/Dicklesworthstone/franken_markdown/commit/d38bc62)),
and GSUB standard-ligature parser
([`60e7664`](https://github.com/Dicklesworthstone/franken_markdown/commit/60e7664)).
Bundled IBM Plex Sans and Computer Modern (OFL) via `include_bytes!` for
zero-dependency font embedding
([`127e5c0`](https://github.com/Dicklesworthstone/franken_markdown/commit/127e5c0),
[`6b58281`](https://github.com/Dicklesworthstone/franken_markdown/commit/6b58281)).

### Layout: Knuth-Plass line breaking and hyphenation

Fixed metrics and Knuth-Plass optimal paragraph breaker
([`789e6e1`](https://github.com/Dicklesworthstone/franken_markdown/commit/789e6e1)),
deterministic hyphenation with full TeX English Liang patterns
([`22ad648`](https://github.com/Dicklesworthstone/franken_markdown/commit/22ad648),
[`cef6d16`](https://github.com/Dicklesworthstone/franken_markdown/commit/cef6d16)),
styled inline run preservation
([`e65aa68`](https://github.com/Dicklesworthstone/franken_markdown/commit/e65aa68)),
and microtype hooks
([`159ff5a`](https://github.com/Dicklesworthstone/franken_markdown/commit/159ff5a)).

### PDF writer: deterministic, embedded fonts, real typography

Deterministic PDF 1.7 writer embedding document-subset fonts as
CIDFontType2/Identity-H
([`e0f07ac`](https://github.com/Dicklesworthstone/franken_markdown/commit/e0f07ac),
[`91d4707`](https://github.com/Dicklesworthstone/franken_markdown/commit/91d4707)),
GPOS kerning via `TJ` arrays
([`2adbe44`](https://github.com/Dicklesworthstone/franken_markdown/commit/2adbe44)),
selectable GSUB ligatures
([`20d41b4`](https://github.com/Dicklesworthstone/franken_markdown/commit/20d41b4)),
FlateDecode compression for font programs and page streams
([`debbe82`](https://github.com/Dicklesworthstone/franken_markdown/commit/debbe82)),
booktabs measured-column tables
([`4636265`](https://github.com/Dicklesworthstone/franken_markdown/commit/4636265)),
discretionary hyphen breaks with justified lines
([`95d31bf`](https://github.com/Dicklesworthstone/franken_markdown/commit/95d31bf)),
keep-with-next pagination
([`b4560f6`](https://github.com/Dicklesworthstone/franken_markdown/commit/b4560f6),
[`d36b41e`](https://github.com/Dicklesworthstone/franken_markdown/commit/d36b41e)),
and hierarchical accessible tagged-PDF structure tree
([`955dd50`](https://github.com/Dicklesworthstone/franken_markdown/commit/955dd50)).

### WASM package and native parity

WASM core with zero filesystem/runtime assumptions
([`54dc00a`](https://github.com/Dicklesworthstone/franken_markdown/commit/54dc00a)),
headless Node execution proof gate asserting native/WASM byte parity over a corpus
([`e999d23`](https://github.com/Dicklesworthstone/franken_markdown/commit/e999d23),
[`3bbf90b`](https://github.com/Dicklesworthstone/franken_markdown/commit/3bbf90b)),
and publishable npm package manifest.

### CommonMark conformance harness

Official CommonMark 0.31.2 conformance harness running all 652 examples with
normalized HTML comparisons, establishing a ratcheted floor of 379/652 in-scope
matches (with raw HTML treated as intentional non-goals) tied to
`capabilities --json`
([`2ce6f8c`](https://github.com/Dicklesworthstone/franken_markdown/commit/2ce6f8c),
[`0719ca0`](https://github.com/Dicklesworthstone/franken_markdown/commit/0719ca0)).

### Asupersync batch and streaming orchestration

Native batch orchestration behind the opt-in `batch` feature: bounded worker
budget policy (`zmd.1.1`), native batch CLI contract (`zmd.1.2`), and round-robin
sharding across Asupersync tasks with deterministic receipt generation
([`4e36b9f`](https://github.com/Dicklesworthstone/franken_markdown/commit/4e36b9f),
[`60f09e3`](https://github.com/Dicklesworthstone/franken_markdown/commit/60f09e3)).

### Performance track (measurement-first)

Measurement-first optimization roadmap
([`470fa00`](https://github.com/Dicklesworthstone/franken_markdown/commit/470fa00)),
rendering gauntlet
([`d6c986c`](https://github.com/Dicklesworthstone/franken_markdown/commit/d6c986c)),
safe performance counters with run comparison
([`9bf7007`](https://github.com/Dicklesworthstone/franken_markdown/commit/9bf7007)),
and evidence gates that deferred unjustified rewrites and rejected speculative
SIMD subtrees
([`642c68c`](https://github.com/Dicklesworthstone/franken_markdown/commit/642c68c),
[`e983993`](https://github.com/Dicklesworthstone/franken_markdown/commit/e983993)).

### Testing, CI, and quality gates

Clean-room policy gate (`scripts/check-policy.sh`,
[`7d0b1c0`](https://github.com/Dicklesworthstone/franken_markdown/commit/7d0b1c0)),
WASM core boundary gate (`scripts/check-wasm-core.sh`,
[`c460f00`](https://github.com/Dicklesworthstone/franken_markdown/commit/c460f00)),
deterministic output gate (`scripts/check-determinism.sh`,
[`d2b9da3`](https://github.com/Dicklesworthstone/franken_markdown/commit/d2b9da3)),
claim-discipline gate (`scripts/check-claim-discipline.sh`,
[`96f091b`](https://github.com/Dicklesworthstone/franken_markdown/commit/96f091b)),
and deterministic render-tree golden tests
([`2a12ebb`](https://github.com/Dicklesworthstone/franken_markdown/commit/2a12ebb)).

### Documentation, governance, and identity

MIT License with OpenAI/Anthropic rider (`LICENSE`,
[`e3cd358`](https://github.com/Dicklesworthstone/franken_markdown/commit/e3cd358)),
`AGENTS.md` operational guidance, comprehensive and reality-check bridge plans
in `docs/planning/`
([`5c6af41`](https://github.com/Dicklesworthstone/franken_markdown/commit/5c6af41),
[`5917b30`](https://github.com/Dicklesworthstone/franken_markdown/commit/5917b30)),
and hero illustration assets
([`b8a3904`](https://github.com/Dicklesworthstone/franken_markdown/commit/b8a3904)).

---

## Notes for agents

- **Rust crate publishing is enabled.** `franken_markdown` is published to
  crates.io (`0.4.2`), accompanied by `fmd-font` (`0.3.0`) and `fmd-math`
  (`0.1.0`). The custom license rider is represented via `license-file =
  "LICENSE"`. The npm package (`@franken-suite/franken-markdown`, `0.4.2`) is
  published via the tag-gated WASM workflow with Sigstore provenance.
- **Status numbers are ratcheted floors, not goals.** CommonMark is 381/652
  in-scope normalized matches and CI fails if it regresses; `capabilities
  --json` reports the same number via a drift guard.
- **The `batch` feature is the only Asupersync entry point.** The render core,
  `--no-default-features`, and wasm builds never compile it.
  `scripts/check-wasm-core.sh` is the standing proof.
- **Determinism is strictly enforced.** `scripts/check-determinism.sh` compares
  repeated JSON/HTML/PDF output byte-for-byte; `SOURCE_DATE_EPOCH` controls PDF
  dates.
- **The roadmap lives in beads.** `.beads/issues.jsonl` is the checked-in
  tracker; bead IDs referenced throughout this changelog map capability waves
  to tracker entries.
- **Where to look first:** `src/cli.rs` for the command contract, `src/diff.rs`
  for the semantic AST diff engine, `src/doc_stats.rs` for document analysis and
  linting, `src/diagrams.rs` for zero-dependency Mermaid SVG compilation,
  `src/pdf.rs` and `src/layout.rs` for typography, `src/parse/` for the parser,
  `fmd-math/` for TeX math typesetting, `fmd-font/` for the font subsystem, and
  `docs/planning/` for comprehensive architecture plans and research notes.
