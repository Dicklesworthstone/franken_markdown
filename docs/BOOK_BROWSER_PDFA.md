# Chapter-aware PDF/A emission in the browser book API

The JavaScript book facade now connects to the existing native
`FmdBook.renderPdfArchival` method. `pdfA: "2b"` selects the chapter-aware archival
writer; `pdfAStrict: true` applies that writer's strict policy. This is emission
configuration, **not independent PDF/A validation or certification**. Navigation
verification (`strictLinks` on the publication API) is a separate check.

```js
import { createBook, renderBookPdf } from './book.js';

const output = await renderBookPdf(chapters, {
  includeSources,
  pdfA: '2b',
  pdfAStrict: true,
  page: { size: 'a4', margins: 36 },
  metadataEpochSeconds: 1700000000,
});
// output.pdfA === '2b'; output.pdfAStrict === true
// output.bytes / output.blob() contain the successful native export.

const book = await createBook(chapters, { pdfA: '2b', pdfAStrict: true });
try {
  const archival = book.renderPdf();
  const ordinary = book.renderPdf({ pdfA: 'off' });
  const archivalAgain = book.renderPdf();
} finally {
  book.dispose();
}
```

## One retained book, no alternate renderer

Sources, includes, typography, metadata, authorized images/fonts and paper
geometry reach the same retained native book. JavaScript does not flatten chapter
source, parse Markdown, convert PDF bytes or install a substitute renderer.
Selective source edits and full chapter/include replacements preserve the
captured archival defaults. EPUB/site/HTML previews use their existing exporters.

Only `off` and `2b` are accepted. Strictness must be boolean and true requires
`2b`. Invalid policy is rejected before asset admission or asynchronous engine
loading. Defaults and explicit off remain absent from normalized captures and
use the original argument-free `renderPdf()` or `renderPdfWithPage()` path.

A session's `renderPdf()` accepts only data properties `page`, `pdfA` and
`pdfAStrict`. Overrides are export-local and never alter stored defaults or
source revisions. Undefined inherits the session value; explicit `pdfA: 'off'`
also disables inherited strictness for that export. Explicit off with strict true
is invalid. Accessors and unknown properties are rejected without invoking them.

The new native method is required only when an archival PDF is actually
requested. A missing method returns `UNSUPPORTED_BOOK_PDFA`. Native strict-policy
failures remain failures, never ordinary-PDF fallback. A failed one-shot export
frees its native book; a failed read-only session export does not change its
policy. Rebuild the matching Rust/WASM package before using the new feature.

## Worker transport and output-cache correctness

`createBookWorker({ retainBook: true })` captures the policy before asynchronous
startup. The worker must acknowledge the exact captured profile and strictness,
even when the first format is EPUB or an HTML preview and a later source-only
request asks for PDF. The client's private idle baseline retains that policy;
source deltas and source-set replacement cannot change it or resend assets.

For PDF, worker acknowledgment additionally requires the actual exporter receipt;
merely echoing input options is insufficient. Missing or mismatched acknowledgment
refuses output and retires the worker. Older worker scripts that silently ignore
new options therefore cannot produce an apparently successful archival export.
Ordinary requests require no new protocol fields. Public PDF outputs expose
`pdfA`/`pdfAStrict` only after success; non-PDF outputs have no PDF emission claim.

Both native-session reuse and the bounded output/history cache include policy
in their settings identity. Switching ordinary/archival/strict mode invalidates
all prior output history. Repeated unchanged requests still use independent
output copies. Source undo/redo retains the existing exact-source cache behavior;
a later miss synchronizes the actual native book, not a transport revision.
Failure/cancellation retires retained state as before. No quota, retry policy,
resource permission, deadline, dependency, native ABI or build gate was changed.

## Verification boundary

```sh
node --test wasm/tests/book_pdfa*.test.mjs wasm/tests/book_pdf_page.test.mjs
```

68 Node tests passed together: 49 new archival adapter/cache/worker cases and
19 unchanged geometry regressions. The geometry test and shared page helper were
checked against their main-branch Git blob hashes. Production modules execute via
normal imports. Worker cases use real Node worker threads, structured cloning,
transferable buffers and termination with an explicit native-engine double.

The native double returns observation payloads, not rendered documents. These
checks establish JavaScript admission, ABI routing, state/cache isolation and
worker protocol behavior. They do not establish Rust compilation, WASM execution,
real PDF bytes, PDF/A conformance, visual quality, browser acceptance or a full
repository/package build. Native tests from the earlier archival API change were
not run here. No existing regression, golden, test gate or dependency was changed.
The new tests are included by the existing `book_pdf*.test.mjs` package wildcard.
