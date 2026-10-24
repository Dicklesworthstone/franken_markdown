# Composable browser build profiles

The browser ABI now separates three optional product surfaces. All feature
selections below use `--no-default-features`; `wasm-full` preserves the complete
browser distribution, and native Rust renderer/editor APIs are unchanged.

| Cargo feature | Additional JavaScript exports |
| --- | --- |
| `wasm-bindgen` | Single-document HTML/PDF/SVG/EPUB and document intelligence |
| `wasm-flow` | `FmdFlowSession`: persistent editing, layout, assets and reading |
| `wasm-book` | `FmdBook`, legacy/configured book PDF, legacy/canonical book sites |
| `wasm-workspace` | `renderInteractiveHtmlConfigured`: self-hosting HTML workspace |
| `wasm-full` | All three optional surfaces |

Each optional feature enables `wasm-bindgen`, but not another optional surface.
Features can be combined, for example `--features wasm-flow,wasm-book`.
`BookRenderer`, `BookWorkspace` and `BrowserFlowSession` remain native Rust APIs.
One-shot book/workspace adapter functions also remain callable from Rust: only
their `wasm_bindgen` export attributes are conditional. This removes their
JavaScript export roots without maintaining a second rendering implementation.
The configured book PDF function is re-exported from `wasm_abi` for Rust callers.

## JavaScript compatibility

The root facade uses namespace lookup, not mandatory named imports, for the
optional book/workspace bindings. A render-only binary can therefore load the
same facade and render ordinary documents. Unsupported book/workspace calls
reject with `UNSUPPORTED_WASM_PACKAGE` and the required Cargo feature before
WASM initialization or reading/copying request payloads. The facade does not
fetch another binary, emulate a missing renderer, or fall back to a different
format. `createRenderer()` retains its methods with the same refusal behavior.

Supported legacy book packages keep their existing narrow routes and warnings.
Configured/canonical-only bindings are accepted where their ABI matches the
request. A configured PDF binding is never called with legacy arguments, nor
are advanced options silently dropped onto a legacy renderer. Workspace source
and settings are captured before asynchronous initialization, like book exports.

## Local packages and verification

Both existing full-package builders explicitly request `wasm-full`; their smoke
corpora and size budgets are retained. `scripts/dsr-wasm-profiles.sh` is the
additional DSR-host entrypoint for isolated render/full builds, create-only local
package assembly and comparison. It does not publish or delete prior artifacts.

The three package builders use Cargo's `wasm-release` profile. It inherits
`release` (LTO, one codegen unit, aborting panics and stripping), with
`opt-level = "z"` for transfer size. Native `release` keeps `opt-level = 3`.
No post-link optimizer or new dependency is required. Full packages also execute
the existing real-engine book source-update, site-publication and retained-preview
smokes against the retained package, including the independent ZIP payload/CRC
decoder and cache-update/recovery checks.

`scripts/assemble-wasm-profile.mjs` recursively closes static JavaScript imports
and re-exports without evaluating them. The render package exports the root
facade/web component; full retains the source manifest's subpaths. Dynamic
imports, workers, declarations and other resources must be declared explicitly.
Packages are private local artifacts with `-render`/`-full` name suffixes and a
SHA-256 inventory, not newly published npm releases. Input directories must be
trusted; this is not confinement against a concurrently hostile filesystem.

`wasm/compare_profiles.mjs` checks actual generated bindings and binaries,
executes flow editing on the full profile, and compares six HTML/PDF outputs.
Both raw and gzip sizes must improve relative to the same full build, while
remaining below the unchanged 7,900,000 / 3,400,000-byte full-package ceilings.
Its output is profile-vs-profile proof, not native-vs-WASM or browser/raster proof.

```sh
node --experimental-vm-modules --test wasm/browser_profile_api.test.mjs
node --experimental-vm-modules --test scripts/assemble-wasm-profile.test.mjs \
  wasm/flow_profile_probe.test.mjs wasm/compare_profiles.test.mjs
```

The 15 root-facade tests pass using the complete production JavaScript module,
the actual paper normalizer and explicit generated-binding doubles. Fourteen
fail against the previous root facade. They cover stripped, partial, legacy and
full export sets, dispatch, errors, request capture and result cleanup. These
are module-linking/API tests, not native rendering or generated-WASM evidence.
JavaScript syntax and the Cargo feature dependency closure also passed.

On 2026-10-24, a DSR/RCH experiment built the same `df772f53` full source and
feature set with release optimization levels `3` and `z`. Generated WASM changed
from 8,199,505 to 7,132,900 raw bytes and from 3,596,158 to 3,197,021 gzip bytes
(gzip mtime zero). Both original ceilings were retained. In the same invocation,
64 selected HTML/PDF/SVG/EPUB render/diagnostic pairs and 24 analysis pairs matched
exactly; capabilities matched too. The size candidate passed the existing smoke,
29 component typography checks and 25 demo download checks in actual Chrome.
This is selected equivalence and a size measurement, not a throughput claim or
universal compatibility proof. The unsuccessful Binaryen experiment was not
adopted.

At source cut `81576af5`, the scoped Rust and profile gates pass. Through
admitted DSR/RCH lanes, native check/Clippy and 4,651 default all-target tests
pass; the native
MCP/LSP/batch all-target suite passes 5,060 tests. Each suite retains 12 ignored
tests and has zero failures. Thirteen feature checks cover native/WASM core,
render/full WASM Clippy, render/full native library tests, selected browser
feature combinations, native MCP/LSP/batch Clippy/tests and doctests. Both
additional native ABI Clippy gates pass. A separate expected-negative check
verifies that the unsupported native batch/browser combination is rejected by
the actual compiler profile guard, rather than treating admission refusal as
the expected error.

At that same cut, all 24 package checks pass with matching wasm-bindgen 0.2.129.
The six-output render/full comparison measures render at 6,484,424 raw /
2,981,668 gzip bytes and full at 7,132,900 raw / 3,188,696 gzip bytes (Node zlib
level 9). All twelve real-package smokes pass, as do 54 component/demo Chrome
checks and ten actual workspace content-save/reopen/PDF checks. Another 64
render/diagnostic pairs and 24 analysis pairs match the retained prior module
exactly. Fresh Node tests pass 3,663/3,663 with zero skips. The Node workspace
loader uses a percent-encoded UTF-8 module URL with independent factory identity;
browser Blob loading is preserved. Content reopening uses virtual HTTPS documents
and does not prove local-file navigation or OS lifecycle.

The reopened PDF matches the original bytes. QPDF validates the selected PDF's
syntax/streams, Poppler and MuPDF independently recover its accented/Greek text,
and Poppler reports embedded subset fonts with Unicode mappings. The MuPDF
raster was inspected too. This single document does not qualify the complete PDF
environment-variable matrix, corpus appearance or PDF conformance.

Original failures remain recorded in Beads: release-level-3 size overflow, two
Node workspace loader failures, the batch fixture's mistaken preflight/render
assumption, and WASM profiling-stub Clippy failures. The timeout fixture retains
its original one-second deadline and 1,400 ms delay, with separate real FIFO
reads for preflight and rendering. Production cancellation is unchanged. The
later `2305404` test-only writer-cleanup refinement has fmt/source review but
has **not** been compiled or executed: the mandatory Data-volume disk probe
became critical before its verification started. Bead `m80e.32` records the exact
unblock probe; `m80e.6` remains open until current-source verification completes.
The retained-preview helper wiring has shell syntax checks and its existing
real-engine probe passes against the qualified package.

Full native parity, browser lifecycle and release gates remain open. The
approximately 4.5 MB render-only target and a lower size ratchet are not claimed.
No release, registry publish, Actions workflow or size-budget change is included.
