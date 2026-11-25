// Public worker declaration contract only; no native renderer is executed.
import { createBookWorker, type BookPdfVerificationWorkerOutput, type BookWorkerFormat } from "./book-worker.js";
import { parseBookPdfVerificationReport, type BookFile, type BookOptions } from "./book.js";

const files: readonly BookFile[] = [{ path: "one.md", source: "# One" }];
const options: BookOptions = { profile: "gfm-plus", strictPagination: true, page: { size: "a4" },
  typography: "line-height=1.5", pdfA: "2b", pdfAStrict: true,
  includeSources: [{ path: "shared.md", source: "included" }],
  images: [{ destination: "logo", bytes: new Uint8Array([1]) }] };
const worker = createBookWorker({ retainBook: true, maxOutputBytes: 8 * 1024 * 1024 });
const signal = new AbortController().signal;
const first: BookPdfVerificationWorkerOutput = await worker.render(files, "pdf-verification", options, { signal });
const format: "book-pdf-verification" = first.format;
const media: "application/json" = first.mimeType;
const profile: "commonmark-gfm" | "gfm-plus" | undefined = first.profile;
const bytes: Uint8Array = first.bytes;
const report = parseBookPdfVerificationReport(bytes, files.map(file => file.path));
const pageCount: number = report.layout.page_count;
const sourceRevision = first.retainedInputRevision ?? 1;
const next: BookPdfVerificationWorkerOutput = await worker.renderSourceUpdate([], "pdf-verification",
  { expectedRevision: sourceRevision }, { signal });
const captured: BookPdfVerificationWorkerOutput = await worker.renderSources(files, "pdf-verification",
  { expectedRevision: next.retainedInputRevision ?? 1 });
const replaced: BookPdfVerificationWorkerOutput = await worker.renderSourceSet(files, "pdf-verification",
  { expectedRevision: captured.retainedInputRevision ?? 1, includeSources: [] });
const mixedFormat: BookWorkerFormat = Math.random() ? "pdf-verification" : "pdf";
const mixed = await worker.render(files, mixedFormat);
if (mixed.format === "book-pdf-verification") parseBookPdfVerificationReport(mixed.bytes);
void [format, media, profile, pageCount, replaced];

// @ts-expect-error Cancellation requires an AbortSignal.
await worker.render(files, "pdf-verification", options, { signal: "cancel" });
// @ts-expect-error Source deltas cannot replace the retained publication settings.
await worker.renderSourceUpdate([], "pdf-verification", { expectedRevision: sourceRevision, pdfA: "off" });
// @ts-expect-error Source sets cannot renew image/resource authority.
await worker.renderSourceSet(files, "pdf-verification", { expectedRevision: sourceRevision, images: [] });
// @ts-expect-error This is a PDF-verification report, not a rendered PDF.
const renderedPdf: "book-pdf" = first.format;
worker.dispose();
