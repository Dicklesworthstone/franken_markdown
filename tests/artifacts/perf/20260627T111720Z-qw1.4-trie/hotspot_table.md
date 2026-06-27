# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `pdf-large` | render-pdf | 60.250 ms | 60.188 ms | 160958 B | 838001 B | `inprocess.jsonl` |
| 2 | `parser-large` | parse | 49.146 ms | 41.307 ms | 1048576 B | 2334041 B | `inprocess.jsonl` |
| 3 | `hyphen-corpus` | hyphenation | 28.821 ms | 28.739 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 4 | `paragraph-1k` | line-break | 11.023 ms | 11.008 ms | 10925 B | 1 B | `inprocess.jsonl` |
| 5 | `pdf-showcase` | render-pdf | 5.319 ms | 4.928 ms | 1343 B | 26169 B | `inprocess.jsonl` |
| 6 | `html-showcase` | render-html | 0.102 ms | 0.087 ms | 1343 B | 7291 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.034 ms | 0.030 ms | 89 B | 15604 B | `inprocess.jsonl` |
