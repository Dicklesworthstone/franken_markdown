# Native Batch Orchestration — API & CLI Contract

Status: implemented contract for the Asupersync-native batch renderer
Owner: bead `zmd.1.2` (blocks `zmd.1.3` scheduler, `zmd.1.4` lab tests,
`zmd.1.5` e2e script, `zmd.1.6` WASM-isolation proof)
Companion: [`BATCH_WORKER_BUDGET.md`](BATCH_WORKER_BUDGET.md) (`zmd.1.1`)

## Hard boundary (non-negotiable)

The pure render core stays **synchronous, dependency-free, and
`--no-default-features`/wasm32-clean**. Asupersync and all batch orchestration
live behind a **native-only `batch` cargo feature**; no `async`, no `asupersync`,
no filesystem/process/thread types ever enter `ast`/`parse`/`theme`/`html`/
`text`/`layout`/`pdf`. `scripts/check-wasm-core.sh` is the standing proof and a
required gate for every `zmd.1.x` implementation step (this is `zmd.1.6`).

```
Cargo features:
  default = ["cli"]
  cli     = ["dep:clap"]
  batch   = ["cli", "dep:asupersync"]   # native-only; never enabled for wasm
```

The render core is the *leaf*: batch workers call the existing synchronous
`render_html` / `render_pdf` and never the other way around.

## CLI surface decision

Three options were considered:

| Option | Verdict | Why |
|---|---|---|
| `fmd render --batch` | rejected | Overloads single-file `render` output semantics (`--out` is one path; batch has many). Muddies the first command an agent guesses. |
| bare glob expansion (`fmd *.md`) | rejected as the primary surface | Ambiguous (is `fmd a.md b.md` two renders or an error today?), shell-dependent, and hides the worker/budget controls. |
| **`fmd batch <inputs…>`** | **chosen** | A dedicated subcommand with its own flags; keeps `fmd render` trivially simple; discoverable via `capabilities`/`robot-docs`; agent-first. |

### `fmd batch` contract

```
fmd batch <inputs...> [--to html|pdf|both] [--out-dir DIR]
                      [--workers N] [--batch-mode interactive|throughput]
                      [--mem-budget BYTES] [--continue-on-error]
                      [--timeout SECS] [--max-pdf-image-bytes BYTES]
                      [--json] [--font sans|serif] [--css FILE] [--no-config]
```

- **inputs**: any mix of files and directories (recursively collected `*.md`/
  `*.markdown`, sorted deterministically). Shell-expanded globs work as ordinary
  path arguments; `fmd` does not implement its own glob parser. Empty/none →
  usage error (exit 64) naming the fix.
- **outputs**: written under `--out-dir` (default: alongside each input), names
  derived from the input stem (`doc.md` → `doc.html`/`doc.pdf`), matching the
  single-file derivation rules. `--out-dir -` is refused (batch can't stream).
- **stdout is data**: with `--json`, the only thing on stdout is the
  machine-readable **batch receipt** (see below). Without `--json`, stdout stays
  empty and a human summary goes to stderr.
- **stderr is diagnostics/status**: per-file progress and the final summary.
- **exit codes** (reuse the stable set, add batch semantics):
  - `0` all inputs rendered;
  - `66` an input error (e.g. a file missing/oversized) with `--continue-on-error` unset;
  - `70` a render failed with `--continue-on-error` unset;
  - `73`/`74` output write errors;
  - with `--continue-on-error`, per-file failures are recorded in the receipt and
    the process still exits `0` unless *every* input failed.
- **agent ergonomics preserved**: bare `fmd` still prints help; `fmd batch` with
  no inputs errors with a corrective hint; `NO_COLOR`/`CI`/`--no-color` honored;
  `--json` available; errors name the flag that fixes them.

## Library API (native-only, `batch` feature)

`&Cx` first on every owned async fn; `Outcome` is preserved until the CLI
boundary (`main.rs`), which is the only place it becomes an exit code.

```rust
// src/batch.rs  — #![cfg(feature = "batch")]
use asupersync::prelude::*;
use std::path::PathBuf;

pub struct BatchPlan {
    pub inputs: Vec<PathBuf>,      // already expanded + deterministically sorted
    pub format: OutputFormat,      // Html | Pdf | Both
    pub out_dir: Option<PathBuf>,
}

pub struct BatchOptions {
    pub html: HtmlOptions,         // shared HTML options
    pub pdf: PdfOptions,           // shared PDF options
    pub mode: BatchMode,           // Interactive | Throughput
    pub workers: Option<usize>,    // user cap; None = derive
    pub mem_budget: Option<u64>,
    pub continue_on_error: bool,
    pub timeout_secs: Option<u64>,
    pub max_input_bytes: u64,
    pub max_pdf_image_bytes: u64,
}

/// Render every input under a bounded worker budget. `&Cx` first; returns the
/// deterministic receipt as an `Outcome`. Cancellation/budget are observed at
/// per-file checkpoints; the synchronous core render of the *current* file runs
/// to completion (it is not interruptible mid-file), then workers stop.
pub async fn render_batch(
    cx: &Cx,
    plan: BatchPlan,
    opts: &BatchOptions,
    budget: WorkerBudget,
) -> Outcome<BatchReceipt, BatchError>;
```

### Orchestration shape (for `zmd.1.3`)

1. Compute the worker budget with the `BATCH_WORKER_BUDGET.md` policy from
   `available_parallelism`, `--workers`, `--batch-mode`, `--mem-budget`, and a
   per-category RSS floor. The runtime worker count is capped to the number of
   inputs.
2. Start a multi-thread Asupersync runtime sized to the clamped worker budget.
   Worker tasks are spawned through `Runtime::current_handle().spawn(..)`.
3. Split inputs round-robin into `workers` shards. Each worker renders its shard
   serially, so the number of in-flight synchronous renders is exactly bounded by
   the worker budget without a shared queue.
4. Each worker loop observes cancellation at per-file checkpoints, calls the
   synchronous `render_html`/`render_pdf`, stages output writes, and pushes one
   receipt entry.
5. On cancellation or timeout, stop starting new files, let in-flight files
   finish, join worker tasks, and return a partial `BatchReceipt` where every
   input is accounted as ok/failed/skipped.

### As implemented (`zmd.1.3`, `src/batch.rs`)

The shipped scheduler keeps the contract above but uses the simplest structure
that bounds concurrency deterministically:

- A multi-thread runtime is sized to `budget.workers` (capped to the input
  count). Tasks are spawned through `Runtime::current_handle().spawn(..)` — the
  documented `block_on` spawn path — rather than `cx.scope()`/`JoinSet`, because
  the ambient `block_on` context is not region-bound for structured spawn on the
  multi-thread runtime.
- Inputs are split **round-robin into `workers` shards**; each worker task
  renders its shard serially (a blocking call into the synchronous core, one
  render in flight per worker → peak memory bounded to `workers × per-job RSS`).
  This replaces the dynamic bounded ready channel: it bounds concurrency and peak
  memory identically and is deterministic, at the cost of static (not
  work-stealing) load balancing. A dynamic queue for uneven job-size balancing is
  a documented future refinement.
- Cancellation is cooperative at the per-file `checkpoint`; a shared flag records
  it and the remaining shard files are marked `skipped`. The CLI reaches this path
  via `--timeout <secs>`: a plain OS-thread watchdog (no `unsafe`, so no POSIX
  signal handler) flips the same flag once the deadline passes, and the receipt is
  marked `cancelled`. A host embedding `render_batch` can also cancel through its
  own `Cx`.
- Receipts are reassembled in input order from `(index, FileEntry)` pairs, so the
  output is independent of completion/scheduling order.

### `BatchReceipt` (deterministic)

The receipt is **timing-free in its golden fields** so it is byte-stable for
tests (wall-clock lives in a separate, clearly non-golden section):

```jsonc
{
  "schema": "fmd-batch-receipt-v1",
  "format": "both",
  "batch_mode": "interactive", "workers": 6, "queue_depth": 12,
  "inputs": 12, "ok": 11, "failed": 1, "skipped": 0, "cancelled": false,
  "files": [
    { "input": "a.md", "status": "ok",
      "outputs": [ { "path": "a.html", "bytes": 8123, "fnv1a64": "…" },
                   { "path": "a.pdf",  "bytes": 20111, "fnv1a64": "…" } ] },
    { "input": "bad.md", "status": "failed",
      "error": "input too large; raise --max-input-bytes",
      "error_kind": "input", "outputs": [] }
  ]
}
```

Per-file outputs reuse the existing deterministic writers, so each file's bytes
+ FNV-1a-64 content fingerprint are reproducible; ordering is the
deterministically-sorted input order, independent of worker scheduling. The hash
is a deterministic receipt fingerprint, not an authenticity checksum. This is
what makes `zmd.1.3`'s receipts and `zmd.1.4`'s cancellation lab tests
assertable.

## Cancellation & budget semantics (for `zmd.1.4`)

- **Granularity**: the synchronous core render of one file is the atomic unit; it
  is not interrupted mid-file. Cancellation is observed at the per-file
  `checkpoint`, so worst-case extra work after a cancel is "one file per worker".
- **Cleanup**: worker tasks are joined before the receipt returns; no temp files
  are left half-written because file outputs are staged and committed atomically,
  or emitted only after render success. Partial output policy is documented and
  tested.
- **Determinism**: cancellation tests inject a cancelled `Cx` before worker spawn
  for exact accounting, and timeout tests assert completed fast runs are not
  spuriously marked cancelled.

## What this unblocks

- `zmd.1.3` — implement `render_batch` + the bounded scheduler + receipts behind
  the `batch` feature, using this contract and the `BATCH_WORKER_BUDGET.md`
  policy.
- `zmd.1.4` — Asupersync `lab` cancellation/budget tests against the contract.
- `zmd.1.5` — `scripts/batch-throughput.sh` e2e with rich logs.
- `zmd.1.6` — prove `scripts/check-wasm-core.sh` stays green (core never sees the
  `batch` feature).
