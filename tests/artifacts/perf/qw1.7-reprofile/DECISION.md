# qw1.7 — layout/hyphenation re-profile decision matrix

Re-profiling gate after the PDF and parser optimization waves. Decides whether
any layout/hyphenation child bead is worth optimizing now, per the project's
"attribute before optimizing" discipline.

## Current p95 ranking (release profile, this host; see `fingerprint.json`)

| Rank | Scenario | Category | p50 | p95 | p99 |
|---|---|---|---|---|---|
| 1 | `pdf-large` | render-pdf | 118.343 ms | **121.293 ms** | 121.293 ms |
| 2 | `parser-large` | parse | 36.945 ms | **43.056 ms** | 43.056 ms |
| 3 | `hyphen-corpus` | hyphenation | 27.305 ms | **27.458 ms** | 27.458 ms |
| 4 | `paragraph-1k` | line-break | 7.443 ms | **7.478 ms** | 7.478 ms |

Notes:
- `hyphen-corpus` measures Liang/TeX hyphenation over a **50,000-word** synthetic
  corpus (~0.55 µs/word). Realistic documents have far fewer words, so the
  *per-document* hyphenation cost is small; this scenario is a stress test, not a
  typical workload.
- `paragraph-1k` is already well-optimized: prior prefix-metric work (qw1.3) took
  it from ~592 ms to ~7.5 ms. Knuth-Plass paragraph breaking is no longer hot.
- The first-order costs are clearly **`pdf-large` (render-pdf)** and
  **`parser-large` (parse)** — together far above all layout/hyphenation work.

## Decision

Layout/hyphenation is **not first-order**. PDF rendering and parsing dominate the
p95 budget by 2–4×. Per the gate's rule (optimize only when a target is top-5 and
EV is high), every remaining layout/hyphenation child is **deferred** until either
(a) the PDF/parser hot paths are addressed and layout re-enters the top-2, or
(b) a realistic workload makes hyphenation/line-breaking top-5 again.

| Child | Title | Decision | Rationale |
|---|---|---|---|
| `qw1.7.1` | bounded per-document hyphenation cache | **DEFER** | Hyphenation is rank 3 and only on a 50k-word stress corpus; a cache helps repeated long words but cannot beat the pdf/parser first-order costs. Low-risk; revisit if a real doc makes hyphenation top-5. |
| `qw1.7.2` | compact/double-array hyphen trie | **DEFER (lean reject)** | The existing trie already cut hyphenation cost; further trie compaction is a memory micro-optimization, not a p95 win. No EV at current ranking. |
| `qw1.7.4` | certified active-list pruning for Knuth-Plass | **DEFER** | `paragraph-1k` is 7.5 ms (rank 4) after prefix metrics; pruning the active list is a micro-optimization on a non-hot path. Keep the certified-pruning design idea for if line-breaking re-enters the top-2. |
| `qw1.7.5` | fixed-point microtypography cost hooks | **DEFER** | This is a typographic *quality* feature (protrusion/expansion/fitness), not a perf win, and it *adds* cost. Gate it behind the layout ledger/scratch groundwork and only after line-break quality + runtime can be proven. |

`qw1.7.3` (scratch arena) and `qw1.7.6` (layout quality/perf e2e script) are already
closed.

## Next first-order targets (outside this subtree)

`pdf-large` (PDF shaping/ToUnicode/serialization — the `fep.6.x` track) and
`parser-large` (parser scanner/allocation — note the `qw1.6` attribution already
rejected scanner prefiltering, so future parser wins must come from elsewhere).
Use `scripts/perf-compare.sh` to confirm any future before/after.
