# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `hyphen-corpus` | 7118.248 ms | 7119.461 ms | 7119.461 ms | 7119.461 ms | 3 | Liang/TeX hyphenation over 50k generated documentation words |
| `pdf-large` | 55.512 ms | 55.747 ms | 55.747 ms | 55.747 ms | 3 | render pre-parsed large mixed Markdown document to PDF |
| `parser-large` | 46.736 ms | 49.903 ms | 49.903 ms | 49.903 ms | 3 | parse generated 1 MiB CommonMark/GFM-like document |
| `paragraph-1k` | 11.199 ms | 11.200 ms | 11.200 ms | 11.200 ms | 3 | Knuth-Plass baseline breaker over 1000 generated words |
| `pdf-showcase` | 2.753 ms | 3.270 ms | 3.270 ms | 3.270 ms | 3 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.080 ms | 0.102 ms | 0.102 ms | 0.102 ms | 3 | parse + html render of examples/showcase.md |
| `font-subset` | 0.029 ms | 0.035 ms | 0.035 ms | 0.035 ms | 3 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
