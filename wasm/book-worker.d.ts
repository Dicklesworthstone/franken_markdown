import type { BookFile, BookOptions, BookOutput } from "./book.js";
export type BookFormat = "pdf" | "epub" | "site";
export type BookWorkerFormat = BookFormat | "preview";
export interface BookPreviewOutput {
  readonly format: "book-preview";
  readonly mimeType: "application/json";
  readonly extension: "json";
  readonly sourceLength: number;
  /** UTF-8 fmd-book-preview-v1 JSON; generated HTML is untrusted host input. */
  readonly bytes: Uint8Array;
  blob(): Blob;
}
export interface BookWorker {
  readonly busy: boolean;
  /** A single export owns a worker. Concurrent calls reject with BOOK_BUSY.
   * Assets are snapshotted, never transferred out of caller-owned buffers. */
  render(files: readonly BookFile[], format: BookFormat, options?: BookOptions,
    request?: { signal?: AbortSignal }): Promise<Omit<BookOutput, "filename">>;
  /** Bounded chapter HTML derived from the Rust site export. Not PDF pages. */
  render(files: readonly BookFile[], format: "preview", options?: BookOptions,
    request?: { signal?: AbortSignal }): Promise<BookPreviewOutput>;
  render(files: readonly BookFile[], format: BookWorkerFormat, options?: BookOptions,
    request?: { signal?: AbortSignal }): Promise<Omit<BookOutput, "filename"> | BookPreviewOutput>;
  /** Terminates the running worker. The client can render again afterward. */
  cancel(): void;
  /** Idempotent; also cancels the current export. */
  dispose(): void;
}
export function createBookWorker(options?: {
  workerFactory?: () => Worker;
  /** Includes startup. Default 120000; allowed 1..600000 milliseconds. */
  timeoutMs?: number;
  /** Default and ceiling 128 MiB. Checked again before publishing to the host. */
  maxOutputBytes?: number;
}): BookWorker;
