import type {FmdPdfRenderOptions, FmdRenderOutput, FmdPdfPage} from './franken_markdown.js';

/** Trusted artifacts from ONE matching wasm-bindgen --target web build.
 * Supply file contents, never URLs. Byte views are snapshotted before loading.
 * Binding JavaScript is executable application code, not untrusted document data.
 */
export interface FmdNativeWorkspaceRuntime {
  wasm: Uint8Array | ArrayBuffer | ArrayBufferView;
  bindings: string;
}

/** Settings supported by both the saved workspace and its native render backend.
 * Unsupported settings throw, including customCss and raw HTML passthrough.
 */
export interface FmdNativeWorkspaceOptions extends Pick<FmdPdfRenderOptions,
  'font' | 'darkMode' | 'title' | 'author' | 'lang' | 'metadataEpochSeconds' |
  'pageNumbers' | 'codeLineNumbers' | 'toc' | 'tocDepth' | 'pdfImages' | 'fontAssets' |
  'typography' | 'optimalPagination' | 'strictPagination' | 'baselineGridPt' |
  'microtype' | 'microtypeProtrusion' | 'fitToPages'> {
  /** Numeric uniform scale from 0.5 through 3; view zoom remains independent. */
  fontScale?: number;
  allowRawHtml?: false;
  /** Paper dimensions, orientation and margins in PDF points. Requires matching
   * page-capable (or baseline-grid) bindings; settings survive saving and reopening.
   */
  page?: FmdPdfPage;
}

export interface FmdNativeWorkspaceOutput extends FmdRenderOutput {
  format: 'interactive-html';
  mimeType: 'text/html;charset=utf-8';
  extension: 'html';
}

/** Complete data-only project. No executable JavaScript, WASM or file handles. */
export interface FmdNativeSourceProjectOutput {
  readonly schema: 'fmd-native-source-project-v1';
  readonly format: 'native-source-project';
  readonly mimeType: 'application/json';
  readonly extension: 'json';
  readonly sourceLength: number;
  /** Owned UTF-8 JSON. Mutating bytes changes text()/blob(), as with render output. */
  readonly bytes: Uint8Array;
  text(): string;
  blob(): Blob;
  /** Uses the .fmd.json suffix. */
  filename(baseName?: string): string;
}
export type FmdNativeSourceProjectInput = string | Uint8Array | ArrayBuffer | ArrayBufferView;
export interface FmdNativeSourceProject {
  readonly source: string;
  /** Compatible with native-workspace rendering and direct HTML/PDF rendering.
   * Options, arrays and image/font bytes are fresh, editable host-owned copies.
   * The runtime/backend is always selected explicitly by the host, not this data.
   */
  readonly options: FmdNativeWorkspaceOptions;
}

/** Synchronous data-only packaging; does not initialize WASM or start a worker. */
export function createNativeSourceProject(markdown: string,
  options?: FmdNativeWorkspaceOptions): FmdNativeSourceProjectOutput;
/** Strict UTF-8/JSON/schema/geometry/resource admission, with no rendering or I/O. */
export function readNativeSourceProject(input: FmdNativeSourceProjectInput): FmdNativeSourceProject;
export const NATIVE_SOURCE_PROJECT_LIMITS: Readonly<{
  sourceBytes: number; assetBytes: number; totalAssetBytes: number; imageCount: number;
  destinationBytes: number; totalDestinationBytes: number; outputBytes: number;
}>;

export interface FmdNativeWorkspaceExporter {
  /** Pure synchronous export once the selected runtime has initialized.
   * Source and assets are not rewritten or retained between render calls.
   * The output contains the engine, source, resources and editable application.
   */
  render(markdown: string, options?: FmdNativeWorkspaceOptions): FmdNativeWorkspaceOutput;
  /** Read a data-only project and render it using this factory's trusted engine.
   * This never imports code, selects a runtime or rewrites source from the file.
   */
  renderProject(input: FmdNativeSourceProjectInput): FmdNativeWorkspaceOutput;
}

/** Initialize one isolated runtime and reuse the exporter across documents.
 * No URLs are fetched by the exporter; supplied trusted JS initializes supplied
 * WASM bytes. Older binaries without the native editor controller are rejected.
 */
export function createNativeWorkspaceExporter(runtime: FmdNativeWorkspaceRuntime): Promise<FmdNativeWorkspaceExporter>;

export const NATIVE_WORKSPACE_LIMITS: Readonly<{
  sourceBytes: number; wasmBytes: number; bindingsBytes: number; assetBytes: number;
  totalAssetBytes: number; imageCount: number; destinationBytes: number;
  totalDestinationBytes: number; outputBytes: number;
}>;
