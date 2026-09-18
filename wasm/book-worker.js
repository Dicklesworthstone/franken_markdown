import { createBookWorkerClient } from "./book_worker.mjs";

/** Importing this module never initializes WASM on the UI thread. */
export function createBookWorker(options = {}) {
  return createBookWorkerClient({
    ...options,
    workerFactory: options.workerFactory ?? (() => new Worker(new URL("./book_worker_entry.js", import.meta.url), { type: "module" }))
  });
}
