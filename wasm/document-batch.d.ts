import type { DocumentFormat, DocumentOptions, DocumentOutput, DocumentControl, DocumentWorkerOptions } from "./document-worker.js";
export { FlowWorkerError } from "./document-worker.js";

/** Options stay correlated with the selected format, including mixed batches. */
export type DocumentBatchItem<K extends DocumentFormat = DocumentFormat> = {
  [F in K]: {
    format: F;
    source: string;
    options?: DocumentOptions[F];
    controls?: DocumentControl;
    /** Optional caller identity, at most 4096 UTF-16 code units. */
    name?: string;
  }
}[K];

export interface DocumentBatchOptions {
  /** Active documents plus retained results; default 2, integer 1..8. */
  concurrency?: number;
  /** Default input order. Completion order lets fast lanes bypass slow jobs. */
  order?: "input" | "completion";
  /** Each lane owns one renderer with maxPendingOperations fixed at 1. */
  worker?: Pick<DocumentWorkerOptions, "workerFactory" | "timeoutMs" | "maxPendingBytes" | "maxOutputBytes">;
  /** Stops the entire batch, including a pending asynchronous source pull. */
  signal?: AbortSignal;
}

export type DocumentBatchReceipt<K extends DocumentFormat = DocumentFormat> = {
  readonly index: number;
  readonly name?: string;
} & (
  | { readonly status: "fulfilled"; readonly value: { [F in K]: DocumentOutput<F> }[K] }
  | { readonly status: "rejected"; readonly reason: unknown }
);

export interface DocumentBatchIterator<K extends DocumentFormat = DocumentFormat> extends AsyncIterable<DocumentBatchReceipt<K>> {
  next(): Promise<IteratorResult<DocumentBatchReceipt<K>, unknown>>;
  /** Synchronously releases workers; requests source cleanup without awaiting stalled I/O. */
  return(value?: unknown): Promise<IteratorResult<DocumentBatchReceipt<K>, unknown>>;
  throw(reason?: unknown): Promise<IteratorResult<DocumentBatchReceipt<K>, unknown>>;
  [Symbol.asyncIterator](): DocumentBatchIterator<K>;
}

export class DocumentBatchError extends Error {
  constructor(code: string, message: string);
  readonly code: string;
}

/** Lazy, bounded worker rendering. Async iteration takes precedence when both
 * protocols exist. Per-document failures yield receipts; source failures and
 * batch cancellation reject iteration. No failed document is replayed.
 */
export function renderDocumentBatch<K extends DocumentFormat>(
  documents: Iterable<DocumentBatchItem<K>> | AsyncIterable<DocumentBatchItem<K>>,
  options?: DocumentBatchOptions,
): DocumentBatchIterator<K>;
