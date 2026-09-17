import type { FlowAssetPage, FlowAssetRequest, FlowAssetResult, FlowPageOptions, FlowToken } from "./flow.js";
import type { FlowCanvasImage } from "./flow-canvas.js";
/** Satisfied by both FlowSession and WorkerFlowSession; neither is owned here. */
export interface ImageFlowSession {
  readonly disposed: boolean;
  readonly token: FlowToken;
  pendingAssets(options?: FlowPageOptions): FlowAssetPage | Promise<FlowAssetPage>;
  provideAsset(result: FlowAssetResult): FlowToken | Promise<FlowToken>;
}
/** Defaults may only be lowered; these are structural budgets, not a browser
 * allocator/GPU sandbox. Host loader memory is outside these counters. */
export interface FlowImageLimits {
  maxConcurrentLoads?: number; // 4, including callbacks still running after abort.
  maxAssets?: number; // 256 pending/attempted per source generation.
  maxAssetBytes?: number; // 8 MiB per encoded image.
  maxInFlightBytes?: number; // 16 MiB of admitted immutable encoded snapshots.
  maxImagePixels?: number; // 16,777,216 per bitmap, checked BEFORE decoding.
  maxRetainedPixels?: number; // 33,554,432 across reserved and retained bitmaps.
  maxDimension?: number; // 8,192 per dimension, before and after orientation.
}
export interface FlowImageLoadContext {
  readonly signal: AbortSignal;
  /** Enforce while reading, before returning bytes; no ambient fetch is used. */
  readonly maxBytes: number;
}
export interface FlowImageDecodeContext {
  readonly signal: AbortSignal;
  readonly mime: "image/png" | "image/jpeg";
  readonly width: number;
  readonly height: number;
  readonly pixels: number;
}
/** A host decoder transfers exclusive bitmap ownership to the manager. It
 * must not retain/mutate/close that bitmap; decode input is an immutable Blob. */
export interface OwnedFlowImage {
  readonly width: number;
  readonly height: number;
  close(): void;
}
export interface FlowImageStats {
  readonly revision: string | null;
  readonly images: number;
  readonly retainedPixels: number;
  readonly reservedPixels: number;
  readonly inFlightBytes: number;
  readonly attempted: number;
  readonly busy: boolean;
}
export interface FlowImageOptions {
  /** Authorize each request in its source generation, including base URI,
   * destination, redirects and credentials. Return null to decline. A loader
   * must bound its own I/O/allocation; bytes are snapshotted on receipt. */
  load: (request: Readonly<FlowAssetRequest>, context: FlowImageLoadContext) => Uint8Array | null | Promise<Uint8Array | null>;
  /** Defaults to createImageBitmap. Only admitted static PNG/JPEG reaches this
   * callback. Return an exclusively owned CanvasImageSource with close(). */
  decode?: (blob: Blob, context: FlowImageDecodeContext) => (CanvasImageSource & OwnedFlowImage) | Promise<CanvasImageSource & OwnedFlowImage>;
  /** Once per physically settled batch that attempted work or was aborted,
   * and on clear/dispose. Never once per image. Observer exceptions are ignored. */
  onChange?: (stats: FlowImageStats) => void;
  /** Opt-in encoded payload retention in the native session for HTML/PDF
   * export. Default false (dimension-only). Native payload budgets still apply.
   * clear/dispose only own bitmaps: revoke native bytes via reloadAssets or
   * session recreation before exporting after an authorization change. */
  retainSourceBytes?: boolean;
  limits?: FlowImageLimits;
  /** Whole-batch deadline; default 30,000 ms. 0 disables the deadline. */
  timeoutMs?: number;
}
export interface FlowImageReport {
  readonly revision: string;
  readonly loaded: number;
  readonly failed: number;
  readonly skipped: number;
  /** Does not expose source URLs, bytes, credentials or callback messages. */
  readonly errors: readonly { readonly requestId: string; readonly code: string }[];
}
export class FlowAssetError extends Error {
  readonly code: string;
  constructor(code: string, message: string, options?: ErrorOptions);
}
export class FlowImageAssets {
  constructor(session: ImageFlowSession, options: FlowImageOptions);
  readonly disposed: boolean;
  /** Physical work may outlive an aborted or timed-out public call. */
  readonly busy: boolean;
  readonly stats: FlowImageStats;
  /** Does not roll back an in-flight native delivery or terminate the worker.
   * One physical batch at a time; overlap throws ASSET_BUSY. Individual image
   * failures are reported without preventing unrelated deliveries. */
  loadPending(options?: { signal?: AbortSignal }): Promise<FlowImageReport>;
  /** Wait for actual callback completion. A host ignoring abort may never settle. */
  whenIdle(): Promise<void>;
  /** Source changes revoke images automatically; reflows preserve them. */
  synchronize(): FlowToken;
  /** Borrow only for the matching current paint; the manager owns close(). */
  resolveImage(image: FlowCanvasImage, token: FlowToken): (CanvasImageSource & OwnedFlowImage) | null;
  /** Immediately revoke and close. Also clear the painter; call reloadAssets
   * on the session before reauthorizing previously resolved requests. */
  clear(): void;
  /** Closes bitmaps, discards late results; never disposes the session. */
  dispose(): void;
}
