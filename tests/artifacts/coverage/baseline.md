# Coverage baseline snapshot (ratcheted post-Phase-B)

Measured **2026-06-30** with `scripts/coverage.sh --update-floor` (cargo-llvm-cov
`--branch`), merging three feature passes: `default(cli)` + `batch` +
`wasm-bindgen`. Machine-readable form: [`baseline.tsv`](./baseline.tsv). The
committed floor enforced in CI is `tests/fixtures/coverage/coverage-floor.txt`.
Regenerate any run with `scripts/coverage.sh <run-id>` (per-run artifacts under
`tests/artifacts/coverage/<run-id>/` are gitignored).

## Totals

| Metric | Coverage | Floor |
|---|---|---|
| lines | 95.6% (11952/12501) | 95 |
| regions | 94.3% (19666/20846) | 94 |
| branches | 85.7% (2370/2766) | 85 |
| functions | 97.4% (1035/1063) | 97 |

This is up from the pre-Phase-B baseline of 88.1% line / 87.8% region / 76.5%
branch / 90.8% function after the grn.2 mock-free per-module gap-fill.

## Per-module (worst line coverage first)

| Module | Lines | Branches | Missed lines |
|---|---|---|---:|
| src/cli.rs | 81.6% (500/613) | 92.3% (120/130) | 113 |
| src/text.rs | 92.3% (967/1048) | 76.6% (239/312) | 81 |
| src/pdf.rs | 94.5% (3788/4007) | 79.1% (563/712) | 219 |
| src/wasm_abi.rs | 94.6% (261/276) | 61.1% (11/18) | 15 |
| src/lib.rs | 96.7% (116/120) | 92.9% (13/14) | 4 |
| src/scanner.rs | 97.1% (167/172) | 86.8% (59/68) | 5 |
| src/theme.rs | 97.2% (174/179) | 75.0% (6/8) | 5 |
| src/parse/mod.rs | 97.7% (1939/1985) | 90.3% (733/812) | 46 |
| src/batch.rs | 97.8% (888/908) | 88.5% (46/52) | 20 |
| src/highlight.rs | 98.1% (617/629) | 91.6% (218/238) | 12 |
| src/html.rs | 98.2% (592/603) | 86.9% (73/84) | 11 |
| src/layout.rs | 98.8% (1031/1043) | 89.9% (151/168) | 12 |
| src/compress.rs | 99.2% (369/372) | 89.3% (75/84) | 3 |
| src/config.rs | 99.2% (258/260) | 96.3% (52/54) | 2 |
| src/wasm.rs | 99.5% (197/198) | 100.0% (8/8) | 1 |
| src/error.rs | 100.0% (21/21) | n/a (0/0) | 0 |
| src/fonts.rs | 100.0% (27/27) | n/a (0/0) | 0 |
| src/span.rs | 100.0% (40/40) | 75.0% (3/4) | 0 |

The residual gaps are documented per module in the closed grn.2.* beads: cli.rs
batch-feature lines (not in the default binary the contract tests drive) and a few
broken-stdout-pipe paths; text.rs composite scale-transforms / extension lookups
no available font exercises; pdf.rs degenerate empty-document / incompressible-
stream fallbacks; wasm_abi.rs JS-boundary error paths that SIGABRT on a non-wasm
host (covered by the real wasm build).
