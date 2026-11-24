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

Direct calls admit at most 32 MiB of UTF-8 source, 4096 image entries, five font
slots, 32 MiB per resource, and 128 MiB of combined image/font bytes. Image
destinations are unique, at most 8 KiB each and 64 KiB combined. Explicit or
inherited title, author, and language values are bounded to 4096 UTF-8 bytes.

Workers retain their tighter publication limits: 4 MiB of UTF-8 source, 1024
images, 8 MiB per image, 32 MiB per font, and the configured queue byte budget.
The complete report is bounded to 8 MiB of UTF-8 JSON; a worker also applies its
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
renderer double to check ownership, protocol, cancellation, and budgets. These
JavaScript tests do not substitute for generated-WASM execution.

```sh
node --experimental-vm-modules --test --test-concurrency=1 \
  wasm/pdf_verification_api.test.mjs wasm/pdf_verification_worker.test.mjs
```
