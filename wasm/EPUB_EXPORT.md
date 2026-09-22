# Browser EPUB publication export

`renderEpub` passes host-supplied images, font faces, stylesheet and navigation
options to the native EPUB writer. It does not build an archive in JavaScript
or capture the browser preview. The Rust renderer owns XHTML, the manifest,
spine, resource paths, font subsetting, and deterministic ZIP output.

```js
import { renderEpub } from "@franken-suite/franken-markdown";

const publication = await renderEpub("# Guide\n\n![Chart](chart.png)", {
  title: "Guide",
  lang: "en",
  toc: true,
  tocDepth: 2,
  pdfImages: [{ destination: "chart.png", bytes: chartBytes }],
  fontAssets: [{ slot: "body-regular", bytes: fontBytes }],
  customCss: ".fmd { line-height: 1.7; }",
});
const blob = publication.blob(); // application/epub+zip
const filename = publication.filename("guide"); // guide.epub
```

`chartBytes` and `fontBytes` are bytes the host already owns and is authorized
to use. Nothing fetches a URL or reads a file implicitly. The historical
`pdfImages` option name is shared with HTML/PDF; EPUB packages the supported
images as archive resources. All five font slots and their optional `weight`
pins use the same font validation as other outputs. Supplying any face opts
into publication-wide subsets; absent slots then use bundled fallbacks.
Supplying no faces preserves the native renderer's font-free publication.

Source, settings and exact asset views are captured before asynchronous WASM
initialization. Mutating input arrays or buffers after calling `renderEpub`
cannot change the pending publication. Buffers are copied, never detached.

The browser boundary admits up to 32 MiB UTF-8 source, 4 MiB stylesheet text,
4,096 images, 32 MiB per image/font face, and 128 MiB per combined image/font
category. Image destinations are unique, nonblank, at most 8,192 UTF-8 bytes
each and 64 KiB combined. Empty, shared or detached asset buffers are refused.
These are input/payload limits, not a bound on total browser or WASM memory.

The additive `renderEpubConfiguredAdvanced` binding must be built into the
WASM package to preserve assets, custom CSS and navigation settings. Old
packages retain their original basic EPUB call, but requests requiring the
advanced binding fail with `UNSUPPORTED_WASM_PACKAGE` instead of silently
dropping those inputs. Raw HTML remains disabled by the native EPUB policy.
An explicitly empty `customCss` is preserved and disables the default style.
EPUB navigation is a publication requirement; `toc`/`tocDepth` control the
HTML-rendered document TOC, not removal of the required EPUB navigation file.

## Export the current editor session

Both direct flow sessions and dedicated flow-worker sessions use the shared
export coordinator. EPUB is accepted through the existing `exportDocument`
request and result contract:

```js
const captured = session.token;
const publication = await session.exportDocument("epub", {
  title: "Edited guide",
  lang: "en",
  toc: true,
  customCss: ".fmd { line-height: 1.7; }",
  maxOutputBytes: 16 * 1024 * 1024,
}, captured);
```

Session export collects its resolved image occurrences at the captured source
and layout revision. Missing or dimension-only image results fail before
rendering. Equal payloads at the same destination are deduplicated; differing
payloads for one destination fail rather than choosing an arbitrary image.
Changes to source, layout or asset state during the awaited render invalidate
the result. Output bytes and diagnostics are owned copies. One physical export
at a time is shared across HTML, PDF and EPUB.

The session's existing 4 MiB source, 1,024 image-occurrence, 8 MiB per payload,
32 MiB unique-image and 64 MiB maximum output limits continue to apply. Custom
font payloads remain a standalone `renderEpub` option, not an override of the
session's font registry. EPUB is reflowable and uses the publication renderer's
styles; it is not a promise to reproduce Canvas geometry or font appearance.

The worker uses the same strict option/result normalizers and existing owned
binary transfer. Rebuild worker JavaScript and native WASM from matching source.
Older workers may refuse the new format; there is no HTML/PDF substitution.
Worker cancellation remains unchanged: queued cancellation prevents execution,
whereas in-flight cancellation terminates the worker without claiming rollback
or replaying the export.

## Verification

`node --test wasm/epub_assets.test.mjs wasm/epub_flow_export.test.mjs` executes
the production JavaScript wrapper/export coordinator and shared validation.
Generated bindings, native snapshots and native render bytes are test doubles;
these tests do not prove compiled-WASM rendering or EPUB reader conformance.

`tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution
NodeNext wasm/epub_export_types_test.mts` checks direct and worker format-specific
options. The five Rust tests in `src/wasm_abi/epub.rs` cover native output parity,
resource/font packaging, determinism and admission when run with the repository's
`wasm-bindgen` feature and configured Rust toolchain.
