# Browser book publishing

Serve the assembled WASM package over HTTP and open `demo/book.html`, or follow
**Open the book publisher** from the Flow editor's export section. The link opens
a new tab without moving or discarding the current Flow document. Download its
Markdown first to import it into a collection; there is no implicit cross-tab
transfer of source or image authority.

The workbench combines the existing Rust `FmdBook` renderer with an ordered local
collection and dedicated module workers. It adds no Markdown renderer, ZIP writer,
browser-print PDF, third-party dependency, network loader, or server-side storage.

## Complete workflow

Add `.md`/`.markdown` files, PNG/JPEG/SVG image files, or a folder. Folder import
strips the one selected root and preserves nested paths. Other extensions are
ignored and counted. Imports append only after the complete batch is valid;
duplicate keys, malformed UTF-8, over-budget data, and edits during an asynchronous
read refuse the import without replacing existing source. File-picker order is
not assumed to be reading order: move chapters up/down explicitly.

Create, edit, rename, reorder, or remove chapters. Relative links and images use
the book-relative path: `guide/start.md` referencing `figure.svg` needs an
explicitly supplied `guide/figure.svg`. Renaming does not rewrite links. The core
owns cross-chapter resolution, transclusion, and format-specific validation;
this UI does not guess those semantics or silently fetch missing resources.

Choose title, author, language, font family/scale, theme, contents, and PDF page
numbers. Prepare a merged PDF, EPUB, or HTML-site ZIP, then click the separate
Download link. An edit, reorder, import, image revocation, cancellation, or page
suspension retires the prepared result. Response identity and the current UI/model
snapshot are checked again before publication and download activation.

## Keep the source

The collection is in-memory, **not autosaved**. Prepare a source project and click
Download before leaving. `book.fmdbook.json` records schema version 1, ordered
chapter paths/source, and settings. It is plain unencrypted JSON. Images, fonts,
worker state, handles, and access grants are deliberately excluded. Keep original
image files separately. Reopening a project requires confirmation, clears all
previous image grants, and refuses replacement if edits occur while reading or
confirming. Reauthorize images before publishing the restored book.

Prepare chapter Markdown remains available when WASM cannot load or publishing
settings are invalid. A half-edited invalid path gets a safe fallback filename.
Untouched imported BOMs and CR/LF bytes survive textarea newline normalization;
edited text follows the textarea's LF convention. Returning to the original view
restores the original bytes. Invalid typed Unicode is retained visibly but must
be repaired before UTF-8 publication. No operation overwrites an existing disk
file; preparing an object URL is not proof that a download completed.

The workbench admits 128 chapters, 4 MiB per chapter and 16 MiB total source plus
paths; 128 images, 8 MiB each and 32 MiB total. Source projects have a 64 MiB JSON
ceiling (escaping can make JSON larger than source). An unusually escape-heavy
collection that exceeds it can still be downloaded chapter by chapter. Projects
are not encrypted vaults or substitutes for backups. Page hiding revokes URLs and
images; back/forward cache retains only the in-memory source collection.

## Worker API

```js
import { createBookWorker } from "@franken-suite/franken-markdown/book-worker";
const publisher = createBookWorker();
try {
  const output = await publisher.render([
    { path: "start.md", source: "# Start\n\n[Next](end.md)" },
    { path: "end.md", source: "# End" }
  ], "epub", { title: "Manual", toc: true });
  // output.bytes, output.blob(), output.mimeType and output.extension
  // Let the host explicitly offer the Blob for download.
} finally {
  publisher.dispose();
}
```

`render(files, format, options, { signal })` accepts `pdf`, `epub`, or `site` and
the existing `BookOptions`. One export owns one worker. Concurrent calls reject
with `BOOK_BUSY`; there is no unbounded queue. `cancel()`, an AbortSignal, or
`dispose()` terminates the worker, including synchronous Rust work. Cancellation
and errors leave the client reusable until disposal. Each export starts a fresh
WASM instance; this trades repeated startup cost for simple ownership and reliable
cancellation. No engine initialization occurs on the UI thread.

Chapter text is immutable and asset views are copied before any asynchronous
startup. Only private copies are transferred, never the caller's buffers. The
shared book facade now rejects malformed Unicode and snapshots images/fonts
across its own asynchronous initialization too. Returned bytes are transferred
as an owned view, never the larger backing WASM memory.

The worker deadline defaults to 120000 ms (configurable 1..600000). Returned output
is capped at 128 MiB; `maxOutputBytes` may lower that ceiling. This is an output
admission limit, **not a bound on the Rust renderer's temporary heap**. Native
book ingress limits remain separate from the smaller workbench limits.

## Verification

```sh
node --test wasm/tests/book_session.test.mjs wasm/tests/book_worker.test.mjs \
  wasm/tests/book_collection.test.mjs wasm/tests/book_controls.test.mjs \
  wasm/tests/book_package.test.mjs
```

The tests use real adapters/controller, native Node File/Blob/object URLs and
explicit engine, DOM, and transport doubles. Package inventory checks are static.
They do not prove generated-WASM rendering, native-browser module workers, IME,
visual output, or publication-format conformance. Run the project's DSR-managed
WASM build/parity gates and browser acceptance before claiming those properties.
Both existing package assemblers include the worker, its dependencies and demo.
