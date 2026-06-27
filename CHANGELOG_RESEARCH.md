# CHANGELOG Research Notes

Scope: full repository history through the initial governance, CLI, formatting,
and Beads roadmap commits.

## Evidence Sources

- `git log --oneline --decorate --max-count=20`
- `git show --stat --oneline HEAD`
- `git show --name-only --format=fuller HEAD`
- repository files after the initial scaffold commit
- no tags are present
- no GitHub Releases were discovered from this checkout
- `.beads/issues.jsonl` now contains the initial roadmap issue graph

## Version Spine

| Version / phase | Date | Evidence | Notes |
|---|---:|---|---|
| Pre-Phase-0 scaffold | 2026-06-26 | commit `8b66477` | Initial working zero-dependency Markdown-to-HTML renderer and `fmd` CLI scaffold |
| Governance/docs wave | 2026-06-26 | commit `e3cd358` | Added project docs, AGENTS guidance, comprehensive plan, changelog, and MIT rider |
| CLI ergonomics wave | 2026-06-26 | commit `98c7f0b` | Added agent-friendly `fmd` shortcuts, JSON/status surfaces, doctor, capabilities, and robot docs |
| Formatting normalization | 2026-06-26 | commit `5fac07e` | Rustfmt-only cleanup of the scaffold source |
| Roadmap bead graph | 2026-06-26 | commit `d694c86` | Seeded the project roadmap into Beads |

## Initial Capability Wave

Commit `8b66477` created the initial product skeleton:

- Rust 2024 crate with nightly toolchain pin.
- Library plus two binaries: `franken_markdown` and `fmd`.
- Clean-room Markdown AST and parser.
- All-in-one HTML emitter with default theme.
- CLI render command for files and stdin.
- Typed PDF not-yet-implemented path.
- Smoke tests for HTML rendering, escaping, custom CSS, serif theme, and PDF
  refusal.
- Example document at `examples/showcase.md`.

## Follow-up Work In This Session

The first post-scaffold wave landed in four logical commits:

- license changed to MIT with OpenAI/Anthropic rider,
- Cargo license metadata aligned with `/dp/asupersync`,
- `fmd` CLI improved with first-try render shortcuts, raw `--text`, global
  `--json`, `capabilities`, `doctor`, and `robot-docs guide`,
- project-local `AGENTS.md`, `README.md`, `CHANGELOG.md`, and comprehensive plan
  added,
- `.beads/issues.jsonl` seeded with the implementation roadmap,
- roadmap tightened around first-class WASM and Asupersync as native
  orchestration rather than render-core dependency.
