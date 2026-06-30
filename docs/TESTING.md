# Testing & Coverage

Date: 2026-06-29 · Bead: `grn.1.2` (epic `grn`: test rigor)

How `franken_markdown` is tested, why the tests look the way they do, and how to
reproduce every measurement locally. The guiding rule: **tests exercise the real
system with real inputs.** A passing test should mean the product works, not that
a mock of the product agrees with another mock.

## Testing philosophy: real inputs over mocks and fakes

The engine is pure, deterministic, and dependency-lean, which makes real inputs
cheap — so we use them everywhere instead of test doubles:

- **Real Markdown.** Parser, HTML, and PDF tests feed actual Markdown source
  (including the official CommonMark 0.31.2 suite, `tests/fixtures/commonmark/`)
  and assert on real rendered output, not on a stubbed AST.
- **Real fonts.** `text.rs`, `layout.rs`, `kern_test.rs`, `lig_test.rs`, and the
  PDF embedding tests parse the *bundled* TrueType fonts (`fonts/`, embedded via
  `include_bytes!`) and assert true metrics, GPOS kerning, GSUB ligatures, and
  subset round-trips through our own reader — never a faked metrics table for the
  system under test.
- **Real config files.** `config_test.rs` writes actual `key=value` files to a
  temp path (via `FMD_CONFIG`) and reads them back through the real config
  resolver; it never monkey-patches the filesystem.
- **The real binary.** `cli_contract.rs` spawns the compiled `fmd` binary and
  asserts on its actual stdout/stderr/exit codes — the agent-facing contract is
  verified against the shipped artifact, not an in-process shim.
- **No mocking framework, no `[dev-dependencies]`.** Every test uses only `std`
  and the crate itself. There is nothing to mock *with*, by design.

### The one documented test double, and why it is not a mock

`tests/layout_test.rs` defines `StubMetrics` — a ~10-line real implementation of
the `AdvanceMetrics`/`PairMetrics` traits (`src/layout.rs`) with hand-chosen glyph
advances (`i=250, m=900, space=250, other=500`) and one kerning pair (`A/V=-80`).

This is **not** a mock of the system under test. The system under test is the
Knuth-Plass line breaker and the measurement arithmetic; `StubMetrics` is a
*controlled input* that makes expected milli-point widths exactly hand-computable
and font-independent (e.g. `"mi mi"` is unambiguously `900+250+250+900+250` units),
so a break-decision regression is caught by an exact integer assertion rather than
a fragile float compare against whatever the bundled font happens to measure. The
same code paths are *also* driven by the real bundled-font metrics (see
`grn.3.1`), so the algorithm is proven against both a known oracle and reality.
The `grn.3.2` gate prevents any *new*, undocumented `Stub`/`Mock`/`Fake`/`Dummy`
double from being introduced without this kind of justification.

## Test tiers

| Tier | What it proves | Run it |
|---|---|---|
| Unit + integration (`tests/*.rs`, inline `#[cfg(test)]`) | Per-module behavior on real inputs | `cargo test` |
| Batch (`--features batch`) | Asupersync orchestration, deterministic receipts, cancellation | `cargo test --features batch` |
| CommonMark conformance (ratcheted) | Parser/emitter spec match, floor can only rise | `scripts/commonmark-conformance.sh` |
| Determinism | Byte-identical output across repeated runs | `scripts/check-determinism.sh` |
| Clean-room policy | Zero-dep core, no banned crates, no `unsafe`, batch isolation | `scripts/check-policy.sh` |
| WASM core + package | `--no-default-features` + wasm32 build, headless render, native parity | `scripts/check-wasm-core.sh`, `scripts/check-wasm-package.sh` |
| CLI output contract | stdout=data / stderr=diagnostics, JSON envelopes, exit codes | `scripts/cli-output-contract.sh`, `tests/cli_contract.rs` |
| Coverage (ratcheted floor) | Line/region/branch coverage across all feature configs | `scripts/coverage.sh` |
| Property + metamorphic | Cross-cutting invariants over generated inputs (injection-safety, determinism, PDF structure) | `tests/parser_metamorphic.rs` (via `cargo test`) |
| Fuzz (generative) | No panic / termination / balanced spans over arbitrary bytes + deep nesting | `tests/parser_fuzz.rs` (via `cargo test`) |
| Golden output (regenerable) | HTML + PDF rendered-output regression snapshots | `tests/golden_output.rs`; update with `UPDATE_GOLDEN=1` |
| Mutation (ratcheted ceiling) | Tests actually *fail* when the code is wrong | `scripts/mutation.sh` |
| Test-double gate | No new undocumented Stub/Mock/Fake/Dummy doubles | `scripts/check-test-doubles.sh` |
| E2E suite (structured logging) | Every CLI workflow/flag/error path against the real binary | `scripts/e2e/run-all.sh` |
| Everything at once | The full gauntlet with a combined report | `scripts/test-all.sh` (`--fast` to skip coverage + e2e) |
| Perf proofs | Profile-guided, evidence-gated optimization decisions | `scripts/perf-*.sh` |

## Coverage methodology

Coverage is measured by `scripts/coverage.sh`, a wrapper over `cargo-llvm-cov`.
It is deliberately more than a one-line `cargo llvm-cov`:

1. **Line, region, AND branch coverage.** It passes `--branch` (nightly,
   unstable) so conditional coverage is measured, not just line/region/function.
   Branch coverage is the honest signal for a parser/layout engine full of
   `match` arms and boundary conditions.
2. **Three merged feature passes**, because feature-gated code is invisible to a
   default run and "0% because unmeasured" must never masquerade as "0% because
   untested":
   - `default(cli)` — the full product surface plus every integration test;
   - `--features batch` — `batch.rs` (Asupersync), absent from the default graph;
   - `--features wasm-bindgen` — `wasm_abi.rs`, the browser ABI adapter, so it
     appears in the report instead of being dead-code-eliminated.
   The passes accumulate via `cargo llvm-cov --no-report` and are combined into
   one report.
3. **Production-code-only view.** The report excludes the integration-test
   sources and the thin `src/bin/` + `src/main.rs` shims
   (`--ignore-filename-regex`). Bundled fonts and the hyphenation pattern table
   are `include_bytes!`/`include_str!` binary blobs and are never instrumented.
4. **Deterministic artifacts** under `tests/artifacts/coverage/<run-id>/`:
   `summary.json` (machine-readable, per-module, no wall-clock), `summary.md`
   (human), `lcov.info`, `coverage.txt`, and a browsable `html/` report. Per-run
   directories are gitignored; the committed baseline ledger is
   `tests/artifacts/coverage/baseline.{tsv,md}`.

### Running coverage

```bash
scripts/coverage.sh                 # full merged run (run-id "local")
scripts/coverage.sh my-run-id       # full run under a named id
scripts/coverage.sh --quick         # lib unit tests only, fast iteration
scripts/coverage.sh --self-test     # CI-fast: prove the toolchain + machinery work
```

Prerequisites: a nightly toolchain (already pinned in `rust-toolchain.toml`),
`cargo-llvm-cov` (`cargo install cargo-llvm-cov`), and the `llvm-tools-preview`
component (`rustup component add llvm-tools-preview` — installed in CI; not pinned
in `rust-toolchain.toml` to keep the default dev install light).

Exit codes: `0` success · `2` missing prerequisite · `3` a coverage pass failed
under instrumentation · `4` report/aggregation failure.

## Beyond line coverage

Line/branch coverage proves a line *ran*; it does not prove a test would *fail* if
that line were wrong. Three further tiers close that gap:

- **Mutation testing** (`scripts/mutation.sh`, cargo-mutants). It rewrites the
  source (negate a condition, swap an operator, change a return) and reruns the
  suite; a surviving mutant is a hole in test *effectiveness*. Because a full-tree
  run is hours, it gates a curated, well-tested scope (`FMD_MUTANTS_FILES` to
  override) with a **ratcheted survivor ceiling**
  (`tests/fixtures/mutation/survivor-ceiling.txt`): survivors can only go down.
  The escaped mutants are recorded in `tests/fixtures/mutation/survivors.txt` for
  triage. Raise/lower with `scripts/mutation.sh --update-ceiling`.
- **Property + metamorphic** (`tests/parser_metamorphic.rs`) and **generative
  fuzz** (`tests/parser_fuzz.rs`): invariants over thousands of seeded inputs —
  no panic, termination (the block-nesting recursion bound), balanced source
  spans, HTML injection-safety, render determinism, and PDF structural soundness.
  Both run under the normal `cargo test`.
- **Golden output regression** (`tests/golden_output.rs`): deterministic HTML +
  PDF snapshots of representative documents. A change in the emitter, theme,
  highlighter, or PDF writer moves a fingerprint and fails the test; regenerate
  after a reviewed intentional change with `UPDATE_GOLDEN=1 cargo test --test
  golden_output`.

## Current numbers

The committed baseline is [`../tests/artifacts/coverage/baseline.md`](../tests/artifacts/coverage/baseline.md)
(machine form: `baseline.tsv`). As of 2026-06-30, after the grn.2 mock-free
per-module gap-fill, merging all three feature passes: **95.6% line · 94.3%
region · 85.7% branch · 97.4% function** (up from a pre-gap-fill 88.1% line /
76.5% branch). Every module is now ≥ 92% line except `cli.rs` (81.6%, whose
residual lines are the `batch`-feature surface that the default binary the
contract tests drive does not compile). The committed floor
(`tests/fixtures/coverage/coverage-floor.txt`: lines ≥ 95, regions ≥ 94, branches
≥ 85, functions ≥ 97) is enforced by the CI `coverage` job
(`scripts/coverage.sh --check`), mirroring the CommonMark conformance floor: the
number can only go up. Raise it with `scripts/coverage.sh --update-floor`.
