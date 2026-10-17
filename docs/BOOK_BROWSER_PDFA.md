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

## Atomic multi-format editions and worker recovery

The complete-edition API accepts the same captured book policy:

```js
import { createBookPublicationWorker } from './book-worker.js';
const publisher = createBookPublicationWorker();
try {
  const edition = await publisher.render(chapters, ['epub', 'pdf', 'site'], {
    includeSources, pdfA: '2b', pdfAStrict: true,
  }, { strictLinks: true });
  const pdf = edition.outputs.find(output => output.format === 'book-pdf');
  // pdf.pdfA === '2b'; pdf.pdfAStrict === true
  // edition.linkReport describes navigation, not PDF/A conformance.
} finally {
  publisher.dispose();
}
```

The coordinator keeps an independent copy of the admitted scalar policy before
passing the capture to any client. Each PDF receipt must agree with that policy;
missing/mismatched archival receipts fail the entire edition. Unexpected PDF/A
metadata on ordinary PDFs or non-PDF outputs also fails. Successful PDF receipts
survive the coordinator's owned-byte copy and client disposal. EPUB/site outputs
never acquire an archival claim. Selecting only non-PDF formats still uses their
existing exporters, even when the book has a saved archival policy.

Worker loss restarts the whole edition with the same policy, input and original
deadline. Both navigation checks run again when strictLinks is selected. A native
archival refusal, missing capability or invalid receipt is terminal; automatic
recovery cannot downgrade archival output to ordinary PDF. An already rendered
EPUB stays private if a later PDF fails. The combined output ceiling and all
cancellation/verification gates remain unchanged.

An additional 27 complete-edition tests exercise the actual public
`createBookPublicationWorker` through normal imports, both existing checker
adapters and the existing report decoder. Real Node workers run the production
session/transport/cache modules around the same explicit native boundary double.
They cover PDF/EPUB/site ordering, independently owned receipts, source/resource
capture, strict and non-strict navigation, actual worker-process failures during
preflight/export/final verification, whole-edition recovery, capability/native
refusal without retry, cancellation and output limits. Separate injected clients
exercise coordinator admission independently of the worker's own policy check.

```sh
node --test wasm/tests/book_pdfa*.test.mjs wasm/tests/book_pdf_page.test.mjs \
  wasm/tests/book_publication_pdfa.test.mjs \
  wasm/tests/book_publication_retained_verification.test.mjs
```

122 tests passed together: 76 new tests plus 19 unchanged geometry and 27
unchanged retained-verification regressions. The latter regression file and
public factory were also verified against their original Git blob hashes. Before
the coordinator change, 21 of the 27 new edition cases failed on missing receipt
propagation or acceptance of invalid policy receipts; all 27 pass after it.
No existing regression was edited. This remains JS/protocol qualification with
explicit native doubles, not a compiled-WASM or real-browser/conformance run.
Workbench profile controls and source-project persistence are not added here;
this change exposes the policy through the public book and publication APIs.
