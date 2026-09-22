# Resource-aware browser SVG export

`renderSvg` now forwards explicit images and font faces to the native SVG
resource renderer through `renderSvgConfiguredResources`. The same method is
available on `createRenderer()`. No new browser rendering dependency is added.

```js
import { renderSvg } from "./franken_markdown.js";

// chartBytes and bodyFontBytes are Uint8Arrays already authorized by the host.
const output = await renderSvg("# Results\n\n![Chart](chart.svg)", {
  maxWidthPt: 360,
  pdfImages: [{ destination: "chart.svg", bytes: chartBytes }],
  fontAssets: [{ slot: "body-regular", bytes: bodyFontBytes, weight: 500 }],
});
const poster = output.blob(); // image/svg+xml
for (const finding of output.diagnostics) {
  console.warn(finding.code ?? "parser", finding.message);
}
```

The `pdfImages` name is retained for compatibility with the shared browser
options. SVG accepts PNG, JPEG and SVG payloads. Destination strings match
Markdown image references; they never authorize network or filesystem reads.
Malformed or unresolved images keep the core's visible fallback and diagnostics.
Invalid supplied fonts fail rather than silently using another font.

## Request capture and admission

Source, scalar options, destinations, image bytes, font bytes and weight pins
are captured before asynchronous initialization yields. Sliced typed arrays,
DataViews and Node Buffer slices copy only the selected byte range, not their
whole backing allocation. Shared and detached buffers are rejected. Editing
caller-owned source objects, arrays or buffers while initialization is pending
does not retarget the export already requested.

Both the JavaScript wrapper and direct Rust ABI admit bounded requests. Source
is at most 32 MiB of UTF-8; there are at most 4096 images and five font slots.
Each supplied asset is nonempty and at most 32 MiB; images and fonts each have
a 128 MiB aggregate limit. Image destinations are unique after trimming, each
at most 8192 UTF-8 bytes and at most 64 KiB combined. Source and destination
strings cannot contain unpaired UTF-16 surrogates. Width is 144..14400 points,
default 612. Existing shared font-scale presets and aliases are retained.

Counts, view bounds and totals are checked before payload copies. This is
bounded synchronous rendering, not background rendering or cancellation.

## Diagnostics and compatibility

Results retain `bytes`, `text()`, `blob()`, `filename()`, `sourceLength`, and the
existing output envelope. Source length and parser spans use UTF-8 byte units.
SVG output has format/extension `svg` and MIME type `image/svg+xml`, also
reflected by the dedicated TypeScript result type.

Parser diagnostics retain their real spans. Renderer findings carry a stable
`code` and `scope: "document"`, with `start: 0, end: 0`: the core does not have
exact inline source spans for those findings. Image/math warnings are no longer
discarded by the browser adapter. `svg_missing_glyphs` reports skipped glyphs.
The original five-argument Rust SVG entry point shares this diagnostic path.

A wrapper update is not a WASM rebuild. When a loaded generated package lacks
the new export, requests with supplied assets reject with
`UNSUPPORTED_WASM_PACKAGE`, before initialization or payload cloning. Basic
asset-free requests still use the legacy binding and receive the explicit
`svg_legacy_package` warning because complete diagnostics cannot be guaranteed.
SVG remains single-page and light-only; the core's existing typography, image
containment, and unsupported-content limitations are unchanged.

## Verification

```sh
node --experimental-vm-modules --test wasm/svg_api_test.mjs
tsc --noEmit --strict --target ES2022 --module esnext --lib ES2022,DOM wasm/svg_types_test.ts
cargo test --features wasm-bindgen --lib wasm_abi::svg::tests
```

The Node suite executes the real public wrapper with isolated generated-binding
stubs. It proves dispatch, packing, request capture, limits, compatibility,
diagnostics and result cleanup, not Rust rendering or native/WASM byte parity.
All 21 cases and strict TypeScript checks passed during implementation. The
packing and asynchronous-capture tests also failed against the original wrapper.
Eight Rust renderer/admission regressions are included, but the implementation
environment had no Cargo/Rust/DSR runner. Those tests, compilation, Clippy,
rebuilt WASM and browser rendering remain unverified. Use the repository's DSR
workflow for the full generated-package and rendering gates, not Actions.
