import * as book from "./book.js";
import { installBookWorker } from "./book_worker.mjs";
installBookWorker(self, book);
