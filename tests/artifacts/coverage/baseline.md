# Coverage baseline snapshot (grn.1.2)

Measured **2026-06-29** with `scripts/coverage.sh` (cargo-llvm-cov `--branch`),
merging three feature passes: `default(cli)` + `batch` + `wasm-bindgen`. This is
the committed reference the phase-B gap-fill tasks target; the machine-readable
form is [`baseline.tsv`](./baseline.tsv). Regenerate any run with
`scripts/coverage.sh <run-id>` (per-run artifacts land under
`tests/artifacts/coverage/<run-id>/` and are gitignored).

## Totals

| Metric | Coverage |
|---|---|
| lines | 88.1% (10726/12171) |
| regions | 87.8% (17887/20366) |
| branches | 76.5% (2098/2744) |
| functions | 90.8% (946/1042) |

Note: this total is lower than an older default-features-only number (≈90.8% line)
because it now *includes* the previously-unmeasured `batch.rs` (feature-gated) and
`wasm_abi.rs` (the wasm-bindgen ABI adapter, 0% under native tests). Making those
modules visible is the whole point of the merged measurement — you cannot ratchet
what you cannot see.

## Per-module (worst line coverage first → phase-B targets)

| Module | Lines | Branches | Missed lines | Phase-B bead |
|---|---|---|---:|---|
| src/wasm_abi.rs | 0.0% (0/276) | n/a (0/0) | 276 | grn.2.10 |
| src/error.rs | 19.1% (4/21) | n/a (0/0) | 17 | grn.2.8 |
| src/config.rs | 62.3% (162/260) | 57.4% (31/54) | 98 | grn.2.1 |
| src/cli.rs | 66.9% (410/613) | 73.1% (95/130) | 203 | grn.2.2 |
| src/lib.rs | 74.2% (89/120) | 28.6% (4/14) | 31 | grn.2.8 |
| src/wasm.rs | 85.9% (170/198) | 75.0% (6/8) | 28 | grn.2.8 |
| src/text.rs | 87.4% (916/1048) | 64.1% (200/312) | 132 | grn.2.4 |
| src/highlight.rs | 88.9% (559/629) | 76.9% (183/238) | 70 | grn.2.6 |
| src/batch.rs | 91.3% (548/600) | 67.3% (35/52) | 52 | grn.2.9 |
| src/pdf.rs | 92.0% (3686/4007) | 75.1% (535/712) | 321 | grn.2.3 (+2.3.1/.2/.3) |
| src/theme.rs | 93.3% (167/179) | 75.0% (6/8) | 12 | grn.2.8 |
| src/parse/mod.rs | 93.4% (1833/1963) | 80.6% (651/808) | 130 | grn.2.5 |
| src/layout.rs | 94.8% (989/1043) | 84.5% (142/168) | 54 | grn.2.7 |
| src/scanner.rs | 97.1% (167/172) | 86.8% (59/68) | 5 | — |
| src/html.rs | 97.8% (590/603) | 86.9% (73/84) | 13 | — |
| src/compress.rs | 99.2% (369/372) | 89.3% (75/84) | 3 | — |
| src/fonts.rs | 100.0% (27/27) | n/a (0/0) | 0 | — |
| src/span.rs | 100.0% (40/40) | 75.0% (3/4) | 0 | — |

`branches = n/a` means the module emits no conditional branches llvm can
instrument (pure data/`include_bytes!` constants or straight-line code).
