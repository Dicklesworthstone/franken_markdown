# Performance Acceleration Plan

Date: 2026-06-27
Status: proposed implementation slate for the next optimization wave
Scope: text rendering, line breaking, parser scanning, PDF writing, native batch
parallelism, SIMD/AVX/NEON, and WASM portability

## Objective

Make `franken_markdown` hyper-optimized without violating the project contract:

- clean-room Rust core,
- no heavy dependency forests,
- deterministic bytes for fixed input/theme/fonts/options,
- first-class `wasm32-unknown-unknown`,
- no native runtime assumptions in the core,
- no `unsafe` unless a future explicitly approved SIMD/font island has scalar
  fallback, safety docs, and differential proof.

The central rule remains: profile first, optimize one lever at a time, prove
behavior unchanged, then re-profile because the bottleneck will move.

## Evidence Snapshot

Current-head in-process harness, 20 iterations, release-perf profile with frame
pointers, run manually on 2026-06-27:

| Rank | Scenario | Category | p95 | Mean | Notes |
|---:|---|---|---:|---:|---|
| 1 | `pdf-large` | render-pdf | 61.169 ms | 60.158 ms | pre-parsed large mixed Markdown document to PDF |
| 2 | `parser-large` | parse | 51.460 ms | 34.703 ms | generated 1 MiB CommonMark/GFM-like document |
| 3 | `hyphen-corpus` | hyphenation | 28.689 ms | 28.539 ms | Liang/TeX hyphenation over 50k words |
| 4 | `paragraph-1k` | line-break | 10.983 ms | 10.941 ms | Knuth-Plass baseline over 1000 generated words |
| 5 | `pdf-showcase` | render-pdf | 4.917 ms | 3.916 ms | showcase PDF |
| 6 | `html-showcase` | render-html | 0.095 ms | 0.066 ms | showcase HTML |
| 7 | `font-subset` | font-subset | 0.033 ms | 0.022 ms | bundled body font subset |

Focused `perf stat` on `pdf-large` over three runs, with temporary Linux PMU
knobs opened and then put back:

```text
cycles:            679,198,142
instructions:    2,445,209,682
IPC:                       3.60
branches:          543,520,870
branch-misses:       3,181,613  (0.59%)
cache-references:    9,199,036
cache-misses:          926,576  (10.07% of cache refs)
elapsed:                 0.257s
```

Interpretation:

- `pdf-large` is the immediate P0. It has high IPC and low branch misses, so
  first wins should come from doing less work: fewer `format!` calls, fewer
  temporary `String`/`Vec` allocations, less repeated shaping, fewer BTree
  lookups, pre-sized PDF buffers, and faster deterministic number/hex writers.
- `parser-large` is the next scanner/allocation target. SIMD should be gated on
  attribution proving byte scanning or escaping is a top-5 cost inside parser or
  PDF/HTML serialization, not merely because SIMD sounds attractive.
- `hyphen-corpus` and `paragraph-1k` already moved down after the prefix-sum and
  trie work. They remain important, but they are no longer first in line.

## EV Matrix

Only implement rows with score at least 2.0, and re-score after each measured
change.

| Candidate | Impact | Confidence | Effort | Score | First bead |
|---|---:|---:|---:|---:|---|
| PDF serializer/shaping fast path: buffer sizing, fast decimal/hex writers, shaped-run cache, subset-map layout | 5 | 4 | 2 | 10.0 | `fep.6` |
| Asupersync batch renderer: file-level parallelism, deterministic receipts, queueing budgets | 5 | 5 | 3 | 8.3 | `zmd.1` |
| Parser scanner attribution and allocation reduction | 4 | 4 | 2 | 8.0 | new child under gauntlet/parser |
| PDF stage instrumentation: split layout/subset/ToUnicode/serialize timings | 4 | 5 | 3 | 6.7 | new child under `fep.6` |
| Hyphen word-result cache or trie layout compaction | 2 | 4 | 2 | 4.0 | future child after profile |
| SIMD special-byte scanner island | 4 | 3 | 4 | 3.0 | `qw1.5` |
| Active-list/page-builder parallelism inside one document | 3 | 2 | 4 | 1.5 | defer |
| AVX-512-specific path | 2 | 2 | 5 | 0.8 | reject until separate hardware proof |

## Architecture

The acceleration architecture should have four layers.

### L0: Scalar Specification

Every hot algorithm keeps a scalar, dependency-free implementation that is the
behavioral oracle:

- parser scanners,
- HTML/PDF escaping,
- PDF hex/string writers,
- hyphenation trie traversal,
- Knuth-Plass paragraph breaking,
- glyph shaping and subset mapping.

Scalar code is always compiled. SIMD/native acceleration is optional and must be
byte-equivalent against the scalar oracle.

### L1: Data Layout and Allocation Discipline

Before SIMD, make the scalar path cache-friendly and allocation-light:

- replace repeated `format!` in PDF hot loops with append-only writers over
  `Vec<u8>`,
- pre-size PDF object/content buffers from page count, segment count, and font
  subset sizes,
- keep per-render scratch buffers for shaped glyph ids, hex output, ToUnicode
  rows, and object headers,
- avoid `String` per word/token where a source span or run arena will do,
- avoid BTree lookups in tight loops once deterministic maps can be frozen into
  sorted vectors with binary search or dense glyph-index arrays,
- keep worker-local scratch in native batch mode to avoid false sharing.

### L2: Native Multicore Outside the Core

The pure render core stays synchronous and embeddable. Native parallelism belongs
in CLI/batch orchestration:

- Asupersync owns batch rendering of many input files.
- The render core exposes deterministic sync functions.
- Job receipts are sorted by stable input ordinal/path before reporting.
- Cancellation and budget checks happen at batch boundaries and at explicit
  long-loop checkpoints.
- Native worker count is a policy decision, never a core assumption.

The first multicore win should be file-level parallelism. It has almost perfect
independence, clean cancellation semantics, and no PDF page-order coupling.

Document-internal parallelism is later and narrower:

- independent code-block highlighting,
- independent paragraph shaping/item construction,
- per-face glyph collection into worker-local sets, merged deterministically,
- per-page content pre-serialization after page breaks are fixed.

Do not parallelize the page builder until it has partition certificates showing
that page decisions cannot be changed by neighboring partitions.

### L3: Approved SIMD Island

SIMD belongs in a narrow `simd-accel` feature, only after `qw1.5` is approved.
It should accelerate byte-level scanners, not semantic layout decisions:

- find Markdown special bytes: newline, backtick, asterisk, underscore, bracket,
  paren, angle bracket, pipe, backslash, ampersand, hash, dash, plus, digit,
- find HTML escape candidates: `&`, `<`, `>`, `"`,
- find PDF string/hex escaping candidates,
- classify ASCII whitespace and line endings,
- scan table/fence candidate bytes before scalar state validation.

Do not start with SIMD inside Knuth-Plass DP. That path is branchy, stateful,
and already below PDF/parser in measured cost. SIMD only makes sense there after
the future line/page engine becomes top-5 again.

## CPU-Specific Strategy

### Apple Silicon

- Use AArch64 NEON as the native vector path when the SIMD island is approved.
- Prefer 128-bit chunking; keep scalar tail handling identical to the oracle.
- Avoid extra copies: Apple Silicon has excellent bandwidth, but pointer-heavy
  data structures still lose to compact arrays.
- Let Asupersync native policy cap workers from `available_parallelism` and
  measured service time. Do not assume all cores have identical power/thermal
  behavior.
- Treat `wasm32` separately. Browser WASM cannot assume native threads or host
  font discovery.

### Intel/AMD x86_64

- Use runtime dispatch, not `target-cpu=native`, for release binaries.
- AVX2 is the likely first high-EV path for byte scanners.
- SSE2 can be a low-risk baseline if AVX2 is absent.
- AVX-512 is not a default target. It can downclock some machines and is not
  uniformly available across Intel/AMD fleets. Add it only after per-machine
  evidence clears the variance envelope.
- Keep scalar fallback and runtime detection:
  `is_x86_feature_detected!("avx2")` before entering AVX2 code.

### WASM

- The core must keep building with `cargo build --no-default-features`.
- Browser API should expose the same scalar core first.
- Optional `simd128` can be a separate WASM feature once scalar/SIMD
  differential tests exist.
- No filesystem, process, native threads, fontconfig, or system-font discovery.
  Fonts are bundled bytes or caller-provided bytes.

## Queueing and Worker Budgets

Batch rendering should use queueing theory explicitly.

Define:

```text
mu = measured jobs/second per worker
lambda = offered jobs/second
c = worker count
rho = lambda / (c * mu)
```

Policy:

- interactive/agent mode: keep `rho <= 0.70`,
- throughput mode: keep `rho <= 0.85`,
- if service-time coefficient of variation exceeds 1.5, lower utilization and
  use smaller work chunks,
- always bound queue length and memory budget.

Native Asupersync artifacts:

- capacity table for `c = 1..available_parallelism`,
- deterministic receipt ordering,
- cancellation automaton with cleanup proof,
- budget exhaustion test with partial outputs cleaned or marked incomplete,
- no Asupersync in the pure render core.

## Compositional Latency Algebra

Every perf run should emit stage timings so total latency can be bounded as:

```text
T_total <= T_parse + T_highlight + T_shape + T_linebreak
         + T_paginate + T_subset + T_serialize + coupling_margin
```

For batch mode:

```text
T_batch_p95 <= max(worker_stage_p95) + queue_wait_p95 + merge_p95
```

The plan should optimize the largest certified stage first. When a stage drops
below the same-host variance envelope, stop polishing it and re-profile.

## Implementation Waves

### Wave 1: Attribution and PDF Fast Path

1. Add profiling-only stage attribution for `pdf-large`:
   layout, used-slot scan, glyph collection, shaping, subsetting, ToUnicode,
   page stream generation, object serialization, xref/trailer.
2. Preserve golden PDFs and determinism checks.
3. Replace hot `format!`/`String` loops with deterministic append writers:
   decimal writer, fixed-precision point writer, hex u16 writer, xref offset
   writer.
4. Pre-size buffers from measured counts.
5. Cache shaped segment results per face/text within one render.
6. Re-profile and only then choose the next lever.

Expected first target: `src/pdf.rs` functions `serialize`, `build_pdf`,
`kerned_tj`, `widths_array`, and `tounicode_cmap`.

### Wave 2: Parser Scanner and Allocation Work

1. Attribute `parser-large` by block scanner, inline parser, table parser, link
   reference collection, span/diagnostic paths, and allocation counts.
2. Replace repeated full-line checks with a single classified scan where it
   preserves CommonMark/GFM behavior.
3. Prefer borrowed spans and compact token/range structures in hot paths.
4. Add differential parser fixtures before each rewrite.
5. Re-score SIMD only if special-byte scanning remains top-5.

### Wave 3: Asupersync Batch Parallelism

1. Add native-only `fmd batch` or equivalent render orchestration.
2. Use `&Cx` first in owned async APIs.
3. Keep render functions synchronous and WASM-clean.
4. Use bounded worker pools and deterministic receipt order.
5. Add cancellation, budget, and cleanup tests with deterministic lab runtime.
6. Add batch throughput scenarios to the gauntlet.

### Wave 4: SIMD Island Design and Approval

1. Write the SIMD design doc before code.
2. Implement scalar scanner corpus and random/metamorphic tests.
3. Define exact APIs:
   `find_any_special_byte`, `find_html_escape`, `find_pdf_escape`,
   `classify_ascii_whitespace`.
4. Implement x86_64 AVX2, AArch64 NEON, optional wasm simd128.
5. Keep unsafe code isolated behind one module, one feature, and one public
   safe wrapper per operation.
6. Run scalar/SIMD differential tests across aligned, unaligned, tiny, empty,
   non-ASCII, and adversarial inputs.

### Wave 5: Text/Layout Specialization

1. If line breaking becomes top-5 again, optimize active-list representation:
   compact arrays, prefix sums, monotonic pruning, and deterministic tie-breaks.
2. Add a per-paragraph scratch arena owned by the render call.
3. Cache hyphenation points for repeated words within a document.
4. Consider trie compaction only if hyphen traversal remains top-5 after cache.
5. Add microtypography cost terms only with golden visual fixtures and no
   floating-point nondeterminism in decisions.

## Proof Obligations

Every optimization commit needs an isomorphism note covering:

- ordering preserved,
- tie-breaking preserved,
- floating-point decisions unchanged or moved to fixed-point,
- scalar fallback preserved,
- golden checksums preserved,
- determinism script green,
- WASM/no-default script green when core code changes,
- same-host p95 change exceeds the variance envelope before claiming speedup.

Every SIMD commit additionally needs:

- scalar oracle test corpus,
- x86_64 AVX2 differential proof when supported,
- AArch64 NEON proof on Apple Silicon before claiming Apple gains,
- wasm simd128 proof before browser acceleration claims,
- safety comment for each unsafe block,
- runtime dispatch proof and fallback proof.

## Bead Mapping

- `fep.6`: Wave 1 PDF fast path and attribution.
- `zmd.1`: Wave 3 Asupersync batch renderer with queueing budgets.
- `qw1.5`: Wave 4 SIMD island design and scalar proof.
- Future parser child: Wave 2 parser scanner attribution and allocation work.
- Future layout child: Wave 5 only after line breaking/page layout returns to
  top-5 by measured p95 or p99.

## Immediate Next Move

Start with `fep.6`, but do not optimize blind. The first implementation step is
stage-level PDF attribution with golden output preservation. The current
`perf stat` result says CPU throughput is already high; the dominant likely
waste is repeated work and formatting/allocation overhead, not branch prediction.
