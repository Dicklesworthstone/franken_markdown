# DEFINE - fmd measured optimization gauntlet

## Scenario
Run the canonical franken_markdown performance scenarios from `PERFORMANCE_OPTIMIZATION_PLAN.md`: HTML/PDF showcase renders, large parser input, 1000-word paragraph breaking, 50k-word TeX hyphenation, bundled font subsetting, large PDF render, and batch-100 CLI throughput.

## Metric
Primary metric is p95 wall-clock latency per scenario. Secondary metrics are output bytes, throughput proxies, peak RSS via `/usr/bin/time -v`, and hardware counters from `perf stat` when permitted.

## Budget
This first run establishes baseline. Future runs should treat >10% p95 drift as noise/investigate boundary and >20% drift as regression or material improvement.

## Golden output
Golden outputs are written to `golden/` and checksummed in `golden_checksums.txt`.

## Schema
This run follows `fmd-perf-artifact-v1`; see `SCHEMA.md`,
`schema_manifest.json`, and `docs/PERFORMANCE_ARTIFACT_SCHEMA.md`.

## Scope boundary
This run does not change code or prove a specific optimization. It ranks targets for one-lever optimization commits.

## Variance envelope
- <=10% drift vs prior same-host run: noise.
- >10% drift: investigate.
- >20%, or 3 consecutive >10%: escalate.

## Stakeholder / requester
Jeffrey requested a hyper-optimized Markdown/text/PDF rendering plan with multicore and SIMD strategy.
