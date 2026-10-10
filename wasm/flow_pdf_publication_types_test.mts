import type { FlowSession, FlowPdfExportOptions, FlowPdfPage, FlowPdfRunning, FlowPdfRunningBand, FlowPdfRunningImage } from "./flow.js";
import type { WorkerFlowSession, FlowPdfPage as WorkerPage } from "./flow-worker.js";
import type { FmdPdfPage, FmdPdfRunning } from "./franken_markdown.js";
import type { BookOptions } from "./book.js";
declare const direct: FlowSession;
declare const worker: WorkerFlowSession;
const band: FlowPdfRunningBand = { left: "{title}", right: "{page}/{pages}", rule: true };
const running: FlowPdfRunning = { header: band, skipFirstPage: true };
const logo: FlowPdfRunningImage = { dest: "brand.svg", position: "right", heightPt: 24 };
const book: BookOptions = { running: { header: { image: logo } }, images: [{ destination: logo.dest, bytes: new Uint8Array([1]) }] };
void book;
const page: FlowPdfPage = { size: "a4", orientation: "landscape", margins: { topPt: 48, leftPt: 36 } };
const nativePage: FmdPdfPage = page;
const nativeRunning: FmdPdfRunning = running;
const workerPage: WorkerPage = nativePage;
const options: FlowPdfExportOptions = { page: workerPage, running: nativeRunning, title: "Print", pageNumbers: true };
void direct.exportDocument("pdf", options, direct.token);
void worker.exportDocument("pdf", options, worker.token, { timeoutMs: 10000 });
void worker.exportDocument("pdf", { running: { header: { image: logo } },
  pdfImages: [{ destination: logo.dest, bytes: new Uint8Array([1]) }] }, worker.token);
void worker.exportDocument("pdf", { page: { size: { widthPt: 400, heightPt: 600 }, margins: 36 } }, worker.token);
// @ts-expect-error Paper orientation has no CSS aliases.
direct.exportDocument("pdf", { page: { orientation: "wide" } }, direct.token);
// @ts-expect-error Header templates are data, not code.
worker.exportDocument("pdf", { running: { header: { left: () => "date" } } }, worker.token);
// @ts-expect-error Paper settings must not be silently accepted by EPUB.
worker.exportDocument("epub", { page }, worker.token);
// @ts-expect-error SVG does not render PDF running bands.
direct.exportDocument("svg", { running }, direct.token);
// @ts-expect-error Margin dimensions are numeric PDF points.
direct.exportDocument("pdf", { page: { margins: "1in" } }, direct.token);
// @ts-expect-error Logos support only left/right placement.
direct.exportDocument("pdf", { running: { header: { image: { dest: "logo", position: "center" } } } }, direct.token);
// @ts-expect-error Explicit export-only images are limited to PDF.
direct.exportDocument("html", { pdfImages: [] }, direct.token);
