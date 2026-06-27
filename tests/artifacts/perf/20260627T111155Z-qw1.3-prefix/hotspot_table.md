# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `hyphen-corpus` | hyphenation | 7119.461 ms | 7112.784 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 2 | `pdf-large` | render-pdf | 55.747 ms | 55.564 ms | 160958 B | 832241 B | `inprocess.jsonl` |
| 3 | `parser-large` | parse | 49.903 ms | 45.036 ms | 1048576 B | 2334041 B | `inprocess.jsonl` |
| 4 | `paragraph-1k` | line-break | 11.200 ms | 11.190 ms | 10925 B | 1 B | `inprocess.jsonl` |
| 5 | `pdf-showcase` | render-pdf | 3.270 ms | 2.925 ms | 1343 B | 17120 B | `inprocess.jsonl` |
| 6 | `html-showcase` | render-html | 0.102 ms | 0.086 ms | 1343 B | 7291 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.035 ms | 0.029 ms | 89 B | 15604 B | `inprocess.jsonl` |
