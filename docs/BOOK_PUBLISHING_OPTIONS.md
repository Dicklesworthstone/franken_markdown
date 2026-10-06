# Browser book publishing options

The reusable `book.js` API and book-worker exports accept the detailed PDF
settings already available in the single-document renderer. `createBook`
retains them alongside the parsed chapters; `renderBookPdf` in **book.js** uses
the same session with guaranteed disposal. The older one-shot book export in
`franken_markdown.js` is a separate API and is not extended by this change.

```js
const publication = await createBook(files, {
  title: "Manual",
  author: "Documentation team",
  toc: true,
  tocDepth: 3,
  pageNumbers: true,
  page: { size: "a4", margins: 54 },
  typography: "homogeneous,antiriver,pareto",
  optimalPagination: true,
  microtype: "expansion",
  baseFontSize: 12,
  tableFontSize: 9,
  codeLineNumbers: true,
  metadataEpochSeconds: 0,
  running: {
    header: { left: "{title}", right: "{author}", rule: true },
    footer: { center: "{page}/{pages}" },
    skipFirstPage: true,
  },
  htmlFontFormat: "woff2",
});
try {
  const pdf = publication.renderPdf();
  const site = publication.renderSite();
  // pdf.bytes and site.bytes remain owned after disposal.
} finally {
  publication.dispose();
}
```

`typography` accepts the shared comma-separated tokens `homogeneous`, `antiriver`,
`pareto`, `optimal-pagination`, `protrusion`, and `expansion`. `optimalPagination`
is a boolean shortcut. `microtype` accepts off/protrusion/expansion/all and, when
explicit, replaces microtype tokens without changing other layout switches.
The compatibility `microtypeProtrusion: true` shortcut is applied last.

Body size allows 6–24 points, heading ratio 1.05–2, table size 5–24 points,
TOC depth integers 1–6, and fitToPages integers 1–4294967295. Adaptive page fitting
is a target, not a hard guarantee. Epochs must be nonnegative safe integers.
Templates are at most 4096 UTF-8 bytes per slot; malformed Unicode and unknown
running fields/accessors are refused. Layout-dependent running-template rules
remain owned by the renderer. `skipFirstPage` is preserved even with no explicit
band text, because it also controls the pageNumbers footer.

## State and compatibility

Input capture precedes asynchronous initialization and worker transfer. The
client, worker and session use the same idempotent option normalization; no
caller-owned running objects or image/font buffers survive in the capture.
A retained preview is rebuilt when publishing options change, including the
HTML font container and TOC depth. Unchanged captures remain reusable.

An explicitly requested configuration requires `FmdBook.setPdfOptions` and/or
`setHtmlFontFormat` in the loaded WASM class. Older packages fail with
`UNSUPPORTED_BOOK_OPTIONS` before constructing a native book; legacy default
calls do not require these methods. A matching Rust/WASM rebuild is necessary.

The raw setPdfOptions method validates a complete replacement profile before
changing retained settings. Invalid tokens/numbers/band lengths cannot partially
change the old profile. Source, theme, metadata title/author, assets and navigation
enablement are preserved. It does not clone asset payloads. PDF paper overrides
remain export-local and cannot reset the retained profile. TOC depth is shared
with HTML; htmlFontFormat affects the site, not EPUB's TrueType package.

## Verification boundary

`node --test wasm/tests/book_session.test.mjs wasm/tests/book_render_options.test.mjs`
passes 29 tests: 13 unchanged session tests and 16 new tests. The new suite uses
the actual adapters and real Node worker threads with an explicit recording
engine, not generated WASM. Thirteen new tests fail against the preceding
adapters. This proves capture, validation, forwarding, cleanup, compatibility
errors and preview invalidation, not PDF appearance or Rust execution.

Seven Rust tests in `browser_render_options_tests.rs` cover native book-renderer
parity, defaults, running content, numeric boundaries and state preservation.
They are authored but unexecuted in the toolchain-less authoring environment.
Run `cargo test --features wasm-book book::browser` and rebuild the generated
package before claiming Rust/WASM or visual-output verification.
