// Compile only: tsc --noEmit --strict --target ES2022 --module NodeNext
// --moduleResolution NodeNext wasm/document_worker_types_test.mts
import { createWorkerRenderer, type DocumentOutput } from "@franken-suite/franken-markdown/document-worker";

const renderer = createWorkerRenderer({
  workerFactory: () => new Worker(new URL("./document_worker_entry.js", import.meta.url), { type: "module" }),
  maxPendingOperations: 2, maxPendingBytes: 16 * 1024 * 1024, maxOutputBytes: 1024,
});
const control = { signal: new AbortController().signal, timeoutMs: 1000 };
const pdf: DocumentOutput<"pdf"> = await renderer.renderPdf("# PDF", { pageNumbers: true, metadataEpochSeconds: 0 }, control);
const pdfExtension: "pdf" = pdf.extension;
const svg: DocumentOutput<"svg"> = await renderer.render("svg", "# SVG", { maxWidthPt: 612 }, control);
const svgExtension: "svg" = svg.extension;
await renderer.renderHtml("# HTML", { customCss: "body{}", pdfImages: [{ destination: "a.png", bytes: new Uint8Array([1]) }] });
await renderer.renderEpub("# Book", { title: "Book", lang: "en" });
await renderer.renderInteractiveHtml("# Editable", { fontScale: "lg" });
// @ts-expect-error EPUB ABI does not accept host image bytes.
await renderer.renderEpub("# Book", { pdfImages: [] });
// @ts-expect-error PDF renderer does not consume CSS.
await renderer.renderPdf("# PDF", { customCss: "body{}" });
// @ts-expect-error Generic dispatch retains the SVG option contract.
await renderer.render("svg", "# SVG", { author: "author" });
// @ts-expect-error Only the five single-document formats are available here.
await renderer.render("book-site", "source");
// @ts-expect-error Metadata timestamps are numbers, not bigint coercions.
await renderer.renderPdf("# PDF", { metadataEpochSeconds: 1n });
// @ts-expect-error Returned SVG extension is not a PDF extension.
const wrongExtension: "pdf" = svg.extension;
void [pdfExtension, svgExtension, wrongExtension];
renderer.dispose();
