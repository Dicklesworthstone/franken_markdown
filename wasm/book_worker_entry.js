import * as book from "./book.js";
import { inspectBook } from "./book_inspection.mjs";
import { renderBookPreview } from "./book_site_preview.mjs";
import { installBookWorker } from "./book_worker.mjs";
import * as documents from "./franken_markdown.js";

installBookWorker(self, {
  ...book,
  renderBookPreview: (files, options) => renderBookPreview(book, files, options),
  inspectBook: (files) => inspectBook(documents, files),
});
