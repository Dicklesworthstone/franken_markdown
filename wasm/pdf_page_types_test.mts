// Compile-only public entry-point contract; no native renderer is executed.
import { renderPdf, renderHtml, renderEpub, renderBookPdf, createRenderer,
  type FmdPdfPage, type FmdPdfRenderOptions } from "@franken-suite/franken-markdown";

const page: FmdPdfPage = { size: "a4", orientation: "landscape",
  margins: { leftPt: 24, rightPt: 36, topPt: 18, bottomPt: 30 } };
const options: FmdPdfRenderOptions = { page, font: "serif", pageNumbers: true, metadataEpochSeconds: 0 };
await renderPdf("# Report", options);
await renderPdf("# Custom", { page: { size: { widthPt: 720, heightPt: 540 }, margins: 0 } });
await (await createRenderer()).renderPdf("# Report", options);
// @ts-expect-error Both custom dimensions are required.
await renderPdf("x", { page: { size: { widthPt: 612 } } });
// @ts-expect-error Margin points cannot be coerced strings.
await renderPdf("x", { page: { margins: "36" } });
// @ts-expect-error HTML has no PDF paper geometry.
await renderHtml("x", { page });
// @ts-expect-error EPUB has no PDF paper geometry.
await renderEpub("x", { page });
// @ts-expect-error The book PDF binding has not yet gained this option.
await renderBookPdf([{ path: "one.md", source: "# One" }], { page });
