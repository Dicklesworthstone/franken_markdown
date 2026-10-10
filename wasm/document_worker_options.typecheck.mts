// Compile-only public option contract; this fixture is not a render test.
import { createWorkerRenderer, type DocumentOutput } from "./document-worker.js";
const worker = createWorkerRenderer();
const bytes = new Uint8Array([1]);
const pdf: Promise<DocumentOutput<"pdf">> = worker.renderPdf("# Report", {
  page: { size: "a4", orientation: "landscape", margins: { leftPt: 12 } },
  running: { header: { left: "{title}", rule: true,
    image: { dest: "brand.svg", position: "right", heightPt: 24 } },
    footer: { center: "{page} / {pages}" }, skipFirstPage: true },
  pdfImages: [{ destination: "brand.svg", bytes }],
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
// @ts-expect-error Running bands are PDF-only, including generic dispatch.
worker.render("html", "x", { running: { header: { left: "Title" } } });
// @ts-expect-error Logos have side positions, not a center position.
worker.renderPdf("x", { running: { header: { image: { dest: "logo.svg", position: "center" } } } });
// @ts-expect-error Logo heights are numeric points without string coercion.
worker.renderPdf("x", { running: { footer: { image: { dest: "logo.svg", heightPt: "24" } } } });
// @ts-expect-error Logo bytes use pdfImages rather than inline running settings.
worker.renderPdf("x", { running: { header: { image: { dest: "logo.svg", bytes } } } });
void pdf; void epub;
