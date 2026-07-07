# CHANGELOG Research Notes

Scope: full repository history through the current `0.3.1` DSR release preparation.

## Evidence Sources

- `git log --oneline --decorate --max-count=20`
- `git show --stat --oneline HEAD`
- `git show --name-only --format=fuller HEAD`
- repository files after the initial scaffold commit
- `.beads/issues.jsonl` now contains the initial roadmap issue graph

## Version Spine

| Version / phase | Date | Evidence | Notes |
|---|---:|---|---|
| Pre-Phase-0 scaffold | 2026-06-26 | commit `8b66477` | Initial working zero-dependency Markdown-to-HTML renderer and `fmd` CLI scaffold |
| Governance/docs wave | 2026-06-26 | commit `e3cd358` | Added project docs, AGENTS guidance, comprehensive plan, changelog, and MIT rider |
| CLI ergonomics wave | 2026-06-26 | commit `98c7f0b` | Added agent-friendly `fmd` shortcuts, JSON/status surfaces, doctor, capabilities, and robot docs |
| Formatting normalization | 2026-06-26 | commit `5fac07e` | Rustfmt-only cleanup of the scaffold source |
| Roadmap bead graph | 2026-06-26 | commit `d694c86` | Seeded the project roadmap into Beads |
| First binary release | 2026-06-30 | tag `v0.1.0` | Release archives and installer asset lookup landed |
| Crates.io hardening release | 2026-07-03 | tag `v0.2.0` | crates.io publication, package trimming, staged writes, and stricter binary validation |
| Renderer capability release | 2026-07-07 | tag `v0.3.0` | SVG/PDF fidelity, Mermaid/MMD highlighting, local PDF assets, safer writes, batch receipts, and measured optimization work |
| DSR publication patch | 2026-07-07 | tag `v0.3.1` | DSR-built artifact line aligned to the release manifest after canceling the stuck Actions binary workflow; includes the late HTML base64 and PDF empty-segment drawing passes and leaves the rejected PDF decimal-string trial out of shipped source |

## 0.3.0 Research Notes

The `v0.2.0..HEAD` log is dominated by two related tracks:

- Renderer fidelity: local PDF asset loading and safer CLI writes (`91afecc`),
  expanded SVG/table/typography rendering (`5423d18`), Mermaid/MMD fence
  highlighting (`791a3c8`), SVG text decorations (`83d6663`), modern SVG color
  tokens (`d469f67`), symbol/use viewport scaling (`be813af`), and checked-in
  frankenmermaid SVG rendering (`af97a82`).
- Measured speed work: parser, HTML, PDF layout/writer, highlighter, table
  layout, font shaping/subsetting, SVG drawing, and compression all received
  small behavior-preserving passes, with rejected trials recorded when the data
  did not justify the change.

The release gate evidence also now includes real WASM package construction and
native/WASM byte-parity checks. Raw `.wasm` grew with the vector-SVG/PDF surface,
so the committed raw budget moves from 3,200,000 to 3,300,000 bytes while the
gzip budget remains 1,600,000 bytes.

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
