import type { BookFile, BookLinksOutput, BookOptions, BookOutput } from "./book.js";
export type BookFormat = "pdf" | "epub" | "site";
export type BookWorkerFormat = BookFormat | "preview" | "inspection" | "links";
export interface BookPreviewOutput {
  readonly format: "book-preview";
  readonly mimeType: "application/json";
  readonly extension: "json";
  readonly sourceLength: number;
  /** UTF-8 fmd-book-preview-v1 JSON; generated HTML is untrusted host input. */
  readonly bytes: Uint8Array;
  blob(): Blob;
}
export interface BookInspectionOutput {
  readonly format: "book-inspection";
  readonly mimeType: "application/json";
  readonly extension: "json";
  readonly sourceLength: number;
  /** UTF-8 fmd-book-inspection-v1 JSON for original source chapters.
   * Failed checks are explicit; this is not publication conformance. */
  readonly bytes: Uint8Array;
  blob(): Blob;
}
export interface BookWorker {
  readonly busy: boolean;
  /** A single export owns a worker. Concurrent calls reject with BOOK_BUSY.
   * Assets are snapshotted, never transferred out of caller-owned buffers. */
  render(
    files: readonly BookFile[],
    format: BookFormat,
    options?: BookOptions,
    request?: { signal?: AbortSignal },
  ): Promise<Omit<BookOutput, "filename">>;
  /** Bounded chapter HTML derived from the Rust site export. Not PDF pages. */
  render(
    files: readonly BookFile[],
    format: "preview",
    options?: BookOptions,
    request?: { signal?: AbortSignal },
  ): Promise<BookPreviewOutput>;
  /** Source-only structural/accessibility inspection. Publication options,
   * images and fonts are ignored and are not transferred to the worker. */
  render(
    files: readonly BookFile[],
    format: "inspection",
    options?: BookOptions,
    request?: { signal?: AbortSignal },
  ): Promise<BookInspectionOutput>;
  /** Expanded local HTML navigation checks. Includes are captured; images,
   * fonts and presentation settings are ignored and never transferred. */
  render(
    files: readonly BookFile[],
    format: "links",
    options?: BookOptions,
    request?: { signal?: AbortSignal },
  ): Promise<Omit<BookLinksOutput, "filename">>;
  render(
    files: readonly BookFile[],
    format: BookWorkerFormat,
    options?: BookOptions,
    request?: { signal?: AbortSignal },
  ): Promise<
    | Omit<BookOutput, "filename">
    | BookPreviewOutput
    | BookInspectionOutput
    | Omit<BookLinksOutput, "filename">
  >;
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
