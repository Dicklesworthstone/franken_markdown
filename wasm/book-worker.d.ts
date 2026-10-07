import type { BookFile, BookLinksOutput, BookOptions, BookOutput } from "./book.js";
export type BookFormat = "pdf" | "epub" | "site";
export type BookWorkerFormat = BookFormat | "preview" | "inspection" | "links";
/** A transport capture revision is local to this worker client, not a native
 * source revision. Present only when the worker negotiated source deltas. */
export interface BookWorkerCapture {
  readonly retainedInputRevision?: number;
}
export interface BookPreviewOutput extends BookWorkerCapture {
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
  /** Whether an idle book last used for preview is currently retained. */
  readonly hasRetainedPreview: boolean;
  /** Whether any idle native book is retained, including preview-only mode. */
  readonly hasRetainedBook: boolean;
  /** Current idle transport capture, or null while busy/released/unsupported.
   * Every successful retained export advances it, even with unchanged source. */
  readonly retainedInputRevision: number | null;
  /** A single export owns a worker. Concurrent calls reject with BOOK_BUSY.
   * Assets are snapshotted, never transferred out of caller-owned buffers. */
  render(
    files: readonly BookFile[],
    format: BookFormat,
    options?: BookOptions,
    request?: { signal?: AbortSignal },
  ): Promise<Omit<BookOutput, "filename"> & BookWorkerCapture>;
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
    | (Omit<BookOutput, "filename"> & BookWorkerCapture)
    | BookPreviewOutput
    | BookInspectionOutput
    | Omit<BookLinksOutput, "filename">
  >;
  /** Export after selective text changes to exact retained chapter/include keys.
   * Sends only changes; no settings, unchanged sources, image or font bytes.
   * [] re-exports unchanged source. Paths/order/roles/resources cannot change.
   * Requires a negotiated idle retainedInputRevision. Stale local revisions
   * fail without discarding the newer capture. After expiry/worker failure,
   * supply a complete current snapshot with render(); no implicit retry occurs.
   * Native update/export/receipt failures retire the worker. Cancellation and
   * timeout still terminate synchronous rendering. Output is always complete. */
  renderSourceUpdate(
    changes: readonly BookFile[],
    format: BookFormat | "preview",
    options: { expectedRevision: number },
    request?: { signal?: AbortSignal },
  ): Promise<(Omit<BookOutput, "filename"> & BookWorkerCapture) | BookPreviewOutput>;
  /** Capture complete current chapter/include text, then send only changed
   * strings against the idle source baseline. No images/fonts/settings are
   * read or transferred. Membership/order/roles must remain identical; use
   * render() for structural or configuration changes. The source baseline
   * shares the idle worker lifetime and is released on expiry/cancellation. */
  renderSources(
    files: readonly BookFile[],
    format: BookFormat | "preview",
    options: { expectedRevision: number; includeSources?: readonly BookFile[] },
    request?: { signal?: AbortSignal },
  ): Promise<(Omit<BookOutput, "filename"> & BookWorkerCapture) | BookPreviewOutput>;
  /** Terminates running AND retained idle workers. The client can render again. */
  cancel(): void;
  /** Cancel in-flight work, but keep an already-idle book. Hosts must use
   * cancel() for resource revocation, suspension and explicit clearing. */
  cancelPending(): void;
  /** Idempotent; also cancels the current export. */
  dispose(): void;
}
export function createBookWorker(options?: {
  workerFactory?: () => Worker;
  /** Includes startup. Default 120000; allowed 1..600000 milliseconds. */
  timeoutMs?: number;
  /** Default and ceiling 128 MiB. Checked again before publishing to the host. */
  maxOutputBytes?: number;
  /** Opt in to one retained native book for preview calls only. Default false.
   * Source edits use native transactions; settings/assets changes reconstruct.
   * render() sends a full snapshot; renderSourceUpdate() sends only changes. */
  retainPreview?: boolean;
  /** Opt in to one native session across PDF, EPUB, site and preview exports.
   * Takes precedence over retainPreview. Text edits use updateSources; source
   * membership/order changes use replaceSources. Old edit APIs reconstruct from
   * the current snapshot. Settings or assets changes also reconstruct.
   * Inspection and links remain one-shot, source-only, and release idle state.
   * Failures/cancellation retire the session; no queue or persistence is added.
   * A matching worker script is required; unsupported responses fail explicitly.
   */
  retainBook?: boolean;
  /** Release idle retained books after this delay. Default 30000; 1..600000 ms. */
  idleTimeoutMs?: number;
}): BookWorker;
