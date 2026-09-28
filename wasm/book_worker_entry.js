import * as book from "./book.js";
import { inspectBook } from "./book_inspection.mjs";
import { renderBookPreview } from "./book_site_preview.mjs";
import { createRetainedBookPreview, installBookWorker } from "./book_worker.mjs";
import * as documents from "./franken_markdown.js";

const retained = createRetainedBookPreview(book, renderBookPreview);
installBookWorker(self, {
  ...book,
  renderBookPreview: (files, options) => renderBookPreview(book, files, options),
  renderRetainedBookPreview: retained.render,
  clearRetainedBookPreview: retained.clear,
  inspectBook: (files) => inspectBook(documents, files),
});
