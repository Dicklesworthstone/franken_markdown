# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `pdf-large` | render-pdf | 346.178 ms | 339.207 ms | 160958 B | 609524 B | `inprocess.jsonl` |
| 2 | `parser-large` | parse | 71.847 ms | 68.745 ms | 1048576 B | 2643891 B | `inprocess.jsonl` |
| 3 | `hyphen-corpus` | hyphenation | 30.791 ms | 29.770 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 4 | `pdf-showcase` | render-pdf | 11.098 ms | 10.861 ms | 3630 B | 53930 B | `inprocess.jsonl` |
| 5 | `html-showcase` | render-html | 1.200 ms | 0.890 ms | 3630 B | 88407 B | `inprocess.jsonl` |
| 6 | `paragraph-1k` | line-break | 0.265 ms | 0.239 ms | 10925 B | 120 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.039 ms | 0.029 ms | 90 B | 8460 B | `inprocess.jsonl` |
