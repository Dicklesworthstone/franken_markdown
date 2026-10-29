import { renderPdf, renderBookPdf, type FmdPdfRenderOptions } from "./franken_markdown.js";
import { createWorkerRenderer, type DocumentPublicationTarget } from "./document-worker.js";
import type { FlowSession, FlowPdfExportOptions } from "./flow.js";

const options: FmdPdfRenderOptions = { pdfA: "2b", pdfAStrict: true, baselineGridPt: 13.125,
  strictPagination: true, typography: { pareto: true }, page: { size: "a4", margins: 36 } };
void renderPdf("# Report", options).then(output => {
  const profile: "2b" | undefined = output.pdfA;
  const strict: boolean | undefined = output.pdfAStrict;
  return [profile, strict];
});
const worker = createWorkerRenderer();
void worker.renderPdf("# Report", options);
const edition: DocumentPublicationTarget[] = [
  { format: "html" }, { format: "pdf", options }, { format: "epub" },
];
void worker.publish("# Report", edition);
declare const flow: FlowSession;
const flowOptions: FlowPdfExportOptions = { pdfA: "2b", pdfAStrict: true, baselineGridPt: 12 };
void flow.exportDocument("pdf", flowOptions, flow.token);
void flow.exportDocuments([{ format: "html" }, { format: "pdf", options: flowOptions }], flow.token);
// @ts-expect-error Only the implemented archival profile can be requested.
void renderPdf("source", { pdfA: "3b" });
// @ts-expect-error Strictness is a boolean, not a form-control string.
void worker.renderPdf("source", { pdfA: "2b", pdfAStrict: "true" });
// @ts-expect-error HTML cannot request PDF archival policy.
void worker.renderHtml("source", { pdfA: "2b" });
// @ts-expect-error EPUB cannot request PDF archival policy.
void flow.exportDocument("epub", { pdfA: "2b" }, flow.token);
// @ts-expect-error Chapter-aware archival publication uses the ./book entry.
void renderBookPdf([{ path: "one.md", source: "# One" }], { pdfA: "2b" });
