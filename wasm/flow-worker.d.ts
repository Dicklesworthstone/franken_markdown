import type {
  FlowAssetPage,
  FlowAssetResult,
  FlowBatchEdit,
  FlowCreateOptions,
  FlowEditOptions,
  FlowExportFormat,
  FlowExportOptionsByFormat,
  FlowExportResult,
  FlowGlyphOutlines,
  FlowHit,
  FlowIdentity,
  FlowLayoutOptions,
  FlowPageOptions,
  FlowReadingPage,
  FlowSelection,
  FlowSnapshot,
  FlowSnapshotOptions,
  FlowToken,
  FlowTokenInput,
  FlowViewportOptions,
  FlowViewportPage,
} from "./flow.js";

export type {
  FlowAssetPage,
  FlowAssetResult,
  FlowBatchEdit,
  FlowCreateOptions,
  FlowEditOptions,
  FlowEpubExportOptions,
  FlowExportDiagnostic,
  FlowExportFormat,
  FlowExportOptions,
  FlowExportOptionsByFormat,
  FlowExportResult,
  FlowGlyphOutlines,
  FlowHit,
  FlowHtmlExportOptions,
  FlowIdentity,
  FlowLayoutOptions,
  FlowPageOptions,
  FlowPdfExportOptions,
  FlowReadingPage,
  FlowSelection,
  FlowSnapshot,
  FlowSnapshotOptions,
  FlowToken,
  FlowTokenInput,
  FlowViewportOptions,
  FlowViewportPage,
} from "./flow.js";

export interface FlowWorkerControl {
  /** Queued abort removes only that request. In-flight abort terminates the
   * worker, loses the session and rejects all queued requests without replay. */
  signal?: AbortSignal;
  /** Deadline starts at enqueue; 0 disables it. Default is the worker's timeoutMs. */
  timeoutMs?: number;
}
/** A dedicated, exclusively owned Worker or a host adapter with this interface. */
export interface FlowWorkerEndpoint {
  postMessage(message: unknown, transfer?: Transferable[]): void;
  terminate(): unknown;
  addEventListener(type: string, listener: EventListener): void;
  removeEventListener(type: string, listener: EventListener): void;
}
export interface FlowWorkerOptions {
  /** Optional bundler/platform integration. Must create a NEW dedicated worker;
   * this session takes ownership and will terminate it. No shared worker pool. */
  workerFactory?: () => FlowWorkerEndpoint;
  /** Applies only to creation, not to the lifetime of the returned session. */
  signal?: AbortSignal;
  /** Startup deadline, including loading WASM and first layout. Default 60000. */
  startupTimeoutMs?: number;
  /** Per-operation enqueue deadline; default 30000, 0 disables. */
  timeoutMs?: number;
  /** Active plus queued requests; default 32, allowed 1..128. */
  maxPendingOperations?: number;
  /** Conservative string/binary pending-payload budget. Default 16 MiB, at
   * most 64 MiB. This is NOT a bound on the WASM heap or total browser memory. */
  maxPendingBytes?: number;
}
/** Async version of the persistent flow facade. Rust parsing, shaping, cache
 * and serialization all run in one dedicated worker. Revisions and options
 * below are last-acknowledged values, not speculative queued mutations. */
export interface WorkerFlowSession {
  readonly disposed: boolean;
  /** Negotiated at creation; false for legacy worker/native packages. */
  readonly supportsViewport: boolean;
  /** Negotiated with the worker/native package. No partial-write fallback. */
  readonly supportsAssetBatches: boolean;
  /** True only when both worker and native package expose atomic source edits. */
  readonly supportsEditBatches: boolean;
  readonly revision: string;
  readonly layoutRevision: string;
  readonly token: FlowToken;
  readonly layoutOptions: Required<FlowLayoutOptions>;
  readonly pendingOperations: number;
  readonly pendingBytes: number;
  getSource(control?: FlowWorkerControl): Promise<string>;
  edit(
    startUtf16: number,
    endUtf16: number,
    replacement: string,
    options: FlowEditOptions,
    control?: FlowWorkerControl,
  ): Promise<FlowToken>;
  editBytes(
    startByte: number,
    endByte: number,
    replacement: string,
    options: FlowEditOptions,
    control?: FlowWorkerControl,
  ): Promise<FlowToken>;
  /** One transaction in original-source UTF-16 coordinates, not sequential edits.
   * Up to 4,096 operations and 4 MiB combined UTF-8 replacements. Caller records
   * are snapshotted at enqueue and charged to the worker queue. No partial-write
   * fallback. Queued cancellation removes the batch; in-flight cancellation
   * loses the session. Empty/identity batches retain the native revision. */
  editMany(
    edits: readonly FlowBatchEdit[],
    options: FlowEditOptions,
    control?: FlowWorkerControl,
  ): Promise<FlowToken>;
  replaceSource(
    source: string,
    options: FlowEditOptions,
    control?: FlowWorkerControl,
  ): Promise<FlowToken>;
  reflow(
    options: FlowLayoutOptions,
    token: FlowTokenInput,
    control?: FlowWorkerControl,
  ): Promise<FlowToken>;
  provideAsset(result: FlowAssetResult, control?: FlowWorkerControl): Promise<FlowToken>;
  /** One atomic completion, up to 1,024 images and 32 MiB (8 MiB per payload).
   * The configured worker queue limit also applies, before copying any bytes.
   * Exact views are copied at enqueue; caller buffers are never detached.
   * Queued abort removes the whole batch; in-flight abort loses the session. */
  provideAssets(
    results: readonly FlowAssetResult[],
    control?: FlowWorkerControl,
  ): Promise<FlowToken>;
  reloadAssets(expectedRevision: FlowIdentity, control?: FlowWorkerControl): Promise<FlowToken>;
  snapshot(options?: FlowSnapshotOptions, control?: FlowWorkerControl): Promise<FlowSnapshot>;
  viewport(options: FlowViewportOptions, control?: FlowWorkerControl): Promise<FlowViewportPage>;
  readingOrder(options?: FlowPageOptions, control?: FlowWorkerControl): Promise<FlowReadingPage>;
  pendingAssets(options?: FlowPageOptions, control?: FlowWorkerControl): Promise<FlowAssetPage>;
  /** Captures the last-acknowledged token immediately. Each next() requests one
   * page; no prefetch or mixed generations. Control applies to each page call. */
  pages(
    options?: Omit<FlowSnapshotOptions, "offset">,
    control?: FlowWorkerControl,
  ): AsyncIterableIterator<FlowSnapshot>;
  hitTest(
    x: number,
    y: number,
    token: FlowTokenInput,
    control?: FlowWorkerControl,
  ): Promise<FlowHit>;
  selectText(
    itemIndex: number,
    startUtf16: number,
    endUtf16: number,
    token: FlowTokenInput,
    control?: FlowWorkerControl,
  ): Promise<FlowSelection>;
  copySource(
    startByte: number,
    endByte: number,
    expectedRevision: FlowIdentity,
    control?: FlowWorkerControl,
  ): Promise<string>;
  fontBytes(fontId: FlowIdentity, control?: FlowWorkerControl): Promise<Uint8Array>;
  /** Immutable glyph paths; input IDs are copied at enqueue, never transferred from the caller. */
  glyphOutlines(
    fontId: FlowIdentity,
    glyphIds: readonly number[] | Uint16Array,
    control?: FlowWorkerControl,
  ): Promise<FlowGlyphOutlines>;
  assetBytes(
    requestId: FlowIdentity,
    expectedRevision: FlowIdentity,
    control?: FlowWorkerControl,
  ): Promise<Uint8Array | null>;
  /** Runs the shared document renderer inside this worker. Input is a captured
   * source/layout revision; queued cancellation is safe, in-flight cancellation
   * terminates this session, like every other synchronous WASM operation. */
  exportDocument<F extends FlowExportFormat>(
    format: F,
    options: FlowExportOptionsByFormat[F] | undefined,
    token: FlowTokenInput,
    control?: FlowWorkerControl,
  ): Promise<FlowExportResult>;
  /** Immediate, idempotent termination. Rejects all outstanding operations. */
  dispose(): void;
}
export class FlowWorkerError extends Error {
  readonly code: string;
  constructor(code: string, message: string, options?: ErrorOptions);
}
/** Owns one worker/WASM instance per session. Never silently falls back to the
 * UI thread, retries a mutation or recovers lost source. Hosts retain their
 * authoritative Markdown and explicitly recreate after a fatal cancellation.
 */
export function createWorkerFlowSession(
  source: string,
  options?: FlowCreateOptions,
  workerOptions?: FlowWorkerOptions,
): Promise<WorkerFlowSession>;
