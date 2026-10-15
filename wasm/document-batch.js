// Optional browser entry: importing it neither starts workers nor loads WASM.
import { createWorkerRenderer } from "./document_worker.mjs";
import { createBatchIterator } from "./document_batch.mjs";

export { DocumentBatchError } from "./document_batch.mjs";
export { FlowWorkerError } from "./document_worker.mjs";

/** Render a pull-driven sequence of independent documents using bounded owned
 * workers. Results settle in input order; no failed document is replayed. */
export function renderDocumentBatch(documents, options = {}) {
  return createBatchIterator(documents, options, createWorkerRenderer);
}
