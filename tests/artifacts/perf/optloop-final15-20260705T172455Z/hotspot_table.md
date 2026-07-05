# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `pdf-large` | render-pdf | 353.764 ms | 346.383 ms | 160958 B | 609524 B | `inprocess.jsonl` |
| 2 | `parser-large` | parse | 63.067 ms | 62.017 ms | 1048576 B | 2643891 B | `inprocess.jsonl` |
| 3 | `hyphen-corpus` | hyphenation | 26.145 ms | 26.086 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 4 | `pdf-showcase` | render-pdf | 11.298 ms | 10.949 ms | 3630 B | 53930 B | `inprocess.jsonl` |
| 5 | `html-showcase` | render-html | 1.313 ms | 0.937 ms | 3630 B | 88407 B | `inprocess.jsonl` |
| 6 | `paragraph-1k` | line-break | 0.211 ms | 0.186 ms | 10925 B | 120 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.038 ms | 0.029 ms | 90 B | 8460 B | `inprocess.jsonl` |
