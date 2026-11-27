// Compile-time public API checks; runtime layout is tested separately.
import { createBook, verifyBookPdf, parseBookPdfVerificationReport,
  type BookPdfVerificationOutput, type BookPdfVerificationReport, type BookFile } from "./book.js";

const files: readonly BookFile[] = [{ path: "one.md", source: "# One" }];
const book = await createBook(files, { profile: "commonmark-gfm", baselineGridPt: 12,
  running: { footer: { center: "{page}/{pages}" } }, images: [], fontAssets: [],
  includeSources: [], strictPagination: true });
const direct: BookPdfVerificationOutput = book.verifyPdf({ page: { size: "a4" }, pdfA: "2b", pdfAStrict: true });
const once: BookPdfVerificationOutput = await verifyBookPdf(files, { page: { size: "a5" }, fitToPages: 2 });
const report: BookPdfVerificationReport = parseBookPdfVerificationReport(once.bytes, ["one.md"]);
const format: "book-pdf-verification" = direct.format;
const text: string | undefined = report.layout.pages[0]?.runs[0]?.text;
const target: number | null | undefined = report.destinations[0]?.page;
const status: "greedy" | "exact_height" | "relaxed_height" | "legacy" = report.pagination.strategy;
void [format, text, target, status];

// @ts-expect-error Source dialect is fixed when a retained book is created.
book.verifyPdf({ profile: "gfm-plus" });
// @ts-expect-error Export-local verification cannot replace retained fonts.
book.verifyPdf({ fontAssets: [] });
// @ts-expect-error A PDF report is not downloadable PDF bytes.
const pdf: "application/pdf" = direct.mimeType;
// @ts-expect-error Returned report data is immutable.
report.layout.pages[0].runs[0].text = "changed";
book.dispose();
