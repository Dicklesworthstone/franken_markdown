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

/** Parsed WASM book. Dispose in a finally block when repeated exports finish. */
export interface BookSession {
  readonly chapterCount: number;
  /** Original chapter plus include-source UTF-8 bytes, counted once each. */
  readonly sourceLength: number;
  setImage(destination: string, bytes: BookAssetBytes): BookSession;
  setFont(slot: BookFontSlot, bytes: BookAssetBytes, weight?: number): BookSession;
  /** Synchronous rendering after asynchronous session creation. */
  renderPdf(): BookOutput;
  renderEpub(): BookOutput;
  renderSite(): BookOutput;
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
