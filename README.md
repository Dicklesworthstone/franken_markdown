<div align="center">

# franken_markdown

**A clean-room, dependency-lean Rust Markdown renderer for beautiful all-in-one
HTML, tiny high-quality PDF, a standalone `fmd` CLI, and first-class WASM use.**

![License](https://img.shields.io/badge/license-MIT%20%2B%20OpenAI%2FAnthropic%20rider-blue)
![Language](https://img.shields.io/badge/language-Rust%202024-dea584)
![Status](https://img.shields.io/badge/status-pre--Phase--0%20scaffold-orange)
![Core](https://img.shields.io/badge/core-clean--room%20std--only-success)
![WASM](https://img.shields.io/badge/WASM-first--class-654ff0)

</div>

> **Status: pre-Phase-0 scaffold.** The Markdown-to-HTML path works today. The
> PDF engine, syntax highlighter, font subsetter, and WASM package are planned
> work tracked in beads. The repository is intentionally early and not yet a
> released package.

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
| Tiny dependency surface | Clean-room core; no `comrak`, `syntect`, `cosmic-text`, `krilla`, Typst, Blitz, or browser engine |
| PDF quality | Planned Knuth-Plass line breaking, kerning, ligatures, leading, hyphenation, pagination, and font subsetting |
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

# Use the serif theme
target/release/fmd examples/showcase.md --font serif --out showcase-serif.html

# Discover the agent-readable contract
target/release/fmd capabilities --json
target/release/fmd doctor --json
target/release/fmd robot-docs guide
target/release/fmd --robot-triage
```

PDF is wired but intentionally refuses until the clean-room PDF stack lands:

```bash
target/release/fmd examples/showcase.md --to pdf --out showcase.pdf
# exits 70 today: not yet implemented: pdf pipeline ...
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
  pipe tables with alignment, thematic breaks,
- inline emphasis, strong, strikethrough, code spans, links, images, autolinks,
  hard and soft breaks,
- safe HTML escaping by default,
- all-in-one HTML with inlined CSS,
- default sans and serif font stacks,
- custom stylesheet replacement,
- `fmd` and `franken_markdown` binaries over one shared CLI entrypoint,
- typed PDF refusal so callers can handle the incomplete path deterministically.

Planned:

- full CommonMark/GFM conformance ladder,
- clean-room syntax highlighting for code blocks,
- embedded curated font families and per-document font subsetting,
- Knuth-Plass paragraph layout and TeX/Liang hyphenation,
- deterministic compact PDF writer,
- browser/WASM package and examples,
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
| `--to pdf` | Write PDF; planned, currently exits 70 |
| `--to both` | Write both outputs, deriving extensions from `--out` |
| `--out <path>` | Output path; HTML without `--out` writes to stdout |
| `--font sans` | Default high-readability sans stack |
| `--font serif` | Long-form serif stack |
| `--css <file>` | Replace the default stylesheet with custom CSS |
| `--title <text>` | Override the document title |
| `--allow-html` | Pass raw HTML through instead of escaping it |
| `--json` | Emit stable status/error JSON to stderr for render commands |

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
  wasm API over the same parser/theme/render core
```

The core modules are:

| Module | Purpose |
|---|---|
| `ast` | Renderer-neutral document model |
| `parse` | Clean-room Markdown block and inline parser |
| `theme` | Shared typography/color model |
| `html` | All-in-one HTML emitter |
| `text` | Planned font reader/shaper/subsetter |
| `layout` | Planned Knuth-Plass and pagination engine |
| `pdf` | Planned compact PDF writer |
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
scripts/check-policy.sh
```

WASM/core portability gate:

```bash
rustup target add wasm32-unknown-unknown
scripts/check-wasm-core.sh
```

That script checks the library with `--no-default-features` for both native Rust
and `wasm32-unknown-unknown`. It must stay green before the project claims any
browser/WASM readiness.

Clean-room policy gate:

```bash
scripts/check-policy.sh
```

That script verifies the no-default core still has zero third-party normal
dependencies, banned renderer/browser/runtime dependency forests are absent, no
native build script has appeared, and unsafe-code lint enforcement is still in
place.

Future release channels are expected to include standalone binaries and a
browser/WASM package.

## Configuration

There is no config file yet. All current rendering choices are explicit flags:

```bash
fmd document.md --font serif --css custom.css --title "Quarterly Memo" --out memo.html
```

Planned native config will cover default font family, output policy, theme
selection, PDF page size/margins, and batch-render behavior. Browser/WASM callers
will pass equivalent options through the library API rather than reading a local
config file.

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
| `PDF output requires --out <path>` | PDF writes binary bytes and must have a path: `fmd doc.md --to pdf --out doc.pdf` |
| `not yet implemented: pdf pipeline` | Expected today; use HTML while PDF beads land |
| HTML printed to terminal | Pass `--out file.html` or redirect stdout |
| Custom CSS removed the default styling | `--css` intentionally replaces the stylesheet; include every rule you want |
| Raw HTML appears escaped | Default is safe escaping; pass `--allow-html` only for trusted input |

## Limitations

- PDF output is not implemented yet.
- Parser coverage is a useful subset, not full CommonMark/GFM conformance yet.
- Syntax highlighting is planned but not implemented.
- Fonts are currently CSS stacks in HTML; embedded font bytes and PDF subsetting
  are planned.
- WASM packaging and browser examples are not present yet.
- There is no installer or published release yet.

## FAQ

**Why not use existing crates?**  
The goal is an extremely focused renderer with a small dependency and security
surface, fast builds, full control over output quality, and first-class WASM.

**Will this support custom styles?**  
Yes. `--css <file>` already replaces the default HTML stylesheet. The PDF style
model will expose controlled theme/page options rather than arbitrary browser CSS.

**Will PDFs really look better than browser print output?**  
That is the intent. The planned PDF path uses TeX-style paragraph breaking,
kerning, ligatures, hyphenation, leading, pagination controls, and font
subsetting rather than a browser print pipeline.

**Does the core work in WASM?**  
That is a first-class design goal. The core must build without the CLI feature;
dedicated WASM exports and tests are planned before stability claims.

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
