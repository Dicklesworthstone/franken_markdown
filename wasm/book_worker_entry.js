import * as book from "./book.js";
import { installBookWorker } from "./book_worker.mjs";
import { renderBookPreview } from "./book_site_preview.mjs";
installBookWorker(self, { ...book,
  renderBookPreview: (files, options) => renderBookPreview(book, files, options)
});
