import type { DocumentFormat, DocumentOptions, DocumentOutput, DocumentControl, DocumentWorkerOptions, DocumentPublicationTarget, DocumentPublication } from "./document-worker.js";
export { FlowWorkerError } from "./document-worker.js";

/** Options stay correlated with the selected format, including mixed batches. */
export type DocumentBatchItem<K extends DocumentFormat = DocumentFormat> = {
  [F in K]: {
    format: F;
    targets?: never;
    source: string;
    options?: DocumentOptions[F];
    controls?: DocumentControl;
    /** Optional caller identity, at most 4096 UTF-16 code units. */
    name?: string;
  }
}[K];

/** One source published atomically to 1..5 distinct formats in one batch slot. */
export interface DocumentBatchPublicationItem<K extends DocumentFormat = DocumentFormat> {
  source: string;
  targets: readonly DocumentPublicationTarget<K>[];
  controls?: DocumentControl;
  name?: string;
  format?: never;
  options?: never;
}
export type DocumentBatchJob<K extends DocumentFormat = DocumentFormat> =
  DocumentBatchItem<K> | DocumentBatchPublicationItem<K>;
type BatchMode = "single" | "publication";
type BatchValue<K extends DocumentFormat, M extends BatchMode> =
  M extends "publication" ? DocumentPublication<K> : { [F in K]: DocumentOutput<F> }[K];

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

export type DocumentBatchReceipt<K extends DocumentFormat = DocumentFormat, M extends BatchMode = "single"> = {
  readonly index: number;
  readonly name?: string;
} & (
  | { readonly status: "fulfilled"; readonly value: BatchValue<K, M> }
  | { readonly status: "rejected"; readonly reason: unknown }
);

export interface DocumentBatchIterator<K extends DocumentFormat = DocumentFormat, M extends BatchMode = "single"> extends AsyncIterable<DocumentBatchReceipt<K, M>> {
  next(): Promise<IteratorResult<DocumentBatchReceipt<K, M>, unknown>>;
  /** Synchronously releases workers; requests source cleanup without awaiting stalled I/O. */
  return(value?: unknown): Promise<IteratorResult<DocumentBatchReceipt<K, M>, unknown>>;
  throw(reason?: unknown): Promise<IteratorResult<DocumentBatchReceipt<K, M>, unknown>>;
  [Symbol.asyncIterator](): DocumentBatchIterator<K, M>;
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

/** Publication-only inputs preserve exact publication output inference. */
export function renderDocumentBatch<K extends DocumentFormat>(
  documents: Iterable<DocumentBatchPublicationItem<K>> | AsyncIterable<DocumentBatchPublicationItem<K>>,
  options?: DocumentBatchOptions,
): DocumentBatchIterator<K, "publication">;
/** Single-output jobs and whole publications may share the same bounded stream. */
export function renderDocumentBatch<K extends DocumentFormat>(
  documents: Iterable<DocumentBatchJob<K>> | AsyncIterable<DocumentBatchJob<K>>,
  options?: DocumentBatchOptions,
): DocumentBatchIterator<K, "single" | "publication">;
