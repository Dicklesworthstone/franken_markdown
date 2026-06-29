# qw1.7.1 — bounded per-document hyphenation cache

A render-call-local `word -> hyphenation-points` cache in the PDF layout path
(`src/pdf.rs`, `flush_pdf_word`), keyed by the lowercase ASCII word, bounded at
`HYPHEN_CACHE_MAX = 16384` distinct words, dropped with the `layout()` call.
A cache hit returns exactly what the hyphenator would compute, so output is
**byte-identical**. Only words that actually hyphenate are inserted, so
non-hyphenating words pay no cache cost.

## Measured (release, same host, `fmd <doc> --to pdf`)

| corpus | without cache | with cache | delta | output |
|---|---|---|---|---|
| repeated-word (14400 words, 7 distinct long hyphenating words) | ~583 ms | ~564 ms | **-3.3%** (beyond ~15 ms noise) | byte-identical |
| realistic (README+showcase ×12, 59412 words, 424 KB) | ~492 ms | ~488 ms | -0.8% (neutral) | byte-identical |
| pathological all-distinct (14400 distinct hyphenating pseudo-words) | ~158 ms | ~167 ms | +5.7% (acknowledged worst case) | byte-identical |

## Reading

- **Repeated-word corpora improve beyond noise** (the bead's target): -3.3%, and
  the improvement scales with how heavily long words repeat.
- **Realistic documents do not regress**: natural vocabulary repetition makes the
  cache neutral-to-slightly-positive on a 424 KB real document.
- The only regression is on a *pathological* corpus where every word is both
  distinct and hyphenating — no real document looks like this (Zipfian
  vocabulary). The per-word insert there is unrepaid; "only cache hyphenating
  words" already removes the cost for the common non-hyphenating words.
- **Correctness:** output PDF bytes are identical with and without the cache on
  all three corpora (verified with `cmp`); the render-tree golden, the full
  hyphenation ledger (layout tests), and `check-determinism.sh` are unchanged.

Method: 3–5 timed runs per cell; `git stash` of `src/pdf.rs` toggles the cache so
both binaries are otherwise identical.
