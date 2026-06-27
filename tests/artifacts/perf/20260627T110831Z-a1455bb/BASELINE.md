# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `hyphen-corpus` | 7096.924 ms | 7111.844 ms | 7111.844 ms | 7111.844 ms | 3 | Liang/TeX hyphenation over 50k generated documentation words |
| `paragraph-1k` | 591.737 ms | 591.900 ms | 591.900 ms | 591.900 ms | 3 | Knuth-Plass baseline breaker over 1000 generated words |
| `pdf-large` | 53.470 ms | 55.107 ms | 55.107 ms | 55.107 ms | 3 | render pre-parsed large mixed Markdown document to PDF |
| `parser-large` | 33.997 ms | 46.810 ms | 46.810 ms | 46.810 ms | 3 | parse generated 1 MiB CommonMark/GFM-like document |
| `pdf-showcase` | 2.912 ms | 3.593 ms | 3.593 ms | 3.593 ms | 3 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.083 ms | 0.103 ms | 0.103 ms | 0.103 ms | 3 | parse + html render of examples/showcase.md |
| `font-subset` | 0.029 ms | 0.037 ms | 0.037 ms | 0.037 ms | 3 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
