import type { FmdDocumentStats, FmdAccessibilityReport } from "./franken_markdown.js";

/** Native JSON reports are deeply frozen, including outlines and findings. */
export type InspectionReadonly<T> = T extends object
  ? { readonly [P in keyof T]: InspectionReadonly<T[P]> } : T;
export interface DocumentInspectionReport {
  readonly schema: "fmd-document-inspection-v1";
  readonly sourceLength: number;
  readonly stats: InspectionReadonly<FmdDocumentStats>;
  /** Source audit with native default options, not PDF/UA certification.
   * Additive native fields are retained and may include document text. */
  readonly accessibility: InspectionReadonly<FmdAccessibilityReport>;
}
export interface DocumentInspection extends DocumentInspectionReport {
  /** Owned UTF-8 JSON for the complete report; no partial report is returned. */
  readonly bytes: Uint8Array;
  readonly mimeType: "application/json";
  readonly extension: "json";
  text(): string;
  blob(): Blob;
  filename(baseName?: string): string;
}
export interface DocumentInspectionControl {
  signal?: AbortSignal;
  /** Queue wait, initialization and both native analyses; 0 disables. */
  timeoutMs?: number;
}
export interface DocumentInspectorOptions {
  /** A fresh dedicated endpoint owned by this inspector, never a shared worker. */
  workerFactory?: () => Pick<Worker, "postMessage" | "terminate" | "addEventListener" | "removeEventListener">;
  /** Default 30,000 ms. */
  timeoutMs?: number;
  /** Active plus queued requests; default 32, maximum 128. */
  maxPendingOperations?: number;
  /** Ingress accounting: default 16 MiB, maximum 64 MiB. Not a WASM heap limit. */
  maxPendingBytes?: number;
  /** Combined UTF-8 JSON, default/maximum 8 MiB. */
  maxOutputBytes?: number;
}
export interface DocumentInspector {
  readonly disposed: boolean;
  readonly pendingOperations: number;
  readonly pendingBytes: number;
  /** Strict UTF-8 source up to 4 MiB, with no raw-HTML or asset authority.
   * Captures one source for both existing native passes. A source edit is not
   * automatically an abort: editor hosts must invalidate stale results. */
  inspect(markdown: string, controls?: DocumentInspectionControl): Promise<DocumentInspection>;
  /** Immediate/idempotent. In-flight abort or timeout also closes this inspector. */
  dispose(): void;
}
export class FlowWorkerError extends Error { readonly code: string; }
export const INSPECTION_SOURCE_LIMIT: number;
export const INSPECTION_OUTPUT_LIMIT: number;
/** Lazy construction: no worker, native code or WASM is started on import. */
export function createDocumentInspector(options?: DocumentInspectorOptions): DocumentInspector;
