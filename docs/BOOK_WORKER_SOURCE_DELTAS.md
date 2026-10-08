# Source-only retained book exports

A retained worker can now export after selective chapter/include edits without
resending unchanged source, images, fonts, or presentation options. Rendering
still runs through the existing native BookSession and complete exporters.

```js
const worker = createBookWorker({ retainBook: true });
try {
  await worker.render(chapters, "pdf", options);
  const expectedRevision = worker.retainedInputRevision;
  if (expectedRevision !== null) {
    const epub = await worker.renderSourceUpdate(
      [{ path: "guide/intro.md", source: "# Revised introduction\n" }],
      "epub", { expectedRevision },
    );
    // epub.bytes is the complete export, not a patch to the previous output.
  }
} finally {
  worker.dispose();
}
```

`retainedInputRevision` is a **transport capture revision**, local to this worker
client. It is not the native u32 source revision or a cross-worker access token.
It advances after each successful retained export, including unchanged source;
read the new value before the next update. It is null while busy, after release,
or when an older worker does not acknowledge source-delta support. The result
also carries its acknowledged revision so an embedding can associate the output
with that capture even if the idle worker is subsequently released.

A change must contain exactly `path` and `source`, using an exact path from the
successful input capture. Chapter and include-only edits can share one batch.
`[]` re-exports the unchanged book, including in another publication format.
The method cannot add/remove/rename/reorder sources, change source roles or grant
resources. Use a complete `render()` for those changes. Preview-only clients
support source deltas for preview; inspection/link jobs remain one-shot.

Both endpoints validate patches. The worker reconstructs and admits the entire
resulting source set against the existing 4096-source/64 MiB UTF-8-and-path
limits before calling native code. Unchanged resource arrays stay worker-owned;
no extra main-thread asset snapshot is retained. Rust still resolves includes,
parses Markdown, and validates the native update receipt. Old native editing
APIs reconstruct from the current worker capture, without retransmitting assets.

Malformed or stale revisions rejected by the client do not consume its newer
idle capture. Unknown source paths, invalid native receipts, rendering errors,
and invalid output envelopes retire the worker rather than silently retrying.
After idle expiry, cancellation or failure, submit the complete current snapshot
with `render()`; deltas cannot create an empty replacement worker or resurrect
revoked resources. Abort/timeout still terminate synchronous native rendering.

The Node suite exercises production admission, book facade, retained session and
worker protocol with real worker threads and an explicit native ABI double.
Its output is deterministic JSON, not typeset PDF/EPUB or a visual preview.
A fixture transfers 12 MiB of image/font bytes initially, then no asset bytes on
its source delta. This is transport-volume evidence, not a renderer-speed claim.
Complete source admission and rendering still have input-dependent cost.
The native Rust/WASM renderer was not rebuilt or executed for this change.


## Complete source capture and workbench integration

`renderSources(chapters, format, { expectedRevision, includeSources })` accepts
all current source strings but sends only their differences from the last
successful capture. It uses the same delta wire contract, native update path,
output validation and cancellation. The chapter/include order and membership
must match; structural changes still require a full `render()`.

The client retains one source-only baseline inside its idle worker entry.
It is detached from mutable caller/transport containers and released together
with that entry on expiry, cancellation, failure or disposal. It contains no
image/font buffers or presentation profile. Explicit `renderSourceUpdate()`
calls advance this same baseline, so callers can mix both source APIs safely.

The publisher's PDF/EPUB/site exports and the independent PDF-proof session now
use this path automatically after a compatible successful capture. They read
`collection.project()` rather than cloning resources with `snapshot()`. Both
the collection configuration identity and worker capture revision must match
that controller's previous success. Structural edits, metadata/page/asset
changes, foreign use of the worker, older adapters and expired captures select
a full snapshot before rendering. A failure after delta submission is not
silently retried. Existing source/checkpoint fences still prevent stale output.
The workbench's separate HTML-preview controller still uses full snapshots;
preview embeddings can explicitly use either source-only worker method.

Tests use the complete production publisher controller and PDF-proof session
with explicitly injected source/DOM/native doubles. They verify that compatible
edits do not call the full-resource snapshot, and resource revocation, failed
exports, silent DOM changes, cancellation and suspension do not revive old
output or authority. The original PDF-proof regression suite runs unchanged.
The Chromium proof harness loads the added runtime dependencies and checks
native-viewer/Blob download lifecycle against its independent ReportLab PDF;
that is browser integration evidence, not FrankenMarkdown typesetting proof.

Run the focused suites from the repository root:

```sh
node --test wasm/tests/book_worker.test.mjs wasm/tests/book_worker_delta.test.mjs wasm/tests/book_publishing_delta.test.mjs wasm/tests/book_pdf_proof.test.mjs
python wasm/book_pdf_proof_browser.py
```
