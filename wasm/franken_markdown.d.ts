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

export type FmdFontCollectionBytes = ArrayBuffer | ArrayBufferView;
export type FmdFontCollectionErrorCode =
  | "INVALID_FONT_BYTES"
  | "FONT_COLLECTION_NOT_COLLECTION"
  | "FONT_COLLECTION_UNSUPPORTED_VERSION"
  | "FONT_COLLECTION_UNSUPPORTED_FACE_FORMAT"
  | "FONT_COLLECTION_FACE_INDEX"
  | "FONT_COLLECTION_MALFORMED"
  | "FONT_COLLECTION_DUPLICATE_TABLE"
  | "FONT_COLLECTION_MISSING_TABLE"
  | "FONT_COLLECTION_LIMIT"
  | "FONT_COLLECTION_FAILED"
  | "UNSUPPORTED_WASM_PACKAGE";

/** Bounded input/extractor errors. Initialization failures retain their original errors. */
export class FontCollectionError extends Error {
  readonly code: FmdFontCollectionErrorCode;
  constructor(code: FmdFontCollectionErrorCode, message: string);
}

export interface FmdDiagnostic {
  severity: "warning" | "error";
  start: number;
  end: number;
  message: string;
  /** Stable renderer reason code when supplied (for example svg_image_missing). */
  code?: string;
  /** Document-level findings use start=end=0, not an inferred source position. */
  scope?: "document";
}

export interface FmdPdfImageAsset {
  /** Markdown image destination, for example `images/chart.png` from `![Chart](images/chart.png)`. */
  destination: string;
  /** Host-owned bytes; SVG supports PNG/JPEG/SVG, HTML/PDF support depends on their decoder. */
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
  /** Standalone TrueType/OpenType bytes. Use extractFontCollectionFace for TTC/OTC inputs. */
  bytes: Uint8Array | ArrayBuffer | ArrayBufferView;
  /**
   * Optional CSS `font-weight` pin (integer 1..=1000) for variable `wght` faces.
   * Static faces ignore the pin. When `body-bold` is omitted and `body-regular`
   * is a variable face, bold instances from that same file at 700 (or this pin).
   */
  weight?: number;
}

/** PDF typography; omitted controls preserve the compact print layout. */
export interface FmdPdfTypography {
  /** Gradual adjacent-line demerits for more homogeneous spacing. */
  homogeneous?: boolean;
  /** Penalize white rivers through justified paragraphs. */
  antiriver?: boolean;
  /** Retain a bounded Pareto front of paragraph alternatives. */
  pareto?: boolean;
  /** Use literal theme leading and table padding. Default false. */
  themeSpacing?: boolean;
  /** Body leading multiplier, finite 1..4; implies themeSpacing. */
  lineHeight?: number;
  /** Horizontal padding per cell side in em, finite 0..8; implies themeSpacing. */
  tableCellPaddingXEm?: number;
  /** Vertical padding per cell side in em, finite 0..8; implies themeSpacing. */
  tableCellPaddingYEm?: number;
}

export type FmdAuthoringProfile = "commonmark-gfm" | "gfm-plus";

export interface FmdRenderOptions {
  /** Single-document authoring dialect. Omission retains the extended dialect.
   * commonmark-gfm keeps definition lists literal; gfm-plus enables them.
   * An explicit profile requires matching profiled WASM bindings. */
  profile?: FmdAuthoringProfile;
  font?: FmdFont;
  darkMode?: FmdDarkMode;
  /** HTML/PDF/EPUB title; omitted metadata inherits source frontmatter with matching WASM bindings. */
  title?: string;
  /** Ordinary HTML/EPUB replacement stylesheet. Empty explicitly disables defaults. */
  customCss?: string;
  allowRawHtml?: boolean;
  /** Verbatim HTML/PDF/EPUB author; omitted values inherit source frontmatter.
   * HTML's explicit empty string suppresses inheritance; PDF/EPUB empty values inherit.
   * HTML/EPUB authors require their matching render*ConfiguredMetadata binding;
   * older packages fail explicitly. HTML/EPUB author text is limited to 64 KiB UTF-8. */
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
  /** BCP-47 language tag used for metadata and hyphenation. HTML/PDF/EPUB inherit frontmatter when omitted. */
  lang?: "en" | "de" | "fr" | "es" | "nl" | string;
  /** HTML/PDF/SVG/EPUB document TOC. Omitted inherits frontmatter; explicit false overrides it. */
  toc?: boolean;
  /** HTML/PDF/SVG/EPUB TOC heading depth (1...6); omitted inherits frontmatter toc_depth. */
  tocDepth?: number;
  /** Adaptive PDF page-count target. */
  fitToPages?: number;
  /**
   * Microtypography for justified PDF text: "protrusion" (optical-margin
   * protrusion with a small glyph-expansion budget) or "expansion" (glyph
   * expansion only).
   */
  microtype?: "disabled" | "protrusion" | "expansion";
  /** Boolean alias for microtype: "protrusion". */
  microtypeProtrusion?: boolean;
  /** Standalone SVG poster width in points (144..14400; default 612). */
  maxWidthPt?: number;
  /** HTML font subset container: "woff1" (default), "woff2" (smallest) or "ttf". */
  htmlFontFormat?: "woff1" | "woff2" | "ttf";
  /** HTML/EPUB font policy. Full retains complete cmap and OpenType layout tables,
   * including bundled fonts. Default subset keeps compact document-specific fonts.
   * Requires matching WASM bindings; PDF/SVG font policies are independent. */
  fontEmbedding?: "subset" | "full";
  /** Opt-in PDF line breaking and literal theme spacing. */
  typography?: FmdPdfTypography;
  /** Globally optimal (Plass) PDF page breaking instead of greedy. */
  optimalPagination?: boolean;
  /** Host-supplied image bytes for HTML, PDF, EPUB and SVG; each export enforces resource limits. */
  pdfImages?: FmdPdfImageAsset[];
  /** Host-supplied TrueType font bytes by renderer slot. */
  fontAssets?: FmdFontAsset[];
}

/** PDF paper dimensions/margins are points, never CSS pixels. */
export interface FmdPdfPage {
  /** Omitted size is Letter. Custom dimensions must each be 144..14400 points. */
  size?: "letter" | "a4" | "a5" | "legal" | "tabloid" | { widthPt: number; heightPt: number };
  /** Rotate only paper; omitted orientation preserves custom width/height order. */
  orientation?: "portrait" | "landscape";
  /** Uniform points or independent sides, each defaulting to 72 points. */
  margins?: number | { topPt?: number; rightPt?: number; bottomPt?: number; leftPt?: number };
}
/** Optional repeated image from explicit host assets. */
export interface FmdPdfRunningImage {
  /** Exact key in the explicitly supplied pdfImages. No URL or file is loaded. */
  dest: string;
  /** Image occupies the left or right edge of the band; default left. */
  position?: "left" | "right";
  /** Positive integer points, at most 65535. Omitted height fits the margin. */
  heightPt?: number;
}
/** One running band. Slot templates accept {page}, {pages}, {title}, {chapter},
 * {section}, {author} and {date} (from metadataEpochSeconds, never the clock).
 * Chapter/section follow the final body's page context; unknown tokens stay literal. */
export interface FmdPdfRunningBand {
  left?: string;
  center?: string;
  right?: string;
  /** Hairline under the header / over the footer. */
  rule?: boolean;
  /** Repeated logo from host image bytes. Text is fitted beside its reserved area. */
  image?: FmdPdfRunningImage;
}
/** Opt-in PDF running header/footer drawn in the page margins. */
export interface FmdPdfRunning {
  header?: FmdPdfRunningBand;
  footer?: FmdPdfRunningBand;
  /** Leave page 1 bare (also suppresses pageNumbers there). */
  skipFirstPage?: boolean;
}
export interface FmdPdfRenderOptions extends FmdRenderOptions {
  /** Single-document PDF/A-2b emission; default/off retains ordinary PDF.
   * Requires matching archival WASM bindings. This selects the native writer;
   * it is not independent PDF/A validation or certification. */
  pdfA?: "off" | "2b";
  /** Apply the native archival writer's strict policy. Requires pdfA: "2b".
   * Native refusal never falls back to an ordinary PDF. */
  pdfAStrict?: boolean;
  /** Require exact page breaking for the final measured lines; implies
   * optimalPagination. Default false. Refuses fallback with an Error carrying
   * a PAGINATION_* code (for example PAGINATION_INFEASIBLE or
   * PAGINATION_BUDGET_EXCEEDED). Requires a matching WASM token parser. */
  strictPagination?: boolean;
  /** Link PDF note numbers back to their first citation. Default false. */
  footnoteBacklinks?: boolean;
  /** Continue long text notes across page feet. Default false.
   * Supports optimal/strict pagination and baseline grids. Exactness covers body
   * boundaries and admitted paragraph shapes under the fixed note-allocation
   * policy; bounded search failures retain baseline shapes with diagnostics.
   * Exact note-prose splits retain at least two measured rows in each fragment
   * and its remaining tail; whole one-line paragraphs remain legal.
   * Requires a matching WASM package; older token
   * parsers fail explicitly. */
  splitFootnotes?: boolean;
  /** Opt-in PDF body baseline spacing in points, 1..144 (rounded to 0.001 pt).
   * Omitted/0 keeps the grid disabled. Incompatible with fitToPages. Requires
   * matching baseline-grid WASM bindings; old packages fail explicitly. */
  baselineGridPt?: number;
  /** PDF layout for single documents and books; margins must leave a 72-point content rectangle. */
  page?: FmdPdfPage;
  /** Running header/footer for single-document PDF renders. A band that does not fit its margin fails the render. */
  running?: FmdPdfRunning;
}

/** Layout/authoring verification using the same options and supplied bytes as
 * PDF export. This does not emit or independently inspect a PDF file and does
 * not certify PDF/A or PDF/UA. Other-format settings and pdfA/pdfAStrict are
 * rejected. A fit target reports the resulting page count, not a hard cap. */
export interface FmdPdfVerifyOptions extends Pick<FmdPdfRenderOptions,
  "profile" | "font" | "darkMode" | "fontScale" | "typeSize" | "title" | "author" |
  "metadataEpochSeconds" | "allowRawHtml" | "codeLineNumbers" | "pageNumbers" |
  "baseFontSize" | "headingScale" | "tableFontSize" | "lang" | "toc" | "tocDepth" |
  "fitToPages" | "microtype" | "microtypeProtrusion" | "typography" |
  "optimalPagination" | "strictPagination" | "footnoteBacklinks" | "splitFootnotes" |
  "pdfImages" | "fontAssets" | "page" | "running" | "baselineGridPt"> {}

/** Complete native schema-v1 verification report, bounded to 8 MiB UTF-8 JSON.
 * Coordinates describe the native body text layer in points. Running bands are
 * admitted with their actual margins/assets but are absent from these runs. */
export interface FmdPdfVerificationReport {
  readonly schema_version: "1";
  readonly target: "pdf";
  readonly profile?: FmdAuthoringProfile;
  readonly verdict: "clean" | "findings";
  readonly page_count: number;
  /** Native FNV-1a change detector for the canonical report, not a signature. */
  readonly digest: string;
  readonly anchors: { readonly resolved: number; readonly unresolved: readonly string[] };
  readonly findings: readonly { readonly code: string; readonly detail: string }[];
  readonly pages: readonly {
    readonly number: number;
    readonly runs: readonly {
      readonly text: string; readonly x: number; readonly y: number;
      readonly size: number; readonly kind: string;
    }[];
  }[];
}

/** Lightweight editable HTML. Configured styles, full font faces, images and
 * navigation survive edits and Save HTML; Markdown editing uses its documented
 * JavaScript subset. Raw passthrough and PDF-only options are rejected.
 * Non-default presentation requires matching configured-workspace bindings.
 * An explicit authoring profile requires the matching profiled binding and is
 * retained across edits, Save HTML, reopening and browser printing.
 */
export interface FmdInteractiveRenderOptions extends Pick<FmdRenderOptions,
  "profile" | "font" | "darkMode" | "title" | "lang" | "fontScale" | "typeSize" | "customCss" |
  "toc" | "tocDepth" | "pdfImages" | "fontAssets" | "htmlFontFormat"> {
  allowRawHtml?: false;
}

export interface FmdRenderOutput {
  /** Present only after a matching native renderer applied an explicit dialect. */
  readonly profile?: FmdAuthoringProfile;
  format: FmdOutputFormat;
  mimeType: string;
  extension: "html" | "pdf" | "svg" | "epub" | "zip";
  sourceLength: number;
  bytes: Uint8Array;
  diagnostics: FmdDiagnostic[];
  /** Present only after successful single-document archival emission. */
  readonly pdfA?: "2b";
  readonly pdfAStrict?: boolean;
  text(): string;
  blob(): Blob;
  filename(baseName?: string): string;
}

/** Visual revision review, using one grammar for both source revisions. */
export interface FmdDiffRenderOutput extends FmdRenderOutput {
  readonly format: "diff-html";
  /** Present only when the explicitly selected native grammar ran. */
  readonly profile?: FmdAuthoringProfile;
}

/** Single-page SVG options. Assets are snapshotted before asynchronous init;
 * a matching resource-capable WASM binary is required when assets are supplied.
 * TOC options require the navigation-capable binding; explicit markers also
 * render linked outlines in matching packages. Explicit darkMode selects the
 * shared theme palette; Auto is deterministic light.
 */
export interface FmdSvgRenderOptions extends Pick<FmdRenderOptions,
  "profile" | "font" | "darkMode" | "fontScale" | "typeSize" | "maxWidthPt" | "pdfImages" | "fontAssets" | "toc" | "tocDepth"> {}
export interface FmdSvgRenderOutput extends FmdRenderOutput {
  format: "svg";
  mimeType: "image/svg+xml";
  extension: "svg";
}

/** Explicit source bundle shared by one-shot book publications. */
export interface FmdBookSourceOptions {
  /** Expand selected sources in Rust (default true); never read external files.
   * Disabling expansion requires includeSources to be empty.
   */
  expandIncludes?: boolean;
  /** Additional UTF-8 include resources. These never become chapter pages.
   * Resources may be non-Markdown text, such as a selected code snippet.
   * Chapters/resources share the 4096-source and 64 MiB text/path budget.
   */
  includeSources?: readonly FmdBookFile[];
}

/** Root one-shot PDF options. Include resources or disabled expansion require
 * renderBookPdfConfiguredSources in the matching generated WASM package.
 * Chapter-aware PDF/A remains available from the ./book entry point.
 */
export interface FmdBookPdfOptions extends Omit<FmdPdfRenderOptions, "pdfA" | "pdfAStrict">,
  FmdBookSourceOptions {}

/** Canonical offline HTML-site options. PDF-only settings are rejected rather
 * than silently discarded. Parsing remains safe; raw HTML is not passed through.
 */
export interface FmdBookSiteOptions extends Pick<FmdRenderOptions,
  "profile" | "font" | "darkMode" | "title" | "customCss" | "fontScale" | "typeSize" |
  "lang" | "toc" | "tocDepth" | "pdfImages" | "fontAssets" | "fontEmbedding">,
  FmdBookSourceOptions {
  allowRawHtml?: false;
}
/** Book PDF output acknowledges an explicitly selected authoring dialect. */
export interface FmdBookPdfOutput extends FmdRenderOutput {
  readonly profile?: FmdAuthoringProfile;
}

export interface FmdBookSiteOutput extends Omit<FmdRenderOutput, "format" | "mimeType" | "extension"> {
  /** Present after the matching native exporter applied an explicit authoring dialect. */
  readonly profile?: FmdAuthoringProfile;
  /** Present after the matching native exporter successfully retained full fonts. */
  readonly fontEmbedding?: "full";
  format: "book-site";
  mimeType: "application/zip";
  extension: "zip";
}

export interface FmdCapabilities {
  schema: "fmd-wasm-capabilities-v1";
  outputs: FmdOutputFormat[];
  input: "markdown_utf8";
  /** Present in packages that expose the bounded TTC/OTC extractor. */
  font_collections?: {
    formats: ["ttc", "otc"];
    versions: [1, 2];
    max_bytes: 33554432;
    max_faces: 1024;
    output: "standalone_sfnt";
  };
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
  fontCollectionFaceCount(bytes: FmdFontCollectionBytes): Promise<number>;
  extractFontCollectionFace(bytes: FmdFontCollectionBytes, faceIndex?: number): Promise<Uint8Array>;
  accessibilityAudit(markdown: string, options?: FmdAnalysisOptions): Promise<FmdAccessibilityReport>;
  documentStats(markdown: string, options?: FmdAnalysisOptions): Promise<FmdDocumentStats>;
  renderBookPdf(files: readonly FmdBookFile[], options?: FmdBookPdfOptions): Promise<FmdBookPdfOutput>;
  renderBookSite(files: readonly FmdBookFile[], options?: FmdBookSiteOptions): Promise<FmdBookSiteOutput>;
  renderEpub(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
  renderInteractiveHtml(markdown: string, options?: FmdInteractiveRenderOptions): Promise<FmdRenderOutput>;
  renderPdf(markdown: string, options?: FmdPdfRenderOptions): Promise<FmdRenderOutput>;
  verifyPdf(markdown: string, options?: FmdPdfVerifyOptions): Promise<FmdPdfVerificationReport>;
  renderSemanticDiff(
    oldMarkdown: string,
    newMarkdown: string,
    options?: FmdDiffOptions,
  ): Promise<FmdDiffRenderOutput>;
  renderSvg(markdown: string, options?: FmdSvgRenderOptions): Promise<FmdSvgRenderOutput>;
  searchIndex(markdown: string, options?: FmdAnalysisOptions): Promise<FmdSearchIndex>;
  semanticDiff(
    oldMarkdown: string,
    newMarkdown: string,
    options?: FmdDiffOptions,
  ): Promise<FmdSemanticDiff>;
}

export interface FmdBookFile {
  path: string;
  source: string;
}
export interface FmdDiffOptions extends FmdAnalysisOptions {
  oldName?: string;
  newName?: string;
}
export interface FmdFinding {
  severity?: string;
  code: string;
  message?: string;
  detail?: string;
}
/** Source grammar only; PDF layout and resources retain engine audit defaults. */
export interface FmdAnalysisOptions {
  profile?: FmdAuthoringProfile;
}
export interface FmdDocumentStats {
  schema: "fmd-document-stats-v1";
  /** Present only after an explicitly selected native authoring profile ran. */
  readonly profile?: FmdAuthoringProfile;
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
  readonly profile?: FmdAuthoringProfile;
  findings: FmdFinding[];
  [key: string]: unknown;
}
export interface FmdSearchIndex {
  schema: "fmd-search-index-v1";
  readonly profile?: FmdAuthoringProfile;
  entries: Array<{ kind: "heading" | "paragraph"; level?: number; anchor: string; text: string }>;
}
export interface FmdSemanticDiff {
  schema: "fmd-diff-v1";
  readonly profile?: FmdAuthoringProfile;
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

export function init(
  input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module,
): Promise<void>;
export function capabilities(): Promise<FmdCapabilities>;
/**
 * Inspect a TTC/OTC v1/v2 header and return 1..1024 faces. Individual face
 * contents are validated by extraction. Standalone TTF/OTF inputs are refused.
 * Snapshots exactly the supplied buffer/view before asynchronous initialization.
 * Input must be nonempty, attached, nonshared, and at most 32 MiB.
 * Requires matching generated WASM; stale packages fail with UNSUPPORTED_WASM_PACKAGE.
 */
export function fontCollectionFaceCount(bytes: FmdFontCollectionBytes): Promise<number>;
/**
 * Extract a zero-based face (default 0) as independent standalone TTF/OTF bytes
 * usable in every existing fontAssets slot. faceIndex must be an integer 0..1023;
 * the selected collection must contain it. The input is snapshotted before await
 * and is never modified or detached. Input and extracted face are bounded at
 * 32 MiB; malformed or unsupported fonts yield FontCollectionError.
 * Extraction rebuilds offsets/checksums and removes invalidated signatures.
 * Rendering performs its existing outline/metric and publication-budget checks.
 */
export function extractFontCollectionFace(bytes: FmdFontCollectionBytes, faceIndex?: number): Promise<Uint8Array>;
export function accessibilityAudit(markdown: string, options?: FmdAnalysisOptions): Promise<FmdAccessibilityReport>;
/** Verify a configured PDF layout, with bounded owned source/resources. Requires
 * matching verifyPdfConfigured WASM bindings; older packages fail explicitly. */
export function verifyPdf(markdown: string, options?: FmdPdfVerifyOptions): Promise<FmdPdfVerificationReport>;
export function documentStats(markdown: string, options?: FmdAnalysisOptions): Promise<FmdDocumentStats>;
/** Publish ordered chapters with the same PDF typography, running bands and
 * explicit pdfImages as renderPdf. Additional includeSources remain resources,
 * never extra chapters; expandIncludes defaults to true. The entire source
 * bundle, settings and assets are captured before initialization can yield.
 * Advanced settings require matching generated WASM; older packages reject
 * them with UNSUPPORTED_WASM_PACKAGE.
 * For chapter-aware PDF/A emission, use renderBookPdf from the ./book entry. */
export function renderBookPdf(
  files: readonly FmdBookFile[],
  options?: FmdBookPdfOptions,
): Promise<FmdBookPdfOutput>;
export function renderBookSite(
  files: readonly FmdBookFile[],
  options?: FmdBookSiteOptions,
): Promise<FmdBookSiteOutput>;
export function renderEpub(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderHtml(markdown: string, options?: FmdRenderOptions): Promise<FmdRenderOutput>;
export function renderInteractiveHtml(
  markdown: string,
  options?: FmdInteractiveRenderOptions,
): Promise<FmdRenderOutput>;
export function renderPdf(markdown: string, options?: FmdPdfRenderOptions): Promise<FmdRenderOutput>;
export function renderSemanticDiff(
  oldMarkdown: string,
  newMarkdown: string,
  options?: FmdDiffOptions,
): Promise<FmdDiffRenderOutput>;
export function renderSvg(markdown: string, options?: FmdSvgRenderOptions): Promise<FmdSvgRenderOutput>;
export function searchIndex(markdown: string, options?: FmdAnalysisOptions): Promise<FmdSearchIndex>;
export function semanticDiff(
  oldMarkdown: string,
  newMarkdown: string,
  options?: FmdDiffOptions,
): Promise<FmdSemanticDiff>;
export function createRenderer(
  input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module,
): Promise<FmdRenderer>;
