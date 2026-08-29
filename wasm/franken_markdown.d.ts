export type FmdOutputFormat =
  | "html"
  | "pdf"
  | "svg"
  | "epub"
  | "interactive-html"
  | "diff-html"
  | "book-site"
  | "book-pdf";
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
  /** Browser-supplied image bytes. PNG and SVG are supported in HTML and PDF output. */
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
  /**
   * Optional CSS `font-weight` pin (integer 1..=1000) for variable `wght` faces.
   * Static faces ignore the pin. When `body-bold` is omitted and `body-regular`
   * is a variable face, bold instances from that same file at 700 (or this pin).
   */
  weight?: number;
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
  /** Render running page numbers in the bottom margin of PDF pages. */
  pageNumbers?: boolean;
  /** Base body size override in points (clamped by the core to [6, 24]). */
  baseFontSize?: number;
  /** Uniform typographic scale factor (e.g. 1.125 = 112.5% / Large) or preset name ('xs' | 'sm' | 'compact' | 'md' | 'lg' | 'xl' | '2xl' | 'huge'). Scales both HTML and PDF uniformly. */
  fontScale?: number | string;
  /** Alias for fontScale. */
  typeSize?: number | string;
  /** Per-step heading ratio, e.g. 1.25 (Major Third); clamped to [1.05, 2]. */
  headingScale?: number;
  /** Nominal table cell size override in points; clamped to [5, base]. */
  tableFontSize?: number;
  /** BCP-47 language tag used for HTML metadata and language-aware hyphenation. */
  lang?: "en" | "de" | "fr" | "es" | "nl" | string;
  /** Generate a document table of contents. */
  toc?: boolean;
  /** Maximum generated TOC heading depth (1...6). */
  tocDepth?: number;
  /** Adaptive PDF page-count target. */
  fitToPages?: number;
  /** Enable optical-margin punctuation protrusion for justified PDF text. */
  microtype?: "disabled" | "protrusion";
  /** Boolean alias for microtype: "protrusion". */
  microtypeProtrusion?: boolean;
  /** Standalone SVG poster width in points. */
  maxWidthPt?: number;
  /** Host-supplied image bytes (HTML data URIs and PDF embedding); any number per render. */
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
    image_assets: "png_svg_v0_host_supplied_bytes";
    font_assets: "ttf_v0_host_supplied_bytes";
    font_slot_weight: "css_1_to_1000_variable_wght";
  };
  pdf: {
    mime_type: "application/pdf";
    deterministic_metadata_epoch: boolean;
    image_assets: "png_svg_v0_host_supplied_bytes";
    font_assets: "ttf_v0_host_supplied_bytes";
    font_slot_weight: "css_1_to_1000_variable_wght";
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
  accessibilityAudit(markdown: string): Promise<FmdAccessibilityReport>;
  documentStats(markdown: string): Promise<FmdDocumentStats>;
  renderBookPdf(files: FmdBookFile[], options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderBookSite(files: FmdBookFile[], options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderEpub(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderInteractiveHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderPdf(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderSemanticDiff(oldMarkdown: string, newMarkdown: string, options?: FmdDiffOptions): Promise<FmdRenderOutput>;
  renderSvg(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  searchIndex(markdown: string): Promise<FmdSearchIndex>;
  semanticDiff(oldMarkdown: string, newMarkdown: string, options?: FmdDiffOptions): Promise<FmdSemanticDiff>;
}

export interface FmdBookFile { path: string; source: string; }
export interface FmdDiffOptions { oldName?: string; newName?: string; }
export interface FmdFinding { severity?: string; code: string; message?: string; detail?: string; }
export interface FmdDocumentStats {
  schema: "fmd-document-stats-v1";
  bytes: number;
  lines: number;
  words: number;
  characters: number;
  sentences: number;
  syllables: number;
  reading_time_secs: number;
  speaking_time_secs: number;
  flesch_reading_ease: number;
  flesch_kincaid_grade: number;
  reading_ease_label: string;
  structure: Record<string, unknown>;
  outline: Array<{ level: number; text: string; slug: string }>;
  findings: FmdFinding[];
}
export interface FmdAccessibilityReport {
  schema_version: "1";
  target: "pdf";
  findings: FmdFinding[];
  [key: string]: unknown;
}
export interface FmdSearchIndex {
  schema: "fmd-search-index-v1";
  entries: Array<{ kind: "heading" | "paragraph"; level?: number; anchor: string; text: string }>;
}
export interface FmdSemanticDiff {
  schema: "fmd-diff-v1";
  old_name: string;
  new_name: string;
  stats: {
    unchanged_blocks: number;
    inserted_blocks: number;
    deleted_blocks: number;
    modified_blocks: number;
    words_inserted: number;
    words_deleted: number;
    similarity_ratio: number;
  };
  [key: string]: unknown;
}

export function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<void>;
export function capabilities(): Promise<FmdCapabilities>;
export function accessibilityAudit(markdown: string): Promise<FmdAccessibilityReport>;
export function documentStats(markdown: string): Promise<FmdDocumentStats>;
export function renderBookPdf(files: FmdBookFile[], options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderBookSite(files: FmdBookFile[], options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderEpub(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderInteractiveHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderPdf(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderSemanticDiff(oldMarkdown: string, newMarkdown: string, options?: FmdDiffOptions): Promise<FmdRenderOutput>;
export function renderSvg(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function searchIndex(markdown: string): Promise<FmdSearchIndex>;
export function semanticDiff(oldMarkdown: string, newMarkdown: string, options?: FmdDiffOptions): Promise<FmdSemanticDiff>;
export function createRenderer(
  input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module
): Promise<FmdRenderer>;
