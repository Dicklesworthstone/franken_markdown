import { createBookWorkerClient } from "./book_worker.mjs";
import { prepareBookInput } from "./book_session.mjs";
import { createBookPublicationClient } from "./book_publication.mjs";

/** Importing this module never initializes WASM on the UI thread. */
export function createBookWorker(options = {}) {
  return createBookWorkerClient({
    ...options,
    workerFactory:
      options.workerFactory ??
      (() => new Worker(new URL("./book_worker_entry.js", import.meta.url), { type: "module" })),
  });
}

/** Publish a selected PDF/EPUB/site edition atomically from one captured book.
 * Each publication owns one private retained worker, released on every outcome.
 * The ordinary createBookWorker API and its retention policy are unchanged.
 */
export function createBookPublicationWorker(options = {}) {
  const { workerFactory, timeoutMs = 120000, maxOutputBytes = 128 * 1024 * 1024 } = options;
  if (workerFactory !== undefined && typeof workerFactory !== "function")
    throw Object.assign(new TypeError("workerFactory must be a function"), { code: "INVALID_OPTIONS" });
  const factory = workerFactory ??
    (() => new Worker(new URL("./book_worker_entry.js", import.meta.url), { type: "module" }));
  return createBookPublicationClient({
    captureInput: prepareBookInput,
    timeoutMs,
    maxOutputBytes,
    createClient: () => createBookWorkerClient({
      workerFactory: factory, timeoutMs, maxOutputBytes, retainBook: true,
      // Only the publication deadline may expire this private capture. The
      // coordinator always disposes the client; no idle worker survives it.
      idleTimeoutMs: 600000,
    }),
  });
}
