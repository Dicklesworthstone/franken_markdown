export type FmdOutputFormat = "html" | "pdf";
export type FmdFont = "sans" | "serif";
export type FmdDarkMode = "auto" | "disabled";

export interface FmdDiagnostic {
  severity: "warning" | "error";
  start: number;
  end: number;
  message: string;
}

export interface FmdPdfImageAsset {
  /** Markdown image destination, for example `images/chart.png` from `![Chart](images/chart.png)`. */
  destination: string;
  /** Browser-supplied image bytes. PNG and SVG assets are supported in PDF output. */
  bytes: Uint8Array | ArrayBuffer | ArrayBufferView;
}

export type FmdFontAssetSlot =
  | "body-regular"
  | "body-bold"
  | "body-italic"
  | "body-bold-italic"
  | "mono-regular";

export interface FmdFontAsset {
  /** Renderer font slot to replace. Missing slots use bundled deterministic fallback fonts. */
  slot: FmdFontAssetSlot;
  /** Browser-supplied TrueType font bytes. */
  bytes: Uint8Array | ArrayBuffer | ArrayBufferView;
}

export interface FmdRenderOptions {
  font?: FmdFont;
  darkMode?: FmdDarkMode;
  title?: string;
  customCss?: string;
  allowRawHtml?: boolean;
  author?: string;
  /** Finite non-negative integer seconds, <= Number.MAX_SAFE_INTEGER. */
  metadataEpochSeconds?: number;
  codeLineNumbers?: boolean;
  /** Host-supplied PDF image bytes; any number of assets may be supplied per render call. */
  pdfImages?: FmdPdfImageAsset[];
  /** Host-supplied TrueType font bytes by renderer slot. */
  fontAssets?: FmdFontAsset[];
}

export interface FmdRenderOutput {
  format: FmdOutputFormat;
  mimeType: string;
  extension: "html" | "pdf";
  sourceLength: number;
  bytes: Uint8Array;
  diagnostics: FmdDiagnostic[];
  text(): string;
  blob(): Blob;
  filename(baseName?: string): string;
}

export interface FmdCapabilities {
  schema: "fmd-wasm-capabilities-v1";
  outputs: FmdOutputFormat[];
  input: "markdown_utf8";
  html: {
    mime_type: "text/html; charset=utf-8";
    self_contained: boolean;
    custom_css_utf8: boolean;
    font_assets: "ttf_v0_host_supplied_bytes";
  };
  pdf: {
    mime_type: "application/pdf";
    deterministic_metadata_epoch: boolean;
    image_assets: "png_svg_v0_host_supplied_bytes";
    font_assets: "ttf_v0_host_supplied_bytes";
  };
  diagnostics: {
    source_spans: "byte_offsets";
    json: boolean;
  };
  runtime_assumptions: {
    filesystem: false;
    process: false;
    network: false;
    threads: false;
  };
  theme: unknown;
}

export interface FmdRenderer {
  capabilities(): Promise<FmdCapabilities>;
  renderHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderPdf(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
}

export function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<void>;
export function capabilities(): Promise<FmdCapabilities>;
export function renderHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderPdf(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function createRenderer(
  input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module
): Promise<FmdRenderer>;
