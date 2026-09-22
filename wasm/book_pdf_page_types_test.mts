// Public /book declaration contract; compile only, no renderer execution.
import { createBook, renderBookPdf, type BookOptions, type BookFile } from "./book.js";
import type { FmdPdfPage } from "./franken_markdown.js";

const files: readonly BookFile[] = [{ path: "one.md", source: "# One" }];
const page: FmdPdfPage = { size: "a4", orientation: "landscape", margins: { leftPt: 24 } };
const options: BookOptions = { page, pageNumbers: true };
const session = await createBook(files, options);
session.renderPdf();
session.renderPdf({ page });
session.renderPdf({ page: { size: { widthPt: 720, heightPt: 540 }, margins: 0 } });
session.renderPdf({ page: {} });
await renderBookPdf(files, options);
// @ts-expect-error Both custom dimensions are required.
session.renderPdf({ page: { size: { widthPt: 612 } } });
// @ts-expect-error Margins cannot be coerced strings.
await createBook(files, { page: { margins: "36" } });
// @ts-expect-error Only page overrides are accepted after creation.
session.renderPdf({ font: "serif" });
// @ts-expect-error Export-local geometry must not change the EPUB API.
session.renderEpub({ page });
session.dispose();
