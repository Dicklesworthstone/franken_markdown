import type { BookFile, BookOptions, BookOutput } from "./book.js";
export type BookFormat = "pdf" | "epub" | "site";
export interface BookWorker {
  readonly busy: boolean;
  /** A single export owns a worker. Concurrent calls reject with BOOK_BUSY.
   * Assets are snapshotted, never transferred out of caller-owned buffers. */
  render(files: readonly BookFile[], format: BookFormat, options?: BookOptions,
    request?: { signal?: AbortSignal }): Promise<Omit<BookOutput, "filename">>;
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
