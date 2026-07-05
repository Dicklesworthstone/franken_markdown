# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `pdf-large` | 338.021 ms | 352.797 ms | 352.797 ms | 352.797 ms | 5 | render pre-parsed large mixed Markdown document to PDF |
| `parser-large` | 66.347 ms | 74.296 ms | 74.296 ms | 74.296 ms | 5 | parse generated 1 MiB CommonMark/GFM-like document |
| `hyphen-corpus` | 27.149 ms | 27.503 ms | 27.503 ms | 27.503 ms | 5 | Liang/TeX hyphenation over 50k generated documentation words |
| `pdf-showcase` | 10.964 ms | 13.100 ms | 13.100 ms | 13.100 ms | 5 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.783 ms | 0.969 ms | 0.969 ms | 0.969 ms | 5 | parse + html render of examples/showcase.md |
| `paragraph-1k` | 0.216 ms | 0.231 ms | 0.231 ms | 0.231 ms | 5 | Knuth-Plass baseline breaker over 1000 generated words |
| `font-subset` | 0.026 ms | 0.040 ms | 0.040 ms | 0.040 ms | 5 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
