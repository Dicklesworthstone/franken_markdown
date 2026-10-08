# Dependency upgrade log

## 2026-10-08 release qualification

The starting source is `856441ac78f4aabee9663f9f7e27e7eab90d6790`.
Release qualification is in progress. No version bump, tag, publication, or
complete dependency-upgrade claim is justified by the current evidence.

### Dependency inventory

The crates.io API was checked for all 190 distinct registry dependencies in
`Cargo.lock` and `fuzz/Cargo.lock` on 2026-10-08. All requests succeeded. The
retained inventory is
`release_wave_2026_10_07/evidence/fmd-qualification-20261008T2200/registry-audit.json`
in the sibling release-wave workspace. Fifty-three package names have no locked
instance at their latest stable version; that includes incompatible major
versions, platform-specific dependencies, and compatible updates awaiting their
own research and tests. They have not been batch-updated.

| Dependency | Current / candidate | Status |
| --- | --- | --- |
| `wasm-bindgen` | `0.2.126` → `0.2.129` | Candidate committed as `df90e2b` in the shared tree; generated WASM and consumer qualification pending. |
| `clap` | lock `4.6.7` | Latest stable; preserved. Its Rust minimum is 1.85. |
| `asupersync` | `0.5.0` | Latest published stable; preserved. |
| `libfuzzer-sys` | fuzz lock `0.4.13` | Latest stable; preserved. Fuzz transitives still need separate qualification. |
| `fmd-font` / `fmd-math` | path `0.3.3` / `0.1.2` | First-party path dependencies preserved. |
| Rust toolchain | `nightly-2026-08-31` | Dated nightly preserved. |

Some latest transitive versions raise the Rust minimum: `aes 0.9.3` requires
1.89, `ctutils 0.4.3` requires 1.87, and most `windows-* 0.100.0` packages
require 1.95. Their adoption cannot silently preserve a claim of Rust 1.85
compatibility. Each remaining compatible update also requires its own tests.

Primary registry source: `https://crates.io/api/v1/crates/<package>`.

### Isolated wasm-bindgen candidate

Primary release notes:
<https://github.com/wasm-bindgen/wasm-bindgen/releases/tag/0.2.129>.
The release raises the library minimum from Rust 1.77 to 1.81, below this
project's declared 1.85 minimum. It fixes generated bindings, bundler exports,
CLI diagnostics, and headless runner handling. Experimental Emscripten/Tokio
and JSPI features are not enabled by this change.

The manifest and CLI installation hint are pinned to `0.2.129`. Cargo's targeted
update synchronizes the required binding family (`wasm-bindgen-macro`,
`wasm-bindgen-macro-support`, `wasm-bindgen-shared`, `js-sys`, `web-sys`, and
`wasm-bindgen-futures`). Macro support now uses the already-locked `syn 3.0.5`.
There was no Tokio entry in the starting lockfile. Cargo adds `tokio 1.53.2`
to satisfy the newly required upstream
`cfg(all(target_os = "emscripten", wasm_bindgen_unstable_tokio))` dependency.
The locked `wasm32-unknown-unknown`/`wasm-full` dependency tree contains neither
Tokio nor Asupersync; the pure render core boundary is preserved. An offline
locked Linux native CLI/batch reverse dependency tree also reports no active
Tokio path. Neither tree inspection is a runtime CLI test.

Official prebuilt CLI archives were retained on external NVMe and verified
against their GitHub release asset SHA-256 digests:

| CLI | Platform | SHA-256 |
| --- | --- | --- |
| `0.2.126` | aarch64-apple-darwin | `7df536babe345deb68828148dbdc71179118afdab42d83547c7cebfbf1426bd5` |
| `0.2.129` | aarch64-apple-darwin | `81d4a23d56b3c3eb8187658329116d50e0b228a93b343825fb71f70179051cd1` |

### Validation and remaining release gates

- The pinned baseline passed strict RCH on `hz3`, using
  `cargo test -j2 --locked --workspace --no-default-features --lib`.
  Renderer result: 1428 passed, 0 failed, 5 ignored. The workspace invocation
  also passed the font and math libraries. Its clean source tree receipt is
  `421b37b1878be1477d12c2ca16460f232b3c6251`, with no working-tree overlay.
- The pinned baseline release build for `wasm32-unknown-unknown` / `wasm-full`
  passed on the same worker, exit 0. The release profile keeps optimization
  level 3, LTO, and one codegen unit. Retained input WASM SHA-256:
  `e55ee10be1a6e763ca999d60846c67585e9e2f1e41eaef2081940debca98881e`.
- Matching prebuilt CLI `0.2.126` generated a fresh retained package under an
  OS denial of file unlink. Its processed WASM SHA-256 is
  `27da39c772b63a6bec0cf28f57c8f557b19b16ee26e79aeb5c81a80f23fc90ea`.
  Raw 7,606,287 bytes and gzip 3,341,529 bytes pass the unchanged raw 7,900,000
  and gzip 3,400,000 budgets. This measures the old dependency baseline.
- With prebuilt Node 26.11.1 and the same OS unlink denial, the genuine generated
  module passed `smoke.mjs` (showcase/probe deterministic HTML/PDF, negative
  options, multiple images, metadata), `flow_smoke`, `flow_worker_smoke`,
  `flow_outlines_smoke`, `flow_export_smoke`, and `document_worker_smoke`.
  `native_workspace_smoke` also passed, producing a deterministic retained
  standalone HTML artifact. These are generated WASM execution tests; they
  do not establish browser UI or the native Apple application's behavior.
- Candidate Rust, generated-module, deterministic HTML/PDF parity, unchanged
  size budgets, and browser consumer tests remain pending. No performance
  regression-free result has been claimed.
- The full CLI, batch, integration, and native Apple suites remain unqualified.
  Several tests and existing helpers delete scratch files; the standing
  no-deletion restriction applies. Test assertions and size budgets have not
  been weakened to make these gates pass.
- GitHub issue #15 (cold-open Markdown files and disabled File > Open) remains
  open. Source inspection alone does not establish that native behavior works.
- Existing DSR/helper scripts contain Cargo wrapper bypasses. They have not
  been executed; native build/publication remains held by the known runner
  routing issue. GitHub Actions remains disabled and unused.
- Further RCH invocations were held after the installed client was found to
  prune/reap source and delete synchronized files without a supported retain-all
  mode. The already admitted baseline finished and its artifacts were retained.
  Tracking: <https://github.com/Dicklesworthstone/remote_compilation_helper/issues/95>.

### Bounded incumbent performance and semantic comparison

The public npm `@franken-suite/franken-markdown 0.5.0` archive was downloaded
with its registry SHA-512 integrity verified. Its WASM SHA-256 is
`dd097c1160ce39388cfd264b49a3df9f54533e7e6d8e26c75d2d8c6f44cd9e50`.
The published package and the freshly built baseline ran in the **same Node
invocation** on `Mac-mini-max`: 10 warmups, 100 balanced alternating paired
samples per scenario, fixed input bytes and PDF metadata epoch. This compares
the published package with source `856441ac` on the old `0.2.126` dependency;
it does not test the `0.2.129` candidate. Npm metadata has no `gitHead`, so exact
source correspondence to the release tag has not been established.

| Corpus / operation | Incumbent p50 / p95 ns | Baseline p50 / p95 ns | p95 change | Output bytes before / after |
| --- | --- | --- | --- | --- |
| showcase HTML | 992,750 / 1,425,667 | 995,167 / 1,513,709 | **+6.18%** | 73,118 / 73,118 |
| showcase PDF | 3,647,000 / 5,003,209 | 3,761,750 / 4,824,167 | -3.58% | 58,995 / 59,736 |
| README HTML | 1,619,625 / 2,759,709 | 1,662,375 / 2,609,542 | -5.44% | 122,762 / 122,730 |
| README PDF | 44,922,125 / 55,670,000 | 45,760,541 / 52,376,792 | -5.92% | 429,064 / 431,443 |

The measured showcase HTML slowdown remains visible. A 5,000-resample paired
percentile bootstrap gives a 95% interval of -8.05% to +20.85% for that p95
change; all four intervals include zero. The host's load averages observed
afterward were 55.63 / 105.52 / 108.89. This does not establish a stable
regression or regression-free performance. Raw timings and the uncertainty
calculation are retained in the release-wave evidence directory; thresholds
and budgets were not changed.

Showcase HTML bytes are identical. Showcase PDF text extracted with Poppler is
identical, both PDFs are tagged, unencrypted three-page documents with the same
fixed metadata and page size; their object bytes differ. README HTML differences
are the intended supported raw-HTML feature: the `div align="center"` and `img`
nodes now render as sanitized markup instead of escaped text. README PDF text
likewise reflects that lowering and resulting reflow; both versions have 27
pages. This bounded semantic inspection does not replace visual goldens or
full PDF structural/accessibility validation.

No unresolved bug-type Beads were found in the canonical JSONL inventory.
The two unresolved entries are existing feature work (`ihx0`, `wplv`), preserved
without closure claims.
