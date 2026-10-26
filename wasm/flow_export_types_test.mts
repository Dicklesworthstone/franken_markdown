import type {
  FlowExportRequest,
  FlowExportResult,
  FlowHtmlExportOptions,
  FlowSession,
} from "@franken-suite/franken-markdown/flow";
import type {
  FlowPdfExportOptions,
  WorkerFlowSession,
} from "@franken-suite/franken-markdown/flow-worker";

declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
const html: FlowHtmlExportOptions = { title: "Document", toc: true, darkMode: "disabled", htmlFontFormat: "woff2" };
const pdf: FlowPdfExportOptions = {
  author: "Host",
  pageNumbers: true,
  metadataEpochSeconds: 0,
  maxOutputBytes: 1024,
  typography: { homogeneous: true, antiriver: false, pareto: true },
  optimalPagination: true,
  strictPagination: true,
  microtype: "expansion",
};
const a: Promise<FlowExportResult> = direct.exportDocument("html", html, direct.token);
const b: Promise<FlowExportResult> = worker.exportDocument("pdf", pdf, worker.token, {
  signal: new AbortController().signal,
});
const batch: readonly FlowExportRequest[] = [
  { format: "html", options: html },
  { format: "pdf", options: pdf },
];
const c: Promise<readonly FlowExportResult[]> = direct.exportDocuments(batch, direct.token);
const d: Promise<readonly FlowExportResult[]> = worker.exportDocuments(batch, worker.token, {
  timeoutMs: 10_000,
});
void [a, b, c, d];
// @ts-expect-error PDF-only options cannot be attached to an HTML batch request
worker.exportDocuments([{ format: "html", options: { pageNumbers: true } }], worker.token);
// @ts-expect-error supported formats are explicit
worker.exportDocument("book-pdf", {}, worker.token);
// @ts-expect-error no remote custom HTML, code, or asset injection
worker.exportDocument("html", { allowRawHtml: true }, worker.token);
// @ts-expect-error raw source is not an export token
direct.exportDocument("pdf", {}, "source");
// @ts-expect-error PDF-only option must not be silently ignored by HTML
worker.exportDocument("html", { pageNumbers: true }, worker.token);
// @ts-expect-error PDF-only typography cannot be silently ignored by HTML.
worker.exportDocument("html", { typography: { pareto: true } }, worker.token);
// @ts-expect-error HTML font-container selection does not apply to PDF.
worker.exportDocument("pdf", { htmlFontFormat: "woff2" }, worker.token);
// @ts-expect-error Native typography flags are booleans.
worker.exportDocument("pdf", { typography: { pareto: "true" } }, worker.token);
// @ts-expect-error EPUB does not use HTML font-container selection.
worker.exportDocument("epub", { htmlFontFormat: "woff2" }, worker.token);
