# Hotspot Table

| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |
|---:|---|---|---:|---:|---:|---:|---|
| 1 | `parser-large` | parse | 1985.255 ms | 1952.836 ms | 1048576 B | 2643891 B | `inprocess.jsonl` |
| 2 | `pdf-large` | render-pdf | 362.162 ms | 340.714 ms | 160958 B | 609524 B | `inprocess.jsonl` |
| 3 | `hyphen-corpus` | hyphenation | 29.779 ms | 28.579 ms | 649996 B | 50000 B | `inprocess.jsonl` |
| 4 | `pdf-showcase` | render-pdf | 11.404 ms | 10.835 ms | 3630 B | 53930 B | `inprocess.jsonl` |
| 5 | `html-showcase` | render-html | 0.967 ms | 0.794 ms | 3630 B | 88407 B | `inprocess.jsonl` |
| 6 | `paragraph-1k` | line-break | 0.358 ms | 0.253 ms | 10925 B | 120 B | `inprocess.jsonl` |
| 7 | `font-subset` | font-subset | 0.042 ms | 0.024 ms | 90 B | 8460 B | `inprocess.jsonl` |
