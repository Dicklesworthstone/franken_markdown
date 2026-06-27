# Changelog

This changelog is researched from git history, repository files, and local
tracker state. It distinguishes actual releases from ordinary commits; this
project has no tags or GitHub Releases yet.

## Timeline

| Date | Version / phase | Evidence | Summary |
|---|---|---|---|
| 2026-06-26 | Pre-Phase-0 scaffold | `8b66477` | Working clean-room Markdown-to-HTML scaffold with `fmd` CLI and typed PDF refusal |

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
- Added `--json` status/error output for render operations.
- Kept stdout as document data and stderr as diagnostics/status.

### Planning

- Tightened the roadmap around first-class browser/WASM support.
- Clarified that Asupersync belongs in native batch orchestration,
  cancellation, budgets, and deterministic tests, not in the pure synchronous
  render core.

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
- Added typed `not_yet_implemented` PDF behavior so callers get deterministic
  failure instead of a panic or silent empty file.
- Added smoke tests and `examples/showcase.md`.

### Notes For Agents

- `CHANGELOG_RESEARCH.md` contains the evidence summary.
- There are no release tags yet.
- The next useful history source should be `.beads/issues.jsonl` once the
  roadmap has been converted into beads.
