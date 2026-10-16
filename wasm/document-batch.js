// Optional browser entry: importing it neither starts workers nor loads WASM.
import { createWorkerRenderer } from "./document_worker.mjs";
import { createBatchIterator } from "./document_batch.mjs";

export { DocumentBatchError } from "./document_batch.mjs";
export { FlowWorkerError } from "./document_worker.mjs";

/** Render a synchronous or asynchronous sequence using bounded owned workers.
 * Results settle in input order by default; order: "completion" delivers each
 * result as it finishes. Jobs may select one format or an atomic targets list.
 * No failed document or publication is replayed. */
export function renderDocumentBatch(documents, options = {}) {
  return createBatchIterator(documents, options, createWorkerRenderer);
}
