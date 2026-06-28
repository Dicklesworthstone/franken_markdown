# SIMD Island Design

Status: approval design for `qw1.5.1`
Date: 2026-06-27
Scope: optional byte-scanner acceleration only

## Decision

No SIMD implementation is approved by this document by itself.

This document defines the only SIMD island that may be implemented later, after
measurement proves the gate below. Until then, the scalar parser, HTML escaper,
PDF escaper, and line classifiers remain the production implementation.

## Approval Gate

A SIMD implementation may start only when a performance artifact following
`docs/PERFORMANCE_ARTIFACT_SCHEMA.md` proves all of the following:

- the target scanner or escaper stage is top-5 by p95 or p99 on the same host,
- the expected-value score is at least `2.0`,
- golden outputs/checksums are preserved by the scalar implementation,
- the scalar oracle and differential corpus already exist,
- the proposed island fits the module boundary below.

Current evidence does not approve production SIMD yet. Parser attribution showed
`parser-large` matters, but the scalar full-line prefilter experiment in
`qw1.6.3` was rejected: full block/inline prefiltering worsened p95 and risked
output-size drift, and reference-only prefiltering also lost. The useful result
from that bead is the scalar oracle API, not a mandate to accelerate it blindly.

## Approved Boundary

The only approved boundary is a future module shaped like:

```text
src/
  simd.rs              safe public wrappers and dispatch
  simd/
    scalar.rs          always compiled oracle implementations
    x86_64_avx2.rs     x86_64 AVX2 implementation
    aarch64_neon.rs    AArch64 NEON implementation
    wasm128.rs         optional wasm simd128 implementation
```

Allowed operations:

- `find_any_markdown_special_byte(bytes: &[u8]) -> Option<usize>`
- `find_html_escape_byte(bytes: &[u8]) -> Option<usize>`
- `find_pdf_escape_byte(bytes: &[u8]) -> Option<usize>`
- `classify_ascii_whitespace(bytes: &[u8]) -> WhitespaceScan`
- `scan_markdown_line_fast(line: &str) -> ParserLineScan`

Disallowed in the SIMD island:

- Markdown semantics beyond byte classification,
- Knuth-Plass line breaking decisions,
- PDF object ordering or numbering,
- font parsing, shaping, kerning, or subsetting,
- allocation, filesystem, process, environment, or thread access,
- any dependency crate.

## Public Safe Wrappers

Every accelerated function must be called through a safe wrapper. The wrapper
selects the implementation and returns exactly the scalar result.

Required wrapper contract:

```rust
pub fn find_html_escape_byte(bytes: &[u8]) -> Option<usize> {
    // dispatch here; callers never call unsafe/arch-specific functions directly
}
```

Wrappers must:

- compile without `simd-accel`,
- compile on `wasm32-unknown-unknown` without native assumptions,
- call scalar fallback for empty, tiny, non-ASCII, unaligned, or unsupported CPU
  cases,
- never require `RUSTFLAGS="-C target-cpu=native"` for release binaries,
- preserve first-match/tie-break behavior exactly.

## Scalar Oracle

Scalar code is the specification.

Existing oracle:

- `scan_markdown_line(line: &str) -> ParserLineScan`

Future scalar oracle functions must live next to the SIMD island and be used by
differential tests. SIMD implementations are not allowed to define semantics.

The scalar oracle must be:

- allocation-free for scanner operations,
- deterministic,
- UTF-8 boundary safe,
- total for arbitrary byte slices where the API accepts bytes,
- documented when it is conservative rather than exact.

## CPU Strategy

### x86_64 AVX2

Use runtime dispatch:

```rust
if std::is_x86_feature_detected!("avx2") {
    // call AVX2 implementation
} else {
    // scalar
}
```

Rules:

- AVX2 is the first x86_64 target.
- SSE2 may be added only as a separate measured fallback if it clears the same
  gate.
- AVX-512 is not enabled by default. It needs separate hardware proof because it
  can downclock some CPUs and has uneven fleet availability.
- Do not require `target-cpu=native`.
- Scalar tails must handle every byte after the last full vector.

### AArch64 NEON

Use NEON only under `target_arch = "aarch64"` and only after proof on Apple
Silicon or representative AArch64 hardware.

Rules:

- 128-bit chunks are the default.
- Tail handling is scalar and shared with the oracle.
- Do not assume homogeneous core performance or thermal behavior.
- No Apple-specific claim is allowed without an Apple Silicon artifact.

### WASM `simd128`

WASM SIMD is optional and separate from native SIMD.

Rules:

- the core must keep passing `scripts/check-wasm-core.sh`,
- `wasm32-unknown-unknown` scalar builds remain the default,
- any `simd128` path is feature-gated,
- browser claims require browser/WASM differential proof,
- no filesystem, native threads, process APIs, fontconfig, or system fonts.

## Feature Flags

Future `Cargo.toml` shape:

```toml
[features]
default = ["cli"]
cli = ["dep:clap"]
simd-accel = []
wasm-simd128 = []
```

`simd-accel` must not be part of default features until it has passed the full
gate on at least x86_64 AVX2 and AArch64 NEON, and even then scalar remains the
fallback.

## Safety Template

Every unsafe block in the SIMD island must use this exact comment structure:

```rust
// SAFETY:
// - CPU feature gate: <how this function is reachable only with feature X>.
// - Pointer provenance: <where pointers come from and why they remain valid>.
// - Bounds: <why every vector load is in-bounds, or why unaligned load is valid>.
// - Alignment: <which intrinsic accepts unaligned input, or how alignment is guaranteed>.
// - Tail: <how remaining bytes are handled by scalar code>.
// - Equivalence: <which scalar oracle test proves identical result ordering>.
```

Unsafe code outside the SIMD island remains forbidden.

## Differential Test Matrix

Every SIMD operation must compare SIMD output to scalar output for:

| Class | Required cases |
|---|---|
| size | empty, 1 byte, 2 bytes, 15, 16, 17, 31, 32, 33, 63, 64, 65, 1 MiB |
| alignment | all offsets from 0 through 31 into an overallocated buffer |
| position | no match, match at first byte, last byte, vector boundary, tail |
| bytes | ASCII prose, Markdown dense punctuation, HTML escape chars, PDF escape chars |
| UTF-8 | multibyte text before/after matches, invalid bytes for byte APIs |
| adversarial | all special bytes, all non-special bytes, alternating patterns |
| corpus | parser differential fixtures, generated parser-large source, HTML/PDF golden inputs |
| platform | scalar-only, x86_64 AVX2 when available, AArch64 NEON when available, wasm simd128 when enabled |

The tests must fail with enough context to identify the byte offset, platform
path, and corpus case.

## Failure Modes

Reviewers must reject an implementation if any of these occur:

- first matching byte differs from scalar,
- non-ASCII text changes behavior,
- an unsafe block lacks the safety template,
- SIMD is reachable without runtime CPU feature detection,
- release builds require `target-cpu=native`,
- AVX-512 is added to default dispatch,
- `wasm32-unknown-unknown` no-default builds fail,
- golden HTML/PDF/parser outputs change,
- p95 speedup is inside the same-host variance envelope,
- implementation pulls in dependency crates,
- module boundary expands into semantic parsing or layout.

## Expected Speedup

Do not claim a speedup until artifacts prove it.

Initial expectation, if the gate is met:

- byte-scanner functions over large buffers: 1.5x to 4x local function speedup,
- end-to-end parser or renderer speedup: only meaningful if the scanner stage is
  top-5 and not hidden by allocation/string construction,
- tiny inputs: often no win; scalar fallback may be faster.

The correct outcome can be "reject SIMD for now." That is preferable to adding
unsafe code that does not move end-to-end p95.

## Implementation Checklist

Before coding:

- open/claim the implementation bead,
- reserve the SIMD module, scalar oracle, tests, and `.beads/issues.jsonl`,
- cite the artifact proving the approval gate,
- confirm `unsafe` island approval in the bead.

During coding:

- implement scalar oracle first,
- add differential tests before architecture code,
- add one architecture path at a time,
- keep every public function safe,
- keep all accelerated paths optional.

Before close:

- `cargo fmt --check`,
- `cargo check --all-targets`,
- `cargo clippy --all-targets -- -D warnings`,
- `cargo test`,
- `scripts/check-policy.sh`,
- `scripts/check-wasm-core.sh`,
- `scripts/parser-diff.sh`,
- `scripts/check-determinism.sh`,
- performance artifact with golden checksums and hypothesis result,
- `br dep cycles`.

