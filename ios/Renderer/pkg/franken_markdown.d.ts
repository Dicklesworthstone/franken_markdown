/* tslint:disable */
/* eslint-disable */

/**
 * Render output object exposed to JavaScript.
 */
export class FmdRenderResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Recoverable parser diagnostics as stable JSON.
     */
    diagnosticsJson(): string;
    /**
     * Rendered output bytes. HTML is UTF-8; PDF is binary.
     */
    readonly bytes: Uint8Array;
    /**
     * Default file extension without a leading dot.
     */
    readonly extension: string;
    /**
     * Stable output format (`html`, `pdf`, `svg`, `epub`, `zip`, or `diff-html`).
     */
    readonly format: string;
    /**
     * Browser MIME type for Blob construction.
     */
    readonly mimeType: string;
    /**
     * Source size in bytes.
     */
    readonly sourceLength: number;
}

/**
 * Run the exact PDF verification pipeline and keep only authoring-time
 * accessibility findings.
 */
export function accessibilityAudit(markdown: string): string;

/**
 * Dependency-free capability contract as JSON.
 */
export function capabilities(): string;

/**
 * Compute the engine's document intelligence, readability, outline, and
 * structural-lint report without rendering a second time in the host.
 */
export function documentStats(markdown: string): string;

/**
 * Compile in-memory Markdown files into one continuous, bookmarked PDF book.
 */
export function renderBookPdf(paths: string[], sources: string[], title: string | null | undefined, author: string | null | undefined, font: string | null | undefined, dark_mode: string | null | undefined, font_scale: number | null | undefined, page_numbers: boolean): FmdRenderResult;

/**
 * Compile in-memory Markdown files into a deterministic, zero-JavaScript HTML
 * site ZIP. The host owns file selection; the core owns include expansion,
 * link rewriting, navigation, parsing, rendering, and the search index.
 */
export function renderBookSite(paths: string[], sources: string[], title?: string | null, font?: string | null, dark_mode?: string | null, font_scale?: number | null): FmdRenderResult;

/**
 * Render an EPUB 3 e-book through the same parser and HTML theme model.
 */
export function renderEpubConfigured(markdown: string, font?: string | null, dark_mode?: string | null, title?: string | null, lang?: string | null, font_scale?: number | null): FmdRenderResult;

/**
 * Render Markdown to self-contained HTML using default browser-safe options.
 *
 * # Errors
 * Returns a JavaScript error when rendering fails.
 */
export function renderHtml(markdown: string): FmdRenderResult;

/**
 * Render Markdown to self-contained HTML with browser package options.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 */
export function renderHtmlConfigured(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, custom_css: string | null | undefined, allow_raw_html: boolean): FmdRenderResult;

/**
 * Render Markdown to self-contained HTML with the complete browser option
 * set, including the core-owned uniform typographic scale.
 *
 * This additive entry point keeps the original narrow ABI stable while giving
 * the ergonomic JavaScript wrapper one canonical path for fonts, images, and
 * type scale. The scale is interpreted by [`WasmRenderOptions`] and therefore
 * changes the Rust-generated theme rather than patching the returned HTML.
 *
 * # Errors
 * Returns a JavaScript error when parallel image arrays are inconsistent, a
 * font asset is invalid, the scale is not positive and finite, or rendering
 * fails.
 */
export function renderHtmlConfiguredAdvanced(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, custom_css: string | null | undefined, allow_raw_html: boolean, font_scale: number | null | undefined, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, lang: string | null | undefined, toc: boolean, toc_depth?: number | null): FmdRenderResult;

/**
 * Render Markdown to self-contained HTML with fonts and any number of host
 * image assets (data-URI inlined). Parallel image arrays match the PDF multi
 * ABI so the JS wrapper can share flattening.
 *
 * # Errors
 * Returns a JavaScript error when the image arrays are inconsistent, an option
 * is invalid, or rendering fails.
 */
export function renderHtmlConfiguredMulti(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, custom_css: string | null | undefined, allow_raw_html: boolean, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array): FmdRenderResult;

/**
 * Render Markdown to self-contained HTML with browser package options and
 * caller-supplied font bytes.
 *
 * Empty byte arrays mean "use bundled fallback" for that slot.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 */
export function renderHtmlConfiguredWithFonts(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, custom_css: string | null | undefined, allow_raw_html: boolean, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array): FmdRenderResult;

/**
 * Render a self-hosting, single-file HTML workspace with its own editor,
 * preview, intelligence panel, and print/PDF path.
 */
export function renderInteractiveHtmlConfigured(markdown: string, font?: string | null, dark_mode?: string | null, title?: string | null, lang?: string | null, font_scale?: number | null): FmdRenderResult;

/**
 * Render Markdown to PDF using default browser-safe options.
 *
 * # Errors
 * Returns a JavaScript error when rendering fails.
 */
export function renderPdf(markdown: string): FmdRenderResult;

/**
 * Render Markdown to PDF with browser package options.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 */
export function renderPdfConfigured(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean): FmdRenderResult;

/**
 * Render Markdown to PDF with browser package options, ANY number of image
 * assets, and caller-supplied font bytes. This is the general form the JS
 * wrapper uses so multi-image documents reach native↔WASM parity (the
 * single-image entry points above remain for a narrower ABI).
 *
 * Images arrive as three parallel arrays — a destination per image, all image
 * bytes concatenated, and the byte length of each image — because wasm-bindgen
 * cannot pass a `Vec<Vec<u8>>` directly. Empty font byte arrays mean "use
 * bundled fallback" for that slot.
 *
 * # Errors
 * Returns a JavaScript error when the image arrays are inconsistent, an option
 * is invalid, or rendering fails.
 */
export function renderPdfConfiguredMulti(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, page_numbers: boolean, font_scale: number | null | undefined, lang: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, microtype_protrusion: boolean): FmdRenderResult;

/**
 * Render Markdown to PDF with browser package options, one optional image
 * asset, and caller-supplied font bytes.
 *
 * Empty image destination/bytes means "no image asset"; empty font byte arrays
 * mean "use bundled fallback" for that slot.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 */
export function renderPdfConfiguredWithAssets(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destination: string, image_bytes: Uint8Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array): FmdRenderResult;

/**
 * Render Markdown to PDF with one browser-supplied image asset.
 *
 * This dependency-free adapter is intentionally narrow: callers pass bytes
 * they already own (for example from a file picker or fetch handled outside
 * the core). The renderer never touches the browser filesystem or network.
 *
 * # Errors
 * Returns a JavaScript error when options are invalid or rendering fails.
 */
export function renderPdfConfiguredWithImage(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destination: string, image_bytes: Uint8Array): FmdRenderResult;

/**
 * Render a semantic AST diff as a self-contained visual HTML document.
 */
export function renderSemanticDiffHtml(old_markdown: string, new_markdown: string, old_name?: string | null, new_name?: string | null): FmdRenderResult;

/**
 * Render a standalone vector poster with glyph outlines embedded as paths.
 */
export function renderSvgConfigured(markdown: string, font?: string | null, dark_mode?: string | null, font_scale?: number | null, max_width_pt?: number | null): FmdRenderResult;

/**
 * Build the deterministic search index used by static-document experiences.
 */
export function searchIndex(markdown: string): string;

/**
 * Compute a semantic AST diff and return its stable JSON contract.
 */
export function semanticDiff(old_markdown: string, new_markdown: string, old_name?: string | null, new_name?: string | null): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_fmdrenderresult_free: (a: number, b: number) => void;
    readonly accessibilityAudit: (a: number, b: number, c: number) => void;
    readonly capabilities: (a: number) => void;
    readonly documentStats: (a: number, b: number, c: number) => void;
    readonly fmdrenderresult_bytes: (a: number, b: number) => void;
    readonly fmdrenderresult_diagnosticsJson: (a: number, b: number) => void;
    readonly fmdrenderresult_extension: (a: number, b: number) => void;
    readonly fmdrenderresult_format: (a: number, b: number) => void;
    readonly fmdrenderresult_mimeType: (a: number, b: number) => void;
    readonly fmdrenderresult_sourceLength: (a: number) => number;
    readonly renderBookPdf: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number) => void;
    readonly renderBookSite: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly renderEpubConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly renderHtml: (a: number, b: number, c: number) => void;
    readonly renderHtmlConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => void;
    readonly renderHtmlConfiguredAdvanced: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number) => void;
    readonly renderHtmlConfiguredMulti: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number) => void;
    readonly renderHtmlConfiguredWithFonts: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number) => void;
    readonly renderInteractiveHtmlConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly renderPdf: (a: number, b: number, c: number) => void;
    readonly renderPdfConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number) => void;
    readonly renderPdfConfiguredMulti: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number, q1: number, r1: number, s1: number, t1: number, u1: number, v1: number) => void;
    readonly renderPdfConfiguredWithAssets: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number) => void;
    readonly renderPdfConfiguredWithImage: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number) => void;
    readonly renderSemanticDiffHtml: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => number;
    readonly renderSvgConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly searchIndex: (a: number, b: number, c: number) => void;
    readonly semanticDiff: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
