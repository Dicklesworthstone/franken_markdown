import * as book from "./book.js";
import { inspectBook } from "./book_inspection.mjs";
import { renderBookPreview } from "./book_site_preview.mjs";
import { createRetainedBook, installBookWorker } from "./book_worker.mjs";
import * as documents from "./franken_markdown.js";

const retained = createRetainedBook(book, renderBookPreview);
installBookWorker(self, {
  ...book,
  renderBookPreview: (files, options) => renderBookPreview(book, files, options),
  renderRetainedBook: retained.render,
  clearRetainedBook: retained.clear,
  renderRetainedBookPreview: (files, options) => retained.render(files, options, "preview"),
  clearRetainedBookPreview: retained.clear,
  inspectBook: (files) => inspectBook(documents, files),
});
