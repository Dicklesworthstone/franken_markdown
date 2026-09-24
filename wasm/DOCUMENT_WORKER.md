# Cancellable document rendering off the UI thread

Import the `./document-worker` subpath to render HTML, PDF, SVG, EPUB, or
interactive HTML with the existing Rust/WASM renderer in a dedicated worker.
Importing or constructing this API starts no worker and loads no WASM on the
calling thread. The first valid render lazily starts the worker and initializes
its renderer; subsequent requests reuse that instance.

```js
import { createWorkerRenderer } from "@franken-suite/franken-markdown/document-worker";

const renderer = createWorkerRenderer();
const abort = new AbortController();
try {
  const pdf = await renderer.renderPdf("# Report\n\nOriginal Markdown.", {
    title: "Report", pageNumbers: true, metadataEpochSeconds: 0,
    page: { size: "a4", orientation: "landscape", margins: 36 }
  }, { signal: abort.signal, timeoutMs: 30000 });
  const blob = pdf.blob();
  // The host decides how to save this Blob and when to revoke any object URL.
  console.log(pdf.filename("report"), blob.size, pdf.diagnostics);
} finally {
  renderer.dispose();
}
```

The five convenience methods are `renderHtml`, `renderPdf`, `renderSvg`,
`renderEpub`, and `renderInteractiveHtml`. `render(format, source, options,
controls)` is the equivalent format-dispatched entry. Outputs retain the general
render API's bytes, MIME type, extension, source byte count, diagnostics, `text()`,
`blob()`, and `filename()` helpers. It does not add a second Markdown parser,
Canvas export, alternate typesetter, network image loader, or new core dependency.

## Cancellation and lifecycle

Each renderer owns one dedicated worker and uses the existing owned RPC
transport. Only one request executes at a time. Queued requests remain on the
caller side and can be cancelled individually before dispatch. An in-flight
abort or deadline terminates the worker because synchronous WASM cannot service
a cancellation message while running. The triggering request rejects with
`ABORTED` or `TIMEOUT`; queued requests reject with `SESSION_LOST`. The renderer
then remains disposed. Explicitly construct a new renderer to continue; no
request is silently replayed and no rollback is claimed.

The default 30-second deadline includes queue wait and first-use initialization;
set `timeoutMs` per renderer or request, or use 0 to disable it. Timer delivery
still depends on the host event loop, not a real-time scheduler. `dispose()` is
immediate/idempotent and rejects pending work. Renderer loading errors, traps,
and malformed output close the worker; there is no main-thread fallback.

Use separate instances when preview cancellation must not terminate a publication
export. The main demo does this: edits cancel obsolete preview/export work,
composition pauses scheduling, and raw source/settings checkpoints are checked
before publishing results or downloading a PDF. Buttons become **Cancel render**
and **Cancel PDF** while active. Page suspension terminates both workers, revokes
PDF URLs, clears the preview and resets raw-HTML permission while retaining source.
Back/forward-cache restoration starts a fresh preview, not a stale worker result.

## Inputs, options and limits

Source must be valid Unicode within 4 MiB of UTF-8. Options are plain data, not
accessors or silently coerced values; unknown and wrong-format fields reject.
HTML, PDF, SVG and EPUB accept supplied images (`pdfImages`) and TrueType font
slots (`fontAssets`), including variable font weight pins. Interactive HTML
retains its narrower options and rejects those assets rather than silently
losing them. Format-specific TypeScript declarations describe the allowlists.
Raw HTML is escaped by default; `allowRawHtml` is an explicit HTML/PDF option.
Worker rendering is not a sandbox for subsequently displaying or opening output.

PDF accepts `page` with `size: "letter"`, `size: "a4"`, or explicit
`size: { widthPt, heightPt }`, optional `orientation`, and uniform or per-side
`margins`. Units are PDF points, not CSS pixels. Dimensions must be 144..14400;
margins must leave at least a 72-point content rectangle in both dimensions.
An explicit page defaults to Letter and 72-point margins. Omitting `page` preserves
the original renderer defaults. Nested geometry is validated and deeply captured
before dispatch using the same normalization and f32 bounds as the direct API.

SVG accepts `maxWidthPt` in 144..14400 and the same owned image/font inputs.
EPUB also accepts `customCss`, `toc`, and `tocDepth` (1..6) for styled publications
with navigation. Custom CSS is limited to 1 MiB of UTF-8 in the worker entry.
For example, with host-selected image and font byte arrays:

```js
async function exportBook(renderer, markdown, imageBytes, fontBytes) {
  return renderer.renderEpub(markdown, {
    title: "Field guide", lang: "en", toc: true, tocDepth: 2,
    customCss: "p { color: navy; }",
    pdfImages: [{ destination: "cover.png", bytes: imageBytes }],
    fontAssets: [{ slot: "body-regular", bytes: fontBytes, weight: 450 }]
  });
}
```

Advanced options require a generated WASM package built from matching source.
When the direct renderer reports `UNSUPPORTED_WASM_PACKAGE`, that code and its
rebuild explanation survive the worker boundary; the worker closes and queued
requests are not replayed. Options are never silently downgraded to fit an older
binary. The synchronous renderer APIs themselves are unchanged.

Input image/font views are copied only after queue admission, and only those owned
copies are transferred. Caller buffers are never detached; edits to options or
bytes after enqueue cannot change a queued request. Shared/detached buffers,
duplicate asset destinations/slots and malformed Unicode are rejected. Defaults
allow 32 active-plus-queued requests and 16 MiB of charged input (maximum 128
requests and 64 MiB). Images are at most 8 MiB each and 1,024 per request; fonts
are at most 32 MiB each and five slots. The aggregate ingress budget applies too.

Output is checked before transfer, default/maximum 64 MiB. Diagnostics are limited
to 1,024 records and 64 Ki UTF-16 units of combined message/reason-code text.
Optional reason `code` strings (1..128 UTF-16 units) and `scope: "document"` are
preserved. Document-scoped findings must use `start=end=0`; other findings retain
their source-bound byte ranges. Legacy diagnostics keep their original shape.
Malformed metadata closes the worker instead of silently dropping its meaning.
These limits bound ingress, copying and publication, not every temporary native
allocation or the entire WASM heap. Host-side source validation, binary copying
and rendering the resulting HTML still do work; expensive Rust rendering moves
off-thread, not every operation in a browser application.

## Packaging and verification

Serve `document_worker_entry.js` beside the assembled package under the host's
normal module-worker/CSP policy. Bundlers can supply `workerFactory`; its entry
can import `@franken-suite/franken-markdown/document-worker/worker`. The factory
must return a fresh owned Worker-compatible endpoint. Existing synchronous and
flow/book APIs remain unchanged. Both package assemblers include this entry and
its dependencies without changing WASM size budgets.

```sh
node --test wasm/document_worker.test.mjs wasm/demo_worker.test.mjs
node wasm/document_worker_smoke.mjs <assembled-package> <generated.wasm>
# With TypeScript installed: validate accepted and rejected public option shapes.
tsc --noEmit --strict --target es2022 --module nodenext --moduleResolution nodenext \
  --lib es2022,dom wasm/document_worker_options.typecheck.mts
```

The first command exercises actual Node worker threads and structured-clone
transfers with an explicitly identified native renderer double, plus production
demo lifecycle code with DOM/renderer doubles. Worker regressions cover geometry,
EPUB publication settings, exact-view resource ownership, cancellation, diagnostic
metadata and package-mismatch errors. They do not prove typography or browser
acceptance. The separate smoke gate loads the production worker entry and actual
generated WASM and compares all five formats against the existing direct API,
including supplied images for HTML/PDF/SVG/EPUB, named/custom PDF geometry, EPUB
CSS/navigation, and document-scoped SVG export diagnostics. Both build routes run
that gate. Native-browser iframe/CSP/download and visual acceptance remain
separate checks.
