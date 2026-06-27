# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `hyphen-corpus` | hyphenation | 7111.844 ms | 7101.870 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 2 | `paragraph-1k` | line-break | 591.900 ms | 591.111 ms | 10925 B | 1 B | `inprocess.jsonl` |
| 3 | `pdf-large` | render-pdf | 55.107 ms | 53.758 ms | 160958 B | 832241 B | `inprocess.jsonl` |
| 4 | `parser-large` | parse | 46.810 ms | 38.227 ms | 1048576 B | 2334041 B | `inprocess.jsonl` |
| 5 | `pdf-showcase` | render-pdf | 3.593 ms | 3.130 ms | 1343 B | 17120 B | `inprocess.jsonl` |
| 6 | `html-showcase` | render-html | 0.103 ms | 0.087 ms | 1343 B | 7291 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.037 ms | 0.030 ms | 89 B | 15604 B | `inprocess.jsonl` |
