# Offline HTML-site publication

The root package's `renderBookSite` and `createRenderer().renderBookSite`
now use the canonical book publisher when the matching WASM binary is present.
The previous root implementation had its own ZIP builder: it bypassed newer
chapter-link resolution, built an index for a merged document instead of
individual pages, and could not deliver supplied images, fonts or styles.

```js
import { renderBookSite } from "@franken-suite/franken-markdown";

const output = await renderBookSite([
  { path: "guide/start.md", source: "# Start\n\n[Next](../end.md)\n\n![Chart](chart.svg)" },
  { path: "end.md", source: "# End\n\n[Start](guide/start.md)" },
], {
  title: "Manual",
  lang: "en",
  font: "serif",
  fontScale: "lg",
  toc: true,
  tocDepth: 2,
  pdfImages: [{ destination: "guide/chart.svg", bytes: chartBytes }],
  fontAssets: [{ slot: "body-regular", bytes: fontBytes, weight: 450 }],
});
// output.mimeType === "application/zip"; output.extension === "zip"
const download = output.blob();
const filename = output.filename("manual");
```

The historical `pdfImages` name is shared with the other root render methods;
it supplies images to HTML here. Use book-root asset keys. Two chapters in
different directories can each reference `figure.svg` while receiving different
host-supplied images. No renderer fetches an image, font or source URL.

## What the ZIP contains

The canonical renderer produces a self-contained page per chapter, shared
navigation, `index.html`, `~fmd-search.html`, a chapter-addressed
`search-index.json`, and the existing `frankenmarkdown-receipt.json` with
schema `fmd-book-receipt-v1`. Chapter titles and languages honor frontmatter;
the book title and language label the landing/search pages and provide defaults.
The search index binds headings to their actual chapter page and emitted ID.
Ordinary chapter browsing needs no JavaScript; the optional search page uses
local JavaScript and embeds its own data, so it works without a server.

`customCss` replaces the chapter stylesheet. An explicit empty string is
preserved, not converted to the built-in stylesheet. PDF-only settings such as
`page` or `pageNumbers` are rejected rather than silently ignored. Source parsing
is safe-only: `allowRawHtml: true` is rejected. CSS remains host-supplied code;
this API does not sanitize it or prevent a custom stylesheet from requesting
external resources.

## Includes and ownership

`expandIncludes` defaults to true. Optional `includeSources` supplies additional
`{ path, source }` files for include expansion. These are not chapters, never
receive chapter pages, and do not appear as independent search/receipt entries.
Missing or cyclic includes fail through the core's source-bundle resolver.
Set `expandIncludes: false` to retain literal include examples; include-only
resources then are not accepted.

Chapter/resource strings, primitive settings and exact image/font views are
captured before asynchronous initialization. Later caller edits cannot retarget
the pending publication. Asset buffers are copied, never detached. Output
`sourceLength` counts original source UTF-8 bytes once, including include-only
resources, but excludes logical filenames and expansion duplication.

Admission limits are checked before payload copies and repeated by the native
entry point: 4096 chapter/include sources and 64 MiB text/path bytes combined;
4 MiB metadata/CSS combined; 4096 images; 32 MiB per image or font;
128 MiB aggregate images and aggregate fonts separately; five unique font
slots; 64 KiB aggregate image destinations. The canonical site has its existing
256 MiB uncompressed-content limit and a 4 MiB receipt limit.

## Compatibility and proof boundaries

A rebuilt package uses `renderBookSitePublication` even for default requests.
The old six-argument `renderBookSite` WASM export is retained for old-package
basic calls; these return an explicit legacy-navigation/search warning.
Advanced requests on an old binary fail with `UNSUPPORTED_WASM_PACKAGE`.
A wrapper-only update cannot supply new native rendering behavior.

The pure Rust library adds `BookRenderer::render_site_publication()`.
Existing `render_site()` still uses the same canonical entries without adding a
receipt; its archive emitter is shared, not duplicated.

`node --test wasm/tests/book_site_publication.test.mjs` checks the real public
wrapper with explicit generated-binding doubles. These tests are not evidence
of native HTML or ZIP rendering. `wasm/book_site_types_test.mts` checks the public
TypeScript contracts. After rebuilding the matching package on a configured
DSR host, `node wasm/site_publication_smoke.mjs` executes the actual WASM engine,
independently inflates and CRC-checks every ZIP entry, checks chapter links,
assets, includes, language, receipt and search anchors, and compares repeated
output bytes. That smoke fails when the real generated package is unavailable;
it never substitutes a fake engine.
