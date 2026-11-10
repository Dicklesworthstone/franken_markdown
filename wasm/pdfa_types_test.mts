import { renderPdf, renderBookPdf, type FmdPdfRenderOptions } from "./franken_markdown.js";
import { createWorkerRenderer, type DocumentPublicationTarget } from "./document-worker.js";
import type { FlowSession, FlowPdfExportOptions } from "./flow.js";

const options: FmdPdfRenderOptions = { pdfA: "2b", pdfAStrict: true, baselineGridPt: 13.125,
  strictPagination: true, footnoteBacklinks: true, typography: { pareto: true }, page: { size: "a4", margins: 36 } };
void renderPdf("# Report", options).then(output => {
  const profile: "2b" | undefined = output.pdfA;
  const strict: boolean | undefined = output.pdfAStrict;
  return [profile, strict];
});
const worker = createWorkerRenderer();
void worker.renderPdf("# Report", options);
void worker.renderPdf("# Report", { footnoteBacklinks: false });
void renderPdf("# Report", { footnoteBacklinks: false });
void renderBookPdf([{ path: "one.md", source: "# One" }], { footnoteBacklinks: true });
void renderBookPdf([{ path: "one.md", source: "# One" }], { footnoteBacklinks: false });
void renderPdf("# Report", { splitFootnotes: true });
void worker.renderPdf("# Report", { splitFootnotes: false });
void renderBookPdf([{ path: "one.md", source: "# One" }], { splitFootnotes: true });
const edition: DocumentPublicationTarget[] = [
  { format: "html" }, { format: "pdf", options }, { format: "epub" },
];
void worker.publish("# Report", edition);
declare const flow: FlowSession;
const flowOptions: FlowPdfExportOptions = { pdfA: "2b", pdfAStrict: true, footnoteBacklinks: true, baselineGridPt: 12 };
void flow.exportDocument("pdf", flowOptions, flow.token);
void flow.exportDocument("pdf", { footnoteBacklinks: false }, flow.token);
void flow.exportDocument("pdf", { splitFootnotes: true }, flow.token);
void flow.exportDocuments([{ format: "html" }, { format: "pdf", options: flowOptions }], flow.token);
// @ts-expect-error Only the implemented archival profile can be requested.
void renderPdf("source", { pdfA: "3b" });
// @ts-expect-error Strictness is a boolean, not a form-control string.
void worker.renderPdf("source", { pdfA: "2b", pdfAStrict: "true" });
// @ts-expect-error Backlinks require a boolean, not a form-control string.
void renderPdf("source", { footnoteBacklinks: "true" });
// @ts-expect-error Worker PDF options retain the same boolean contract.
void worker.renderPdf("source", { footnoteBacklinks: "true" });
// @ts-expect-error Flow PDF options retain the same boolean contract.
void flow.exportDocument("pdf", { footnoteBacklinks: "true" }, flow.token);
// @ts-expect-error Continuation requires a boolean at every PDF entry point.
void renderPdf("source", { splitFootnotes: "true" });
// @ts-expect-error Worker continuation does not coerce form-control strings.
void worker.renderPdf("source", { splitFootnotes: "true" });
// @ts-expect-error Flow continuation does not coerce form-control strings.
void flow.exportDocument("pdf", { splitFootnotes: "true" }, flow.token);
// @ts-expect-error HTML cannot silently discard PDF continuation settings.
void worker.renderHtml("source", { splitFootnotes: false });
// @ts-expect-error HTML cannot silently discard PDF backlink settings.
void worker.renderHtml("source", { footnoteBacklinks: false });
// @ts-expect-error HTML cannot request PDF archival policy.
void worker.renderHtml("source", { pdfA: "2b" });
// @ts-expect-error EPUB cannot request PDF archival policy.
void flow.exportDocument("epub", { pdfA: "2b" }, flow.token);
// @ts-expect-error Chapter-aware archival publication uses the ./book entry.
void renderBookPdf([{ path: "one.md", source: "# One" }], { pdfA: "2b" });
