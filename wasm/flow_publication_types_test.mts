import type { FlowSession, FlowExportResult, FlowSvgExportOptions } from "./flow.js";
import type { WorkerFlowSession, FlowSvgExportOptions as WorkerSvgOptions } from "./flow-worker.js";
declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
const options: FlowSvgExportOptions = { maxWidthPt: 360, maxOutputBytes: 1024 };
const workerOptions: WorkerSvgOptions = options;
const a: Promise<FlowExportResult> = direct.exportDocument("svg", options, direct.token);
const b: Promise<FlowExportResult> = worker.exportDocument("svg", workerOptions, worker.token, {
  signal: new AbortController().signal, timeoutMs: 5000,
});
void Promise.all([a, b]).then(results => {
  for (const result of results) {
    const possibleMime: FlowExportResult["mimeType"] = "image/svg+xml";
    const reason: string | undefined = result.diagnostics[0]?.code;
    const scope: "document" | undefined = result.diagnostics[0]?.scope;
    void [possibleMime, reason, scope];
  }
});
// @ts-expect-error SVG is not an HTML TOC export.
direct.exportDocument("svg", { toc: true }, direct.token);
// @ts-expect-error Paper is a PDF concern, not a poster-width option.
worker.exportDocument("svg", { pageNumbers: true }, worker.token);
// @ts-expect-error Assets come from the captured session, never options.
direct.exportDocument("svg", { pdfImages: [] }, direct.token);
// @ts-expect-error Widths are numeric points, not CSS strings.
worker.exportDocument("svg", { maxWidthPt: "360px" }, worker.token);
// @ts-expect-error PDF does not accept the SVG width option.
worker.exportDocument("pdf", { maxWidthPt: 360 }, worker.token);
