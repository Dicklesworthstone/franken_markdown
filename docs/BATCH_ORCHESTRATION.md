# Native Batch Orchestration — API & CLI Contract

Status: design contract for the Asupersync-native batch renderer
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
                      [--json] [--font sans|serif] [--no-config]
```

- **inputs**: any mix of files, directories (recursively collected `*.md`/
  `*.markdown`, sorted deterministically), and globs. Empty/none → usage error
  (exit 64) naming the fix.
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
use asupersync::cx::Cx;
use asupersync::types::Outcome;

pub struct BatchPlan {
    pub inputs: Vec<BatchInput>,   // already expanded + deterministically sorted
    pub format: OutputFormat,      // Html | Pdf | Both
    pub out_dir: Option<PathBuf>,
}

pub struct BatchOptions {
    pub render: PdfOptions,        // shared theme/render options (reused as-is)
    pub mode: BatchMode,           // Interactive | Throughput
    pub workers: Option<usize>,    // user cap; None = derive
    pub mem_budget: Option<u64>,
    pub continue_on_error: bool,
}

/// Render every input under a bounded worker budget. `&Cx` first; returns the
/// deterministic receipt as an `Outcome`. Cancellation/budget are observed at
/// per-file checkpoints; the synchronous core render of the *current* file runs
/// to completion (it is not interruptible mid-file), then workers stop.
pub async fn render_batch(
    cx: &Cx,
    plan: BatchPlan,
    opts: &BatchOptions,
) -> Outcome<BatchReceipt>;
```

### Orchestration shape (for `zmd.1.3`)

1. Compute the worker budget with the `BATCH_WORKER_BUDGET.md` policy from
   `available_parallelism`, `--workers`, `--batch-mode`, `--mem-budget`, and a
   per-category RSS floor. → `Budget { workers, queue_depth, chunk }`.
2. Open one **scoped parent region** (`RegionMode`/`RegionPriority`), then spawn
   `workers` **child regions**. Region scope guarantees structured teardown:
   when the parent ends (success, error, or cancel) all children are joined and
   cleaned up — no orphan tasks (asupersync's region-tree invariant).
3. Feed a **bounded ready channel** (`queue_depth`) of per-file tasks. Producers
   block when full → backpressure and a hard cap on simultaneously-resident job
   state.
4. Each worker loop: `checkpoint(cx)` (cancellation/budget observation point) →
   pull next task → call the **synchronous** `render_html`/`render_pdf` →
   write output → push a per-file receipt entry. Wrap the budget with
   `with_budget` so a global byte/time budget can refuse further work cleanly
   (`BudgetRefusal`).
5. On cancellation (`CancelKind`) or budget exhaustion: stop pulling new tasks,
   let in-flight files finish, join children, and return a **partial**
   `BatchReceipt` (every started file is accounted as ok/failed/skipped).

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
  it and the remaining shard files are marked `skipped`.
- Receipts are reassembled in input order from `(index, FileEntry)` pairs, so the
  output is independent of completion/scheduling order.

### `BatchReceipt` (deterministic)

The receipt is **timing-free in its golden fields** so it is byte-stable for
tests (wall-clock lives in a separate, clearly non-golden section):

```jsonc
{
  "schema": "fmd-batch-receipt-v1",
  "format": "both",
  "workers": 6, "queue_depth": 12, "batch_mode": "interactive",
  "inputs": 12, "ok": 11, "failed": 1, "skipped": 0,
  "files": [
    { "input": "a.md", "status": "ok",
      "outputs": [ { "path": "a.html", "bytes": 8123, "sha256": "…" },
                   { "path": "a.pdf",  "bytes": 20111, "sha256": "…" } ] },
    { "input": "bad.md", "status": "failed", "error": "input too large; raise --max-input-bytes" }
  ],
  "cancelled": false, "budget_refused": false
}
```

Per-file outputs reuse the existing deterministic writers, so each file's bytes
+ sha256 are reproducible; ordering is the deterministically-sorted input order,
independent of worker scheduling. This is what makes `zmd.1.3`'s receipts and
`zmd.1.4`'s cancellation lab tests assertable.

## Cancellation & budget semantics (for `zmd.1.4`)

- **Granularity**: the synchronous core render of one file is the atomic unit; it
  is not interrupted mid-file. Cancellation is observed at the per-file
  `checkpoint`, so worst-case extra work after a cancel is "one file per worker".
- **Cleanup**: region scope joins all worker children on cancel; no temp files
  are left half-written (write to a temp path + atomic rename, or only emit on
  success). Partial output policy is documented and tested.
- **Determinism**: `asupersync::lab` (`SporkAppHarness`, injection/chaos) drives
  deterministic cancellation/cleanup lab tests — cancel after k files, assert the
  receipt accounts for exactly the started set and that no orphan regions or
  output temp files remain.

## What this unblocks

- `zmd.1.3` — implement `render_batch` + the bounded scheduler + receipts behind
  the `batch` feature, using this contract and the `BATCH_WORKER_BUDGET.md`
  policy.
- `zmd.1.4` — Asupersync `lab` cancellation/budget tests against the contract.
- `zmd.1.5` — `scripts/batch-throughput.sh` e2e with rich logs.
- `zmd.1.6` — prove `scripts/check-wasm-core.sh` stays green (core never sees the
  `batch` feature).
