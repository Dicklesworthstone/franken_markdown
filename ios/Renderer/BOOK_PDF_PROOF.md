# Proof the actual book PDF before publishing

The assembled `demo/book.html` publisher has a **Review the actual book PDF**
section. Choose **Generate book PDF proof**, inspect it using the browser's
embedded PDF viewer, then **Download this PDF proof**. The viewer and download
use the same immutable Blob. Repeating generation on the unchanged book reuses
that PDF instead of rendering again or changing captured metadata timestamps.

This calls the existing `renderBookPdf` path through a dedicated book worker.
It is not browser printing, HTML pagination, an alternate Markdown renderer, or
a second PDF engine. The captured collection includes ordered chapters, shared
include sources, presentation settings, image destinations/bytes, and supplied
font slots/weight pins. Current editor text is captured before rendering; source
validation, original line endings and BOM preservation remain collection-owned.

The existing HTML-site reader and ordinary PDF/EPUB/site export controls stay
independent. They have their own workers and download links. **Cancel PDF
rendering** terminates only the proof operation. **Clear PDF proof** releases its
bytes and object URL; neither operation changes source or resource authorizations.
Proofing never saves, uploads, or persists a file automatically.

## Stale work and memory ownership

Collection revisions and raw editor checkpoints fence capture, result retention,
viewer installation, and download activation. Source, source-role, chapter order,
settings, image and font changes retire old proofs. Import and text composition
block proof generation and cancel pending work. Silent editor changes that did
not emit an input event are still checked before result retention and download.

Each job owns an AbortSignal. An obsolete job cannot cancel a newer proof or an
unrelated export, and its late result cannot replace current output. Cancellation
settles the host promise even if a defective adapter ignores abort. The production
book worker terminates synchronous Rust work when its signal is aborted.

Suspension and disposal release viewing, downloads and retained proof bytes.
Returning through the browser back/forward cache does not regenerate the proof or
restore image/font permission. Initialization failure retires the dedicated
client without preventing the remaining workbench features from loading.

A proof retains at most 128 MiB of PDF output, matching the existing worker
ceiling. The worker's existing two-minute deadline and source/resource limits
remain in force. The retained Blob copies only the returned Uint8Array view;
subsequent mutation of a worker-result backing buffer cannot retarget a download.
These are retained-output/admission bounds, not whole-process heap guarantees.

## Viewer compatibility and verification boundaries

Viewing uses an `object` with `type="application/pdf"` and an owned Blob URL.
A missing native viewer, restrictive hosting policy or blank plugin surface
still leaves **Download this PDF proof** available. No remote viewer, external
PDF library, or network service is loaded. The host browser controls navigation
and interaction inside its native PDF viewer. The restricted HTML-site iframe's
sandbox and content-security policy are not changed.

The feature checks the worker result envelope, byte limit and PDF header, not
PDF structural validity, pagination quality or accessibility conformance. It does
not infer page counts from HTML or treat a plugin load event as visual proof.
The book worker currently returns output bytes and source length, not a complete
PDF-warning inventory. A successful proof is not a claim that all resources were
supported without fallback. Inspect the downloaded artifact and use the native
engine's verification facilities where appropriate.

## Embedding and tests

`createBookPdfProof` in `book_pdf_proof.mjs` owns a dedicated `createBookWorker()`
client. Supply the book `collection` and editor `controls`. `render({signal})`
returns a frozen `{blob, size, filename, chapters, sourceLength, revision}`.
Read `current` before using retained output; it is null after invalidation.
`cancel`, `invalidate`, `suspend`, `resume`, `subscribe` and `dispose` manage the
lifetime. Object URL ownership remains with the host. These are workbench helper
modules, not new top-level package exports.

```sh
node --test wasm/tests/book_pdf*.test.mjs
CHROMIUM=/usr/bin/chromium python wasm/book_pdf_proof_browser.py
```

Node tests execute the production proof session/controller with explicitly named
source/editor, DOM and worker adapters. Package checks verify both assembly copy
lists, load the staged helper graph, and execute the unchanged publisher bootstrap
with isolated dependency adapters to check worker ownership and lifecycle wiring.
These checks do not replace the full generated-WASM package gate.

The Chromium harness uses the production proof modules, real DOM, Workers and
Blob downloads with an independent two-page ReportLab fixture. It requires
Playwright and ReportLab and retains artifacts in a fresh temporary directory.
Its worker transport and source host are explicit adapters; it does not render
Markdown through compiled WASM. A screenshot of the surrounding controls or a
blank embedded plugin is not native page-rendering acceptance evidence.
