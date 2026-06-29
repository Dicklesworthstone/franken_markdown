# perf-compare: tests/artifacts/perf/fep.6.2-before  ->  tests/artifacts/perf/fep.6.2-after
variance envelope: ±3.0% on p95   (negative % = faster)

| scenario | class | p50 before→after (Δ%) | p95 before→after (Δ%) | p99 Δ% | out bytes Δ |
|---|---|---|---|---|---|
| pdf-large | improvement |    99.956ms→   84.590ms (  -15.4%) |   103.620ms→   85.928ms (  -17.1%) |   -17.1% | +0 |

top p95 hotspot: before=pdf-large  after=pdf-large  (unchanged)
verdict: improvements in pdf-large
recommended next target: pdf-large (render-pdf) — PDF shaping/serialization perf (fep.6.x / pdf-perf-proof.sh) [highest p95 in the AFTER run, confidence=high]
