# Atomic multi-format book publication

`createBookPublicationWorker` is an additive export of the existing `book-worker`
package entry. It prepares a PDF, EPUB and HTML-site ZIP as one captured edition:
one source/settings/resource snapshot, one private retained worker, and one
complete result only after every selected format succeeds.

```js
import { createBookPublicationWorker } from "@franken-suite/franken-markdown/book-worker";

const publisher = createBookPublicationWorker({
  timeoutMs: 120000,
  maxOutputBytes: 128 * 1024 * 1024,
});
try {
  const edition = await publisher.render([
    { path: "start.md", source: "# Manual\n\n[Next](next.md)" },
    { path: "next.md", source: "# Next\n\nThe next chapter." },
  ], ["pdf", "epub", "site"], {
    title: "Manual",
    toc: true,
    metadataEpochSeconds: 1700000000,
  });
  // edition.outputs contains three native outputs in the requested order.
  // Offer each output.blob() for an explicit download using output.extension.
  // No object URLs, disk writes or downloads occur automatically.
} finally {
  publisher.dispose();
}
```

Omitting the format list requests `pdf`, `epub`, then `site`. One through three
unique formats are accepted, in caller order. Preview and inspection operations
are not publication formats. The returned frozen container has `format:
"book-publication"`, `sourceLength`, `byteLength` (the sum of all output lengths),
and `outputs`. Each output has the existing native format identifier, MIME type,
extension, source length, caller-owned bytes and a `blob()` method. The outputs
are separate native files, not a new wrapper archive.

## One edition, one capture

The public facade captures through `prepareBookInput`, the existing book Unicode,
source-budget, settings and resource normalizer. It does not introduce another
Markdown parser or resource policy. Subsequent caller edits cannot change the
captured chapter/include text, options, images or fonts during publication.

The first export sends the complete owned snapshot through `createBookWorkerClient`
with `retainBook` enabled. Each later export sends an **empty source delta** against
the last acknowledged `retainedInputRevision`. It sends no replacement settings,
unchanged source strings, image buffers or font buffers. The native retained
session therefore renders each format from the same captured book. Every receipt
must advance the transport revision and retain the same source length. Older
workers without source-capture support fail explicitly; the coordinator does not
silently substitute independent per-format workers or recapture newer input.

The private client is never exposed for unrelated renders or edits. One `busy`
slot covers the complete edition. Overlapping calls reject with `BOOK_BUSY`;
there is no queue. Each completed or failed edition releases its worker. A later
publication starts a fresh capture, not a hidden cache of old resource grants.

## All-or-nothing delivery and limits

Any native export error, malformed receipt, worker failure, cancellation or limit
failure rejects the entire operation. Earlier successful formats remain private
and are not returned as a partial edition. Successful output views are copied
before later work or disposal can reuse their backing storage.

`maxOutputBytes` is a **combined** output ceiling: default and maximum 128 MiB.
Two individually legal outputs whose sum exceeds it still reject the edition.
This bounds admitted output bytes, not temporary renderer heap or the peak memory
of snapshots, transfers and output copies. Once no output budget remains, the
next native export is refused before starting. Existing native/source/asset admission
limits remain unchanged.

`timeoutMs` is a single deadline across input capture, worker initialization and
all formats, not a fresh deadline per format. Monotonic phase checks also refuse
late results when synchronous capture/copying or custom endpoint microtasks prevent
a timer callback from running. Its default is 120000 ms and allowed
range is 1..600000. An AbortSignal is accepted as the fourth render argument:
`publisher.render(files, formats, options, { signal })`. `cancel()` rejects the
current edition and disposes its private worker, including synchronous Rust work;
the publisher remains reusable. `dispose()` also prevents future calls. Late
results cannot publish or interfere with a newer operation.

Atomicity refers to returning the in-memory edition. It does not make separately
requested browser downloads transactional or prove that a file reached disk.
These are programmatic APIs; no new workbench button is introduced in this slice.
The existing single-output `createBookWorker` behavior is unchanged.

## Verification boundary

```sh
node --test wasm/tests/book_publication*.test.mjs
```

All 34 Node tests pass, including 12 public-facade checks in a real ESM VM
subprocess and execution of both package-copy loops.

The coordinator tests execute the complete production module through normal ESM
imports. One integration test additionally executes the unchanged production
retained-session manager. Native exporters, input capture and transport receipts
are explicit doubles in these tests. Public-facade checks load its unchanged ESM
source with explicit dependency bindings in a Node VM subprocess. Assembly tests
execute the actual package copy loops in isolated source fixtures, compare copied
bytes, and import and execute the copied coordinator. No build gate, size budget,
parity check or worker protocol is replaced or relaxed.

These tests do not establish native/WASM rendering, full package startup, real
browser workers, publication-format conformance or visual output. The Rust/Cargo
and prescribed remote-build tools were unavailable during implementation. Run the
existing DSR-managed package/parity gates with the matching native package before
claiming those properties. Both assemblers now include the coordinator and this
document, and invoke the publication regression suite.
