// Compile-only public option contract; this fixture is not a render test.
import { createWorkerRenderer, type DocumentOutput } from "./document-worker.js";
const worker = createWorkerRenderer();
const bytes = new Uint8Array([1]);
const pdf: Promise<DocumentOutput<"pdf">> = worker.renderPdf("# Report", {
  page: { size: "a4", orientation: "landscape", margins: { leftPt: 12 } },
});
worker.renderPdf("custom", { page: { size: { widthPt: 700, heightPt: 900 }, margins: 36 } });
worker.renderSvg("poster", { maxWidthPt: 720,
  pdfImages: [{ destination: "plot.png", bytes }],
  fontAssets: [{ slot: "body-bold", bytes, weight: 700 }],
});
const epub: Promise<DocumentOutput<"epub">> = worker.render("epub", "book", {
  customCss: "p { color: navy; }", toc: true, tocDepth: 3,
  pdfImages: [{ destination: "photo.jpg", bytes }],
  fontAssets: [{ slot: "body-regular", bytes }],
});
// @ts-expect-error Paper dimensions are PDF-only.
worker.renderSvg("x", { page: { size: "a4" } });
// @ts-expect-error EPUB does not support raw HTML passthrough.
worker.renderEpub("x", { allowRawHtml: true });
// @ts-expect-error Interactive HTML does not accept document resource arrays.
worker.renderInteractiveHtml("x", { pdfImages: [] });
// @ts-expect-error Named paper sizes are limited to the documented choices.
worker.renderPdf("x", { page: { size: "legal" } });
void pdf; void epub;
