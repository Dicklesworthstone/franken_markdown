# qw1.8.4 — run comparison and bottleneck-shift report

Evidence for `scripts/perf-compare.sh` (bead qw1.8.4). It compares two perf
artifact directories (`docs/PERFORMANCE_ARTIFACT_SCHEMA.md`) and reports
per-scenario p50/p95/p99 deltas, output-byte deltas, peak-RSS deltas (when
`time.stderr` is present), the top-hotspot order and whether it shifted, a
variance-envelope classification (noise / improvement / regression), and a
recommended next target (the new top p95 hotspot).

- `self-test.txt` — `--self-test`: synthetic fixtures prove that a same-run
  comparison reports no material change, and that improvement, regression, and
  added-scenario cases are classified correctly.
- `prefix-comparison.md` / `.json` — baseline vs the qw1.3 prefix-metrics
  line-breaker. `paragraph-1k` p95 drops **591.9ms -> 11.2ms (-98.1%)** (the
  expected large improvement), and the tool also honestly flags a `parser-large`
  regression and a peak-RSS reduction in the same run.
- `fep62-comparison.md` — fep.6.2 PDF shaping before/after: `pdf-large` p95
  **103.6ms -> 85.9ms (-17.1%)**, classified as an improvement.

Run it yourself:

    scripts/perf-compare.sh BEFORE_DIR AFTER_DIR        # human table
    scripts/perf-compare.sh --json BEFORE_DIR AFTER_DIR # machine summary
    scripts/perf-compare.sh --self-test                 # prove the classifier
