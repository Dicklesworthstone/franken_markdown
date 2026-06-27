# Comprehensive Plan For franken_markdown

Date: 2026-06-26  
Status: pre-Phase-0 plan, scaffold exists  
Product shape: Rust library + `fmd` CLI + first-class WASM renderer

## 1. Goal

Build the best focused Markdown renderer possible:

```text
Markdown file / stdin / raw Markdown text -> beautiful HTML and/or tiny PDF
```

The default output should feel like an excellent Cursor-style Markdown preview:
quiet, readable, polished, and optimized for real documents. Users can provide
custom stylesheets for HTML. The PDF path should go beyond browser print output
with real typography: Knuth-Plass line breaking, kerning, ligatures, leading,
hyphenation, pagination, and embedded subset fonts.

## 2. Non-Goals

- Do not wrap a browser engine.
- Do not build a general HTML/CSS-to-PDF engine.
- Do not depend on Typst, Pandoc, `comrak`, `pulldown-cmark`, `syntect`,
  `cosmic-text`, `krilla`, Blitz, or similar dependency forests.
- Do not put native filesystem/runtime assumptions in the render core.
- Do not sacrifice PDF validity or visual fidelity for premature speed claims.

## 3. Product Surfaces

### 3.1 Rust Library

The library exposes:

- `parse(&str) -> Document`
- `render_html(&str, &HtmlOptions) -> Result<String>`
- `render_pdf(&str, &PdfOptions) -> Result<Vec<u8>>`

Planned additions:

- render from an already parsed `Document`,
- incremental parse/render APIs for large inputs,
- stable option structs for page size, margins, font family, syntax theme, and
  output determinism,
- WASM-safe API accepting strings, option structs, and font/style bytes.

### 3.2 CLI

The `fmd` CLI must satisfy first-try commands:

```bash
fmd README.md --out README.html
fmd - --out stdin.html < README.md
fmd --text '# Hi' --out hi.html
fmd render README.md --to both --out README.html
fmd capabilities --json
fmd doctor --json
fmd robot-docs guide
```

Agent ergonomics requirements:

- stdout is data, stderr is diagnostics,
- stable exit codes,
- JSON discovery surfaces,
- no blocking TUI on bare invocation,
- helpful errors with exact next commands,
- deterministic output by default.

### 3.3 WASM

WASM is first-class.

Planned WASM package shape:

- feature-gated `wasm` export surface,
- no CLI, filesystem, process, native font discovery, or native runtime
  dependency,
- browser examples rendering Markdown to HTML strings and PDF `Uint8Array`,
- size budget and performance benchmarks,
- deterministic output tests under wasm.

## 4. Architecture

```text
Source
  |
  v
Lexer / block parser / inline parser
  |
  v
Document AST
  |-----------------------------.
  v                             v
HTML renderer               PDF renderer
  |                          |
CSS/theme                   style cascade subset
  |                          |
self-contained HTML         text shaping -> layout -> PDF objects
```

Native batch mode adds an Asupersync orchestration layer around the synchronous
core:

```text
file discovery -> render jobs -> write outputs -> receipts
       Asupersync Cx / Scope / Budget / Outcome
```

Asupersync is not part of the render core. It is for cancellation, budgets,
parallelism, deterministic tests, and future long-running native workflows.

## 5. Core Subsystems

### 5.1 Markdown Parser

Current parser covers the common document subset. Target:

- full CommonMark block and inline conformance ladder,
- GFM tables, task lists, strikethrough, autolinks,
- reference links,
- raw HTML policy with safe defaults,
- source spans for diagnostics and future editor integrations,
- zero panics on malformed input,
- fuzz and differential tests against curated fixtures.

### 5.2 Theme And Style Model

One style model should drive both HTML and PDF:

- base font family: sans or serif,
- mono font for code,
- colors, spacing, borders, table density,
- page size and margins for PDF,
- code syntax theme,
- custom CSS replacement for HTML,
- compact serializable theme config for CLI and WASM.

### 5.3 HTML Renderer

Output should be a single file:

- inline stylesheet,
- no JavaScript requirement,
- safe escaping by default,
- custom stylesheet option,
- polished tables, blockquotes, code, lists, images,
- optional embedded font CSS once curated fonts are bundled,
- deterministic formatting for snapshot tests.

### 5.4 Syntax Highlighting

Build a focused highlighter rather than pulling `syntect`.

Recommended staged path:

1. Token classes for common languages used in Markdown docs: Rust, Python,
   JavaScript/TypeScript, Bash, JSON, TOML, YAML, SQL, HTML/CSS, Markdown.
2. A small deterministic lexer framework with per-language state machines.
3. Graceful fallback to plain code for unknown languages.
4. Shared token stream consumed by HTML and PDF renderers.
5. Golden snapshots for representative snippets.

This does not need to parse every edge of every language. It needs to make
documentation code blocks look excellent, safely and fast.

### 5.5 Font System

Curated fonts should be bundled or caller-provided as bytes.

Default families:

- sans: a high-quality readable UI/document face,
- serif: a long-form reading face,
- mono: a code face with excellent numerals and punctuation.

Implementation targets:

- parse `cmap`, `head`, `hhea`, `hmtx`, `maxp`, `name`, `OS/2`, `kern`,
  selected GPOS pair adjustments, selected GSUB ligatures,
- Latin-first shaping with future extension points for complex scripts,
- deterministic subset generation,
- no system font discovery in core or WASM.

### 5.6 Text Layout

PDF text layout target:

- boxes, glue, and penalties,
- Knuth-Plass optimal paragraph breaking,
- badness and demerit tuning per block type,
- TeX/Liang hyphenation patterns,
- kerning-aware line widths,
- ligature-aware glyph runs,
- leading and baseline grid,
- optional justification and optical margin protrusion.

### 5.7 Page Layout

PDF pages need:

- page boxes and margins,
- heading keep-with-next,
- widow/orphan control,
- blockquote/list/code/table spacing rules,
- table column measurement and page breaks,
- repeated table headers when a table splits,
- image sizing and fallback boxes,
- deterministic pagination.

### 5.8 PDF Writer

Build a minimal valid PDF writer:

- deterministic object numbering,
- catalog/pages/page/content/font objects,
- compressed content streams,
- embedded subset fonts,
- metadata controlled by options and `SOURCE_DATE_EPOCH`,
- no timestamps unless explicitly requested,
- structural validation tests.

Tiny-file strategy:

- subset fonts,
- reuse resources,
- compress streams,
- avoid redundant path/text operators,
- deterministic object reuse for repeated styles.

## 6. Performance Strategy

Targets:

- sub-100 ms HTML render for ordinary README-size docs in release mode,
- fast cold builds because the core has no heavy dependencies,
- PDF rendering limited mostly by font parsing/layout, not dependency overhead,
- WASM bundle kept small enough for browser/editor embedding.

Measurement:

- parse throughput benchmark,
- HTML render throughput benchmark,
- PDF render throughput benchmark once available,
- output byte-size ledger,
- compile-time/dependency-size ledger,
- browser/WASM size and latency checks.

## 7. Reliability And Security

- Safe escaping by default.
- No network access in rendering.
- No ambient filesystem access in core.
- No panics for untrusted Markdown.
- Fuzz parser and inline parser.
- Snapshot HTML outputs.
- Validate generated PDFs with independent parsers/tools in CI when available.
- Determinism tests across repeated runs.

## 8. Asupersync Plan

Use Asupersync for native orchestration once the core render APIs are stable:

- `fmd batch` for many files,
- `fmd watch` only if explicitly wanted later,
- child budgets per render job,
- cancellation checkpoints between phases,
- `Outcome` preserved until CLI boundary,
- structured receipts with success/error/cancelled status,
- deterministic lab tests for cancellation and cleanup.

Do not use Asupersync in the browser/WASM core. If a browser runtime is ever
useful, use the explicit Asupersync browser profile and keep it outside the
basic render API.

## 9. Phases

### Phase 0 - Governance And Contract

- AGENTS, README, CHANGELOG, plan, beads.
- License rider.
- CLI capabilities/doctor/robot-docs.
- Core build and test gates.
- WASM target proof for `--no-default-features` through
  `scripts/check-wasm-core.sh` and CI.

### Phase 1 - Parser Conformance

- CommonMark fixture harness.
- GFM table/task/strike fixtures.
- Source spans.
- Fuzz harness.

### Phase 2 - HTML Excellence

- Refine default theme.
- Syntax highlighting.
- Embedded font CSS path.
- HTML snapshot tests.
- Browser visual regression harness.

### Phase 3 - Font And Text Engine

- Font reader.
- Metrics, kerning, ligatures.
- Subsetting.
- Golden font fixtures.

### Phase 4 - Layout Engine

- Knuth-Plass implementation.
- Hyphenation.
- Block and page layout.
- Tables/code pagination.

### Phase 5 - PDF Writer

- Minimal deterministic PDF writer.
- Fonts and content streams.
- Validation and size benchmarks.

### Phase 6 - WASM Package

- wasm-bindgen or equivalent exports.
- Browser examples.
- Size and latency budgets.
- WASM CI checks.

### Phase 7 - Asupersync Native Batch

- Feature-gated native orchestration.
- Batch render command.
- Budgets, cancellation, receipts.
- Deterministic LabRuntime tests.

### Phase 8 - Release Hardening

- Cross-platform CI.
- Golden artifacts.
- Performance ledger.
- Installers/binaries.

## 10. Best Ideas To Make This Exceptional

1. **Visual quality gate:** maintain a curated corpus of Markdown documents and
   compare HTML screenshots/PDF rasterizations across changes.
2. **Output-size leaderboard:** every PDF fixture tracks bytes, font bytes,
   content-stream bytes, and object count.
3. **Typography ledger:** every line-breaking or shaping improvement records
   before/after raggedness, hyphen count, rivers, and page count.
4. **WASM demo page:** browser textarea -> live HTML preview -> PDF download,
   all using the same core.
5. **Agent-readable receipts:** every batch render can emit deterministic JSONL
   receipts with input hash, output hash, bytes, warnings, and timings.
6. **Negative evidence file:** record rejected dependencies and optimizations so
   future agents do not relitigate them.
7. **Tiny fixture PDFs:** keep hand-inspectable PDFs for each primitive:
   text-only, table, code block, blockquote, image, multipage.
8. **No silent fallbacks:** if a feature is not implemented, return typed errors
   with stable codes and exact next steps.

## 11. CI And Boundary Gates

The repository should fail fast when a change weakens the clean-room or WASM
boundary:

- `cargo fmt --check`
- `cargo check --all-targets`
- `cargo check --no-default-features --lib`
- `scripts/check-wasm-core.sh`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

The WASM gate intentionally checks the library only. CLI features may depend on
native process, stdin/stdout, and filesystem behavior, but the render core must
keep compiling for `wasm32-unknown-unknown` without default features.

## 11. Current State

Working:

- parser subset,
- HTML output,
- CLI render,
- tests for core HTML behavior,
- typed PDF refusal.

Not working yet:

- real PDF,
- syntax highlighting,
- embedded fonts,
- WASM package,
- Asupersync batch mode,
- full CommonMark conformance.
