<div align="center">

# franken_markdown

**A clean-room, dependency-lean Rust Markdown renderer for beautiful all-in-one
HTML, tiny high-quality PDF, a standalone `fmd` CLI, and first-class WASM use.**

![License](https://img.shields.io/badge/license-MIT%20%2B%20OpenAI%2FAnthropic%20rider-blue)
![Language](https://img.shields.io/badge/language-Rust%202024-dea584)
![Status](https://img.shields.io/badge/status-early%20but%20capable-orange)
![Core](https://img.shields.io/badge/core-clean--room%20std--only-success)
![WASM](https://img.shields.io/badge/WASM-first--class-654ff0)

</div>

> **Status: early, but the core HTML and PDF paths already work.** The
> Markdown-to-HTML path renders today, including clean-room syntax highlighting
> for common documentation languages. The PDF path produces compact,
> deterministic, embedded-subset-font documents with Knuth–Plass line breaking,
> real GPOS kerning and GSUB ligatures, measured-column tables, nested lists,
> tinted blockquotes, strikethrough, H1/H2 heading rules, syntax-highlighted code
> panels, and selectable tagged-PDF text. The browser/WASM package now builds a
> real wasm-bindgen module that loads in node/the browser and renders HTML and
> PDF with byte-identical parity to the native core, and is publish-ready
> (validated manifest plus a tag-gated npm release workflow), all proven by
> `scripts/check-wasm-package.sh`; the actual npm publish (one tag push) and
> deeper pagination controls remain active roadmap work tracked in beads.

## TL;DR

**The problem.** Markdown preview looks good in tools like Cursor, but turning
that same Markdown into portable HTML and polished PDF usually means pulling in a
browser, a giant document engine, Python/Node tooling, C libraries, or hundreds
of transitive Rust crates.

**The solution.** `franken_markdown` is a focused Rust renderer that owns the
entire pipeline: Markdown parser, AST, theme, HTML emitter, typography/layout,
font handling, and PDF writer. The core library is designed to be tiny,
memory-safe, deterministic, and usable from native Rust, the `fmd` CLI, and
browser/WASM.

| Goal | Design choice |
|---|---|
| Beautiful default output | Cursor/GitHub-style theme, high-readable measure, polished tables, blockquotes, code blocks |
| Shared style model | Typed theme v1 for font family, mono family, colors, spacing, table density, code theme, dark mode, and page contract |
| Tiny dependency surface | Clean-room core; no `comrak`, `syntect`, `cosmic-text`, `krilla`, Typst, Blitz, or browser engine |
| PDF quality | Compact deterministic embedded-font PDF with Knuth-Plass line breaking, deterministic discretionary hyphenation and glue justification for body paragraphs, measured/striped tables, nested lists, syntax-highlighted code, tags, links, outlines, metadata, and compressed streams; deeper pagination remains roadmap |
| WASM-first | Core render API stays free of CLI/filesystem/runtime assumptions |
| Agent-friendly CLI | `fmd README.md`, `fmd --text`, `capabilities --json`, `doctor --json`, `robot-docs guide`, `--robot-triage` |
| Cross-platform | Windows, macOS, Linux, and browser/WASM are product targets |

## Quick Example

```bash
# Build from source for now
cargo build --release

# Render a Markdown file to HTML
target/release/fmd examples/showcase.md --out showcase.html

# Render stdin
target/release/fmd - --out stdin.html < examples/showcase.md

# Render raw Markdown text directly
target/release/fmd --text '# Hello from fmd' --out hello.html
target/release/fmd --text '# Hello from fmd' --out - > hello.html

# Use the serif theme
target/release/fmd examples/showcase.md --font serif --out showcase-serif.html

# Render the current compact PDF MVP
target/release/fmd examples/showcase.md --to pdf --out showcase.pdf
target/release/fmd examples/showcase.md --to pdf --title "Showcase" --author "FMD" --out showcase.pdf
SOURCE_DATE_EPOCH=1700000000 target/release/fmd examples/showcase.md --to pdf --out showcase.pdf

# Discover the agent-readable contract
target/release/fmd capabilities --json
target/release/fmd doctor --json
target/release/fmd robot-docs guide
target/release/fmd --robot-triage

# Persist native CLI defaults, or bypass them for reproducibility
target/release/fmd config show --json
target/release/fmd config set font serif --json
target/release/fmd --no-config examples/showcase.md --out showcase.html
```

The PDF path is intentionally honest about its stage: it produces valid,
deterministic PDFs today with embedded curated font subsets, real metrics,
focused GPOS kerning, GSUB ligatures, selectable text, link annotations,
outlines, title/author/SOURCE_DATE_EPOCH metadata, tagged-PDF structure, and
compressed large page streams. The writer now does Knuth-Plass line breaking,
deterministic discretionary hyphenation and glue justification for body
paragraphs, measured-column tables with per-cell alignment and zebra striping,
nested lists, tinted blockquotes, strikethrough, H1/H2 heading rules, and
syntax-highlighted code panels. The beads for deeper pagination controls and
further visual polish remain open.

```bash
target/release/fmd examples/showcase.md --to pdf --out showcase.pdf
# writes a compact deterministic PDF
```

## Design Philosophy

1. **Focused beats general.** This is a Markdown renderer, not a browser, not a
   full HTML/CSS engine, and not a document programming language.
2. **Own the hot path.** Parser, layout, font metrics, line breaking, and PDF
   writing are built for this exact workflow instead of inherited from large
   general-purpose stacks.
3. **Correctness before speed, then speed hard.** Visual fidelity, parser
   conformance, PDF validity, and deterministic output are gates; performance
   optimization happens against those gates.
4. **WASM is a first-class target.** The core must be embeddable in browsers and
   editors without native filesystem, fontconfig, process, or async-runtime
   assumptions.
5. **Agent ergonomics are part of the product.** The CLI is designed so coding
   agents can discover capabilities and recover from mistakes without external
   documentation.

## Current Capabilities

Implemented today:

- clean-room AST and parser for a useful CommonMark/GFM subset,
- headings, paragraphs, fenced code, blockquotes, unordered/ordered/task lists,
  indented code, pipe tables with alignment, thematic breaks,
- lazy list-item continuation and nested ordered/unordered lists,
- inline emphasis, strong, strikethrough, code spans, links, images, autolinks,
  hard and soft breaks,
- reference-style links and images (`[text][id]`, `[text][]`, shortcut
  `[text]`, and matching image forms),
- parser conformance, metamorphic, approved fixture snapshot, and official
  CommonMark 0.31.2 spec-suite harnesses, plus a deterministic PDF render-tree
  golden (`tests/golden/render_tree/`) that catches layout/baseline/color/structure
  regressions byte-determinism alone would miss,
- additive spanned parse API with recoverable diagnostics for editor/WASM
  integrations,
- structured shared theme model for HTML, PDF, CLI JSON, and WASM callers,
- conservative raw HTML block/inline parsing that escapes by default and only
  passes through with `--allow-html`,
- safe HTML escaping by default,
- all-in-one HTML with inlined CSS,
- clean-room syntax highlighting for common documentation languages,
- deterministic browser `@font-face` embedding with document-subset bundled
  fonts plus high-quality sans/serif fallback stacks,
- custom stylesheet replacement,
- compact deterministic PDF with embedded curated font subsets,
- real TrueType metrics, focused GPOS pair kerning, and GSUB standard ligatures
  in PDF output,
- Knuth-Plass optimal line breaking in the PDF paragraph layout,
- deterministic TeX/Liang discretionary hyphenation and glue justification for
  PDF body paragraphs,
- nested list rendering, measured-column pipe tables with per-cell alignment and
  zebra striping, tinted/nested blockquotes, strikethrough, H1/H2 hairline
  heading rules, inline-code chips, and syntax-highlighted fenced-code panels in
  PDF output,
- `ToUnicode` mappings for selectable text, including ligature glyphs,
- PDF link annotations, heading outlines/bookmarks, internal heading
  destinations, and deterministic Info metadata,
- reproducible PDF Info dates controlled by explicit library options or the
  CLI's `SOURCE_DATE_EPOCH`,
- hierarchical tagged-PDF structure tree rooted at a single `/Document`, with
  accurate nesting for headings, paragraphs, nested lists (`/L`/`/LI`/`/LBody`),
  blockquotes, per-cell tables (`/Table`/`/TR`/`/TH`/`/TD` with header column
  scope), code, figures (`/Alt` + layout `/BBox`), and links referenced back from
  the tree via `/OBJR`; all decoration (rules, panels, stripes, quote bars) is
  marked `/Artifact` so nothing pollutes the reading order — see
  [`docs/PDF_ACCESSIBILITY.md`](docs/PDF_ACCESSIBILITY.md),
- browser/WASM package sources, wasm-bindgen adapter tests, and an interactive
  local demo that exercise the same dependency-free render core,
- `fmd` and `franken_markdown` binaries over one shared CLI entrypoint,
- typed render errors so callers can handle future incomplete/invalid render
  paths deterministically.

Planned:

- full CommonMark/GFM conformance ladder,
- full widow/orphan + keep-with-next pagination controls,
- code-block pagination polish and broader visual golden fixtures,
- browser/WASM package hardening, published package assembly, and visual
  regression fixtures,
- Asupersync-backed native batch renderer with cancellation and budgets.

## Command Reference

### Render

```bash
fmd render <input.md|-> [--to html|pdf|both] [--out path] [--font sans|serif]
fmd <input.md|-> [--out path]
fmd --text '<markdown>' --out path.html
```

Useful flags:

| Flag | Meaning |
|---|---|
| `--to html` | Write HTML; default |
| `--to pdf` | Write the compact deterministic embedded-font PDF |
| `--to both` | Write both outputs, deriving extensions from `--out` |
| `--out <path>` | Output path. HTML without `--out` (or `--out -`) writes to stdout. PDF and `--to both` always write files: an explicit `--out`, otherwise a path derived from the input filename (`doc.md` → `doc.pdf`), or `document.*` for stdin/`--text`. `--out -` is refused for PDF and `--to both` |
| `--font sans` | Default high-readability sans stack |
| `--font serif` | Long-form serif stack |
| `--css <file>` | Replace the default stylesheet with custom CSS |
| `--title <text>` | Override the document title |
| `--author <text>` | Set PDF author metadata |
| `--allow-html` | Pass raw HTML through instead of escaping it |
| `--json` | Emit stable status/error JSON to stderr for render commands |
| `--no-config` | Ignore native config for a reproducible config-free render |
| `--max-input-bytes <n>` | Refuse file/stdin/`--text` input above `n` bytes before parsing |

### Config

```bash
fmd config show --json
fmd config get font --json
fmd config set font serif --json
fmd config path --json
```

Native config is intentionally outside the WASM/core renderer. It uses a small
dependency-free `key=value` file at `$FMD_CONFIG`, then XDG/platform defaults,
then `~/.config/fmd/config`. Supported keys are `font`, `dark_mode`,
`custom_css`, `page_size`, and individual page margins such as
`margin_top_pt`.

### Capabilities

```bash
fmd capabilities --json
```

Prints the stable machine-readable CLI contract: commands, examples, output
formats, exit codes, and feature status.

### Robot Docs

```bash
fmd robot-docs guide
```

Prints a short in-tool guide for coding agents.

### Robot Triage

```bash
fmd --robot-triage
```

Prints one JSON envelope with quick reference commands, current subsystem
health, and recommended next actions.

### Doctor

```bash
fmd doctor
fmd doctor --json
```

Reports which subsystems are available or planned.

## Architecture

```text
Markdown source
  |
  v
clean-room parser
  |
  v
Document AST
  |------------------------.
  v                        v
HTML emitter          PDF layout pipeline
  |                   (text/font/layout/PDF writer)
  v                        v
self-contained HTML   optimized PDF

Native CLI/batch orchestration:
  fmd + Asupersync feature layer for cancellation, budgets, and parallel jobs

Browser path:
  wasm API over the same parser/theme/render core, with explicit host-supplied
  CSS/font/image bytes and bundled fallback assets
```

The core modules are:

| Module | Purpose |
|---|---|
| `ast` | Renderer-neutral document model |
| `span` | Source-span wrappers and parser diagnostics for tooling/editor/WASM callers |
| `parse` | Clean-room Markdown block and inline parser |
| `theme` | Shared typed style model: fonts, colors, spacing, code theme, dark mode, and page contract |
| `html` | All-in-one HTML emitter |
| `text` | Clean-room TrueType reader, metrics, subsetter, focused GPOS kerning, and GSUB ligatures |
| `layout` | Knuth-Plass line breaking and TeX/Liang hyphenation; richer pagination remains roadmap |
| `pdf` | Compact deterministic embedded-font PDF with Knuth-Plass breaking, body-paragraph hyphenation/justification, tables, lists, and syntax-highlighted code; deeper pagination remains roadmap |
| `cli` | Feature-gated `fmd` command surface |

## Installation

There is no published release yet.

From source:

```bash
git clone https://github.com/Dicklesworthstone/franken_markdown
cd franken_markdown
cargo build --release
target/release/fmd --help
```

For local development:

```bash
cargo test
cargo run -- examples/showcase.md --out showcase.html
scripts/parser-diff.sh
scripts/check-policy.sh
scripts/check-determinism.sh
```

WASM/core portability gate:

```bash
rustup target add wasm32-unknown-unknown
scripts/check-wasm-core.sh
```

That script checks the library with `--no-default-features` for both native Rust
and `wasm32-unknown-unknown`. It must stay green before the project claims any
browser/WASM readiness.

The browser-facing API never discovers fonts from the host system. Callers can
provide UTF-8 CSS bytes and TrueType font bytes for the documented renderer
slots (`body-regular`, `body-bold`, `body-italic`, `body-bold-italic`,
`mono-regular`); any missing slot falls back to bundled deterministic fonts.

Clean-room policy gate:

```bash
scripts/check-policy.sh
```

That script verifies the no-default core still has zero third-party normal
dependencies, banned renderer/browser/runtime dependency forests are absent, no
native build script has appeared, and unsafe-code lint enforcement is still in
place.

Determinism gate:

```bash
scripts/check-determinism.sh
```

That script compares byte-for-byte output across repeated runs for the
agent-readable JSON surfaces, the current HTML renderer, and the current PDF
writer.

Parser conformance gate:

```bash
scripts/parser-diff.sh
```

That script runs the focused parser regressions, dependency-free metamorphic
pseudo-fuzz tests, and approved article-body fixture snapshots under
`tests/fixtures/parser/`.

Future release channels are expected to include standalone binaries and a
browser/WASM package.

## Configuration

Native config is available for CLI defaults and intentionally remains outside
the WASM/core renderer:

```bash
fmd document.md --font serif --css custom.css --title "Quarterly Memo" --out memo.html
fmd config show --json
fmd config get font --json
fmd config set font serif --json
fmd config path --json
```

The config file is dependency-free `key=value` text. Resolution order is:
`$FMD_CONFIG`, then XDG/platform defaults, then `~/.config/fmd/config`.
Supported keys are `font`, `dark_mode`, `custom_css`, `page_size`, and
individual page margins such as `margin_top_pt`. Browser/WASM callers pass
equivalent options through the library API rather than reading local config.

## Comparison

| Tool/approach | Strength | Why `franken_markdown` exists |
|---|---|---|
| Headless Chrome | Excellent browser fidelity | Heavy runtime, large attack surface, not ideal for tiny deterministic CLI/WASM rendering |
| Typst | Gorgeous typesetting | Different source model, heavier stack, PDF-first rather than Cursor-preview-like Markdown |
| Pandoc | Extremely capable conversion | Large external toolchain; not a small embeddable Rust/WASM renderer |
| `comrak` + PDF crate stack | Fast way to build a renderer | Pulls in broad dependency trees and cedes control of parser/layout/font/PDF behavior |
| `franken_markdown` | Focused Markdown -> HTML/PDF | Clean-room, dependency-lean, controllable, WASM-first, designed for one workflow |

## Troubleshooting

| Symptom | Fix |
|---|---|
| `PDF and --to both require a real output path` (after `--out -`) | PDF can't stream to stdout. Omit `--out` to derive a path from the input (`doc.md` → `doc.pdf`, stdin/`--text` → `document.pdf`), or pass one: `fmd doc.md --to pdf --out doc.pdf` |
| PDF differs from the HTML preview | The PDF now does Knuth-Plass breaking, body-paragraph hyphenation/justification, measured/striped tables, nested lists, syntax highlighting, and host-supplied standalone PNG images; remaining gaps vs HTML include inline styling inside table cells, inline image-in-prose rendering, and deeper pagination polish |
| HTML printed to terminal | Pass `--out file.html` or redirect stdout |
| Custom CSS removed the default styling | `--css` intentionally replaces the stylesheet; include every rule you want |
| Raw HTML appears escaped | Default is safe escaping; pass `--allow-html` only for trusted input |
| Input is refused as too large | Raise the render guard explicitly, e.g. `--max-input-bytes 134217728` |
| `SOURCE_DATE_EPOCH` is rejected | Use non-negative decimal seconds, e.g. `SOURCE_DATE_EPOCH=1700000000 fmd doc.md --to pdf --out doc.pdf` |

## Limitations

- PDF output is a deterministic writer with embedded subset fonts, real metrics,
  focused kerning/ligatures, selectable text, compressed page streams, a
  hierarchical tagged-PDF structure tree (per-cell tables with header column
  scope, nested lists, blockquotes, figures with alt/bbox, links referenced via
  `/OBJR`, and decoration marked as `/Artifact`;
  see [`docs/PDF_ACCESSIBILITY.md`](docs/PDF_ACCESSIBILITY.md)), Knuth-Plass line
  breaking, measured/striped tables, nested lists, syntax-highlighted code, and
  deterministic discretionary hyphenation/justification for body paragraphs. Full
  widow/orphan and keep-with-next pagination controls, broader visual golden
  fixtures, and finer accessibility semantics (H4–H6 levels, `/Headers`
  cell-to-header id linkage, sub-line inline-link tagging, page-spanning logical
  elements) remain roadmap.
- Parser coverage is measured against the official CommonMark 0.31.2 suite by
  `scripts/commonmark-conformance.sh`: **357/652 examples match** after
  normalizing fmd's styled HTML (60.4% of the 591 in-scope examples; the 61
  raw-HTML examples are intentional non-goals since fmd escapes raw HTML by
  default). The number is a ratcheted floor (CI fails if it drops) and a lower
  bound on parser correctness — see `tests/fixtures/commonmark/`.
- HTML font subsets are embedded as TTF data URLs, not WOFF2; output is
  deterministic and portable, but future work can make these subsets smaller.
- The WASM package builds a real wasm-bindgen module that loads and renders with
  proven byte-identical native parity, and is publish-ready (validated manifest +
  tag-gated release workflow), all checked by `scripts/check-wasm-package.sh`; it
  is not actually published to npm yet (one tag push away), and
  browser visual/golden fixture coverage is still early.
- There is no installer or published release yet.

## FAQ

**Why not use existing crates?**  
The goal is an extremely focused renderer with a small dependency and security
surface, fast builds, full control over output quality, and first-class WASM.

**Will this support custom styles?**  
Yes. `--css <file>` already replaces the default HTML stylesheet. The PDF style
model will expose controlled theme/page options rather than arbitrary browser CSS.

**Will PDFs really look better than browser print output?**  
That is the intent. The PDF path already uses Knuth-Plass paragraph breaking,
real metrics, focused kerning, GSUB ligatures, leading, measured-column tables,
deterministic discretionary hyphenation/justification for body paragraphs, and
syntax-highlighted code rather than a browser print pipeline; deeper pagination
controls remain roadmap. It embeds subset fonts with selectable text, metadata,
outlines, link annotations, tagged-PDF structure, SOURCE_DATE_EPOCH-controlled
dates, and compressed page streams.

**Does the core work in WASM?**  
Yes, as a first-class design goal. The core builds without the CLI feature, and
the repository now includes dedicated wasm-bindgen exports, package sources, a
browser demo, and package contract tests; published package hardening and browser
visual fixtures remain before stability claims.

**Where does Asupersync fit?**  
In native orchestration: batch rendering, cancellation, budgets, structured
parallelism, and deterministic tests. It should not be forced into the pure
render core.

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

`franken_markdown` is licensed under the MIT License with OpenAI/Anthropic rider.
See [`LICENSE`](./LICENSE).
