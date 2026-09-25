# Review two Markdown revisions

Open `demo/review.html` in the assembled browser package, served under the same
HTTP/module-worker policy as the existing renderer demo. **Compare revisions**
is also linked from the demo. No renderer starts until Compare is requested.

Paste a before/after revision or explicitly select `.md`, `.markdown` or `.txt`
UTF-8 files. The two labels identify revisions, not filesystem paths. Imported
BOMs, CRLF and non-BMP Unicode remain exact until edited; edited text follows the
textarea's newline convention. **Swap revisions** swaps exact source and labels.
Both source downloads remain available after a renderer failure. File selection
never grants access to adjacent files or referenced image URLs.

**Compare revisions** runs the existing Rust semantic JSON and visual HTML APIs
in a dedicated worker. It does not introduce a JavaScript diff algorithm. The
summary reports block/word changes and *structural* similarity. Semantically
unchanged Markdown is not necessarily byte-identical; the report is not a merge,
source patch, PDF comparison or certification that all render differences have
been detected. The comparison uses the native default theme and accepts only
revision labels, not PDF settings, custom CSS, or additional image/font resources.

**Download comparison HTML** saves the native visual report with an additional
resource-denying content policy. The preview uses the same guarded HTML inside
an iframe without script or same-origin permission. External resources are not
fetched. The downloadable HTML includes no editor or WASM runtime.
**Download comparison JSON** preserves the semantic report and additive fields;
it is serialized from the direct API's parsed JSON, not a promise of retaining
the native JSON's original whitespace. Neither download rewrites either source.

Cancel, editing either source or label, swapping revisions, composition and page
suspension retire pending work and prepared results. Both captured revisions and
labels are checked before publication and again at download activation, including
silent edits without input events. An edit/undo round trip with input events
invalidates the old request too. Failed, stale, malformed or oversized comparisons
cannot publish only one of the two artifacts. A fresh explicit comparison owns a
new worker; no failed request is replayed on the main thread.

File reads are bounded, cancellable and revision-checked. Invalid UTF-8, excessive
size, an intervening edit or declined replacement leaves the previous source
intact. There is no autosave, browser storage, network upload, persistent handle,
implicit cross-tab transfer or in-place disk write. Download source before leaving;
a download request is not proof the browser or user saved it.

## Embedding the worker API

```js
import { createWorkerRenderer } from '@franken-suite/franken-markdown/document-worker';

const renderer = createWorkerRenderer();
try {
  const comparison = await renderer.compare(before, after, {
    oldName: 'Before editing', newName: 'Current draft'
  }, { signal: abortController.signal, timeoutMs: 30000 });
  console.log(comparison.report.stats);
  // The host decides how to display/save comparison.html or comparison.json.
} finally {
  renderer.dispose();
}
```

Each source is limited to 4 MiB UTF-8 and each label to 4 KiB. Both sources count
against the shared ingress budget (default 16 MiB). Report JSON is limited to
8 MiB; HTML and JSON together share `maxOutputBytes` (default/maximum 64 MiB).
These are boundary limits, not bounds on native temporary allocations. The two
existing native passes run sequentially, not through a newly shared AST cache.

The reusable API shares the document renderer's FIFO admission, queue deadlines
and individual queued cancellation. An in-flight abort or timeout terminates the
worker permanently and loses queued work without replay; explicitly create a new
renderer to continue. Missing comparison methods reject with
`UNSUPPORTED_WASM_PACKAGE` rather than falling back to a JavaScript differ.

## Verification

`node --test wasm/document_comparison.test.mjs` runs the production transport and
comparison validation in real Node worker threads with an explicit renderer double.
`python wasm/revision_review_browser.py --chromium /path/to/chromium` exercises the
workbench and module entry at a fully routed local test origin. Only the native
renderer module is a fixture; no external requests are made. Where managed policy
blocks that origin, `--transport content` explicitly loads the document content,
resolves production imports to Blob URLs and imports the worker entry from a
classic-worker bootstrap. That alternative does not establish hosted-module or
`file://` acceptance. Composition and page-transition events are synthetic, not
OS IME or real BFCache tests. Artifacts are retained in a new directory; no browser
or dependency is installed by the probe.

Both package builders also run the generated-WASM smoke test, which compares the
worker's JSON/HTML to the actual direct native API for changed, identical, empty,
Unicode and rich Markdown revisions. Passing renderer-double tests does not prove
that native gate; a matching rebuilt WASM package is required to execute it.
