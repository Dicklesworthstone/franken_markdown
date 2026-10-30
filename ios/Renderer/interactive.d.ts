import type { FmdRenderOptions, FmdRenderOutput } from './franken_markdown.js';

/** Executable code, not document data. Supply trusted wasm-bindgen --target web
 * JavaScript and the matching binary from the same build as the installed
 * package. No module URL or network lookup is inferred by this API.
 */
export interface FmdOfflineRuntime {
  /** Complete self-contained generated ES module source (1 byte through 4 MiB UTF-8). */
  bindings: string;
  /** Matching version-1 WASM bytes (up to 64 MiB); exact views are copied. */
  wasm: Uint8Array | ArrayBuffer | ArrayBufferView;
}

/** Settings captured before initialization and retained across saved generations.
 * Other render options are rejected, not silently discarded. Raw HTML and custom
 * CSS are not enabled in this offline, safe-parsed workspace API.
 */
export interface FmdOfflineWorkspaceOptions extends Pick<FmdRenderOptions,
  'font' | 'darkMode' | 'title' | 'lang' | 'toc' | 'tocDepth' | 'author' |
  'metadataEpochSeconds' | 'pageNumbers' | 'codeLineNumbers' | 'pdfImages' | 'fontAssets'> {
  /** Numeric 0.5..3 (default 1). View zoom is separate from PDF typography. */
  fontScale?: number;
}

/** Embed a native editable runtime into one offline HTML document. Requires a
 * matching rebuilt package whose bundled controller supports the native handoff;
 * old packages reject with error.code === 'UNSUPPORTED_WASM_PACKAGE'.
 *
 * Source is bounded to 32 MiB UTF-8, each image/font to 32 MiB, and runtime plus
 * assets to 128 MiB combined. Shared/detached buffers are rejected. A subsequent
 * native render failure is visible and does not switch to a reduced parser.
 * No npm publication or WASM rebuild is performed by this function.
 */
export function renderOfflineWorkspace(
  markdown: string,
  runtime: FmdOfflineRuntime,
  options?: FmdOfflineWorkspaceOptions,
): Promise<FmdRenderOutput>;
