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

## Saved native and offline workspaces

`pdfA` and `pdfAStrict` are supported by `exporter.render(markdown, options)`
after `const exporter = await createNativeWorkspaceExporter(runtime)` from
`@franken-suite/franken-markdown/interactive/exporter`.
`renderOfflineWorkspace(markdown, runtime, options)` from
`@franken-suite/franken-markdown/interactive` accepts the same policy.
These settings control subsequent PDF exports and survive
saving and reopening the HTML workspace. Unchanged omitted settings and explicit
`off`/`false` values retain their original spelling.

The `createNativeSourceProject` and `readNativeSourceProject` APIs also preserve
these settings in `.fmd.json` projects. `exporter.renderProject(project.bytes)`
reopens that source, assets and policy using the exporter's explicitly supplied
trusted runtime; the project contains no executable bindings or WASM.

In **Document settings → PDF typesetting**, the **PDF format** selector offers
**Standard PDF**, **PDF/A-2b archival**, and **PDF/A-2b archival, strict**. Applying
a changed selection commits the profile and strictness together. Cancelling or
failing the settings preflight preserves the previous committed settings. The
HTML preview does not validate archival output; the native strict policy is
enforced when exporting the PDF.

The native renderer requires the archival binding's 38-argument ABI, or 39
arguments when a positive baseline grid is requested. Missing or older archival bindings
fail with `UNSUPPORTED_WASM_PACKAGE`; initialization failures in a saved page are
shown as native runtime errors. No path falls back to ordinary PDF. The separate
HTML-only `renderInteractiveHtml` API rejects `pdfA` and `pdfAStrict`, including
explicit `off` and `false`, because it cannot retain PDF export settings.

## Verification boundaries

`wasm/pdfa_api.test.mjs` executes the production facade with explicit generated
binding doubles to check ABI routing, complete request capture, capability
refusal and ordinary compatibility. `wasm/document_pdfa.test.mjs` uses real Node
worker threads, production transports and explicit native renderer doubles to
check single and atomic exports, ownership and receipt rejection. These tests
do not render PDFs or establish conformance. Native archival writer tests and
generated-WASM execution are separate verification steps.
