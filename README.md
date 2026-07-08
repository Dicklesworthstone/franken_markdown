<div align="center">

# franken_markdown

<img src="franken_markdown_illustration.webp" alt="franken_markdown clean-room Rust Markdown renderer for HTML, PDF, fmd CLI, and WASM">

**A clean-room Rust renderer and `fmd` CLI for turning Markdown into polished
self-contained HTML, compact tagged PDF, and browser/WASM output from one
auditable core.**

![License](https://img.shields.io/badge/license-MIT%20%2B%20OpenAI%2FAnthropic%20rider-blue)
![Language](https://img.shields.io/badge/language-Rust%202024-dea584)
![Status](https://img.shields.io/badge/status-HTML%20%26%20PDF%20already%20work-success)
![Core](https://img.shields.io/badge/core-clean--room%20std--only-success)
![WASM](https://img.shields.io/badge/WASM-first--class-654ff0)

```bash
curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.sh | bash
# or build the tagged source: cargo install --git https://github.com/Dicklesworthstone/franken_markdown --tag v0.3.1 franken_markdown
```

</div>

> **Current status.** The `v0.3.1` GitHub release ships checksum-verified `fmd`
> archives for Linux x86_64, macOS Intel, macOS Apple Silicon, and Windows
> x86_64, built and smoke-tested with DSR. The browser/WASM package is published
> to npm as `@franken-suite/franken-markdown`. Crates.io still serves
> `franken_markdown = "0.2.0"` as checked on July 8, 2026, so use the release
> archives or tagged source for the current `0.3.1` CLI and library until the
> Rust crate catches up. The current renderer ships shared HTML/PDF syntax
> highlighting including Mermaid/MMD source fences, measured PDF table
> allocation, fitted ASCII diagrams, frankenmermaid-generated SVG diagrams drawn
> as PDF vectors, staged native writes, optional Asupersync batch rendering,
> browser/WASM package sources, and a long set of measured scalar optimizations.
> SIMD and deeper pagination remain roadmap items until they have proof.

## Contents

- [TL;DR](#tldr)
- [Quick Example](#quick-example)
- [Design Philosophy](#design-philosophy)
- [Performance And CPU Strategy](#performance-and-cpu-strategy)
- [Comparison](#comparison)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Command Reference](#command-reference)
- [Library Use](#library-use)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [Troubleshooting](#troubleshooting)
- [Limitations](#limitations)
- [FAQ](#faq)

---

## TL;DR

**The problem.** Markdown preview is easy inside an editor. Producing a portable
HTML document and a polished PDF from the same Markdown source usually means
adding a browser, LaTeX, a Python or Node stack, native C libraries, or a large
Rust dependency graph. That brings install weight, audit surface, and rendering
drift.

**The solution.** `franken_markdown` owns the full Markdown-to-output pipeline in
pure Rust: parser, AST, theme model, HTML emitter, syntax highlighter, table
layout, typography, font subsetting, SVG drawing, compression, and PDF
serialization. The engine library has **zero third-party dependencies**. The
default build adds only `clap`, and only for the CLI. The same render core works
as a Rust library, the `fmd` binary, and browser/WASM package sources.

### First Minute

| Need | Use |
|---|---|
| Install the released CLI | `curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.sh \| bash` |
| Render a self-contained HTML preview | `fmd README.md --out README.html` |
| Render a compact deterministic PDF | `SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf` |
| Render HTML and PDF together | `fmd README.md --to both --out README.html` |
| Render raw Markdown or stdin | `fmd --text '# Hi' --out hi.html` or `fmd - --out README.html < README.md` |
| Ask for the machine-readable contract | `fmd capabilities --json`, `fmd doctor --json`, `fmd robot-docs guide` |
| Render a directory with bounded native workers | `cargo build --release --bin fmd --features batch`, then `fmd batch docs examples --to both --json` |
| Use it in the browser | `npm install @franken-suite/franken-markdown`, then pass Markdown, font bytes, image bytes, and options from the host |

### What Works Today

HTML, PDF, CLI, optional batch mode, and WASM package sources all use one parsed
AST and one typed theme model. Normal renders do not invoke a browser print
pipeline, a second PDF-only parser, Mermaid.js, or a JavaScript runtime.

| Area | Current functionality |
|---|---|
| Parser and AST | Clean-room block and inline parser with GFM tables, task lists, fenced code, links, images, source spans, recoverable diagnostics, safe raw-HTML escaping by default, and a ratcheted CommonMark 0.31.2 conformance floor |
| HTML output | Self-contained preview document with inlined CSS, deterministic embedded TTF font subsets, dark-mode support, responsive tables, polished blockquotes/code blocks, safe escaping, shared syntax highlighting, and optional stylesheet replacement |
| PDF typography | Curated embedded font subsets, real metrics, focused GPOS kerning, GSUB ligatures, Knuth-Plass line breaking, Liang/TeX hyphenation, body justification, selectable text, outlines, metadata, links, compressed streams, and hierarchical tagged-PDF structure |
| PDF tables | Per-column min-content and max-content measurement feeds a constrained wrapping-badness allocator, so dense headers get useful width instead of equal-column squeeze |
| Code blocks | HTML and PDF share the clean-room highlighter for Rust, Python, JS/TS, JSON, shell, PowerShell, Go, C/C++, TOML/INI, YAML, SQL, HTML/XML/SVG, CSS, Markdown, and Mermaid/MMD. PDF code blocks can include muted line numbers, and unknown languages fall back to escaped plain text |
| ASCII diagrams | Diagram-shaped fences retain row geometry in PDF and scale long rows down when needed, so flow diagrams do not collapse into wrapped prose |
| Mermaid diagrams | `examples/showcase.md` includes highlighted Mermaid source plus a checked-in SVG generated from `examples/showcase-mermaid.mmd` by frankenmermaid. HTML and PDF can include the same diagram without Mermaid.js during render |
| PNG and SVG assets | File-input PDF renders auto-load relative local PNG/SVG destinations. Hosts can also provide explicit image bytes through `--pdf-image` or the library API |
| Vector SVG PDF drawing | Supported SVGs become native PDF drawing operators: paths, shapes, text with baseline-shift handling, transforms, gradients, spread modes, patterns, masks, clips, marker view boxes/orientation/units, marker-child `paint-order`, object-bounding-box clip/mask units, opacity, drop shadows, CSS variables/selectors, `use`/symbol reuse, embedded PNG data URIs, and current frankenmermaid output |
| Library API | `parse_markdown`, `parse_markdown_spanned`, `render_html_document`, and `render_pdf_document` share one AST. Hosts supply fonts and image assets as bytes; the core never reads files or fetches URLs |
| CLI contract | `fmd README.md` works as the first guessed command. `capabilities --json`, `doctor --json`, `robot-docs guide`, `--robot-triage`, stable exit codes, input/image byte limits, JSON render status, and structured render warnings are built for humans and agents |
| Native safety | HTML, PDF, config, and batch outputs are staged where applicable. `--to both` rolls back sibling outputs on later failure, and the CLI refuses to overwrite the input file |
| Config | Dependency-free `key=value` config supports persistent font, dark-mode, custom CSS, page size, and margin defaults. `--no-config` gives reproducible config-free runs |
| Batch | The optional native `batch` feature uses Asupersync for bounded workers, cancellation, timeout handling, deterministic receipts, and stable output ordering |
| Browser/WASM | The wasm-bindgen package sources expose typed HTML/PDF rendering, host-supplied fonts/assets, a plain ESM browser demo, native-parity tests, and a no-default core that stays dependency-free |
| Releases | Checksum-verified GitHub release archives for Linux, macOS Intel, macOS Apple Silicon, and Windows, each built and smoke-tested with DSR; npm package `@franken-suite/franken-markdown`; crates.io currently lists `0.2.0` |

### Mainline Highlights

| Area | Result on `main` |
|---|---|
| Document fidelity | Tables, code blocks, blockquotes, lists, links, images, SVG diagrams, headings, and metadata all render through the same AST and theme model for HTML and PDF |
| Table quality | The allocator measures content ranges and spends column width where it reduces wrapping. Performance-plan style tables no longer force narrow, ugly header wraps |
| Syntax highlighting | Rust, Python, JavaScript/TypeScript, JSON/JSONC, Bash/shell, PowerShell, Go, C/C++, TOML/INI, YAML, SQL, HTML/XML/SVG, CSS/SCSS/Sass, Markdown, and Mermaid/MMD use the same clean-room highlighting model in HTML and PDF |
| Diagram support | ASCII diagrams and frankenmermaid-generated SVGs are now first-class documentation assets in the PDF path instead of screenshots or browser-only fallbacks |
| Vector SVG coverage | The PDF path draws current frankenmermaid showcase output as vector content, including markers, CSS variables, masks, clips, gradients, drop shadows, baseline-shift text, `paint-order`, and embedded PNG data URIs |
| Agent and CI friendliness | JSON capabilities, JSON doctor output, robot docs, stable exit codes, `SOURCE_DATE_EPOCH`, staged writes, and no-default/WASM gates make the tool easy to script and verify |
| Performance work | Parser, HTML, PDF layout/writing, font subsetting, compression, SVG drawing, and batch orchestration hot paths have been profiled and optimized in behavior-preserving passes with golden-output checks |

### Why Use franken_markdown?

| Goal | What you get |
|---|---|
| Good output without setup | Cursor/GitHub-style HTML, compact tagged PDF, readable measure, polished tables, blockquotes, syntax-highlighted code, and diagram support |
| One theme, two surfaces | A typed theme drives HTML and PDF, so colors, spacing, code styling, page size, and document rhythm stay coherent |
| Small audit surface | No `comrak`, `pulldown-cmark`, `syntect`, `cosmic-text`, `krilla`, Typst, Blitz, headless browser, or browser-engine wrapper in the core |
| Deterministic builds and renders | Fixed input plus fixed options yields stable bytes across runs; `SOURCE_DATE_EPOCH` controls PDF dates |
| WASM-first core | The render core has no filesystem, fontconfig, process, thread, network, or async-runtime assumptions; fonts and assets arrive as bytes |
| Measured optimization discipline | Hot paths are scalar, cache-friendly, allocation-conscious, and checked against golden outputs before optimizations land |

---

## Quick Example

```bash
# Install the released CLI
curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.sh | bash

# Render a Markdown file to self-contained HTML
fmd examples/showcase.md --out showcase.html

# Render from stdin, or from a raw string, with no temp file
fmd - --out stdin.html < examples/showcase.md
fmd --text '# Hello from fmd' --out hello.html
fmd --text '# Hello from fmd' --out - > piped.html

# Use the long-form serif theme instead of the default sans
fmd examples/showcase.md --font serif --out showcase-serif.html

# Render a compact, deterministic PDF with embedded subset fonts
fmd examples/showcase.md --to pdf --out showcase.pdf
fmd examples/showcase.md --to pdf --title "Showcase" --author "FMD" --out showcase.pdf

# Render HTML and PDF together (extensions derived from --out)
fmd examples/showcase.md --to both --out showcase.html

# Render a document with a sibling SVG diagram; file-input PDF auto-loads it
fmd examples/showcase.md --to pdf --out showcase.pdf

# Build the native batch renderer and render a directory with deterministic JSON
cargo build --release --bin fmd --features batch
fmd batch docs examples --to both --out-dir rendered --json

# Reproducible PDF metadata dates
SOURCE_DATE_EPOCH=1700000000 fmd examples/showcase.md --to pdf --out showcase.pdf

# Discover the machine-readable contract (for humans and agents alike)
fmd capabilities --json
fmd doctor --json
fmd robot-docs guide
fmd --robot-triage
```

The PDF path is already useful for real technical documents. It produces valid,
deterministic PDFs with embedded curated font subsets, real metrics, focused
kerning, ligatures, selectable text, link annotations, heading outlines,
title/author/`SOURCE_DATE_EPOCH` metadata, hierarchical tagged-PDF structure,
compressed page streams, measured-column tables, syntax-highlighted code panels,
standalone PNG embedding, and vector SVG drawing for supported diagrams. Full
widow/orphan control and finer block pagination remain roadmap work.

---

## Design Philosophy

1. **Focused beats general.** This is a Markdown renderer, not a browser, not a
   full HTML/CSS engine, and not a document programming language. Doing one
   workflow (`Markdown -> HTML and/or PDF`) exceptionally well is the whole goal.
2. **Own the hot path.** The parser, layout, font metrics, line breaking, SVG
   drawing, compression, and PDF writer are built for this exact workflow instead
   of inherited from large general-purpose stacks. That control keeps
   typography, diagrams, determinism, and performance in one auditable codebase.
3. **A small, auditable surface.** The engine compiles with zero third-party
   crates; `clap` is the only default dependency and exists solely for the CLI.
   `scripts/check-policy.sh` enforces this in CI, and `unsafe` is forbidden at
   the crate level.
4. **Determinism is intentional.** Given fixed input, theme,
   fonts, and options, output bytes are stable across runs and operating
   systems. `scripts/check-determinism.sh` compares repeated renders byte for
   byte, and `SOURCE_DATE_EPOCH` pins PDF dates.
5. **WASM is first-class, not a port.** The render core builds without the CLI
   feature and carries no filesystem, fontconfig, process, thread, or
   async-runtime assumptions. Fonts and stylesheets enter as bytes. Native and
   WASM renders are proven byte-identical over a corpus.
6. **Asupersync is for orchestration, not the core.** Structured concurrency,
   cancellation, and budgets live behind the native-only `batch` feature. The
   pure render path stays synchronous and embeddable.
7. **Agent ergonomics are part of the product.** The first command an agent
   guesses works, stdout is data, stderr is diagnostics, errors name the flag
   that fixes them, and a JSON contract is always one flag away.

---

## Performance And CPU Strategy

The production renderer is optimized scalar Rust. That scalar path is the
correctness oracle for native builds, WASM, Apple Silicon, and Intel/AMD x86_64.
Performance work starts with `release-perf` profiles, changes one lever at a
time, and lands only when golden output plus targeted tests keep behavior
stable.

CPU-specific optimization in this project starts with target-family native
archives, layout, memory traffic, and branch behavior rather than intrinsics.
M-series, Intel, and AMD cores all benefit when the renderer keeps hot data
compact, scans bytes linearly, writes PDF buffers append-only, and avoids
allocator churn. Published archives are specialized by target family
(`aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-gnu`, and `x86_64-pc-windows-msvc`) while staying portable
within that family. Local source builds can opt into host-specific codegen when
portability is not required.

### Optimizations Already In The Renderer

| Pipeline stage | Optimization | Practical effect |
|---|---|---|
| Parser line scanning | Byte-level candidate guards skip reference, table, URL, email, and block probes on ordinary prose | Large README-style files stay on contiguous byte walks instead of branch-heavy semantic checks |
| Multiline paragraphs | Plain multiline paragraphs bypass the full inline parser until a line contains Markdown syntax | Prose-heavy documents avoid unnecessary state-machine work and allocation |
| Inline and HTML escaping | Shared scanner primitives find Markdown/HTML escape bytes without per-character allocation on clean text runs | The common path fits branch predictors and cache lines on ARM64 and x86_64 |
| Syntax highlighting | Supported language fences, including Mermaid/MMD, dispatch directly into the clean-room highlighter. Unknown fences stay on the plain escaped path, and PDF avoids duplicate lexer lookup where possible | Code-heavy documents avoid repeated language normalization and duplicate token construction |
| Font use tracking | HTML font subsetting records ASCII glyph use with a compact bitset, then preserves first-seen order for non-ASCII glyphs | Embedded-font CSS stays deterministic while per-render bookkeeping drops |
| HTML heading anchors | Repeated heading-id collisions write exact-capacity decimal suffixes directly into the output candidate instead of allocating `format!` temporaries | Large generated documents with repeated headings keep byte-identical anchors while reducing allocator traffic in the HTML hot path |
| PDF shaping and layout | Render-local shaped-width caches avoid recomputing font shaping, kerning, ligatures, and repeated word widths. Table-cell inline tokens are reused for measurement and final layout | Repeated table, code, and body tokens reuse nearby cache entries instead of re-entering shaping loops |
| PDF glyph collection | Cached shaped runs are collected with a single shape-cache lookup on hits | Cuts deterministic map traffic during subset glyph collection without changing PDF bytes |
| PDF content streams | Text segment operators, `TJ` arrays, decimal tokens, object references, and common structure tokens stream directly into page buffers | Reduces temporary string traffic in text-heavy PDF pages and lowers allocator pressure |
| PDF table allocation | Columns use measured min/max widths and a constrained wrapping-badness solver | Width goes to columns that can reduce wrapping, so dense tables look better and avoid wasted layout work |
| Compression | The hand-rolled zlib/DEFLATE path precomputes fixed-Huffman codes and match symbols, accumulates Adler-32 during emission, and compares equal LZ77 match prefixes in 8-byte chunks before the exact scalar tail | Removes repeated bit-prep work, avoids a second full scan over page/font streams, and reduces byte-by-byte compare work on repeated runs |
| SVG rendering | Common frankenmermaid/SVG constructs become native PDF drawing operators. Default fill+stroke stays compact; custom `paint-order` expands only affected shapes or marker children | Supported diagrams remain vector and local without a browser/rasterizer, while ordinary SVG shapes stay on a compact path |
| Batch rendering | Native batch mode sizes workers by interactive/throughput policy and writes deterministic receipts | Throughput mode can use all cores; interactive mode leaves CPU headroom for the machine running the job |

### Apple Silicon

| Choice | What it means |
|---|---|
| Native ARM64 release archive | The `aarch64-apple-darwin` archive is built and smoke-tested on macOS Apple Silicon, so M-series Macs run `fmd` natively without Rosetta or a translated x86_64 path |
| Cache-friendly scalar loops | Parser gates, highlighter dispatch, font bitsets, shape caches, compression tables, and SVG/PDF appenders keep work in compact arrays and small buffers |
| Append-style PDF writing | Page streams, object records, text operators, and repeated tokens write into reusable byte buffers instead of allocating many short strings |
| Laptop-aware batch mode | `fmd batch --batch-mode interactive` keeps CPU headroom available for foreground work; `throughput` can use all available workers when that is what you want |
| Portable public flags | Release archives do not require `RUSTFLAGS="-C target-cpu=native"`, so one Apple Silicon binary remains portable across ordinary Apple Silicon Macs |
| Future NEON boundary | AArch64 NEON is reserved for the documented byte-scanner island after same-host Apple Silicon proof shows p95 improvement with byte-identical output |

The current Apple Silicon gains come from native ARM64 codegen, fewer
allocations, smaller hot loops, linear byte scanners, fixed-size tables, and
worker policies that avoid taking over the whole laptop. NEON is not claimed as
shipped.

### Intel And AMD

| Choice | What it means |
|---|---|
| Separate x86_64 release archives | The release workflow targets `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, and `x86_64-pc-windows-msvc`, each with a native smoke path instead of reusing the Apple Silicon artifact |
| Portable scalar default | Shipped binaries do not assume AVX2, AVX-512, BMI, or build-host-only instructions, so they work across a broad Intel/AMD fleet |
| Branch-light scans | Parser gates, direct highlighter dispatch, render-local caches, append-style PDF writing, and precomputed compression tables reduce mispredicts and allocator churn |
| Local native builds | Personal builds can use `RUSTFLAGS="-C target-cpu=native"` when the target machine is known and portability does not matter |
| AVX-512 left out by default | AVX-512 availability is uneven and can downclock some CPUs, so it is not part of the default performance story |
| Runtime-gated SIMD only after proof | A future AVX2 path must use `std::is_x86_feature_detected!`, keep scalar fallback, and prove byte-for-byte parity plus same-host speedup |

The Intel/AMD path is tuned for the broad x86_64 fleet first. It avoids assuming
AVX2 or AVX-512 while still benefiting from native x86_64 codegen,
branch-light scans, direct dispatch tables, append-style serializers, and
cache-local compression state.

### Build Profiles And Local CPU Tuning

| Build | Intended use | CPU behavior |
|---|---|---|
| `cargo build --release --bin fmd` | Normal local binary | Portable optimized Rust codegen for the target triple |
| `cargo build --profile release-perf --example fmd_perf_harness` | Performance lab runs and hotspot proof | Keeps symbols and frame pointers useful enough for before/after comparisons |
| `RUSTFLAGS="-C target-cpu=native" cargo build --release --bin fmd` | Local-only benchmarking or personal builds | Lets LLVM select instructions for the current machine. Do not publish this binary as a portable artifact |
| Published release archives | Public installs | Built without hidden `target-cpu=native` assumptions so each archive runs across ordinary machines in that target family |

### Platform Release Matrix

| Target family | What ships | What is intentionally avoided |
|---|---|---|
| Apple Silicon | Native `aarch64-apple-darwin` archives built and smoke-tested on macOS Apple Silicon runners | Rosetta requirement and unproven cross-build-only ARM claims |
| Intel macOS | Separate `x86_64-apple-darwin` archives built and smoke-tested on Intel macOS runners | Treating a universal package as enough proof by itself |
| Intel/AMD Linux | Portable `x86_64-unknown-linux-gnu` archives with scalar hot paths | AVX2, AVX-512, BMI, or build-host-only instruction requirements |
| Windows x86_64 | `x86_64-pc-windows-msvc` archives with an inline PowerShell smoke test | A separate Windows-only command contract |
| Browser/WASM | Scalar byte-identical behavior by default, with host-supplied fonts and assets | System fonts, native threads, filesystem access, and implicit CPU-feature dependency |

### Future SIMD Gate

SIMD is planned but deliberately not claimed as shipped.
[`docs/SIMD_ISLAND_DESIGN.md`](docs/SIMD_ISLAND_DESIGN.md) defines the only
approved future island for AArch64 NEON, x86_64 AVX2, and optional WASM
`simd128`. Any accelerated path must keep scalar fallback, use runtime CPU
detection on x86_64, preserve byte-for-byte golden outputs, and prove the
speedup on the same host family it targets. The correct decision can still be
"keep scalar" when a benchmark does not move end-to-end p95.

---

## Comparison

Honest tradeoffs against the common ways people turn Markdown into HTML and PDF.

| Tool / approach | Real strength | Why `franken_markdown` exists |
|---|---|---|
| Headless Chrome (print to PDF) | Excellent browser fidelity | Heavy runtime, nondeterministic output, large attack surface; not suited to a tiny embeddable CLI/WASM renderer |
| Pandoc (+ LaTeX for PDF) | Extremely capable format conversion | Large external toolchain (Haskell, and usually a LaTeX install for PDF); not a small embeddable Rust/WASM library |
| Typst | Excellent typesetting | Different source language and a heavier stack; PDF-first rather than Markdown-preview-shaped |
| `comrak` / `pulldown-cmark` | Fast, conformant Markdown to HTML | HTML only, no PDF; you still bolt on a separate PDF stack and font tooling, and you cede control of layout |
| `comrak` + a PDF crate + `syntect` | A quick way to assemble a renderer | Pulls broad dependency forests (regex/onig, font stacks, PDF libs) and hands parser/layout/font/PDF behavior to code you do not control |
| `franken_markdown` | Focused Markdown to HTML/PDF | Clean-room, dependency-lean, deterministic, WASM-first, and tuned for one workflow end to end |

Where the others win: a headless browser will out-render arbitrary CSS, and
Pandoc converts far more formats. `franken_markdown` trades that breadth for a
small surface, fast builds, determinism, and a PDF path it controls top to
bottom.

---

## Installation

> The `curl`/PowerShell installers (`install.sh` / `install.ps1`) prefer
> prebuilt release archives and verify checksums from the combined `SHA256SUMS`
> file or per-archive `.sha256` sidecars when either is published. Use
> `--from-source` / `-FromSource`, or the source commands below, only when you
> intentionally want to compile locally.

### One-line install (Unix: macOS, Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.sh | bash
```

### One-line install (Windows PowerShell)

```powershell
irm https://raw.githubusercontent.com/Dicklesworthstone/franken_markdown/main/install.ps1 | iex
```

The installer accepts flags (pass them after `| bash -s --` for the Unix
one-liner):

| Flag | Purpose |
|---|---|
| `--version <tag>` | Install a specific version instead of the latest |
| `--dest <dir>` | Install the binary to a chosen directory |
| `--system` | Install system-wide instead of into your user bindir |
| `--easy-mode` | Pick safe defaults and minimize prompts |
| `--verify` | Run a post-install self-test; archive checksums/signatures are attempted automatically |
| `--from-source` | Build `fmd` from source rather than downloading a binary |
| `--quiet` | Suppress non-essential output |
| `--no-gum` | Skip the prettified prompts and use plain output |
| `--force` | Overwrite an existing install without asking |

### From source

```bash
git clone https://github.com/Dicklesworthstone/franken_markdown
cd franken_markdown

# Build just the fmd binary
cargo build --release --bin fmd
./target/release/fmd --help

# Or install it onto your PATH
cargo install --path .
fmd --help

# Or install the published crates.io package. As checked on July 8, 2026, this
# currently installs 0.2.0 until 0.3.1 is published to crates.io.
cargo install franken_markdown
```

`fmd` and the long alias `franken_markdown` are the same program built from one
shared entrypoint; type whichever you like.

### Prebuilt binaries and browser package sources

The `v0.3.1` release includes a `fmd` archive per platform: Linux
(`x86_64-unknown-linux-gnu`), macOS Intel (`x86_64-apple-darwin`), macOS Apple
Silicon (`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
Native archives are built and smoke-tested with DSR on the release fleet before
they are attached to the GitHub release. Each archive includes a `.sha256`
sidecar and the release also includes a combined `SHA256SUMS`. Download the
archive for your platform and verify it before unpacking (Linux example):

```bash
sha256sum -c fmd-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf fmd-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
```

The browser/WASM build is assembled separately as
`@franken-suite/franken-markdown` by `.github/workflows/release-wasm.yml` and is
published on npm (`npm install @franken-suite/franken-markdown`). Tag pushes
re-verify and publish new versions; the workflow skips versions that are
already on the registry.

---

## Quick Start

1. **Build the binary.**

   ```bash
   cargo build --release --bin fmd
   ```

2. **Render your first HTML file.** With no `--out`, HTML goes to stdout, so
   redirect it or pass a path.

   ```bash
   target/release/fmd README.md --out README.html
   ```

3. **Render a PDF.** PDF cannot stream to stdout, so it needs a real path (or it
   derives one from the input name).

   ```bash
   target/release/fmd README.md --to pdf --out README.pdf
   ```

4. **Pick a theme or a custom stylesheet.**

   ```bash
   target/release/fmd README.md --font serif --out README.html
   target/release/fmd README.md --css mystyle.css --out README.html
   ```

5. **Persist a default so you do not repeat yourself.**

   ```bash
   target/release/fmd config set font serif --json
   ```

6. **Ask the tool what it can do.** No external docs required.

   ```bash
   target/release/fmd capabilities --json
   target/release/fmd robot-docs guide
   ```

---

## Command Reference

`fmd` exposes a small, stable command set. Global flags can appear on any
command.

### Global flags

| Flag | Meaning |
|---|---|
| `--json` | Emit stable machine-readable JSON for the command's status/metadata |
| `--no-color` | Disable decorative color (accepted for `NO_COLOR`/`CI`/`TERM=dumb` parity; current output is already plain) |
| `--no-config` | Ignore native config files for a reproducible, config-free invocation |
| `--robot-triage` | Print one JSON envelope: quick-reference commands, subsystem health, and recommended next actions |

Bare `fmd` prints help and exits successfully; it never opens a blocking TUI.
Common `--json` typos (`--jsno`, `--jason`, `--json=true`) and color spellings
(`--no-colour`, `--color=never`) are normalized before parsing.

### `render` (the default command)

Render a Markdown file, stdin, or a raw string to HTML and/or PDF. `render` is
implicit: `fmd README.md`, `fmd - < README.md`, and `fmd --text '# Hi' --out
hi.html` all route to it.

```bash
fmd render <input.md|-> [--to html|pdf|both] [--out PATH] [--font sans|serif] ...
fmd <input.md|->            # render is inferred
fmd --text '<markdown>' --out out.html
```

| Flag | Meaning |
|---|---|
| `<input>` (positional) | Input `.md` path, or `-` to read Markdown from stdin |
| `--text <markdown>` | Render a raw Markdown string directly, with no input file |
| `--to html\|pdf\|both` | Output format(s). Default `html` |
| `--out, -o <path>` | Output path. HTML with no `--out` (or `--out -`) writes to stdout. PDF and `--to both` always write files |
| `--font sans\|serif` | Override the body font for this render |
| `--css <file>` | Replace the default stylesheet entirely with your CSS (HTML) |
| `--title <text>` | Set the document title (otherwise the first heading, then "Document") |
| `--author <text>` | Set PDF author metadata |
| `--allow-html` | Pass raw HTML in the source through instead of escaping it (trusted input only) |
| `--pdf-line-numbers` | Render muted line numbers in PDF fenced code blocks |
| `--pdf-image DEST=PATH` | Provide or override one Markdown image destination for PDF rendering; repeat for multiple images. File-input PDF renders also auto-load relative local PNG/SVG image destinations. The render core never reads files or fetches network resources itself |
| `--max-pdf-image-bytes <n>` | Max bytes accepted per explicit or auto-loaded PDF image file before rendering (default `33554432`, 32 MiB) |
| `--max-input-bytes <n>` | Refuse file/stdin/`--text` input above `n` bytes before parsing (default `67108864`, 64 MiB) |
| `--json` | Emit a stable JSON status envelope to stderr after writing outputs |

PDF render warnings are also reported on stderr. They are intentionally
structured and stable enough for agents to notice lossy fallbacks such as
unsupported SVG constructs while keeping stdout reserved for document bytes.

Output-path rules:

- HTML with no `--out`, or `--out -`, streams to stdout.
- PDF and `--to both` require a real file. With no `--out`, the path derives from
  the input stem (`doc.md` -> `doc.pdf`), or `document.*` for stdin/`--text`.
- `--out -` is refused for PDF and `--to both` (they cannot stream).

```bash
fmd README.md --out README.html
fmd README.md --to pdf --out README.pdf
fmd README.md --to pdf --pdf-line-numbers --out README.pdf
fmd README.md --to pdf --pdf-image images/chart.png=./chart.png --out README.pdf
fmd README.md --to both --out README.html        # writes README.html + README.pdf
fmd --max-input-bytes 1048576 README.md --out README.html
SOURCE_DATE_EPOCH=1700000000 fmd README.md --to pdf --out README.pdf
```

### `config`

Read or edit the native CLI config. This never touches the WASM/core renderer.

```bash
fmd config show [--json]            # show the resolved config + equivalent theme
fmd config get <key> [--json]       # print the resolved value of one key
fmd config set <key> <value> [--json]
fmd config path [--json]            # print the config file path
```

```bash
fmd config show --json
fmd config get font --json
fmd config set font serif --json
fmd config path --json
```

`config set` cannot be combined with `--no-config`. See
[Configuration](#configuration) for the supported keys.

### `capabilities`

Print the stable, machine-readable CLI contract: commands, examples, output
formats, exit codes, feature status, the theme model, and the CommonMark
conformance number.

```bash
fmd capabilities --json
```

### `doctor`

Report which subsystems are available or planned, the dependency posture, and
the license.

```bash
fmd doctor
fmd doctor --json
```

### `robot-docs guide`

Print a short, in-tool guide written for coding agents: canonical commands and
the rules for stdout/stderr, `--out -`, size limits, and image assets.

```bash
fmd robot-docs guide
```

### `batch` (opt-in `batch` feature)

Render many Markdown inputs in parallel under a bounded worker budget, backed by
Asupersync structured concurrency. This subcommand only exists in builds with
the native-only `batch` feature; the default build and any `--no-default-features`
or wasm build never include it. Build it with:

```bash
cargo build --release --bin fmd --features batch
```

```bash
fmd batch <inputs...> [--to html|pdf|both] [--out-dir DIR] [--workers N]
                      [--batch-mode interactive|throughput] [--mem-budget BYTES]
                      [--continue-on-error] [--font sans|serif] [--css FILE] [--json]
```

| Flag | Meaning |
|---|---|
| `<inputs...>` | Files and/or directories (directories are recursed for `*.md`/`*.markdown`, sorted deterministically) |
| `--to html\|pdf\|both` | Output(s) to produce for every input. Default `html` |
| `--out-dir <dir>` | Where to write outputs (default: alongside each input) |
| `--workers <n>` | Worker cap (default: derived from CPU count and the batch mode) |
| `--batch-mode interactive\|throughput` | `interactive` reserves CPU headroom; `throughput` uses all cores. Default `interactive` |
| `--mem-budget <bytes>` | Soft concurrency cap: workers ≈ `bytes / 64 MiB-per-job` (a static per-job estimate, not measured resident memory) |
| `--timeout <secs>` | Wall-clock deadline; on expiry the run cancels at the next per-file checkpoint and the receipt is marked `cancelled` |
| `--continue-on-error` | Record per-file failures in the receipt instead of failing the whole run |
| `--font`, `--css` | Shared theme overrides, as in `render` |
| `--json` | Emit the deterministic batch receipt JSON to stdout |

With `--json`, the only thing on stdout is the deterministic batch receipt (a
`fmd-batch-receipt-v1` object listing each input, its status, and per-output byte
counts plus content hashes). Without it, stdout stays empty and a human summary
goes to stderr. See [`docs/BATCH_ORCHESTRATION.md`](docs/BATCH_ORCHESTRATION.md)
and [`docs/BATCH_WORKER_BUDGET.md`](docs/BATCH_WORKER_BUDGET.md).

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `64` | Usage error (bad flags or an unsupported combination) |
| `66` | Input error (missing file, oversized input, bad config read) |
| `70` | Render unavailable or failed |
| `73` | Output file write error |
| `74` | stdout / write error |

---

## Library Use

`franken_markdown` is a library first. Parse once, render many targets from one
AST.

```rust
use franken_markdown::{parse_markdown, render_html_document, render_pdf_document,
                       HtmlOptions, PdfOptions, RenderError};

fn build(src: &str) -> Result<(String, Vec<u8>), RenderError> {
    let doc = parse_markdown(src);                          // parse once
    let html = render_html_document(&doc, &HtmlOptions::default())?;
    let pdf = render_pdf_document(&doc, &PdfOptions::default())?; // reuse the AST
    Ok((html, pdf))
}
```

Convenience wrappers `render_html(src, &opts)` and `render_pdf(src, &opts)` parse
and render in one call. For editor and tooling integrations,
`parse_markdown_spanned` returns a spanned document with recoverable
diagnostics. Hosts can supply their own fonts as bytes through
`FontAssets`/`FontAssetSlot` (`body-regular`, `body-bold`, `body-italic`,
`body-bold-italic`, `mono-regular`); any missing slot falls back to bundled
deterministic fonts. PDF images are supplied as bytes through `PdfImageAsset`, so
the core never reads files or the network.

### Browser / WASM

The same core powers the browser package. The WASM build provides a wasm-bindgen
module, TypeScript types, an interactive demo, and a headless smoke harness, and
renders HTML and PDF with byte-identical parity to native. It is published to npm
as [`@franken-suite/franken-markdown`](https://www.npmjs.com/package/@franken-suite/franken-markdown). See
[`wasm/README.md`](wasm/README.md) and run `scripts/check-wasm-package.sh` to
build and verify it.

---

## Configuration

Native config provides persistent CLI defaults and intentionally stays outside
the WASM/core renderer. The file is dependency-free `key=value` text; lines
starting with `#` are comments.

```bash
fmd config show --json
fmd config get font --json
fmd config set font serif --json
fmd config path --json
```

Resolution order for the config path:

1. `$FMD_CONFIG` (explicit override)
2. `$XDG_CONFIG_HOME/fmd/config`
3. `%APPDATA%\fmd\config` (Windows)
4. `$HOME/.config/fmd/config`
5. `fmd.config` in the current directory (last resort)

### Supported keys

| Key | Values | Default |
|---|---|---|
| `font` | `sans`, `serif` | `sans` |
| `dark_mode` | `auto`, `disabled` (also `on`/`off`/`true`/`false`/`none`) | `auto` |
| `custom_css` | path to a stylesheet, or `none` | unset |
| `page_size` | `letter` (612 x 792 pt) | `letter` |
| `margin_top_pt` | non-negative points | `72` |
| `margin_right_pt` | non-negative points | `72` |
| `margin_bottom_pt` | non-negative points | `72` |
| `margin_left_pt` | non-negative points | `72` |

Example config file:

```ini
# ~/.config/fmd/config
font=serif
dark_mode=auto
margin_top_pt=54
margin_bottom_pt=54
```

Browser/WASM callers pass equivalent options through the library API instead of
reading any of these files. Use `--no-config` for a fully reproducible,
config-free render.

---

## Architecture

```text
                         Markdown source (file | stdin | --text)
                                        |
                                        v
                          scanner  +  clean-room parser
                          (byte/line scan, block + inline)
                                        |
                                        v
                                  Document AST  ---------  shared Theme model
                                  (one parse)              (colors, spacing,
                                        |                   code theme, page)
              .-------------------------+--------------------------.
              v                         v                          v
        HTML emitter            layout + text + PDF            WASM API
     (inlined CSS, dark      (metrics, Knuth-Plass,       (wasm-bindgen over the
      mode, clean-room        Liang hyphenation,           same core; CSS/font/
      highlighting)           GPOS kerning, GSUB           image bytes supplied
              |               ligatures, subsetting,        by the host)
              |               FlateDecode streams)              |
              v                         v                       v
   self-contained HTML        compact tagged PDF        byte-identical HTML + PDF

  Native batch orchestration (opt-in `batch` feature):
    fmd batch -> Asupersync structured concurrency, worker budgets,
                 cancellation, and a deterministic receipt; calls the same
                 synchronous render core per file.
```

Core modules:

| Module | Purpose |
|---|---|
| `ast` | Renderer-neutral document model |
| `scanner` | Low-level byte/line scanning primitives shared by the parser |
| `span` | Source-span wrappers and parser diagnostics for tooling/editor/WASM callers |
| `parse` | Clean-room Markdown block and inline parser (CommonMark/GFM subset) |
| `theme` | Shared typed style model: fonts, colors, spacing, code theme, dark mode, page contract |
| `highlight` | Clean-room syntax highlighter for common documentation languages |
| `html` | All-in-one HTML emitter with inlined CSS and dark-mode support |
| `text` | Clean-room TrueType reader: metrics, cmap, glyf/loca outlines, subsetter, GPOS kerning, GSUB ligatures |
| `layout` | Knuth-Plass line breaking and Liang/TeX hyphenation; richer pagination is roadmap |
| `pdf` | Deterministic PDF writer: embedded subset fonts, tables, lists, code, tagged structure, compressed streams |
| `compress` | Hand-rolled FlateDecode/zlib for font programs and page streams |
| `fonts` | Bundled font registry (IBM Plex Sans + Computer Modern, OFL) |
| `wasm` / `wasm_abi` | Browser API over the core; the `wasm-bindgen` ABI is feature-gated |
| `error` | Hand-rolled typed render errors (no `thiserror`) |
| `cli` / `config` | Feature-gated `fmd` command surface and native config |
| `batch` | Native-only Asupersync batch renderer (feature `batch`) |

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `--out -` writes HTML to stdout only; PDF and --to both require a real output path | PDF cannot stream. Omit `--out` to derive a path (`doc.md` -> `doc.pdf`, stdin/`--text` -> `document.pdf`), or pass one: `fmd doc.md --to pdf --out doc.pdf` |
| `PDF output requires --out <path>` | A PDF render needs a destination file; add `--out doc.pdf` |
| Input is refused as too large (exit 66) | Raise the guard explicitly, for example `fmd --max-input-bytes 134217728 big.md --out big.html` |
| `SOURCE_DATE_EPOCH must be non-negative decimal seconds` | Use plain decimal seconds: `SOURCE_DATE_EPOCH=1700000000 fmd doc.md --to pdf --out doc.pdf` |
| HTML printed to the terminal | That is stdout. Pass `--out file.html` or redirect: `fmd doc.md > doc.html` |
| Raw HTML appears as escaped text | Default is safe escaping. Pass `--allow-html` only for trusted input |
| Custom CSS removed all styling | `--css` replaces the stylesheet entirely; include every rule you want to keep |
| `unknown config key ...` | Run `fmd capabilities --json` or see [Configuration](#configuration) for the supported key list |
| `config set` errors with `--no-config` | They are mutually exclusive; drop `--no-config` to write config |
| `invalid --pdf-image ...` | Use `--pdf-image MARKDOWN_DEST=PATH`, for example `--pdf-image images/chart.png=./chart.png`; ordinary file-input renders auto-load relative local `.png` and `.svg` destinations when the files are present |

---

## Limitations

Honest about what the renderer does not do yet.

- **PDF pagination is still maturing.** Keep-with-next (headings, captions, list
  intros), basic widow handling, and repeatable table headers across page breaks
  work today; full widow/orphan control and finer block pagination remain
  roadmap.
- **PDF vs HTML gaps.** The PDF path does not yet render inline images within
  running prose or arbitrary CSS. (Inline styling and links *inside table cells*
  now render, with bold/italic/mono faces and clickable link annotations.) PDF
  images are standalone PNG or SVG assets supplied by the host; the native CLI
  auto-loads relative local image destinations for file-input renders, and
  `--pdf-image` can provide or override assets explicitly.
- **SVG support is practical, not browser-complete.** The PDF renderer covers
  the shapes, gradients, masks, clips, markers, CSS variables/selectors,
  embedded PNGs, marker view boxes/orientation/units, and `paint-order` behavior
  needed by the current showcase and frankenmermaid output. Unsupported SVG
  features are reported as structured render warnings rather than silently
  pretending to match a browser.
- **CommonMark coverage is partial and measured.** Against the official
  CommonMark 0.31.2 suite (`scripts/commonmark-conformance.sh`), **379/652
  examples match** after normalizing fmd's styled HTML (64.1% of the 591 in-scope
  examples; the 61 raw-HTML examples are intentional non-goals, since fmd escapes
  raw HTML by default). This is a ratcheted floor: CI fails if it drops.
- **HTML font subsets are TTF data URLs, not WOFF2.** Output is deterministic and
  portable; smaller WOFF2 subsets are future work.
- **Browser visual/golden fixtures are still early.** The npm package
  (`@franken-suite/franken-markdown`) is published with proven native parity and
  gated manifest/size budgets, but browser-side visual fixtures remain thin.
- **`batch` is opt-in and native-only.** It is not in the default build; enable
  it with `--features batch`, which pulls in Asupersync.
- **Tagged-PDF accessibility is partial.** H1-H3 headings keep exact heading
  tags, H4-H6 headings collapse to generic `/H`, and lists, tables (with header
  column scope), blockquotes, figures, task-list markers, strikethrough runs,
  and links are tagged. Cell-to-header id linkage, sub-line inline-link tagging,
  full PDF/UA validation, and page-spanning logical elements remain roadmap. See
  [`docs/PDF_ACCESSIBILITY.md`](docs/PDF_ACCESSIBILITY.md).
- **Release binaries are tag-driven.** The installers prefer the GitHub release
  assets and fall back to local compilation only when explicitly requested or
  when no matching asset exists for the current platform.

---

## FAQ

**Why not use existing crates?**
The point is an extremely focused renderer with a small dependency and security
surface, fast builds, full control over output quality, deterministic bytes, and
first-class WASM. A `comrak` + PDF-crate + `syntect` stack would pull broad
dependency forests and hand parser/layout/font/PDF behavior to code the project
does not control.

**Will the PDFs really look better than browser print output?**
That is the intent. The PDF path uses Knuth-Plass paragraph breaking, real
metrics, focused GPOS kerning, GSUB ligatures, leading, measured-column tables,
deterministic hyphenation and justification for body paragraphs, and
syntax-highlighted code rather than a browser print pipeline. It embeds subset
fonts with selectable text, metadata, outlines, link annotations, tagged-PDF
structure, `SOURCE_DATE_EPOCH`-controlled dates, and compressed streams. Deeper
pagination control is still landing.

**Which languages get syntax highlighting?**
The clean-room highlighter covers the languages that show up in technical
writing: Rust, Python, JavaScript/TypeScript (JSX/TSX files are tokenized as
JavaScript; keywords, strings, comments, and numbers are highlighted, while
embedded markup tags are not), JSON/JSONC, Bash and other shells, PowerShell,
Go, C/C++ (including `#` preprocessor directives), TOML/INI, YAML, SQL
(case-insensitive keywords), HTML/XML/SVG, CSS/SCSS/Sass, Markdown, and
Mermaid/MMD diagram source. Unknown languages fall back to plain, escaped code.

**Does `fmd` have a `completions` subcommand?**
No. There is no shell-completion generator today; the command set is `render`,
`config`, `capabilities`, `doctor`, `robot-docs`, and (with the `batch` feature)
`batch`, plus the global flags. `fmd --help` and `fmd capabilities --json`
describe the full surface.

**How do I get byte-for-byte reproducible output?**
Render with `--no-config` so machine-local defaults do not leak in, and set
`SOURCE_DATE_EPOCH` for PDF dates. Determinism is enforced in CI by
`scripts/check-determinism.sh`.

**Can I supply my own fonts or stylesheet?**
Yes. `--css <file>` replaces the HTML stylesheet entirely. Library and WASM
callers can supply TrueType font bytes per slot through `FontAssets`; missing
slots use the bundled fonts.

**Does the core really work in WASM?**
Yes, by design. The core builds with `--no-default-features` for both native and
`wasm32-unknown-unknown` (gated by `scripts/check-wasm-core.sh`), and the repo
ships wasm-bindgen exports, a browser demo, and parity tests. Published-package
hardening and browser visual fixtures come before any stability claim.

**Where does Asupersync fit?**
In native orchestration only: the `batch` subcommand's structured concurrency,
cancellation, budgets, and deterministic receipts. It never enters the pure
render core, the `--no-default-features` build, or any wasm build.

---

## About Contributions

Please don't take this the wrong way, but I do not accept outside contributions
for any of my projects. I simply don't have the mental bandwidth to review
anything, and it's my name on the thing, so I'm responsible for any problems it
causes; thus, the risk-reward is highly asymmetric from my perspective. I'd also
have to worry about other "stakeholders," which seems unwise for tools I mostly
make for myself for free. Feel free to submit issues, and even PRs if you want
to illustrate a proposed fix, but know I won't merge them directly. Instead,
I'll have Claude or Codex review submissions via `gh` and independently decide
whether and how to address them. Bug reports in particular are welcome. Sorry if
this offends, but I want to avoid wasted time and hurt feelings. I understand
this isn't in sync with the prevailing open-source ethos that seeks community
contributions, but it's the only way I can move at this velocity and keep my
sanity.

## License

`franken_markdown` is licensed under the MIT License with OpenAI/Anthropic rider
(`LicenseRef-MIT-OpenAI-Anthropic-Rider`). See [`LICENSE`](./LICENSE).
