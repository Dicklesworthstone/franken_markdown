# Baseline

| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |
|---|---:|---:|---:|---:|---:|---|
| `parser-large` | 1952.066 ms | 1985.255 ms | 1985.255 ms | 1985.255 ms | 20 | parse generated 1 MiB CommonMark/GFM-like document |
| `pdf-large` | 337.662 ms | 362.162 ms | 362.162 ms | 362.162 ms | 20 | render pre-parsed large mixed Markdown document to PDF |
| `hyphen-corpus` | 28.373 ms | 29.779 ms | 29.779 ms | 29.779 ms | 20 | Liang/TeX hyphenation over 50k generated documentation words |
| `pdf-showcase` | 10.783 ms | 11.404 ms | 11.404 ms | 11.404 ms | 20 | parse + embedded-font PDF render of examples/showcase.md |
| `html-showcase` | 0.789 ms | 0.967 ms | 0.967 ms | 0.967 ms | 20 | parse + html render of examples/showcase.md |
| `paragraph-1k` | 0.233 ms | 0.358 ms | 0.358 ms | 0.358 ms | 20 | Knuth-Plass baseline breaker over 1000 generated words |
| `font-subset` | 0.022 ms | 0.042 ms | 0.042 ms | 0.042 ms | 20 | subset bundled IBM Plex Sans over generated document character set |

## CLI Baselines

See `hyperfine.txt` and `hyperfine.json`.

## Peak RSS

See `time.stderr`.

## Hardware Counters

See `perf-stat.stderr` and `perf-stat.stdout`.
