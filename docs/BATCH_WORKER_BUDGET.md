# Batch Worker-Budget Policy

Status: design contract for the native Asupersync batch renderer
Owner: bead `zmd.1.1` (blocks `zmd.1.3` scheduler implementation)
Scope: native CLI batch/watch rendering only. The pure render core stays
synchronous and `--no-default-features`/wasm32-clean; nothing here enters core
modules.

## Purpose

Choose, deterministically and without a solver dependency, **how many worker
regions** the batch renderer runs and **how deep the ready queue** is allowed to
get, from cheap inputs available at startup. The goal is high throughput without
oversubscribing CPU, exhausting memory, or starving an interactive agent that is
blocking on the run.

The policy is a pure function of its inputs (no clocks, no randomness), so a
given environment + flag set always yields the same budget — a prerequisite for
the deterministic receipts in `zmd.1.3`.

## Inputs

| Input | Symbol | Source | Notes |
|---|---|---|---|
| Available parallelism | `c_avail` | `std::thread::available_parallelism()` (fallback 1) | Logical CPUs visible to the process |
| User worker cap | `c_cap` | `--workers N` (optional) | `0`/absent = unset |
| Mode | `mode` | `--batch-mode interactive\|throughput` (default interactive) | Agent-blocking vs. unattended |
| Memory budget | `M` bytes | `--mem-budget` or 60% of detected/declared RAM | Soft ceiling for concurrent jobs |
| Per-job peak RSS estimate | `r` bytes | max(observed, floor) per category; default 64 MiB | Conservative; refined from receipts |
| Offered rate (watch only) | `lambda` | measured arrivals/sec | Batch mode treats the whole set as backlog |
| Per-worker service rate | `mu` | `1 / mean_service_time` from a warmup/probe | jobs/sec/worker |
| Service-time CV | `CV` | `stddev/mean` of probe service times | Tail/variance signal |

`mu`, `lambda`, and `CV` are only needed for the **watch/streaming** path; the
**batch** path (a bounded set of N files known up front) does not need an arrival
model and must work even when they are unknown.

## Queueing model

For the open (watch) case we use the standard multi-server utilization identity
(`M/M/c` / `M/G/c` for sizing purposes):

```
rho = lambda / (c * mu)          # fraction of worker capacity in use
```

Targets (chosen so the expected queue wait stays bounded and the machine keeps
headroom):

- **interactive / agent mode:** `rho <= 0.70`
- **throughput mode:** `rho <= 0.85`

Solving the target for the worker count gives the *minimum* `c` that keeps
utilization under target:

```
c_needed = ceil( lambda / (rho_target * mu) )
```

High variance inflates queue wait (Pollaczek–Khinchine: wait grows with
`(1 + CV^2)/2`). So when `CV > 1.5` we lower the effective utilization target by
one band and prefer **finer chunking** (one file per task, never coarse batches)
so a few huge PDFs cannot pin every worker while small jobs wait:

```
if CV > 1.5: rho_target <- max(0.50, rho_target - 0.15); chunk <- 1
```

## Policy function (deterministic, solver-free)

```text
fn worker_budget(in) -> Budget:
    c_avail   = max(1, in.c_avail)

    # 1. Desired concurrency from CPU + user cap.
    desired   = in.c_cap if in.c_cap > 0 else c_avail
    desired   = clamp(desired, 1, /* hard sanity cap */ 1024)

    # 2. Interactive mode reserves headroom so the blocking agent stays
    #    responsive; throughput mode uses all visible CPUs.
    if in.mode == interactive and in.c_cap == 0:
        desired = max(1, floor(0.75 * c_avail))   # leave ~1/4 of cores

    # 3. Memory ceiling: never run more concurrent jobs than the budget allows.
    mem_workers = max(1, floor(in.M / max(in.r, 1)))
    workers     = min(desired, mem_workers)

    # 4. Variance guard.
    chunk = 1
    if in.CV > 1.5:
        workers = max(1, floor(0.85 * workers))
        chunk   = 1                                # already 1; never coarsen

    # 5. Watch/streaming: ensure enough workers to keep rho under target,
    #    but never exceed the CPU/mem-bounded ceiling above.
    if in.lambda > 0 and in.mu > 0:
        rho_t   = (0.70 if in.mode == interactive else 0.85)
        if in.CV > 1.5: rho_t = max(0.50, rho_t - 0.15)
        c_need  = ceil(in.lambda / (rho_t * in.mu))
        workers = clamp(c_need, 1, workers)        # cannot exceed ceiling

    # 6. Bounded ready queue → backpressure + memory bound. Depth scales with
    #    workers; 2x gives one in-flight + one queued per worker.
    queue_depth = clamp(2 * workers, 2, 4 * c_avail)

    return Budget { workers, queue_depth, chunk }
```

`Budget.workers` sizes the Asupersync worker regions; `queue_depth` bounds the
ready channel (producers block when full → natural backpressure and a hard cap
on simultaneously-resident job state); `chunk` is the files-per-task granularity.

## Edge cases (required by acceptance)

| Case | Inputs | Result | Why |
|---|---|---|---|
| Single core | `c_avail=1` | `workers=1`, `queue_depth=2` | No parallelism; still bounded queue |
| Use all cores | `c_avail=8`, throughput, no cap | `workers=8` | Throughput mode uses every CPU |
| Interactive default | `c_avail=8`, interactive, no cap | `workers=6` | `floor(0.75*8)`; reserves 2 cores for the agent |
| Cap below cores | `c_avail=8`, `c_cap=3` | `workers=3` | User cap respected (mode headroom not applied when cap set) |
| Cap above cores | `c_avail=4`, `c_cap=16` | `workers=4` | Capped at `c_avail`; never oversubscribe |
| Very large PDFs | `M=2 GiB`, `r=512 MiB`, `c_avail=8` | `workers=min(desired,4)=4` | Memory ceiling `floor(2048/512)=4` dominates |
| Memory-constrained | `M=256 MiB`, `r=64 MiB`, `c_avail=8` | `workers=4` | `floor(256/64)=4` |
| Tight memory, 1 fits | `M=100 MiB`, `r=64 MiB` | `workers=1` | `floor(100/64)=1`; never 0 |
| High variance | `c_avail=8` throughput, `CV=2.0` | `workers=floor(0.85*8)=6`, `chunk=1` | Tail guard drops a band, keeps fine chunks |
| Watch, light load | `lambda=2/s`, `mu=8/s`, interactive | `workers=ceil(2/(0.7*8))=1` | One worker keeps `rho<=0.7`; ceiling unused |
| Watch, heavy load | `lambda=40/s`, `mu=8/s`, `c_avail=8`, throughput | `workers=min(ceil(40/(0.85*8)),8)=min(6,8)=6` | Sized for target utilization under the ceiling |

## Non-goals / notes

- No external queueing-theory solver or LP; the formulas above are closed-form
  and integer-clamped.
- `r` (per-job RSS) starts conservative (64 MiB default, or a per-category floor)
  and is refined from the deterministic receipts the scheduler emits, but the
  policy never *requires* live feedback to produce a safe budget.
- This document is the contract; the implementation lives behind the native-only
  batch feature gate (`zmd.1.2` API/CLI contract, `zmd.1.3` scheduler) and must
  keep `scripts/check-wasm-core.sh` green.
