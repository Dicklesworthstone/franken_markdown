# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `pdf-large` | 347.433 ms | 353.764 ms | 353.764 ms | 353.764 ms | 5 | render pre-parsed large mixed Markdown document to PDF |
| `parser-large` | 62.030 ms | 63.067 ms | 63.067 ms | 63.067 ms | 5 | parse generated 1 MiB CommonMark/GFM-like document |
| `hyphen-corpus` | 26.085 ms | 26.145 ms | 26.145 ms | 26.145 ms | 5 | Liang/TeX hyphenation over 50k generated documentation words |
| `pdf-showcase` | 10.837 ms | 11.298 ms | 11.298 ms | 11.298 ms | 5 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.809 ms | 1.313 ms | 1.313 ms | 1.313 ms | 5 | parse + html render of examples/showcase.md |
| `paragraph-1k` | 0.179 ms | 0.211 ms | 0.211 ms | 0.211 ms | 5 | Knuth-Plass baseline breaker over 1000 generated words |
| `font-subset` | 0.026 ms | 0.038 ms | 0.038 ms | 0.038 ms | 5 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
