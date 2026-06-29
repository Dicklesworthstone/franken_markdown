# qw1.7.4 — certified Knuth-Plass active-list pruning (DEFERRED / REJECTED NOW)

**Decision: do not implement now** — the bead's precondition is unmet and the
acceptance bar ("performance gains clear same-host noise") cannot be reached on
the current non-hot path. The certified-pruning *design* is preserved for if
line breaking re-enters the top-2.

## Precondition check (unmet)

The bead triggers "**if** Knuth-Plass line breaking becomes top-5 again"; the
qw1.7 re-profile DECISION sharpens this to "if line-breaking re-enters the
**top-2**". Current ranking:

| Rank | Scenario | p95 |
|---|---|---|
| 1 | pdf-large (render-pdf) | 121.3 ms |
| 2 | parser-large (parse) | 43.1 ms |
| 3 | hyphen-corpus (hyphenation) | 27.5 ms |
| 4 | **paragraph-1k (line-break)** | **7.5 ms** |

`paragraph-1k` is **7.5 ms** (rank 4) after the qw1.3 prefix-metric work took it
from ~592 ms to ~7.5 ms. Knuth-Plass is no longer hot.

## Why reject now (evidence)

- Active-list pruning is an inner-loop micro-optimization on the O(breakpoints²)
  candidate scan. On a 7.5 ms scenario any honest gain would sit **inside
  same-host noise** — failing the bead's own acceptance bar.
- The optimization is correctness-sensitive (it must provably preserve the
  optimal break sequence and deterministic tie-breaking) — non-trivial proof
  burden for no measurable win.
- Per AGENTS.md's "attribute before optimizing" discipline and the re-profile
  gate, optimizing a non-hot path is benchmark theater.

## Preserved design (for if line-breaking re-enters top-2)

Demerit-dominance pruning + feasible-ratio windows + monotonic lower bounds, with
an **exact scalar fallback** when proof preconditions do not hold, validated by a
certificate test comparing pruned vs exhaustive breaks over adversarial
paragraphs (byte-identical breaks, unchanged tie/flagged-penalty behavior).
