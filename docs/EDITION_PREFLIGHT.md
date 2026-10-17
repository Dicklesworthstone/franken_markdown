# Captured-edition navigation preflight

## Retained worker analysis

A book worker with `retainBook: true` can now use `"links"` in
`renderSourceUpdate`, `renderSources`, and `renderSourceSet`. The operation
requires the same exact `expectedRevision` fence as retained exports. An empty
source update checks the already parsed publication, including expanded includes,
without another native book, render, or image/font transfer:

```js
const pdf = await worker.render(files, "pdf", options);
const links = await worker.renderSourceUpdate([], "links", {
  expectedRevision: pdf.retainedInputRevision,
});
```

The existing native `BookSession.validateLinks()` owns the analysis. The worker
returns `book-links` JSON and a new `retainedInputRevision`; subsequent source
operations must use this new revision. Changed sources are synchronized before
analysis through the same native transactions used for rendering. Reports are
capped at 4 MiB on both sides of the transport, even with a larger output budget.
Native/capability/protocol failures retire the capture rather than silently
rendering a replacement. Cancellation terminates the endpoint.

The full-snapshot `render(files, "links", options)` remains **one-shot and
source-only**, even on a retained client. It does not read image/font/style
getters and clears previous retained authority. Preview-only retention cannot
perform retained navigation checks.

These reports cover `expanded-html-navigation`, not external URL liveness,
asset availability, source accessibility, or PDF/EPUB conformance. Consumers
must admit the JSON with `parseBookLinkReport` against their captured chapter
paths and must not trust a worker-provided summary or optimistic verdict.

## Tests

`node --test wasm/tests/book_retained_links.test.mjs` exercises the production
facade, retained adapter and both worker transport sides with explicit native
class and endpoint doubles. It covers identity/resource reuse, source edits,
source-set replacement, stale revisions, one-shot isolation, byte limits,
failures and cancellation. It is not a Rust/WASM or rendered-output proof.
