# Verify the PDF layout you configured

`verifyPdf` exposes the complete native PDF verification pipeline through the
browser facade and document worker. It accepts the same page, typography,
authoring-profile, running-content, image, and font settings used for PDF export.
The report includes the resulting pages and text runs, internal-anchor checks,
parser findings, render warnings, overflow findings, and a deterministic digest.

```js
import { createRenderer } from "@franken-suite/franken-markdown";

const renderer = await createRenderer(wasmBytes);
const options = {
  profile: "commonmark-gfm",
  font: "serif",
  baseFontSize: 12,
  page: { size: "a4", margins: { leftPt: 64, rightPt: 64 } },
  typography: { themeSpacing: true, lineHeight: 1.4 },
  footnoteBacklinks: true,
  running: { footer: { right: "{page} / {pages}" } },
};

const report = await renderer.verifyPdf(markdown, options);
console.log(report.verdict, report.page_count, report.anchors.unresolved);
for (const finding of report.findings) {
  console.log(finding.code, finding.detail);
}

const publication = await renderer.renderPdf(markdown, options);
```

The direct `verifyPdf(markdown, options)` export is also available. Verification
and publication are separate native passes. Each call owns its captured source,
settings, and exact image/font byte ranges before asynchronous initialization
yields. Keep the source and resources unchanged between calls when comparing
their results; there is no shared parse cache or combined audit/export operation.

## Use a worker for document-sized layouts

```js
import { createWorkerRenderer } from "@franken-suite/franken-markdown/document-worker";

const renderer = createWorkerRenderer({
  maxOutputBytes: 4 * 1024 * 1024,
  timeoutMs: 15_000,
});

try {
  const report = await renderer.verifyPdf(markdown, options, {
    signal: abortController.signal,
  });
  console.log(report.pages.map(page => ({
    number: page.number,
    text: page.runs.map(run => run.text).join("\n"),
  })));
} finally {
  renderer.dispose();
}
```

Verification shares the document worker's queue, ingress accounting, deadlines,
and cancellation protocol. It returns the same report shape as the direct API.
Queued requests can be cancelled independently. Cancelling a request already
running in WASM terminates the owned worker and closes that renderer; create a
new renderer for later work. Host buffers are copied, never transferred away.
The report crosses the worker boundary as an exact owned UTF-8 JSON buffer and
is checked again against its source length and selected profile before delivery.

## Verify a persistent Flow session

Flow editing sessions expose the same configured verifier through
`session.verifyPdf(options, token)`. The result wraps the complete native report
with the captured source/layout revisions, source byte length, session font and
authoring profile, and image counts. The same method is available on a
`WorkerFlowSession`, with optional cancellation and deadline controls.

```js
import { createWorkerFlowSession } from "@franken-suite/franken-markdown/flow-worker";

const session = await createWorkerFlowSession(markdown, {
  profile: "commonmark-gfm",
  font: "serif",
});
const printSettings = {
  page: { size: "a4", margins: { leftPt: 64, rightPt: 64 } },
  typography: { themeSpacing: true, lineHeight: 1.4 },
  running: { footer: { right: "{page} / {pages}" } },
};

try {
  const token = session.token;
  const checked = await session.verifyPdf(printSettings, token, { timeoutMs: 15_000 });
  console.log(checked.revision, checked.report.verdict, checked.report.digest);
  const publication = await session.exportDocument("pdf", printSettings, token);
  // Both calls acknowledge this exact source/layout token or reject as stale.
} finally {
  session.dispose();
}
```

For synchronous editing on the current thread, use `createFlowSession` from
`@franken-suite/franken-markdown/flow` and omit the third control argument.
Verification itself remains asynchronous. It shares the direct session's
publication slot: overlapping verification/export calls reject with
`EXPORT_BUSY`; worker sessions serialize them through their owned queue.

Flow captures the retained original Markdown, its immutable grammar and font,
and the complete authorized image inventory, including images outside the
current viewport. Explicit `pdfImages` can supply running-band logos absent from
Markdown. Their exact byte ranges and nested page/typography/running settings
are copied before asynchronous work or enqueue returns. Resolve pending session
images before verification; unresolved images and conflicting payloads for the
same destination fail exactly as they do for Flow PDF export. No image URL is
fetched. This is stricter than the stateless renderer's missing-image findings.

Keep both revisions unchanged between verification and export. Source edits,
asset completion or reflow invalidate the token; verification neither updates
the session nor retries against newer source. A direct verification also rejects
if the session changes or is disposed while its renderer is pending. The report
is owned by its caller and does not share mutable backend storage.

`supportsPdfVerification` reports adapter support. Older worker adapters report
`false` and reject verification locally while their existing editing/export
methods remain usable. A matching generated WASM package is still required when
the adapter reports `true`; a missing configured verifier fails explicitly.

Flow retains its 4 MiB UTF-8 source, 1024-image, 8 MiB per-image and 32 MiB combined
image limits. `maxOutputBytes` in `FlowPdfVerificationOptions` bounds the entire
report as escaped UTF-8 JSON, including additional native fields; its default and
maximum are 8 MiB. Report nesting beyond 64 levels is refused.
This limit is separate from PDF publication byte limits and is not a native heap
ceiling. A report over budget is rejected whole, and the session remains usable.

Worker requests charge explicit assets to the existing queue budget before
copying them. The report crosses the boundary in one exact owned UTF-8 buffer;
the receiver validates its schema, profile, font and captured revisions before
accepting state or starting queued edits. Cancelling queued verification leaves
the active request alone. Cancelling or timing out verification already running
terminates the worker, loses that session and rejects queued work without replay.

## Report and option semantics

The native report retains schema version `"1"` and target `"pdf"`:

- `verdict` is `"clean"` or `"findings"`.
- `page_count` and `pages` describe the measured layout. Each numbered page
  contains `text`, `x`, `y`, `size`, and `kind` runs in points.
- `anchors.resolved` and `anchors.unresolved` describe internal destinations.
- `findings` retains native codes and details, including parser degradation with
  original source byte offsets.
- `digest` is the native FNV-1a change detector for the canonical report. It is
  not a cryptographic signature. An explicit `profile` is acknowledged as an
  additional property without changing that digest.

Running bands use the same margin and supplied-image admission as publication.
They are absent from the verifier's body text-run list. `fitToPages` uses the
same fitting solver as browser export and reports the actual resulting count;
it does not add a separate hard page-count policy. Baseline grids cannot be
combined with a page-fit target. Strict-pagination errors retain their native
`PAGINATION_*` code and reason; verification never retries with weaker settings.

Explicit title, author, language, and TOC settings override source frontmatter
using the same precedence as browser PDF export. Missing assets remain native
render findings; the API does not fetch URLs or access filesystem paths.

This is a native layout and authoring audit. It neither emits nor independently
decodes a PDF file and does not certify PDF/A or PDF/UA conformance. Accordingly,
`pdfA`, `pdfAStrict`, CSS, HTML font embedding, and other-format options are
rejected. The existing `accessibilityAudit` remains the original default-layout
authoring subset and keeps its existing grammar-only options.

## Bounds and package compatibility

Stateless direct calls admit at most 32 MiB of UTF-8 source, 4096 image entries, five font
slots, 32 MiB per resource, and 128 MiB of combined image/font bytes. Image
destinations are unique, at most 8 KiB each and 64 KiB combined. Explicit or
inherited title, author, and language values are bounded to 4096 UTF-8 bytes.

Document workers retain their tighter publication limits: 4 MiB of UTF-8 source, 1024
images, 8 MiB per image, 32 MiB per font, and the configured queue byte budget.
The complete report is bounded to 8 MiB of UTF-8 JSON; a document worker also applies its
`maxOutputBytes`, whichever is smaller. An over-budget report fails as a whole.

The facade requires a matching generated WASM package with the 38-argument
`verifyPdfConfigured` export. Older or incomplete packages fail explicitly with
`UNSUPPORTED_WASM_PACKAGE` before initialization or payload copying. They do not
fall back to a default-layout or accessibility-only report. A source checkout
alone does not update an existing generated package.

## Regression coverage

Rust tests under `src/wasm_abi/pdf_page/verify/tests.rs` compare the complete
report with the native verifier using configured resources, profiles, page
fitting, grids, running bands, parser diagnostics, and strict failures.

`pdf_verification_api.test.mjs` compares verification's option conversion with
the production PDF export route using generated-binding doubles.
`pdf_verification_worker.test.mjs` uses real worker threads and an explicit
renderer double to check ownership, protocol, cancellation, and budgets.

`flow_pdf_verification.test.mjs` and `flow_pdf_verification_worker.test.mjs`
cover persistent-session capture, revisions, complete assets, report ownership,
byte budgets, capability negotiation, cancellation and malformed responses using
explicit native-session doubles. `flow_pdf_verification_types_test.mts` covers
the direct/worker declaration surface. These JavaScript and declaration tests do
not substitute for generated-WASM execution.

```sh
node --experimental-vm-modules --test --test-concurrency=1 \
  wasm/pdf_verification_api.test.mjs wasm/pdf_verification_worker.test.mjs \
  wasm/flow_pdf_verification.test.mjs wasm/flow_pdf_verification_worker.test.mjs
```

With TypeScript installed, check the direct and worker API declarations:

```sh
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext \
  --lib ES2022,DOM,DOM.Iterable wasm/flow_pdf_verification_types_test.mts
```
