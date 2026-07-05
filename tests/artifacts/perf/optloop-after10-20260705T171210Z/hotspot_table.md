# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `pdf-large` | render-pdf | 352.797 ms | 339.276 ms | 160958 B | 609524 B | `inprocess.jsonl` |
| 2 | `parser-large` | parse | 74.296 ms | 66.825 ms | 1048576 B | 2643891 B | `inprocess.jsonl` |
| 3 | `hyphen-corpus` | hyphenation | 27.503 ms | 27.103 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 4 | `pdf-showcase` | render-pdf | 13.100 ms | 11.379 ms | 3630 B | 53930 B | `inprocess.jsonl` |
| 5 | `html-showcase` | render-html | 0.969 ms | 0.817 ms | 3630 B | 88407 B | `inprocess.jsonl` |
| 6 | `paragraph-1k` | line-break | 0.231 ms | 0.219 ms | 10925 B | 120 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.040 ms | 0.029 ms | 90 B | 8460 B | `inprocess.jsonl` |
