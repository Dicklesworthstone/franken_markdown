# Performance Optimization Plan

Date: 2026-06-27
Status: proposed measurement-first roadmap
Owner: franken_markdown agents

## Goal

Make Markdown parsing, text shaping, Knuth-Plass line breaking, hyphenation,
HTML escaping, PDF serialization, and native batch rendering extremely fast on:

- Apple Silicon ARM64,
- Intel/AMD x86_64 CPUs,
- browser/WASM using the same safe scalar core by default.

The project rule is strict: no optimization lands without a baseline, a ranked
hotspot table, golden-output proof, and one-lever-at-a-time verification.

## Current Facts

- `release-perf` already exists in `Cargo.toml` with debug line tables and
  unstripped symbols.
- Local profiling tools exist on the current host: `hyperfine`, `samply`,
  `cargo-flamegraph`, and `perf`.
- Current host evidence: AMD Threadripper PRO 5995WX, 64 physical cores, 128
  threads, AVX2 available, AVX-512 not available.
- Core/no-default build is dependency-free and WASM-clean. Keep that property.
- `unsafe_code = "forbid"` is repo policy. SIMD requires a future explicitly
  approved, isolated unsafe island with scalar fallback and safety docs.

## Non-Negotiable Workflow

1. Define scenario, metric, budget, golden output, and scope boundary.
2. Capture `tests/artifacts/perf/<run-id>/fingerprint.json`.
3. Capture baseline: p50/p95/p99, throughput, peak RSS, and golden checksums.
4. Profile CPU, allocation, and cache behavior.
5. Produce `hotspot_table.md`, `hypothesis.md`, and `scaling_law.md`.
6. Score opportunities as `(Impact * Confidence) / Effort`; implement only
   score >= 2.0.
7. Apply exactly one lever per commit.
8. Re-run golden outputs, determinism, WASM, policy, and performance comparison.

No "it should be faster" commits.

## Baseline Scenarios

Create these perf scenarios before optimizing:

| Scenario | Command shape | Primary metric | Golden output |
|---|---|---:|---|
| `html-showcase` | `fmd examples/showcase.md --out out.html` | p95 wall time | SHA256 HTML |
| `pdf-showcase` | `fmd examples/showcase.md --to pdf --out out.pdf` | p95 wall time, bytes | SHA256 PDF |
| `parser-large` | render generated 1 MiB Markdown to HTML | MB/s | article-body snapshot |
| `paragraph-1k` | line-break 1000-word paragraph | ns/word | line-break vector |
| `hyphen-corpus` | hyphenate 50k English words | words/s | points ledger |
| `font-subset` | subset bundled body/mono fonts | p95, peak RSS | subset SHA256 |
| `pdf-large` | 100-page mixed Markdown to PDF | p95, RSS, file bytes | SHA256 PDF |
| `batch-100` | render 100 files via native CLI | throughput, p95/file | receipt ledger |

Artifacts live under `tests/artifacts/perf/<run-id>/`. Do not overwrite runs.

## Likely Hotspot Hypotheses

These are hypotheses, not permission to edit without profiling.

| Rank | Suspected area | Evidence to collect | Likely lever |
|---:|---|---|---|
| 1 | Hyphenation | CPU profile around `hyphenation_points` | compact trie/automaton |
| 2 | Paragraph DP | flame + instruction count around `break_paragraph` | prefix sums + active list |
| 3 | Inline parser | alloc profile around `Vec<char>` conversion | byte scanner + ASCII fast path |
| 4 | HTML escaping | CPU profile around `escape_text`/attrs | chunked scanner + SIMD island later |
| 5 | PDF shaping/serialization | profile `shape`, `kerned_tj`, `tounicode_cmap` | glyph-run cache, custom writers |
| 6 | Font subsetting | profile BTree maps/sets and table assembly | sorted Vec remap tables |
| 7 | Batch rendering | throughput scaling vs workers | Asupersync bounded worker plan |

## Text And Line-Break Algorithm Strategy

### Paragraph DP

The current paragraph breaker is a correctness baseline. It pairs legal break
candidates and calls `segment_metrics` for each pair, so cost can grow
unacceptably on long paragraphs.

First optimization after profiling:

- precompute prefix sums for width, stretch, and shrink,
- answer segment metrics in O(1),
- preserve exact demerits and exact tie behavior,
- keep the greedy emergency fallback unchanged.

Second optimization only if still hot:

- TeX-style active list keyed by feasible break range,
- dominance pruning only when a certificate proves one state cannot beat
  another under the future cost lower bound,
- store a `LineBreakCertificate` in tests for representative paragraphs.

Do not assume Monge/Knuth/SMAWK optimization globally. Hyphen penalties,
flagged-break penalties, and fitness-class penalties can violate quadrangle
inequality. If a restricted cost matrix is ever optimized with Monge/SMAWK,
the implementation must emit a certificate proving the condition for that run
and fall back to exact DP when it fails.

### Hyphenation

Current TeX pattern handling checks every encoded pattern for every word. That
is simple and correct but likely too slow for large documents.

High-EV replacement:

- generate a committed compact trie or double-array trie from
  `data/hyph-en-us.patterns`,
- scan each word position through the trie,
- apply matched value vectors directly,
- preserve exact points for the existing TeX corpus tests,
- add a corpus ledger with word -> points golden outputs.

No build script is needed. A checked-in generator can refresh the static table,
but the runtime core should just `include!`/`include_bytes!` committed data.

### Shaping And Metrics

Use data-oriented shaped runs:

```text
text bytes -> char/glyph map -> glyph ids -> advances -> pair adjustments
```

Store hot shaped data as parallel arrays where profiling proves it matters:

- `gids: Vec<u16>`,
- `advances: Vec<i32>`,
- `x_adjust: Vec<i16>` or fixed-point layout units,
- source byte ranges for ToUnicode/copy behavior.

Keep styled Markdown semantics separate from glyph-level layout. The AST and
styled runs should remain readable; the shaped-run cache is a performance layer.

## Multicore Strategy

### Native CLI And Batch

Use Asupersync outside the render core for native orchestration:

- file-level parallelism first: independent Markdown files render concurrently,
- bounded worker count from `std::thread::available_parallelism()`,
- cancellation/budget checkpoints between files and major render stages,
- deterministic receipt ordering by input path/index,
- no filesystem/thread assumptions in core or WASM.

Queueing model:

```text
rho = lambda / (c * mu)
target rho <= 0.70 for latency-sensitive interactive CLI
target rho <= 0.85 for throughput-oriented batch mode
```

Worker-count policy should start conservative:

```text
c = min(available_parallelism, input_count, user_limit_or_infinity)
```

Then tune from measured service time variance. High variance (`C_s > 1.5`)
means lower utilization and smaller per-worker queues.

### Single Large Document

Parallelize only deterministic independent stages:

- parse remains mostly serial until profiling proves otherwise,
- syntax-highlight code blocks independently,
- hyphenate and shape paragraphs independently,
- line-break paragraphs independently once page-width/style inputs are fixed,
- subset each used font face independently,
- serialize final PDF objects in stable object-number order.

Page building stays serial until there is a proof-friendly parallel page
partitioner. Pagination is full of keep-with-next, widow/orphan, table headers,
and footnotes; naive parallel pagination will produce bad documents.

## SIMD Strategy

SIMD should accelerate scanners, not semantics. The scalar parser remains the
source of truth.

Best targets:

- find HTML text escapes: `&`, `<`, `>`,
- find HTML attr escapes: `&`, `<`, `>`, `"`,
- find Markdown inline trigger bytes: `\\`, '`', '!', '[', '<', '&', '~', '*',
  '_', newline,
- classify ASCII whitespace and punctuation,
- scan table pipes and escaped pipes,
- scan ASCII words for hyphenation eligibility.

Architecture plan:

| Target | Default | Optional fast path |
|---|---|---|
| x86_64 | scalar + SSE2-compatible assumptions | AVX2 runtime dispatch |
| Apple Silicon/AArch64 | scalar | NEON runtime/static dispatch |
| wasm32 | scalar | optional `simd128` build target |

Implementation rules:

- no SIMD until profiling shows scanner cost is top-5,
- keep SIMD behind an explicit feature such as `simd-accel`,
- isolate all `unsafe` in `src/simd/` with a scalar equivalent,
- one public safe function per scanner,
- run differential tests comparing scalar and SIMD on generated edge cases,
- runtime-dispatch once and cache a function pointer,
- never require `target-cpu=native` for release binaries.

Avoid AVX-512 as a default target. Many machines do not support it, downclocking
can erase wins, and the current reference x86 host has AVX2 but not AVX-512.

## Apple Silicon Notes

- NEON is 128-bit. Design vector kernels around 16-byte lanes and bitmasks.
- Avoid x86 assumptions about 32-byte vectors.
- Keep hot arrays compact and sequential; Apple cores reward predictable memory
  access and branch-light loops.
- Do not rely on Apple AMX or private matrix units for text rendering.
- Browser/WASM on Apple still goes through the WASM path, not native NEON.

## Intel/AMD Notes

- SSE2 is baseline on x86_64, but AVX2 is the practical high-value target.
- Use `is_x86_feature_detected!("avx2")` for native dispatch.
- Keep scalar fallback identical and testable.
- Measure AVX2 against scalar on the same host; some scanner loops are memory
  bound and will not improve enough to justify complexity.
- Use `perf stat` for cycles, instructions, branch misses, cache misses, and
  IPC before claiming a SIMD win.

## WASM Notes

WASM remains first-class:

- default `wasm32-unknown-unknown` build stays scalar and dependency-free,
- SIMD is optional via a separate `simd128` build/profile,
- no threads in the core API,
- browser worker parallelism is a wrapper concern, not render-core behavior,
- fonts are bytes passed in or bundled; no system font discovery.

## Alien-Artifact Layer

### Certified Rewrite Pipeline

For SIMD scanners and arithmetic rewrites:

- scalar implementation is the specification,
- SIMD implementation emits the same match positions/escaped output,
- test corpus includes exhaustive small byte strings where feasible plus
  generated long strings,
- counterexamples are retained as fixtures,
- promotion requires golden outputs and perf delta.

### Latency Algebra

Treat rendering as a staged pipeline:

```text
T_total <= T_parse + T_highlight + T_shape + T_linebreak + T_paginate + T_pdf
```

Each perf run should record per-stage envelopes. If batch mode is added, compose
stage budgets with queueing wait time:

```text
T_batch_p95 ~= W_queue_p95 + sum(stage_p95)
```

### Queueing Certificate

Native batch mode should report:

- worker count,
- observed throughput,
- mean service time,
- coefficient of variation,
- estimated utilization,
- queue depth high-water mark,
- cancellation cleanup proof.

### Convex/Optimization Guard

Use convex allocation only where it compiles to simple runtime policy. Example:
allocate worker shares across file-level and document-level queues by minimizing
max expected latency under a fixed core budget. The runtime artifact should be a
small policy table, not a solver dependency.

## Opportunity Matrix

| Candidate | Impact | Confidence | Effort | Score | Gate |
|---|---:|---:|---:|---:|---|
| Perf gauntlet + artifact ledger | 5 | 5 | 2 | 12.5 | do first |
| Prefix sums for paragraph metrics | 5 | 4 | 2 | 10.0 | profile DP top-5 |
| Hyphenation trie/automaton | 5 | 4 | 3 | 6.7 | profile hyphenation top-5 |
| PDF `ToUnicode`/hex writer cleanup | 4 | 4 | 2 | 8.0 | profile PDF top-5 |
| Parser/HTML byte scanner | 4 | 4 | 3 | 5.3 | profile parser/escape top-5 |
| Asupersync batch renderer | 5 | 4 | 4 | 5.0 | after perf scenarios |
| SIMD scanner island | 4 | 3 | 5 | 2.4 | only after scalar scanner hot |
| Monge/SMAWK line-break shortcut | 3 | 2 | 5 | 1.2 | reject unless certified |

## Proof Obligations

Every optimization commit must record:

- ordering preserved or intentionally changed,
- tie-break behavior preserved,
- floating-point impact (ideally none; layout decisions stay integer),
- WASM/no-default impact,
- scalar fallback behavior,
- golden checksum proof,
- before/after perf artifact paths,
- rollback commit plan.

## Immediate Next Steps

1. Create the perf gauntlet scripts/artifacts and baseline scenarios.
2. Run baseline on `release-perf` with frame pointers.
3. Produce the first ranked hotspot table.
4. Optimize only the highest-scoring top-5 hotspot.
5. Repeat until the bottleneck shifts to non-core work or deltas fall below the
   10-20% meaningful-improvement threshold.
