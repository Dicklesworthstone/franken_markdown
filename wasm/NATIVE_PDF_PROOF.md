# Native PDF proof sessions

`createNativePdfProof` in `native_pdf_proof.mjs` retains one explicitly requested
PDF from the native workspace export worker. A proof is the actual PDF output,
not browser printing or a screenshot of the HTML preview. The same immutable
Blob can be displayed and downloaded without re-rendering or changing a
render-time metadata timestamp.

```js
import {createNativePdfProof} from './native_pdf_proof.mjs';

const proof = createNativePdfProof(runtime, () => ({
  source: currentLosslessMarkdown(),
  settings: runtime.settings,
  resources: runtime.documentRevision,
}));
const result = await proof.render();
// result.blob: application/pdf; result.size; result.diagnostics: plain strings.
// The host owns any object URL it creates, and must revoke it when retired.
```

The snapshot callback is a trusted, side-effect-free host function. Source is
captured exactly. Settings must have immutable identity, and `resources` must
change whenever relevant resources change. `documentRevision` covers committed
settings, images and fonts, including rollback; preview reads and viewing zoom
do not change it. Staging a draft does not change committed document state.

`render()` reuses a retained current proof; a second pending request rejects with
`PROOF_BUSY`. There is no queue, automatic retry, UI-thread renderer fallback,
new parser, font engine or additional parallel WASM instance. Native proofing
occupies the same bounded document-operation slot as exports and analyses.

Call `invalidate()` on edits, source replacement, composition and lifecycle
changes, including edit/undo round trips. The session also compares live source,
settings and resource state before admitting a result or returning `current`.
It never advertises a mismatched proof as current. `cancel()` immediately settles
the pending proof promise; `dispose()` also releases the retained proof and
permanently closes the session. Hosts must separately remove viewers and release
URLs; a Blob already given to a caller cannot be revoked by a JavaScript API.

## Owned native export handles

`runtime.beginExportDocument(format, source, isCurrent)` returns a frozen
`{promise, cancel}` handle for the existing native HTML/PDF/EPUB/SVG export path.
Admission is synchronous, with the same busy/unsupported/suspended errors as
`exportDocument`. The promise has the existing output contract. `cancel()` returns
true only when this exact operation still owns the native slot. Calling an old
handle after completion, failure or replacement cannot cancel a newer export or
analysis. Existing `exportDocument` and global `cancelExport` remain compatible.

The workspace and matching exporter must be rebuilt to carry updated runtime
code into newly generated standalone files. Older exported files do not acquire
new capabilities automatically.

## Verification

```sh
node --test wasm/native_pdf_proof.test.mjs
```

Tests execute the production proof session and native bootstrap/resource
renderer. Deferred worker and DOM/URL adapters cover cancellation, shared-slot
ownership, exact source/byte views, resource revision tracking, immutable output,
late results, busy admission and serialized execution. They do not certify Rust
compilation, font shaping, PDF visual quality or browser PDF viewer support.
