# Native PDF proofing

## Authoring workflow

Newly generated native workspaces include **PDF proof**. Open it and choose
**Generate PDF proof** to request the actual configured native PDF in the existing
background export worker. The dialog shows renderer diagnostics as literal text
and hands a private Blob URL to the browser’s embedded PDF viewer. **Download
this PDF** saves those exact bytes, not a new render or a print of the HTML.
Repeated generation of an unchanged retained proof reuses the same immutable
bytes, including metadata timestamps. Closing the dialog releases that proof.

The proof uses committed typography, metadata, paper/margins, fonts and images.
Viewing zoom and unapplied settings drafts do not change it. Source comes from
the production controller’s lossless anchor, preserving imported BOM/CRLF even
though the textarea displays normalized line endings.

**Cancel rendering** stops only this operation. Source/resource/settings changes,
text composition, closing/Escape and page suspension retire the proof and revoke
its viewer URL. Silent changes are rechecked before displaying or downloading.
No automatic retry, implicit publication, source edit or additional parallel
engine occurs. Busy/failing native operations remain visible and can be retried
explicitly.

Save HTML and recovery copies strip temporary proof controls, viewers, URLs and
results. Reopening rebuilds fresh controls after parsing, preserving the editable
source and committed resources rather than a stale PDF. The dialog is hidden
when browser printing.

Embedded viewing depends on browser PDF support and policy. A blank/unavailable
viewer is not treated as rendering success: the dialog always offers an exact
PDF download for inspection in a separate viewer. No PDF.js, network service,
extra parser, or PDF/UA claim is introduced. The parent workspace policy permits
local Blob objects/frames for the browser viewer; the Markdown preview retains
its separate script-free, resource-restricted sandbox and CSP.

Rebuild both the matching WASM shell (including the updated controller) and the
standalone exporter to include the controls. Old exported files are unchanged.
Older shells without the optional proof host remain usable but do not gain the
new dialog. Both package assemblers and the npm inventory carry the new modules.

## Session API

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
node --test wasm/native_pdf_proof.test.mjs wasm/native_pdf_proof_package.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/native_pdf_proof_browser.py
```

Tests execute the production proof session and native bootstrap/resource
renderer. Deferred worker and DOM/URL adapters cover cancellation, shared-slot
ownership, exact source/byte views, resource revision tracking, immutable output,
late results, busy admission and serialized execution. They do not certify Rust
compilation, font shaping, PDF visual quality or browser PDF viewer support.


The browser harness runs the production controller, exporter, bootstrap, resource
transactions and real Chromium Workers. Explicit native ABI and shell adapters
record PDF arguments and return a minimal PDF fixture; these are not a compiled
Rust/WASM render or font parser. It checks exact byte reuse, source imports and
undo, save/reopen, stale downloads, cancellation/failure, resource changes,
composition/lifecycle cleanup, viewer fallback and CSP/network isolation. Package
checks load an isolated declared import closure, not the complete release build.
The harness does not certify page rasterization in the embedded browser viewer.
