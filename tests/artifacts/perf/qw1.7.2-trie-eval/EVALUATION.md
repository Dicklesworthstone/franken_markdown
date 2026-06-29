# qw1.7.2 — compact hyphen trie evaluation (REJECTED)

**Decision: reject** — the candidate's gains are within noise at the current
ranking, which the bead specifies as the close-as-rejected condition.

## What the current trie already is

`HyphenTrie` (`src/layout.rs`) is **already a compact, flattened trie**, not a
pointer-chasing node graph:

- `nodes: Vec<HyphenTrieNode>` where each node is `{ first_edge: u32,
  edge_count: u16, values_start: u32, values_len: u8 }` — a CSR/struct-of-arrays
  layout.
- `edges: Vec<HyphenTrieEdge>` — one flat, contiguous edge array indexed by
  `first_edge .. first_edge + edge_count` (the bead's "denser edge arrays"
  candidate, already done).
- `values: Vec<u8>` — one flat values pool indexed by `values_start/len`.
- Built once per process via `OnceLock`; 4938 TeX pattern tokens, 31 KB of
  pattern data.

A double-array trie would be a further constant-factor memory/locality tweak on
top of this already-flat layout.

## Why reject (evidence)

- **Not first-order.** The qw1.7 re-profile ranks `hyphen-corpus` rank 3
  (27.5 ms p95 over a 50,000-word synthetic stress corpus ≈ 0.55 µs/word); `pdf-large`
  and `parser-large` dominate p95 by 2–4×. Real documents have far fewer words,
  so per-document hyphenation is small.
- **qw1.7.1 already cut the repeated-word cost** with a per-document cache, so the
  trie is hit even less on realistic inputs.
- **Cost/benefit.** A double-array trie adds build complexity and a correctness
  proof burden (byte-identical hyphen ledger) for a memory/locality micro-win
  with no measurable p95 effect — exactly the "reject if within noise" case.

Revisit only if a realistic workload pushes hyphenation into the top-2 and the
trie traversal (not pattern count) is shown to be the bottleneck.
