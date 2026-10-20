import type { FmdDiagnostic, FmdRenderOptions, FmdPdfRenderOptions, FmdDiffOptions, FmdSemanticDiff } from "./franken_markdown.js";

export type DocumentFormat = "html" | "pdf" | "svg" | "epub" | "interactive-html";
type Shared = Pick<FmdRenderOptions, "font" | "darkMode" | "fontScale" | "typeSize">;
export interface DocumentOptions {
  html: Shared & Pick<FmdRenderOptions, "title" | "customCss" | "allowRawHtml" | "lang" | "toc" | "tocDepth" | "pdfImages" | "fontAssets" | "htmlFontFormat">;
  pdf: Shared & Pick<FmdPdfRenderOptions, "title" | "author" | "metadataEpochSeconds" | "allowRawHtml" | "codeLineNumbers" | "pageNumbers" | "baseFontSize" | "headingScale" | "tableFontSize" | "lang" | "toc" | "tocDepth" | "fitToPages" | "microtype" | "microtypeProtrusion" | "typography" | "optimalPagination" | "pdfImages" | "fontAssets" | "page" | "running">;
  svg: Shared & Pick<FmdRenderOptions, "maxWidthPt" | "pdfImages" | "fontAssets">;
  epub: Shared & Pick<FmdRenderOptions, "title" | "lang" | "customCss" | "toc" | "tocDepth" | "pdfImages" | "fontAssets">;
  "interactive-html": Shared & Pick<FmdRenderOptions, "title" | "lang">;
}
export interface DocumentOutput<K extends DocumentFormat = DocumentFormat> {
  readonly format: K;
  readonly mimeType: string;
  readonly extension: K extends "pdf" ? "pdf" : K extends "svg" ? "svg" : K extends "epub" ? "epub" : "html";
  readonly sourceLength: number;
  readonly bytes: Uint8Array;
  /** Source-byte findings and optional document scope/reason codes are preserved. */
  readonly diagnostics: FmdDiagnostic[];
  text(): string;
  blob(): Blob;
  filename(baseName?: string): string;
}
/** One publication target with format-specific options. Formats must be unique. */
export type DocumentPublicationTarget<K extends DocumentFormat = DocumentFormat> = {
  [F in K]: { readonly format: F; readonly options?: DocumentOptions[F] };
}[K];
/** All requested artifacts from one source snapshot, in target order. */
export interface DocumentPublication<K extends DocumentFormat = DocumentFormat> {
  readonly format: "document-publication";
  readonly sourceLength: number;
  readonly totalBytes: number;
  readonly outputs: readonly { [F in K]: DocumentOutput<F> }[K][];
}
/** Native semantic report and HTML from one captured pair of source revisions.
 * Two existing native passes run in the worker; no new JavaScript differ is used.
 */
export interface RevisionComparison {
  readonly oldSourceLength: number;
  readonly newSourceLength: number;
  readonly report: FmdSemanticDiff;
  readonly html: Omit<DocumentOutput<"html">, "format"> & { readonly format: "diff-html" };
  readonly json: Pick<DocumentOutput<"html">, "bytes" | "text" | "blob" | "filename"> & {
    readonly mimeType: "application/json";
    readonly extension: "json";
  };
}
export interface DocumentControl {
  signal?: AbortSignal;
  /** Deadline includes queue wait and first-use WASM initialization; 0 disables it. */
  timeoutMs?: number;
}
export interface DocumentWorkerOptions {
  /** A fresh dedicated endpoint whose lifetime is owned by this renderer. */
  workerFactory?: () => Pick<Worker, "postMessage" | "terminate" | "addEventListener" | "removeEventListener">;
  /** Default 30,000 ms, measured from enqueue. */
  timeoutMs?: number;
  /** Active plus queued requests; default 32, maximum 128. */
  maxPendingOperations?: number;
  /** Charged ingress, default 16 MiB, maximum 64 MiB. Not a WASM heap limit. */
  maxPendingBytes?: number;
  /** Checked before transfer; default/maximum 64 MiB. Comparisons and publications charge all their outputs together. */
  maxOutputBytes?: number;
}
export interface DocumentWorkerRenderer {
  readonly disposed: boolean;
  readonly pendingOperations: number;
  readonly pendingBytes: number;
  /** Each source is bounded to 4 MiB UTF-8; only oldName/newName are supported.
   * Uses the same queue, cancellation and deadline as document renders.
   */
  compare(oldMarkdown: string, newMarkdown: string, options?: FmdDiffOptions, controls?: DocumentControl): Promise<RevisionComparison>;
  /** Render 1..5 distinct formats from one synchronously captured source/options/assets.
   * One queue slot and deadline; all outputs succeed or the promise rejects without
   * partial artifacts. The existing renderers run sequentially, without shared ASTs.
   * maxOutputBytes and diagnostic limits apply to the complete publication.
   * Exact asset views are copied once; asset keys remain scoped to their target.
   */
  publish<K extends DocumentFormat>(markdown: string, targets: readonly DocumentPublicationTarget<K>[], controls?: DocumentControl): Promise<DocumentPublication<K>>;
  render<K extends DocumentFormat>(format: K, markdown: string, options?: DocumentOptions[K], controls?: DocumentControl): Promise<DocumentOutput<K>>;
  renderHtml(markdown: string, options?: DocumentOptions["html"], controls?: DocumentControl): Promise<DocumentOutput<"html">>;
  renderPdf(markdown: string, options?: DocumentOptions["pdf"], controls?: DocumentControl): Promise<DocumentOutput<"pdf">>;
  renderSvg(markdown: string, options?: DocumentOptions["svg"], controls?: DocumentControl): Promise<DocumentOutput<"svg">>;
  renderEpub(markdown: string, options?: DocumentOptions["epub"], controls?: DocumentControl): Promise<DocumentOutput<"epub">>;
  renderInteractiveHtml(markdown: string, options?: DocumentOptions["interactive-html"], controls?: DocumentControl): Promise<DocumentOutput<"interactive-html">>;
  /** Immediate and idempotent. In-flight cancellation also permanently closes the renderer. */
  dispose(): void;
}
export class FlowWorkerError extends Error { readonly code: string; }
export const DOCUMENT_SOURCE_LIMIT: number;
export const DOCUMENT_OUTPUT_LIMIT: number;
export const COMPARISON_REPORT_LIMIT: number;
/** Construction is synchronous and lazy; no worker or WASM starts until a valid render request. */
export function createWorkerRenderer(options?: DocumentWorkerOptions): DocumentWorkerRenderer;
