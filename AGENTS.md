# AGENTS.md - franken_markdown

Guidelines for AI coding agents working in this repository.

## Rule 0

If Jeffrey gives a direct instruction, follow it. Project-local guidance exists
to encode standing preferences, not to overrule the user.

## No Deletion Or Destructive Git

Do not delete files without explicit written permission. Do not run
`git reset --hard`, `git clean -fd`, `rm -rf`, or equivalent destructive
commands unless the user explicitly provides the exact command and confirms the
irreversible consequences.

Work happens on `main`. Do not create feature branches unless the user
explicitly asks for one.

## Project Mission

`franken_markdown` is a clean-room, dependency-lean Rust library plus standalone
CLI (`fmd`) for rendering Markdown into:

- beautiful, self-contained HTML that looks like a polished Cursor/GitHub-style
  Markdown preview,
- tiny, optimized PDF with high-quality typography, embedded subset fonts, and
  LaTeX-grade paragraph breaking,
- first-class browser/WASM output using the same pure rendering core.

The product goal is not to become a browser engine, Typst clone, or generic
document platform. The goal is to do one workflow exceptionally well:

```text
Markdown file / stdin / raw text -> HTML and/or PDF
```

## Architecture Doctrine

The renderer core must stay focused, auditable, and portable.

- The engine library is our code, not a wrapper around `comrak`,
  `pulldown-cmark`, `syntect`, `cosmic-text`, `krilla`, `typst`, `blitz`, or
  headless browser stacks.
- The core render path should remain free of heavy third-party dependency
  forests. Any dependency needs a written justification, feature gate, cargo-tree
  proof, and security/performance rationale.
- `unsafe` is forbidden unless a future, explicitly approved SIMD/font-parsing
  island is created with scalar fallback and safety documentation.
- HTML and PDF must share one parsed AST and one theme model so visual output
  stays coherent.
- WASM is first-class. The render core must compile without CLI, filesystem,
  process, network, threads, or native runtime assumptions.
- Determinism matters. Given fixed input, theme, fonts, and options, output bytes
  should be stable across OSes and runs.

## Text And PDF Quality Bar

The PDF engine is the differentiator. It must eventually include:

- Knuth-Plass optimal paragraph breaking, not greedy wrapping,
- real font metrics, kerning, ligatures, leading, and baseline grids,
- Liang/TeX hyphenation with deterministic patterns,
- widow/orphan and keep-with-next pagination controls,
- high-quality tables with measured columns, repeatable headers, and sane page
  breaks,
- beautiful blockquotes, code blocks, images, lists, and headings,
- embedded curated fonts with document-specific subsetting,
- compact deterministic PDF objects and compressed content streams.

The HTML path should remain immediately useful and attractive:

- subtle table striping, readable padding, and alignment,
- polished blockquotes and code blocks,
- safe escaping by default,
- custom stylesheet support,
- font-family toggle between attractive sans and serif defaults,
- no JavaScript requirement for normal output.

## WASM Requirements

WASM is not a later afterthought.

- Keep `cargo build --no-default-features` as the core portability gate.
- Add a dedicated WASM API and tests before declaring the engine stable.
- Do not let CLI-only concerns leak into the core library.
- Browser output must not rely on system fonts, fontconfig, filesystem access, or
  native process features.
- Bundle or accept fonts as bytes. Font discovery is a native-shell concern, not
  a browser-core concern.

## Asupersync Usage

Use Asupersync where it creates real leverage:

- native CLI batch rendering,
- cancellation and budgets for large file trees,
- structured parallel render jobs,
- deterministic lab tests for cancellation, cleanup, and orchestration,
- future daemon/server/watch modes if they are added.

Do not put Asupersync in the pure render core just because this is a Franken
project. Core parsing/layout/rendering should remain synchronous and embeddable.
When async orchestration is needed, put `&Cx` first in owned async APIs, use
scoped child regions, preserve `Outcome` semantics until a CLI boundary, and add
deterministic tests for cancellation and cleanup.

## CLI Ergonomics Requirements

`fmd` is an agent-first CLI.

- The first command an agent guesses should work: `fmd README.md`,
  `fmd - < README.md`, and `fmd --text '# Hi' --out hi.html`.
- Stdout is data. Stderr is diagnostics and status.
- Read/discovery commands must offer JSON: `capabilities --json`,
  `doctor --json`, and render status via `--json`.
- Provide in-tool docs through `robot-docs guide`.
- Exit codes are stable and documented.
- Errors should say exactly what failed and what command or flag fixes it.
- Bare `fmd` prints help and exits; it must never open a blocking TUI.
- Honor `NO_COLOR`, `CI`, `TERM=dumb`, and `--no-color` as the surface grows.

## Testing And Verification

After substantive Rust changes, run:

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

Before claiming PDF quality, add golden visual/output fixtures, PDF structural
validation, file-size benchmarks, determinism checks, and browser/WASM tests.

## Helper Script Artifact Safety

Helper scripts that write under `tests/artifacts` must treat caller-provided
run ids as path components, not trusted paths.

- Use the shared policy in `scripts/validate-run-id.sh` for artifact run ids:
  `^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$`.
- Source that helper and call `fmd_validate_run_id` before constructing,
  cleaning, or recursively overwriting any artifact directory derived from a
  run id.
- Keep artifact roots under the repository, normally
  `$ROOT/tests/artifacts/<suite>/$RUN_ID`. Do not accept absolute paths,
  traversal, blank ids, dot-only ids, spaces, or shell metacharacters as run
  ids.
- Prefer `mktemp` for scratch work that does not need to become a durable
  artifact. When cleanup is unavoidable, clean only validated, repo-rooted
  paths and use option terminators for commands that accept them.
- Add source-shape tests when the critical property is ordering, such as
  validation before cleanup. Add behavior tests when parser/security semantics
  matter, such as checksum matching or no-network installer behavior.
- Do not add broad manual cleanup instructions or cleanup helpers that operate
  on caller-supplied paths directly.

## Beads

Use `br` for roadmap and implementation tracking. After editing beads:

```bash
br dep cycles
br sync --flush-only
git add .beads/
```

Do not run bare `bv`; use only `bv --robot-*` modes.

## Agent Mail

In multi-agent sessions, register with Agent Mail, reserve files before editing,
and coordinate through threads. Treat unrecognized working-tree changes as peer
work. Do not revert or overwrite them.
