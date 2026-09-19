/** One chapter. Array order is reading order; paths are book-relative. */
export interface BookFile { path: string; source: string; }
export type BookAssetBytes = Uint8Array | ArrayBuffer | ArrayBufferView;
export type BookFontSlot =
  | "body-regular" | "body-bold" | "body-italic"
  | "body-bold-italic" | "mono-regular";

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
  /** Host fonts apply to PDF/HTML. EPUB font styling uses its stylesheet. */
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
  /** Idempotent. All subsequent operations except dispose throw. */
  dispose(): void;
}

/** Expand selected sources and parse once. Uses the main renderer's shared init. */
export function createBook(files: readonly BookFile[], options?: BookOptions): Promise<BookSession>;
/** One-shot exports always dispose their WASM book, including on failure. */
export function renderBookPdf(files: readonly BookFile[], options?: BookOptions): Promise<BookOutput>;
export function renderBookEpub(files: readonly BookFile[], options?: BookOptions): Promise<BookOutput>;
export function renderBookSite(files: readonly BookFile[], options?: BookOptions): Promise<BookOutput>;
