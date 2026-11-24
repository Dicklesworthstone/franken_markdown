// Compile-only public direct/worker API contract.
import { createRenderer, verifyPdf, type FmdPdfVerificationReport, type FmdPdfVerifyOptions } from "./franken_markdown.js";
import { createWorkerRenderer } from "./document-worker.js";

const options: FmdPdfVerifyOptions = {
  profile: "commonmark-gfm", font: "serif", fontScale: "lg", lang: "fr",
  page: { size: "a4", margins: { leftPt: 64 } }, baselineGridPt: 12,
  typography: { themeSpacing: true, lineHeight: 1.4 }, strictPagination: true,
  footnoteBacklinks: true, running: { footer: { right: "{page}/{pages}" } },
  pdfImages: [{ destination: "logo", bytes: new Uint8Array([1, 2]) }],
};
const report: FmdPdfVerificationReport = await verifyPdf("# Source", options);
const instanceReport: FmdPdfVerificationReport = await (await createRenderer()).verifyPdf("# Source", options);
const worker = createWorkerRenderer();
const workerReport: FmdPdfVerificationReport = await worker.verifyPdf("# Source", options,
  { signal: new AbortController().signal, timeoutMs: 5000 });
for (const value of [report, instanceReport, workerReport]) {
  const digest: string = value.digest;
  const pages: number = value.page_count;
  const unresolved: readonly string[] = value.anchors.unresolved;
  const text: string | undefined = value.pages[0]?.runs[0]?.text;
  void [digest, pages, unresolved, text];
}
worker.dispose();

// @ts-expect-error layout audits do not request archival certification
verifyPdf("source", { pdfA: "2b" });
// @ts-expect-error PDF/A strictness belongs to publication
worker.verifyPdf("source", { pdfAStrict: true });
// @ts-expect-error CSS is an HTML option
verifyPdf("source", { customCss: "body {}" });
// @ts-expect-error authoring profile choices remain canonical
verifyPdf("source", { profile: "commonmark" });
// @ts-expect-error verification controls cannot grant a PDF format discriminator
worker.verifyPdf("source", { format: "pdf" });
// @ts-expect-error reports preserve their native target
report.target = "html";
