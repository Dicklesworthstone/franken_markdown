import type { FmdPdfPage, FmdPdfRunningImage } from "./franken_markdown.js";

/** One chapter. Array order is reading order; paths are book-relative. */
export interface BookFile {
  path: string;
  source: string;
}
export type BookAssetBytes = Uint8Array | ArrayBuffer | ArrayBufferView;
export type BookFontSlot =
  | "body-regular"
  | "body-bold"
  | "body-italic"
  | "body-bold-italic"
  | "mono-regular";

export interface BookImage {
  /** For guide/start.md using figure.svg, supply guide/figure.svg. */
  destination: string;
  bytes: BookAssetBytes;
}

export interface BookFont {
  slot: BookFontSlot;
  bytes: BookAssetBytes;
  /** Integer CSS weight, 1..=1000. */
  weight?: number;
}

/** Template text is bounded to 4096 UTF-8 bytes per slot. */
export interface BookRunningBand {
  left?: string;
  center?: string;
  right?: string;
  rule?: boolean;
  /** Image dest is a book-root key from options.images. Requires a matching
   * FmdBook.setPdfOptionsWithRunningImages binding; older packages reject it. */
  image?: FmdPdfRunningImage;
}
export interface BookRunningContent {
  header?: BookRunningBand;
  footer?: BookRunningBand;
  /** Also suppresses the pageNumbers footer on the first page. */
  skipFirstPage?: boolean;
}

export interface BookOptions {
  /** Expand includes in Rust (default true). False preserves literal source. */
  expandIncludes?: boolean;
  /** Explicit include-only UTF-8 files; never chapters or EPUB spine items.
   * Paths are literal, case-sensitive, relative to the logical book root.
   * Chapters and resources share the 4096-source and 64 MiB text/path limits.
   * Resources are snapshotted before initialization or worker transfer.
   * Requires expandIncludes; no filesystem or network lookup is performed.
   */
  includeSources?: readonly BookFile[];
  title?: string;
  /** PDF author metadata. */
  author?: string;
  lang?: string;
  font?: "sans" | "serif";
  darkMode?: "auto" | "disabled" | "light" | "system";
  /** Replacement HTML/EPUB CSS; the empty string is preserved. */
  customCss?: string;
  /** Finite, positive numeric scale. String presets are not accepted here. */
  fontScale?: number;
  toc?: boolean;
  pageNumbers?: boolean;
  /** Comma-separated homogeneous, antiriver, pareto, optimal-pagination,
   * protrusion or expansion. Unknown tokens fail before WASM initialization.
   * Requires FmdBook.setPdfOptions; old packages fail explicitly.
   */
  typography?: string;
  optimalPagination?: boolean;
  /** An explicit mode overrides microtype tokens in typography. */
  microtype?: "off" | "protrusion" | "expansion" | "all";
  /** Compatibility shortcut, applied after microtype. */
  microtypeProtrusion?: boolean;
  /** Body size in points, 6..=24. */
  baseFontSize?: number;
  /** Heading ratio, 1.05..=2. */
  headingScale?: number;
  /** Table size in points, 5..=24. */
  tableFontSize?: number;
  /** In-content TOC depth, integer 1..=6; also applies to HTML. */
  tocDepth?: number;
  /** Adaptive target, integer 1..=4294967295; not a hard page-count guarantee. */
  fitToPages?: number;
  codeLineNumbers?: boolean;
  /** Nonnegative safe-integer epoch; no ambient clock is read. */
  metadataEpochSeconds?: number;
  running?: BookRunningContent;
  /** HTML-site font container only; EPUB keeps its TrueType package. */
  htmlFontFormat?: "ttf" | "woff1" | "woff2";
  /** Default paper/margins for PDF exports only; HTML/EPUB are unaffected.
   * Uses the single-document point-based geometry contract. Captured before
   * asynchronous initialization or worker transfer, with no host references.
   * An explicit page requires FmdBook.renderPdfWithPage in the WASM package;
   * unsupported packages fail rather than silently using the wrong paper.
   */
  page?: FmdPdfPage;
  images?: readonly BookImage[];
  /** Explicit host faces apply to PDF/HTML and opt EPUB into shared TrueType
   * subsets across all chapters. Missing slots then use bundled faces.
   * EPUB custom CSS stays verbatim and can override generated font families.
   * With no supplied faces, EPUB retains its historical font-free output.
   * Requires a matching rebuilt Rust/WASM renderer; no external font fetches.
   */
  fontAssets?: readonly BookFont[];
}

export interface BookOutput {
  readonly format: "book-pdf" | "book-epub" | "book-site";
  readonly mimeType: "application/pdf" | "application/epub+zip" | "application/zip";
  readonly extension: "pdf" | "epub" | "zip";
  /** Original chapter plus include-source UTF-8 bytes, counted once each. */
  readonly sourceLength: number;
  /** An owned binary output, valid after session disposal. */
  readonly bytes: Uint8Array;
  blob(): Blob;
  filename(baseName?: string): string;
}

/** A committed source transaction. Counts describe native parser work, not
 * rendered-output invalidations. Source changes may reparse no chapters when
 * only unused resources or unselected include ranges change. */
export interface BookSourceUpdate {
  readonly revision: number;
  readonly sourceLength: number;
  readonly chapterCount: number;
  readonly changedSources: number;
  /** Strictly increasing, zero-based indexes in the unchanged reading order. */
  readonly reparsedChapters: readonly number[];
}
export interface BookSourceUpdateOptions {
  /** Expected source revision, integer 0..=4294967295. Omitted means capture the
   * current revision at call entry. Supply explicitly for asynchronously
   * prepared edits; stale requests fail without replacing any source. */
  expectedRevision?: number;
}

/** Receipt for replacing the complete ordered chapter/include source set. */
export interface BookSourceSetUpdate {
  readonly revision: number;
  readonly sourceLength: number;
  readonly chapterCount: number;
  readonly resourceCount: number;
  readonly changed: boolean;
  /** Changed sets rebuild every chapter against the new map; exact no-ops
   * retain the AST and report zero. Use updateSources for selective parsing. */
  readonly reparsedChapterCount: number;
}
export interface BookSourceSetOptions extends BookSourceUpdateOptions {
  /** The COMPLETE next include-only capture. Omitted/undefined means empty,
   * not retain previous resources. Requires an expanding book. Chapter and
   * resource paths/text together are limited to 4096 sources and 64 MiB. */
  includeSources?: readonly BookFile[];
}

/** Parsed WASM book. Dispose in a finally block when repeated exports finish. */
export interface BookSession {
  readonly chapterCount: number;
  /** Original chapter plus include-source UTF-8 bytes, counted once each. */
  readonly sourceLength: number;
  /** Starts at zero; advances once per changed source batch. Exact no-ops,
   * assets and presentation edits do not advance it. Throws on old WASM. */
  readonly sourceRevision: number;
  /** Atomically replace existing chapter/include sources, preserving all render
   * settings/assets and unchanged parsed chapters. No source is added, removed,
   * renamed or reordered. Expansion policy is fixed by createBook options.
   * Synchronous: no Promise, worker queue or cancellation is implied.
   * Rejects bad input/stale revisions/includes while preserving the last book.
   * A malformed successful native report instead disposes the session because
   * its committed source state cannot safely be verified.
   */
  updateSources(files: readonly BookFile[], options?: BookSourceUpdateOptions): BookSourceUpdate;
  /** Replace the COMPLETE ordered chapter list and include-only capture.
   * Supports adding/removing/renaming/reordering selected sources while keeping
   * settings, assets and PDF page defaults. Does not write/delete host files or
   * rewrite source links. Expansion policy is fixed at createBook time.
   * Shares optimistic revisions and the admission guard with updateSources.
   * Rejections keep the previous book; an unverifiable successful native
   * receipt instead disposes it. Old WASM throws UNSUPPORTED_BOOK_SOURCE_SET.
   * Synchronous: no worker queue, Promise or cancellation is implied.
   */
  replaceSources(files: readonly BookFile[], options?: BookSourceSetOptions): BookSourceSetUpdate;
  setImage(destination: string, bytes: BookAssetBytes): BookSession;
  setFont(slot: BookFontSlot, bytes: BookAssetBytes, weight?: number): BookSession;
  /** Synchronous rendering after asynchronous session creation. An optional
   * page overrides only this export, without reparsing or changing defaults.
   * Omitted/undefined page uses BookOptions.page; page: {} requests Letter
   * with 72-point margins. No other per-export option is accepted.
   */
  renderPdf(options?: Pick<BookOptions, "page">): BookOutput;
  renderEpub(): BookOutput;
  renderSite(): BookOutput;
  /** Whether this live WASM book implements the additive chapter-preview ABI. */
  readonly supportsChapterPreview: boolean;
  /** Render only the selected zero-based chapter (default 0), with a complete
   * ordered navigation map. Synchronous; no ZIP, worker, or cancellation is
   * implied. Requires rebuilt WASM; old packages fail explicitly rather than
   * rendering the whole site. The bounded JSON reply is checked against the
   * requested index and chapter count. HTML is not sanitized for host insertion.
   */
  renderChapterPreview(selected?: number): BookChapterPreviewOutput;
  /** Check local HTML navigation on the retained AST without rendering pages. */
  validateLinks(): BookLinksOutput;
  /** Idempotent. All subsequent operations except dispose throw. */
  dispose(): void;
}

/** Expand selected sources and parse once. Uses the main renderer's shared init. */
export function createBook(files: readonly BookFile[], options?: BookOptions): Promise<BookSession>;
/** One-shot exports always dispose their WASM book, including on failure. */
export function renderBookPdf(
  files: readonly BookFile[],
  options?: BookOptions,
): Promise<BookOutput>;
export function renderBookEpub(
  files: readonly BookFile[],
  options?: BookOptions,
): Promise<BookOutput>;
export function renderBookSite(
  files: readonly BookFile[],
  options?: BookOptions,
): Promise<BookOutput>;

export interface BookLinkFinding {
  readonly code:
    | "missing_chapter"
    | "missing_anchor"
    | "ambiguous_anchor"
    | "invalid_fragment"
    | "invalid_local_destination"
    | "missing_footnote";
  readonly destination: string;
  readonly message: string;
}
export interface ChapterLinkReport {
  readonly path: string;
  readonly checked: number;
  readonly external: number;
  readonly unchecked: number;
  readonly findings: readonly BookLinkFinding[];
}
export interface BookLinkReport {
  readonly schema: "fmd-book-link-report-v1";
  readonly scope: "expanded-html-navigation";
  readonly chapters: readonly ChapterLinkReport[];
  readonly summary: Readonly<{
    chapters: number;
    checked: number;
    external: number;
    unchecked: number;
    findings: number;
  }>;
}
export interface BookLinksOutput {
  readonly format: "book-links";
  readonly mimeType: "application/json";
  readonly extension: "json";
  readonly sourceLength: number;
  /** UTF-8 fmd-book-link-report-v1 JSON. Not PDF/EPUB conformance or a network check. */
  readonly bytes: Uint8Array;
  blob(): Blob;
  filename(baseName?: string): string;
}
/** Expand and check with guaranteed disposal. Only includeSources and
 * expandIncludes are used; fonts, images and presentation settings are ignored.
 * Use the worker "links" route for cancellation of synchronous Rust work. */
export function checkBookLinks(
  files: readonly BookFile[],
  options?: BookOptions,
): Promise<BookLinksOutput>;
/** Validate report bytes against canonical, normalized chapter paths in reading
 * order. Recomputes totals and freezes the result. Does not parse Markdown. */
export function parseBookLinkReport(
  bytes: Uint8Array,
  expectedPaths: readonly string[],
): BookLinkReport;

/** Metadata for every chapter, in Rust-defined reading order. */
export interface BookChapterPreviewPage {
  readonly path: string;
  readonly source: string;
  readonly title: string;
}
export interface BookChapterPreview {
  readonly schema: "fmd-book-chapter-preview-v1";
  readonly selected: number;
  readonly pages: readonly BookChapterPreviewPage[];
  /** Only the selected chapter's HTML; untrusted content requiring isolation. */
  readonly html: string;
}
export interface BookChapterPreviewOutput {
  readonly format: "book-chapter-preview";
  readonly mimeType: "application/json";
  readonly extension: "json";
  readonly sourceLength: number;
  /** Owned UTF-8 fmd-book-chapter-preview-v1 JSON, valid after disposal. */
  readonly bytes: Uint8Array;
  blob(): Blob;
  filename(baseName?: string): string;
}
/** Create a book, render one chapter, and dispose even when rendering fails. */
export function renderBookChapterPreview(
  files: readonly BookFile[],
  selected?: number,
  options?: BookOptions,
): Promise<BookChapterPreviewOutput>;
/** Validate bounded UTF-8 JSON and optionally bind it to a requested index and
 * count. Allows 4096 metadata entries, 8 MiB selected HTML, 64 MiB wire bytes.
 * Returns deeply frozen metadata. Does not sanitize or execute the HTML. */
export function parseBookChapterPreview(
  bytes: Uint8Array,
  expectedSelected?: number,
  expectedCount?: number,
): BookChapterPreview;
