# Retained book publishing workers

`createBookWorker({ retainBook: true })` retains one native book between PDF,
EPUB, site and HTML-preview requests. The default remains one-shot. Preview-only
embeddings can still use `retainPreview: true`; `retainBook` takes precedence.

```js
import { createBookWorker } from "@franken-suite/franken-markdown/book-worker";

const worker = createBookWorker({ retainBook: true });
try {
  await worker.render(chapters, "preview", options);
  // Each request supplies the complete current source/asset/settings snapshot.
  const pdf = await worker.render(editedChapters, "pdf", options);
  const epub = await worker.render(editedChapters, "epub", options);
  // pdf.bytes and epub.bytes are owned outputs, valid after worker disposal.
} finally {
  worker.dispose();
}
```

## Source capture and compatibility

Exact unchanged captures reuse the parsed book without a source transaction.
Text changes to existing chapters/includes call the native `updateSources` API.
Membership, role or order changes call `replaceSources` against the current native
revision. The final chapter and include sets are submitted together. Source text
and links are not rewritten by JavaScript. Missing old editing APIs cause explicit
reconstruction from the complete current capture. Other native errors are not
hidden by fallback; failed exports and invalid success receipts retire the session.

Metadata, paper, typography, image bindings and font bindings are compared exactly.
Changed profiles or resource bytes reconstruct the native book. No hashes stand
in for byte equality. Each request still admits and transfers the complete owned
snapshot, including private asset copies; this is not an incremental transport
or a claim of constant-time editing or measured speedup.

The worker script and client must agree on the retained-book protocol. Missing
acknowledgement fails rather than keeping an incompatible worker. Inspection and
link checks remain source-only, one-shot operations and release idle state.
Concurrent exports reject with `BOOK_BUSY`; there is no implicit queue or save.

## Lifetime and workbench behavior

An idle book expires after 30 seconds by default (`idleTimeoutMs`). `cancel()` and
`dispose()` release running and idle workers. `cancelPending()` kills in-flight
work but preserves an already-idle book. Cancellation and timeout terminate the
worker, so synchronous native rendering stops rather than merely losing its
Promise. `hasRetainedBook` reports an idle retained endpoint, not saved output.

The publishing workbench opts its publication and PDF-proof clients into retention
independently. Sequential PDF/EPUB/site downloads reuse one publication session.
Source-text edits immediately revoke old downloads/proof views while permitting
an idle native session to process the next source update. Imports, metadata,
resources, structural UI edits, explicit clearing and page suspension remain
conservative full invalidations. The generic worker API supports transactional
structural reuse, but the UI deliberately keeps its existing broader invalidation
boundary. Preview, proof, publishing, font preflight and checks remain separately
owned; cancelling one does not cancel the others.

PDF proof downloads still use the exact immutable proof Blob. Retaining parsed
source never authorizes a stale PDF view. Image/font revocation clears both idle
native sessions immediately, including when no subsequent export is requested.

## Verification boundary

The Node suites `book_retained_exports.test.mjs` and
`book_publishing_retention.test.mjs` exercise production JavaScript adapters,
worker protocol and controllers with explicit native/source/DOM doubles. The
first suite includes real Node worker threads and a synchronously blocked worker
cancellation test. Fixture PDF headers are admission probes, not typeset PDFs.
Existing worker and PDF-proof suites also run unchanged. Package-list checks do
not replace the generated-WASM/native-parity gates.

This implementation pass did not rebuild Rust/WASM, execute the browser helper,
or verify visual PDF output. A matching package build is required before release;
both package assemblers copy the added runtime module and run the new Node suites.
