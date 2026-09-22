import type { FlowSession, FlowEpubExportOptions, FlowExportResult } from "./flow.js";
import type { WorkerFlowSession, FlowEpubExportOptions as WorkerEpubOptions } from "./flow-worker.js";

declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
const options: FlowEpubExportOptions = { title: "Book", lang: "fr", toc: true, tocDepth: 2,
  customCss: "", darkMode: "disabled", maxOutputBytes: 1024 * 1024 };
const workerOptions: WorkerEpubOptions = options;
const a: Promise<FlowExportResult> = direct.exportDocument("epub", options, direct.token);
const b: Promise<FlowExportResult> = worker.exportDocument("epub", workerOptions, worker.token,
  { signal: new AbortController().signal });
void a; void b;
direct.exportDocument("html", { darkMode: "auto" }, direct.token);
worker.exportDocument("pdf", { pageNumbers: true, author: "Writer" }, worker.token);
// @ts-expect-error EPUB has no fixed paper page numbers.
direct.exportDocument("epub", { pageNumbers: true }, direct.token);
// @ts-expect-error Font payloads are a standalone renderEpub option, not session export options.
worker.exportDocument("epub", { fontAssets: [] }, worker.token);
// @ts-expect-error Raw HTML cannot be enabled on session export.
direct.exportDocument("epub", { allowRawHtml: true }, direct.token);
// @ts-expect-error Publication stylesheet overrides do not silently apply to PDF.
worker.exportDocument("pdf", { customCss: "" }, worker.token);
