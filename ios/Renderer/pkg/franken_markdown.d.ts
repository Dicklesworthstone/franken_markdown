/* tslint:disable */
/* eslint-disable */

/**
 * A parsed book and bounded source capture retained across browser exports.
 * Source edits are transactional and preserve render options and assets.
 *
 * Construct with parallel arrays of book-relative paths and Markdown sources
 * in reading order. Call `free()` when the book is no longer needed, as with
 * other wasm-bindgen classes. Render methods return `Uint8Array` payloads:
 * PDF (`application/pdf`), EPUB (`application/epub+zip`), and site ZIP
 * (`application/zip`).
 */
export class FmdBook {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Expand includes from the selected chapters and optional include-only
     * resources, then retain one parsed book for every export format.
     * Resources never become chapters. The original constructor remains
     * parse-only so hosts can deliberately supply already expanded source.
     *
     * # Errors
     * Rejects mismatched arrays, invalid or duplicate paths, missing sources,
     * cycles, selectors, and whole-book source/expansion budgets.
     */
    static fromSources(paths: string[], sources: string[], include_paths: string[], include_sources: string[]): FmdBook;
    /**
     * Parse all chapters once, with bounded source bytes and validated paths.
     *
     * # Errors
     * Rejects mismatched arrays, empty books, invalid paths, collisions,
     * or the source limits enforced by the pure book renderer.
     */
    constructor(paths: string[], sources: string[]);
    /**
     * Render one chapter and the complete navigation map, without rendering
     * or compressing a full site. Returns fmd-book-chapter-preview-v1 JSON.
     *
     * # Errors
     * Rejects lossy JavaScript indexes, invalid assets and preview budgets.
     */
    renderChapterPreview(selected: number): Uint8Array;
    /**
     * Export a multi-chapter EPUB with ordered spine and packaged images.
     *
     * # Errors
     * Returns asset validation or EPUB rendering failures as JavaScript errors.
     */
    renderEpub(): Uint8Array;
    /**
     * Render the retained book with export-local paper geometry in points:
     * width, height, top, right, bottom, left. An empty vector uses the stored
     * theme. No chapter is reparsed and no retained option or asset is changed,
     * including when validation or rendering fails. The ordinary book pipeline
     * still binds chapter links, isolates citations, and resolves image paths.
     *
     * # Errors
     * Rejects invalid geometry using the single-document browser policy, or
     * returns book, asset-budget, and PDF rendering errors.
     */
    renderPdfWithPage(page_geometry: Float64Array): Uint8Array;
    /**
     * Export a continuous PDF with isolated citations and chapter assets.
     *
     * # Errors
     * Returns asset validation or PDF rendering failures as JavaScript errors.
     */
    renderPdf(): Uint8Array;
    /**
     * Export an HTML-site ZIP with navigation and chapter-addressed search.
     *
     * # Errors
     * Returns path, resource-budget, or HTML rendering failures.
     */
    renderSite(): Uint8Array;
    /**
     * Replace the complete ordered chapter/include capture at an expected
     * revision, preserving publishing settings, page defaults and all assets.
     * Empty include arrays remove the include-only capture; expansion mode
     * remains fixed. No host files or source text are rewritten. Changed sets
     * rebuild all chapter bindings; exact normalized no-ops retain the AST.
     * Returns bounded fmd-book-source-set-v1 JSON.
     *
     * # Errors
     * Invalid/stale revisions, mismatched arrays, source/path/output limits,
     * and invalid include graphs leave the previous usable book untouched.
     */
    replaceSources(paths: string[], sources: string[], include_paths: string[], include_sources: string[], expected_revision: number): string;
    /**
     * Replace HTML/EPUB CSS. `undefined` restores the built-in stylesheet;
     * an empty string deliberately requests no stylesheet rules.
     */
    setCustomCss(css?: string | null): void;
    /**
     * Set a finite, positive uniform font scale (1.0 means no scaling).
     *
     * # Errors
     * Non-finite, zero, negative, or unrepresentable scales are rejected
     * without changing the current value.
     */
    setFontScale(scale: number): void;
    /**
     * Pin a variable font's CSS weight in the range 1 through 1000.
     * Static fonts ignore the weight pin.
     *
     * # Errors
     * Unknown slots or out-of-range weights do not mutate the font settings.
     */
    setFontWeight(slot: string, weight: number): void;
    /**
     * Supply a validated font for one shared PDF/HTML renderer slot. EPUB
     * font styling remains controlled by its stylesheet.
     *
     * # Errors
     * Unknown slots or unsupported font bytes leave the existing slot intact.
     */
    setFont(slot: string, bytes: Uint8Array): void;
    /**
     * Select the HTML site's embedded font container; EPUB still uses its
     * publication-local TrueType package. Does not change PDF or source state.
     *
     * # Errors
     * Unknown formats leave the last valid format unchanged.
     */
    setHtmlFontFormat(format: string): void;
    /**
     * Add or replace one image keyed by its book-relative destination.
     * For `guide/start.md` containing `figure.svg`, supply
     * `guide/figure.svg`. Different chapters may use the same basename.
     *
     * # Errors
     * Rejects invalid keys and image budgets. No partial replacement occurs
     * when validation fails. The core does not fetch missing images.
     */
    setImage(destination: string, bytes: Uint8Array): void;
    /**
     * Set shared metadata. Absent or blank values restore renderer defaults.
     * Per-chapter frontmatter language overrides the book's language in HTML
     * and EPUB. Author metadata is used by the PDF path.
     */
    setMetadata(title?: string | null, author?: string | null, language?: string | null): void;
    /**
     * Configure table-of-contents generation and PDF running page numbers.
     * EPUB's required navigation document is generated independently of this
     * optional in-content table of contents.
     */
    setNavigation(table_of_contents: boolean, page_numbers: boolean): void;
    /**
     * Replace the PDF settings profile, including optional running logos, as
     * one transaction. Image keys refer to assets supplied with `setImage`.
     * The original `setPdfOptions` ABI still resets to text-only bands.
     *
     * # Errors
     * Invalid image references, position codes, heights or other settings leave
     * the entire retained profile unchanged. Payload decoding occurs at render.
     */
    setPdfOptionsWithRunningImages(typography: string | null | undefined, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, code_line_numbers: boolean, metadata_epoch_seconds: number | null | undefined, running_slots: string[], header_rule: boolean, footer_rule: boolean, skip_first_page: boolean, image_destinations: string[], image_positions: Uint32Array, image_heights: Uint32Array): void;
    /**
     * Replace detailed PDF layout, typography, metadata epoch and running
     * bands as one transaction. Absent fields reset this profile to defaults;
     * title, author, theme, assets, source and navigation enablement survive.
     * Typography tokens use the single-document contract. Running templates
     * are header left/center/right followed by footer left/center/right.
     * Geometry and template validity that depends on it are checked at render.
     * The TOC depth also applies to subsequent HTML table-of-contents output.
     *
     * # Errors
     * Invalid tokens, sizes, integers or band sizes leave every setting intact.
     */
    setPdfOptions(typography: string | null | undefined, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, code_line_numbers: boolean, metadata_epoch_seconds: number | null | undefined, running_slots: string[], header_rule: boolean, footer_rule: boolean, skip_first_page: boolean): void;
    /**
     * Choose the shared font family and HTML dark-mode policy. `auto` follows
     * the browser's preferred color scheme; `light` or `disabled` emits only
     * the default light palette. EPUB uses its own or caller-supplied CSS.
     *
     * # Errors
     * Invalid names leave the previous theme unchanged.
     */
    setTheme(font: string, dark_mode: string): void;
    /**
     * Atomically replace selected chapter/include sources at an expected
     * revision, without recreating the renderer or reparsing unchanged chapters.
     * Source membership, reading order and expansion mode remain fixed.
     * Returns bounded fmd-book-source-update-v1 JSON; an unchanged batch keeps
     * its revision. Existing render settings and image/font assets survive.
     *
     * # Errors
     * Rejects invalid/stale revisions, mismatched arrays, unknown/duplicate
     * source keys, source/expansion limits and include failures. The last
     * successful source capture remains usable after every rejected update.
     */
    updateSources(paths: string[], sources: string[], expected_revision: number): string;
    /**
     * Check local HTML navigation on the retained, expanded book. Returns
     * fmd-book-link-report-v1 JSON without rendering pages or inspecting assets.
     * External URLs and non-chapter downloads are counted, not verified.
     *
     * # Errors
     * Rejects invalid books or validation/report budget overruns.
     */
    validateLinks(): string;
    /**
     * Number of chapters, in reading order.
     */
    readonly chapterCount: number;
    /**
     * Original Markdown size in UTF-8 bytes, excluding logical filenames.
     * Includes include-only resource bytes when constructed by `fromSources`.
     */
    readonly sourceLength: number;
    /**
     * Committed source revision, independent of presentation and asset edits.
     */
    readonly sourceRevision: number;
}

/**
 * Persistent browser editing and measured display output. The generated class
 * owns Rust allocations until free() (the public JS adapter calls dispose()).
 */
export class FmdFlowSession {
    free(): void;
    [Symbol.dispose](): void;
    assetBytes(request_id: string, revision: string): Uint8Array | undefined;
    copySource(revision: string, start: number, end: number): string;
    editBytes(revision: string, start: number, end: number, replacement: string, verified_asset_reuse: boolean): void;
    /**
     * One atomic editor transaction in original UTF-16 coordinates.
     */
    editManyUtf16Packed(revision: string, ranges: Uint32Array, lengths: Uint32Array, replacements: string, verified_asset_reuse: boolean): void;
    editUtf16(revision: string, start: number, end: number, replacement: string, verified_asset_reuse: boolean): void;
    fontBytes(font_id: string): Uint8Array;
    /**
     * Batch paths from the same immutable font identities as the display list.
     */
    glyphOutlinesJson(font_id: string, glyph_ids: Uint16Array): string;
    hitTestJson(revision: string, layout_revision: string, x: number, y: number): string;
    constructor(source: string, font: string, width: number, body: number, code: number, leading: number);
    pendingAssetsJson(revision: string, offset: number, limit: number): string;
    provideAsset(request_id: string, generation: string, width: number, height: number, bytes?: Uint8Array | null): void;
    /**
     * One data-only batch, one native parse/reflow, no partial publication.
     */
    provideAssetsPacked(metadata: string, payload: Uint8Array): void;
    readingJson(revision: string, layout_revision: string, offset: number, limit: number): string;
    reflow(revision: string, layout_revision: string, width: number, body: number, code: number, leading: number): void;
    reloadAssets(revision: string): void;
    replaceSource(revision: string, source: string, verified_asset_reuse: boolean): void;
    selectItemJson(revision: string, layout_revision: string, index: number, start: number, end: number): string;
    setCodeHighlighting(revision: string, layout_revision: string, enabled: boolean): void;
    snapshotJson(revision: string, layout_revision: string, offset: number, limit: number, glyphs: boolean): string;
    /**
     * Bounded indexed query; the original snapshot API remains available.
     */
    viewportJson(revision: string, layout_revision: string, x: number, y: number, width: number, height: number, after: number, limit: number, glyphs: boolean): string;
    readonly codeHighlighting: boolean;
    readonly layoutRevision: string;
    readonly revision: string;
    readonly source: string;
}

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
 * Compile selected chapters with the complete PDF configuration contract.
 * Unlike the narrow legacy book ABI, this route retains host images and fonts,
 * typography, metadata, navigation and optional paper geometry. Includes use
 * the bounded source-bundle resolver, and chapter links/citations are bound by
 * the ordinary native book renderer. No source is fetched from the host.
 *
 * # Errors
 * Rejects source/asset budgets, invalid options, include resolution errors,
 * and book or PDF rendering failures.
 */
export function renderBookPdfConfiguredPage(paths: string[], sources: string[], font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, page_numbers: boolean, font_scale: number | null | undefined, lang: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, microtype_protrusion: boolean, page_geometry: Float64Array): FmdRenderResult;

/**
 * Compile in-memory Markdown files into a deterministic, zero-JavaScript HTML
 * site ZIP. The host owns file selection; the core owns include expansion,
 * link rewriting, navigation, parsing, rendering, and the search index.
 */
export function renderBookSite(paths: string[], sources: string[], title?: string | null, font?: string | null, dark_mode?: string | null, font_scale?: number | null): FmdRenderResult;

/**
 * Publish chapters and optional include-only resources as one offline ZIP.
 *
 * Images use book-root keys; fonts use the five ordinary renderer slots.
 * Chapter order is preserved. The output includes the canonical chapter-aware
 * search page/index and `frankenmarkdown-receipt.json`. Source parsing remains
 * safe: this API deliberately has no raw-HTML trust switch.
 *
 * # Errors
 * Rejects source, asset, metadata and output budgets, malformed options,
 * missing/cyclic includes, colliding paths and rendering failures.
 */
export function renderBookSitePublication(paths: string[], sources: string[], include_paths: string[], include_sources: string[], expand_includes: boolean, title: string | null | undefined, font: string | null | undefined, dark_mode: string | null | undefined, font_scale: number | null | undefined, lang: string | null | undefined, custom_css: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, image_destinations: string[], image_bytes_flat: Uint8Array, image_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array): Uint8Array;

/**
 * Render an EPUB 3 e-book through the same parser and HTML theme model.
 */
export function renderEpubConfigured(markdown: string, font?: string | null, dark_mode?: string | null, title?: string | null, lang?: string | null, font_scale?: number | null): FmdRenderResult;

/**
 * Render an EPUB with host-owned images, font faces, stylesheet and navigation.
 * The original six-argument `renderEpubConfigured` remains available unchanged.
 * No asset is fetched; the native EPUB writer owns XHTML, resource manifests,
 * font subsetting and ZIP serialization. Raw HTML is always escaped.
 *
 * # Errors
 * Rejects invalid options, inconsistent packed arrays, excessive input/assets,
 * invalid fonts, and publication-rendering failures before publishing output.
 */
export function renderEpubConfiguredAdvanced(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, lang: string | null | undefined, font_scale: number | null | undefined, custom_css: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array): FmdRenderResult;

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
export function renderHtmlConfiguredAdvanced(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, custom_css: string | null | undefined, allow_raw_html: boolean, font_scale: number | null | undefined, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, lang: string | null | undefined, toc: boolean, toc_depth?: number | null, html_font_format?: string | null): FmdRenderResult;

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
export function renderPdfConfiguredMulti(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, page_numbers: boolean, font_scale: number | null | undefined, lang: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, microtype_protrusion: boolean, typography?: string | null): FmdRenderResult;

/**
 * Render using the ordinary PDF engine with explicit paper and margins.
 *
 * `page_geometry` is empty for the existing theme defaults, or exactly six
 * point values: width, height, top, right, bottom, left. Values are checked
 * before parsing or copying per-asset payloads, including after f32 conversion.
 * No output scaling, MediaBox patching, browser printing, or global state.
 *
 * # Errors
 * Returns an error for invalid geometry, assets, options, or rendering.
 */
export function renderPdfConfiguredPage(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, page_numbers: boolean, font_scale: number | null | undefined, lang: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, microtype_protrusion: boolean, page_geometry: Float64Array, typography?: string | null): FmdRenderResult;

/**
 * [`render_pdf_configured_page`] plus running header/footer chrome (GH #13).
 *
 * `running_slots` is empty (no chrome) or exactly six templates: header
 * left, center, right, then footer left, center, right. Empty strings are
 * absent slots. Tokens: `{page}`, `{pages}`, `{title}`, `{author}`,
 * `{date}` (from `metadata_epoch_seconds`; never the clock). The existing
 * bindings keep their signatures and output.
 *
 * # Errors
 * Returns an error for malformed running slots, invalid geometry, assets or
 * options, a band that does not fit its margin, or a render failure.
 */
export function renderPdfConfiguredRunning(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, page_numbers: boolean, font_scale: number | null | undefined, lang: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, microtype_protrusion: boolean, page_geometry: Float64Array, running_slots: string[], header_rule: boolean, footer_rule: boolean, skip_first_page: boolean, typography?: string | null): FmdRenderResult;

/**
 * Running text and optional image logos from explicitly supplied image assets.
 * Image vectors are empty, or two entries (header, footer). A blank destination
 * is absent; positions are 0/1 (left/right), heights are 0 (automatic) or positive
 * u16 points. The text-only ABI remains unchanged.
 *
 * # Errors
 * Rejects malformed image settings before rendering; missing or invalid image
 * payloads and images that do not fit are reported by the shared PDF renderer.
 */
export function renderPdfConfiguredRunningImages(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, title: string | null | undefined, author: string | null | undefined, metadata_epoch_seconds: number | null | undefined, allow_raw_html: boolean, code_line_numbers: boolean, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array, base_font_size: number | null | undefined, heading_scale: number | null | undefined, table_font_size: number | null | undefined, page_numbers: boolean, font_scale: number | null | undefined, lang: string | null | undefined, toc: boolean, toc_depth: number | null | undefined, fit_to_pages: number | null | undefined, microtype_protrusion: boolean, page_geometry: Float64Array, running_slots: string[], header_rule: boolean, footer_rule: boolean, skip_first_page: boolean, running_image_destinations: string[], running_image_positions: Uint32Array, running_image_heights: Uint32Array, typography?: string | null): FmdRenderResult;

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
 * Render an SVG poster with explicit image bytes and the shared five font slots.
 * Images arrive as parallel destinations, packed bytes and lengths. Empty font
 * slots select bundled faces. Nothing is fetched from a destination string.
 *
 * The result retains parser diagnostics AND recoverable image/math warnings.
 * The original five-argument `renderSvgConfigured` remains supported.
 *
 * # Errors
 * Rejects excessive source/assets, inconsistent packed arrays, duplicate image
 * keys, invalid fonts/weights and non-finite or out-of-range geometry. Input
 * admission precedes per-resource copies, font parsing and Markdown parsing.
 */
export function renderSvgConfiguredResources(markdown: string, font: string | null | undefined, dark_mode: string | null | undefined, font_scale: number | null | undefined, max_width_pt: number | null | undefined, image_destinations: string[], image_bytes_flat: Uint8Array, image_bytes_lengths: Uint32Array, body_regular: Uint8Array, body_bold: Uint8Array, body_italic: Uint8Array, body_bold_italic: Uint8Array, mono_regular: Uint8Array, font_weights: Uint32Array): FmdRenderResult;

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
    readonly __wbg_fmdbook_free: (a: number, b: number) => void;
    readonly __wbg_fmdflowsession_free: (a: number, b: number) => void;
    readonly __wbg_fmdrenderresult_free: (a: number, b: number) => void;
    readonly accessibilityAudit: (a: number, b: number, c: number) => void;
    readonly capabilities: (a: number) => void;
    readonly documentStats: (a: number, b: number, c: number) => void;
    readonly fmdbook_chapterCount: (a: number) => number;
    readonly fmdbook_fromSources: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly fmdbook_new: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly fmdbook_renderChapterPreview: (a: number, b: number, c: number) => void;
    readonly fmdbook_renderEpub: (a: number, b: number) => void;
    readonly fmdbook_renderPdf: (a: number, b: number) => void;
    readonly fmdbook_renderPdfWithPage: (a: number, b: number, c: number, d: number) => void;
    readonly fmdbook_renderSite: (a: number, b: number) => void;
    readonly fmdbook_replaceSources: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly fmdbook_setCustomCss: (a: number, b: number, c: number) => void;
    readonly fmdbook_setFont: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdbook_setFontScale: (a: number, b: number, c: number) => void;
    readonly fmdbook_setFontWeight: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly fmdbook_setHtmlFontFormat: (a: number, b: number, c: number, d: number) => void;
    readonly fmdbook_setImage: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdbook_setMetadata: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly fmdbook_setNavigation: (a: number, b: number, c: number) => void;
    readonly fmdbook_setPdfOptions: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number) => void;
    readonly fmdbook_setPdfOptionsWithRunningImages: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number) => void;
    readonly fmdbook_setTheme: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdbook_sourceLength: (a: number) => number;
    readonly fmdbook_sourceRevision: (a: number) => number;
    readonly fmdbook_updateSources: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly fmdbook_validateLinks: (a: number, b: number) => void;
    readonly fmdflowsession_assetBytes: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdflowsession_codeHighlighting: (a: number) => number;
    readonly fmdflowsession_copySource: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdflowsession_editBytes: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly fmdflowsession_editManyUtf16Packed: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly fmdflowsession_editUtf16: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly fmdflowsession_fontBytes: (a: number, b: number, c: number, d: number) => void;
    readonly fmdflowsession_glyphOutlinesJson: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdflowsession_hitTestJson: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly fmdflowsession_layoutRevision: (a: number, b: number) => void;
    readonly fmdflowsession_new: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly fmdflowsession_pendingAssetsJson: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdflowsession_provideAsset: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => void;
    readonly fmdflowsession_provideAssetsPacked: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly fmdflowsession_readingJson: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly fmdflowsession_reflow: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => void;
    readonly fmdflowsession_reloadAssets: (a: number, b: number, c: number, d: number) => void;
    readonly fmdflowsession_replaceSource: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly fmdflowsession_revision: (a: number, b: number) => void;
    readonly fmdflowsession_selectItemJson: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly fmdflowsession_setCodeHighlighting: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly fmdflowsession_snapshotJson: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
    readonly fmdflowsession_source: (a: number, b: number) => void;
    readonly fmdflowsession_viewportJson: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly fmdrenderresult_bytes: (a: number, b: number) => void;
    readonly fmdrenderresult_diagnosticsJson: (a: number, b: number) => void;
    readonly fmdrenderresult_extension: (a: number, b: number) => void;
    readonly fmdrenderresult_format: (a: number, b: number) => void;
    readonly fmdrenderresult_mimeType: (a: number, b: number) => void;
    readonly fmdrenderresult_sourceLength: (a: number) => number;
    readonly renderBookPdf: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number) => void;
    readonly renderBookPdfConfiguredPage: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number, q1: number, r1: number, s1: number, t1: number, u1: number, v1: number, w1: number, x1: number, y1: number, z1: number) => void;
    readonly renderBookSite: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly renderBookSitePublication: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number) => void;
    readonly renderEpubConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly renderEpubConfiguredAdvanced: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number) => void;
    readonly renderHtml: (a: number, b: number, c: number) => void;
    readonly renderHtmlConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => void;
    readonly renderHtmlConfiguredAdvanced: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number) => void;
    readonly renderHtmlConfiguredMulti: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number) => void;
    readonly renderHtmlConfiguredWithFonts: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number) => void;
    readonly renderInteractiveHtmlConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => void;
    readonly renderPdf: (a: number, b: number, c: number) => void;
    readonly renderPdfConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number) => void;
    readonly renderPdfConfiguredMulti: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number, q1: number, r1: number, s1: number, t1: number, u1: number, v1: number, w1: number, x1: number) => void;
    readonly renderPdfConfiguredPage: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number, q1: number, r1: number, s1: number, t1: number, u1: number, v1: number, w1: number, x1: number, y1: number, z1: number) => void;
    readonly renderPdfConfiguredRunning: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number, q1: number, r1: number, s1: number, t1: number, u1: number, v1: number, w1: number, x1: number, y1: number, z1: number, a2: number, b2: number, c2: number, d2: number, e2: number) => void;
    readonly renderPdfConfiguredRunningImages: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number, d1: number, e1: number, f1: number, g1: number, h1: number, i1: number, j1: number, k1: number, l1: number, m1: number, n1: number, o1: number, p1: number, q1: number, r1: number, s1: number, t1: number, u1: number, v1: number, w1: number, x1: number, y1: number, z1: number, a2: number, b2: number, c2: number, d2: number, e2: number, f2: number, g2: number, h2: number, i2: number, j2: number, k2: number) => void;
    readonly renderPdfConfiguredWithAssets: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number) => void;
    readonly renderPdfConfiguredWithImage: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number) => void;
    readonly renderSemanticDiffHtml: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => number;
    readonly renderSvgConfigured: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number) => void;
    readonly renderSvgConfiguredResources: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number, q: number, r: number, s: number, t: number, u: number, v: number, w: number, x: number, y: number, z: number, a1: number, b1: number, c1: number) => void;
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
