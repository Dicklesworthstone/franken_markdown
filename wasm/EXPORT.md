# Revision-bound HTML and PDF export

Flow sessions export complete documents through the existing HTML/PDF renderer.
The input is the session's original Markdown, its bundled font preset and its
host-authorized encoded image payloads. No Canvas screenshot, DOM-to-PDF library,
second JavaScript parser, remote asset fetch or browser print dialog is involved.

```js
import { createWorkerFlowSession } from "@franken-suite/franken-markdown/flow-worker";

const session = await createWorkerFlowSession("# Report\n\nOriginal **Markdown**.");
try {
  const document = await session.exportDocument("pdf", {
    title: "Report", pageNumbers: true, metadataEpochSeconds: 0
  }, session.token);
  const blob = new Blob([document.bytes], { type: document.mimeType });
  // The host owns presenting/saving this Blob and releasing any object URL.
  // Inspect document.diagnostics before treating a render as publication-ready.
} finally {
  session.dispose();
}
```

The same `exportDocument(format, options, token)` method exists on synchronous
sessions, but it returns a Promise. That Promise does NOT make synchronous WASM
execution nonblocking: use a worker session for UI applications. Worker exports
are serialized with edits, reflow and asset delivery. A request queued behind an
edit keeps its original token and fails stale rather than changing the requested
source. The returned data-only record includes the source/layout token, format,
MIME type, owned bytes, original source byte length, asset counts and diagnostics.

## Fidelity and options

This is document export, not pixel parity with the live flow layout. It uses the
shared document renderer's paragraph, page, table and typography implementation.
The session's `sans`/`serif` preset is retained; viewport width and Canvas pixel
sizes are not converted into invented PDF page geometry. PDF metadata defaults
to epoch zero for repeatable bytes; supply a nonnegative safe integer timestamp
to use another date. Raw HTML is always escaped on this entry point. Custom CSS,
font injection, ambient URL loading and arbitrary renderer settings are not
accepted. Use the separately documented general render API for other policies.

Both formats accept title, language, TOC/depth and a lowered `maxOutputBytes`.
HTML also accepts dark mode. PDF accepts author, page/line numbers, body/table
sizes, heading scale, a page target and the existing protrusion setting. Unknown,
wrong-format and coerced settings are errors, not silently ignored preferences.
The TypeScript declarations expose format-specific option types.

## Image authority and completeness

Every flow image must be resolved AND have encoded bytes retained in the native
session by `provideAsset({ ..., bytes })`. A dimensions-only Canvas bitmap is not
an exportable asset. Missing bytes fail with `UNRESOLVED_EXPORT_ASSET`; exports do
not fetch, invent placeholders or silently omit that image. After a source edit
or asset reload, authorize and supply the new generation before exporting again.
The existing `reuseAssets` option remains an explicit host attestation, not an
automatic export optimization.

Assets are addressed by exact Markdown destination in the shared renderer. Equal
payloads for repeated destinations are deduplicated. Different payloads for the
same destination fail with `AMBIGUOUS_EXPORT_ASSET`, since the renderer cannot
represent that occurrence-specific distinction honestly. The native renderer's
image-format capabilities and diagnostics still apply; successful admission of
bytes does not certify that they decode or that the exported document is perfect.
Opening an exported HTML document is a separate host action, not sandboxed viewing.

## Lifecycle and bounds

The adapter captures and owns all input before awaiting the renderer, then checks
the token again before publishing output. A concurrent edit/reflow, asset change
or disposal makes the delayed result unusable. It retains one physical export
slot per session; another direct call fails `EXPORT_BUSY` until that slot settles.
Worker queue admission, deadlines and cancellation use the existing RPC rules:
queued cancellation removes the request, while in-flight cancellation terminates
the owned worker and loses the session. Never assume rollback or automatic replay;
retain authoritative Markdown in the host and restart explicitly when necessary.

Limits are 4 MiB source (existing flow admission), 500,000 scanned display items,
1,024 image occurrences, 8 MiB per image, 32 MiB total unique image payloads and
64 Ki UTF-16 destination units. Total image bytes inspected, including duplicate
occurrences, are separately capped at 64 MiB to bound copy/comparison work. Result diagnostics are bounded to 1,024 entries
and 64 Ki UTF-16 message units. Published output defaults to 64 MiB and may be
limited further per call. These limits bound export admission/copying/publication,
NOT every temporary allocation, native render duration or the whole WASM heap.
The output cap is checked after native rendering and before worker transfer.

## Verification

```sh
node --test wasm/flow_export.test.mjs wasm/flow_export_worker.test.mjs
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext wasm/flow_export_types_test.mts
scripts/check-wasm-package.sh export
```

The Node tests execute the actual adapter/protocol/transport, including real Node
worker threads and transfer lists, with explicitly identified native-core doubles.
The package gate separately runs `flow_export_smoke.mjs` against generated WASM:
it compares synchronous and worker HTML/PDF bytes with the general render API,
including real PNG embedding, source edits, stale tokens and diagnostics. A test
of doubles is not a substitute for that generated-artifact gate.

## Live editor and image payload retention

`demo/flow-canvas.html` now has **Prepare HTML**, **Prepare PDF**, and a separate
**Download** link. Preparing a document does not save or open it automatically.
The controller requires the actual textarea source to match the successfully
applied source and displayed revision, and refuses an active image batch. Source
edits, session restart, changed image authorization, new geometry or disposal
invalidate old output. Typing also revokes the download before the animation-frame
preview update; programmatic source changes are checked again on link activation.
Scrolling alone preserves an already prepared download. At most one physical
export and one Blob URL are retained; replaced/disposed URLs are revoked.

The demo opts into `FlowImageAssets({ ..., retainSourceBytes: true })` (passed as
the second constructor argument). This keeps the exact admitted immutable PNG/JPEG
bytes in the native session as well as the Canvas bitmap. The default remains
`false`, so existing Canvas-only applications keep dimension-only delivery.
Copies are serialized with native publication, and authorization is checked again
after the Blob copy. Native per-payload/aggregate budgets and export budgets remain
independent; a native payload rejection leaves that image unpublished. Image
statistics count admitted input payloads/bitmap pixels, not extra native or worker
copies and not total browser memory. The host bitmap cache retains no encoded copy.

`clear()` and `dispose()` on the image manager do not own or dispose the session.
After revoking authorization, also call the session's `reloadAssets` or recreate
it before any export; discarding Canvas bitmaps alone cannot revoke native bytes.
The demo always recreates the session when the local-file grant changes.

```sh
node --test wasm/tests/flow_assets_export.test.mjs wasm/tests/flow_preview_export.test.mjs wasm/tests/flow_export_controls.test.mjs
```

These tests exercise production image admission, export gating and download
controls with native/decoder/element doubles and real EventTarget/Blob objects.
They do not constitute browser-download, image-codec or generated-WASM acceptance.
