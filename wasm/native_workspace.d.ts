import type {FmdRenderOptions, FmdRenderOutput} from './franken_markdown.js';

/** Trusted artifacts from ONE matching wasm-bindgen --target web build.
 * Supply file contents, never URLs. Byte views are snapshotted before loading.
 * Binding JavaScript is executable application code, not untrusted document data.
 */
export interface FmdNativeWorkspaceRuntime {
  wasm: Uint8Array | ArrayBuffer | ArrayBufferView;
  bindings: string;
}

/** Settings supported by both the saved workspace and its native render backend.
 * Unsupported settings throw, including customCss, page and raw HTML passthrough.
 */
export interface FmdNativeWorkspaceOptions extends Pick<FmdRenderOptions,
  'font' | 'darkMode' | 'title' | 'author' | 'lang' | 'metadataEpochSeconds' |
  'pageNumbers' | 'codeLineNumbers' | 'toc' | 'tocDepth' | 'pdfImages' | 'fontAssets'> {
  /** Numeric uniform scale from 0.5 through 3; view zoom remains independent. */
  fontScale?: number;
  allowRawHtml?: false;
}

export interface FmdNativeWorkspaceOutput extends FmdRenderOutput {
  format: 'interactive-html';
  mimeType: 'text/html;charset=utf-8';
  extension: 'html';
}

export interface FmdNativeWorkspaceExporter {
  /** Pure synchronous export once the selected runtime has initialized.
   * Source and assets are not rewritten or retained between render calls.
   * The output contains the engine, source, resources and editable application.
   */
  render(markdown: string, options?: FmdNativeWorkspaceOptions): FmdNativeWorkspaceOutput;
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
