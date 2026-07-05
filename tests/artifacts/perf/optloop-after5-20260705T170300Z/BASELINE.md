# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `pdf-large` | 339.610 ms | 346.178 ms | 346.178 ms | 346.178 ms | 5 | render pre-parsed large mixed Markdown document to PDF |
| `parser-large` | 67.624 ms | 71.847 ms | 71.847 ms | 71.847 ms | 5 | parse generated 1 MiB CommonMark/GFM-like document |
| `hyphen-corpus` | 29.428 ms | 30.791 ms | 30.791 ms | 30.791 ms | 5 | Liang/TeX hyphenation over 50k generated documentation words |
| `pdf-showcase` | 10.752 ms | 11.098 ms | 11.098 ms | 11.098 ms | 5 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.802 ms | 1.200 ms | 1.200 ms | 1.200 ms | 5 | parse + html render of examples/showcase.md |
| `paragraph-1k` | 0.233 ms | 0.265 ms | 0.265 ms | 0.265 ms | 5 | Knuth-Plass baseline breaker over 1000 generated words |
| `font-subset` | 0.028 ms | 0.039 ms | 0.039 ms | 0.039 ms | 5 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
