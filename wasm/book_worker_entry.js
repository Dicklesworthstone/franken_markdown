import * as book from "./book.js";
import * as documents from "./franken_markdown.js";
import { installBookWorker } from "./book_worker.mjs";
import { renderBookPreview } from "./book_site_preview.mjs";
import { inspectBook } from "./book_inspection.mjs";
installBookWorker(self, { ...book,
  renderBookPreview: (files, options) => renderBookPreview(book, files, options),
  inspectBook: files => inspectBook(documents, files)
});
