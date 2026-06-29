# zmd.1.5 — native batch throughput e2e (smoke run)

Canonical metadata for `scripts/batch-throughput.sh` (the work/ corpus + rendered
outputs are regenerable and not committed). Reproduce a full run:

    scripts/batch-throughput.sh                 # 100 files, 5 iters
    scripts/batch-throughput.sh --self-test     # tiny corpus, CI-friendly

- `fingerprint.json` — git SHA, toolchain, host CPU, available parallelism.
- `DEFINE.md` — scenarios, the queueing view (mu/lambda/c/rho; rho~1 = saturated
  closed batch), and where cancellation/budget accounting is proven (zmd.1.4).
- `inprocess.jsonl` — one `perf_sample` per scenario (schema fmd-perf-artifact-v1).
- `summary.json` — per-scenario ok/failed/workers/queue/p50-95-99/throughput/RSS/
  policy, plus `parallel_speedup_pdf` (serial vs parallel PDF — the meaningful
  scaling number). The script also asserts receipt byte-identity across two runs
  and that the unwritable-output case fails (exit 70).
