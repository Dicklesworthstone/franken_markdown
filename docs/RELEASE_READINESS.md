# Release-readiness reality check (bead mwm.5)

Date: 2026-06-29 · Branch: `main`

A final docs-vs-code-vs-beads reality check before any production-ready / release
claim. Every line of the project vision is mapped to evidence, and every gap is
either implemented, documented as a limitation, or tracked by a still-open bead.
The point is to prevent the bead-completion illusion: a closed bead only counts
if the capability is real and proven.

## Vision → evidence

| Vision promise | Status | Evidence |
|---|---|---|
| Clean-room Markdown → beautiful self-contained HTML | **Implemented** | `src/html.rs`, theme model, syntax highlighting; zero third-party deps in the core (`check-policy.sh`) |
| Tiny, high-quality PDF (Knuth-Plass, real font metrics, kerning, ligatures, hyphenation, subset fonts, compressed streams) | **Implemented** | `src/pdf.rs`, `src/layout.rs`, `src/font/`; layout + pdf tests; perf artifacts under `tests/artifacts/perf/` |
| Tagged, accessible PDF (PDF/UA structure) | **Implemented** | `docs/PDF_ACCESSIBILITY.md`, `pdf_structure_tree_is_hierarchical_and_accessible`, bidirectional `/Link` `/StructParent` |
| Standalone agent-friendly CLI (`fmd`) | **Implemented** | render aliasing, stdout=data/stderr=diagnostics, `capabilities`/`doctor`/`robot-docs` JSON, stable exit codes; `release-smoke.sh` |
| First-class browser/WASM package + demo | **Implemented; publish gated** | `wasm/` package (`@franken-suite/franken-markdown`), `wasm/demo/`, `check-wasm-package.sh`, tag-gated `release-wasm.yml`; npm publish is a maintainer tag-push (bead 3i5) |
| Asupersync-native batch orchestration (off the render core) | **Implemented** | `src/batch.rs` behind the `batch` feature, `fmd batch`, deterministic receipts, cancellation lab test; isolation proven (`check-wasm-core.sh` + asupersync-only-under-`--features batch` in `check-policy.sh`) |
| Performance proof discipline | **Implemented** | qw1.8 perf track (shared schema, counters, comparer), `tests/artifacts/perf/qw1.7-reprofile/DECISION.md`; layout micro-opts rejected on evidence (qw1.7.2/.4) |
| Microtypography (LaTeX-grade) | **Hooks implemented; default wiring gated** | fixed-point protrusion/expansion hooks + tests, `docs/MICROTYPOGRAPHY.md`; default-render wiring deferred by the re-profile DECISION (bead qw1.7.5) |
| Official CommonMark conformance | **Implemented (ratcheted floor)** | `commonmark-conformance.sh ci` — 371/652 spec examples (371/591 in-scope, 62.8%), pass count at the enforced floor of 371 |
| Determinism across runs/OSes | **Implemented** | `check-determinism.sh` (byte-stable across `SOURCE_DATE_EPOCH`) |
| Cross-platform release + installers | **Implemented; tag push gated** | Win/macOS/Linux + WASM CI; `install.sh`/`install.ps1`; tag-gated `release.yml` (4 targets, checksums, per-platform smoke); README Installation section (bead 08f) |

## Full verification gauntlet (all green, 2026-06-29)

`cargo fmt --check` · `cargo check --all-targets` ·
`cargo clippy --all-targets -- -D warnings` · `cargo test` ·
`check-policy.sh` · `check-claim-discipline.sh` · `check-wasm-core.sh` ·
`check-determinism.sh` · `parser-diff.sh` · `commonmark-conformance.sh ci`
(pass 371/652 ≥ floor 371) · `cargo test --features batch` (7) + batch clippy ·
`release-smoke.sh` · `perf-counters.sh --self-test` ·
`perf-compare.sh --self-test`.

## Claims honesty

- `capabilities --json` reports `version: 0.0.0` and a `contract_version`; it
  advertises only features that `check-claim-discipline.sh` can map to a flag and
  a proof artifact (the gate passes).
- README does not claim "production-ready", "best-in-class", or "fastest"; the
  header states `0.0.0`, that no release is tagged, and that the npm publish is a
  maintainer action. CHANGELOG is explicit pre-release history.

## What remains (maintainer-only, not engineering gaps)

- **Tag a `v*` release** — triggers `release.yml` (binaries + checksums) and
  `release-wasm.yml` (npm publish, needs the `NPM_TOKEN` secret). No agent can or
  should perform the publish.
- Documented, intentionally-deferred quality work: microtypography default-render
  wiring (qw1.7.5), and the PDF accessibility limitations enumerated in
  `docs/PDF_ACCESSIBILITY.md`. None of these block honest pre-release claims.

## Verdict

The engineering scope of the vision is **implemented and proven**; the only
remaining items are maintainer release actions and explicitly-documented,
bead-tracked deferrals. Public claims match the evidence. The project is
**release-ready pending a maintainer tag push**; it does not over-claim
production status.
