# perf-compare: tests/artifacts/perf/20260627T110831Z-a1455bb  ->  tests/artifacts/perf/20260627T111155Z-qw1.3-prefix
variance envelope: ±3.0% on p95   (negative % = faster)

| scenario | class | p50 before→after (Δ%) | p95 before→after (Δ%) | p99 Δ% | out bytes Δ |
|---|---|---|---|---|---|
| font-subset | improvement |     0.029ms→    0.029ms (   -1.6%) |     0.037ms→    0.035ms (   -3.4%) |    -3.4% | +0 |
| html-showcase | noise |     0.083ms→    0.080ms (   -4.3%) |     0.103ms→    0.102ms (   -0.7%) |    -0.7% | +0 |
| hyphen-corpus | noise |  7096.924ms→ 7118.248ms (   +0.3%) |  7111.844ms→ 7119.461ms (   +0.1%) |    +0.1% | +0 |
| paragraph-1k | improvement |   591.737ms→   11.199ms (  -98.1%) |   591.900ms→   11.200ms (  -98.1%) |   -98.1% | +0 |
| parser-large | regression |    33.997ms→   46.736ms (  +37.5%) |    46.810ms→   49.903ms (   +6.6%) |    +6.6% | +0 |
| pdf-large | noise |    53.470ms→   55.512ms (   +3.8%) |    55.107ms→   55.747ms (   +1.2%) |    +1.2% | +0 |
| pdf-showcase | improvement |     2.912ms→    2.753ms (   -5.5%) |     3.593ms→    3.270ms (   -9.0%) |    -9.0% | +0 |

top p95 hotspot: before=hyphen-corpus  after=hyphen-corpus  (unchanged)
peak RSS: 5240 KiB -> 4172 KiB (-1068 KiB)
verdict: improvements in font-subset, paragraph-1k, pdf-showcase
verdict: REGRESSIONS in parser-large
recommended next target: hyphen-corpus (hyphenation) — hyphenation cache perf (qw1.7.1 / layout-perf-proof.sh) [highest p95 in the AFTER run, confidence=high]
