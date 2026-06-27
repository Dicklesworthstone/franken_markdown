# Changelog

This changelog is researched from git history, repository files, and local
tracker state. It distinguishes actual releases from ordinary commits; this
project has no tags or GitHub Releases yet.

## Timeline

| Date | Version / phase | Evidence | Summary |
|---|---|---|---|
| 2026-06-26 | Pre-Phase-0 scaffold | `8b66477` | Working clean-room Markdown-to-HTML scaffold with `fmd` CLI and typed PDF refusal |
| 2026-06-26 | Governance and CLI hardening | `e3cd358`, `98c7f0b`, `d694c86` | License rider, project docs, agent-friendly CLI surfaces, and roadmap Beads |

## Unreleased

### Documentation And Governance

- Switched the project license to the MIT License with OpenAI/Anthropic rider,
  matching the `/dp/asupersync` convention.
- Added project-local agent guidance covering clean-room rendering, WASM as a
  first-class target, Asupersync usage boundaries, CLI ergonomics, testing, and
  Beads/Agent Mail coordination.
- Added a README that is explicit about current status, command examples,
  architecture, limitations, and roadmap.

### CLI Ergonomics

- Added first-try render aliases so `fmd README.md`, `fmd -`, and
  `fmd --text '# Hi' --out hi.html` route to `render`.
- Added `capabilities`, `doctor`, and `robot-docs guide` surfaces.
- Added `--robot-triage` as a one-call JSON quick reference and health envelope.
- Added `--json` status/error output for render operations.
- Normalized common `--json` typos before parsing.
- Returned documented exit code 64 for usage errors and added a teaching hint
  that names `fmd --help`, `fmd capabilities --json`, and
  `fmd robot-docs guide`.
- Kept stdout as document data and stderr as diagnostics/status.
- Added binary-level contract tests for help, file/stdin/text render paths,
  discovery JSON, PDF rendering, usage errors, typo inference, and
  `NO_COLOR`/CI/`TERM=dumb` expectations.

### Planning

- Tightened the roadmap around first-class browser/WASM support.
- Added `scripts/check-wasm-core.sh` and a GitHub Actions workflow plan for the
  native no-default-features core check plus `wasm32-unknown-unknown`.
- Added `scripts/check-policy.sh` to enforce the clean-room dependency and
  unsafe-code boundary in CI.
- Added `scripts/check-determinism.sh` to compare repeated agent JSON, HTML,
  and PDF render outputs byte-for-byte in CI.
- Clarified that Asupersync belongs in native batch orchestration,
  cancellation, budgets, and deterministic tests, not in the pure synchronous
  render core.

### Rendering Engine

- Replaced the flat theme fields with a structured dependency-free theme model
  covering font family, mono font, colors, spacing, table density, code theme,
  dark-mode policy, and PDF/page contract; exposed stable hand-rolled JSON for
  CLI/config/WASM callers.
- Added a clean-room syntax highlighter for common documentation languages,
  wired into the HTML emitter with token CSS and regression tests.
- Added document-centric library entrypoints:
  `parse_markdown`, `render_html_document`, and `render_pdf_document`, so callers
  can parse once and render multiple outputs from one AST.
- Landed the first deterministic PDF writer: valid PDF 1.7 output, Base-14
  fonts, automatic pagination, headings, paragraphs, code blocks, lists,
  blockquotes, thematic rules, simple table text, xref/startxref output, and
  byte-level structural tests.
- Updated `fmd doctor`, `capabilities --json`, `robot-docs guide`, and
  `--robot-triage` to report the PDF path as `available_v0_base14` while keeping
  Knuth-Plass layout and font subsetting marked as planned.

### Parser Conformance

- Added setext heading support for `===`, `---`, and single-dash paragraph
  underlines while preserving standalone thematic breaks.
- Added focused parser conformance tests for level-one setext headings,
  level-two setext headings, single-dash setext headings, thematic breaks, and
  indented non-underlines.
- Added a reference-definition collection pass with normalized labels and
  first-definition-wins behavior.
- Added full, collapsed, and shortcut reference links plus matching reference
  images, with malformed-definition regressions that keep bad definitions
  visible as normal text.
- Added lazy list-item continuation and nested ordered/unordered list parsing,
  including task-list continuation and blockquote/list nesting regressions.
- Added `scripts/parser-diff.sh`, dependency-free parser metamorphic tests, and
  approved article-body fixture snapshots under `tests/fixtures/parser/`.
- Added `parse_markdown_spanned`, source-span wrapper types, and recoverable
  parser diagnostics for malformed reference definitions and unclosed fences.

## 2026-06-26 - Pre-Phase-0 Scaffold

### Delivered

- Created the Rust 2024 crate, nightly toolchain pin, and release profiles.
- Added a clean-room renderer core with no third-party dependencies.
- Added the `fmd` and `franken_markdown` binaries over one shared CLI entrypoint.
- Implemented a useful Markdown parser subset:
  headings, paragraphs, fenced code, blockquotes, lists, task lists, pipe
  tables, thematic breaks, emphasis, strong, strikethrough, code spans, links,
  images, autolinks, and breaks.
- Implemented all-in-one HTML output with inlined default CSS, custom stylesheet
  replacement, sans/serif font stacks, table styling, blockquotes, code blocks,
  task lists, and dark-mode CSS.
- Added typed `not_yet_implemented` PDF behavior so callers initially got
  deterministic failure instead of a panic or silent empty file. This has since
  been superseded by the v0 deterministic PDF writer in Unreleased.
- Added smoke tests and `examples/showcase.md`.

### Notes For Agents

- `CHANGELOG_RESEARCH.md` contains the evidence summary.
- There are no release tags yet.
- `.beads/issues.jsonl` contains the initial roadmap issue graph.
