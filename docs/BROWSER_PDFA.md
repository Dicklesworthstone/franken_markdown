# Single-document PDF/A emission in browser APIs

The root `renderPdf` facade, document worker and Flow document exports accept
`pdfA: "2b"` and `pdfAStrict: true`. They call the existing native single-document
archival writer with the same Markdown, authorized image/font bytes, metadata,
paper geometry, running bands and typesetting settings as ordinary PDF export.
There is no synthetic book, PDF byte conversion or browser print step.

```js
import { renderPdf } from '@franken-suite/franken-markdown';

const output = await renderPdf('# Report\n\nBody text.', {
  pdfA: '2b',
  pdfAStrict: true,
  title: 'Report',
  metadataEpochSeconds: 1700000000,
  page: { size: 'a4', margins: 36 },
  strictPagination: true,
  baselineGridPt: 13.125,
});
// output.pdfA === '2b'; output.pdfAStrict === true
// output.bytes / output.blob() contain the successful native export.
```

These fields select **PDF/A-2b emission**, not independent PDF/A validation or
certification. Strict mode applies the native writer's existing policy, including
refusing forbidden URI actions. A failure stays a failure; it cannot produce an
ordinary PDF while claiming archival emission.

## Policy and compatibility

Only `off` and `2b` are accepted. Strictness must be boolean; `true` requires
`2b`. Omitted policy and explicit `off` retain the original ordinary ABI and
output envelope. Only successful archival outputs carry `pdfA` and `pdfAStrict`.

The matching `renderPdfConfiguredArchival` WASM binding is required before assets
are copied or the engine is initialized. Combining PDF/A with a positive baseline
grid requires its extended 39-argument binding. Older packages fail with
`UNSUPPORTED_WASM_PACKAGE`; they never silently lose either setting. As with
ordinary exports, a fixed grid cannot be combined with `fitToPages`.

Source, scalar settings, paper/running records and exact image/font byte ranges
are captured before asynchronous initialization. Later host changes do not
retarget an admitted publication. Typed `PAGINATION_*` refusals retain their
existing behavior when strict pagination is requested.

The root's legacy `renderBookPdf` entry does not support these single-document
options. Chapter-aware archival publication remains available from the `./book`
entry, as described in [BOOK_BROWSER_PDFA.md](BOOK_BROWSER_PDFA.md).

## Worker and atomic publication

```js
import { createWorkerRenderer } from '@franken-suite/franken-markdown/document-worker';

const renderer = createWorkerRenderer();
try {
  const edition = await renderer.publish('# Report\n\nBody text.', [
    { format: 'html' },
    { format: 'pdf', options: { pdfA: '2b', pdfAStrict: true } },
    { format: 'epub' },
  ]);
  const pdf = edition.outputs.find(output => output.format === 'pdf');
  // Only the PDF has pdfA/pdfAStrict. All artifacts belong to one source snapshot.
} finally {
  renderer.dispose();
}
```

The worker checks the actual exporter receipt against the captured request before
transferring output. The client checks it again against its own request after
receipt. Missing, downgraded, unexpected or non-PDF archival claims fail the
publication and retire a mismatched worker. An already rendered HTML or EPUB
artifact is kept private if another target fails. Byte budgets, cancellation and
deadlines retain their existing semantics.

Flow sessions use the same policy with
`session.exportDocument('pdf', options, session.token)` or a PDF entry in
`session.exportDocuments(...)`. Both direct and worker Flow preserve the
source/layout fence and archival receipt. A native refusal is recoverable for
the retained Flow session; a mismatched worker acknowledgment retires it.

## Verification boundaries

`wasm/pdfa_api.test.mjs` executes the production facade with explicit generated
binding doubles to check ABI routing, complete request capture, capability
refusal and ordinary compatibility. `wasm/document_pdfa.test.mjs` uses real Node
worker threads, production transports and explicit native renderer doubles to
check single and atomic exports, ownership and receipt rejection. These tests
do not render PDFs or establish conformance. Native archival writer tests and
generated-WASM execution are separate verification steps.
