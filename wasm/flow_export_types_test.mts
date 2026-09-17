import type { FlowSession, FlowExportResult, FlowHtmlExportOptions } from "@franken-suite/franken-markdown/flow";
import type { WorkerFlowSession, FlowPdfExportOptions } from "@franken-suite/franken-markdown/flow-worker";
declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
const html: FlowHtmlExportOptions = { title: "Document", toc: true, darkMode: "disabled" };
const pdf: FlowPdfExportOptions = { author: "Host", pageNumbers: true, metadataEpochSeconds: 0, maxOutputBytes: 1024 };
const a: Promise<FlowExportResult> = direct.exportDocument("html", html, direct.token);
const b: Promise<FlowExportResult> = worker.exportDocument("pdf", pdf, worker.token, { signal: new AbortController().signal });
void [a, b];
// @ts-expect-error supported formats are explicit
worker.exportDocument("svg", {}, worker.token);
// @ts-expect-error no remote custom HTML, code, or asset injection
worker.exportDocument("html", { allowRawHtml: true }, worker.token);
// @ts-expect-error raw source is not an export token
direct.exportDocument("pdf", {}, "source");
// @ts-expect-error PDF-only option must not be silently ignored by HTML
worker.exportDocument("html", { pageNumbers: true }, worker.token);
