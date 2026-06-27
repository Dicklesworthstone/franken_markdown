# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `pdf-large` | 60.188 ms | 60.250 ms | 60.250 ms | 60.250 ms | 3 | render pre-parsed large mixed Markdown document to PDF |
| `parser-large` | 38.073 ms | 49.146 ms | 49.146 ms | 49.146 ms | 3 | parse generated 1 MiB CommonMark/GFM-like document |
| `hyphen-corpus` | 28.801 ms | 28.821 ms | 28.821 ms | 28.821 ms | 3 | Liang/TeX hyphenation over 50k generated documentation words |
| `paragraph-1k` | 11.015 ms | 11.023 ms | 11.023 ms | 11.023 ms | 3 | Knuth-Plass baseline breaker over 1000 generated words |
| `pdf-showcase` | 4.847 ms | 5.319 ms | 5.319 ms | 5.319 ms | 3 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.082 ms | 0.102 ms | 0.102 ms | 0.102 ms | 3 | parse + html render of examples/showcase.md |
| `font-subset` | 0.030 ms | 0.034 ms | 0.034 ms | 0.034 ms | 3 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
