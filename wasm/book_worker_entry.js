import * as book from "./book.js";
import { inspectBook } from "./book_inspection.mjs";
import { renderBookPreview, renderBookSelectedPreview, renderBookSessionChapterPreview } from "./book_site_preview.mjs";
import { createRetainedBook, installBookWorker } from "./book_worker.mjs";
import * as documents from "./franken_markdown.js";

const retained = createRetainedBook(book, renderBookPreview, renderBookSessionChapterPreview);
installBookWorker(self, {
  ...book,
  renderBookPreview: (files, options) => renderBookPreview(book, files, options),
  renderBookSelectedPreview: (files, options, selected) => renderBookSelectedPreview(book, files, options, selected),
  renderRetainedBook: retained.render,
  clearRetainedBook: retained.clear,
  renderRetainedBookPreview: (files, options, selected) => retained.render(files, options,
    selected === undefined ? "preview" : "chapter-preview", selected),
  clearRetainedBookPreview: retained.clear,
  inspectBook: (files) => inspectBook(documents, files),
});
